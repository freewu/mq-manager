use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::mq::types::{AppSettings, ConnectionProfile};

const WORKSPACE_FILE: &str = "workspace.json";
const WORKSPACE_VERSION: u32 = 1;

/// Everything MQ Manager persists between runs. Deliberately a single JSON file
/// so that “export / import my workspace” stays trivial.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub version: u32,
    #[serde(default)]
    pub connections: Vec<ConnectionProfile>,
    #[serde(default)]
    pub settings: AppSettings,
    #[serde(default)]
    pub favourites: Vec<String>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            version: WORKSPACE_VERSION,
            connections: Vec::new(),
            settings: AppSettings::default(),
            favourites: Vec::new(),
        }
    }
}

pub struct WorkspaceStore {
    path: PathBuf,
    data: RwLock<Workspace>,
}

impl WorkspaceStore {
    pub fn load(app: &AppHandle) -> AppResult<Self> {
        let directory = app.path().app_config_dir()?;
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(WORKSPACE_FILE);

        let data = match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Workspace>(&raw) {
                Ok(workspace) => workspace,
                Err(error) => {
                    // Never silently discard a user's connections: keep the broken
                    // file around and start from an empty workspace.
                    tracing::warn!(%error, path = %path.display(), "corrupt workspace file");
                    let backup = path.with_extension("json.corrupt");
                    std::fs::rename(&path, &backup).map_err(|io| {
                        AppError::Storage(format!(
                            "workspace file is corrupt ({error}) and could not be moved to {}: {io}",
                            backup.display()
                        ))
                    })?;
                    Workspace::default()
                }
            },
            Err(_) => Workspace::default(),
        };

        tracing::info!(path = %path.display(), connections = data.connections.len(), "workspace loaded");
        Ok(Self {
            path,
            data: RwLock::new(data),
        })
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn snapshot(&self) -> Workspace {
        self.data.read().clone()
    }

    /// Apply a mutation and write the result back to disk atomically.
    pub fn mutate<T>(&self, change: impl FnOnce(&mut Workspace) -> T) -> AppResult<T> {
        let mut guard = self.data.write();
        let output = change(&mut guard);
        self.persist(&guard)?;
        Ok(output)
    }

    fn persist(&self, workspace: &Workspace) -> AppResult<()> {
        let raw = serde_json::to_string_pretty(workspace)?;
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, raw)?;
        std::fs::rename(&temporary, &self.path)?;
        Ok(())
    }
}
