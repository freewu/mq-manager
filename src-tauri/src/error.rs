use serde::{Serialize, Serializer};

/// Every fallible operation in the backend returns this error.
///
/// It is serialised to the frontend as `{ kind, message }` so the UI can decide
/// between a toast, an inline form error or a blocking dialog.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0} is not supported by this broker")]
    Unsupported(String),

    #[error("connection `{0}` is not open")]
    ConnectionNotFound(String),

    #[error("unknown provider `{0}`")]
    ProviderNotFound(String),

    #[error("unknown connection profile `{0}`")]
    ProfileNotFound(String),

    #[error("{0}")]
    Invalid(String),

    #[error("{0}")]
    Broker(String),

    #[error("timed out after {0} ms")]
    Timeout(u64),

    #[error("could not access the local configuration store: {0}")]
    Storage(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Tauri(#[from] tauri::Error),

    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl AppError {
    pub fn unsupported(what: impl Into<String>) -> Self {
        AppError::Unsupported(what.into())
    }

    pub fn invalid(what: impl Into<String>) -> Self {
        AppError::Invalid(what.into())
    }

    pub fn broker(what: impl Into<String>) -> Self {
        AppError::Broker(what.into())
    }

    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Unsupported(_) => "unsupported",
            AppError::ConnectionNotFound(_) => "connectionNotFound",
            AppError::ProviderNotFound(_) => "providerNotFound",
            AppError::ProfileNotFound(_) => "profileNotFound",
            AppError::Invalid(_) => "invalid",
            AppError::Broker(_) => "broker",
            AppError::Timeout(_) => "timeout",
            AppError::Storage(_) => "storage",
            AppError::Io(_) => "io",
            AppError::Json(_) => "json",
            AppError::Tauri(_) => "tauri",
            AppError::Join(_) => "join",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
