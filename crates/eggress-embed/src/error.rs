/// Stable error type for the embed API.
///
/// All variants carry string messages with credentials redacted.
/// The variants are designed for stable PyO3 mapping in later phases.
#[derive(Debug, thiserror::Error)]
pub enum EggressError {
    /// Configuration parsing or validation error.
    #[error("config error: {0}")]
    Config(String),

    /// Runtime initialization error.
    #[error("runtime error: {0}")]
    Runtime(String),

    /// Service startup error.
    #[error("startup error: {0}")]
    Startup(String),

    /// Configuration reload error.
    #[error("reload error: {0}")]
    Reload(String),

    /// Shutdown error.
    #[error("shutdown error: {0}")]
    Shutdown(String),

    /// Attempted to use a feature not supported by the embed API.
    #[error("unsupported feature: {feature}: {message}")]
    UnsupportedFeature {
        /// Feature name.
        feature: String,
        /// Human-readable explanation.
        message: String,
    },

    /// Internal error (should not occur in normal usage).
    #[error("internal error: {0}")]
    Internal(String),
}

impl EggressError {
    /// Return a short category label for the error.
    pub fn category(&self) -> &'static str {
        match self {
            EggressError::Config(_) => "config",
            EggressError::Runtime(_) => "runtime",
            EggressError::Startup(_) => "startup",
            EggressError::Reload(_) => "reload",
            EggressError::Shutdown(_) => "shutdown",
            EggressError::UnsupportedFeature { .. } => "unsupported_feature",
            EggressError::Internal(_) => "internal",
        }
    }
}
