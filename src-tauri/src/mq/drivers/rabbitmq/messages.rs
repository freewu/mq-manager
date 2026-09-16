//! Producing, browsing and tailing RabbitMQ queues.
//!
//! Publishing is real AMQP with publisher confirms. Reading is *never*
//! destructive: it goes through the management API's `get` endpoint with
//! `ack_requeue_true`, which peeks at the head of the queue without removing
//! anything.
//!
//! That has one honest consequence — a peek always starts at the head, so
//! browsing shows the oldest ready messages and tailing only sees messages as
//! long as they are inside the peek window. Nothing is ever lost or consumed on
//! the user's behalf.

use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};
use crate::mq::provider::StreamEvent;
use crate::mq::types::*;

use super::api::PeekedMessage;
use super::RabbitMqConnection;

/// Hard ceiling for a single peek; the management API caps `count` as well.
const MAX_PEEK: usize = 1_000;
/// How many messages one tail poll looks at.
const TAIL_WINDOW: u32 = 100;
/// Delay between two tail polls.
const TAIL_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// How many recently seen message fingerprints are remembered while tailing.
const SEEN_CAPACITY: usize = 4_096;

impl RabbitMqConnection {
    pub(crate) async fn produce_impl(
        &self,
        mut request: ProduceRequest,
    ) -> AppResult<ProduceResult> {
        // The workspace lists exchanges as well as queues. Publishing “to an
        // exchange” means using it instead of the default one, keeping the
        // topic name as the routing key.
        let has_exchange = request
            .options
            .get("exchange")
            .and_then(Value::as_str)
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        if !has_exchange && self.is_exchange(request.topic.trim()) {
            request
                .options
                .insert("exchange".into(), Value::String(request.topic.clone()));
        }
        self.amqp().publish(&request).await
    }

    pub(crate) async fn browse_impl(&self, request: BrowseRequest) -> AppResult<BrowseResult> {
        let started = Instant::now();
        let api = self.api()?;
        let vhost = self.vhost().to_string();
        let queue = self.require_queue(&request.topic).await?;
        let limit = request.limit.clamp(1, MAX_PEEK);

        let peeked = api
            .peek(
                &vhost,
                &queue,
                limit as u32,
                super::api::AckMode::RequeueTrue,
            )
            .await?;
        let available = peeked
            .first()
            .and_then(|message| message.message_count)
            .unwrap_or(peeked.len() as i64);

        let scanned = peeked.len();
        let mut messages: Vec<Message> = peeked
            .into_iter()
            .map(|message| message_from_peek(&queue, message))
            .collect();
        apply_keyword(&mut messages, request.keyword.as_deref());

        Ok(BrowseResult {
            // `truncated` means: the window was full, more messages are waiting.
            truncated: scanned >= limit && available > scanned as i64,
            messages,
            scanned,
            elapsed_ms: started.elapsed().as_millis() as u64,
            watermarks: vec![PartitionInfo {
                id: 0,
                begin_offset: None,
                end_offset: Some(available),
                message_count: Some(available),
                ..Default::default()
            }],
        })
    }

    pub(crate) async fn open_stream_impl(
        &self,
        request: StreamRequest,
    ) -> AppResult<mpsc::Receiver<StreamEvent>> {
        let api = self.api()?.clone();
        let vhost = self.vhost().to_string();
        let queue = self.require_queue(&request.topic).await?;
        let idle_timeout =
            Duration::from_millis(request.idle_timeout_ms.unwrap_or(15_000).max(1_000));
        let max_messages = request.max_messages;

        let (sender, receiver) = mpsc::channel::<StreamEvent>(64);
        tokio::spawn(async move {
            let outcome =
                tail_loop(&api, &vhost, &queue, max_messages, idle_timeout, &sender).await;
            let final_event = match outcome {
                Ok(()) => StreamEvent::End,
                Err(error) => StreamEvent::Failed(error.to_string()),
            };
            let _ = sender.send(final_event).await;
        });
        Ok(receiver)
    }

    /// Resolve `topic` to an existing queue name, the only entity that holds
    /// messages.
    pub(crate) async fn require_queue(&self, topic: &str) -> AppResult<String> {
        let api = self.api()?;
        let name = topic.trim();
        if name.is_empty() {
            return Err(AppError::invalid("no queue selected"));
        }
        api.queue_optional(self.vhost(), name)
            .await?
            .map(|queue| queue.name)
            .ok_or_else(|| {
                AppError::invalid(format!(
                    "`{name}` is not a queue in virtual host `{}` — only queues store messages",
                    self.vhost()
                ))
            })
    }
}

// ---------------------------------------------------------------------------
// Tailing
// ---------------------------------------------------------------------------

async fn tail_loop(
    api: &super::api::ManagementApi,
    vhost: &str,
    queue: &str,
    max_messages: u64,
    idle_timeout: Duration,
    sender: &mpsc::Sender<StreamEvent>,
) -> AppResult<()> {
    let mut seen = SeenMessages::default();
    let mut sent: u64 = 0;
    let mut last_activity = Instant::now();

    loop {
        let peeked = api
            .peek(vhost, queue, TAIL_WINDOW, super::api::AckMode::RequeueTrue)
            .await?;
        let mut batch: Vec<Message> = Vec::new();
        for message in peeked {
            if !seen.insert(&message) {
                continue;
            }
            batch.push(message_from_peek(queue, message));
        }

        if batch.is_empty() {
            if last_activity.elapsed() >= idle_timeout {
                if sender.send(StreamEvent::Idle).await.is_err() {
                    return Ok(());
                }
                last_activity = Instant::now();
            }
            tokio::time::sleep(TAIL_POLL_INTERVAL).await;
            continue;
        }

        last_activity = Instant::now();
        if max_messages > 0 {
            let remaining = max_messages.saturating_sub(sent) as usize;
            batch.truncate(remaining.max(1));
        }
        sent += batch.len() as u64;
        let reached_limit = max_messages > 0 && sent >= max_messages;
        if sender.send(StreamEvent::Batch(batch)).await.is_err() {
            return Ok(());
        }
        if reached_limit {
            return Ok(());
        }
        tokio::time::sleep(TAIL_POLL_INTERVAL).await;
    }
}

