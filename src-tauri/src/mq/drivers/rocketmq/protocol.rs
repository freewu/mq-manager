//! RocketMQ remoting protocol: frames, commands and the store message format.
//!
//! RocketMQ speaks a tiny binary framing protocol over plain TCP:
//!
//! ```text
//! | total length (i32) | header length (i32, high byte = serialization type) |
//! | header (JSON or RocketMQ binary) | body |
//! ```
//!
//! The total length excludes its own four bytes. Only the JSON serialization
//! type (0) is produced and understood here — it is what every 4.x broker and
//! the 5.x compatibility layer accept, and it keeps the codec dependency free
//! (`serde_json` instead of a bespoke binary reader).
//!
//! The request and response code tables mirror `RequestCode` / `ResponseCode`
//! from the Java client in full, so a few entries are only kept for reference
//! and for reading broker traffic by hand.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::Read;

use flate2::read::ZlibDecoder;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::mq::types::{Message, MessageHeader, PayloadEncoding};

/// Serialization type flag stored in the high byte of the header length.
pub const SERIALIZE_JSON: u32 = 0;
/// Serialization type used by the Java client when talking to 5.x brokers.
pub const SERIALIZE_ROCKETMQ: u32 = 1;

/// The largest frame we are willing to allocate, as a guard against garbage.
const MAX_FRAME: usize = 32 * 1024 * 1024;

/// Request codes (`RemotingCommand` / `RequestCode` in the Java client).
pub mod code {
    pub const SEND_MESSAGE: i32 = 310;
    pub const PULL_MESSAGE: i32 = 11;
    pub const QUERY_CONSUMER_OFFSET: i32 = 14;
    pub const UPDATE_CONSUMER_OFFSET: i32 = 15;
    pub const UPDATE_AND_CREATE_TOPIC: i32 = 17;
    pub const GET_ALL_TOPIC_CONFIG: i32 = 21;
    pub const GET_TOPIC_CONFIG_LIST: i32 = 22;
    pub const GET_BROKER_RUNTIME_INFO: i32 = 28;
    pub const SEARCH_OFFSET_BY_TIMESTAMP: i32 = 29;
    pub const GET_MIN_OFFSET: i32 = 30;
    pub const GET_MAX_OFFSET: i32 = 31;
    pub const GET_EARLIEST_MSG_STORETIME: i32 = 32;
    pub const VIEW_MESSAGE_BY_ID: i32 = 33;
    pub const HEART_BEAT: i32 = 34;
    pub const GET_CONSUMER_LIST_BY_GROUP: i32 = 38;
    pub const GET_ROUTEINFO_BY_TOPIC: i32 = 105;
    pub const GET_BROKER_CLUSTER_INFO: i32 = 106;
    pub const UPDATE_AND_CREATE_SUBSCRIPTIONGROUP: i32 = 200;
    pub const GET_ALL_SUBSCRIPTIONGROUP_CONFIG: i32 = 201;
    pub const DELETE_SUBSCRIPTIONGROUP: i32 = 202;
    pub const GET_CONSUMER_CONNECTION_LIST: i32 = 203;
    pub const GET_ALL_TOPIC_LIST_FROM_NAMESERVER: i32 = 206;
    pub const GET_CONSUME_STATS: i32 = 208;
    pub const GET_TOPIC_CONFIG: i32 = 212;
    pub const DELETE_TOPIC_IN_BROKER: i32 = 216;
    pub const DELETE_TOPIC_IN_NAMESRV: i32 = 217;
}

/// Response codes (`ResponseCode` in the Java client).
pub mod response_code {
    pub const SUCCESS: i32 = 0;
    pub const SYSTEM_ERROR: i32 = 1;
    pub const SYSTEM_BUSY: i32 = 2;
    pub const REQUEST_CODE_NOT_SUPPORTED: i32 = 3;
    pub const TOPIC_NOT_EXIST: i32 = 17;
    pub const TOPIC_EXIST_ALREADY: i32 = 18;
    pub const PULL_NOT_FOUND: i32 = 19;
    pub const PULL_RETRY_IMMEDIATELY: i32 = 20;
    pub const PULL_OFFSET_MOVED: i32 = 21;
    pub const QUERY_NOT_FOUND: i32 = 22;
    pub const SUBSCRIPTION_GROUP_NOT_EXIST: i32 = 26;
    pub const CONSUMER_NOT_ONLINE: i32 = 206;
    pub const NO_MESSAGE: i32 = 208;
}

