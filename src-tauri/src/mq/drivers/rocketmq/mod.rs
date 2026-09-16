//! RocketMQ driver.
//!
//! RocketMQ is spoken through its own remoting protocol (a small binary frame
//! with a JSON header) rather than a well known standard, so this driver
//! implements the client itself: see [`protocol`] for the frame and message
//! format and [`remoting`] for the TCP client.
//!
//! There are two consequences worth knowing about:
//!
//! * Browsing and tailing use a pull with a throw-away consumer group and never
//!   commit an offset, so nothing is consumed and no message is lost.
//! * RocketMQ can neither purge a topic nor serve a real non-destructive
//!   `basic.get`; the capabilities reflect that.

mod groups;
mod messages;
mod options;
mod protocol;
mod remoting;
mod topics;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use parking_lot::RwLock;
use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::mq::provider::{MqConnection, MqProvider};
use crate::mq::types::*;

use self::options::RocketMqOptions;
use self::remoting::RemotingClient;
use self::topics::{ClusterDto, TopicRouteData};

pub(crate) const PROVIDER_ID: &str = "rocketmq";

/// How long a cached route / broker table stays fresh.
const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct RocketMqProvider;

#[async_trait]
impl MqProvider for RocketMqProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        descriptor()
    }

    async fn connect(&self, profile: &ConnectionProfile) -> AppResult<Arc<dyn MqConnection>> {
        let connection = RocketMqConnection::open(profile).await?;
        Ok(Arc::new(connection))
    }
}

pub fn descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: PROVIDER_ID.into(),
        name: "Apache RocketMQ".into(),
        description:
            "RocketMQ 4.x / 5.x remoting clients with non destructive browsing and tailing.".into(),
        vendor: "Apache Software Foundation".into(),
        accent: "#3fb8af".into(),
        docs_url: Some("https://rocketmq.apache.org/docs/".into()),
        default_port: Some(options::DEFAULT_NAMESERVER_PORT),
        driver_version: env!("CARGO_PKG_VERSION").into(),
        capabilities: capabilities(),
        fields: connection_fields(),
    }
}

fn capabilities() -> Capabilities {
    Capabilities {
        topics: TopicCapabilities {
            list: true,
            create: true,
            delete: true,
            update: true,
            config: true,
            // RocketMQ "partitions" are the read/write queues of a topic.
            partitions: true,
            // No API empties a topic while keeping it.
            purge: false,
        },
        messages: MessageCapabilities {
            produce: true,
            browse: true,
            tail: true,
            keys: true,
            headers: true,
            partitions: true,
            timestamp_seek: true,
            offset_seek: true,
            persistent: true,
        },
        groups: GroupCapabilities {
            list: true,
            describe: true,
            members: true,
            offsets: true,
            lag: true,
            reset_offsets: true,
            delete: true,
        },
        nodes: true,
        metrics: false,
        acl: false,
        schemas: false,
    }
}

fn connection_fields() -> Vec<ConnectionField> {
    vec![
        ConnectionField::text("nameserver", "Nameserver addresses")
            .required()
            .placeholder("localhost:9876")
            .help("One or more `host:port` entries, separated by commas. Brokers are discovered through them.")
            .group("Connection"),
        ConnectionField::number("nameserverPort", "Nameserver port")
            .default_value(json!(options::DEFAULT_NAMESERVER_PORT))
            .help("Used for entries written without a port.")
            .group("Connection")
            .advanced(),
        ConnectionField::number("timeoutMs", "Timeout (ms)")
            .default_value(json!(options::DEFAULT_TIMEOUT_MS))
            .help("Per request timeout. RocketMQ admin calls are cheap; 5s is generous.")
            .group("Advanced")
            .advanced(),
    ]
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// Shared state, so the tail task can keep the connection alive after the
/// command that started it has returned.
struct Inner {
    profile_id: String,
    options: RocketMqOptions,
    client: RemotingClient,
    routes: RwLock<HashMap<String, (Instant, TopicRouteData)>>,
    cluster: RwLock<Option<(Instant, ClusterDto)>>,
    browse_group_ready: AtomicBool,
}

pub struct RocketMqConnection {
    inner: Arc<Inner>,
    /// Kept outside of `Inner` because `describe` needs it synchronously.
    nameservers: Vec<String>,
    timeout: std::time::Duration,
}

impl Clone for RocketMqConnection {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            nameservers: self.nameservers.clone(),
            timeout: self.timeout,
        }
    }
}

