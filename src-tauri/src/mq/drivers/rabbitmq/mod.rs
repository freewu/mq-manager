//! RabbitMQ driver.
//!
//! Two protocols are used, each for what it is good at:
//!
//! * **AMQP 0-9-1** (`lapin`) for the connection itself and for publishing —
//!   publisher confirms included, so a “sent” message really was accepted.
//! * **Management HTTP API** (`reqwest`) for discovery and peeking. AMQP has no
//!   “list queues” primitive, and browsing through `basic.get` would either
//!   dead-loop on the same message or consume it. The management API can peek
//!   with `ack_requeue_true`, which is non destructive.
//!
//! Listing therefore needs the `rabbitmq_management` plugin. Publishing works
//! without it — set `managementApi` to false and the UI hides the rest.

mod amqp;
mod api;
mod groups;
mod messages;
mod options;
mod topics;

use async_trait::async_trait;
use parking_lot::RwLock;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;

use crate::error::{AppError, AppResult};
use crate::mq::provider::{MqConnection, MqProvider};
use crate::mq::types::*;

use self::amqp::AmqpClient;
use self::api::{ManagementApi, Overview};
use self::options::RabbitMqOptions;

pub(crate) const PROVIDER_ID: &str = "rabbitmq";

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct RabbitMqProvider;

#[async_trait]
impl MqProvider for RabbitMqProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        descriptor()
    }

    async fn connect(&self, profile: &ConnectionProfile) -> AppResult<Arc<dyn MqConnection>> {
        let connection = RabbitMqConnection::open(profile).await?;
        Ok(Arc::new(connection))
    }
}

pub fn descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: PROVIDER_ID.into(),
        name: "RabbitMQ".into(),
        description: "AMQP 0-9-1 broker. Queues, exchanges, bindings and non destructive peeking."
            .into(),
        vendor: "Broadcom (VMware)".into(),
        accent: "#ff7043".into(),
        docs_url: Some("https://www.rabbitmq.com/docs".into()),
        default_port: Some(options::DEFAULT_AMQP_PORT),
        driver_version: env!("CARGO_PKG_VERSION").into(),
        capabilities: Capabilities {
            topics: TopicCapabilities {
                list: true,
                create: true,
                delete: true,
                update: true,
                config: true,
                // AMQP has no partitions: a queue is a queue.
                partitions: false,
                purge: true,
            },
            messages: MessageCapabilities {
                produce: true,
                browse: true,
                tail: true,
                keys: true,
                headers: true,
                partitions: false,
                timestamp_seek: false,
                offset_seek: false,
                persistent: true,
            },
            groups: GroupCapabilities {
                list: true,
                describe: true,
                members: true,
                offsets: false,
                lag: false,
                reset_offsets: false,
                delete: false,
            },
            nodes: true,
            metrics: true,
            acl: false,
            schemas: false,
        },
        fields: connection_fields(),
    }
}

