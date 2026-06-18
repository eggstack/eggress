use std::io::Read;

use crate::error::ConfigError;

const MAX_CONFIG_SIZE: usize = 1024 * 1024;

pub fn load_config_file(path: &str) -> Result<String, ConfigError> {
    let mut file = std::fs::File::open(path).map_err(|e| ConfigError::Io(e.to_string()))?;

    let metadata = file
        .metadata()
        .map_err(|e| ConfigError::Io(e.to_string()))?;

    if metadata.len() > MAX_CONFIG_SIZE as u64 {
        return Err(ConfigError::Io(format!(
            "config file exceeds maximum size of {} bytes",
            MAX_CONFIG_SIZE
        )));
    }

    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| ConfigError::Io(e.to_string()))?;

    Ok(contents)
}
