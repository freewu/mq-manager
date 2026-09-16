//! AMQP consumers, mapped onto the neutral “consumer group” model.
//!
//! RabbitMQ has no consumer groups and no committed offsets: a consumer is
//! bound to one channel and one queue, and messages are pushed until
//! acknowledged. Each `basic.consume` therefore shows up here as one entry —
//! which is exactly what the management UI does as well.

use crate::error::{AppError, AppResult};
use crate::mq::types::*;

use super::api::ConsumerDto;
use super::RabbitMqConnection;

impl RabbitMqConnection {
    pub(crate) async fn groups_impl(&self) -> AppResult<Vec<ConsumerGroupSummary>> {
        let api = self.api()?;
        let mut consumers = api.consumers(self.vhost()).await?;
        // Stable order so the table does not shuffle between refreshes.
        consumers.sort_by(|a, b| a.consumer_tag.cmp(&b.consumer_tag));
        Ok(consumers.iter().map(consumer_summary).collect())
    }

    pub(crate) async fn group_detail_impl(&self, group: &str) -> AppResult<ConsumerGroupDetail> {
        let api = self.api()?;
        let consumers = api.consumers(self.vhost()).await?;
        let consumer = consumers
            .iter()
            .find(|consumer| consumer.consumer_tag == group)
            .ok_or_else(|| {
                AppError::invalid(format!(
                    "consumer `{group}` is not active on virtual host `{}` — \
                     consumers disappear as soon as a client disconnects",
                    self.vhost()
                ))
            })?;

        let queue = consumer
            .queue
            .as_ref()
            .map(|queue| queue.name.clone())
            .unwrap_or_default();
        let channel = consumer.channel_details.as_ref();
        let client_id = channel
            .and_then(|details| details.connection_name.clone())
            .or_else(|| channel.and_then(|details| details.name.clone()));

        let member = GroupMember {
            id: consumer.consumer_tag.clone(),
            client_id,
            client_host: channel.map(|details| match details.peer_port {
                Some(port) if port > 0 => {
                    format!("{}:{port}", details.peer_host.clone().unwrap_or_default())
                }
                _ => details.peer_host.clone().unwrap_or_default(),
            }),
            assignments: vec![MemberAssignment {
                topic: queue.clone(),
                partitions: Vec::new(),
            }],
        };

        let mut members = vec![member];
        // One row per consumer tag is the rule, but RabbitMQ groups several
        // tags per channel; surface the siblings too so the picture is complete.
        if let Some(channel_name) = channel.and_then(|details| details.name.clone()) {
            for sibling in consumers.iter().filter(|candidate| {
                candidate.consumer_tag != group
                    && candidate
                        .channel_details
                        .as_ref()
                        .and_then(|details| details.name.clone())
                        .as_deref()
                        == Some(channel_name.as_str())
            }) {
                members.push(GroupMember {
                    id: sibling.consumer_tag.clone(),
                    client_id: sibling
                        .channel_details
                        .as_ref()
                        .and_then(|details| details.connection_name.clone()),
                    client_host: sibling
                        .channel_details
                        .as_ref()
                        .and_then(|details| details.peer_host.clone()),
                    assignments: vec![MemberAssignment {
                        topic: sibling
                            .queue
                            .as_ref()
                            .map(|queue| queue.name.clone())
                            .unwrap_or_default(),
                        partitions: Vec::new(),
                    }],
                });
            }
        }

        Ok(ConsumerGroupDetail {
            summary: consumer_summary(consumer),
            members,
            // No committed offsets in AMQP.
            offsets: Vec::new(),
        })
    }
}

fn consumer_summary(consumer: &ConsumerDto) -> ConsumerGroupSummary {
    ConsumerGroupSummary {
        id: consumer.consumer_tag.clone(),
        state: Some(
            consumer
                .activity_status
                .clone()
                .unwrap_or_else(|| if consumer.active { "active" } else { "idle" }.to_string()),
        ),
        protocol_type: Some("amqp".to_string()),
        member_count: 1,
        topics: consumer
            .queue
            .as_ref()
            .map(|queue| vec![queue.name.clone()])
            .unwrap_or_default(),
        total_lag: None,
        kind: Some("consumer".to_string()),
    }
}