/// Human readable text for a response code.
pub fn response_text(code: i32) -> String {
    let name = match code {
        response_code::SUCCESS => "success",
        response_code::SYSTEM_ERROR => "broker system error",
        response_code::SYSTEM_BUSY => "broker busy",
        response_code::REQUEST_CODE_NOT_SUPPORTED => "request code not supported by this broker",
        response_code::TOPIC_NOT_EXIST => "topic does not exist",
        response_code::TOPIC_EXIST_ALREADY => "topic already exists",
        response_code::PULL_NOT_FOUND => "no message found",
        response_code::PULL_RETRY_IMMEDIATELY => "retry immediately",
        response_code::PULL_OFFSET_MOVED => "the pull offset is not valid any more",
        response_code::QUERY_NOT_FOUND => "not found",
        response_code::SUBSCRIPTION_GROUP_NOT_EXIST => "subscription group does not exist",
        response_code::CONSUMER_NOT_ONLINE => "consumer group is not online",
        response_code::NO_MESSAGE => "no message",
        _ => "unknown",
    };
    format!("{name} (code {code})")
}

// ---------------------------------------------------------------------------
// Log/queue file message flags
// ---------------------------------------------------------------------------

/// `MessageSysFlag` bits used while reading stored messages.
pub mod sys_flag {
    pub const COMPRESSED: i32 = 0x1;
    pub const TRANSACTION_PREPARED: i32 = 0x4;
    pub const MULTI_TAGS: i32 = 0x8;
    pub const BORNHOST_V6: i32 = 0x10;
    pub const STOREHOST_V6: i32 = 0x20;
}

/// Properties are stored as one string: `KEY\u{1}VALUE\u{2}KEY\u{1}VALUE`.
const PROPERTY_SEPARATOR: char = '\u{2}';
const NAME_VALUE_SEPARATOR: char = '\u{1}';

// ---------------------------------------------------------------------------
// RemotingCommand
// ---------------------------------------------------------------------------

/// A decoded (or to-be-encoded) remoting command.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemotingCommand {
    pub code: i32,
    /// `JAVA`, `CPP`, `OTHER` — we present ourselves as `OTHER` when needed.
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub version: i32,
    #[serde(default)]
    pub opaque: i32,
    #[serde(default)]
    pub flag: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remark: Option<String>,
    #[serde(
        rename = "extFields",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub ext_fields: BTreeMap<String, String>,
    /// Never serialized as part of the JSON header — it travels as the body.
    #[serde(skip)]
    pub body: Vec<u8>,
}

