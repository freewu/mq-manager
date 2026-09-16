use std::collections::HashMap;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rdkafka::consumer::{BaseConsumer, Consumer};
use rdkafka::message::{Header, Headers, Message as KafkaMessage, OwnedHeaders};
use rdkafka::producer::FutureRecord;
use rdkafka::util::Timeout;
use rdkafka::{Offset, TopicPartitionList};
use tokio::sync::mpsc;

use super::{kerr, KafkaConnection, KafkaHandle};
use crate::error::{AppError, AppResult};
use crate::mq::provider::StreamEvent;
use crate::mq::types::*;

/// How many messages are flushed into one UI batch while tailing.
const STREAM_BATCH_SIZE: usize = 50;
/// How long a poll waits before we consider the tail idle.
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Hard ceiling so a stray `limit` value cannot lock up the UI.
const MAX_BROWSE_LIMIT: usize = 10_000;

impl KafkaConnection {
    pub(crate) async fn produce_impl(&self, request: ProduceRequest) -> AppResult<ProduceResult> {
        let started = Instant::now();
        let payload = decode_bytes(request.payload.as_deref(), request.encoding)?;
        let key = match request.key.as_deref() {
            Some(value) => Some(decode_bytes(Some(value), request.encoding)?),
            None => None,
        };

        let mut headers = OwnedHeaders::new();
        for header in &request.headers {
            let value = decode_bytes(header.value.as_deref(), header.encoding)?;
            headers = headers.insert(Header {
                key: &header.key,
                value: Some(value.as_slice()),
            });
        }

        let mut record = FutureRecord::<[u8], [u8]>::to(&request.topic).payload(payload.as_slice());
        if let Some(key) = key.as_ref() {
            record = record.key(key.as_slice());
        }
        if let Some(partition) = request.partition {
            record = record.partition(partition);
        }
        if let Some(timestamp) = request.timestamp {
            record = record.timestamp(timestamp);
        }
        if !request.headers.is_empty() {
            record = record.headers(headers);
        }

        let delivery = self
            .producer
            .send(record, Timeout::After(self.request_timeout()))
            .await
            .map_err(|(error, _)| kerr(error))?;

        Ok(ProduceResult {
            topic: request.topic,
            partition: delivery.partition,
            offset: delivery.offset,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }

    pub(crate) async fn browse_impl(&self, request: BrowseRequest) -> AppResult<BrowseResult> {
        let handle = self.handle();
        tokio::task::spawn_blocking(move || browse_blocking(&handle, request)).await?
    }

    pub(crate) async fn open_stream_impl(
        &self,
        request: StreamRequest,
    ) -> AppResult<mpsc::Receiver<StreamEvent>> {
        let handle = self.handle();
        // A dedicated OS thread keeps long lived tails away from the async
        // blocking pool, which must stay available for short admin calls.
        let (sender, receiver) = mpsc::channel::<StreamEvent>(64);
        std::thread::Builder::new()
            .name("mq-manager-tail".to_string())
            .spawn(move || {
                let outcome = stream_blocking(&handle, &request, &sender);
                let final_event = match outcome {
                    Ok(()) => StreamEvent::End,
                    Err(error) => StreamEvent::Failed(error.to_string()),
                };
                let _ = sender.blocking_send(final_event);
            })
            .map_err(AppError::Io)?;
        Ok(receiver)
    }
}

// ---------------------------------------------------------------------------
// Blocking implementations
// ---------------------------------------------------------------------------

struct Prepared {
    consumer: BaseConsumer,
    watermarks: Vec<PartitionInfo>,
}

fn prepare(
    handle: &KafkaHandle,
    topic: &str,
    selected: Option<&Vec<i32>>,
    start: &SeekPosition,
    timeout: Duration,
) -> AppResult<Prepared> {
    let metadata = handle
        .client()
        .fetch_metadata(Some(topic), handle.admin_timeout())
        .map_err(kerr)?;

    let topic_metadata = metadata
        .topics()
        .iter()
        .find(|candidate| candidate.name() == topic)
        .ok_or_else(|| AppError::invalid(format!("topic `{topic}` does not exist")))?;

    let mut partitions: Vec<i32> = topic_metadata
        .partitions()
        .iter()
        .map(|partition| partition.id())
        .collect();
    partitions.sort_unstable();

    if let Some(filter) = selected {
        partitions.retain(|partition| filter.contains(partition));
    }
    if partitions.is_empty() {
        return Err(AppError::invalid("no partition selected"));
    }

    let consumer = handle.consumer(&uuid::Uuid::new_v4().to_string())?;

    let mut assignment = TopicPartitionList::new();
    match start {
        SeekPosition::Beginning => {
            for partition in &partitions {
                assignment
                    .add_partition_offset(topic, *partition, Offset::Beginning)
                    .map_err(kerr)?;
            }
        }
        SeekPosition::End => {
            for partition in &partitions {
                assignment
                    .add_partition_offset(topic, *partition, Offset::End)
                    .map_err(kerr)?;
            }
        }
        SeekPosition::Offset { offset } => {
            for partition in &partitions {
                assignment
                    .add_partition_offset(topic, *partition, Offset::Offset(*offset))
                    .map_err(kerr)?;
            }
        }
        SeekPosition::Timestamp { timestamp } => {
            let mut requested = TopicPartitionList::new();
            for partition in &partitions {
                requested
                    .add_partition_offset(topic, *partition, Offset::Offset(*timestamp))
                    .map_err(kerr)?;
            }
            let resolved = consumer
                .offsets_for_times(requested, timeout)
                .map_err(kerr)?;
            let mut by_partition: HashMap<i32, Offset> = HashMap::new();
            for element in resolved.elements() {
                by_partition.insert(element.partition(), element.offset());
            }
            for partition in &partitions {
                let offset = by_partition
                    .get(partition)
                    .copied()
                    .filter(|offset| offset.to_raw().is_some())
                    .unwrap_or(Offset::End);
                assignment
                    .add_partition_offset(topic, *partition, offset)
                    .map_err(kerr)?;
            }
        }
    }

    consumer.assign(&assignment).map_err(kerr)?;

    let mut watermarks = Vec::new();
    for partition in &partitions {
        if let Ok((low, high)) = handle.client().fetch_watermarks(topic, *partition, timeout) {
            watermarks.push(PartitionInfo {
                id: *partition,
                begin_offset: Some(low),
                end_offset: Some(high),
                message_count: Some((high - low).max(0)),
                ..Default::default()
            });
        }
    }
    watermarks.sort_by_key(|partition| partition.id);

    Ok(Prepared {
        consumer,
        watermarks,
    })
}

fn browse_blocking(handle: &KafkaHandle, request: BrowseRequest) -> AppResult<BrowseResult> {
    let started = Instant::now();
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(5_000).clamp(200, 120_000));
    let limit = request.limit.clamp(1, MAX_BROWSE_LIMIT);
    let keyword = request
        .keyword
        .as_ref()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());

