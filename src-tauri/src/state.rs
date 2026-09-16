use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::task::AbortHandle;

use crate::config::WorkspaceStore;
use crate::error::{AppError, AppResult};
use crate::mq::types::*;
use crate::mq::{MqConnection, ProviderRegistry};

/// A live connection plus everything we learned about it at connect time.
pub struct ActiveConnection {
    pub profile: ConnectionProfile,
    pub connection: Arc<dyn MqConnection>,
    pub cluster: ClusterInfo,
    pub capabilities: Capabilities,
    pub connected_at: i64,
    pub latency_ms: u64,
}

/// A background stream (live tail) owned by the backend.
pub struct JobHandle {
    pub info: JobInfo,
    pub received: Arc<AtomicU64>,
    pub abort: AbortHandle,
}

pub struct AppState {
    pub registry: ProviderRegistry,
    pub workspace: WorkspaceStore,
    connections: RwLock<HashMap<String, Arc<ActiveConnection>>>,
    jobs: RwLock<HashMap<String, JobHandle>>,
}

impl AppState {
    pub fn new(registry: ProviderRegistry, workspace: WorkspaceStore) -> Self {
        Self {
            registry,
            workspace,
            connections: RwLock::new(HashMap::new()),
            jobs: RwLock::new(HashMap::new()),
        }
    }

    // --- connections -----------------------------------------------------

    pub fn insert_connection(&self, active: ActiveConnection) {
        debug_assert_eq!(
            active.connection.profile_id(),
            active.profile.id,
            "connection was created for a different profile"
        );
        let id = active.profile.id.clone();
        self.connections.write().insert(id, Arc::new(active));
    }

    pub fn active(&self, profile_id: &str) -> AppResult<Arc<ActiveConnection>> {
        self.connections
            .read()
            .get(profile_id)
            .cloned()
            .ok_or_else(|| AppError::ConnectionNotFound(profile_id.to_string()))
    }

    pub fn connection(&self, profile_id: &str) -> AppResult<Arc<dyn MqConnection>> {
        Ok(self.active(profile_id)?.connection.clone())
    }

    pub fn take_connection(&self, profile_id: &str) -> Option<Arc<ActiveConnection>> {
        self.connections.write().remove(profile_id)
    }

    pub fn statuses(&self) -> Vec<ConnectionStatus> {
        let connections = self.connections.read();
        let mut statuses: Vec<ConnectionStatus> = connections
            .values()
            .map(|active| ConnectionStatus {
                profile_id: active.profile.id.clone(),
                provider_id: active.connection.provider_id().to_string(),
                display_name: Some(active.profile.name.clone()),
                state: ConnectionState::Connected,
                error: None,
                cluster: Some(active.cluster.clone()),
                connected_at: Some(active.connected_at),
                latency_ms: Some(active.latency_ms),
            })
            .collect();
        statuses.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
        statuses
    }

    // --- jobs ------------------------------------------------------------

    pub fn insert_job(&self, handle: JobHandle) {
        self.jobs.write().insert(handle.info.id.clone(), handle);
    }

    pub fn remove_job(&self, job_id: &str) -> Option<JobHandle> {
        self.jobs.write().remove(job_id)
    }

    pub fn abort_job(&self, job_id: &str) -> AppResult<JobInfo> {
        let mut jobs = self.jobs.write();
        let handle = jobs
            .remove(job_id)
            .ok_or_else(|| AppError::invalid(format!("job `{job_id}` is not running")))?;
        handle.abort.abort();
        let mut info = handle.info.clone();
        info.state = JobState::Stopped;
        info.received = handle.received.load(std::sync::atomic::Ordering::Relaxed);
        Ok(info)
    }

    pub fn jobs(&self) -> Vec<JobInfo> {
        let jobs = self.jobs.read();
        let mut infos: Vec<JobInfo> = jobs
            .values()
            .map(|handle| {
                let mut info = handle.info.clone();
                info.received = handle.received.load(std::sync::atomic::Ordering::Relaxed);
                info
            })
            .collect();
        infos.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        infos
    }

    /// Abort every job belonging to a connection (used on disconnect).
    pub fn abort_jobs_for(&self, profile_id: &str) {
        let mut jobs = self.jobs.write();
        let doomed: Vec<String> = jobs
            .values()
            .filter(|handle| handle.info.connection_id == profile_id)
            .map(|handle| handle.info.id.clone())
            .collect();
        for id in doomed {
            if let Some(handle) = jobs.remove(&id) {
                handle.abort.abort();
            }
        }
    }

    pub fn abort_all_jobs(&self) {
        let mut jobs = self.jobs.write();
        for (_, handle) in jobs.drain() {
            handle.abort.abort();
        }
    }
}
