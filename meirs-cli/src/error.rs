use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("failed to resolve system config directory")]
    ConfigDirUnavailable,

    #[error("portal info file not found: {0}")]
    PortalInfoNotFound(PathBuf),

    #[error(
        "missing required option(s) for non-interactive {command}: {options}. Run this command in a terminal to be prompted, or pass the option(s) explicitly"
    )]
    NonInteractiveMissingOptions {
        command: &'static str,
        options: &'static str,
    },

    #[error("portal ISP information not found")]
    IspInfoNotFound,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Portal(#[from] meirs_core::EPortalError),
}
