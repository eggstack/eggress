/// Error types for HTTP proxy protocol operations.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("malformed request: {0}")]
    MalformedRequest(String),

    #[error("malformed response: {0}")]
    MalformedResponse(String),

    #[error("unsupported HTTP version: {0}")]
    UnsupportedVersion(String),

    #[error("missing required header: {0}")]
    MissingHeader(String),

    #[error("invalid header value: {0}")]
    InvalidHeaderValue(String),

    #[error("header too large")]
    HeaderTooLarge,

    #[error("too many header lines")]
    TooManyHeaders,

    #[error("target address parse error: {0}")]
    TargetParseError(String),

    #[error("authentication required (407 Proxy Authentication Required)")]
    AuthRequired,

    #[error("authentication failed (403 Forbidden)")]
    AuthFailed,

    #[error("connection refused by upstream")]
    ConnectionRefused,

    #[error("gateway timeout (504)")]
    GatewayTimeout,

    #[error("bad gateway (502)")]
    BadGateway,

    #[error("unexpected status code: {0}")]
    UnexpectedStatus(u16),

    #[error("upstream error: {0}")]
    Upstream(String),
}

impl HttpError {
    /// Returns the appropriate HTTP status code for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            HttpError::MalformedRequest(_) => 400,
            HttpError::AuthRequired => 407,
            HttpError::AuthFailed => 403,
            HttpError::ConnectionRefused => 502,
            HttpError::BadGateway => 502,
            HttpError::GatewayTimeout => 504,
            HttpError::HeaderTooLarge => 431,
            HttpError::TooManyHeaders => 431,
            _ => 500,
        }
    }
}
