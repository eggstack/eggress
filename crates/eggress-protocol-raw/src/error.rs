#[derive(Debug, thiserror::Error)]
pub enum RawTunnelError {
    #[error("no target configured for raw tunnel")]
    NoTarget,
    #[error("target connect failed: {0}")]
    TargetConnect(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
