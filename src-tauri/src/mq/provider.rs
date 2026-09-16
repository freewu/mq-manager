//! The provider contract.
//!
//! Adding support for a new broker means implementing [`MqProvider`] plus
//! [`MqConnection`] in a new module under `mq::drivers` and registering it in
//! `mq::registry` — no changes to the command layer or the UI are required.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::error::AppResult;
use crate::mq::types::*;

/// Events emitted by a live stream.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// One poll cycle produced messages.
    Batch(Vec<Message>),
    /// Nothing arrived within the idle window — used for UI heart-beats.
    Idle,
    /// The stream finished on its own.
    End,
    /// The stream failed.
    Failed(String),
}

/// A broker specific driver factory. Stateless and cheap to clone.
#[async_trait]
pub trait MqProvider: Send + Sync + 'static {
    /// Static description used to drive the connection form and capability gating.
    fn descriptor(&self) -> ProviderDescriptor;

    /// Open a connection. Implementations should fail fast with a readable message.
    async fn connect(&self, profile: &ConnectionProfile) -> AppResult<Arc<dyn MqConnection>>;

    /// Connect, gather the cluster banner, then close. Used by “Test connection”.
    async fn test(
        &self,
        profile: &ConnectionProfile,
    ) -> AppResult<(Arc<dyn MqConnection>, ClusterInfo)> {
        let connection = self.connect(profile).await?;
        let info = connection.cluster_info().await?;
        Ok((connection, info))
    }
}

/// One live connection to one broker.
///
/// Every method has a default that reports “not supported”, so a driver only
/// implements what its protocol actually offers. The UI never calls a method
/// whose capability flag is `false`.
#[async_trait]
pub trait MqConnection: Send + Sync + 'static {
    /// Provider id of the driver backing this connection.
    fn provider_id(&self) -> &str;

    /// Profile id this connection was created from.
    fn profile_id(&self) -> &str;

    fn capabilities(&self) -> Capabilities;

    /// Cheap round-trip used for latency measurement.
    async fn ping(&self) -> AppResult<()>;

    // --- cluster ---------------------------------------------------------

    async fn cluster_info(&self) -> AppResult<ClusterInfo>;

    async fn list_nodes(&self) -> AppResult<Vec<NodeInfo>> {
        Ok(Vec::new())
    }

    // --- topics ----------------------------------------------------------

    async fn list_topics(&self) -> AppResult<Vec<TopicSummary>>;

    async fn topic_detail(&self, topic: &str) -> AppResult<TopicDetail>;

    async fn topic_configs(&self, topic: &str) -> AppResult<Vec<ConfigEntry>> {
        Ok(self.topic_detail(topic).await?.configs)
    }

    async fn create_topic(&self, request: CreateTopicRequest) -> AppResult<()> {
        let _ = request;
        Err(crate::error::AppError::unsupported("create_topic"))
    }

    async fn update_topic(&self, request: UpdateTopicRequest) -> AppResult<()> {
        let _ = request;
        Err(crate::error::AppError::unsupported("update_topic"))
    }

    async fn delete_topic(&self, topic: &str) -> AppResult<()> {
        let _ = topic;
        Err(crate::error::AppError::unsupported("delete_topic"))
    }

    /// Remove every message from a topic without deleting it.
    async fn purge_topic(&self, topic: &str) -> AppResult<()> {
        let _ = topic;
        Err(crate::error::AppError::unsupported("purge_topic"))
    }

    // --- groups ----------------------------------------------------------

    async fn list_groups(&self) -> AppResult<Vec<ConsumerGroupSummary>> {
        Ok(Vec::new())
    }

    async fn group_detail(&self, group: &str) -> AppResult<ConsumerGroupDetail> {
        let _ = group;
        Err(crate::error::AppError::unsupported("group_detail"))
    }

    async fn delete_group(&self, group: &str) -> AppResult<()> {
        let _ = group;
        Err(crate::error::AppError::unsupported("delete_group"))
    }

    async fn reset_group_offsets(&self, request: ResetOffsetsRequest) -> AppResult<()> {
        let _ = request;
        Err(crate::error::AppError::unsupported("reset_group_offsets"))
    }

    // --- messages --------------------------------------------------------

    async fn produce(&self, request: ProduceRequest) -> AppResult<ProduceResult> {
        let _ = request;
        Err(crate::error::AppError::unsupported("produce"))
    }

    async fn browse(&self, request: BrowseRequest) -> AppResult<BrowseResult> {
        let _ = request;
        Err(crate::error::AppError::unsupported("browse"))
    }

    /// Start a long running stream. The driver owns the background task and
    /// pushes [`StreamEvent`]s into the returned channel.
    async fn open_stream(&self, request: StreamRequest) -> AppResult<mpsc::Receiver<StreamEvent>> {
        let _ = request;
        Err(crate::error::AppError::unsupported("tail"))
    }

    // --- lifecycle -------------------------------------------------------

    /// Release resources. Called when the user closes a connection.
    async fn close(&self) -> AppResult<()> {
        Ok(())
    }
}
