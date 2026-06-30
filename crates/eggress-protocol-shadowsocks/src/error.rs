/// Error types for Shadowsocks protocol operations.
#[derive(Debug, thiserror::Error)]
pub enum ShadowsocksError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("unsupported method: {0}")]
    UnsupportedMethod(String),

    #[error("legacy stream cipher '{0}' is not supported; use an AEAD method (aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305)")]
    LegacyMethodUnsupported(String),

    #[error("ShadowsocksR (SSR) is not supported; SSR protocol, obfs, and legacy features are intentionally excluded")]
    SsrUnsupported,

    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("invalid address: {0}")]
    InvalidAddress(String),

    #[error("frame too large")]
    FrameTooLarge,

    #[error("invalid key length")]
    InvalidKeyLength,

    #[error("password too long")]
    PasswordTooLong,

    #[error("{0}")]
    Other(String),
}
