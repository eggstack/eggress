//! Native outbound connector for proxy chains.
//!
//! This module provides [`OutboundConnector`], which executes the chain engine
//! directly to open TCP connections through a configured proxy chain without
//! starting a listener service. It is the listener-free Rust dependency;
//! `eggress-embed` re-exports this API as a full-service facade.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::OutboundError;

/// Maximum outbound UDP payload accepted by the listener-free API.
///
/// Mirrors `UdpLimits::default().max_datagram_size` so direct and SOCKS5
/// paths enforce the same architectural bound as listener UDP.
pub const OUTBOUND_MAX_DATAGRAM_SIZE: usize = 65535;

/// Timeout applied to the SOCKS5 UDP ASSOCIATE handshake for outbound UDP.
#[cfg(feature = "udp")]
const OUTBOUND_UDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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

#[cfg(feature = "pproxy-compat")]
use super::compat::{map_compat_parse_error, map_compat_translate_error, redact_pproxy_expression};
use super::connect_error::{
    map_classified_kind, normalize_protocol_label, ClassifiedFailure, OutboundConnectError,
    OutboundConnectErrorKind, OutboundConnectStage,
};
#[cfg(feature = "udp")]
use super::udp::UdpAssociation;
#[cfg(feature = "udp")]
use super::udp::{
    map_socks5_upstream_error, resolve_udp_target, target_to_socks, wildcard_bind_for_resolved,
};

/// Resolve a proxy endpoint address (host:port) to a SocketAddr.
///
/// For IP addresses, returns directly. For domains, performs DNS lookup.
/// Tuple-form lookup handles bare IPv6 literals (`::1`) without
/// bracketed-string formatting.
async fn resolve_endpoint_addr(
    endpoint: &eggress_uri::EndpointSpec,
) -> Option<std::net::SocketAddr> {
    if let Ok(ip) = endpoint.host.parse::<std::net::IpAddr>() {
        return Some(std::net::SocketAddr::new(ip, endpoint.port));
    }
    let mut addresses = tokio::net::lookup_host((endpoint.host.as_str(), endpoint.port))
        .await
        .ok()?;
    addresses.next()
}

/// Listener-free outbound route owned by the connector.
///
/// `Direct` executes no proxy hops; `Chain` holds the compiled native chain
/// plus the original upstream count for `upstream_count()` semantics. The
/// full service `RuntimeConfig` is never retained: TOML construction extracts
/// the first upstream's chain and drops the compiled service configuration,
/// while pproxy/native construction stores the chain directly.
#[derive(Clone)]
enum OutboundRoute {
    Direct,
    Chain {
        chain: Arc<eggress_uri::ProxyChainSpec>,
        upstream_count: usize,
    },
}

/// A native outbound connector that executes the chain engine directly.
///
/// This executes a compiled native chain (or direct routing) and provides
/// methods to open TCP connections through the configured proxy chain
/// without starting a listener service.
pub struct OutboundConnector {
    route: OutboundRoute,
    chain_executor: eggress_core::chain::ChainExecutor,
    udp_live: Arc<AtomicU64>,
}

#[derive(Clone, Copy)]
enum ExecutorMode {
    Native,
    Direct,
    #[cfg(feature = "pproxy-compat")]
    PproxyCompatibility,
}

fn build_outbound_executor(mode: ExecutorMode) -> eggress_core::chain::ChainExecutor {
    #[cfg(feature = "ssh")]
    let ssh_sessions = match mode {
        ExecutorMode::Native => Some(Arc::new(eggress_transport_ssh::SshSessionCache::new())),
        ExecutorMode::Direct => None,
        #[cfg(feature = "pproxy-compat")]
        ExecutorMode::PproxyCompatibility => Some(Arc::new(
            eggress_transport_ssh::SshSessionCache::new_compatibility(),
        )),
    };

    #[cfg(not(feature = "ssh"))]
    let _ = mode;

    #[cfg(feature = "ssh")]
    {
        crate::build_chain_executor(None, None, ssh_sessions)
    }
    #[cfg(not(feature = "ssh"))]
    {
        crate::build_chain_executor(None, None)
    }
}

/// Adapt the canonical config error into the outbound facade's established
/// message family. Outbound-specific post-compilation checks remain in the
/// constructors below.
#[cfg(feature = "toml")]
fn config_error_message(error: eggress_config::ConfigError) -> String {
    match error {
        eggress_config::ConfigError::Parse(error) => error.to_string(),
        eggress_config::ConfigError::UnsupportedVersion(version) => {
            format!("unsupported config version: {version}")
        }
        eggress_config::ConfigError::Validation { message, .. } => message,
        eggress_config::ConfigError::Io(message) => message,
    }
}

/// Delegate TOML parsing, version checking, validation, and compilation to
/// the canonical `eggress-config` boundary.
#[cfg(feature = "toml")]
fn parse_validate_compile(input: &str) -> Result<eggress_config::compile::RuntimeConfig, String> {
    eggress_config::validate_and_compile_toml(input).map_err(config_error_message)
}

