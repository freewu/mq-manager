use rdkafka::admin::{AlterConfig, NewPartitions, NewTopic, ResourceSpecifier, TopicReplication};
use rdkafka::{Offset, TopicPartitionList};

use super::{kerr, KafkaConnection};
use crate::error::{AppError, AppResult};
use crate::mq::types::*;

impl KafkaConnection {
    /// Cluster banner shown on the overview page.
    pub(crate) async fn cluster_impl(&self) -> AppResult<ClusterInfo> {
        let target = self.target().to_string();
        self.blocking(move |handle| {
            let client = handle.client();
            let timeout = handle.admin_timeout();
            let metadata = client.fetch_metadata(None::<&str>, timeout).map_err(kerr)?;
            let cluster_id = client.fetch_cluster_id(timeout);

            let mut topic_count = 0u32;
            let mut partition_count = 0i64;
            for topic in metadata.topics() {
                if topic.error().is_some() {
                    continue;
                }
                topic_count += 1;
                partition_count += topic.partitions().len() as i64;
            }

            let group_count = client
                .fetch_group_list(None::<&str>, timeout)
                .ok()
                .map(|list| list.groups().len() as u32);

            let mut attributes = vec![
                Attribute::new("Bootstrap servers", target),
                Attribute::new("Brokers", metadata.brokers().len().to_string()),
            ];
            if let Some(id) = &cluster_id {
                attributes.insert(0, Attribute::new("Cluster id", id.clone()));
            }

            Ok(ClusterInfo {
                id: cluster_id,
                name: handle.timeouts.target.clone(),
                provider: super::PROVIDER_ID.to_string(),
                version: None,
                controller_id: None,
                node_count: metadata.brokers().len() as u32,
                topic_count,
                partition_count,
                consumer_group_count: group_count,
                attributes,
            })
        })
        .await
    }

    pub(crate) async fn nodes_impl(&self) -> AppResult<Vec<NodeInfo>> {
        self.blocking(|handle| {
            let metadata = handle
                .client()
                .fetch_metadata(None::<&str>, handle.admin_timeout())
                .map_err(kerr)?;
            let responder = metadata.orig_broker_id();
            let mut nodes: Vec<NodeInfo> = metadata
                .brokers()
                .iter()
                .map(|broker| NodeInfo {
                    id: broker.id(),
                    host: broker.host().to_string(),
                    port: broker.port(),
                    rack: None,
                    is_controller: broker.id() == responder,
                    role: Some(if broker.id() == responder {
                        "responder".to_string()
                    } else {
                        "broker".to_string()
                    }),
                })
                .collect();
            nodes.sort_by_key(|node| node.id);
            Ok(nodes)
        })
        .await
    }

    pub(crate) async fn topics_impl(&self) -> AppResult<Vec<TopicSummary>> {
        self.blocking(|handle| {
            let metadata = handle
                .client()
                .fetch_metadata(None::<&str>, handle.admin_timeout())
                .map_err(kerr)?;

            let mut topics: Vec<TopicSummary> = metadata
                .topics()
                .iter()
                .filter(|topic| topic.error().is_none())
                .map(|topic| {
                    let name = topic.name().to_string();
                    TopicSummary {
                        internal: name.starts_with("__"),
                        name,
                        kind: EntityKind::Topic,
                        partition_count: Some(topic.partitions().len() as u32),
                        message_count: None,
                        size_bytes: None,
                        consumer_count: None,
                        attributes: Vec::new(),
                    }
                })
                .collect();

            topics.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(topics)
        })
        .await
    }

    pub(crate) async fn topic_detail_impl(&self, topic: &str) -> AppResult<TopicDetail> {
        let name = topic.to_string();
        let configs = self.topic_configs_impl(topic).await.unwrap_or_default();

        self.blocking(move |handle| {
            let metadata = handle
                .client()
                .fetch_metadata(Some(&name), handle.admin_timeout())
                .map_err(kerr)?;

            let topic_metadata = metadata
                .topics()
                .iter()
                .find(|candidate| candidate.name() == name.as_str())
                .ok_or_else(|| AppError::invalid(format!("topic `{name}` does not exist")))?;

            if let Some(error) = topic_metadata.error() {
                return Err(AppError::broker(format!(
                    "cannot describe topic `{name}`: {error:?}"
                )));
            }

            let mut partitions: Vec<PartitionInfo> = Vec::new();
            let mut total_messages: i64 = 0;

            for partition in topic_metadata.partitions() {
                let mut info = PartitionInfo {
                    id: partition.id(),
                    leader: Some(partition.leader()),
                    replicas: partition.replicas().to_vec(),
                    isr: partition.isr().to_vec(),
                    ..Default::default()
                };

                if let Ok((low, high)) = handle.client().fetch_watermarks(
                    &name,
                    partition.id(),
                    handle.request_timeout(),
                ) {
                    info.begin_offset = Some(low);
                    info.end_offset = Some(high);
                    info.message_count = Some((high - low).max(0));
                    total_messages += (high - low).max(0);
                }

                partitions.push(info);
            }
            partitions.sort_by_key(|partition| partition.id);

            let replication_factor = partitions
                .first()
                .map(|partition| partition.replicas.len())
                .unwrap_or(0);

            let under_replicated = partitions
                .iter()
                .filter(|partition| partition.isr.len() < partition.replicas.len())
                .count();

            let offline = partitions
                .iter()
                .filter(|partition| partition.leader.unwrap_or(-1) < 0)
                .count();

            let attributes = vec![
                Attribute::new("Partitions", partitions.len().to_string()),
                Attribute::new("Replication factor", replication_factor.to_string()),
                Attribute::new("Under replicated", under_replicated.to_string()),
                Attribute::new("Offline partitions", offline.to_string()),
            ];

            Ok(TopicDetail {
                summary: TopicSummary {
                    name: name.clone(),
                    kind: EntityKind::Topic,
                    internal: name.starts_with("__"),
                    partition_count: Some(partitions.len() as u32),
                    message_count: Some(total_messages),
                    size_bytes: None,
                    consumer_count: None,
                    attributes: Vec::new(),
                },
                partitions,
                configs: configs.clone(),
                attributes,
            })
        })
        .await
    }

