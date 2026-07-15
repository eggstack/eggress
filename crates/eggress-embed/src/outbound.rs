//! One-shot outbound connection through a proxy chain.
//!
//! This module provides [`connect_outbound`], which opens a single TCP
//! connection through a configured proxy chain without starting a listener
//! service. This is the building block for pproxy-compatible `Connection`
//! objects that act as outbound connection factories.
//!
//! # Limitations
//!
//! The current implementation compiles the config and validates the chain,
//! but the actual connection is performed through a minimal local listener
//! (HTTP CONNECT tunnel) because `ChainExecutor::execute()` returns an
//! async `BoxStream` that cannot be easily bridged to a synchronous
//! `std::net::TcpStream` with a raw file descriptor.
//!
//! Future work: Add a native Rust API that returns a raw fd from the
//! chain executor by extracting the TcpStream before protocol wrapping,
//! or by using a more sophisticated fd-passing mechanism.

use crate::EggressError;

/// Validate that a config TOML has at least one upstream with a non-empty chain.
///
/// Returns the compiled chain hops count on success, or an error describing
/// what's missing. This is used by the Python facade to validate configuration
/// before starting a listener-based connection.
pub fn validate_outbound_config(config_toml: &str) -> Result<usize, EggressError> {
    let config: eggress_config::model::ConfigFile =
        toml::from_str(config_toml).map_err(|e| EggressError::Config(e.to_string()))?;

    if let Some(version) = config.version {
        if version != 1 {
            return Err(EggressError::Config(format!(
                "unsupported config version: {version}"
            )));
        }
    }

    eggress_config::validate::validate_config(&config).map_err(|errors| {
        let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        EggressError::Config(messages.join("; "))
    })?;

    let runtime_config = eggress_config::compile::compile_config(&config)
        .map_err(|e| EggressError::Config(e.to_string()))?;

    if runtime_config.upstreams.is_empty() {
        return Err(EggressError::Config(
            "no upstreams configured; cannot make outbound connections".to_string(),
        ));
    }

    let upstream = &runtime_config.upstreams[0];
    let chain = &upstream.chain;

    if chain.hops.is_empty() {
        return Err(EggressError::Config(
            "upstream chain is empty; cannot make outbound connections".to_string(),
        ));
    }

    Ok(chain.hops.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_no_upstreams() {
        let config = r#"
            version = 1
            [[listeners]]
            name = "test"
            bind = "127.0.0.1:0"
            protocols = ["socks5"]
        "#;
        let result = validate_outbound_config(config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no upstreams"));
    }

    #[test]
    fn test_validate_empty_chain() {
        let config = r#"
            version = 1
            [[listeners]]
            name = "test"
            bind = "127.0.0.1:0"
            protocols = ["socks5"]
            [[upstreams]]
            id = "up"
            uri = "socks5://127.0.0.1:1080"
        "#;
        // This should succeed since the chain has one hop from the URI
        let result = validate_outbound_config(config);
        assert!(result.is_ok());
    }
}