impl RemotingCommand {
    /// Build a request command.
    pub fn request(code: i32, opaque: i32, ext_fields: BTreeMap<String, String>) -> Self {
        Self {
            code,
            language: "OTHER".to_string(),
            version: 401,
            opaque,
            // RPC type 0 = sync request, bit 0 = response flag (0 = request).
            flag: 0,
            remark: None,
            ext_fields,
            body: Vec::new(),
        }
    }

    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }

    pub fn with_ext(mut self, key: &str, value: impl Into<String>) -> Self {
        self.ext_fields.insert(key.to_string(), value.into());
        self
    }

    pub fn header_str(&self, key: &str) -> Option<&str> {
        self.ext_fields.get(key).map(String::as_str)
    }

    pub fn header_i64(&self, key: &str) -> Option<i64> {
        self.ext_fields
            .get(key)
            .and_then(|value| value.parse().ok())
    }

    pub fn header_i32(&self, key: &str) -> Option<i32> {
        self.ext_fields
            .get(key)
            .and_then(|value| value.parse().ok())
    }

    pub fn header_bool(&self, key: &str) -> Option<bool> {
        self.ext_fields
            .get(key)
            .and_then(|value| value.parse().ok())
    }

    /// Body interpreted as a UTF-8 string (RocketMQ sends JSON there a lot).
    pub fn body_str(&self) -> AppResult<&str> {
        std::str::from_utf8(&self.body)
            .map_err(|error| AppError::broker(format!("response body is not valid UTF-8: {error}")))
    }

    pub fn body_json<T: serde::de::DeserializeOwned>(&self) -> AppResult<T> {
        serde_json::from_slice(&self.body).map_err(|error| {
            AppError::broker(format!(
                "could not decode the RocketMQ response body: {error}"
            ))
        })
    }

    /// Serialize to one frame, length prefix included.
    pub fn encode_frame(&self) -> AppResult<Vec<u8>> {
        let header = serde_json::to_vec(self)
            .map_err(|error| AppError::broker(format!("could not encode the header: {error}")))?;
        if header.len() > 0xFF_FFFF {
            return Err(AppError::invalid("the RocketMQ header is too large"));
        }
        let total = 4 + header.len() + self.body.len();
        let mut frame = Vec::with_capacity(4 + total);
        frame.extend_from_slice(&(total as i32).to_be_bytes());
        let mark = ((SERIALIZE_JSON << 24) | (header.len() as u32 & 0xFF_FFFF)) as i32;
        frame.extend_from_slice(&mark.to_be_bytes());
        frame.extend_from_slice(&header);
        frame.extend_from_slice(&self.body);
        Ok(frame)
    }

    /// Decode the payload that follows the four length bytes.
    pub fn decode(payload: &[u8]) -> AppResult<Self> {
        if payload.len() < 4 {
            return Err(AppError::broker("truncated RocketMQ frame"));
        }
        let mark = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let serialize_type = mark >> 24;
        let header_len = (mark & 0xFF_FFFF) as usize;
        if payload.len() < 4 + header_len {
            return Err(AppError::broker("truncated RocketMQ header"));
        }
        let header = &payload[4..4 + header_len];
        let body = payload[4 + header_len..].to_vec();

        if serialize_type != SERIALIZE_JSON {
            return Err(AppError::broker(format!(
                "the broker answered with the ROCKETMQ binary serialization (type {serialize_type}), \
                 which is not supported yet"
            )));
        }

        let mut command: RemotingCommand = serde_json::from_slice(header).map_err(|error| {
            AppError::broker(format!("could not decode the RocketMQ header: {error}"))
        })?;
        command.body = body;
        Ok(command)
    }

    /// Length of the frame announced by a four byte prefix.
    pub fn frame_len(prefix: [u8; 4]) -> AppResult<usize> {
        let total = i32::from_be_bytes(prefix);
        if total < 4 || total as usize > MAX_FRAME {
            return Err(AppError::broker(format!(
                "invalid RocketMQ frame length {total}"
            )));
        }
        Ok(total as usize)
    }
}

// ---------------------------------------------------------------------------
// Stored messages
// ---------------------------------------------------------------------------

/// One message as stored on the broker (the layout of the commit log).
#[derive(Debug, Clone, Default)]
pub struct StoredMessage {
    pub topic: String,
    pub queue_id: i32,
    pub queue_offset: i64,
    pub physical_offset: i64,
    pub sys_flag: i32,
    pub born_timestamp: i64,
    pub store_timestamp: i64,
    pub born_host: Vec<u8>,
    pub store_host: Vec<u8>,
    pub reconsume_times: i32,
    pub flag: i32,
    pub body: Vec<u8>,
    pub properties: BTreeMap<String, String>,
    /// Message id computed from the born host + physical offset.
    pub msg_id: String,
}

impl StoredMessage {
    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }

    /// Convert to the neutral model.
    pub fn into_message(self) -> Message {
        let (payload, encoding) = crate::mq::drivers::codec::encode_bytes(&self.body);
        let mut headers: Vec<MessageHeader> = Vec::with_capacity(self.properties.len() + 3);
        for (key, value) in &self.properties {
            // These two are surfaced as first class fields.
            if key == "KEYS" || key == "UNIQ_KEY" || key == "TAGS" {
                continue;
            }
            headers.push(MessageHeader {
                key: key.clone(),
                // Message properties are always stored as UTF-8 text.
                value: Some(value.clone()),
                encoding: PayloadEncoding::Utf8,
            });
        }
        if let Some(tags) = self.property("TAGS") {
            headers.push(MessageHeader {
                key: "TAGS".into(),
                value: Some(tags.to_string()),
                encoding: PayloadEncoding::Utf8,
            });
        }

        Message {
            id: if self.msg_id.is_empty() {
                format!("{}:{}", self.physical_offset, self.queue_offset)
            } else {
                self.msg_id.clone()
            },
            topic: self.topic.clone(),
            partition: Some(self.queue_id),
            offset: Some(self.queue_offset),
            // The broker clock is the only timestamp RocketMQ stores.
            timestamp: Some(self.store_timestamp),
            producer: Some(host_string(&self.born_host)),
            key: self.property("KEYS").map(str::to_string),
            key_encoding: PayloadEncoding::Utf8,
            payload: Some(payload),
            encoding,
            headers,
            size: self.body.len(),
        }
    }
}

