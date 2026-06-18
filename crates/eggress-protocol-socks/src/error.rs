/// Error types for SOCKS5 protocol operations.
#[derive(Debug, thiserror::Error)]
pub enum Socks5Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("unsupported SOCKS version: {0}")]
    UnsupportedVersion(u8),

    #[error("unsupported command: {0}")]
    UnsupportedCommand(u8),

    #[error("unsupported address type: {0}")]
    UnsupportedAddressType(u8),

    #[error("unsupported authentication method: {0}")]
    UnsupportedAuthMethod(u8),

    #[error("authentication failed")]
    AuthFailed,

    #[error("credentials too long (max 255 bytes)")]
    CredentialsTooLong,

    #[error("connection refused by SOCKS server")]
    ConnectionRefused,

    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("malformed message: {0}")]
    MalformedMessage(String),

    #[error("method negotiation failed")]
    MethodNegotiationFailed,

    #[error("unexpected end of stream")]
    UnexpectedEof,

    #[error("address too long")]
    AddressTooLong,
}

impl From<Socks5Error> for std::io::Error {
    fn from(e: Socks5Error) -> Self {
        match e {
            Socks5Error::Io(io_err) => io_err,
            other => std::io::Error::other(other),
        }
    }
}

impl Socks5Error {
    /// Returns the display string for an error with hex formatting.
    pub fn display_hex(&self) -> String {
        match self {
            Socks5Error::UnsupportedVersion(v) => {
                format!("unsupported SOCKS version: {v:#04x}")
            }
            Socks5Error::UnsupportedCommand(c) => {
                format!("unsupported command: {c:#04x}")
            }
            Socks5Error::UnsupportedAddressType(a) => {
                format!("unsupported address type: {a:#04x}")
            }
            Socks5Error::UnsupportedAuthMethod(m) => {
                format!("unsupported authentication method: {m:#04x}")
            }
            _ => self.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        // thiserror displays u8 as decimal, not hex
        assert_eq!(
            Socks5Error::UnsupportedVersion(0x04).to_string(),
            "unsupported SOCKS version: 4"
        );
        assert_eq!(
            Socks5Error::UnsupportedCommand(0x02).to_string(),
            "unsupported command: 2"
        );
        assert_eq!(
            Socks5Error::UnsupportedAddressType(0x05).to_string(),
            "unsupported address type: 5"
        );
        assert_eq!(
            Socks5Error::UnsupportedAuthMethod(0x03).to_string(),
            "unsupported authentication method: 3"
        );
        assert_eq!(Socks5Error::AuthFailed.to_string(), "authentication failed");
        assert_eq!(
            Socks5Error::CredentialsTooLong.to_string(),
            "credentials too long (max 255 bytes)"
        );
        assert_eq!(
            Socks5Error::ConnectionRefused.to_string(),
            "connection refused by SOCKS server"
        );
        assert_eq!(
            Socks5Error::ConnectionFailed("timeout".to_string()).to_string(),
            "connection failed: timeout"
        );
        assert_eq!(
            Socks5Error::MalformedMessage("bad".to_string()).to_string(),
            "malformed message: bad"
        );
        assert_eq!(
            Socks5Error::MethodNegotiationFailed.to_string(),
            "method negotiation failed"
        );
        assert_eq!(
            Socks5Error::UnexpectedEof.to_string(),
            "unexpected end of stream"
        );
        assert_eq!(Socks5Error::AddressTooLong.to_string(), "address too long");
    }

    #[test]
    fn test_error_display_hex() {
        // Test the display_hex method for hex formatting
        assert_eq!(
            Socks5Error::UnsupportedVersion(0x04).display_hex(),
            "unsupported SOCKS version: 0x04"
        );
        assert_eq!(
            Socks5Error::UnsupportedCommand(0x02).display_hex(),
            "unsupported command: 0x02"
        );
        assert_eq!(
            Socks5Error::UnsupportedAddressType(0x05).display_hex(),
            "unsupported address type: 0x05"
        );
        assert_eq!(
            Socks5Error::UnsupportedAuthMethod(0x03).display_hex(),
            "unsupported authentication method: 0x03"
        );
        // Non-hex variants fall back to to_string()
        assert_eq!(
            Socks5Error::AuthFailed.display_hex(),
            "authentication failed"
        );
    }
}