    let prepared = prepare(
        handle,
        &request.topic,
        request.partitions.as_ref(),
        &request.start,
        timeout,
    )?;

    let deadline = Instant::now() + timeout;
    let mut messages: Vec<Message> = Vec::new();
    let mut scanned = 0usize;

    while messages.len() < limit {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let budget = (deadline - now).min(POLL_INTERVAL);
        match prepared.consumer.poll(budget) {
            Some(Ok(record)) => {
                scanned += 1;
                let message = convert_message(&record);
                if let Some(keyword) = &keyword {
                    let haystack = message.payload.clone().unwrap_or_default().to_lowercase();
                    if !haystack.contains(keyword) {
                        continue;
                    }
                }
                messages.push(message);
            }
            Some(Err(error)) => {
                if messages.is_empty() {
                    return Err(kerr(error));
                }
                break;
            }
            None => {}
        }
    }

    // We stopped because the limit was reached — more data may be available.
    let truncated = messages.len() >= limit;

    sort_messages(&mut messages);

    Ok(BrowseResult {
        messages,
        truncated,
        scanned,
        elapsed_ms: started.elapsed().as_millis() as u64,
        watermarks: prepared.watermarks,
    })
}

fn stream_blocking(
    handle: &KafkaHandle,
    request: &StreamRequest,
    sender: &mpsc::Sender<StreamEvent>,
) -> AppResult<()> {
    let timeout = Duration::from_millis(5_000);
    let idle_timeout = Duration::from_millis(request.idle_timeout_ms.unwrap_or(15_000).max(1_000));
    let prepared = prepare(
        handle,
        &request.topic,
        request.partitions.as_ref(),
        &request.start,
        timeout,
    )?;

    let mut batch: Vec<Message> = Vec::new();
    let mut sent: u64 = 0;
    let mut last_activity = Instant::now();

    loop {
        match prepared.consumer.poll(POLL_INTERVAL) {
            Some(Ok(record)) => {
                batch.push(convert_message(&record));
                last_activity = Instant::now();
                sent += 1;

                let reached_limit = request.max_messages > 0 && sent >= request.max_messages;
                if batch.len() >= STREAM_BATCH_SIZE || reached_limit {
                    sort_messages(&mut batch);
                    if sender
                        .blocking_send(StreamEvent::Batch(std::mem::take(&mut batch)))
                        .is_err()
                    {
                        return Ok(());
                    }
                }
                if reached_limit {
                    return Ok(());
                }
            }
            Some(Err(error)) => return Err(kerr(error)),
            None => {
                if !batch.is_empty() {
                    sort_messages(&mut batch);
                    if sender
                        .blocking_send(StreamEvent::Batch(std::mem::take(&mut batch)))
                        .is_err()
                    {
                        return Ok(());
                    }
                } else if last_activity.elapsed() >= idle_timeout {
                    if sender.blocking_send(StreamEvent::Idle).is_err() {
                        return Ok(());
                    }
                    last_activity = Instant::now();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sort_messages(messages: &mut [Message]) {
    messages.sort_by(|a, b| {
        a.timestamp
            .unwrap_or_default()
            .cmp(&b.timestamp.unwrap_or_default())
            .then(
                a.partition
                    .unwrap_or_default()
                    .cmp(&b.partition.unwrap_or_default()),
            )
            .then(
                a.offset
                    .unwrap_or_default()
                    .cmp(&b.offset.unwrap_or_default()),
            )
    });
}

pub(crate) fn encode_bytes(bytes: &[u8]) -> (String, PayloadEncoding) {
    match std::str::from_utf8(bytes) {
        Ok(text)
            if !text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) =>
        {
            (text.to_string(), PayloadEncoding::Utf8)
        }
        _ => (BASE64.encode(bytes), PayloadEncoding::Base64),
    }
}

fn decode_bytes(value: Option<&str>, encoding: PayloadEncoding) -> AppResult<Vec<u8>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    match encoding {
        PayloadEncoding::Utf8 => Ok(value.as_bytes().to_vec()),
        PayloadEncoding::Base64 => BASE64
            .decode(value.trim())
            .map_err(|error| AppError::invalid(format!("invalid base64 payload: {error}"))),
    }
}

pub(crate) fn convert_message<M: KafkaMessage>(record: &M) -> Message {
    let (payload, payload_encoding) = match record.payload() {
        Some(bytes) => {
            let (text, encoding) = encode_bytes(bytes);
            (Some(text), encoding)
        }
        None => (None, PayloadEncoding::Utf8),
    };

    let (key, key_encoding) = match record.key() {
        Some(bytes) => {
            let (text, encoding) = encode_bytes(bytes);
            (Some(text), encoding)
        }
        None => (None, PayloadEncoding::Utf8),
    };

    let mut headers = Vec::new();
    let mut header_size = 0usize;
    if let Some(borrowed) = record.headers() {
        for header in borrowed.iter() {
            let (value, encoding) = match header.value {
                Some(bytes) => {
                    let (text, encoding) = encode_bytes(bytes);
                    header_size += bytes.len() + header.key.len();
                    (Some(text), encoding)
                }
                None => (None, PayloadEncoding::Utf8),
            };
            headers.push(MessageHeader {
                key: header.key.to_string(),
                value,
                encoding,
            });
        }
    }

    let size = record.payload().map(|bytes| bytes.len()).unwrap_or(0)
        + record.key().map(|bytes| bytes.len()).unwrap_or(0)
        + header_size;

    Message {
        id: uuid::Uuid::new_v4().to_string(),
        topic: record.topic().to_string(),
        partition: Some(record.partition()),
        offset: Some(record.offset()),
        timestamp: record.timestamp().to_millis(),
        producer: None,
        key,
        payload,
        encoding: payload_encoding,
        key_encoding,
        headers,
        size,
    }
}