impl RocketMqConnection {
    pub async fn open(profile: &ConnectionProfile) -> AppResult<Self> {
        let options = RocketMqOptions::from_profile(profile)?;
        let client = RemotingClient::new(options.timeout);

        // Fail fast: a wrong nameserver address must not surface as an empty
        // topic list three clicks later.
        client.touch(options.primary()).await.map_err(|error| {
            AppError::broker(format!(
                "could not reach the RocketMQ nameserver at {}: {error}",
                options.primary()
            ))
        })?;

        tracing::info!(
            target: "mq_manager::rocketmq",
            nameservers = %options.authority(),
            "connected"
        );

        Ok(Self {
            nameservers: options.nameservers.clone(),
            timeout: options.timeout,
            inner: Arc::new(Inner {
                profile_id: profile.id.clone(),
                options,
                client,
                routes: RwLock::new(HashMap::new()),
                cluster: RwLock::new(None),
                browse_group_ready: AtomicBool::new(false),
            }),
        })
    }

    /// Shared remoting client; only the driver submodules use it.
    fn client(&self) -> &RemotingClient {
        &self.inner.client
    }

    /// Every configured nameserver; only the driver submodules use it.
    fn nameservers(&self) -> &[String] {
        &self.nameservers
    }

    /// The nameserver used for metadata requests.
    fn nameserver(&self) -> &str {
        self.inner.options.primary()
    }

    fn cached_cluster(&self) -> Option<ClusterDto> {
        let guard = self.inner.cluster.read();
        guard
            .as_ref()
            .filter(|(at, _)| at.elapsed() < CACHE_TTL)
            .map(|(_, cluster)| cluster.clone())
    }

    fn remember_cluster(&self, cluster: ClusterDto) {
        *self.inner.cluster.write() = Some((Instant::now(), cluster));
    }

    fn cached_route(&self, topic: &str) -> Option<TopicRouteData> {
        let guard = self.inner.routes.read();
        guard
            .get(topic)
            .filter(|(at, _)| at.elapsed() < CACHE_TTL)
            .map(|(_, route)| route.clone())
    }

    fn remember_route(&self, topic: &str, route: TopicRouteData) {
        self.inner
            .routes
            .write()
            .insert(topic.to_string(), (Instant::now(), route));
    }

    fn invalidate_route(&self, topic: &str) {
        self.inner.routes.write().remove(topic);
        *self.inner.cluster.write() = None;
    }

    fn browse_group_ready(&self) -> bool {
        self.inner.browse_group_ready.load(Ordering::Relaxed)
    }

    fn mark_browse_group_ready(&self) {
        self.inner.browse_group_ready.store(true, Ordering::Relaxed);
    }
}

#[async_trait]
impl MqConnection for RocketMqConnection {
    fn provider_id(&self) -> &str {
        PROVIDER_ID
    }

    fn profile_id(&self) -> &str {
        &self.inner.profile_id
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    async fn ping(&self) -> AppResult<()> {
        self.client().touch(self.nameserver()).await
    }

    async fn cluster_info(&self) -> AppResult<ClusterInfo> {
        topics::cluster_impl(self).await
    }

    async fn list_nodes(&self) -> AppResult<Vec<NodeInfo>> {
        topics::nodes_impl(self).await
    }

    async fn list_topics(&self) -> AppResult<Vec<TopicSummary>> {
        topics::topics_impl(self).await
    }

    async fn topic_detail(&self, topic: &str) -> AppResult<TopicDetail> {
        topics::topic_detail_impl(self, topic).await
    }

    async fn create_topic(&self, request: CreateTopicRequest) -> AppResult<()> {
        topics::create_topic_impl(self, request).await
    }

    async fn update_topic(&self, request: UpdateTopicRequest) -> AppResult<()> {
        topics::update_topic_impl(self, request).await
    }

    async fn delete_topic(&self, topic: &str) -> AppResult<()> {
        topics::delete_topic_impl(self, topic).await
    }

    async fn purge_topic(&self, _topic: &str) -> AppResult<()> {
        Err(AppError::unsupported(
            "RocketMQ cannot purge a topic — delete the topic or wait for the retention period",
        ))
    }

    async fn list_groups(&self) -> AppResult<Vec<ConsumerGroupSummary>> {
        groups::groups_impl(self).await
    }

    async fn group_detail(&self, group: &str) -> AppResult<ConsumerGroupDetail> {
        groups::group_detail_impl(self, group).await
    }

    async fn reset_group_offsets(&self, request: ResetOffsetsRequest) -> AppResult<()> {
        groups::reset_offsets_impl(self, request).await
    }

    async fn delete_group(&self, group: &str) -> AppResult<()> {
        groups::delete_group_impl(self, group).await
    }

    async fn produce(&self, request: ProduceRequest) -> AppResult<ProduceResult> {
        messages::produce_impl(self, request).await
    }

    async fn browse(&self, request: BrowseRequest) -> AppResult<BrowseResult> {
        messages::browse_impl(self, request).await
    }

    async fn open_stream(
        &self,
        request: StreamRequest,
    ) -> AppResult<tokio::sync::mpsc::Receiver<crate::mq::provider::StreamEvent>> {
        messages::open_stream_impl(self, request).await
    }

    async fn close(&self) -> AppResult<()> {
        self.client().clear();
        Ok(())
    }
}
