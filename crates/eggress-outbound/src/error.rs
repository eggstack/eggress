//! Outbound error types.
//!
//! `OutboundError` is the failure surface for listener-free chain
//! construction and execution (`OutboundConnector` compatibility methods,
//! UDP association, TOML/pproxy constructors). Display strings preserve the
//! established `EggressError` message families (`config error: …`,
//! `runtime error: …`, `unsupported feature: …`) so diagnostics remain stable
//! across the `eggress-embed` compatibility facade. All variants carry
//! redacted strings only; credentials never enter these messages.

/// Failure for listener-free outbound construction and execution.
///
/// General-purpose and protocol-neutral: callers match on variants for
/// policy without parsing display strings. Detailed per-connection TCP
/// failures use [`crate::OutboundConnectError`] instead.
#[derive(Debug, thiserror::Error)]
pub enum OutboundError {
    /// Configuration parsing, validation, or construction error.
    #[error("config error: {0}")]
    Config(String),

    /// Connection or execution error (compatibility surface).
    #[error("runtime error: {0}")]
    Runtime(String),

    /// Attempted to use a feature not available in this build.
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

impl OutboundError {
    /// Return a short category label for the error.
    pub fn category(&self) -> &'static str {
        match self {
            OutboundError::Config(_) => "config",
            OutboundError::Runtime(_) => "runtime",
            OutboundError::UnsupportedFeature { .. } => "unsupported_feature",
            OutboundError::Internal(_) => "internal",
        }
    }
}
