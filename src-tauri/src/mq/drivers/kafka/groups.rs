use std::collections::{BTreeSet, HashMap};

use rdkafka::consumer::{CommitMode, Consumer};
use rdkafka::{Offset, TopicPartitionList};

use super::proto;
use super::{kerr, KafkaConnection};
use crate::error::{AppError, AppResult};
use crate::mq::types::*;

/// Safety valve: querying offsets for a group with no members means asking for
/// every partition in the cluster. We refuse to do that on absurdly wide clusters.
const MAX_PARTITIONS_PER_OFFSET_QUERY: usize = 20_000;

impl KafkaConnection {
    pub(crate) async fn groups_impl(&self) -> AppResult<Vec<ConsumerGroupSummary>> {
        self.blocking(|handle| {
            let list = handle
                .client()
                .fetch_group_list(None::<&str>, handle.admin_timeout())
                .map_err(kerr)?;

            let mut groups: Vec<ConsumerGroupSummary> = list
                .groups()
                .iter()
                .map(|group| {
                    let mut topics: BTreeSet<String> = BTreeSet::new();
                    for member in group.members() {
                        if let Some(raw) = member.metadata() {
                            topics.extend(proto::parse_subscription(raw));
                        }
                        if let Some(raw) = member.assignment() {
                            topics
                                .extend(proto::parse_assignment(raw).into_iter().map(|a| a.topic));
                        }
                    }
                    ConsumerGroupSummary {
                        id: group.name().to_string(),
                        state: Some(group.state().to_string()),
                        protocol_type: Some(group.protocol_type().to_string()),
                        member_count: group.members().len() as u32,
                        topics: topics.into_iter().collect(),
                        total_lag: None,
                        kind: Some("consumer-group".to_string()),
                    }
                })
                .collect();

            groups.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(groups)
        })
        .await
    }

    pub(crate) async fn group_detail_impl(&self, group: &str) -> AppResult<ConsumerGroupDetail> {
        let group_id = group.to_string();
        self.blocking(move |handle| {
            let client = handle.client();
            let timeout = handle.admin_timeout();

            let list = client
                .fetch_group_list(Some(&group_id), timeout)
                .map_err(kerr)?;
            let info = list
                .groups()
                .iter()
                .find(|candidate| candidate.name() == group_id.as_str());

            let mut members = Vec::new();
            let mut subscribed: BTreeSet<String> = BTreeSet::new();
            let (state, protocol_type) = match info {
                Some(info) => {
                    for member in info.members() {
                        let assignments = member
                            .assignment()
                            .map(proto::parse_assignment)
                            .unwrap_or_default();
                        for assignment in &assignments {
                            subscribed.insert(assignment.topic.clone());
                        }
                        if let Some(raw) = member.metadata() {
                            subscribed.extend(proto::parse_subscription(raw));
                        }
                        members.push(GroupMember {
                            id: member.id().to_string(),
                            client_id: Some(member.client_id().to_string()),
                            client_host: Some(member.client_host().to_string()),
                            assignments,
                        });
                    }
                    (
                        Some(info.state().to_string()),
                        Some(info.protocol_type().to_string()),
                    )
                }
                None => (None, None),
            };

            let metadata = client.fetch_metadata(None::<&str>, timeout).map_err(kerr)?;

            // Leader lookup so the UI can show which broker owns each partition.
            let mut leaders: HashMap<(String, i32), i32> = HashMap::new();
            let mut list = TopicPartitionList::new();
            let mut scoped = subscribed.clone();
            // An empty (inactive) group has no membership information, so fall
            // back to asking for every partition and filtering out the gaps.
            let wildcard = scoped.is_empty();

            for topic in metadata.topics() {
                if topic.error().is_some() {
                    continue;
                }
                let name = topic.name();
                if !wildcard && !scoped.contains(name) {
                    continue;
                }
                for partition in topic.partitions() {
                    leaders.insert((name.to_string(), partition.id()), partition.leader());
                    list.add_partition(name, partition.id());
                    if list.count() >= MAX_PARTITIONS_PER_OFFSET_QUERY {
                        break;
                    }
                }
                if list.count() >= MAX_PARTITIONS_PER_OFFSET_QUERY {
                    break;
                }
            }
            scoped.clear();

            let consumer = handle.consumer(&uuid::Uuid::new_v4().to_string())?;
            let committed = consumer.committed_offsets(list, timeout).map_err(kerr)?;

            let mut offsets = Vec::new();
            for element in committed.elements() {
                let current = match element.offset().to_raw() {
                    Some(value) if value >= 0 => value,
                    _ => continue,
                };
                let watermarks = consumer
                    .fetch_watermarks(element.topic(), element.partition(), timeout)
                    .ok();
                let (begin, end) = match watermarks {
                    Some((low, high)) if low >= 0 && high >= 0 => (Some(low), Some(high)),
                    _ => (None, None),
                };
                let metadata_value = element.metadata();
                offsets.push(GroupOffset {
                    topic: element.topic().to_string(),
                    partition: Some(element.partition()),
                    current_offset: Some(current),
                    begin_offset: begin,
                    end_offset: end,
                    lag: end.map(|end| (end - current).max(0)),
                    metadata: (!metadata_value.is_empty()).then(|| metadata_value.to_string()),
                    node: leaders
                        .get(&(element.topic().to_string(), element.partition()))
                        .copied(),
                });
            }

            offsets.sort_by(|a, b| {
                a.topic
                    .cmp(&b.topic)
                    .then(a.partition.unwrap_or(-1).cmp(&b.partition.unwrap_or(-1)))
            });

            let topics: Vec<String> = {
                let mut set: BTreeSet<String> = subscribed;
                set.extend(offsets.iter().map(|offset| offset.topic.clone()));
                set.into_iter().collect()
            };

            let total_lag = if offsets.iter().all(|offset| offset.lag.is_none()) {
                None
            } else {
                Some(offsets.iter().filter_map(|offset| offset.lag).sum())
            };

            Ok(ConsumerGroupDetail {
                summary: ConsumerGroupSummary {
                    id: group_id,
                    state,
                    protocol_type,
                    member_count: members.len() as u32,
                    topics,
                    total_lag,
                    kind: Some("consumer-group".to_string()),
                },
                members,
                offsets,
            })
        })
        .await
    }

