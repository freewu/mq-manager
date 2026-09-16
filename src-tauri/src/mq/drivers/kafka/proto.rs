//! Decoders for the Kafka consumer-group protocol blobs.
//!
//! `ListGroups`/`DescribeGroups` return the member `assignment` and `metadata`
//! fields as opaque bytes encoded with the classic Kafka binary format:
//!
//! ```text
//! ConsumerProtocolAssignment {
//!   version: int16
//!   assigned_partitions: [ { topic: string, partitions: [int32] } ]
//!   user_data: bytes
//! }
//!
//! ConsumerProtocolSubscription {
//!   version: int16
//!   topics: [string]
//!   user_data: bytes
//!   owned_partitions: [...]   // v1+
//! }
//! ```
//!
//! Kafka also wraps the payload in a `bytes` header when the group uses the
//! `range`/`roundrobin` assignor (a nullable byte-array prefix). We therefore
//! try the raw buffer first and fall back to skipping the nullable prefix.

use crate::mq::types::MemberAssignment;

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn i16(&mut self) -> Option<i16> {
        let end = self.pos.checked_add(2)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(i16::from_be_bytes([slice[0], slice[1]]))
    }

    fn i32(&mut self) -> Option<i32> {
        let end = self.pos.checked_add(4)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(i32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    fn string(&mut self) -> Option<String> {
        let len = self.i16()?;
        if len < 0 {
            return Some(String::new());
        }
        let len = len as usize;
        let end = self.pos.checked_add(len)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(String::from_utf8_lossy(slice).to_string())
    }
}

/// Try to decode an assignment, tolerating the nullable-bytes wrapper.
fn decode_assignment(data: &[u8]) -> Option<Vec<MemberAssignment>> {
    let mut reader = Reader::new(data);
    let version = reader.i16()?;
    if !(0..=3).contains(&version) {
        return None;
    }
    let count = reader.i32()?;
    if count < 0 || count > 100_000 {
        return None;
    }
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let topic = reader.string()?;
        let partition_count = reader.i32()?;
        if partition_count < 0 || partition_count > 1_000_000 {
            return None;
        }
        let mut partitions = Vec::with_capacity(partition_count as usize);
        for _ in 0..partition_count {
            partitions.push(reader.i32()?);
        }
        if !topic.is_empty() {
            out.push(MemberAssignment { topic, partitions });
        }
    }
    Some(out)
}

/// Decode the `assignment` field of a group member.
pub fn parse_assignment(data: &[u8]) -> Vec<MemberAssignment> {
    if let Some(parsed) = decode_assignment(data) {
        return parsed;
    }
    // Skip a nullable byte-array length prefix and retry.
    if data.len() > 4 {
        let start = 4usize;
        if let Some(parsed) = decode_assignment(&data[start..]) {
            return parsed;
        }
    }
    Vec::new()
}

fn decode_subscription(data: &[u8]) -> Option<Vec<String>> {
    let mut reader = Reader::new(data);
    let version = reader.i16()?;
    if !(0..=3).contains(&version) {
        return None;
    }
    let count = reader.i32()?;
    if count < 0 || count > 100_000 {
        return None;
    }
    let mut topics = Vec::with_capacity(count as usize);
    for _ in 0..count {
        topics.push(reader.string()?);
    }
    Some(topics.into_iter().filter(|t| !t.is_empty()).collect())
}

/// Decode the `metadata` (subscription) field of a group member.
pub fn parse_subscription(data: &[u8]) -> Vec<String> {
    if let Some(parsed) = decode_subscription(data) {
        return parsed;
    }
    if data.len() > 4 {
        if let Some(parsed) = decode_subscription(&data[4..]) {
            return parsed;
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_string(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(&(value.len() as i16).to_be_bytes());
        out.extend_from_slice(value.as_bytes());
    }

    fn encode_assignment(entries: &[(&str, &[i32])]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0i16.to_be_bytes());
        out.extend_from_slice(&(entries.len() as i32).to_be_bytes());
        for (topic, partitions) in entries {
            encode_string(&mut out, topic);
            out.extend_from_slice(&(partitions.len() as i32).to_be_bytes());
            for partition in *partitions {
                out.extend_from_slice(&partition.to_be_bytes());
            }
        }
        out.extend_from_slice(&0i32.to_be_bytes());
        out
    }

    #[test]
    fn parses_a_plain_assignment() {
        let raw = encode_assignment(&[("orders", &[0, 1, 2]), ("events", &[7])]);
        let parsed = parse_assignment(&raw);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].topic, "orders");
        assert_eq!(parsed[0].partitions, vec![0, 1, 2]);
        assert_eq!(parsed[1].partitions, vec![7]);
    }

    #[test]
    fn parses_a_wrapped_assignment() {
        let inner = encode_assignment(&[("orders", &[4])]);
        let mut wrapped = Vec::new();
        wrapped.extend_from_slice(&(inner.len() as i32).to_be_bytes());
        wrapped.extend_from_slice(&inner);
        let parsed = parse_assignment(&wrapped);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].topic, "orders");
        assert_eq!(parsed[0].partitions, vec![4]);
    }

    #[test]
    fn garbage_does_not_panic() {
        assert!(parse_assignment(&[0xff; 3]).is_empty());
        assert!(parse_subscription(&[]).is_empty());
    }
}
