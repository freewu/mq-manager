//! Producing, browsing and tailing RocketMQ topics.
//!
//! RocketMQ has no `basic.get` style non destructive browse. The only way to
//! read stored messages without consuming them is to *pull* with a throw-away
//! consumer group and never commit the offset — which is exactly what this
//! module does. The group is created on demand (RocketMQ refuses pulls from
//! unknown groups) and is never used by a real consumer, so offsets written to
//! it can never make another application skip messages.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use super::protocol::{self, code, response_code, RemotingCommand, StoredMessage};
use super::topics::{masters, route};
use super::RocketMqConnection;
use crate::error::{AppError, AppResult};
use crate::mq::drivers::codec;
use crate::mq::provider::StreamEvent;
use crate::mq::types::*;

/// Hard ceiling so a stray `limit` value cannot lock up the UI.
const MAX_BROWSE_LIMIT: usize = 10_000;
/// The broker caps how many messages one pull returns; stay well below it.
const PULL_PAGE: usize = 32;
/// How long the tail waits between polls.
const TAIL_INTERVAL: Duration = Duration::from_millis(400);
/// Messages flushed into one UI batch while tailing.
const STREAM_BATCH_SIZE: usize = 50;
/// The throw-away consumer group used for browsing and tailing.
pub(super) const BROWSE_GROUP: &str = "CID_mq-manager-browse";

// ---------------------------------------------------------------------------
// Offsets
// ---------------------------------------------------------------------------

async fn offset(
    connection: &RocketMqConnection,
    addr: &str,
    request_code: i32,
    topic: &str,
    queue_id: i32,
) -> AppResult<i64> {
    let response = connection
        .client()
        .request(
            addr,
            request_code,
            &[
                ("topic", topic.to_string()),
                ("queueId", queue_id.to_string()),
            ],
        )
        .await?;
    if response.code != response_code::SUCCESS {
        return Err(AppError::broker(format!(
            "could not read the offset of {topic}[{queue_id}]: {}{}",
            protocol::response_text(response.code),
            remark_of(&response)
        )));
    }
    response.header_i64("offset").ok_or_else(|| {
        AppError::broker(format!(
            "the broker did not return an offset for {topic}[{queue_id}]"
        ))
    })
}

/// Highest offset (== number of messages ever written) of one queue.
pub(super) async fn max_offset(
    connection: &RocketMqConnection,
    addr: &str,
    topic: &str,
    queue_id: i32,
) -> AppResult<i64> {
    offset(connection, addr, code::GET_MAX_OFFSET, topic, queue_id).await
}

/// Lowest offset still on disk (retention moves this forward).
pub(super) async fn min_offset(
    connection: &RocketMqConnection,
    addr: &str,
    topic: &str,
    queue_id: i32,
) -> AppResult<i64> {
    offset(connection, addr, code::GET_MIN_OFFSET, topic, queue_id).await
}

/// First offset whose store timestamp is at or after `timestamp`.
pub(super) async fn offset_by_timestamp(
    connection: &RocketMqConnection,
    addr: &str,
    topic: &str,
    queue_id: i32,
    timestamp: i64,
) -> AppResult<i64> {
    let response = connection
        .client()
        .request(
            addr,
            code::SEARCH_OFFSET_BY_TIMESTAMP,
            &[
                ("topic", topic.to_string()),
                ("queueId", queue_id.to_string()),
                ("timestamp", timestamp.to_string()),
            ],
        )
        .await?;
    if response.code != response_code::SUCCESS {
        return Err(AppError::broker(format!(
            "could not seek {topic}[{queue_id}] by timestamp: {}{}",
            protocol::response_text(response.code),
            remark_of(&response)
        )));
    }
    Ok(response.header_i64("offset").unwrap_or(0))
}

// ---------------------------------------------------------------------------
// Publishing
// ---------------------------------------------------------------------------