    pub(crate) async fn delete_group_impl(&self, group: &str) -> AppResult<()> {
        let options = self.admin_options();
        let results = self
            .admin
            .delete_groups(&[group], &options)
            .await
            .map_err(kerr)?;
        for result in results {
            if let Err((name, code)) = result {
                return Err(AppError::broker(format!(
                    "could not delete consumer group `{name}`: {code}"
                )));
            }
        }
        Ok(())
    }

    pub(crate) async fn reset_offsets_impl(&self, request: ResetOffsetsRequest) -> AppResult<()> {
        self.blocking(move |handle| {
            let timeout = handle.request_timeout();
            let metadata = handle
                .client()
                .fetch_metadata(Some(&request.topic), handle.admin_timeout())
                .map_err(kerr)?;
            let topic = metadata
                .topics()
                .iter()
                .find(|candidate| candidate.name() == request.topic.as_str())
                .ok_or_else(|| {
                    AppError::invalid(format!("topic `{}` does not exist", request.topic))
                })?;

            let consumer = handle.consumer_as(&request.group)?;
            let mut list = TopicPartitionList::new();

            for partition in topic.partitions() {
                if let Some(filter) = &request.partitions {
                    if !filter.contains(&partition.id()) {
                        continue;
                    }
                }
                let offset = match request.mode {
                    ResetMode::Earliest => {
                        consumer
                            .fetch_watermarks(&request.topic, partition.id(), timeout)
                            .map_err(kerr)?
                            .0
                    }
                    ResetMode::Latest => {
                        consumer
                            .fetch_watermarks(&request.topic, partition.id(), timeout)
                            .map_err(kerr)?
                            .1
                    }
                    ResetMode::Offset => request.offset.unwrap_or(0),
                };
                list.add_partition_offset(&request.topic, partition.id(), Offset::Offset(offset))
                    .map_err(kerr)?;
            }

            if list.count() == 0 {
                return Err(AppError::invalid("no partition matched the request"));
            }

            consumer.commit(&list, CommitMode::Sync).map_err(kerr)?;
            Ok(())
        })
        .await
    }
}
