//! RabbitMQ management HTTP API client.
//!
//! The AMQP protocol has no notion of “list every queue”, so the driver leans on
//! the `rabbitmq_management` plugin for discovery: overview, nodes, queues,
//! exchanges, consumers and non destructive message peeking (`/get` with
//! `ack_requeue_true` never removes anything).
//!
//! Every endpoint lives under `/api`; the vhost is a path segment, so the
//! default vhost `/` is sent as `%2F`.
//!
//! The DTOs below mirror the JSON the API returns, so they keep fields that the
//! current screens do not show yet — that is why the module allows dead code.
#![allow(dead_code)]

use reqwest::{Client, RequestBuilder, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::error::{AppError, AppResult};

use super::options::RabbitMqOptions;

// ---------------------------------------------------------------------------
// DTOs (all fields optional: RabbitMQ adds and removes them between releases)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Overview {
    #[serde(default)]
    pub rabbitmq_version: Option<String>,
    #[serde(default)]
    pub management_version: Option<String>,
    #[serde(default)]
    pub product_version: Option<String>,
    #[serde(default)]
    pub cluster_name: Option<String>,
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub object_totals: ObjectTotals,
    #[serde(default)]
    pub queue_totals: QueueTotals,
    #[serde(default)]
    pub message_stats: Option<MessageStats>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ObjectTotals {
    #[serde(default)]
    pub connections: u32,
    #[serde(default)]
    pub channels: u32,
    #[serde(default)]
    pub exchanges: u32,
    #[serde(default)]
    pub queues: u32,
    #[serde(default)]
    pub consumers: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QueueTotals {
    #[serde(default)]
    pub messages: i64,
    #[serde(default)]
    pub messages_ready: i64,
    #[serde(default)]
    pub messages_unacknowledged: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MessageStats {
    #[serde(default)]
    pub publish: i64,
    #[serde(default)]
    pub publish_in: i64,
    #[serde(default)]
    pub publish_out: i64,
    #[serde(default)]
    pub deliver: i64,
    #[serde(default)]
    pub deliver_get: i64,
    #[serde(default)]
    pub deliver_no_ack: i64,
    #[serde(default)]
    pub ack: i64,
    #[serde(default)]
    pub redeliver: i64,
    #[serde(default)]
    pub confirm: i64,
    #[serde(default)]
    pub disk_reads: i64,
    #[serde(default)]
    pub disk_writes: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NodeDto {
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub mem_used: i64,
    #[serde(default)]
    pub mem_limit: i64,
    #[serde(default)]
    pub disk_free: i64,
    #[serde(default)]
    pub disk_free_limit: i64,
    #[serde(default)]
    pub proc_used: Option<u32>,
    #[serde(default)]
    pub os_pid: Option<String>,
    #[serde(default)]
    pub uptime: Option<i64>,
    #[serde(default)]
    pub partitions: Vec<String>,
    #[serde(default)]
    pub enabled_plugins: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QueueDto {
    pub name: String,
    #[serde(default)]
    pub vhost: Option<String>,
    /// `running`, `idle`, `flow` or `down`.
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub durable: bool,
    #[serde(default)]
    pub auto_delete: bool,
    #[serde(default)]
    pub exclusive: bool,
    #[serde(default)]
    pub internal: bool,
    #[serde(default)]
    pub messages: Option<i64>,
    #[serde(default)]
    pub messages_ready: Option<i64>,
    #[serde(default)]
    pub messages_unacknowledged: Option<i64>,
    #[serde(default)]
    pub consumers: Option<u32>,
    #[serde(default)]
    pub memory: Option<i64>,
    #[serde(default)]
    pub message_bytes: Option<i64>,
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub policy: Option<String>,
    /// `classic`, `quorum` or `stream`.
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub arguments: Map<String, Value>,
    #[serde(default)]
    pub message_stats: Option<MessageStats>,
    #[serde(default)]
    pub leader: Option<String>,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub online: Vec<String>,
    #[serde(default)]
    pub single_active_consumer_tag: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExchangeDto {
    pub name: String,
    #[serde(default)]
    pub vhost: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub durable: bool,
    #[serde(default)]
    pub auto_delete: bool,
    #[serde(default)]
    pub internal: bool,
    #[serde(default)]
    pub arguments: Map<String, Value>,
    #[serde(default)]
    pub message_stats: Option<MessageStats>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QueueRef {
    pub name: String,
    #[serde(default)]
    pub vhost: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChannelDetails {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub number: Option<u32>,
    #[serde(default)]
    pub peer_host: Option<String>,
    #[serde(default)]
    pub peer_port: Option<u32>,
    #[serde(default)]
    pub connection_name: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConsumerDto {
    pub consumer_tag: String,
    #[serde(default)]
    pub ack_required: bool,
    #[serde(default)]
    pub prefetch_count: u32,
    #[serde(default)]
    pub active: bool,
    /// `up`, `idle`, `blocked`, `flow` (RabbitMQ 3.13+) — absent on older nodes.
    #[serde(default)]
    pub activity_status: Option<String>,
    #[serde(default)]
    pub queue: Option<QueueRef>,
    #[serde(default)]
    pub channel_details: Option<ChannelDetails>,
    #[serde(default)]
    pub arguments: Map<String, Value>,
}

/// One message returned by `POST /api/queues/{vhost}/{name}/get`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PeekedMessage {
    #[serde(default)]
    pub payload: Option<String>,
    #[serde(default)]
    pub payload_bytes: Option<i64>,
    /// `string` or `base64`.
    #[serde(default)]
    pub payload_encoding: Option<String>,
    #[serde(default)]
    pub properties: Map<String, Value>,
    #[serde(default)]
    pub routing_key: Option<String>,
    #[serde(default)]
    pub exchange: Option<String>,
    #[serde(default)]
    pub redelivered: Option<bool>,
    #[serde(default)]
    pub message_count: Option<i64>,
}

/// How `/get` should treat the messages it returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckMode {
    /// Peek without removing — used by browse and tail.
    RequeueTrue,
    /// Peek and remove.
    AckRequeueFalse,
    /// Peek and dead letter.
    RejectRequeueTrue,
}

impl AckMode {
    fn as_str(self) -> &'static str {
        match self {
            AckMode::RequeueTrue => "ack_requeue_true",
            AckMode::AckRequeueFalse => "ack_requeue_false",
            AckMode::RejectRequeueTrue => "reject_requeue_true",
        }
    }
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ManagementApi {
    client: Client,
    base: Url,
    username: String,
    password: String,
}

impl ManagementApi {
    /// Build a client from the profile. Returns `None` when the management API
    /// is switched off.
    pub fn new(options: &RabbitMqOptions) -> AppResult<Option<Self>> {
        let Some(base) = options.management_base() else {
            return Ok(None);
        };
        let base = Url::parse(&base).map_err(|error| {
            AppError::invalid(format!("invalid management url `{base}`: {error}"))
        })?;
        let (username, password) = options.management_credentials();
        Ok(Some(Self {
            client: options.client()?,
            base,
            username,
            password,
        }))
    }

    /// One cheap round trip used to decide whether the plugin is available.
    pub async fn overview(&self) -> AppResult<Overview> {
        self.get(&["api", "overview"]).await
    }

    pub async fn nodes(&self) -> AppResult<Vec<NodeDto>> {
        self.get(&["api", "nodes"]).await
    }

    pub async fn queues(&self, vhost: &str) -> AppResult<Vec<QueueDto>> {
        self.get(&["api", "queues", vhost]).await
    }

    pub async fn queue(&self, vhost: &str, name: &str) -> AppResult<QueueDto> {
        self.get(&["api", "queues", vhost, name]).await
    }

    pub async fn queue_optional(&self, vhost: &str, name: &str) -> AppResult<Option<QueueDto>> {
        self.get_optional(&["api", "queues", vhost, name]).await
    }

    pub async fn declare_queue(&self, vhost: &str, name: &str, body: Value) -> AppResult<()> {
        self.put(&["api", "queues", vhost, name], body).await
    }

    pub async fn delete_queue(&self, vhost: &str, name: &str) -> AppResult<()> {
        self.delete(&["api", "queues", vhost, name]).await
    }

    pub async fn purge_queue(&self, vhost: &str, name: &str) -> AppResult<()> {
        self.delete(&["api", "queues", vhost, name, "contents"])
            .await
    }

    pub async fn exchanges(&self, vhost: &str) -> AppResult<Vec<ExchangeDto>> {
        self.get(&["api", "exchanges", vhost]).await
    }

    pub async fn exchange(&self, vhost: &str, name: &str) -> AppResult<ExchangeDto> {
        self.get(&["api", "exchanges", vhost, name]).await
    }

    pub async fn exchange_optional(
        &self,
        vhost: &str,
        name: &str,
    ) -> AppResult<Option<ExchangeDto>> {
        self.get_optional(&["api", "exchanges", vhost, name]).await
    }

    pub async fn declare_exchange(&self, vhost: &str, name: &str, body: Value) -> AppResult<()> {
        self.put(&["api", "exchanges", vhost, name], body).await
    }

    pub async fn delete_exchange(&self, vhost: &str, name: &str) -> AppResult<()> {
        self.delete(&["api", "exchanges", vhost, name]).await
    }

    /// Bindings where the queue is the destination.
    pub async fn queue_bindings(&self, vhost: &str, name: &str) -> AppResult<Vec<Value>> {
        self.get(&["api", "queues", vhost, name, "bindings"]).await
    }

    /// Bindings where the exchange is the source.
    pub async fn exchange_bindings(&self, vhost: &str, name: &str) -> AppResult<Vec<Value>> {
        self.get(&["api", "exchanges", vhost, name, "bindings", "source"])
            .await
    }

    pub async fn consumers(&self, vhost: &str) -> AppResult<Vec<ConsumerDto>> {
        self.get(&["api", "consumers", vhost]).await
    }

    /// Non destructive peek at the head of a queue.
    pub async fn peek(
        &self,
        vhost: &str,
        name: &str,
        count: u32,
        mode: AckMode,
    ) -> AppResult<Vec<PeekedMessage>> {
        let body = json!({
            "count": count.clamp(1, 500),
            "ackmode": mode.as_str(),
            "encoding": "auto",
            // Larger than the default 50 kB so that browse shows real payloads.
            "truncate": 1_048_576,
        });
        self.post(&["api", "queues", vhost, name, "get"], body)
            .await
    }

    // -- plumbing ---------------------------------------------------------

    fn endpoint(&self, segments: &[&str]) -> AppResult<Url> {
        let mut url = self.base.clone();
        {
            let mut path = url.path_segments_mut().map_err(|_| {
                AppError::invalid("the management url cannot be used as a base url")
            })?;
            path.pop_if_empty();
            for segment in segments {
                path.push(segment);
            }
        }
        Ok(url)
    }

    fn request(&self, method: reqwest::Method, segments: &[&str]) -> AppResult<RequestBuilder> {
        let url = self.endpoint(segments)?;
        Ok(self
            .client
            .request(method, url)
            .basic_auth(&self.username, Some(&self.password))
            .header(reqwest::header::ACCEPT, "application/json"))
    }

    async fn get<T: DeserializeOwned>(&self, segments: &[&str]) -> AppResult<T> {
        let response = self
            .request(reqwest::Method::GET, segments)?
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;
        self.decode(response).await
    }

    async fn get_optional<T: DeserializeOwned>(&self, segments: &[&str]) -> AppResult<Option<T>> {
        let response = self
            .request(reqwest::Method::GET, segments)?
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        self.decode(response).await.map(Some)
    }

    async fn put(&self, segments: &[&str], body: Value) -> AppResult<()> {
        let response = self
            .request(reqwest::Method::PUT, segments)?
            .json(&body)
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;
        self.discard(response).await
    }

    async fn post<T: DeserializeOwned>(&self, segments: &[&str], body: Value) -> AppResult<T> {
        let response = self
            .request(reqwest::Method::POST, segments)?
            .json(&body)
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;
        self.decode(response).await
    }

    async fn delete(&self, segments: &[&str]) -> AppResult<()> {
        let response = self
            .request(reqwest::Method::DELETE, segments)?
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;
        self.discard(response).await
    }

    async fn decode<T: DeserializeOwned>(&self, response: reqwest::Response) -> AppResult<T> {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(self.api_error(status, &body));
        }
        serde_json::from_str(&body).map_err(|error| {
            AppError::broker(format!(
                "could not parse the management API response ({error}): {}",
                truncate(&body, 400)
            ))
        })
    }

    async fn discard(&self, response: reqwest::Response) -> AppResult<()> {
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        Err(self.api_error(status, &body))
    }

    fn api_error(&self, status: StatusCode, body: &str) -> AppError {
        let detail = extract_reason(body);
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => AppError::broker(format!(
                "the management API rejected `{}` ({status}): {detail}. Check the management credentials and that the user has the `management` tag.",
                self.username
            )),
            StatusCode::NOT_FOUND => AppError::broker(format!("not found: {detail}")),
            _ => AppError::broker(format!("management API error ({status}): {detail}")),
        }
    }

    fn transport_error(&self, error: reqwest::Error) -> AppError {
        if error.is_timeout() {
            AppError::Timeout(0)
        } else {
            AppError::broker(format!(
                "cannot reach the RabbitMQ management API at {}: {error}. \
                 Enable the `rabbitmq_management` plugin or set `managementApi` to false.",
                self.base
            ))
        }
    }
}

/// RabbitMQ errors are `{"error": "...", "reason": "..."}`; fall back to the raw body.
pub fn extract_reason(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            let reason = value.get("reason").and_then(Value::as_str);
            let error = value.get("error").and_then(Value::as_str);
            match (error, reason) {
                (Some(error), Some(reason)) => Some(format!("{error}: {reason}")),
                (None, Some(reason)) => Some(reason.to_string()),
                (Some(error), None) => Some(error.to_string()),
                _ => None,
            }
        })
        .unwrap_or_else(|| truncate(body, 400))
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_string()
    } else {
        format!("{}…", &value[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mq::drivers::rabbitmq::options::RabbitMqOptions;
    use std::time::Duration;

    fn api() -> ManagementApi {
        let options = RabbitMqOptions {
            host: "localhost".into(),
            port: 5672,
            vhost: "/".into(),
            username: "guest".into(),
            password: "guest".into(),
            use_tls: false,
            management_url: Some("http://localhost:15672".into()),
            management_username: "guest".into(),
            management_password: "guest".into(),
            management_enabled: true,
            connection_name: "test".into(),
            heartbeat: 60,
            timeout: Duration::from_secs(5),
        };
        ManagementApi::new(&options).unwrap().unwrap()
    }

    #[test]
    fn encodes_the_default_vhost() {
        let url = api().endpoint(&["api", "queues", "/", "orders"]).unwrap();
        assert_eq!(url.as_str(), "http://localhost:15672/api/queues/%2F/orders");
    }

    #[test]
    fn keeps_custom_vhosts_readable() {
        let url = api()
            .endpoint(&["api", "queues", "team-a", "orders"])
            .unwrap();
        assert_eq!(
            url.as_str(),
            "http://localhost:15672/api/queues/team-a/orders"
        );
    }

    #[test]
    fn reads_the_error_reason() {
        assert_eq!(
            extract_reason(r#"{"error":"Object Not Found","reason":"Not Found"}"#),
            "Object Not Found: Not Found"
        );
    }
}
