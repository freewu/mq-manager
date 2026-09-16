//! In-memory demo driver.
//!
//! Its only reason to exist is to prove that the provider abstraction really is
//! broker agnostic — and to let the UI be developed and reviewed without a
//! running broker. It is intentionally simple: no persistence, no auth.

use async_trait::async_trait;
use parking_lot::Mutex;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};
use crate::mq::provider::{MqConnection, MqProvider, StreamEvent};
use crate::mq::types::*;

pub const PROVIDER_ID: &str = "demo";

pub struct DemoProvider;

#[async_trait]
impl MqProvider for DemoProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        descriptor()
    }

    async fn connect(&self, profile: &ConnectionProfile) -> AppResult<Arc<dyn MqConnection>> {
        let partitions = profile.option_u64("partitions", 3).clamp(1, 32) as usize;
        let seed = profile.option_bool("seedData", true);
        Ok(Arc::new(DemoConnection::new(&profile.id, partitions, seed)))
    }
}

pub fn descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: PROVIDER_ID.into(),
        name: "Demo broker".into(),
        description: "In-memory broker shipped with the app. Nothing is persisted.".into(),
        vendor: "MQ Manager".into(),
        accent: "#8b6cf0".into(),
        docs_url: None,
        default_port: None,
        driver_version: env!("CARGO_PKG_VERSION").into(),
        capabilities: Capabilities {
            topics: TopicCapabilities {
                list: true,
                create: true,
                delete: true,
                update: true,
                config: false,
                partitions: true,
                purge: true,
            },
            messages: MessageCapabilities {
                produce: true,
                browse: true,
                tail: true,
                keys: true,
                headers: true,
                partitions: true,
                timestamp_seek: true,
                offset_seek: true,
                persistent: false,
            },
            groups: GroupCapabilities {
                list: true,
                describe: true,
                members: true,
                offsets: true,
                lag: true,
                reset_offsets: false,
                delete: true,
            },
            nodes: true,
            metrics: false,
            acl: false,
            schemas: false,
        },
        fields: vec![
            ConnectionField::text("displayName", "Display name")
                .placeholder("Demo playground")
                .group("Connection"),
            ConnectionField::number("partitions", "Default partitions")
                .default_value(json!(3))
                .group("Connection"),
            ConnectionField::boolean("seedData", "Create sample topics")
                .default_value(json!(true))
                .help("Creates `orders`, `events` and `logs` with a few messages each.")
                .group("Connection"),
            ConnectionField::text("note", "Note")
                .help("Stored with the profile, never sent anywhere.")
                .group("Advanced")
                .advanced(),
        ],
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Default)]
struct DemoState {
    topics: HashMap<String, DemoTopic>,
    groups: HashMap<String, DemoGroup>,
}

struct DemoTopic {
    partitions: Vec<Vec<Message>>,
    created_at: i64,
}

struct DemoGroup {
    topics: Vec<String>,
    offsets: HashMap<(String, i32), i64>,
}

pub struct DemoConnection {
    profile_id: String,
    default_partitions: usize,
    state: Arc<Mutex<DemoState>>,
    sequence: Arc<AtomicU64>,
    connected_at: i64,
}

impl DemoConnection {
    fn new(profile_id: &str, partitions: usize, seed: bool) -> Self {
        let connection = Self {
            profile_id: profile_id.to_string(),
            default_partitions: partitions,
            state: Arc::new(Mutex::new(DemoState::default())),
            sequence: Arc::new(AtomicU64::new(0)),
            connected_at: now(),
        };
        if seed {
            connection.seed();
        }
        connection
    }

