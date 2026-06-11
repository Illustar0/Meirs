use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("failed to resolve system config directory")]
    ConfigDirUnavailable,

    #[error("portal info file not found: {0}")]
    PortalInfoNotFound(PathBuf),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Portal(#[from] meirs_core::EPortalError),
}
