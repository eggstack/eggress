/// Error types for SOCKS4/4a protocol operations.
#[derive(Debug, thiserror::Error)]
pub enum Socks4Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid SOCKS version: expected 0x04, got 0x{0:02x}")]
    InvalidVersion(u8),

    #[error("unsupported command: 0x{0:02x} (only CONNECT is supported)")]
    UnsupportedCommand(u8),

    #[error("connection refused by SOCKS server")]
    ConnectionRefused,

    #[error("connection failed: failed to connect to target")]
    ConnectionFailed,

    #[error("connection failed: no identifying userid accepted")]
    FailedNoIdent,

    #[error("connection failed: different userid than expected")]
    FailedDifferentUser,

    #[error("unknown reply status: {0}")]
    UnknownStatus(u8),

    #[error("user ID exceeds maximum length of 255 bytes")]
    UserIdTooLong,

    #[error("malformed request: {0}")]
    MalformedRequest(String),

    #[error("domain name exceeds maximum length")]
    DomainTooLong,
}