    fn seed(&self) {
        for (name, partitions, count) in [
            ("orders", 3usize, 12usize),
            ("events", 3, 8),
            ("logs", 1, 5),
        ] {
            let mut state = self.state.lock();
            let topic = state.topics.entry(name.to_string()).or_insert(DemoTopic {
                partitions: vec![Vec::new(); partitions],
                created_at: now(),
            });
            for index in 0..count {
                let partition = index % topic.partitions.len();
                let offset = topic.partitions[partition].len() as i64;
                let payload = json!({
                    "id": index + 1,
                    "topic": name,
                    "status": if index % 3 == 0 { "pending" } else { "ok" },
                })
                .to_string();
                let message = Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    topic: name.to_string(),
                    partition: Some(partition as i32),
                    offset: Some(offset),
                    timestamp: Some(now() - ((count - index) as i64) * 1_000),
                    producer: Some("demo-seed".into()),
                    key: Some(format!("key-{index}")),
                    key_encoding: PayloadEncoding::Utf8,
                    payload: Some(payload),
                    encoding: PayloadEncoding::Utf8,
                    headers: vec![MessageHeader {
                        key: "origin".into(),
                        value: Some("seed".into()),
                        encoding: PayloadEncoding::Utf8,
                    }],
                    size: 80,
                };
                topic.partitions[partition].push(message);
            }
        }
        let mut state = self.state.lock();
        state.groups.insert(
            "demo-consumer".to_string(),
            DemoGroup {
                topics: vec!["orders".to_string(), "events".to_string()],
                offsets: HashMap::new(),
            },
        );
    }

    fn append(&self, request: &ProduceRequest) -> AppResult<ProduceResult> {
        let started = Instant::now();
        let mut state = self.state.lock();
        let partitions = self.default_partitions;
        let topic = state
            .topics
            .entry(request.topic.clone())
            .or_insert_with(|| DemoTopic {
                partitions: vec![Vec::new(); partitions],
                created_at: now(),
            });

        let partition = match request.partition {
            Some(index) if index >= 0 && (index as usize) < topic.partitions.len() => {
                index as usize
            }
            _ => {
                (self.sequence.fetch_add(1, Ordering::Relaxed) % topic.partitions.len() as u64)
                    as usize
            }
        };
        let offset = topic.partitions[partition].len() as i64;
        let (key, key_encoding) = match request.key.clone() {
            Some(key) => (Some(key), request.encoding),
            None => (None, PayloadEncoding::Utf8),
        };
        let message = Message {
            id: uuid::Uuid::new_v4().to_string(),
            topic: request.topic.clone(),
            partition: Some(partition as i32),
            offset: Some(offset),
            timestamp: Some(request.timestamp.unwrap_or_else(now)),
            producer: Some("mq-manager".into()),
            key,
            key_encoding,
            payload: request.payload.clone(),
            encoding: request.encoding,
            headers: request.headers.clone(),
            size: request
                .payload
                .as_ref()
                .map(|value| value.len())
                .unwrap_or(0),
        };
        topic.partitions[partition].push(message);

        Ok(ProduceResult {
            topic: request.topic.clone(),
            partition: partition as i32,
            offset,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ---------------------------------------------------------------------------
// Trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl MqConnection for DemoConnection {
    fn provider_id(&self) -> &str {
        PROVIDER_ID
    }

    fn profile_id(&self) -> &str {
        &self.profile_id
    }

    fn capabilities(&self) -> Capabilities {
        descriptor().capabilities
    }

    async fn ping(&self) -> AppResult<()> {
        Ok(())
    }

    async fn cluster_info(&self) -> AppResult<ClusterInfo> {
        let state = self.state.lock();
        let partitions: usize = state
            .topics
            .values()
            .map(|topic| topic.partitions.len())
            .sum();
        Ok(ClusterInfo {
            id: Some(format!("demo-{PROVIDER_ID}")),
            name: "In-memory demo broker".into(),
            provider: PROVIDER_ID.into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            controller_id: Some(1),
            node_count: 1,
            topic_count: state.topics.len() as u32,
            partition_count: partitions as i64,
            consumer_group_count: Some(state.groups.len() as u32),
            attributes: vec![
                Attribute::new("Storage", "in-memory"),
                Attribute::new("Connected at", self.connected_at.to_string()),
            ],
        })
    }

    async fn list_nodes(&self) -> AppResult<Vec<NodeInfo>> {
        Ok(vec![NodeInfo {
            id: 1,
            host: "in-memory".into(),
            port: 0,
            rack: None,
            is_controller: true,
            role: Some("controller".into()),
        }])
    }

    async fn list_topics(&self) -> AppResult<Vec<TopicSummary>> {
        let state = self.state.lock();
        let mut topics: Vec<TopicSummary> = state
            .topics
            .iter()
            .map(|(name, topic)| TopicSummary {
                name: name.clone(),
                kind: EntityKind::Topic,
                internal: false,
                partition_count: Some(topic.partitions.len() as u32),
                message_count: Some(topic.partitions.iter().map(|p| p.len() as i64).sum()),
                size_bytes: None,
                consumer_count: None,
                attributes: Vec::new(),
            })
            .collect();
        topics.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(topics)
    }

    async fn topic_detail(&self, topic: &str) -> AppResult<TopicDetail> {
        let state = self.state.lock();
        let entry = state
            .topics
            .get(topic)
            .ok_or_else(|| AppError::invalid(format!("topic `{topic}` does not exist")))?;

        let partitions = entry
            .partitions
            .iter()
            .enumerate()
            .map(|(index, messages)| PartitionInfo {
                id: index as i32,
                leader: Some(1),
                replicas: vec![1],
                isr: vec![1],
                offline_replicas: Vec::new(),
                begin_offset: Some(0),
                end_offset: Some(messages.len() as i64),
                message_count: Some(messages.len() as i64),
            })
            .collect::<Vec<_>>();

        let total: i64 = partitions
            .iter()
            .filter_map(|partition| partition.message_count)
            .sum();

        Ok(TopicDetail {
            summary: TopicSummary {
                name: topic.to_string(),
                kind: EntityKind::Topic,
                internal: false,
                partition_count: Some(partitions.len() as u32),
                message_count: Some(total),
                size_bytes: None,
                consumer_count: None,
                attributes: Vec::new(),
            },
            partitions,
            configs: Vec::new(),
            attributes: vec![
                Attribute::new("Partitions", entry.partitions.len().to_string()),
                Attribute::new("Created at", entry.created_at.to_string()),
            ],
        })
    }

    async fn create_topic(&self, request: CreateTopicRequest) -> AppResult<()> {
        let mut state = self.state.lock();
        if state.topics.contains_key(&request.name) {
            return Err(AppError::invalid(format!(
                "topic `{}` already exists",
                request.name
            )));
        }
        let partitions = request
            .partition_count
            .map(|value| value.max(1) as usize)
            .unwrap_or(self.default_partitions);
        state.topics.insert(
            request.name,
            DemoTopic {
                partitions: vec![Vec::new(); partitions],
                created_at: now(),
            },
        );
        Ok(())
    }

    async fn update_topic(&self, request: UpdateTopicRequest) -> AppResult<()> {
        let mut state = self.state.lock();
        let topic = state
            .topics
            .get_mut(&request.name)
            .ok_or_else(|| AppError::invalid(format!("topic `{}` does not exist", request.name)))?;
        if let Some(count) = request.partition_count {
            let count = count.max(1) as usize;
            if count < topic.partitions.len() {
                return Err(AppError::invalid(
                    "partitions can only be added, never removed",
                ));
            }
            topic.partitions.resize_with(count, Vec::new);
        }
        Ok(())
    }

    async fn delete_topic(&self, topic: &str) -> AppResult<()> {
        let mut state = self.state.lock();
        state
            .topics
            .remove(topic)
            .map(|_| ())
            .ok_or_else(|| AppError::invalid(format!("topic `{topic}` does not exist")))
    }

    async fn purge_topic(&self, topic: &str) -> AppResult<()> {
        let mut state = self.state.lock();
        let entry = state
            .topics
            .get_mut(topic)
            .ok_or_else(|| AppError::invalid(format!("topic `{topic}` does not exist")))?;
        for partition in &mut entry.partitions {
            partition.clear();
        }
        Ok(())
    }

    async fn list_groups(&self) -> AppResult<Vec<ConsumerGroupSummary>> {
        let state = self.state.lock();
        let mut groups: Vec<ConsumerGroupSummary> = state
            .groups
            .iter()
            .map(|(id, group)| ConsumerGroupSummary {
                id: id.clone(),
                state: Some("Stable".into()),
                protocol_type: Some("consumer".into()),
                member_count: 1,
                topics: group.topics.clone(),
                total_lag: None,
                kind: Some("consumer-group".into()),
            })
            .collect();
        groups.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(groups)
    }

    async fn group_detail(&self, group: &str) -> AppResult<ConsumerGroupDetail> {
        let state = self.state.lock();
        let entry = state
            .groups
            .get(group)
            .ok_or_else(|| AppError::invalid(format!("group `{group}` does not exist")))?;

        let mut offsets = Vec::new();
        for topic_name in &entry.topics {
            let Some(topic) = state.topics.get(topic_name) else {
                continue;
            };
            for (index, messages) in topic.partitions.iter().enumerate() {
                let end = messages.len() as i64;
                let current = entry
                    .offsets
                    .get(&(topic_name.clone(), index as i32))
                    .copied()
                    .unwrap_or_else(|| (end / 2).max(0));
                offsets.push(GroupOffset {
                    topic: topic_name.clone(),
                    partition: Some(index as i32),
                    current_offset: Some(current),
                    begin_offset: Some(0),
                    end_offset: Some(end),
                    lag: Some((end - current).max(0)),
                    metadata: None,
                    node: Some(1),
                });
            }
        }

        Ok(ConsumerGroupDetail {
            summary: ConsumerGroupSummary {
                id: group.to_string(),
                state: Some("Stable".into()),
                protocol_type: Some("consumer".into()),
                member_count: 1,
                topics: entry.topics.clone(),
                total_lag: Some(offsets.iter().filter_map(|offset| offset.lag).sum()),
                kind: Some("consumer-group".into()),
            },
            members: vec![GroupMember {
                id: format!("{group}-member-1"),
                client_id: Some("demo-client".into()),
                client_host: Some("127.0.0.1".into()),
                assignments: entry
                    .topics
                    .iter()
                    .map(|topic| MemberAssignment {
                        topic: topic.clone(),
                        partitions: vec![0],
                    })
                    .collect(),
            }],
            offsets,
        })
    }

    async fn delete_group(&self, group: &str) -> AppResult<()> {
        let mut state = self.state.lock();
        state
            .groups
            .remove(group)
            .map(|_| ())
            .ok_or_else(|| AppError::invalid(format!("group `{group}` does not exist")))
    }

    async fn produce(&self, request: ProduceRequest) -> AppResult<ProduceResult> {
        self.append(&request)
    }

    async fn browse(&self, request: BrowseRequest) -> AppResult<BrowseResult> {
        let started = Instant::now();
        let state = self.state.lock();
        let topic = state.topics.get(&request.topic).ok_or_else(|| {
            AppError::invalid(format!("topic `{}` does not exist", request.topic))
        })?;

        let limit = request.limit.max(1);
        let mut watermarks = Vec::new();
        let mut messages = Vec::new();

        for (index, partition) in topic.partitions.iter().enumerate() {
            let id = index as i32;
            if let Some(filter) = &request.partitions {
                if !filter.contains(&id) {
                    continue;
                }
            }
            watermarks.push(PartitionInfo {
                id,
                leader: Some(1),
                replicas: vec![1],
                isr: vec![1],
                offline_replicas: Vec::new(),
                begin_offset: Some(0),
                end_offset: Some(partition.len() as i64),
                message_count: Some(partition.len() as i64),
            });

            let start = match &request.start {
                SeekPosition::Beginning => 0,
                SeekPosition::End => partition.len(),
                SeekPosition::Offset { offset } => (*offset).max(0) as usize,
                SeekPosition::Timestamp { timestamp } => partition
                    .iter()
                    .position(|message| message.timestamp.unwrap_or(0) >= *timestamp)
                    .unwrap_or(partition.len()),
            };

            for message in partition.iter().skip(start).take(limit) {
                if let Some(keyword) = request.keyword.as_deref() {
                    let keyword = keyword.to_lowercase();
                    if !keyword.is_empty()
                        && !message
                            .payload
                            .clone()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&keyword)
                    {
                        continue;
                    }
                }
                messages.push(message.clone());
            }
        }

        let scanned = messages.len();
        messages.sort_by(|a, b| {
            a.timestamp
                .unwrap_or_default()
                .cmp(&b.timestamp.unwrap_or_default())
        });
        messages.truncate(limit);

        Ok(BrowseResult {
            truncated: messages.len() >= limit,
            messages,
            scanned,
            elapsed_ms: started.elapsed().as_millis() as u64,
            watermarks,
        })
    }

    async fn open_stream(&self, request: StreamRequest) -> AppResult<mpsc::Receiver<StreamEvent>> {
        let state = self.state.clone();
        let (sender, receiver) = mpsc::channel(64);

        tokio::spawn(async move {
            let mut cursor: HashMap<i32, i64> = HashMap::new();
            {
                let guard = state.lock();
                if let Some(topic) = guard.topics.get(&request.topic) {
                    for (index, partition) in topic.partitions.iter().enumerate() {
                        cursor.insert(index as i32, partition.len() as i64);
                    }
                }
            }

            let mut sent: u64 = 0;
            let mut last_activity = Instant::now();
            let mut ticker = tokio::time::interval(Duration::from_millis(400));

            loop {
                ticker.tick().await;

                // The lock is scoped so the future stays `Send`.
                let polled = {
                    let guard = state.lock();
                    match guard.topics.get(&request.topic) {
                        None => None,
                        Some(topic) => {
                            let mut batch = Vec::new();
                            for (index, partition) in topic.partitions.iter().enumerate() {
                                let id = index as i32;
                                if let Some(filter) = &request.partitions {
                                    if !filter.contains(&id) {
                                        continue;
                                    }
                                }
                                let from = cursor.entry(id).or_insert(0);
                                if (*from as usize) < partition.len() {
                                    for message in partition.iter().skip(*from as usize) {
                                        batch.push(message.clone());
                                    }
                                    *from = partition.len() as i64;
                                }
                            }
                            Some(batch)
                        }
                    }
                };

                let Some(batch) = polled else {
                    let _ = sender
                        .send(StreamEvent::Failed("topic disappeared".into()))
                        .await;
                    return;
                };

                if !batch.is_empty() {
                    sent += batch.len() as u64;
                    last_activity = Instant::now();
                    if sender.send(StreamEvent::Batch(batch)).await.is_err() {
                        return;
                    }
                    if request.max_messages > 0 && sent >= request.max_messages {
                        let _ = sender.send(StreamEvent::End).await;
                        return;
                    }
                } else if last_activity.elapsed() >= Duration::from_millis(10_000) {
                    if sender.send(StreamEvent::Idle).await.is_err() {
                        return;
                    }
                    last_activity = Instant::now();
                }
            }
        });

        Ok(receiver)
    }
}
