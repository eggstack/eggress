#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(String),
    #[error("failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("configuration error at {path}: {message}")]
    Validation { path: String, message: String },
    #[error("unsupported config version: {0}")]
    UnsupportedVersion(u32),
}

impl ConfigError {
    pub fn validation(path: &str, message: &str) -> Self {
        ConfigError::Validation {
            path: path.to_string(),
            message: message.to_string(),
        }
    }
}
