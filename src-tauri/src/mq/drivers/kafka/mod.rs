//! Apache Kafka driver.
//!
//! Implemented on top of `librdkafka` (rdkafka crate). The connection object
//! keeps a long lived admin client plus a producer, and creates short lived
//! consumers for browsing / tailing so that no consumer group is ever polluted.

mod config;
mod groups;
mod messages;
mod proto;
mod topics;

use async_trait::async_trait;
use rdkafka::admin::{AdminClient, AdminOptions};
use rdkafka::client::DefaultClientContext;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::DefaultConsumerContext;
use rdkafka::error::KafkaError;
use rdkafka::producer::{FutureProducer, Producer};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

use crate::error::{AppError, AppResult};
use crate::mq::provider::{MqConnection, MqProvider};
use crate::mq::types::*;

use self::config::{KafkaOptions, SASL_MECHANISMS, SECURITY_PROTOCOLS};

pub(crate) const PROVIDER_ID: &str = "kafka";

/// Convert any driver error into the neutral application error.
pub(crate) fn kerr<E: std::fmt::Display>(err: E) -> AppError {
    AppError::broker(err.to_string())
}

pub(crate) fn kres<T, E: std::fmt::Display>(result: Result<T, E>) -> AppResult<T> {
    result.map_err(kerr)
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct KafkaProvider;

#[async_trait]
impl MqProvider for KafkaProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        descriptor()
    }

    async fn connect(&self, profile: &ConnectionProfile) -> AppResult<Arc<dyn MqConnection>> {
        let connection = KafkaConnection::open(profile).await?;
        Ok(Arc::new(connection))
    }
}

pub fn descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: PROVIDER_ID.into(),
        name: "Apache Kafka".into(),
        description: "Distributed commit log. Topics, partitions, consumer groups and offsets."
            .into(),
        vendor: "Apache Software Foundation".into(),
        accent: "#5b8def".into(),
        docs_url: Some("https://kafka.apache.org/documentation/".into()),
        default_port: Some(9092),
        driver_version: env!("CARGO_PKG_VERSION").into(),
        capabilities: Capabilities::full(),
        fields: connection_fields(),
    }
}

