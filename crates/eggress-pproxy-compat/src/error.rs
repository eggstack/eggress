use crate::diagnostics::DiagnosticCode;

/// Errors from pproxy compatibility translation.
///
/// Each variant maps to a stable [`DiagnosticCode`] via [`CompatError::code()`].
#[derive(Debug, thiserror::Error)]
pub enum CompatError {
    /// Maps to [`DiagnosticCode::UnsupportedProtocol`].
    #[error("unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    /// Maps to a code determined by the feature name (see [`DiagnosticCode`] docs).
    #[error("unsupported feature: {feature}")]
    UnsupportedFeature {
        feature: &'static str,
        detail: String,
    },

    /// Maps to [`DiagnosticCode::InvalidUriSyntax`].
    #[error("invalid URI: {message}")]
    InvalidUri { message: String },

    /// Maps to [`DiagnosticCode::InvalidUriSyntax`].
    #[error("invalid pproxy arguments: {message}")]
    InvalidArgs { message: String },

    /// Maps to [`DiagnosticCode::InvalidChainComposition`].
    #[error("config validation failed: {message}")]
    ConfigValidation { message: String },

    /// Maps to [`DiagnosticCode::MissingTarget`].
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

    /// Return the stable [`DiagnosticCode`] that classifies this error.
    pub fn code(&self) -> DiagnosticCode {
        match self {
            Self::UnsupportedProtocol(_) => DiagnosticCode::UnsupportedProtocol,
            Self::UnsupportedFeature { feature, .. } => {
                crate::diagnostics::classify_unsupported_feature_code(feature)
            }
            Self::InvalidUri { .. } => DiagnosticCode::InvalidUriSyntax,
            Self::InvalidArgs { .. } => DiagnosticCode::InvalidUriSyntax,
            Self::ConfigValidation { .. } => DiagnosticCode::InvalidChainComposition,
            Self::MissingArgument(_) => DiagnosticCode::MissingTarget,
        }
    }
}
