#[derive(Debug, thiserror::Error)]
pub enum EPortalError {
    #[error("invalid JSONP response: {0}")]
    InvalidJsonp(String),

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid IP address: {0}")]
    InvalidIp(#[from] std::net::AddrParseError),

    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("invalid portal redirect URL: {0}")]
    InvalidRedirectUrl(String),

    #[error("portal redirect URL not found")]
    PortalRedirectUrlNotFound,

    #[error("portal not detected")]
    PortalNotDetected,

    #[error("portal returned failure: {0}")]
    PortalRejected(String),

    #[error("client is already online")]
    AlreadyOnline,
}