/// Parse a pull response body: `[i32 size][message]*`.
pub fn parse_pull_body(body: &[u8]) -> AppResult<Vec<StoredMessage>> {
    let mut messages = Vec::new();
    let mut cursor = 0usize;
    while cursor + 4 <= body.len() {
        let size = i32::from_be_bytes([
            body[cursor],
            body[cursor + 1],
            body[cursor + 2],
            body[cursor + 3],
        ]);
        cursor += 4;
        if size <= 0 {
            break;
        }
        let size = size as usize;
        let end = cursor.checked_add(size).filter(|end| *end <= body.len());
        let Some(end) = end else {
            // Truncated tail: keep what we already parsed.
            break;
        };
        if let Ok(message) = decode_message(&body[cursor..end]) {
            messages.push(message);
        }
        cursor = end;
    }
    Ok(messages)
}

/// Decode one commit-log entry.
pub fn decode_message(bytes: &[u8]) -> AppResult<StoredMessage> {
    // Fixed layout, offsets as documented in `MessageDecoder`:
    //   0 totalSize, 4 magicCode, 8 bodyCRC, 12 queueId, 16 flag,
    //   20 queueOffset, 28 physicalOffset, 36 sysFlag, 40 bornTimestamp,
    //   48 bornHost, 56 storeTimestamp, 64 storeHost, 72 reconsumeTimes,
    //   76 preparedTransactionOffset, 84 bodyLength, 88 body, ...
    if bytes.len() < 88 {
        return Err(AppError::broker("truncated RocketMQ message"));
    }
    let read_i32 = |offset: usize| -> i32 {
        i32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    let read_i64 = |offset: usize| -> i64 {
        i64::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ])
    };

    let magic = read_i32(4);
    if magic != 0xAABB_CCDD_u32 as i32 {
        return Err(AppError::broker(format!(
            "unsupported RocketMQ message magic code 0x{magic:08X}"
        )));
    }

    let queue_id = read_i32(12);
    let flag = read_i32(16);
    let queue_offset = read_i64(20);
    let physical_offset = read_i64(28);
    let sys_flag = read_i32(36);
    let born_timestamp = read_i64(40);
    let born_host_len = if sys_flag & sys_flag::BORNHOST_V6 != 0 {
        20
    } else {
        8
    };
    let store_host_len = if sys_flag & sys_flag::STOREHOST_V6 != 0 {
        20
    } else {
        8
    };
    let born_host = bytes[48..48 + born_host_len].to_vec();
    let store_timestamp = read_i64(48 + born_host_len);
    let store_host_start = 56 + born_host_len;
    let store_host = bytes[store_host_start..store_host_start + store_host_len].to_vec();
    let reconsume_times = read_i32(store_host_start + store_host_len);
    let body_length_offset = store_host_start + store_host_len + 12;
    let body_length = read_i32(body_length_offset).max(0) as usize;
    let body_start = body_length_offset + 4;
    let body_end = body_start
        .checked_add(body_length)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| AppError::broker("truncated RocketMQ message body"))?;
    let body = &bytes[body_start..body_end];

    // topic (1 byte length) then properties (2 byte length)
    let mut cursor = body_end;
    let mut topic = String::new();
    if cursor < bytes.len() {
        let topic_len = bytes[cursor] as usize;
        cursor += 1;
        if cursor + topic_len <= bytes.len() {
            topic = String::from_utf8_lossy(&bytes[cursor..cursor + topic_len]).to_string();
            cursor += topic_len;
        }
    }
    let mut properties = BTreeMap::new();
    if cursor + 2 <= bytes.len() {
        let properties_len = i16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]).max(0) as usize;
        cursor += 2;
        if cursor + properties_len <= bytes.len() {
            let raw = String::from_utf8_lossy(&bytes[cursor..cursor + properties_len]);
            properties = parse_properties(&raw);
        }
    }

    let compressed = sys_flag & sys_flag::COMPRESSED != 0;
    let body = if compressed {
        decompress(body)?
    } else {
        body.to_vec()
    };

    // The broker always sets UNIQ_KEY to the message id; recompute it when a
    // producer did not use the Java client.
    let msg_id = properties
        .get("UNIQ_KEY")
        .cloned()
        .unwrap_or_else(|| build_message_id(&born_host, physical_offset));

    Ok(StoredMessage {
        topic,
        queue_id,
        queue_offset,
        physical_offset,
        sys_flag,
        born_timestamp,
        store_timestamp,
        born_host,
        store_host,
        reconsume_times,
        flag,
        body,
        properties,
        msg_id,
    })
}