impl OutboundConnector {
    /// Create a connector from a TOML config string.
    ///
    /// TOML parsing, version checking, validation, and compilation go
    /// through the canonical `eggress-config` boundary; only outbound-specific
    /// post-compilation checks live here. The first upstream's chain is
    /// extracted into the outbound-owned route and the full compiled service
    /// configuration is dropped.
    #[cfg(feature = "toml")]
    pub fn from_toml(config_toml: &str) -> Result<Self, OutboundError> {
        let runtime_config = parse_validate_compile(config_toml).map_err(OutboundError::Config)?;

        if runtime_config.upstreams.is_empty() {
            return Err(OutboundError::Config("no upstreams configured".to_string()));
        }

        let upstream_count = runtime_config.upstreams.len();
        let chain = runtime_config.upstreams[0].chain.clone();
        if chain.hops.is_empty() {
            return Err(OutboundError::Config("upstream chain is empty".to_string()));
        }

        Ok(Self {
            route: OutboundRoute::Chain {
                chain: Arc::new(chain),
                upstream_count,
            },
            chain_executor: build_outbound_executor(ExecutorMode::Native),
            udp_live: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Create a connector from an already-compiled native chain.
    ///
    /// General-purpose constructor for Rust consumers that already hold a
    /// compiled Eggress chain: no TOML and no pproxy compatibility required,
    /// and no server/runtime types cross the boundary. Empty chains are
    /// rejected with the same chain-validation rules; invalid chains are
    /// never silently turned into direct routes.
    pub fn from_chain(chain: eggress_uri::ProxyChainSpec) -> Result<Self, OutboundError> {
        if chain.hops.is_empty() {
            return Err(OutboundError::Config("upstream chain is empty".to_string()));
        }
        Ok(Self {
            route: OutboundRoute::Chain {
                chain: Arc::new(chain),
                upstream_count: 1,
            },
            chain_executor: build_outbound_executor(ExecutorMode::Native),
            udp_live: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Create a direct connector with no proxy hops.
    ///
    /// Explicit alternative to encoding a fake chain: connections open
    /// directly to the requested target.
    pub fn direct() -> Self {
        Self {
            route: OutboundRoute::Direct,
            chain_executor: build_outbound_executor(ExecutorMode::Direct),
            udp_live: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a connector from a pproxy-style remote expression.
    ///
    /// Accepts a single pproxy URI or a canonical `__`-separated multi-hop
    /// chain (e.g. `"socks5://127.0.0.1:1080__http://127.0.0.1:8080"`).
    /// The expression is parsed with the compatibility chain parser and
    /// translated through the existing compatibility layer, then executed
    /// in-process via `ChainExecutor`. No listener is started.
    /// Unsupported chain members fail construction instead of being dropped.
    #[cfg(feature = "pproxy-compat")]
    pub fn from_pproxy_uri(uri: &str) -> Result<Self, OutboundError> {
        let redacted_expr = redact_pproxy_expression(uri);
        let chain = eggress_pproxy_compat::uri::parse_pproxy_chain(uri)
            .map_err(|e| map_compat_parse_error(uri, &redacted_expr, e))?;
        if chain.hops.len() == 1 && chain.hops[0].scheme == "direct" {
            return Ok(Self::direct());
        }
        // Direct native compilation: typed `PproxyChain` -> native
        // `ProxyChainSpec` with no TOML serialize/parse round trip. Validation
        // (backward, unsupported hops, plugins, local-bind, schemes) lives in
        // the compatibility crate so direct and TOML paths agree.
        let native_chain = eggress_pproxy_compat::translate::compile_chain_to_native(&chain)
            .map_err(|e| map_compat_translate_error(&chain, uri, &redacted_expr, e))?;
        if native_chain.hops.is_empty() {
            return Err(OutboundError::Config(format!(
                "pproxy chain '{}' produced an empty native chain",
                chain.redacted_display()
            )));
        }
        Ok(Self {
            route: OutboundRoute::Chain {
                chain: Arc::new(native_chain),
                upstream_count: 1,
            },
            chain_executor: build_outbound_executor(ExecutorMode::PproxyCompatibility),
            udp_live: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Connect to a target host:port through the configured proxy chain.
    ///
    /// Returns the connected stream and connection metadata.
    ///
    /// Compatibility surface: failures remain `OutboundError::Runtime` with
    /// the established message shape. New consumers that need stable failure
    /// categories without parsing strings should use
    /// [`OutboundConnector::connect_tcp_detailed`].
    pub async fn connect_tcp(
        &self,
        host: &str,
        port: u16,
    ) -> Result<(eggress_core::BoxStream, OutboundInfo), OutboundError> {
        self.connect_tcp_inner(host, port)
            .await
            .map_err(|failure| OutboundError::Runtime(failure.compat_message))
    }

    /// Connect with typed failure details.
    ///
    /// Opt-in detailed surface over the same single route construction and
    /// chain execution as [`OutboundConnector::connect_tcp`]: the returned
    /// [`OutboundConnectError`] exposes stable `kind`/`stage`/hop/protocol
    /// facts for embedding/routing policy. Kind, stage, hop, and protocol
    /// are diagnostic facts, not retry recommendations; callers decide
    /// retry/backoff policy and no direct fallback occurs on proxy failure.
    pub async fn connect_tcp_detailed(
        &self,
        host: &str,
        port: u16,
    ) -> Result<(eggress_core::BoxStream, OutboundInfo), OutboundConnectError> {
        self.connect_tcp_inner(host, port)
            .await
            .map_err(|failure| failure.error)
    }

    /// Connect with a timeout.
    ///
    /// Compatibility surface: the outer deadline remains
    /// `OutboundError::Runtime("connection timed out")`. See
    /// [`OutboundConnector::connect_tcp_timeout_detailed`] for the typed
    /// variant that distinguishes the caller deadline (`Deadline` stage)
    /// from underlying transport timeouts.
    pub async fn connect_tcp_timeout(
        &self,
        host: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<(eggress_core::BoxStream, OutboundInfo), OutboundError> {
        match tokio::time::timeout(timeout, self.connect_tcp_inner(host, port)).await {
            Err(_) => Err(OutboundError::Runtime("connection timed out".to_string())),
            Ok(inner) => inner.map_err(|failure| OutboundError::Runtime(failure.compat_message)),
        }
    }

    /// Connect with a timeout and typed failure details.
    ///
    /// The caller-supplied outer deadline maps to
    /// `kind=Timeout, stage=Deadline`; underlying direct, hop-connect, or
    /// handshake timeouts keep their own stage. Cancellation remains
    /// cancellation by future drop and is never synthesized into an error.
    pub async fn connect_tcp_timeout_detailed(
        &self,
        host: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<(eggress_core::BoxStream, OutboundInfo), OutboundConnectError> {
        match tokio::time::timeout(timeout, self.connect_tcp_inner(host, port)).await {
            Err(_) => Err(OutboundConnectError::new(
                OutboundConnectErrorKind::Timeout,
                OutboundConnectStage::Deadline,
                None,
                None,
            )),
            Ok(inner) => inner.map_err(|failure| failure.error),
        }
    }

    /// Single internal connection implementation preserving typed sources.
    ///
    /// Both legacy (`connect_tcp`) and detailed (`connect_tcp_detailed`)
    /// surfaces execute this routine exactly once per call; it retains enough
    /// structure to produce the typed error and the legacy compatibility
    /// string from the same failure. No second chain executor or duplicated
    /// route selection exists.
    async fn connect_tcp_inner(
        &self,
        host: &str,
        port: u16,
    ) -> Result<(eggress_core::BoxStream, OutboundInfo), ClassifiedFailure> {
        fn direct_failure(source: eggress_core::ConnectError) -> ClassifiedFailure {
            let compat_message = source.to_string();
            let kind = map_classified_kind(crate::classify::classify_connect_error(&source));
            ClassifiedFailure {
                error: OutboundConnectError::new(
                    kind,
                    OutboundConnectStage::DirectConnect,
                    None,
                    None,
                ),
                compat_message,
            }
        }

        fn chain_failure(error: eggress_core::chain::ChainError) -> ClassifiedFailure {
            let compat_message = error.to_string();
            let detailed = match &error {
                eggress_core::chain::ChainError::ConnectFailed {
                    hop_index, source, ..
                } => {
                    let kind = map_classified_kind(crate::classify::classify_connect_error(source));
                    OutboundConnectError::new(
                        kind,
                        OutboundConnectStage::HopConnect,
                        Some(*hop_index),
                        None,
                    )
                }
                eggress_core::chain::ChainError::HandshakeFailed {
                    hop_index,
                    protocol,
                    source,
                } => {
                    let mut kind =
                        map_classified_kind(crate::classify::classify_handshake_source(&**source));
                    // Explicit TLS wrapping stage carries TLS provenance even
                    // when the boxed source is generic.
                    if protocol.eq_ignore_ascii_case("tls")
                        && matches!(
                            kind,
                            OutboundConnectErrorKind::Protocol | OutboundConnectErrorKind::Other
                        )
                    {
                        kind = OutboundConnectErrorKind::Tls;
                    }
                    OutboundConnectError::new(
                        kind,
                        OutboundConnectStage::HopHandshake,
                        Some(*hop_index),
                        Some(normalize_protocol_label(protocol)),
                    )
                }
                eggress_core::chain::ChainError::EmptyChain
                | eggress_core::chain::ChainError::InvalidChain { .. } => {
                    OutboundConnectError::new(
                        OutboundConnectErrorKind::Policy,
                        OutboundConnectStage::DirectConnect,
                        None,
                        None,
                    )
                }
            };
            ClassifiedFailure {
                error: detailed,
                compat_message,
            }
        }

        let target = eggress_core::TargetAddr {
            host: if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                eggress_core::TargetHost::Ip(ip)
            } else {
                eggress_core::TargetHost::Domain(host.to_string())
            },
            port,
        };

        let chain = match &self.route {
            OutboundRoute::Direct => {
                let stream = eggress_core::connector::DirectConnector
                    .connect_with_options(
                        &target,
                        &eggress_core::connector::ConnectOptions::default(),
                    )
                    .await
                    .map_err(direct_failure)?;
                return Ok((
                    stream,
                    OutboundInfo {
                        local_addr: None,
                        peer_addr: None,
                        hop_count: 0,
                    },
                ));
            }
            OutboundRoute::Chain { chain, .. } => chain.clone(),
        };

        // Resolve the first hop endpoint address for metadata
        let first_hop = &chain.hops[0];
        let peer_addr = resolve_endpoint_addr(&first_hop.endpoint).await;

        let stream = self
            .chain_executor
            .execute(&chain.hops, &target)
            .await
            .map_err(chain_failure)?;

        let info = OutboundInfo {
            local_addr: None,
            peer_addr,
            hop_count: chain.hops.len(),
        };

        Ok((stream, info))
    }

    /// Create a listener-free UDP association for a fixed target.
    ///
    /// Fixed-target (connected) semantics: `target_host`/`target_port` select
    /// the destination once. Use [`UdpAssociation::send`] and
    /// [`UdpAssociation::recv`] for payload bytes. No hidden local listener
    /// is started; the association is built directly over the existing UDP
    /// routing/upstream primitives.
    ///
    /// Supported compositions in this surface:
    /// - direct routing (`direct://` connectors, or direct fallback);
    /// - single-hop SOCKS5 UDP relay (the UDP path implemented by
    ///   `eggress-udp::upstream_socks5`).
    ///
    /// Composed multi-hop and Shadowsocks UDP chains are explicitly
    /// unsupported in the listener-free surface and fail with a structured
    /// [`OutboundError::UnsupportedFeature`] rather than silent direct
    /// fallback. Private and loopback targets are allowed because the caller
    /// explicitly selected the destination.
    #[cfg(feature = "udp")]
    pub async fn associate_udp(
        &self,
        target_host: &str,
        target_port: u16,
    ) -> Result<UdpAssociation, OutboundError> {
        let target_socks = target_to_socks(target_host, target_port)?;

        let chain = match &self.route {
            OutboundRoute::Direct => {
                let resolved = resolve_udp_target(&target_socks, target_host, target_port).await?;
                // Family-aware wildcard bind: `0.0.0.0:0` for IPv4, `[::]:0`
                // for IPv6. Never loopback-bound; the destination is
                // caller-selected.
                let bind = wildcard_bind_for_resolved(&resolved);
                let socket = tokio::net::UdpSocket::bind(bind)
                    .await
                    .map_err(|e| OutboundError::Runtime(format!("udp bind failed: {e}")))?;
                socket
                    .connect(resolved)
                    .await
                    .map_err(|e| OutboundError::Runtime(format!("udp connect failed: {e}")))?;
                return Ok(UdpAssociation::new_direct(
                    socket,
                    target_socks,
                    self.udp_live.clone(),
                ));
            }
            OutboundRoute::Chain { chain, .. } => chain.clone(),
        };

        match eggress_udp::udp_capability(&chain) {
            eggress_udp::UdpRelayCapability::SupportedSocks5 => {
                let hop = chain.hops.first().ok_or_else(|| {
                    OutboundError::Config("upstream chain is empty".to_string())
                })?;
                // Unspecified bind hint matching the proxy endpoint family
                // where known (IPv6 literals get `[::]:0`, otherwise
                // `0.0.0.0:0`). The upstream primitive family-corrects
                // against the negotiated relay address, so domain relays
                // resolving to IPv6 are still handled without a hard-coded
                // IPv4 loopback failure.
                let udp_bind: std::net::SocketAddr =
                    if hop.endpoint.host.parse::<std::net::Ipv6Addr>().is_ok() {
                        "[::]:0".parse().expect("ipv6 wildcard parses")
                    } else {
                        "0.0.0.0:0".parse().expect("ipv4 wildcard parses")
                    };
                let assoc = eggress_udp::upstream_socks5::open_socks5_udp_upstream(
                    eggress_udp::upstream_socks5::Socks5UdpUpstreamConfig {
                        upstream_id: eggress_core::UpstreamId::new("outbound-upstream-0"),
                        hop: hop.clone(),
                        connect_timeout: OUTBOUND_UDP_CONNECT_TIMEOUT,
                        udp_bind,
                    },
                    Some(target_socks.clone()),
                )
                .await
                .map_err(map_socks5_upstream_error)?;
                Ok(UdpAssociation::new_socks5(
                    assoc.udp_socket,
                    assoc.relay_addr,
                    target_socks,
                    assoc.control_cancel,
                    assoc.control_task,
                    self.udp_live.clone(),
                ))
            }
            eggress_udp::UdpRelayCapability::SupportedComposed => Err(OutboundError::UnsupportedFeature {
                feature: "composed-udp".to_string(),
                message: "listener-free UDP supports direct and single-hop SOCKS5 only; composed multi-hop UDP chains are not supported in OutboundConnector::associate_udp".to_string(),
            }),
            eggress_udp::UdpRelayCapability::UnsupportedProtocol { protocol } => {
                Err(OutboundError::UnsupportedFeature {
                    feature: protocol.clone(),
                    message: format!(
                        "upstream protocol '{protocol}' cannot carry UDP in OutboundConnector::associate_udp; use direct or single-hop SOCKS5"
                    ),
                })
            }
            eggress_udp::UdpRelayCapability::UnsupportedMultiHop => {
                Err(OutboundError::UnsupportedFeature {
                    feature: "multi-hop".to_string(),
                    message: "listener-free UDP supports direct and single-hop SOCKS5 only; multi-hop UDP chains are not supported in OutboundConnector::associate_udp".to_string(),
                })
            }
            // Covers Shadowsocks UDP (feature-gated in eggress-udp) and any
            // future UDP-capable modes: explicitly unsupported in the
            // listener-free surface rather than silent fallback. Reachable
            // only when the `eggress-udp/shadowsocks` capability is unified
            // in (e.g. via `eggress-server/extended`); lean `udp` builds
            // without it legitimately never reach this arm.
            #[allow(unreachable_patterns)]
            _ => Err(OutboundError::UnsupportedFeature {
                feature: "udp-upstream".to_string(),
                message: "listener-free UDP supports direct and single-hop SOCKS5 only; this UDP-capable upstream mode is not supported in OutboundConnector::associate_udp".to_string(),
            }),
        }
    }

    /// Create a UDP association with a timeout on establishment.
    ///
    /// The timeout covers DNS resolution, TCP control handshake, and SOCKS5
    /// UDP ASSOCIATE. A timeout does not leak the partially built socket or
    /// control task.
    #[cfg(feature = "udp")]
    pub async fn associate_udp_timeout(
        &self,
        target_host: &str,
        target_port: u16,
        timeout: Duration,
    ) -> Result<UdpAssociation, OutboundError> {
        tokio::time::timeout(timeout, self.associate_udp(target_host, target_port))
            .await
            .map_err(|_| OutboundError::Runtime("udp association timed out".to_string()))?
    }

    /// Number of live listener-free UDP associations from this connector.
    ///
    /// Incremented on successful [`OutboundConnector::associate_udp`] and
    /// decremented exactly once on close/drop. Used to verify lifecycle
    /// cleanup without a listener registry.
    pub fn active_udp_associations(&self) -> u64 {
        self.udp_live.load(Ordering::Relaxed)
    }

    /// Get the number of upstreams configured.
    pub fn upstream_count(&self) -> usize {
        match &self.route {
            OutboundRoute::Direct => 0,
            OutboundRoute::Chain { upstream_count, .. } => *upstream_count,
        }
    }

    /// Number of proxy hops in the configured chain (0 for direct).
    pub fn hop_count(&self) -> usize {
        match &self.route {
            OutboundRoute::Direct => 0,
            OutboundRoute::Chain { chain, .. } => chain.hops.len(),
        }
    }

    /// Validate that the config is usable for outbound connections.
    ///
    /// Returns the number of hops in the first upstream's chain.
    ///
    /// Parsing/validation/compilation goes through the canonical
    /// `eggress-config` TOML boundary exactly once.
    #[cfg(feature = "toml")]
    pub fn validate_outbound_config(config_toml: &str) -> Result<usize, OutboundError> {
        let runtime_config = parse_validate_compile(config_toml).map_err(OutboundError::Config)?;

        if runtime_config.upstreams.is_empty() {
            return Err(OutboundError::Config(
                "no upstreams configured; cannot make outbound connections".to_string(),
            ));
        }

        let upstream = &runtime_config.upstreams[0];
        let chain = &upstream.chain;

        if chain.hops.is_empty() {
            return Err(OutboundError::Config(
                "upstream chain is empty; cannot make outbound connections".to_string(),
            ));
        }

        Ok(chain.hops.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain_hops(connector: &OutboundConnector) -> Vec<eggress_uri::ProxyHopSpec> {
        match &connector.route {
            OutboundRoute::Direct => Vec::new(),
            OutboundRoute::Chain { chain, .. } => chain.hops.clone(),
        }
    }

    #[test]
    fn from_chain_rejects_empty() {
        let empty = eggress_uri::ProxyChainSpec { hops: Vec::new() };
        let err = match OutboundConnector::from_chain(empty) {
            Err(e) => e,
            Ok(_) => panic!("empty chain must fail"),
        };
        assert!(
            matches!(err, OutboundError::Config(_)),
            "empty chain must stay Config, got {err:?}"
        );
        assert!(
            err.to_string().contains("upstream chain is empty"),
            "empty chain message changed: {err}"
        );
    }

    #[test]
    fn from_chain_single_hop_constructs() {
        let chain = eggress_uri::parse_proxy_chain("socks5://127.0.0.1:1080").unwrap();
        let connector = OutboundConnector::from_chain(chain).unwrap();
        assert_eq!(connector.upstream_count(), 1);
        assert_eq!(connector.hop_count(), 1);
        let hops = chain_hops(&connector);
        assert_eq!(hops.len(), 1);
        assert!(
            hops[0]
                .protocols
                .contains(&eggress_uri::ProtocolSpec::Socks5),
            "expected SOCKS5 hop, got {:?}",
            hops[0].protocols
        );
    }

    #[test]
    fn direct_connector_has_no_hops() {
        let connector = OutboundConnector::direct();
        assert_eq!(connector.upstream_count(), 0);
        assert_eq!(connector.hop_count(), 0);
    }

    #[cfg(feature = "toml")]
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
        assert_eq!(connector.hop_count(), 1);
    }

    #[cfg(feature = "toml")]
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

    #[cfg(feature = "toml")]
    #[test]
    fn test_validate_nonempty_chain_ok() {
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

    #[cfg(feature = "toml")]
    fn toml_with_version(version: u32) -> String {
        format!(
            r#"
            version = {version}
            [[upstreams]]
            id = "up"
            uri = "socks5://127.0.0.1:1080"
        "#
        )
    }

    #[cfg(feature = "toml")]
    fn toml_with_uri(uri: &str) -> String {
        format!(
            r#"
            version = 1
            [[upstreams]]
            id = "up"
            uri = "{uri}"
        "#
        )
    }

    #[cfg(feature = "toml")]
    #[test]
    fn from_toml_unsupported_version_is_config() {
        let input = toml_with_version(99);
        let err = match OutboundConnector::from_toml(&input) {
            Err(e) => e,
            Ok(_) => panic!("version 99 must fail"),
        };
        match &err {
            OutboundError::Config(msg) => {
                assert!(
                    msg.contains("unsupported config version"),
                    "version error family changed: {msg}"
                );
            }
            other => panic!("version mismatch must be Config, got {other:?}"),
        }
    }

    #[cfg(feature = "toml")]
    #[test]
    fn from_toml_malformed_is_config() {
        let input = "not valid toml {{{";
        let err = match OutboundConnector::from_toml(input) {
            Err(e) => e,
            Ok(_) => panic!("malformed TOML must fail"),
        };
        assert!(
            matches!(err, OutboundError::Config(_)),
            "malformed TOML must be Config, got {err:?}"
        );
    }

    #[cfg(feature = "toml")]
    #[test]
    fn from_toml_invalid_uri_is_config() {
        // Passes TOML parsing but fails `validate_config` (invalid upstream
        // URI); must classify as `Config`.
        let input = toml_with_uri("://bad-uri");
        let err = match OutboundConnector::from_toml(&input) {
            Err(e) => e,
            Ok(_) => panic!("invalid URI must fail"),
        };
        assert!(
            matches!(err, OutboundError::Config(_)),
            "validation failure must be Config, got {err:?}"
        );
    }

    #[cfg(feature = "toml")]
    #[test]
    fn from_toml_no_upstreams_outbound_specific() {
        // Upstream-free configs pass shared validation but fail the
        // outbound-only post-compilation check.
        let input = r#"
            version = 1
            [[listeners]]
            name = "test"
            bind = "127.0.0.1:0"
            protocols = ["socks5"]
        "#;
        let err = match OutboundConnector::from_toml(input) {
            Err(e) => e,
            Ok(_) => panic!("upstream-free config must fail for outbound"),
        };
        assert!(
            err.to_string().contains("no upstreams configured"),
            "outbound-specific condition changed: {err}"
        );
        let validate_err = OutboundConnector::validate_outbound_config(input).unwrap_err();
        assert!(
            validate_err.to_string().contains("no upstreams"),
            "validate_outbound_config condition changed: {validate_err}"
        );
    }

    #[cfg(feature = "toml")]
    #[test]
    fn from_toml_empty_chain_branch_preserved() {
        // Empty chains are not constructible through TOML fixtures:
        // `parse_proxy_chain("")` fails before compilation, so any
        // upstream URI reaching compilation yields at least one hop.
        // This pins that valid configs never hit the defensive
        // `upstream chain is empty` branch while the branch itself
        // remains explicit in `from_toml`.
        let input = toml_with_uri("socks5://127.0.0.1:1080");
        let connector = OutboundConnector::from_toml(&input).unwrap();
        assert_eq!(connector.upstream_count(), 1);
        // An empty URI fails shared validation, not the empty-chain
        // branch, proving the branch is unreachable via TOML but preserved.
        let empty_uri = toml_with_uri("");
        let err = match OutboundConnector::from_toml(&empty_uri) {
            Err(e) => e,
            Ok(_) => panic!("empty URI must fail"),
        };
        assert!(
            matches!(err, OutboundError::Config(_)),
            "empty URI must stay Config, got {err:?}"
        );
        assert!(
            !err.to_string().contains("upstream chain is empty"),
            "empty URI must fail in shared validation, not empty-chain branch: {err}"
        );
    }

    #[cfg(feature = "toml")]
    #[test]
    fn from_toml_supported_http_socks_tls() {
        for uri in [
            "socks5://127.0.0.1:1080",
            "http://127.0.0.1:8080",
            "socks5+tls://127.0.0.1:1080",
        ] {
            let input = toml_with_uri(uri);
            let connector = OutboundConnector::from_toml(&input)
                .unwrap_or_else(|e| panic!("supported uri {uri} must construct: {e}"));
            assert_eq!(connector.upstream_count(), 1);
        }
    }

    #[cfg(feature = "toml")]
    #[test]
    fn from_toml_records_all_upstreams_in_count() {
        let input = r#"
            version = 1
            [[upstreams]]
            id = "one"
            uri = "socks5://127.0.0.1:1080"
            [[upstreams]]
            id = "two"
            uri = "http://127.0.0.1:8080"
        "#;
        let connector = OutboundConnector::from_toml(input).unwrap();
        // The connector executes the first upstream's chain but preserves
        // the original upstream count.
        assert_eq!(connector.upstream_count(), 2);
        assert_eq!(connector.hop_count(), 1);
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri() {
        let connector = OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080").unwrap();
        assert_eq!(connector.upstream_count(), 1);
        assert_eq!(connector.hop_count(), 1);
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_single_http() {
        let connector =
            OutboundConnector::from_pproxy_uri("http://127.0.0.1:8080").expect("single HTTP");
        assert_eq!(connector.upstream_count(), 1);
        let hops = chain_hops(&connector);
        assert_eq!(hops.len(), 1);
        assert!(
            hops[0].protocols.contains(&eggress_uri::ProtocolSpec::Http),
            "expected HTTP hop, got {:?}",
            hops[0].protocols
        );
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_two_hop_chain() {
        let connector =
            OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080__http://127.0.0.1:8080")
                .expect("two-hop chain should construct");
        assert_eq!(connector.upstream_count(), 1);
        let hops = chain_hops(&connector);
        assert_eq!(hops.len(), 2, "expected two ordered hops, got {hops:?}");
        assert!(
            hops[0]
                .protocols
                .contains(&eggress_uri::ProtocolSpec::Socks5),
            "hop 0 should be SOCKS5, got {:?}",
            hops[0].protocols
        );
        assert!(
            hops[1].protocols.contains(&eggress_uri::ProtocolSpec::Http),
            "hop 1 should be HTTP, got {:?}",
            hops[1].protocols
        );
        assert_eq!(connector.hop_count(), 2);
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_three_hop_chain() {
        let connector = OutboundConnector::from_pproxy_uri(
            "socks5://127.0.0.1:1080__http://127.0.0.1:8080__socks4://127.0.0.1:1081",
        )
        .expect("three-hop chain should construct");
        let hops = chain_hops(&connector);
        assert_eq!(hops.len(), 3);
        assert!(hops[0]
            .protocols
            .contains(&eggress_uri::ProtocolSpec::Socks5));
        assert!(hops[1].protocols.contains(&eggress_uri::ProtocolSpec::Http));
        assert!(hops[2]
            .protocols
            .contains(&eggress_uri::ProtocolSpec::Socks4));
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_direct_fast_path() {
        let connector = OutboundConnector::from_pproxy_uri("direct://").expect("direct://");
        assert!(matches!(connector.route, OutboundRoute::Direct));
        assert_eq!(connector.upstream_count(), 0);
        assert_eq!(connector.hop_count(), 0);
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_malformed_chain_rejected() {
        for uri in ["socks5://127.0.0.1:1080__", "__socks5://127.0.0.1:1080"] {
            let result = OutboundConnector::from_pproxy_uri(uri);
            assert!(result.is_err(), "malformed chain should fail: {uri}");
        }
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_multihop_direct_not_collapsed() {
        let result = OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080__direct://");
        assert!(result.is_err(), "multi-hop direct must fail closed");
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_unsupported_hop_fails_closed() {
        let result =
            OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080__redir://127.0.0.1:1234");
        assert!(result.is_err(), "unsupported hop must fail closed");
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_valid_credentialed_chain_keeps_credentials() {
        let connector = OutboundConnector::from_pproxy_uri(
            "socks5://user1:pass1@127.0.0.1:1080__http://user2:pass2@127.0.0.1:8080",
        )
        .expect("credentialed chain should construct");
        let hops = chain_hops(&connector);
        assert_eq!(hops.len(), 2);
        let first = hops[0].credentials.as_ref().expect("hop 0 credentials");
        assert_eq!(first.username, "user1");
        assert_eq!(first.password, "pass1");
        let second = hops[1].credentials.as_ref().expect("hop 1 credentials");
        assert_eq!(second.username, "user2");
        assert_eq!(second.password, "pass2");
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_malformed_chain_redacts_credentials() {
        let uri =
            "socks5://user_a:secret_a@127.0.0.1:1080__http://user_b:secret_b@127.0.0.1:8080__";
        let err = match OutboundConnector::from_pproxy_uri(uri) {
            Ok(_) => panic!("trailing __ must fail"),
            Err(err) => err,
        };
        let rendered = format!("{err:?} {err}");
        for secret in ["user_a", "secret_a", "user_b", "secret_b"] {
            assert!(
                !rendered.contains(secret),
                "error leaked {secret:?}: {rendered}"
            );
        }
    }

    #[cfg(feature = "udp")]
    async fn start_raw_udp_echo() -> std::net::SocketAddr {
        eggress_udp::testkit::start_udp_echo_server().await
    }

    #[cfg(feature = "udp")]
    #[test]
    fn wildcard_bind_matches_resolved_family() {
        let v4: std::net::SocketAddr = "192.0.2.1:53".parse().unwrap();
        let v6: std::net::SocketAddr = "[2001:db8::1]:53".parse().unwrap();
        assert!(wildcard_bind_for_resolved(&v4).is_ipv4());
        assert!(wildcard_bind_for_resolved(&v6).is_ipv6());
        // Direct outbound never uses loopback binds.
        assert!(!wildcard_bind_for_resolved(&v4).ip().is_loopback());
        assert!(!wildcard_bind_for_resolved(&v6).ip().is_loopback());
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_direct_echo_round_trip() {
        let echo = start_raw_udp_echo().await;
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        let assoc = connector
            .associate_udp("127.0.0.1", echo.port())
            .await
            .unwrap();
        assoc.send(b"hello udp").await.unwrap();
        let mut buf = [0u8; 65535];
        let n = assoc.recv(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello udp");
        // Target metadata preserved for fixed-target association.
        let target = assoc.target();
        assert_eq!(target.port, echo.port());
        assoc.close();
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_direct_multiple_datagrams() {
        let echo = start_raw_udp_echo().await;
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        let assoc = connector
            .associate_udp("127.0.0.1", echo.port())
            .await
            .unwrap();
        for i in 0..5 {
            let msg = format!("packet {i}");
            assoc.send(msg.as_bytes()).await.unwrap();
            let mut buf = [0u8; 65535];
            let n = assoc.recv(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], msg.as_bytes());
        }
        assoc.close();
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_direct_ipv6_echo_round_trip() {
        let Some(echo) = eggress_udp::testkit::try_start_udp_echo_server_ipv6().await else {
            eprintln!("SKIP outbound_udp_direct_ipv6_echo_round_trip: IPv6 loopback unavailable");
            return;
        };
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        assert_eq!(connector.active_udp_associations(), 0);
        let assoc = connector.associate_udp("::1", echo.port()).await.unwrap();
        // Family-compatible local bind: IPv6 destination must yield an
        // IPv6 local socket, not an IPv4 loopback socket.
        let local = assoc.local_addr().expect("direct IPv6 has local addr");
        assert!(
            local.is_ipv6(),
            "IPv6 target must use IPv6 local bind, got {local}"
        );
        assert_eq!(connector.active_udp_associations(), 1);
        assoc.send(b"hello udp6").await.unwrap();
        let mut buf = [0u8; 65535];
        let n = tokio::time::timeout(Duration::from_secs(5), assoc.recv(&mut buf))
            .await
            .expect("ipv6 echo recv timed out")
            .unwrap();
        assert_eq!(&buf[..n], b"hello udp6");
        let target = assoc.target();
        assert_eq!(target.port, echo.port());
        assoc.close();
        assert!(assoc.is_closed());
        assert_eq!(connector.active_udp_associations(), 0);
        // Idempotent close preserves exactly-once accounting for IPv6.
        assoc.close();
        assert_eq!(connector.active_udp_associations(), 0);
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_direct_ipv4_uses_wildcard_family_bind() {
        let echo = start_raw_udp_echo().await;
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        let assoc = connector
            .associate_udp("127.0.0.1", echo.port())
            .await
            .unwrap();
        let local = assoc.local_addr().expect("direct IPv4 has local addr");
        assert!(
            local.is_ipv4(),
            "IPv4 target must use IPv4 local bind, got {local}"
        );
        assoc.close();
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_socks5_ipv6_relay_round_trip() {
        use eggress_udp::testkit::{Socks5TestMode, Socks5TestServerConfig, Socks5UdpTestServer};
        let server = match Socks5UdpTestServer::start_ipv6(Socks5TestServerConfig {
            mode: Socks5TestMode::Echo,
            relay_addr: None,
        })
        .await
        {
            Ok(server) => server,
            Err(e) => {
                eprintln!(
                    "SKIP outbound_udp_socks5_ipv6_relay_round_trip: IPv6 loopback unavailable: {e}"
                );
                return;
            }
        };
        assert!(
            server.tcp_addr.is_ipv6(),
            "ipv6 test server must listen on IPv6, got {}",
            server.tcp_addr
        );
        let uri = format!("socks5://{}", server.tcp_addr);
        let connector = OutboundConnector::from_pproxy_uri(&uri).unwrap();
        let assoc = connector.associate_udp("127.0.0.1", 53).await.unwrap();
        // Relay is IPv6: local UDP socket must be IPv6 for send_to to work.
        let relay = assoc.relay_addr().expect("socks5 has relay addr");
        assert!(relay.is_ipv6(), "IPv6 relay expected, got {relay}");
        let local = assoc
            .local_addr()
            .expect("socks5 ipv6 relay has local addr");
        assert!(
            local.is_ipv6(),
            "IPv6 relay must use IPv6 local bind, got {local}"
        );
        // No silent fallback to direct: relay addr present proves the
        // configured SOCKS5 upstream was selected.
        assoc.send(b"via socks5 ipv6").await.unwrap();
        let mut buf = [0u8; 65535];
        let n = tokio::time::timeout(Duration::from_secs(5), assoc.recv(&mut buf))
            .await
            .expect("ipv6 relay echo timed out")
            .unwrap();
        assert_eq!(&buf[..n], b"via socks5 ipv6");
        assoc.close();
        assert_eq!(connector.active_udp_associations(), 0);
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_socks5_upstream_round_trip() {
        use eggress_udp::testkit::{Socks5TestMode, Socks5TestServerConfig, Socks5UdpTestServer};
        let server = Socks5UdpTestServer::start(Socks5TestServerConfig {
            mode: Socks5TestMode::Echo,
            relay_addr: None,
        })
        .await
        .unwrap();
        let echo = start_raw_udp_echo().await;
        let uri = format!("socks5://{}", server.tcp_addr);
        let connector = OutboundConnector::from_pproxy_uri(&uri).unwrap();
        let assoc = connector
            .associate_udp("127.0.0.1", echo.port())
            .await
            .unwrap();
        // Routing selected the configured SOCKS5 upstream: relay addr present.
        assert!(assoc.relay_addr().is_some());
        // NOTE: the shared testkit Echo server echoes SOCKS5-framed packets
        // back to the sender, but does not forward to the ultimate target.
        // A successful framed echo proves the ASSOCIATE handshake, relay
        // addressing, encode, and decode paths without requiring an external
        // forwarder.
        assoc.send(b"via socks5").await.unwrap();
        let mut buf = [0u8; 65535];
        let n = tokio::time::timeout(Duration::from_secs(5), assoc.recv(&mut buf))
            .await
            .expect("echo recv timed out")
            .unwrap();
        assert_eq!(&buf[..n], b"via socks5");
        assoc.close();
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_unsupported_chain_fails_structured() {
        let connector = OutboundConnector::from_pproxy_uri("http://127.0.0.1:8080").unwrap();
        let err = connector.associate_udp("127.0.0.1", 53).await.unwrap_err();
        match err {
            OutboundError::UnsupportedFeature { feature, .. } => {
                assert!(!feature.is_empty());
            }
            other => panic!("expected UnsupportedFeature, got {other:?}"),
        }
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_multihop_fails_structured() {
        let connector =
            OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080__http://127.0.0.1:8080")
                .unwrap();
        let err = connector.associate_udp("127.0.0.1", 53).await.unwrap_err();
        assert!(matches!(err, OutboundError::UnsupportedFeature { .. }));
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_dns_failure_classified() {
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        let err = connector
            .associate_udp("nonexistent.invalid.", 1234)
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("dns"),
            "dns failure should be classified, got: {msg}"
        );
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_oversized_rejected() {
        let echo = start_raw_udp_echo().await;
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        let assoc = connector
            .associate_udp("127.0.0.1", echo.port())
            .await
            .unwrap();
        let big = vec![0u8; super::OUTBOUND_MAX_DATAGRAM_SIZE + 1];
        let err = assoc.send(&big).await.unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("too large")
                || err.to_string().to_lowercase().contains("datagram"),
            "oversized should be rejected, got: {err}"
        );
        assoc.close();
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_timeout_does_not_leak() {
        let echo = start_raw_udp_echo().await;
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        assert_eq!(connector.active_udp_associations(), 0);
        let assoc = connector
            .associate_udp("127.0.0.1", echo.port())
            .await
            .unwrap();
        assert_eq!(connector.active_udp_associations(), 1);
        let mut buf = [0u8; 65535];
        let err = assoc
            .recv_timeout(&mut buf, Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("timed out"));
        // Timeout leaves the association open and accounted.
        assert!(!assoc.is_closed());
        assert_eq!(connector.active_udp_associations(), 1);
        // Still usable after timeout.
        assoc.send(b"after timeout").await.unwrap();
        let n = tokio::time::timeout(Duration::from_secs(5), assoc.recv(&mut buf))
            .await
            .expect("recv after timeout")
            .unwrap();
        assert_eq!(&buf[..n], b"after timeout");
        assoc.close();
        assert_eq!(connector.active_udp_associations(), 0);
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_close_is_idempotent() {
        let echo = start_raw_udp_echo().await;
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        let assoc = connector
            .associate_udp("127.0.0.1", echo.port())
            .await
            .unwrap();
        assoc.close();
        assoc.close();
        assert!(assoc.is_closed());
        // Use-after-close fails with a stable error.
        let err = assoc.send(b"nope").await.unwrap_err();
        assert!(err.to_string().to_lowercase().contains("closed"));
        assert_eq!(connector.active_udp_associations(), 0);
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_drop_decrements_accounting() {
        let echo = start_raw_udp_echo().await;
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        {
            let _assoc = connector
                .associate_udp("127.0.0.1", echo.port())
                .await
                .unwrap();
            assert_eq!(connector.active_udp_associations(), 1);
        }
        // Dropped association released its count.
        assert_eq!(connector.active_udp_associations(), 0);
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_connector_reuse_independent() {
        let echo = start_raw_udp_echo().await;
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        let a1 = connector
            .associate_udp("127.0.0.1", echo.port())
            .await
            .unwrap();
        let a2 = connector
            .associate_udp("127.0.0.1", echo.port())
            .await
            .unwrap();
        assert_eq!(connector.active_udp_associations(), 2);
        a1.send(b"one").await.unwrap();
        a2.send(b"two").await.unwrap();
        let mut buf = [0u8; 65535];
        let n1 = a1.recv(&mut buf).await.unwrap();
        assert_eq!(&buf[..n1], b"one");
        let n2 = a2.recv(&mut buf).await.unwrap();
        assert_eq!(&buf[..n2], b"two");
        a1.close();
        assert_eq!(connector.active_udp_associations(), 1);
        a2.close();
        assert_eq!(connector.active_udp_associations(), 0);
    }

    #[cfg(all(feature = "udp", feature = "pproxy-compat"))]
    #[tokio::test]
    async fn outbound_udp_associate_timeout() {
        let connector = OutboundConnector::from_pproxy_uri("direct://").unwrap();
        // Unroutable TEST-NET-1 address with a tiny timeout exercises the
        // timeout wrapper without depending on external DNS.
        let err = connector
            .associate_udp_timeout("192.0.2.1", 53, Duration::from_millis(1))
            .await;
        // Either succeeds fast (loopback bind/connect is instant) or times
        // out; both prove the wrapper does not hang. If it succeeds, close
        // cleanly so accounting returns to zero.
        if let Ok(assoc) = err {
            assoc.close();
        }
        assert_eq!(connector.active_udp_associations(), 0);
    }
}
