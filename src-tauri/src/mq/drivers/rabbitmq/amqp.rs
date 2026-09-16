//! Thin AMQP 0-9-1 layer.
//!
//! Publishing goes through a real AMQP connection (with publisher confirms) so
//! that authentication, vhost permissions, exchanges, bindings and routing keys
//! all behave exactly like any other client. Browsing, listing and peeking use
//! the management API instead, which cannot consume (or lose) messages.

use std::time::Instant;

use base64::Engine;
use lapin::options::{BasicPublishOptions, ConfirmSelectOptions};
use lapin::protocol::constants::REPLY_SUCCESS;
use lapin::types::{AMQPValue, FieldTable, LongString, ShortString};
use lapin::{BasicProperties, Confirmation, Connection, ConnectionProperties};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::mq::types::{PayloadEncoding, ProduceRequest, ProduceResult};

use super::options::RabbitMqOptions;

/// The AMQP protocol has no partitioning; everything is reported as queue `0`.
pub const DEFAULT_PARTITION: i32 = 0;

pub fn amqp_err<E: std::fmt::Display>(error: E) -> AppError {
    AppError::broker(error.to_string())
}

pub struct AmqpClient {
    connection: Connection,
    /// Recreated on demand; kept alive between publishes to avoid a handshake.
    channel: Mutex<Option<lapin::Channel>>,
}

impl AmqpClient {
    pub async fn open(options: &RabbitMqOptions) -> AppResult<Self> {
        let uri = options.amqp_uri();
        let properties = ConnectionProperties::default()
            .with_connection_name(LongString::from(options.connection_name.clone()));
        let connection =
            tokio::time::timeout(options.timeout, Connection::connect(&uri, properties))
                .await
                .map_err(|_| AppError::Timeout(options.timeout.as_millis() as u64))?
                .map_err(|error| {
                    AppError::broker(format!(
                        "AMQP handshake with {} failed: {error}",
                        options.authority()
                    ))
                })?;
        Ok(Self {
            connection,
            channel: Mutex::new(None),
        })
    }

    pub fn is_connected(&self) -> bool {
        self.connection.status().connected()
    }

    /// Round trip that proves the connection is still usable.
    pub async fn ping(&self) -> AppResult<()> {
        if !self.is_connected() {
            return Err(AppError::broker("the AMQP connection is closed"));
        }
        let channel = self.connection.create_channel().await.map_err(amqp_err)?;
        channel
            .close(REPLY_SUCCESS, ShortString::from("ping"))
            .await
            .map_err(amqp_err)
    }

    /// Publish one message. Publisher confirms are enabled on the channel, so a
    /// successful return means the broker accepted the message.
    pub async fn publish(&self, request: &ProduceRequest) -> AppResult<ProduceResult> {
        let started = Instant::now();
        let payload = decode_payload(request)?;
        let properties = build_properties(request, &payload)?;
        let (exchange, routing_key) = destination(request);

        let channel = self.channel().await?;
        let confirm = channel
            .basic_publish(
                ShortString::from(exchange.as_str()),
                ShortString::from(routing_key.as_str()),
                BasicPublishOptions {
                    mandatory: bool_option(request, "mandatory", true),
                    immediate: false,
                },
                &payload,
                properties,
            )
            .await
            .map_err(amqp_err)?;

        match confirm.await.map_err(amqp_err)? {
            Confirmation::Ack(None) | Confirmation::NotRequested => Ok(ProduceResult {
                topic: request.topic.clone(),
                partition: DEFAULT_PARTITION,
                // AMQP has no per-message offset; queue depth is reported by the
                // management API instead.
                offset: 0,
                elapsed_ms: started.elapsed().as_millis() as u64,
            }),
            Confirmation::Ack(Some(returned)) | Confirmation::Nack(Some(returned)) => {
                Err(AppError::broker(format!(
                    "the broker did not route the message to `{exchange}`/`{routing_key}`: {} {}",
                    returned.reply_code, returned.reply_text
                )))
            }
            Confirmation::Nack(None) => Err(AppError::broker(
                "the broker rejected the message (basic.nack)",
            )),
        }
    }

    pub async fn close(&self) -> AppResult<()> {
        if let Some(channel) = self.channel.lock().await.take() {
            let _ = channel.close(REPLY_SUCCESS, ShortString::from("bye")).await;
        }
        if self.is_connected() {
            let _ = self
                .connection
                .close(REPLY_SUCCESS, ShortString::from("mq-manager disconnected"))
                .await;
        }
        Ok(())
    }

