use serde::Serialize;
use tauri::State;

use crate::error::AppResult;
use crate::mq::types::AppSettings;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub tauri_version: String,
    pub os: String,
    pub arch: String,
    /// `false` when the binary was built without the `tls` feature.
    pub tls_supported: bool,
    pub workspace_path: String,
    pub providers: Vec<String>,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        name: "MQ Manager".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        tauri_version: tauri::VERSION.into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        tls_supported: cfg!(feature = "tls"),
        workspace_path: state.workspace.path().display().to_string(),
        providers: state
            .registry
            .descriptors()
            .into_iter()
            .map(|descriptor| descriptor.id)
            .collect(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.workspace.snapshot().settings
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> AppResult<AppSettings> {
    state.workspace.mutate(|workspace| {
        workspace.settings = settings.clone();
        settings.clone()
    })
}
