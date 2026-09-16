use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::mq::types::{JobState, Message};

pub const EVENT_MESSAGES: &str = "mq://messages";
pub const EVENT_JOB: &str = "mq://job";
pub const EVENT_CONNECTIONS: &str = "mq://connections";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageBatchEvent {
    pub job_id: String,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    pub job_id: String,
    pub state: JobState,
    #[serde(default)]
    pub error: Option<String>,
    pub received: u64,
    /// `true` when the state only reflects an idle heart-beat.
    #[serde(default)]
    pub idle: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsEvent {
    pub reason: String,
    pub profile_id: Option<String>,
}

pub fn emit_messages(app: &AppHandle, job_id: &str, messages: Vec<Message>) {
    let payload = MessageBatchEvent {
        job_id: job_id.to_string(),
        messages,
    };
    if let Err(error) = app.emit(EVENT_MESSAGES, payload) {
        tracing::warn!(%error, "could not emit message batch");
    }
}

pub fn emit_job(
    app: &AppHandle,
    job_id: &str,
    state: JobState,
    error: Option<String>,
    received: u64,
    idle: bool,
) {
    let payload = JobEvent {
        job_id: job_id.to_string(),
        state,
        error,
        received,
        idle,
    };
    if let Err(error) = app.emit(EVENT_JOB, payload) {
        tracing::warn!(%error, "could not emit job state");
    }
}

pub fn emit_connections(app: &AppHandle, reason: &str, profile_id: Option<String>) {
    let payload = ConnectionsEvent {
        reason: reason.to_string(),
        profile_id,
    };
    if let Err(error) = app.emit(EVENT_CONNECTIONS, payload) {
        tracing::warn!(%error, "could not emit connection change");
    }
}
