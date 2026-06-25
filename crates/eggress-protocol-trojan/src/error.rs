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
