use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc::Receiver;

use crate::error::{AppError, AppResult};
use crate::events;
use crate::mq::provider::StreamEvent;
use crate::mq::types::*;
use crate::state::{AppState, JobHandle};

#[tauri::command]
pub async fn produce_message(
    state: State<'_, AppState>,
    connection_id: String,
    request: ProduceRequest,
) -> AppResult<ProduceResult> {
    state.connection(&connection_id)?.produce(request).await
}

#[tauri::command]
pub async fn browse_messages(
    state: State<'_, AppState>,
    connection_id: String,
    request: BrowseRequest,
) -> AppResult<BrowseResult> {
    let budget = Duration::from_millis(request.timeout_ms.unwrap_or(5_000).clamp(200, 120_000));
    let connection = state.connection(&connection_id)?;
    // librdkafka polls are bounded, but a wedged socket must never hang the UI.
    tokio::time::timeout(budget + Duration::from_secs(10), connection.browse(request))
        .await
        .map_err(|_| AppError::Timeout(budget.as_millis() as u64 + 10_000))?
}

/// Start a live tail. Returns the job id; messages arrive on `mq://messages`.
#[tauri::command]
pub async fn start_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    request: StreamRequest,
) -> AppResult<String> {
    let connection = state.connection(&connection_id)?;
    let target = request.topic.clone();
    let receiver: Receiver<StreamEvent> = connection.open_stream(request).await?;

    let job_id = uuid::Uuid::new_v4().to_string();
    let received = Arc::new(AtomicU64::new(0));

    let info = JobInfo {
        id: job_id.clone(),
        kind: "tail".into(),
        connection_id: connection_id.clone(),
        target,
        state: JobState::Running,
        error: None,
        started_at: chrono::Utc::now().timestamp_millis(),
        received: 0,
    };

    state.insert_job(JobHandle {
        info,
        received: received.clone(),
        // Placeholder: replaced below once the task exists.
        abort: tokio::spawn(async {}).abort_handle(),
    });

    let worker_app = app.clone();
    let worker_job = job_id.clone();
    let worker_received = received.clone();
    let task = tokio::spawn(async move {
        pump(worker_app, worker_job, worker_received, receiver).await;
    });

    // Swap in the real abort handle.
    if let Some(handle) = state.remove_job(&job_id) {
        state.insert_job(JobHandle {
            info: handle.info,
            received: handle.received,
            abort: task.abort_handle(),
        });
    }

    events::emit_job(&app, &job_id, JobState::Running, None, 0, false);
    Ok(job_id)
}

async fn pump(
    app: AppHandle,
    job_id: String,
    received: Arc<AtomicU64>,
    mut receiver: Receiver<StreamEvent>,
) {
    let mut failure: Option<String> = None;

    while let Some(event) = receiver.recv().await {
        match event {
            StreamEvent::Batch(messages) => {
                if messages.is_empty() {
                    continue;
                }
                let total = received.fetch_add(messages.len() as u64, Ordering::Relaxed)
                    + messages.len() as u64;
                events::emit_messages(&app, &job_id, messages);
                events::emit_job(&app, &job_id, JobState::Running, None, total, false);
            }
            StreamEvent::Idle => {
                let total = received.load(Ordering::Relaxed);
                events::emit_job(&app, &job_id, JobState::Running, None, total, true);
            }
            StreamEvent::End => break,
            StreamEvent::Failed(error) => {
                failure = Some(error);
                break;
            }
        }
    }

    let total = received.load(Ordering::Relaxed);
    let state = app.state::<AppState>();
    state.remove_job(&job_id);

    match failure {
        Some(error) => events::emit_job(&app, &job_id, JobState::Failed, Some(error), total, false),
        None => events::emit_job(&app, &job_id, JobState::Stopped, None, total, false),
    }
}

#[tauri::command]
pub fn stop_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<JobInfo> {
    let info = state.abort_job(&job_id)?;
    events::emit_job(&app, &job_id, JobState::Stopped, None, info.received, false);
    Ok(info)
}

#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>) -> Vec<JobInfo> {
    state.jobs()
}
