#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("listener bind error on {addr}: {source}")]
    ListenerBind {
        addr: String,
        source: std::io::Error,
    },
    #[error("admin bind error on {addr}: {source}")]
    AdminBind {
        addr: String,
        source: std::io::Error,
    },
    #[error("runtime error: {0}")]
    Other(String),
}

impl From<eggress_config::ConfigError> for RuntimeError {
    fn from(e: eggress_config::ConfigError) -> Self {
        RuntimeError::Config(e.to_string())
    }
}
