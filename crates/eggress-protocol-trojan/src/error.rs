use std::fmt;

/// Stable diagnostic codes for Trojan protocol errors.
///
/// These codes classify errors for structured logging, metrics, and
/// diagnostic output. They are distinct from the pproxy compatibility
/// layer's `DiagnosticCode` to avoid cross-crate dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrojanDiagnosticCode {
    /// IO error during handshake read/write (connection reset, EOF, etc.)
    IoError,
    /// TLS handshake or configuration failure
    TlsError,
    /// Password hash mismatch (constant-time comparison)
    AuthenticationFailed,
    /// Server refused the connection (CONNECT reply not supported by spec)
    ConnectionRefused,
    /// Protocol violation: unsupported ATYP, invalid command, bad CRLF, etc.
    ProtocolViolation,
    /// Target domain exceeds 1-255 byte limit
    InvalidTarget,
}

impl fmt::Display for TrojanDiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::IoError => "io_error",
            Self::TlsError => "tls_error",
            Self::AuthenticationFailed => "authentication_failed",
            Self::ConnectionRefused => "connection_refused",
            Self::ProtocolViolation => "protocol_violation",
            Self::InvalidTarget => "invalid_target",
        };
        f.write_str(label)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TrojanError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("authentication failed")]
    AuthFailed,
    #[error("connection refused by server")]
    ConnectionRefused,
    #[error("protocol error: {0}")]
    Protocol(String),
}

impl TrojanError {
    /// Return the stable [`TrojanDiagnosticCode`] that classifies this error.
    pub fn diagnostic_code(&self) -> TrojanDiagnosticCode {
        match self {
            Self::Io(_) => TrojanDiagnosticCode::IoError,
            Self::Tls(_) => TrojanDiagnosticCode::TlsError,
            Self::AuthFailed => TrojanDiagnosticCode::AuthenticationFailed,
            Self::ConnectionRefused => TrojanDiagnosticCode::ConnectionRefused,
            Self::Protocol(_) => TrojanDiagnosticCode::ProtocolViolation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_code_io() {
        let err = TrojanError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "reset",
        ));
        assert_eq!(err.diagnostic_code(), TrojanDiagnosticCode::IoError);
    }

    #[test]
    fn diagnostic_code_tls() {
        let err = TrojanError::Tls("handshake failed".into());
        assert_eq!(err.diagnostic_code(), TrojanDiagnosticCode::TlsError);
    }

    #[test]
    fn diagnostic_code_auth_failed() {
        let err = TrojanError::AuthFailed;
        assert_eq!(
            err.diagnostic_code(),
            TrojanDiagnosticCode::AuthenticationFailed
        );
    }

    #[test]
    fn diagnostic_code_connection_refused() {
        let err = TrojanError::ConnectionRefused;
        assert_eq!(
            err.diagnostic_code(),
            TrojanDiagnosticCode::ConnectionRefused
        );
    }

    #[test]
    fn diagnostic_code_protocol() {
        let err = TrojanError::Protocol("bad atyp".into());
        assert_eq!(
            err.diagnostic_code(),
            TrojanDiagnosticCode::ProtocolViolation
        );
    }

    #[test]
    fn diagnostic_code_display_is_snake_case() {
        assert_eq!(TrojanDiagnosticCode::IoError.to_string(), "io_error");
        assert_eq!(TrojanDiagnosticCode::TlsError.to_string(), "tls_error");
        assert_eq!(
            TrojanDiagnosticCode::AuthenticationFailed.to_string(),
            "authentication_failed"
        );
        assert_eq!(
            TrojanDiagnosticCode::ConnectionRefused.to_string(),
            "connection_refused"
        );
        assert_eq!(
            TrojanDiagnosticCode::ProtocolViolation.to_string(),
            "protocol_violation"
        );
        assert_eq!(
            TrojanDiagnosticCode::InvalidTarget.to_string(),
            "invalid_target"
        );
    }
}