fn connection_fields() -> Vec<ConnectionField> {
    vec![
        ConnectionField::text("bootstrapServers", "Bootstrap servers")
            .required()
            .placeholder("localhost:9092, broker2:9092")
            .help("Comma separated list of `host:port` entries used for the initial handshake.")
            .group("Connection"),
        ConnectionField::text("clientId", "Client id")
            .default_value(json!("mq-manager"))
            .help("Identifies this client in broker logs and quotas.")
            .group("Connection"),
        ConnectionField::select("securityProtocol", "Security protocol")
            .options(SECURITY_PROTOCOLS)
            .default_value(json!("PLAINTEXT"))
            .group("Security"),
        ConnectionField::select("saslMechanism", "SASL mechanism")
            .options(SASL_MECHANISMS)
            .default_value(json!("PLAIN"))
            .group("Security")
            .when("securityProtocol", &["SASL_PLAINTEXT", "SASL_SSL"]),
        ConnectionField::text("saslUsername", "SASL username")
            .group("Security")
            .when("securityProtocol", &["SASL_PLAINTEXT", "SASL_SSL"]),
        ConnectionField::password("saslPassword", "SASL password")
            .group("Security")
            .when("securityProtocol", &["SASL_PLAINTEXT", "SASL_SSL"]),
        ConnectionField::text("sslCaLocation", "CA certificate path")
            .placeholder("/etc/kafka/ca.pem")
            .group("Security")
            .when("securityProtocol", &["SSL", "SASL_SSL"]),
        ConnectionField::text("sslCertificateLocation", "Client certificate path")
            .group("Security")
            .advanced()
            .when("securityProtocol", &["SSL", "SASL_SSL"]),
        ConnectionField::text("sslKeyLocation", "Client key path")
            .group("Security")
            .advanced()
            .when("securityProtocol", &["SSL", "SASL_SSL"]),
        ConnectionField::password("sslKeyPassword", "Client key password")
            .group("Security")
            .advanced()
            .when("securityProtocol", &["SSL", "SASL_SSL"]),
        ConnectionField::boolean(
            "enableSslCertificateVerification",
            "Verify TLS certificates",
        )
        .default_value(json!(true))
        .group("Security")
        .advanced()
        .when("securityProtocol", &["SSL", "SASL_SSL"]),
        ConnectionField::number("requestTimeoutMs", "Request timeout (ms)")
            .default_value(json!(30000))
            .group("Advanced")
            .advanced(),
        ConnectionField::number("socketTimeoutMs", "Socket timeout (ms)")
            .default_value(json!(60000))
            .group("Advanced")
            .advanced(),
        ConnectionField::number("messageMaxBytes", "Max message size (bytes)")
            .default_value(json!(1048576))
            .group("Advanced")
            .advanced(),
        ConnectionField::key_value("additionalProperties", "Additional librdkafka properties")
            .help("Escape hatch for any `librdkafka` configuration key, e.g. `fetch.wait.max.ms`.")
            .group("Advanced")
            .advanced(),
    ]
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

pub struct KafkaConnection {
    profile_id: String,
    target: String,
    options: KafkaOptions,
    client_config: Arc<ClientConfig>,
    admin: Arc<AdminClient<DefaultClientContext>>,
    producer: Arc<FutureProducer<DefaultClientContext>>,
}

impl KafkaConnection {
    pub async fn open(profile: &ConnectionProfile) -> AppResult<Self> {
        let options = KafkaOptions::from_profile(profile)?;
        let client_config = Arc::new(options.client_config());

        let admin_config = client_config.clone();
        let timeout = options.admin_timeout();
        let admin: AdminClient<DefaultClientContext> = tokio::task::spawn_blocking(move || {
            let admin = admin_config
                .create_with_context::<DefaultClientContext, AdminClient<DefaultClientContext>>(
                    DefaultClientContext,
                )?;
            // Force a real round trip so that "Test connection" actually tests something.
            admin.inner().fetch_metadata(None::<&str>, timeout)?;
            Ok::<_, KafkaError>(admin)
        })
        .await
        .map_err(AppError::from)
        .and_then(|result| kres(result))?;

        let producer: FutureProducer<DefaultClientContext> =
            client_config.create().map_err(kerr)?;

        Ok(Self {
            profile_id: profile.id.clone(),
            target: options.target.clone(),
            options,
            client_config,
            admin: Arc::new(admin),
            producer: Arc::new(producer),
        })
    }

    pub(crate) fn target(&self) -> &str {
        &self.target
    }

    pub(crate) fn request_timeout(&self) -> Duration {
        self.options.request_timeout()
    }

    pub(crate) fn admin_timeout(&self) -> Duration {
        self.options.admin_timeout()
    }

    /// Options shared by every admin call (timeouts, retries).
    pub(crate) fn admin_options(&self) -> AdminOptions {
        AdminOptions::new()
            .request_timeout(Some(self.admin_timeout()))
            .operation_timeout(Some(self.request_timeout()))
    }

    /// Run a blocking librdkafka call on the blocking pool.
    pub(crate) async fn blocking<T, F>(&self, task: F) -> AppResult<T>
    where
        F: FnOnce(&KafkaHandle) -> AppResult<T> + Send + 'static,
        T: Send + 'static,
    {
        let handle = self.handle();
        tokio::task::spawn_blocking(move || task(&handle)).await?
    }

    /// A cheap clone-able handle used to move the connection into blocking tasks.
    fn handle(&self) -> KafkaHandle {
        KafkaHandle {
            admin: self.admin.clone(),
            client_config: self.client_config.clone(),
            timeouts: self.options.clone(),
        }
    }
}

/// Small owned bundle that can be moved into `spawn_blocking` closures.
pub(crate) struct KafkaHandle {
    pub admin: Arc<AdminClient<DefaultClientContext>>,
    pub client_config: Arc<ClientConfig>,
    pub timeouts: KafkaOptions,
}

impl KafkaHandle {
    pub fn client(&self) -> &rdkafka::client::Client<DefaultClientContext> {
        self.admin.inner()
    }

    pub fn request_timeout(&self) -> Duration {
        self.timeouts.request_timeout()
    }

    pub fn admin_timeout(&self) -> Duration {
        self.timeouts.admin_timeout()
    }

    /// Build a consumer that never joins a consumer group and never commits.
    ///
    /// A random `group.id` is mandatory for the protocol but manual assignment
    /// means no rebalance is ever triggered.
    pub fn consumer(&self, suffix: &str) -> AppResult<rdkafka::consumer::BaseConsumer> {
        self.consumer_as(&format!("mq-manager-{suffix}"))
    }

    /// Like [`KafkaHandle::consumer`] but pins the consumer group id, which is
    /// required to commit offsets on behalf of an existing group.
    pub fn consumer_as(&self, group_id: &str) -> AppResult<rdkafka::consumer::BaseConsumer> {
        let mut config = (*self.client_config).clone();
        config
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")
            .set("auto.offset.reset", "earliest")
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "10000");
        config
            .create_with_context::<DefaultConsumerContext, rdkafka::consumer::BaseConsumer>(
                DefaultConsumerContext,
            )
            .map_err(kerr)
    }
}

#[async_trait]
impl MqConnection for KafkaConnection {
    fn provider_id(&self) -> &str {
        PROVIDER_ID
    }

    fn profile_id(&self) -> &str {
        &self.profile_id
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::full()
    }

    async fn ping(&self) -> AppResult<()> {
        self.blocking(|handle| {
            handle
                .client()
                .fetch_metadata(None::<&str>, handle.request_timeout())
                .map_err(kerr)
                .map(|_| ())
        })
        .await
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

    async fn delete_group(&self, group: &str) -> AppResult<()> {
        self.delete_group_impl(group).await
    }

    async fn reset_group_offsets(&self, request: ResetOffsetsRequest) -> AppResult<()> {
        self.reset_offsets_impl(request).await
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
        let producer = self.producer.clone();
        tokio::task::spawn_blocking(move || {
            let _ = producer.flush(Duration::from_secs(2));
        })
        .await?;
        Ok(())
    }
}
