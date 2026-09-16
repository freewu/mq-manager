//! Provider registry bootstrap.
//!
//! Add a new broker by writing a driver module and appending it here — the
//! command layer and the UI pick it up automatically from its descriptor.

pub mod codec;
pub mod demo;
pub mod kafka;
pub mod rabbitmq;
pub mod rocketmq;

use std::sync::Arc;

use crate::mq::registry::ProviderRegistry;

/// Every driver compiled into this build, in wizard order.
pub fn registry_from_builtin() -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(kafka::KafkaProvider));
    registry.register(Arc::new(rocketmq::RocketMqProvider));
    registry.register(Arc::new(rabbitmq::RabbitMqProvider));
    registry.register(Arc::new(demo::DemoProvider));
    registry
}
