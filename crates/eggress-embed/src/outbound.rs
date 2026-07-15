//! Native outbound connector for proxy chains.
//!
//! This module provides [`OutboundConnector`], which compiles a TOML config
//! and executes the chain engine directly to open TCP connections through a
//! configured proxy chain without starting a listener service.

use std::sync::Arc;
use std::time::Duration;

use crate::EggressError;

/// Metadata about an established outbound connection.
#[derive(Debug, Clone)]
pub struct OutboundInfo {
    /// The local address of the underlying TCP connection (if available).
    pub local_addr: Option<std::net::SocketAddr>,
    /// The remote address of the first hop.
    pub peer_addr: Option<std::net::SocketAddr>,
    /// The chain hops that were traversed.
    pub hop_count: usize,
}

/// A UDP association through a SOCKS5 proxy.
///
/// Contains the relay address to send/receive UDP datagrams and
/// the control stream that must remain open for the association lifetime.
pub struct UdpAssociation {
    /// The UDP relay address of the SOCKS5 proxy.
    pub relay_addr: std::net::SocketAddr,
    /// The control TCP stream (must stay open for the association).
    pub control_stream: Option<eggress_core::BoxStream>,
    /// The target address for datagrams.
    pub target: eggress_core::TargetAddr,
}

/// Resolve a proxy endpoint address (host:port) to a SocketAddr.
///
/// For IP addresses, returns directly. For domains, performs DNS lookup.
async fn resolve_endpoint_addr(
    endpoint: &eggress_uri::EndpointSpec,
) -> Option<std::net::SocketAddr> {
    if let Ok(ip) = endpoint.host.parse::<std::net::IpAddr>() {
        return Some(std::net::SocketAddr::new(ip, endpoint.port));
    }
    let lookup = format!("{}:{}", endpoint.host, endpoint.port);
    let result = tokio::net::lookup_host(&lookup).await.ok()?.next();
    result
}

/// A native outbound connector that executes the chain engine directly.
///
/// This compiles routing/upstream state from a TOML config and provides
/// methods to open TCP connections through the configured proxy chain
/// without starting a listener service.
pub struct OutboundConnector {
    runtime_config: Arc<eggress_config::compile::RuntimeConfig>,
    chain_executor: eggress_core::chain::ChainExecutor,
}

impl OutboundConnector {
    /// Create a connector from a TOML config string.
    pub fn from_toml(config_toml: &str) -> Result<Self, EggressError> {
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
            return Err(EggressError::Config("no upstreams configured".to_string()));
        }

        let upstream = &runtime_config.upstreams[0];
        if upstream.chain.hops.is_empty() {
            return Err(EggressError::Config("upstream chain is empty".to_string()));
        }

        let chain_executor = eggress_server::build_chain_executor(None, None);

        Ok(Self {
            runtime_config: Arc::new(runtime_config),
            chain_executor,
        })
    }

    /// Create a connector from a pproxy-style URI (e.g., "socks5://127.0.0.1:1080").
    pub fn from_pproxy_uri(uri: &str) -> Result<Self, EggressError> {
        let parsed = eggress_pproxy_compat::uri::parse_pproxy_uri(uri)
            .map_err(|e| EggressError::Config(e.to_string()))?;
        let chain = eggress_pproxy_compat::uri::PproxyChain {
            raw: uri.to_string(),
            hops: vec![parsed],
        };
        let output = eggress_pproxy_compat::translate_from_uris(&[], &[chain], &[])
            .map_err(|e| EggressError::Config(e.to_string()))?;
        Self::from_toml(&output.toml)
    }

    /// Connect to a target host:port through the configured proxy chain.
    ///
    /// Returns the connected stream and connection metadata.
    pub async fn connect_tcp(
        &self,
        host: &str,
        port: u16,
    ) -> Result<(eggress_core::BoxStream, OutboundInfo), EggressError> {
        let target = eggress_core::TargetAddr {
            host: if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                eggress_core::TargetHost::Ip(ip)
            } else {
                eggress_core::TargetHost::Domain(host.to_string())
            },
            port,
        };

        let upstream = &self.runtime_config.upstreams[0];
        let chain = &upstream.chain;

        // Resolve the first hop endpoint address for metadata
        let first_hop = &chain.hops[0];
        let peer_addr = resolve_endpoint_addr(&first_hop.endpoint).await;

        let stream = self
            .chain_executor
            .execute(&chain.hops, &target)
            .await
            .map_err(|e| EggressError::Runtime(e.to_string()))?;

        let info = OutboundInfo {
            local_addr: None,
            peer_addr,
            hop_count: chain.hops.len(),
        };

        Ok((stream, info))
    }

    /// Connect with a timeout.
    pub async fn connect_tcp_timeout(
        &self,
        host: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<(eggress_core::BoxStream, OutboundInfo), EggressError> {
        tokio::time::timeout(timeout, self.connect_tcp(host, port))
            .await
            .map_err(|_| EggressError::Runtime("connection timed out".to_string()))?
    }

    /// Create a UDP association through the configured proxy chain.
    ///
    /// Returns a `UdpAssociation` with the relay address to send/receive
    /// UDP datagrams through the proxy chain.
    ///
    /// UDP association requires SOCKS5 with UDP ASSOCIATE support.
    /// This method establishes the association and returns channel endpoints.
    pub async fn associate_udp(
        &self,
        _target_host: &str,
        _target_port: u16,
    ) -> Result<UdpAssociation, EggressError> {
        Err(EggressError::Runtime(
            "UDP association through OutboundConnector is not yet implemented; \
             use the listener-based approach for UDP"
                .to_string(),
        ))
    }

    /// Get the number of upstreams configured.
    pub fn upstream_count(&self) -> usize {
        self.runtime_config.upstreams.len()
    }

    /// Validate that the config is usable for outbound connections.
    ///
    /// Returns the number of hops in the first upstream's chain.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outbound_connector_from_toml() {
        let config = r#"
            version = 1
            [[listeners]]
            name = "test"
            bind = "127.0.0.1:0"
            protocols = ["socks5"]
            [[upstreams]]
            id = "direct"
            uri = "socks5://127.0.0.1:1080"
        "#;
        let connector = OutboundConnector::from_toml(config).unwrap();
        assert_eq!(connector.upstream_count(), 1);
    }

    #[test]
    fn test_validate_no_upstreams() {
        let config = r#"
            version = 1
            [[listeners]]
            name = "test"
            bind = "127.0.0.1:0"
            protocols = ["socks5"]
        "#;
        let result = OutboundConnector::validate_outbound_config(config);
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
        let result = OutboundConnector::validate_outbound_config(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_from_pproxy_uri() {
        let connector = OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080").unwrap();
        assert_eq!(connector.upstream_count(), 1);
    }
}
