//! Provider agnostic domain model.
//!
//! Everything in this module is deliberately *neutral*: it must be expressible
//! for a log based broker (Kafka, RocketMQ), a queue based broker
//! (RabbitMQ, ActiveMQ) and a pub/sub broker (MQTT, EMQX).
//!
//! That is why most fields are `Option`s and why every provider publishes a
//! [`Capabilities`] block — the UI uses it to hide what a broker cannot do
//! instead of hard coding broker names.

use serde::{Deserialize, Serialize};

pub type JsonMap = serde_json::Map<String, serde_json::Value>;

fn default_group() -> String {
    "Connection".to_string()
}

// ---------------------------------------------------------------------------
// Provider descriptor + dynamic connection form
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    /// Stable machine id, e.g. `kafka`.
    pub id: String,
    /// Human readable name, e.g. `Apache Kafka`.
    pub name: String,
    /// Short tagline shown in the connection wizard.
    pub description: String,
    pub vendor: String,
    /// Accent colour (hex) used for badges / icons in the UI.
    pub accent: String,
    #[serde(default)]
    pub docs_url: Option<String>,
    pub default_port: Option<u16>,
    pub driver_version: String,
    /// What the driver is able to do. The UI gates features on this.
    pub capabilities: Capabilities,
    /// Declarative description of the connection form.
    pub fields: Vec<ConnectionField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    Text,
    Password,
    Number,
    Boolean,
    Select,
    Textarea,
    KeyValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldOption {
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub hint: Option<String>,
}

/// Very small conditional-visibility rule: `field.condition.key == one of equals`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCondition {
    pub key: String,
    pub equals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionField {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub options: Vec<FieldOption>,
    /// Form section, rendered as a collapsible group.
    #[serde(default = "default_group")]
    pub group: String,
    /// Advanced fields live behind an “Advanced” toggle.
    #[serde(default)]
    pub advanced: bool,
    #[serde(default)]
    pub condition: Option<FieldCondition>,
    #[serde(default)]
    pub secret: bool,
}

impl ConnectionField {
    fn of(key: &str, label: &str, kind: FieldKind) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind,
            help: None,
            placeholder: None,
            required: false,
            default: None,
            options: Vec::new(),
            group: default_group(),
            advanced: false,
            condition: None,
            secret: false,
        }
    }

    pub fn text(key: &str, label: &str) -> Self {
        Self::of(key, label, FieldKind::Text)
    }

    pub fn password(key: &str, label: &str) -> Self {
        Self::of(key, label, FieldKind::Password).secret()
    }

    pub fn number(key: &str, label: &str) -> Self {
        Self::of(key, label, FieldKind::Number)
    }

    pub fn boolean(key: &str, label: &str) -> Self {
        Self::of(key, label, FieldKind::Boolean)
    }

    pub fn select(key: &str, label: &str) -> Self {
        Self::of(key, label, FieldKind::Select)
    }

    /// Rendered as an editable key/value table (used for raw driver properties).
    pub fn key_value(key: &str, label: &str) -> Self {
        Self::of(key, label, FieldKind::KeyValue)
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn placeholder(mut self, value: &str) -> Self {
        self.placeholder = Some(value.into());
        self
    }

    pub fn help(mut self, value: &str) -> Self {
        self.help = Some(value.into());
        self
    }

    pub fn group(mut self, value: &str) -> Self {
        self.group = value.into();
        self
    }

    pub fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }

    pub fn secret(mut self) -> Self {
        self.secret = true;
        self
    }

    pub fn default_value(mut self, value: serde_json::Value) -> Self {
        self.default = Some(value);
        self
    }

    pub fn options(mut self, values: &[(&str, &str)]) -> Self {
        self.options = values
            .iter()
            .map(|(value, label)| FieldOption {
                value: (*value).into(),
                label: (*label).into(),
                hint: None,
            })
            .collect();
        self
    }

    pub fn when(mut self, key: &str, equals: &[&str]) -> Self {
        self.condition = Some(FieldCondition {
            key: key.into(),
            equals: equals.iter().map(|v| (*v).to_string()).collect(),
        });
        self
    }
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicCapabilities {
    pub list: bool,
    pub create: bool,
    pub delete: bool,
    pub update: bool,
    pub config: bool,
    pub partitions: bool,
    pub purge: bool,
}

