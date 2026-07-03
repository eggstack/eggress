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

/// A non-fatal security warning emitted during config validation.
///
/// Warnings indicate potentially dangerous configurations that should be
/// reviewed by the operator but do not prevent the config from loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigWarning {
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "warning at {}: {}", self.path, self.message)
    }
}