/// `KEY\u{1}VALUE\u{2}…` → map.
pub fn parse_properties(raw: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in raw.split(PROPERTY_SEPARATOR) {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, NAME_VALUE_SEPARATOR);
        let key = parts.next().unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        map.insert(
            key.to_string(),
            parts.next().unwrap_or_default().to_string(),
        );
    }
    map
}

/// Inverse of [`parse_properties`].
pub fn encode_properties(properties: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (key, value) in properties {
        out.push_str(key);
        out.push(NAME_VALUE_SEPARATOR);
        out.push_str(value);
        out.push(PROPERTY_SEPARATOR);
    }
    if !out.is_empty() {
        out.push(PROPERTY_SEPARATOR);
    }
    out
}

fn decompress(body: &[u8]) -> AppResult<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(body);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(|error| {
        AppError::broker(format!("could not inflate the message body: {error}"))
    })?;
    Ok(out)
}

/// Java: `MessageDecoder.createMessageId` — host bytes + offset, hex encoded.
pub fn build_message_id(host: &[u8], physical_offset: i64) -> String {
    let mut bytes = Vec::with_capacity(host.len() + 8);
    bytes.extend_from_slice(host);
    bytes.extend_from_slice(&physical_offset.to_be_bytes());
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

/// `ip:port` of a stored host field (4 or 16 bytes + 4 byte port).
pub fn host_string(host: &[u8]) -> String {
    if host.len() >= 20 {
        let ip: [u8; 16] = host[..16].try_into().unwrap_or([0; 16]);
        let port = i32::from_be_bytes(host[16..20].try_into().unwrap_or([0; 4]));
        format!("[{}]:{port}", std::net::Ipv6Addr::from(ip))
    } else if host.len() >= 8 {
        let ip = std::net::Ipv4Addr::new(host[0], host[1], host[2], host[3]);
        let port = i32::from_be_bytes(host[4..8].try_into().unwrap_or([0; 4]));
        format!("{ip}:{port}")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_command() {
        let command = RemotingCommand::request(code::GET_MAX_OFFSET, 7, BTreeMap::new())
            .with_ext("topic", "orders")
            .with_ext("queueId", "2")
            .with_body(b"{}".to_vec());

        let frame = command.encode_frame().unwrap();
        let total = RemotingCommand::frame_len(frame[..4].try_into().unwrap()).unwrap();
        assert_eq!(total, frame.len() - 4);
        let decoded = RemotingCommand::decode(&frame[4..]).unwrap();
        assert_eq!(decoded.code, code::GET_MAX_OFFSET);
        assert_eq!(decoded.opaque, 7);
        assert_eq!(decoded.header_str("topic"), Some("orders"));
        assert_eq!(decoded.body, b"{}");
    }

    #[test]
    fn parses_properties() {
        let map = parse_properties("TAGS\u{1}tagA\u{2}KEYS\u{1}k1\u{2}");
        assert_eq!(map.get("TAGS").map(String::as_str), Some("tagA"));
        assert_eq!(map.get("KEYS").map(String::as_str), Some("k1"));
        assert_eq!(
            encode_properties(&map),
            "KEYS\u{1}k1\u{2}TAGS\u{1}tagA\u{2}\u{2}"
        );
    }

    #[test]
    fn rejects_a_binary_header() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&((SERIALIZE_ROCKETMQ << 24 | 0) as i32).to_be_bytes());
        assert!(RemotingCommand::decode(&payload).is_err());
    }

    #[test]
    fn builds_a_message_id() {
        let id = build_message_id(&[127, 0, 0, 1, 0, 0, 0x2A, 0x9B], 0x1234);
        assert_eq!(id, "7F00000100002A9B0000000000001234");
    }
}