    pub(crate) async fn topic_configs_impl(&self, topic: &str) -> AppResult<Vec<ConfigEntry>> {
        let options = self.admin_options();
        let specifier = ResourceSpecifier::Topic(topic);
        let results = self
            .admin
            .describe_configs([&specifier], &options)
            .await
            .map_err(kerr)?;

        let mut entries = Vec::new();
        for result in results {
            match result {
                Ok(resource) => entries.extend(resource.entries.iter().map(convert_config)),
                Err(code) => {
                    return Err(AppError::broker(format!(
                        "could not read the configuration of `{topic}`: {code}"
                    )))
                }
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    pub(crate) async fn create_topic_impl(&self, request: CreateTopicRequest) -> AppResult<()> {
        let options = self.admin_options();
        let partitions = request.partition_count.unwrap_or(6).max(1) as i32;
        let replication = request.replication_factor.unwrap_or(-1) as i32;

        let mut topic = NewTopic::new(
            &request.name,
            partitions,
            TopicReplication::Fixed(replication),
        );
        for entry in &request.configs {
            if let Some(value) = &entry.value {
                topic = topic.set(&entry.name, value);
            }
        }

        let results = self
            .admin
            .create_topics([&topic], &options)
            .await
            .map_err(kerr)?;

        for result in results {
            if let Err((name, code)) = result {
                return Err(AppError::broker(format!(
                    "could not create topic `{name}`: {code}"
                )));
            }
        }
        Ok(())
    }

    pub(crate) async fn update_topic_impl(&self, request: UpdateTopicRequest) -> AppResult<()> {
        let options = self.admin_options();

        if let Some(partitions) = request.partition_count {
            let new_partitions = NewPartitions::new(&request.name, partitions as usize);
            let results = self
                .admin
                .create_partitions([&new_partitions], &options)
                .await
                .map_err(kerr)?;
            for result in results {
                if let Err((name, code)) = result {
                    return Err(AppError::broker(format!(
                        "could not add partitions to `{name}`: {code}"
                    )));
                }
            }
        }

        let writable: Vec<&ConfigEntry> = request
            .configs
            .iter()
            .filter(|entry| entry.value.is_some())
            .collect();

        if !writable.is_empty() {
            let mut alter = AlterConfig::new(ResourceSpecifier::Topic(&request.name));
            for entry in &writable {
                alter = alter.set(&entry.name, entry.value.as_deref().unwrap_or_default());
            }
            let results = self
                .admin
                .alter_configs([&alter], &options)
                .await
                .map_err(kerr)?;
            for result in results {
                if let Err((specifier, code)) = result {
                    return Err(AppError::broker(format!(
                        "could not update the configuration of {specifier:?}: {code}"
                    )));
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn delete_topic_impl(&self, topic: &str) -> AppResult<()> {
        let options = self.admin_options();
        let results = self
            .admin
            .delete_topics(&[topic], &options)
            .await
            .map_err(kerr)?;
        for result in results {
            if let Err((name, code)) = result {
                return Err(AppError::broker(format!(
                    "could not delete topic `{name}`: {code}"
                )));
            }
        }
        Ok(())
    }

    /// Delete every record up to the current high watermark.
    pub(crate) async fn purge_topic_impl(&self, topic: &str) -> AppResult<()> {
        let name = topic.to_string();
        let offsets = self
            .blocking(move |handle| {
                let metadata = handle
                    .client()
                    .fetch_metadata(Some(&name), handle.admin_timeout())
                    .map_err(kerr)?;
                let topic_metadata = metadata
                    .topics()
                    .iter()
                    .find(|candidate| candidate.name() == name.as_str())
                    .ok_or_else(|| AppError::invalid(format!("topic `{name}` does not exist")))?;

                let mut list = TopicPartitionList::new();
                for partition in topic_metadata.partitions() {
                    let (_, high) = handle
                        .client()
                        .fetch_watermarks(&name, partition.id(), handle.request_timeout())
                        .map_err(kerr)?;
                    list.add_partition_offset(&name, partition.id(), Offset::Offset(high))
                        .map_err(kerr)?;
                }
                Ok(list)
            })
            .await?;

        let options = self.admin_options();
        self.admin
            .delete_records(&offsets, &options)
            .await
            .map_err(kerr)?;
        Ok(())
    }
}

fn convert_config(entry: &rdkafka::admin::ConfigEntry) -> ConfigEntry {
    ConfigEntry {
        name: entry.name.clone(),
        value: entry.value.clone(),
        source: Some(format!("{:?}", entry.source)),
        read_only: entry.is_read_only,
        sensitive: entry.is_sensitive,
        is_default: entry.is_default,
    }
}
