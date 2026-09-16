use std::time::Instant;

use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::events;
use crate::mq::types::*;
use crate::state::{ActiveConnection, AppState};

#[tauri::command]
pub fn list_providers(state: State<'_, AppState>) -> Vec<ProviderDescriptor> {
    state.registry.descriptors()
}

#[tauri::command]
pub fn list_connections(state: State<'_, AppState>) -> Vec<ConnectionProfile> {
    state.workspace.snapshot().connections
}

#[tauri::command]
pub fn connection_statuses(state: State<'_, AppState>) -> Vec<ConnectionStatus> {
    state.statuses()
}

/// Capabilities of a live connection — the UI caches them after a reload.
#[tauri::command]
pub fn connection_capabilities(state: State<'_, AppState>, id: String) -> AppResult<Capabilities> {
    Ok(state.active(&id)?.capabilities.clone())
}

#[tauri::command]
pub fn save_connection(
    state: State<'_, AppState>,
    mut profile: ConnectionProfile,
) -> AppResult<ConnectionProfile> {
    if state.registry.get(&profile.provider).is_none() {
        return Err(AppError::ProviderNotFound(profile.provider.clone()));
    }

    let now = chrono::Utc::now().timestamp_millis();
    if profile.id.trim().is_empty() {
        profile.id = uuid::Uuid::new_v4().to_string();
    }
    if profile.name.trim().is_empty() {
        profile.name = format!("{} connection", profile.provider);
    }
    profile.updated_at = now;

    state.workspace.mutate(|workspace| {
        match workspace
            .connections
            .iter_mut()
            .find(|candidate| candidate.id == profile.id)
        {
            Some(existing) => {
                profile.created_at = if existing.created_at == 0 {
                    now
                } else {
                    existing.created_at
                };
                profile.last_connected_at = existing.last_connected_at;
                *existing = profile.clone();
            }
            None => {
                profile.created_at = now;
                workspace.connections.push(profile.clone());
            }
        }
        profile.clone()
    })
}

#[tauri::command]
pub async fn connect(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<ConnectResult> {
    let profile = state
        .workspace
        .snapshot()
        .connections
        .into_iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| AppError::ProfileNotFound(id.clone()))?;

    // Reconnecting replaces the previous handle.
    if let Some(active) = state.take_connection(&id) {
        let _ = active.connection.close().await;
    }
    state.abort_jobs_for(&id);

    let provider = state
        .registry
        .get(&profile.provider)
        .ok_or_else(|| AppError::ProviderNotFound(profile.provider.clone()))?;

    let started = Instant::now();
    let connection = provider.connect(&profile).await?;
    let latency_ms = started.elapsed().as_millis() as u64;
    let cluster = connection.cluster_info().await?;
    let capabilities = connection.capabilities();
    let now = chrono::Utc::now().timestamp_millis();

    state.insert_connection(ActiveConnection {
        profile: profile.clone(),
        connection,
        cluster: cluster.clone(),
        capabilities: capabilities.clone(),
        connected_at: now,
        latency_ms,
    });

    state.workspace.mutate(|workspace| {
        if let Some(stored) = workspace
            .connections
            .iter_mut()
            .find(|candidate| candidate.id == id)
        {
            stored.last_connected_at = Some(now);
        }
    })?;

    events::emit_connections(&app, "connected", Some(id.clone()));

    Ok(ConnectResult {
        status: ConnectionStatus {
            profile_id: id,
            provider_id: connection_provider_id(&state, &profile.provider),
            display_name: Some(profile.name.clone()),
            state: ConnectionState::Connected,
            error: None,
            cluster: Some(cluster),
            connected_at: Some(now),
            latency_ms: Some(latency_ms),
        },
        capabilities,
        provider: provider.descriptor(),
    })
}

/// Connect without touching the workspace — used by the “Test” button.
#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    mut profile: ConnectionProfile,
) -> AppResult<ConnectResult> {
    let provider = state
        .registry
        .get(&profile.provider)
        .ok_or_else(|| AppError::ProviderNotFound(profile.provider.clone()))?;

    if profile.id.trim().is_empty() {
        profile.id = "test".to_string();
    }

    let started = Instant::now();
    let (connection, cluster) = provider.test(&profile).await?;
    let latency_ms = started.elapsed().as_millis() as u64;
    let capabilities = connection.capabilities();
    let _ = connection.close().await;

    Ok(ConnectResult {
        status: ConnectionStatus {
            profile_id: profile.id,
            provider_id: provider.descriptor().id,
            display_name: Some(profile.name.clone()),
            state: ConnectionState::Connected,
            error: None,
            cluster: Some(cluster),
            connected_at: Some(chrono::Utc::now().timestamp_millis()),
            latency_ms: Some(latency_ms),
        },
        capabilities,
        provider: provider.descriptor(),
    })
}

#[tauri::command]
pub async fn disconnect(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.abort_jobs_for(&id);
    if let Some(active) = state.take_connection(&id) {
        active.connection.close().await?;
        events::emit_connections(&app, "disconnected", Some(id));
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_connection(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    state.abort_jobs_for(&id);
    if let Some(active) = state.take_connection(&id) {
        let _ = active.connection.close().await;
    }
    state
        .workspace
        .mutate(|workspace| workspace.connections.retain(|profile| profile.id != id))?;
    events::emit_connections(&app, "removed", Some(id));
    Ok(())
}

/// Close every socket — called on window close so Kafka threads shut down.
pub async fn disconnect_all(state: &AppState) {
    state.abort_all_jobs();
    let ids: Vec<String> = state.statuses().into_iter().map(|s| s.profile_id).collect();
    for id in ids {
        if let Some(active) = state.take_connection(&id) {
            if let Err(error) = active.connection.close().await {
                tracing::warn!(%error, profile = %id, "failed to close connection");
            }
        }
    }
}

/// Convenience used by sibling command modules.
fn connection_provider_id(state: &AppState, provider: &str) -> String {
    state
        .registry
        .get(provider)
        .map(|provider| provider.descriptor().id)
        .unwrap_or_else(|| provider.to_string())
}
