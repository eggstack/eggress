#[derive(Debug, thiserror::Error)]
pub enum WebSocketError {
    #[error("WebSocket handshake failed: {0}")]
    Handshake(String),
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("message too large: {size} > {max}")]
    MessageTooLarge { size: usize, max: usize },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
