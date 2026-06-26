/// Errors from pproxy compatibility translation.
#[derive(Debug, thiserror::Error)]
pub enum CompatError {
    #[error("unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    #[error("unsupported feature: {feature}")]
    UnsupportedFeature {
        feature: &'static str,
        detail: String,
    },

    #[error("invalid URI: {message}")]
    InvalidUri { message: String },

    #[error("invalid pproxy arguments: {message}")]
    InvalidArgs { message: String },

    #[error("config validation failed: {message}")]
    ConfigValidation { message: String },

    #[error("missing required argument: {0}")]
    MissingArgument(String),
}

impl CompatError {
    pub fn unsupported(feature: &'static str, detail: impl Into<String>) -> Self {
        Self::UnsupportedFeature {
            feature,
            detail: detail.into(),
        }
    }
}