fn connection_fields() -> Vec<ConnectionField> {
    vec![
        ConnectionField::text("host", "Host")
            .required()
            .placeholder("localhost")
            .group("Connection"),
        ConnectionField::number("port", "AMQP port")
            .default_value(json!(options::DEFAULT_AMQP_PORT))
            .help("5672 for plain AMQP, 5671 for AMQPS.")
            .group("Connection"),
        ConnectionField::text("vhost", "Virtual host")
            .default_value(json!("/"))
            .help("The default virtual host is `/`.")
            .group("Connection"),
        ConnectionField::text("username", "Username")
            .default_value(json!("guest"))
            .group("Connection"),
        ConnectionField::password("password", "Password")
            .default_value(json!("guest"))
            .group("Connection"),
        ConnectionField::text("connectionName", "Connection name")
            .default_value(json!("mq-manager"))
            .help("Shown in the RabbitMQ management UI under Connections.")
            .group("Connection")
            .advanced(),
        ConnectionField::boolean("useTls", "Use TLS (AMQPS)")
            .default_value(json!(false))
            .help("Encrypts the AMQP connection and switches the management API to HTTPS.")
            .group("Security"),
        ConnectionField::boolean("managementApi", "Use the management API")
            .default_value(json!(true))
            .help(
                "Required to list queues, exchanges and consumers and to browse messages. \
                 Needs the `rabbitmq_management` plugin.",
            )
            .group("Management API"),
        ConnectionField::text("managementUrl", "Management URL")
            .placeholder("http://localhost:15672")
            .help("Leave empty to derive it from the host and port (15672 / 15671 with TLS).")
            .group("Management API")
            .advanced(),
        ConnectionField::text("managementUsername", "Management username")
            .help("Defaults to the AMQP username. Needs the `management` tag.")
            .group("Management API")
            .advanced(),
        ConnectionField::password("managementPassword", "Management password")
            .help("Defaults to the AMQP password.")
            .group("Management API")
            .advanced(),
        ConnectionField::number("heartbeat", "Heartbeat (seconds)")
            .default_value(json!(60))
            .help("0 disables heartbeats. Must be lower than the server's negotiated value.")
            .group("Advanced")
            .advanced(),
        ConnectionField::number("timeoutMs", "Timeout (ms)")
            .default_value(json!(15_000))
            .group("Advanced")
            .advanced(),
    ]
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

pub struct RabbitMqConnection {
    profile_id: String,
    options: RabbitMqOptions,
    amqp: AmqpClient,
    management: Option<ManagementApi>,
    overview: Option<Overview>,
    /// Exchange names seen in the last topology refresh. Publishing to one of
    /// them must go through the exchange instead of the default one.
    exchanges: RwLock<HashSet<String>>,
}

impl RabbitMqConnection {
    pub async fn open(profile: &ConnectionProfile) -> AppResult<Self> {
        let options = RabbitMqOptions::from_profile(profile)?;
        let amqp = AmqpClient::open(&options).await?;

        let management = match ManagementApi::new(&options)? {
            Some(api) => Some(api),
            None => None,
        };

        // Fail fast when the management API is configured but unusable, so the
        // user gets one actionable message instead of empty pages.
        let overview = match management.as_ref() {
            Some(api) => Some(api.overview().await?),
            None => None,
        };

        tracing::info!(
            target: "mq_manager::rabbitmq",
            host = %options.authority(),
            vhost = %options.vhost,
            management = management.is_some(),
            "connected"
        );

        Ok(Self {
            profile_id: profile.id.clone(),
            options,
            amqp,
            management,
            overview,
            exchanges: RwLock::new(HashSet::new()),
        })
    }

    fn vhost(&self) -> &str {
        &self.options.vhost
    }

    pub(crate) fn amqp(&self) -> &AmqpClient {
        &self.amqp
    }

    /// The management API, or a clear error when it is switched off.
    pub(crate) fn api(&self) -> AppResult<&ManagementApi> {
        self.management.as_ref().ok_or_else(|| {
            AppError::unsupported(
                "this operation needs the RabbitMQ management API — enable `managementApi` \
                 in the connection settings and install the `rabbitmq_management` plugin",
            )
        })
    }

    pub(crate) fn is_exchange(&self, name: &str) -> bool {
        self.exchanges.read().contains(name)
    }

    /// Refresh the exchange cache from the broker.
    pub(crate) fn remember_exchanges(&self, names: impl IntoIterator<Item = String>) {
        let mut guard = self.exchanges.write();
        guard.clear();
        guard.extend(names);
    }
}

#[async_trait]
impl MqConnection for RabbitMqConnection {
    fn provider_id(&self) -> &str {
        PROVIDER_ID
    }

    fn profile_id(&self) -> &str {
        &self.profile_id
    }

    fn capabilities(&self) -> Capabilities {
        // Everything except publishing needs the management API.
        let management = self.management.is_some();
        Capabilities {
            topics: TopicCapabilities {
                list: management,
                create: management,
                delete: management,
                update: management,
                config: management,
                partitions: false,
                purge: management,
            },
            messages: MessageCapabilities {
                produce: true,
                browse: management,
                tail: management,
                keys: true,
                headers: true,
                partitions: false,
                timestamp_seek: false,
                offset_seek: false,
                persistent: true,
            },
            groups: GroupCapabilities {
                list: management,
                describe: management,
                members: management,
                offsets: false,
                lag: false,
                reset_offsets: false,
                delete: false,
            },
            nodes: management,
            metrics: management,
            acl: false,
            schemas: false,
        }
    }

    async fn ping(&self) -> AppResult<()> {
        self.amqp.ping().await
    }

    async fn cluster_info(&self) -> AppResult<ClusterInfo> {
        self.cluster_impl().await
    }

    async fn list_nodes(&self) -> AppResult<Vec<NodeInfo>> {
        self.nodes_impl().await
    }

    async fn list_topics(&self) -> AppResult<Vec<TopicSummary>> {
        self.topics_impl().await
    }

    async fn topic_detail(&self, topic: &str) -> AppResult<TopicDetail> {
        self.topic_detail_impl(topic).await
    }

    async fn create_topic(&self, request: CreateTopicRequest) -> AppResult<()> {
        self.create_topic_impl(request).await
    }

    async fn update_topic(&self, request: UpdateTopicRequest) -> AppResult<()> {
        self.update_topic_impl(request).await
    }

    async fn delete_topic(&self, topic: &str) -> AppResult<()> {
        self.delete_topic_impl(topic).await
    }

    async fn purge_topic(&self, topic: &str) -> AppResult<()> {
        self.purge_topic_impl(topic).await
    }

    async fn list_groups(&self) -> AppResult<Vec<ConsumerGroupSummary>> {
        self.groups_impl().await
    }

    async fn group_detail(&self, group: &str) -> AppResult<ConsumerGroupDetail> {
        self.group_detail_impl(group).await
    }

    async fn produce(&self, request: ProduceRequest) -> AppResult<ProduceResult> {
        self.produce_impl(request).await
    }

    async fn browse(&self, request: BrowseRequest) -> AppResult<BrowseResult> {
        self.browse_impl(request).await
    }

    async fn open_stream(
        &self,
        request: StreamRequest,
    ) -> AppResult<tokio::sync::mpsc::Receiver<crate::mq::provider::StreamEvent>> {
        self.open_stream_impl(request).await
    }

    async fn close(&self) -> AppResult<()> {
        self.amqp.close().await
    }
}
