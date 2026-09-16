use tauri::State;

use crate::error::AppResult;
use crate::mq::types::*;
use crate::state::AppState;

#[tauri::command]
pub async fn list_topics(
    state: State<'_, AppState>,
    connection_id: String,
    include_internal: Option<bool>,
) -> AppResult<Vec<TopicSummary>> {
    let mut topics = state.connection(&connection_id)?.list_topics().await?;
    if !include_internal.unwrap_or(false) {
        topics.retain(|topic| !topic.internal);
    }
    Ok(topics)
}

#[tauri::command]
pub async fn get_topic(
    state: State<'_, AppState>,
    connection_id: String,
    topic: String,
) -> AppResult<TopicDetail> {
    state.connection(&connection_id)?.topic_detail(&topic).await
}

/// Lighter than [`get_topic`] — only the configuration entries.
#[tauri::command]
pub async fn get_topic_configs(
    state: State<'_, AppState>,
    connection_id: String,
    topic: String,
) -> AppResult<Vec<ConfigEntry>> {
    state
        .connection(&connection_id)?
        .topic_configs(&topic)
        .await
}

#[tauri::command]
pub async fn create_topic(
    state: State<'_, AppState>,
    connection_id: String,
    request: CreateTopicRequest,
) -> AppResult<()> {
    state
        .connection(&connection_id)?
        .create_topic(request)
        .await
}

#[tauri::command]
pub async fn update_topic(
    state: State<'_, AppState>,
    connection_id: String,
    request: UpdateTopicRequest,
) -> AppResult<()> {
    state
        .connection(&connection_id)?
        .update_topic(request)
        .await
}

#[tauri::command]
pub async fn delete_topic(
    state: State<'_, AppState>,
    connection_id: String,
    topic: String,
) -> AppResult<()> {
    state.connection(&connection_id)?.delete_topic(&topic).await
}

#[tauri::command]
pub async fn purge_topic(
    state: State<'_, AppState>,
    connection_id: String,
    topic: String,
) -> AppResult<()> {
    state.connection(&connection_id)?.purge_topic(&topic).await
}
