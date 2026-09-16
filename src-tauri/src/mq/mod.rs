//! Broker abstraction layer.
//!
//! ```text
//!  commands/            Tauri IPC surface, provider agnostic
//!      |
//!  mq::MqConnection     object-safe trait every driver implements
//!      |
//!  mq::drivers::kafka   first driver
//!  mq::drivers::demo    in-memory driver used for development & tests
//! ```
//!
//! To add RocketMQ / RabbitMQ / MQTT create `mq/drivers/<name>/mod.rs`,
//! implement [`MqProvider`](provider::MqProvider) and
//! [`MqConnection`](provider::MqConnection), then register it in
//! [`registry_from_builtin`](drivers::registry_from_builtin).

pub mod drivers;
pub mod provider;
pub mod registry;
pub mod types;

pub use provider::MqConnection;
pub use registry::ProviderRegistry;
