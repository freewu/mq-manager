//! Consumer groups for RocketMQ.
//!
//! RocketMQ keeps two halves of the picture on two different roles: the group
//! *configuration* lives on every broker, the *subscription* and the member
//! connections live on the broker the consumers are connected to. Both are
//! merged here, degrading to “offline” when a group has no live consumer.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use serde::Deserialize;

use super::protocol::{code, response_code, RemotingCommand};
use super::remoting::is_missing;
use super::topics::{masters, route};
use super::RocketMqConnection;
use crate::error::{AppError, AppResult};
use crate::mq::types::*;

/// Total time spent probing which groups have live consumers.
const MEMBER_PROBE_BUDGET: Duration = Duration::from_millis(4_000);
/// Per probe timeout — a missing broker must not stall the list.
const MEMBER_PROBE_TIMEOUT: Duration = Duration::from_millis(1_000);
/// Groups probed for live members before we stop and report unknown counts.
const MEMBER_PROBE_LIMIT: usize = 40;
/// Queues described with offsets before we stop asking.
const OFFSET_QUEUE_LIMIT: usize = 256;

// ---------------------------------------------------------------------------
// Wire structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SubscriptionGroupDto {
    #[serde(default)]
    pub consume_enable: bool,
    #[serde(default)]
    pub consume_from_min_enable: bool,
    #[serde(default)]
    pub consume_broadcast_enable: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SubscriptionGroupWrapper {
    #[serde(default)]
    pub subscription_group_table: BTreeMap<String, SubscriptionGroupDto>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ConnectionDto {
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_addr: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ConsumerConnectionDto {
    #[serde(default)]
    pub subscription_table: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub connection_set: Vec<ConnectionDto>,
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

/// The group table of every master broker, merged by group name.
pub(super) async fn group_table(
    connection: &RocketMqConnection,
) -> AppResult<BTreeMap<String, SubscriptionGroupDto>> {
    let mut table: BTreeMap<String, SubscriptionGroupDto> = BTreeMap::new();
    let mut reachable = false;

    for (_, addr) in masters(connection).await? {
        let response = connection
            .client()
            .request(&addr, code::GET_ALL_SUBSCRIPTIONGROUP_CONFIG, &[])
            .await;
        let Ok(response) = response else { continue };
        if response.code != response_code::SUCCESS {
            continue;
        }
        let Ok(wrapper) = response.body_json::<SubscriptionGroupWrapper>() else {
            continue;
        };
        reachable = true;
        for (name, config) in wrapper.subscription_group_table {
            table.entry(name).or_insert(config);
        }
    }

    if !reachable {
        return Err(AppError::broker(
            "no master broker answered the subscription group request",
        ));
    }
    Ok(table)
}

/// Live consumers of one group, from every master broker.
async fn connections(
    connection: &RocketMqConnection,
    group: &str,
) -> AppResult<Vec<ConnectionDto>> {
    let mut seen = BTreeSet::new();
    let mut members = Vec::new();

    for (_, addr) in masters(connection).await? {
        let response = connection
            .client()
            .request(
                &addr,
                code::GET_CONSUMER_CONNECTION_LIST,
                &[("consumerGroup", group.to_string())],
            )
            .await;
        let Ok(response) = response else { continue };
        if response.code != response_code::SUCCESS {
            continue;
        }
        let Ok(consumer) = response.body_json::<ConsumerConnectionDto>() else {
            continue;
        };
        for member in consumer.connection_set {
            if seen.insert(member.client_id.clone()) {
                members.push(member);
            }
        }
    }
    Ok(members)
}

/// The topics a group subscribes to, from every master broker.
async fn subscriptions(
    connection: &RocketMqConnection,
    group: &str,
) -> AppResult<BTreeSet<String>> {
    let mut topics = BTreeSet::new();
    for (_, addr) in masters(connection).await? {
        let response = connection
            .client()
            .request(
                &addr,
                code::GET_CONSUMER_CONNECTION_LIST,
                &[("consumerGroup", group.to_string())],
            )
            .await;
        let Ok(response) = response else { continue };
        if response.code != response_code::SUCCESS {
            continue;
        }
        let Ok(consumer) = response.body_json::<ConsumerConnectionDto>() else {
            continue;
        };
        topics.extend(consumer.subscription_table.into_keys());
    }
    Ok(topics)
}

pub(super) async fn groups_impl(
    connection: &RocketMqConnection,
) -> AppResult<Vec<ConsumerGroupSummary>> {
    let table = group_table(connection).await?;
    let deadline = std::time::Instant::now() + MEMBER_PROBE_BUDGET;

    let mut groups = Vec::with_capacity(table.len());
    for (name, config) in &table {
        let mut member_count = 0u32;
        if groups.len() < MEMBER_PROBE_LIMIT && std::time::Instant::now() < deadline {
            let members = tokio::time::timeout(MEMBER_PROBE_TIMEOUT, connections(connection, name))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            member_count = members.len() as u32;
        }
        groups.push(ConsumerGroupSummary {
            id: name.clone(),
            state: Some(if member_count > 0 { "active" } else { "idle" }.into()),
            protocol_type: Some("rocketmq".into()),
            member_count,
            topics: Vec::new(),
            total_lag: None,
            kind: Some(group_kind(config)),
        });
    }

    groups.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(groups)
}

fn group_kind(config: &SubscriptionGroupDto) -> String {
    if config.consume_broadcast_enable && !config.consume_enable {
        "broadcast".into()
    } else if config.consume_from_min_enable {
        "from-first-offset".into()
    } else {
        "consumer".into()
    }
}

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

pub(super) async fn group_detail_impl(
    connection: &RocketMqConnection,
    group: &str,
) -> AppResult<ConsumerGroupDetail> {
    let table = group_table(connection).await?;
    let config = table
        .get(group)
        .cloned()
        .ok_or_else(|| AppError::invalid(format!("the consumer group `{group}` does not exist")))?;

    let members = connections(connection, group).await?;
    let topics = subscriptions(connection, group).await?;

    let online = !members.is_empty();
    let mut offsets = Vec::new();
    let mut total_lag: i64 = 0;
    let mut lag_known = false;

    if !topics.is_empty() {
        for topic in &topics {
            match collect_offsets(connection, group, topic).await {
                Ok(collected) => {
                    for offset in &collected {
                        if let Some(lag) = offset.lag {
                            total_lag += lag.max(0);
                            lag_known = true;
                        }
                    }
                    offsets.extend(collected);
                }
                Err(error) => tracing::debug!(
                    target: "mq_manager::rocketmq",
                    %topic,
                    %error,
                    "could not read the offsets of a subscribed topic"
                ),
            }
        }
    }

    // RocketMQ does not expose the per-client assignment through the remoting
    // admin API — the broker keeps it in memory for the rebalance only. Every
    // member is therefore shown with the queues of the topics it subscribes to.
    let assignments: Vec<MemberAssignment> = topics
        .iter()
        .map(|topic| MemberAssignment {
            topic: topic.clone(),
            partitions: offsets
                .iter()
                .filter(|offset| &offset.topic == topic)
                .filter_map(|offset| offset.partition)
                .collect(),
        })
        .collect();

    let members: Vec<GroupMember> = members
        .into_iter()
        .map(|member| GroupMember {
            id: member.client_id.clone(),
            client_id: Some(member.client_id),
            client_host: Some(member.client_addr),
            assignments: assignments.clone(),
        })
        .collect();

    let mut summary = ConsumerGroupSummary {
        id: group.to_string(),
        state: Some(if online { "active" } else { "offline" }.into()),
        protocol_type: Some("rocketmq".into()),
        member_count: members.len() as u32,
        topics: topics.iter().cloned().collect(),
        total_lag: lag_known.then_some(total_lag),
        kind: Some(group_kind(&config)),
    };
    // Offline groups keep their committed offsets, but nothing is consuming.
    if !online {
        summary.state = Some("offline".into());
    }

    Ok(ConsumerGroupDetail {
        summary,
        members,
        offsets,
    })
}

/// Committed offsets of one group for one topic, queue by queue.
async fn collect_offsets(
    connection: &RocketMqConnection,
    group: &str,
    topic: &str,
) -> AppResult<Vec<GroupOffset>> {
    let route = route(connection, topic).await?;
    let queues = route.queues();
    let mut offsets = Vec::with_capacity(queues.len().min(OFFSET_QUEUE_LIMIT));

    for (_, addr, queue_id) in queues.iter().take(OFFSET_QUEUE_LIMIT) {
        let committed = consumer_offset(connection, addr, group, topic, *queue_id).await;
        let end = super::messages::max_offset(connection, addr, topic, *queue_id)
            .await
            .ok();
        let begin = super::messages::min_offset(connection, addr, topic, *queue_id)
            .await
            .ok();

        let current = committed.unwrap_or(None);
        let lag = match (current, end) {
            (Some(current), Some(end)) => Some((end - current).max(0)),
            _ => None,
        };
        if current.is_none() && end.is_none() {
            continue;
        }
        offsets.push(GroupOffset {
            topic: topic.to_string(),
            partition: Some(*queue_id),
            current_offset: current,
            end_offset: end,
            begin_offset: begin,
            lag,
            metadata: None,
            node: Some(0),
        });
    }

    Ok(offsets)
}

/// Committed offset of one queue; `Ok(None)` when nothing was committed yet.
async fn consumer_offset(
    connection: &RocketMqConnection,
    addr: &str,
    group: &str,
    topic: &str,
    queue_id: i32,
) -> AppResult<Option<i64>> {
    let response = connection
        .client()
        .request(
            addr,
            code::QUERY_CONSUMER_OFFSET,
            &[
                ("consumerGroup", group.to_string()),
                ("topic", topic.to_string()),
                ("queueId", queue_id.to_string()),
            ],
        )
        .await?;
    if response.code == response_code::SUCCESS {
        return Ok(response.header_i64("offset"));
    }
    if is_missing(response.code) {
        return Ok(None);
    }
    Err(AppError::broker(format!(
        "could not read the offset of `{group}` on {topic}[{queue_id}]: {}",
        super::protocol::response_text(response.code)
    )))
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

pub(super) async fn reset_offsets_impl(
    connection: &RocketMqConnection,
    request: ResetOffsetsRequest,
) -> AppResult<()> {
    let route = route(connection, &request.topic).await?;
    let mut queues = route.queues();
    if let Some(filter) = &request.partitions {
        queues.retain(|(_, _, queue)| filter.contains(queue));
    }
    if queues.is_empty() {
        return Err(AppError::invalid(format!(
            "the topic `{}` has no queue to reset",
            request.topic
        )));
    }

    let explicit = match request.mode {
        ResetMode::Offset => Some(request.offset.ok_or_else(|| {
            AppError::invalid("an explicit offset is required when resetting to a fixed position")
        })?),
        _ => None,
    };

    let mut failures = Vec::new();
    let mut changed = 0usize;
    for (_, addr, queue_id) in &queues {
        let offset = match (explicit, request.mode) {
            (Some(offset), _) => Some(offset),
            (None, ResetMode::Earliest) => {
                super::messages::min_offset(connection, addr, &request.topic, *queue_id)
                    .await
                    .ok()
            }
            (None, ResetMode::Latest) => {
                super::messages::max_offset(connection, addr, &request.topic, *queue_id)
                    .await
                    .ok()
            }
            (None, ResetMode::Offset) => None,
        };
        let Some(offset) = offset else {
            failures.push(format!(
                "queue {queue_id}: could not resolve the target offset"
            ));
            continue;
        };

        let command = RemotingCommand::request(code::UPDATE_CONSUMER_OFFSET, 0, Default::default())
            .with_ext("consumerGroup", request.group.clone())
            .with_ext("topic", request.topic.clone())
            .with_ext("queueId", queue_id.to_string())
            .with_ext("commitOffset", offset.to_string());
        match connection.client().invoke(addr, command).await {
            Ok(response) if response.code == response_code::SUCCESS => changed += 1,
            Ok(response) => failures.push(format!(
                "queue {queue_id}: {}",
                super::protocol::response_text(response.code)
            )),
            Err(error) => failures.push(format!("queue {queue_id}: {error}")),
        }
    }

    if !failures.is_empty() {
        return Err(AppError::broker(format!(
            "could not reset the offsets of `{}`: {}",
            request.group,
            failures.join("; ")
        )));
    }
    if changed == 0 {
        return Err(AppError::broker(format!(
            "no offset of `{}` could be reset",
            request.group
        )));
    }
    tracing::info!(
        target: "mq_manager::rocketmq",
        group = %request.group,
        topic = %request.topic,
        changed,
        "reset consumer offsets"
    );
    Ok(())
}

pub(super) async fn delete_group_impl(
    connection: &RocketMqConnection,
    group: &str,
) -> AppResult<()> {
    let mut failures = Vec::new();
    let mut removed = false;
    for (name, addr) in masters(connection).await? {
        let command =
            RemotingCommand::request(code::DELETE_SUBSCRIPTIONGROUP, 0, Default::default())
                .with_ext("groupName", group.to_string())
                .with_ext("removeOffset", "true");
        match connection.client().invoke(&addr, command).await {
            Ok(response) if response.code == response_code::SUCCESS => removed = true,
            Ok(response) if is_missing(response.code) => {}
            Ok(response) => failures.push(format!(
                "{name}: {}",
                super::protocol::response_text(response.code)
            )),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }

    if !failures.is_empty() {
        return Err(AppError::broker(format!(
            "could not delete the consumer group `{group}`: {}",
            failures.join("; ")
        )));
    }
    if !removed {
        return Err(AppError::invalid(format!(
            "the consumer group `{group}` was not found on any broker"
        )));
    }
    tracing::info!(
        target: "mq_manager::rocketmq",
        group = %group,
        "deleted consumer group"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_group_kinds() {
        let mut config = SubscriptionGroupDto::default();
        config.consume_enable = true;
        assert_eq!(group_kind(&config), "consumer");

        config.consume_from_min_enable = true;
        assert_eq!(group_kind(&config), "from-first-offset");

        let broadcast = SubscriptionGroupDto {
            consume_broadcast_enable: true,
            ..Default::default()
        };
        assert_eq!(group_kind(&broadcast), "broadcast");
    }
}