pub(super) async fn produce_impl(
    connection: &RocketMqConnection,
    request: ProduceRequest,
) -> AppResult<ProduceResult> {
    let started = Instant::now();
    let payload = codec::decode_bytes(request.payload.as_deref(), request.encoding)?;
    let route = route(connection, &request.topic).await?;

    // A `perm` without the write bit means the topic is read only.
    if route.perm() != 0 && route.perm() & 0x2 == 0 {
        return Err(AppError::invalid(format!(
            "the topic `{}` is read only",
            request.topic
        )));
    }

    let queues = route.queues();
    if queues.is_empty() {
        return Err(AppError::broker(format!(
            "no queue of `{}` is writable right now",
            request.topic
        )));
    }
    // `queueId = -1` lets the broker pick, but resolving it here keeps the
    // `defaultTopicQueueNums` of an auto-created topic correct.
    let (broker, addr, queue_id) = match request.partition {
        Some(wanted) => {
            let found = queues
                .iter()
                .find(|(_, _, queue)| *queue == wanted)
                .or_else(|| queues.first());
            found
                .map(|(broker, addr, queue)| (broker.clone(), addr.clone(), *queue))
                .ok_or_else(|| AppError::invalid("no writable queue found"))?
        }
        None => {
            let index = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos() as usize)
                .unwrap_or(0))
                % queues.len();
            let (broker, addr, queue) = &queues[index];
            (broker.clone(), addr.clone(), *queue)
        }
    };
    let _ = broker;

    let mut properties = BTreeMap::new();
    if let Some(key) = request.key.as_deref().filter(|key| !key.is_empty()) {
        properties.insert("KEYS".to_string(), key.to_string());
    }
    for header in &request.headers {
        let Some(value) = header.value.as_deref() else {
            continue;
        };
        // `TAGS` is special: the broker indexes it for tag filtering.
        properties.insert(header.key.clone(), value.to_string());
    }
    if let Some(tag) = request.options.get("tags").and_then(|value| value.as_str()) {
        properties.insert("TAGS".to_string(), tag.to_string());
    }
    if let Some(level) = request
        .options
        .get("delayTimeLevel")
        .and_then(|value| value.as_i64())
    {
        properties.insert("DELAY".to_string(), level.to_string());
    }
    properties.insert(
        "WAIT".to_string(),
        request
            .options
            .get("waitStore")
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
            .to_string(),
    );
    let properties = protocol::encode_properties(&properties);

    let born_timestamp = request.timestamp.unwrap_or_else(now_ms);
    let write_queue_nums = route
        .queue_datas
        .iter()
        .map(|data| data.write_queue_nums)
        .max()
        .unwrap_or(4)
        .max(1);

    let header = |code_point: i32| {
        let mut command = RemotingCommand::request(code_point, 0, Default::default());
        for (key, value) in [
            ("producerGroup", "mq-manager".to_string()),
            ("topic", request.topic.clone()),
            ("defaultTopic", "TBW102".to_string()),
            ("defaultTopicQueueNums", write_queue_nums.to_string()),
            ("queueId", queue_id.to_string()),
            ("sysFlag", "0".to_string()),
            ("bornTimestamp", born_timestamp.to_string()),
            ("flag", "0".to_string()),
            ("properties", properties.clone()),
            ("reconsumeTimes", "0".to_string()),
            ("unitMode", "false".to_string()),
            ("batch", "false".to_string()),
            ("maxReconsumeTimes", "0".to_string()),
        ] {
            command.ext_fields.insert(key.to_string(), value);
        }
        command
    };

    let mut response = connection
        .client()
        .invoke(&addr, header(code::SEND_MESSAGE).with_body(payload.clone()))
        .await?;
    // Some 5.x brokers only accept the V2 code; the header shape is the same.
    if response.code == response_code::REQUEST_CODE_NOT_SUPPORTED {
        response = connection
            .client()
            .invoke(&addr, header(10).with_body(payload))
            .await?;
    }

    if response.code != response_code::SUCCESS {
        return Err(AppError::broker(format!(
            "could not publish to `{}`: {}{}",
            request.topic,
            protocol::response_text(response.code),
            remark_of(&response)
        )));
    }

    Ok(ProduceResult {
        topic: request.topic,
        partition: response.header_i32("queueId").unwrap_or(queue_id),
        offset: response.header_i64("queueOffset").unwrap_or(-1),
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

// ---------------------------------------------------------------------------
// Pulling
// ---------------------------------------------------------------------------

struct PullOutcome {
    messages: Vec<StoredMessage>,
    next_offset: i64,
}

/// One pull request against one queue.
async fn pull_once(
    connection: &RocketMqConnection,
    addr: &str,
    group: &str,
    topic: &str,
    queue_id: i32,
    offset: i64,
    max_messages: usize,
) -> AppResult<PullOutcome> {
    let command = RemotingCommand::request(code::PULL_MESSAGE, 0, Default::default())
        .with_ext("consumerGroup", group)
        .with_ext("topic", topic)
        .with_ext("queueId", queue_id.to_string())
        .with_ext("queueOffset", offset.to_string())
        .with_ext("maxMsgNums", max_messages.min(PULL_PAGE).to_string())
        // PullSysFlag: FLAG_SUBSCRIPTION only — no commit, no long polling.
        .with_ext("sysFlag", "4")
        .with_ext("commitOffset", "0")
        .with_ext("suspendTimeoutMillis", "0")
        .with_ext("subscription", "*")
        .with_ext("subVersion", now_ms().to_string())
        .with_ext("expressionType", "TAG");

    let response = connection.client().invoke(addr, command).await?;
    let next_offset = response.header_i64("nextBeginOffset").unwrap_or(offset);

    match response.code {
        response_code::SUCCESS => Ok(PullOutcome {
            messages: protocol::parse_pull_body(&response.body)?,
            next_offset,
        }),
        // No message at this offset (yet), or the offset was moved/compacted.
        response_code::PULL_NOT_FOUND
        | response_code::NO_MESSAGE
        | response_code::PULL_RETRY_IMMEDIATELY
        | response_code::PULL_OFFSET_MOVED
        | response_code::QUERY_NOT_FOUND => Ok(PullOutcome {
            messages: Vec::new(),
            next_offset,
        }),
        code => Err(AppError::broker(format!(
            "could not pull {topic}[{queue_id}]: {}{}",
            protocol::response_text(code),
            remark_of(&response)
        ))),
    }
}

/// Make sure the throw-away browse group exists on every master broker.
///
/// RocketMQ refuses to serve a pull for an unknown subscription group, and
/// there is no way to pull anonymously, so the group is created once per
/// connection — exactly what `mqadmin` does for its own temporary consumers.
async fn ensure_browse_group(connection: &RocketMqConnection) -> AppResult<()> {
    if connection.browse_group_ready() {
        return Ok(());
    }
    let body = serde_json::to_vec(&serde_json::json!({
        "groupName": BROWSE_GROUP,
        "consumeEnable": true,
        "consumeFromMinEnable": false,
        "consumeBroadcastEnable": true,
        "retryQueueNums": 1,
        "retryMaxTimes": 16,
        "brokerId": 0,
        "whichBrokerWhenConsumeSlowly": 1,
        "notifyConsumerIdsChangedEnable": true,
    }))
    .map_err(|error| AppError::broker(format!("could not encode the group config: {error}")))?;

    for (name, addr) in masters(connection).await? {
        let command = RemotingCommand::request(
            code::UPDATE_AND_CREATE_SUBSCRIPTIONGROUP,
            0,
            Default::default(),
        )
        .with_body(body.clone());
        match connection.client().invoke(&addr, command).await {
            Ok(response) if response.code == response_code::SUCCESS => {}
            Ok(response) => tracing::debug!(
                target: "mq_manager::rocketmq",
                broker = %name,
                code = response.code,
                "the broker refused the browse group"
            ),
            Err(error) => tracing::debug!(
                target: "mq_manager::rocketmq",
                broker = %name,
                %error,
                "could not reach the broker for the browse group"
            ),
        }
    }
    connection.mark_browse_group_ready();
    Ok(())
}

/// Pull from a queue. The browse group is created up-front by the caller, so
/// this is a plain pull — see [`ensure_browse_group`].
async fn pull(
    connection: &RocketMqConnection,
    addr: &str,
    topic: &str,
    queue_id: i32,
    offset: i64,
    max_messages: usize,
) -> AppResult<PullOutcome> {
    pull_once(
        connection,
        addr,
        BROWSE_GROUP,
        topic,
        queue_id,
        offset,
        max_messages,
    )
    .await
}

// ---------------------------------------------------------------------------
// Browsing
// ---------------------------------------------------------------------------

pub(super) async fn browse_impl(
    connection: &RocketMqConnection,
    request: BrowseRequest,
) -> AppResult<BrowseResult> {
    let started = Instant::now();
    let limit = request.limit.clamp(1, MAX_BROWSE_LIMIT);
    let keyword = request
        .keyword
        .as_ref()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());

    let route = route(connection, &request.topic).await?;
    let mut queues = route.queues();
    if let Some(filter) = &request.partitions {
        queues.retain(|(_, _, queue)| filter.contains(queue));
    }
    if queues.is_empty() {
        return Err(AppError::invalid(format!(
            "the topic `{}` has no queue to read from",
            request.topic
        )));
    }

    ensure_browse_group(connection).await?;

    // Start offset + watermarks per queue.
    let mut watermarks = Vec::with_capacity(queues.len());
    let mut cursors: HashMap<i32, i64> = HashMap::with_capacity(queues.len());
    for (_, addr, queue_id) in &queues {
        let begin = min_offset(connection, addr, &request.topic, *queue_id)
            .await
            .ok();
        let end = max_offset(connection, addr, &request.topic, *queue_id)
            .await
            .ok();
        // Offsets are monotonic, so a swapped pair means the broker answered
        // with something we did not expect — trust the ordered one.
        let (begin, end) = match (begin, end) {
            (Some(begin), Some(end)) if begin > end => (Some(end), Some(begin)),
            pair => pair,
        };
        watermarks.push(PartitionInfo {
            id: *queue_id,
            leader: Some(0),
            begin_offset: begin,
            end_offset: end,
            message_count: match (begin, end) {
                (Some(begin), Some(end)) => Some((end - begin).max(0)),
                _ => None,
            },
            ..Default::default()
        });

        let start = match &request.start {
            SeekPosition::Beginning => begin.unwrap_or(0),
            SeekPosition::End => end.unwrap_or(0),
            SeekPosition::Offset { offset } => *offset,
            SeekPosition::Timestamp { timestamp } => {
                offset_by_timestamp(connection, addr, &request.topic, *queue_id, *timestamp)
                    .await
                    .unwrap_or_else(|_| begin.unwrap_or(0))
            }
        };
        cursors.insert(*queue_id, start);
    }

    // Spread the limit over the queues: browsing queue 0 only would hide the
    // rest of the topic.
    let per_queue = (limit / queues.len()).max(1);
    let mut messages = Vec::new();
    let mut scanned = 0usize;

    for (_, addr, queue_id) in &queues {
        let mut remaining = per_queue;
        let cursor = cursors.entry(*queue_id).or_insert(0);
        while remaining > 0 && messages.len() < limit {
            let outcome = match pull(
                connection,
                addr,
                &request.topic,
                *queue_id,
                *cursor,
                remaining,
            )
            .await
            {
                Ok(outcome) => outcome,
                // A queue that cannot be read (retention, replica down) must not
                // hide the other queues.
                Err(error) => {
                    tracing::debug!(
                        target: "mq_manager::rocketmq",
                        queue = *queue_id,
                        %error,
                        "skipping a queue while browsing"
                    );
                    break;
                }
            };

            if outcome.messages.is_empty() {
                if outcome.next_offset > *cursor {
                    // The offset moved (retention): jump forward instead of
                    // hammering the same position.
                    *cursor = outcome.next_offset;
                    continue;
                }
                break;
            }

            let received = outcome.messages.len();
            *cursor = outcome.next_offset;
            for stored in outcome.messages {
                scanned += 1;
                if let Some(keyword) = &keyword {
                    if !matches_keyword(&stored, keyword) {
                        continue;
                    }
                }
                messages.push(stored.into_message());
                remaining = remaining.saturating_sub(1);
                if messages.len() >= limit {
                    break;
                }
            }
            if received == 0 {
                break;
            }
        }
    }

    messages.sort_by(|left, right| {
        right
            .timestamp
            .unwrap_or_default()
            .cmp(&left.timestamp.unwrap_or_default())
            .then_with(|| {
                right
                    .offset
                    .unwrap_or_default()
                    .cmp(&left.offset.unwrap_or_default())
            })
    });

    let truncated = messages.len() >= limit;
    Ok(BrowseResult {
        messages,
        truncated,
        scanned,
        elapsed_ms: started.elapsed().as_millis() as u64,
        watermarks,
    })
}

fn matches_keyword(message: &StoredMessage, keyword: &str) -> bool {
    if message
        .property("KEYS")
        .map(|keys| keys.to_lowercase().contains(keyword))
        .unwrap_or(false)
    {
        return true;
    }
    if message
        .property("TAGS")
        .map(|tags| tags.to_lowercase().contains(keyword))
        .unwrap_or(false)
    {
        return true;
    }
    String::from_utf8_lossy(&message.body)
        .to_lowercase()
        .contains(keyword)
}

// ---------------------------------------------------------------------------
// Tailing
// ---------------------------------------------------------------------------

pub(super) async fn open_stream_impl(
    connection: &RocketMqConnection,
    request: StreamRequest,
) -> AppResult<mpsc::Receiver<StreamEvent>> {
    let route = route(connection, &request.topic).await?;
    let mut queues = route.queues();
    if let Some(filter) = &request.partitions {
        queues.retain(|(_, _, queue)| filter.contains(queue));
    }
    if queues.is_empty() {
        return Err(AppError::invalid(format!(
            "the topic `{}` has no queue to follow",
            request.topic
        )));
    }
    ensure_browse_group(connection).await?;

    // Resolve the starting offsets before spawning, so an unreachable broker
    // surfaces as an error on the command instead of a silent empty stream.
    let mut cursors: Vec<(String, String, i32, i64)> = Vec::with_capacity(queues.len());
    for (_, addr, queue_id) in queues {
        let start = match &request.start {
            SeekPosition::Beginning => min_offset(connection, &addr, &request.topic, queue_id)
                .await
                .unwrap_or(0),
            SeekPosition::End => max_offset(connection, &addr, &request.topic, queue_id)
                .await
                .unwrap_or(0),
            SeekPosition::Offset { offset } => *offset,
            SeekPosition::Timestamp { timestamp } => {
                offset_by_timestamp(connection, &addr, &request.topic, queue_id, *timestamp)
                    .await
                    .unwrap_or(0)
            }
        };
        cursors.push((String::new(), addr, queue_id, start));
    }

    let idle_timeout = Duration::from_millis(request.idle_timeout_ms.unwrap_or(10_000).max(500));
    let max_messages = request.max_messages;
    let topic = request.topic.clone();
    let connection = connection.clone();
    let (sender, receiver) = mpsc::channel::<StreamEvent>(64);

    tokio::spawn(async move {
        let mut sent: u64 = 0;
        let mut last_activity = Instant::now();
        let mut batch: Vec<Message> = Vec::new();

        loop {
            let mut received = 0usize;
            for (_, addr, queue_id, cursor) in cursors.iter_mut() {
                if max_messages > 0 && sent >= max_messages {
                    break;
                }
                match pull_once(
                    &connection,
                    addr,
                    BROWSE_GROUP,
                    &topic,
                    *queue_id,
                    *cursor,
                    STREAM_BATCH_SIZE,
                )
                .await
                {
                    Ok(outcome) => {
                        if outcome.next_offset > *cursor {
                            *cursor = outcome.next_offset;
                        }
                        received += outcome.messages.len();
                        batch.extend(
                            outcome
                                .messages
                                .into_iter()
                                .map(StoredMessage::into_message),
                        );
                    }
                    Err(error) => {
                        // A broker restart or a leadership change is normal for
                        // a long lived tail: report it and keep going.
                        if sender
                            .send(StreamEvent::Failed(error.to_string()))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }

            if !batch.is_empty() {
                sent += batch.len() as u64;
                last_activity = Instant::now();
                let drained = std::mem::take(&mut batch);
                if sender.send(StreamEvent::Batch(drained)).await.is_err() {
                    return;
                }
                if max_messages > 0 && sent >= max_messages {
                    let _ = sender.send(StreamEvent::End).await;
                    return;
                }
            } else if received == 0 && last_activity.elapsed() >= idle_timeout {
                if sender.send(StreamEvent::Idle).await.is_err() {
                    return;
                }
                last_activity = Instant::now();
            }

            tokio::time::sleep(TAIL_INTERVAL).await;
        }
    });

    Ok(receiver)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

fn remark_of(response: &RemotingCommand) -> String {
    response
        .remark
        .as_deref()
        .filter(|remark| !remark.is_empty())
        .map(|remark| format!(" — {remark}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_keywords_in_keys_tags_and_body() {
        let mut message = StoredMessage {
            body: b"hello world".to_vec(),
            ..Default::default()
        };
        message.properties.insert("KEYS".into(), "order-1".into());
        assert!(matches_keyword(&message, "order"));
        assert!(matches_keyword(&message, "world"));
        assert!(!matches_keyword(&message, "missing"));
    }

    #[test]
    fn ignores_unknown_route_brokers() {
        let route = super::super::topics::TopicRouteData::default();
        assert!(route.queues().is_empty());
    }
}
