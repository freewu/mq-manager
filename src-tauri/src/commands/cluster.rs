use tauri::State;

use crate::error::AppResult;
use crate::mq::types::*;
use crate::state::AppState;

#[tauri::command]
pub async fn cluster_info(
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<ClusterInfo> {
    state.connection(&connection_id)?.cluster_info().await
}

#[tauri::command]
pub async fn list_nodes(
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<Vec<NodeInfo>> {
    state.connection(&connection_id)?.list_nodes().await
}

#[tauri::command]
pub async fn ping(state: State<'_, AppState>, connection_id: String) -> AppResult<u64> {
    let started = std::time::Instant::now();
    state.connection(&connection_id)?.ping().await?;
    Ok(started.elapsed().as_millis() as u64)
}