/// Remembers the fingerprints of the messages already emitted.
///
/// A peek always returns the head of the queue, so without this the same
/// messages would be streamed again on every poll.
#[derive(Default)]
struct SeenMessages {
    order: VecDeque<u64>,
    set: HashSet<u64>,
}

impl SeenMessages {
    fn insert(&mut self, message: &PeekedMessage) -> bool {
        let fingerprint = fingerprint(message);
        if !self.set.insert(fingerprint) {
            return false;
        }
        self.order.push_back(fingerprint);
        while self.order.len() > SEEN_CAPACITY {
            if let Some(oldest) = self.order.pop_front() {
                self.set.remove(&oldest);
            }
        }
        true
    }
}

fn fingerprint(message: &PeekedMessage) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    message.payload.hash(&mut hasher);
    message.payload_bytes.hash(&mut hasher);
    message.routing_key.hash(&mut hasher);
    message.exchange.hash(&mut hasher);
    message.properties.get("message_id").hash(&mut hasher);
    message.properties.get("timestamp").hash(&mut hasher);
    message
        .properties
        .get("headers")
        .map(|value| value.to_string())
        .hash(&mut hasher);
    message.redelivered.hash(&mut hasher);
    hasher.finish()
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

fn message_from_peek(queue: &str, peeked: PeekedMessage) -> Message {
    let encoding = match peeked.payload_encoding.as_deref() {
        Some("base64") => PayloadEncoding::Base64,
        _ => PayloadEncoding::Utf8,
    };
    let properties = &peeked.properties;
    let key = property_string(properties, "message_id")
        .or_else(|| property_string(properties, "correlation_id"));
    let producer =
        property_string(properties, "app_id").or_else(|| property_string(properties, "user_id"));

    Message {
        id: key
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        topic: queue.to_string(),
        // AMQP is not partitioned; the UI shows a single pseudo partition.
        partition: Some(0),
        // No offsets in AMQP, but the peek reports the queue depth.
        offset: None,
        timestamp: property_timestamp(properties),
        producer,
        key,
        key_encoding: PayloadEncoding::Utf8,
        payload: peeked.payload.clone(),
        encoding,
        headers: headers_from(properties),
        size: peeked
            .payload_bytes
            .unwrap_or_else(|| {
                peeked
                    .payload
                    .as_ref()
                    .map(|value| value.len())
                    .unwrap_or(0) as i64
            })
            .max(0) as usize,
    }
}

fn headers_from(properties: &serde_json::Map<String, Value>) -> Vec<MessageHeader> {
    let mut headers = Vec::new();
    // AMQP basic properties are the useful, well known ones; the free-form
    // `headers` table follows.
    for field in [
        "content_type",
        "content_encoding",
        "delivery_mode",
        "priority",
        "correlation_id",
        "reply_to",
        "expiration",
        "type",
        "user_id",
        "app_id",
    ] {
        if let Some(value) = properties.get(field).filter(|value| !value.is_null()) {
            headers.push(MessageHeader {
                key: field.to_string(),
                value: Some(render(value)),
                encoding: PayloadEncoding::Utf8,
            });
        }
    }
    if let Some(Value::Object(table)) = properties.get("headers") {
        for (key, value) in table {
            headers.push(MessageHeader {
                key: key.clone(),
                value: Some(render(value)),
                encoding: PayloadEncoding::Utf8,
            });
        }
    }
    headers
}

/// AMQP timestamps are POSIX **seconds**; the neutral model uses milliseconds.
fn property_timestamp(properties: &serde_json::Map<String, Value>) -> Option<i64> {
    let value = properties.get("timestamp").and_then(|value| match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    })?;
    if value <= 0 {
        return None;
    }
    // Anything below ~1973 in milliseconds is really a seconds value.
    Some(if value < 100_000_000_000 {
        value * 1_000
    } else {
        value
    })
}

fn property_string(properties: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    properties
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn render(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn apply_keyword(messages: &mut Vec<Message>, keyword: Option<&str>) {
    let Some(keyword) = keyword
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    messages.retain(|message| {
        message
            .payload
            .as_ref()
            .map(|payload| payload.to_lowercase().contains(&keyword))
            .unwrap_or(false)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peek(payload: &str) -> PeekedMessage {
        PeekedMessage {
            payload: Some(payload.to_string()),
            payload_bytes: Some(payload.len() as i64),
            payload_encoding: Some("string".into()),
            ..Default::default()
        }
    }

    #[test]
    fn tail_deduplicates_the_head() {
        let mut seen = SeenMessages::default();
        assert!(seen.insert(&peek("a")));
        assert!(!seen.insert(&peek("a")));
        assert!(seen.insert(&peek("b")));
    }

    #[test]
    fn converts_seconds_to_milliseconds() {
        let mut properties = serde_json::Map::new();
        properties.insert("timestamp".into(), Value::Number(1_700_000_000i64.into()));
        assert_eq!(property_timestamp(&properties), Some(1_700_000_000_000));
        properties.insert(
            "timestamp".into(),
            Value::Number(1_700_000_000_000i64.into()),
        );
        assert_eq!(property_timestamp(&properties), Some(1_700_000_000_000));
    }
}
