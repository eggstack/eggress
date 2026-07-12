use std::io::Read;

use crate::error::ConfigError;

const MAX_CONFIG_SIZE: usize = 1024 * 1024;

pub fn load_config_file(path: &str) -> Result<String, ConfigError> {
    load_bounded_text_file(path, "config")
}

/// Load a compatibility rules file with the same memory bound as the main
/// configuration file.
pub fn load_rules_file(path: &str) -> Result<String, ConfigError> {
    load_bounded_text_file(path, "rules")
}

fn load_bounded_text_file(path: &str, kind: &str) -> Result<String, ConfigError> {
    let file = std::fs::File::open(path).map_err(|e| ConfigError::Io(e.to_string()))?;

    let metadata = file
        .metadata()
        .map_err(|e| ConfigError::Io(e.to_string()))?;

    if metadata.len() > MAX_CONFIG_SIZE as u64 {
        return Err(ConfigError::Io(format!(
            "{kind} file exceeds maximum size of {MAX_CONFIG_SIZE} bytes"
        )));
    }

    let mut contents = String::new();
    // The metadata check is only an early rejection. Read one extra byte as
    // well so a file that grows after stat(2) cannot bypass the bound.
    file.take((MAX_CONFIG_SIZE + 1) as u64)
        .read_to_string(&mut contents)
        .map_err(|e| ConfigError::Io(e.to_string()))?;

    if contents.len() > MAX_CONFIG_SIZE {
        return Err(ConfigError::Io(format!(
            "{kind} file exceeds maximum size of {} bytes",
            MAX_CONFIG_SIZE
        )));
    }

    Ok(contents)
}
