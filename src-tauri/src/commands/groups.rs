use tauri::State;

use crate::error::AppResult;
use crate::mq::types::*;
use crate::state::AppState;

#[tauri::command]
pub async fn list_groups(
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<Vec<ConsumerGroupSummary>> {
    state.connection(&connection_id)?.list_groups().await
}

#[tauri::command]
pub async fn get_group(
    state: State<'_, AppState>,
    connection_id: String,
    group: String,
) -> AppResult<ConsumerGroupDetail> {
    state.connection(&connection_id)?.group_detail(&group).await
}

#[tauri::command]
pub async fn delete_group(
    state: State<'_, AppState>,
    connection_id: String,
    group: String,
) -> AppResult<()> {
    state.connection(&connection_id)?.delete_group(&group).await
}

#[tauri::command]
pub async fn reset_group_offsets(
    state: State<'_, AppState>,
    connection_id: String,
    request: ResetOffsetsRequest,
) -> AppResult<()> {
    state
        .connection(&connection_id)?
        .reset_group_offsets(request)
        .await
}