    /// Publish channel with confirms enabled, created on first use.
    async fn channel(&self) -> AppResult<lapin::Channel> {
        let mut guard = self.channel.lock().await;
        if let Some(channel) = guard.as_ref() {
            if channel.status().connected() {
                return Ok(channel.clone());
            }
        }
        let channel = self.connection.create_channel().await.map_err(amqp_err)?;
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await
            .map_err(amqp_err)?;
        *guard = Some(channel.clone());
        Ok(channel)
    }
}

/// Decode the payload according to the requested encoding.
fn decode_payload(request: &ProduceRequest) -> AppResult<Vec<u8>> {
    let raw = request.payload.clone().unwrap_or_default();
    match request.encoding {
        PayloadEncoding::Utf8 => Ok(raw.into_bytes()),
        PayloadEncoding::Base64 => base64::engine::general_purpose::STANDARD
            .decode(raw.trim())
            .map_err(|error| AppError::invalid(format!("payload is not valid base64: {error}"))),
    }
}

/// `(exchange, routing key)`. Publishing to a queue uses the default exchange
/// with the queue name as routing key.
fn destination(request: &ProduceRequest) -> (String, String) {
    let option = |key: &str| {
        request
            .options
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let exchange = option("exchange").unwrap_or_default();
    let routing_key = option("routingKey").unwrap_or_else(|| request.topic.clone());
    (exchange, routing_key)
}

fn bool_option(request: &ProduceRequest, key: &str, fallback: bool) -> bool {
    match request.options.get(key) {
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => matches!(value.as_str(), "true" | "1" | "yes"),
        _ => fallback,
    }
}

fn string_option(request: &ProduceRequest, key: &str) -> Option<String> {
    request
        .options
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

/// Content type defaults to JSON when the payload looks like JSON, which is
/// what most consumers expect from a management UI.
fn guess_content_type(request: &ProduceRequest, payload: &[u8]) -> String {
    if let Some(explicit) = string_option(request, "contentType") {
        return explicit;
    }
    if request.encoding == PayloadEncoding::Utf8
        && !payload.is_empty()
        && serde_json::from_slice::<Value>(payload).is_ok()
    {
        "application/json".to_string()
    } else {
        "text/plain".to_string()
    }
}

fn build_properties(request: &ProduceRequest, payload: &[u8]) -> AppResult<BasicProperties> {
    // The message key has no AMQP equivalent; exposing it as `message_id` keeps
    // it visible in the management UI and to consumers.
    let message_id = request
        .key
        .as_ref()
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut properties = BasicProperties::default()
        .with_content_type(ShortString::from(
            guess_content_type(request, payload).as_str(),
        ))
        .with_delivery_mode(
            string_option(request, "deliveryMode")
                .and_then(|value| value.parse::<u8>().ok())
                .unwrap_or(2),
        )
        .with_message_id(ShortString::from(message_id.as_str()))
        .with_app_id(ShortString::from("mq-manager"));

    if !payload.is_empty() {
        if let Some(encoding) = string_option(request, "contentEncoding") {
            properties = properties.with_content_encoding(ShortString::from(encoding.as_str()));
        }
    }
    if let Some(timestamp) = request.timestamp {
        properties = properties.with_timestamp(timestamp.max(0) as u64);
    }
    for (key, setter) in [
        ("correlationId", "correlation_id"),
        ("replyTo", "reply_to"),
        ("expiration", "expiration"),
        ("userId", "user_id"),
        ("type", "type"),
    ] {
        if let Some(value) = string_option(request, key) {
            let value = ShortString::from(value.as_str());
            properties = match setter {
                "correlation_id" => properties.with_correlation_id(value),
                "reply_to" => properties.with_reply_to(value),
                "expiration" => properties.with_expiration(value),
                "user_id" => properties.with_user_id(value),
                _ => properties.with_type(value),
            };
        }
    }
    if let Some(priority) = string_option(request, "priority").and_then(|value| value.parse().ok())
    {
        properties = properties.with_priority(priority);
    }

    let headers = build_headers(request)?;
    if headers.inner().is_empty() {
        Ok(properties)
    } else {
        Ok(properties.with_headers(headers))
    }
}

fn build_headers(request: &ProduceRequest) -> AppResult<FieldTable> {
    let mut table = FieldTable::default();
    for header in &request.headers {
        let Some(value) = header.value.as_ref() else {
            continue;
        };
        let key = ShortString::from(header.key.as_str());
        let value = match header.encoding {
            PayloadEncoding::Utf8 => AMQPValue::LongString(LongString::from(value.clone())),
            PayloadEncoding::Base64 => AMQPValue::ByteArray(
                base64::engine::general_purpose::STANDARD
                    .decode(value.trim())
                    .map_err(|error| {
                        AppError::invalid(format!(
                            "header `{}` is not valid base64: {error}",
                            header.key
                        ))
                    })?
                    .into(),
            ),
        };
        table.insert(key, value);
    }
    Ok(table)
}