impl Default for TopicCapabilities {
    fn default() -> Self {
        Self {
            list: true,
            create: false,
            delete: false,
            update: false,
            config: false,
            partitions: false,
            purge: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageCapabilities {
    /// Can the client publish messages?
    pub produce: bool,
    /// Can the client read historical messages?
    pub browse: bool,
    /// Does the client support a live tail?
    pub tail: bool,
    /// Does the protocol carry a message key?
    pub keys: bool,
    /// Does the protocol carry headers / user properties?
    pub headers: bool,
    /// Are topics split into partitions / queues?
    pub partitions: bool,
    /// Can we translate a timestamp into a start position?
    pub timestamp_seek: bool,
    /// Can we seek to an absolute offset?
    pub offset_seek: bool,
    /// Messages survive the consumer being offline (queue semantics)?
    pub persistent: bool,
}

impl Default for MessageCapabilities {
    fn default() -> Self {
        Self {
            produce: false,
            browse: false,
            tail: false,
            keys: true,
            headers: true,
            partitions: false,
            timestamp_seek: false,
            offset_seek: false,
            persistent: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCapabilities {
    pub list: bool,
    pub describe: bool,
    pub members: bool,
    pub offsets: bool,
    pub lag: bool,
    pub reset_offsets: bool,
    pub delete: bool,
}

impl Default for GroupCapabilities {
    fn default() -> Self {
        Self {
            list: true,
            describe: false,
            members: false,
            offsets: false,
            lag: false,
            reset_offsets: false,
            delete: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub topics: TopicCapabilities,
    pub messages: MessageCapabilities,
    pub groups: GroupCapabilities,
    pub nodes: bool,
    /// Broker level metrics (throughput, disk usage, ...).
    pub metrics: bool,
    pub acl: bool,
    pub schemas: bool,
}

impl Capabilities {
    /// Everything a modern log-based broker can do.
    pub fn full() -> Self {
        Self {
            topics: TopicCapabilities {
                list: true,
                create: true,
                delete: true,
                update: true,
                config: true,
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
                persistent: true,
            },
            groups: GroupCapabilities {
                list: true,
                describe: true,
                members: true,
                offsets: true,
                lag: true,
                reset_offsets: true,
                delete: true,
            },
            nodes: true,
            metrics: false,
            acl: false,
            schemas: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Connections
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    /// Provider id, see [`ProviderDescriptor::id`].
    pub provider: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Provider specific values, keyed by [`ConnectionField::key`].
    #[serde(default)]
    pub options: JsonMap,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub last_connected_at: Option<i64>,
}

impl ConnectionProfile {
    pub fn option_str(&self, key: &str) -> Option<String> {
        match self.options.get(key) {
            Some(serde_json::Value::String(value)) if !value.trim().is_empty() => {
                Some(value.trim().to_string())
            }
            Some(serde_json::Value::Number(number)) => Some(number.to_string()),
            Some(serde_json::Value::Bool(value)) => Some(value.to_string()),
            _ => None,
        }
    }

    pub fn option_bool(&self, key: &str, fallback: bool) -> bool {
        match self.options.get(key) {
            Some(serde_json::Value::Bool(value)) => *value,
            Some(serde_json::Value::String(value)) => {
                matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes")
            }
            _ => fallback,
        }
    }

    pub fn option_u64(&self, key: &str, fallback: u64) -> u64 {
        match self.options.get(key) {
            Some(serde_json::Value::Number(number)) => number.as_u64().unwrap_or(fallback),
            Some(serde_json::Value::String(value)) => value.parse().unwrap_or(fallback),
            _ => fallback,
        }
    }

    /// Extra librdkafka (or driver specific) properties, stored as a JSON object.
    pub fn option_map(&self, key: &str) -> Vec<(String, String)> {
        match self.options.get(key) {
            Some(serde_json::Value::Object(map)) => map
                .iter()
                .filter_map(|(k, v)| match v {
                    serde_json::Value::String(s) if !s.is_empty() => Some((k.clone(), s.clone())),
                    serde_json::Value::Number(n) => Some((k.clone(), n.to_string())),
                    serde_json::Value::Bool(b) => Some((k.clone(), b.to_string())),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// Aggregated view of one connection slot, used by the workspace sidebar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub profile_id: String,
    /// Echoed from the live connection so the sidebar can badge each profile
    /// without having to look the provider up again.
    pub provider_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub state: ConnectionState,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub cluster: Option<ClusterInfo>,
    #[serde(default)]
    pub connected_at: Option<i64>,
    #[serde(default)]
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResult {
    pub status: ConnectionStatus,
    pub capabilities: Capabilities,
    /// Provider descriptor, handy for the UI to render a fresh connection.
    pub provider: ProviderDescriptor,
}

// ---------------------------------------------------------------------------
// Cluster / nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterInfo {
    pub id: Option<String>,
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub controller_id: Option<i32>,
    pub node_count: u32,
    pub topic_count: u32,
    pub partition_count: i64,
    #[serde(default)]
    pub consumer_group_count: Option<u32>,
    /// Free-form extra information rendered as key/value rows.
    #[serde(default)]
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribute {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub mono: bool,
}

impl Attribute {
    pub fn new(label: &str, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            mono: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    pub id: i32,
    pub host: String,
    pub port: i32,
    #[serde(default)]
    pub rack: Option<String>,
    pub is_controller: bool,
    #[serde(default)]
    pub role: Option<String>,
}

// ---------------------------------------------------------------------------
// Topics / queues / exchanges
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntityKind {
    Topic,
    Queue,
    Exchange,
}

impl Default for EntityKind {
    fn default() -> Self {
        EntityKind::Topic
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicSummary {
    pub name: String,
    #[serde(default)]
    pub kind: EntityKind,
    pub internal: bool,
    #[serde(default)]
    pub partition_count: Option<u32>,
    #[serde(default)]
    pub message_count: Option<i64>,
    #[serde(default)]
    pub size_bytes: Option<i64>,
    #[serde(default)]
    pub consumer_count: Option<u32>,
    #[serde(default)]
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionInfo {
    pub id: i32,
    #[serde(default)]
    pub leader: Option<i32>,
    #[serde(default)]
    pub replicas: Vec<i32>,
    #[serde(default)]
    pub isr: Vec<i32>,
    #[serde(default)]
    pub offline_replicas: Vec<i32>,
    #[serde(default)]
    pub begin_offset: Option<i64>,
    #[serde(default)]
    pub end_offset: Option<i64>,
    #[serde(default)]
    pub message_count: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntry {
    pub name: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    pub read_only: bool,
    pub sensitive: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicDetail {
    pub summary: TopicSummary,
    #[serde(default)]
    pub partitions: Vec<PartitionInfo>,
    #[serde(default)]
    pub configs: Vec<ConfigEntry>,
    #[serde(default)]
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTopicRequest {
    pub name: String,
    #[serde(default)]
    pub partition_count: Option<u32>,
    #[serde(default)]
    pub replication_factor: Option<i16>,
    #[serde(default)]
    pub configs: Vec<ConfigEntry>,
    /// Queue based brokers may need extra topology (exchanges, routing keys, ...).
    #[serde(default)]
    pub options: JsonMap,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTopicRequest {
    pub name: String,
    /// Grow (never shrink) the topic to this many partitions.
    #[serde(default)]
    pub partition_count: Option<u32>,
    #[serde(default)]
    pub configs: Vec<ConfigEntry>,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PayloadEncoding {
    Utf8,
    Base64,
}

impl Default for PayloadEncoding {
    fn default() -> Self {
        PayloadEncoding::Utf8
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageHeader {
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
    pub encoding: PayloadEncoding,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    #[serde(default)]
    pub id: String,
    pub topic: String,
    #[serde(default)]
    pub partition: Option<i32>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub timestamp: Option<i64>,
    #[serde(default)]
    pub producer: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub key_encoding: PayloadEncoding,
    #[serde(default)]
    pub payload: Option<String>,
    pub encoding: PayloadEncoding,
    #[serde(default)]
    pub headers: Vec<MessageHeader>,
    pub size: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProduceRequest {
    pub topic: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub payload: Option<String>,
    #[serde(default)]
    pub headers: Vec<MessageHeader>,
    #[serde(default)]
    pub partition: Option<i32>,
    #[serde(default)]
    pub timestamp: Option<i64>,
    pub encoding: PayloadEncoding,
    #[serde(default)]
    pub options: JsonMap,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProduceResult {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub elapsed_ms: u64,
}

/// Where a browse / tail should start reading.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum SeekPosition {
    /// Oldest available message.
    Beginning,
    /// Only new messages.
    End,
    Offset {
        offset: i64,
    },
    Timestamp {
        timestamp: i64,
    },
}

impl Default for SeekPosition {
    fn default() -> Self {
        SeekPosition::Beginning
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseRequest {
    pub topic: String,
    /// `None` means “all partitions”.
    #[serde(default)]
    pub partitions: Option<Vec<i32>>,
    #[serde(default)]
    pub start: SeekPosition,
    pub limit: usize,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Only keep messages whose payload contains this substring (case-insensitive).
    #[serde(default)]
    pub keyword: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseResult {
    pub messages: Vec<Message>,
    /// `true` when the limit was hit before the timeout, ie. more data exists.
    pub truncated: bool,
    pub scanned: usize,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub watermarks: Vec<PartitionInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamRequest {
    pub topic: String,
    #[serde(default)]
    pub partitions: Option<Vec<i32>>,
    #[serde(default)]
    pub start: SeekPosition,
    /// Stop after N messages (0 = unlimited).
    #[serde(default)]
    pub max_messages: u64,
    /// Emit a heart-beat every N milliseconds so the UI can show “idle”.
    #[serde(default)]
    pub idle_timeout_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// Consumer groups / subscriptions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerGroupSummary {
    pub id: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub protocol_type: Option<String>,
    #[serde(default)]
    pub member_count: u32,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub total_lag: Option<i64>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberAssignment {
    pub topic: String,
    #[serde(default)]
    pub partitions: Vec<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub id: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_host: Option<String>,
    #[serde(default)]
    pub assignments: Vec<MemberAssignment>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupOffset {
    pub topic: String,
    #[serde(default)]
    pub partition: Option<i32>,
    #[serde(default)]
    pub current_offset: Option<i64>,
    #[serde(default)]
    pub end_offset: Option<i64>,
    #[serde(default)]
    pub begin_offset: Option<i64>,
    #[serde(default)]
    pub lag: Option<i64>,
    #[serde(default)]
    pub metadata: Option<String>,
    #[serde(default)]
    pub node: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerGroupDetail {
    pub summary: ConsumerGroupSummary,
    #[serde(default)]
    pub members: Vec<GroupMember>,
    #[serde(default)]
    pub offsets: Vec<GroupOffset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResetMode {
    Earliest,
    Latest,
    Offset,
}

impl Default for ResetMode {
    fn default() -> Self {
        ResetMode::Latest
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetOffsetsRequest {
    pub group: String,
    pub topic: String,
    #[serde(default)]
    pub partitions: Option<Vec<i32>>,
    #[serde(default)]
    pub mode: ResetMode,
    #[serde(default)]
    pub offset: Option<i64>,
}

// ---------------------------------------------------------------------------
// Jobs (live streams)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobInfo {
    pub id: String,
    pub kind: String,
    pub connection_id: String,
    pub target: String,
    pub state: JobState,
    #[serde(default)]
    pub error: Option<String>,
    pub started_at: i64,
    pub received: u64,
}

// ---------------------------------------------------------------------------
// Application settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,
    pub page_size: u32,
    pub default_message_limit: u32,
    pub pretty_json: bool,
    pub confirm_destructive: bool,
    pub auto_connect: bool,
    pub timestamp_format: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            page_size: 50,
            default_message_limit: 200,
            pretty_json: true,
            confirm_destructive: true,
            auto_connect: false,
            timestamp_format: "datetime".into(),
        }
    }
}
