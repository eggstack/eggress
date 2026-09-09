//! Native outbound connector for proxy chains.
//!
//! This module provides [`OutboundConnector`], which compiles a TOML config
//! and executes the chain engine directly to open TCP connections through a
//! configured proxy chain without starting a listener service.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::EggressError;

/// Maximum outbound UDP payload accepted by the listener-free API.
///
/// Mirrors `UdpLimits::default().max_datagram_size` so direct and SOCKS5
/// paths enforce the same architectural bound as listener UDP.
pub const OUTBOUND_MAX_DATAGRAM_SIZE: usize = 65535;

/// Timeout applied to the SOCKS5 UDP ASSOCIATE handshake for outbound UDP.
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

/// Listener-free UDP association.
///
/// Fixed-target (connected) semantics: the target is fixed at
/// [`OutboundConnector::associate_udp`] time. Use [`UdpAssociation::send`]
/// and [`UdpAssociation::recv`] for payload bytes; SOCKS5 framing is handled
/// internally for upstream paths.
///
/// Supported upstream modes in this surface are direct routing and single-hop
/// SOCKS5 UDP relay (the UDP path already implemented by `eggress-udp`).
/// Composed multi-hop and Shadowsocks UDP chains are explicitly unsupported
/// here even though the listener data plane supports them; they fail with a
/// structured [`EggressError::UnsupportedFeature`] rather than silent fallback.
///
/// Close is idempotent; dropping the association also releases sockets,
/// upstream control tasks, and the connector's live-association count.
pub struct UdpAssociation {
    inner: Arc<UdpAssociationInner>,
}

impl std::fmt::Debug for UdpAssociation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Target host/port are not secret; relay/local addrs are operational
        // metadata. No credentials are stored in this type.
        f.debug_struct("UdpAssociation")
            .field("target", &self.inner.target)
            .field("closed", &self.is_closed())
            .field("relay_addr", &self.relay_addr())
            .finish()
    }
}

struct UdpAssociationInner {
    kind: UdpAssociationKind,
    target: eggress_protocol_socks::socks5::server::SocksAddr,
    max_datagram_size: usize,
    closed: AtomicBool,
    cancel: tokio_util::sync::CancellationToken,
    live_count: Arc<AtomicU64>,
}

enum UdpAssociationKind {
    Direct {
        socket: tokio::net::UdpSocket,
    },
    Socks5 {
        socket: Arc<tokio::net::UdpSocket>,
        relay_addr: std::net::SocketAddr,
        control_cancel: tokio_util::sync::CancellationToken,
        control_task: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    },
}

impl UdpAssociation {
    fn new_direct(
        socket: tokio::net::UdpSocket,
        target: eggress_protocol_socks::socks5::server::SocksAddr,
        live_count: Arc<AtomicU64>,
    ) -> Self {
        live_count.fetch_add(1, Ordering::AcqRel);
        Self {
            inner: Arc::new(UdpAssociationInner {
                kind: UdpAssociationKind::Direct { socket },
                target,
                max_datagram_size: OUTBOUND_MAX_DATAGRAM_SIZE,
                closed: AtomicBool::new(false),
                cancel: tokio_util::sync::CancellationToken::new(),
                live_count,
            }),
        }
    }

    fn new_socks5(
        socket: Arc<tokio::net::UdpSocket>,
        relay_addr: std::net::SocketAddr,
        target: eggress_protocol_socks::socks5::server::SocksAddr,
        control_cancel: tokio_util::sync::CancellationToken,
        control_task: tokio::task::JoinHandle<()>,
        live_count: Arc<AtomicU64>,
    ) -> Self {
        live_count.fetch_add(1, Ordering::AcqRel);
        Self {
            inner: Arc::new(UdpAssociationInner {
                kind: UdpAssociationKind::Socks5 {
                    socket,
                    relay_addr,
                    control_cancel,
                    control_task: std::sync::Mutex::new(Some(control_task)),
                },
                target,
                max_datagram_size: OUTBOUND_MAX_DATAGRAM_SIZE,
                closed: AtomicBool::new(false),
                cancel: tokio_util::sync::CancellationToken::new(),
                live_count,
            }),
        }
    }

    fn ensure_open(&self) -> Result<(), EggressError> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(EggressError::Runtime(
                "udp association is closed".to_string(),
            ));
        }
        Ok(())
    }

    /// Fixed target for this association.
    pub fn target(&self) -> eggress_core::TargetAddr {
        socks_to_target(&self.inner.target)
    }

    /// Local UDP socket address, if available.
    pub fn local_addr(&self) -> Option<std::net::SocketAddr> {
        match &self.inner.kind {
            UdpAssociationKind::Direct { socket } => socket.local_addr().ok(),
            UdpAssociationKind::Socks5 { socket, .. } => socket.local_addr().ok(),
        }
    }

    /// SOCKS5 relay address for upstream associations; `None` for direct.
    pub fn relay_addr(&self) -> Option<std::net::SocketAddr> {
        match &self.inner.kind {
            UdpAssociationKind::Direct { .. } => None,
            UdpAssociationKind::Socks5 { relay_addr, .. } => Some(*relay_addr),
        }
    }

    /// Whether [`UdpAssociation::close`] has been called.
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }

    /// Send one datagram payload to the fixed target.
    pub async fn send(&self, payload: &[u8]) -> Result<(), EggressError> {
        self.ensure_open()?;
        eggress_udp::security::validate_datagram_size(payload.len(), self.inner.max_datagram_size)
            .map_err(|e| EggressError::Config(e.to_string()))?;
        match &self.inner.kind {
            UdpAssociationKind::Direct { socket } => {
                tokio::select! {
                    result = socket.send(payload) => {
                        result.map(|_| ()).map_err(|e| EggressError::Runtime(format!("udp send failed: {e}")))
                    }
                    _ = self.inner.cancel.cancelled() => {
                        Err(EggressError::Runtime("udp association closed during send".to_string()))
                    }
                }
            }
            UdpAssociationKind::Socks5 {
                socket, relay_addr, ..
            } => {
                let mut out = Vec::new();
                eggress_protocol_socks::socks5::udp_codec::encode_socks5_udp_datagram(
                    &self.inner.target,
                    payload,
                    &mut out,
                )
                .map_err(|e| EggressError::Config(format!("udp encode failed: {e}")))?;
                tokio::select! {
                    result = socket.send_to(&out, *relay_addr) => {
                        result.map(|_| ()).map_err(|e| EggressError::Runtime(format!("udp upstream send failed: {e}")))
                    }
                    _ = self.inner.cancel.cancelled() => {
                        Err(EggressError::Runtime("udp association closed during send".to_string()))
                    }
                }
            }
        }
    }

    /// Receive one datagram payload from the fixed target.
    ///
    /// For direct associations this is the raw UDP payload. For SOCKS5
    /// upstream associations the SOCKS5 header is stripped and only the
    /// payload is copied into `buf`. Returns the payload length.
    pub async fn recv(&self, buf: &mut [u8]) -> Result<usize, EggressError> {
        self.ensure_open()?;
        match &self.inner.kind {
            UdpAssociationKind::Direct { socket } => {
                tokio::select! {
                    result = socket.recv(buf) => {
                        result.map_err(|e| EggressError::Runtime(format!("udp recv failed: {e}")))
                    }
                    _ = self.inner.cancel.cancelled() => {
                        Err(EggressError::Runtime("udp association closed during recv".to_string()))
                    }
                }
            }
            UdpAssociationKind::Socks5 { socket, .. } => {
                let mut tmp = vec![0u8; OUTBOUND_MAX_DATAGRAM_SIZE];
                let (n, _) = tokio::select! {
                    result = socket.recv_from(&mut tmp) => {
                        result.map_err(|e| EggressError::Runtime(format!("udp upstream recv failed: {e}")))?
                    }
                    _ = self.inner.cancel.cancelled() => {
                        return Err(EggressError::Runtime("udp association closed during recv".to_string()));
                    }
                };
                let req = eggress_protocol_socks::socks5::udp_codec::decode_socks5_udp_datagram(
                    &tmp[..n],
                )
                .map_err(|e| EggressError::Runtime(format!("udp upstream decode failed: {e}")))?;
                let payload = req.payload;
                if payload.len() > buf.len() {
                    return Err(EggressError::Config(format!(
                        "udp payload {} exceeds receive buffer {}",
                        payload.len(),
                        buf.len()
                    )));
                }
                buf[..payload.len()].copy_from_slice(payload);
                Ok(payload.len())
            }
        }
    }

    /// Send with a timeout. A timeout does not close the association.
    pub async fn send_timeout(
        &self,
        payload: &[u8],
        timeout: Duration,
    ) -> Result<(), EggressError> {
        tokio::time::timeout(timeout, self.send(payload))
            .await
            .map_err(|_| EggressError::Runtime("udp send timed out".to_string()))?
    }

    /// Receive with a timeout. A timeout does not close the association and
    /// does not leak the underlying recv task.
    pub async fn recv_timeout(
        &self,
        buf: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, EggressError> {
        tokio::time::timeout(timeout, self.recv(buf))
            .await
            .map_err(|_| EggressError::Runtime("udp recv timed out".to_string()))?
    }

    /// Close the association idempotently.
    ///
    /// Releases the UDP socket, aborts the SOCKS5 control keepalive where
    /// present, cancels pending send/recv, and decrements the connector's
    /// live-association count exactly once.
    pub fn close(&self) {
        if !self.inner.closed.swap(true, Ordering::AcqRel) {
            self.inner.cancel.cancel();
            if let UdpAssociationKind::Socks5 {
                control_cancel,
                control_task,
                ..
            } = &self.inner.kind
            {
                control_cancel.cancel();
                if let Ok(mut guard) = control_task.lock() {
                    if let Some(handle) = guard.take() {
                        handle.abort();
                    }
                }
            }
            self.inner.live_count.fetch_sub(1, Ordering::AcqRel);
        }
    }

    /// Wait until the association is closed.
    pub async fn wait_closed(&self) {
        loop {
            if self.is_closed() {
                return;
            }
            tokio::select! {
                _ = self.inner.cancel.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_millis(5)) => {}
            }
        }
    }
}

impl Drop for UdpAssociation {
    fn drop(&mut self) {
        self.close();
    }
}

fn socks_to_target(
    addr: &eggress_protocol_socks::socks5::server::SocksAddr,
) -> eggress_core::TargetAddr {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    match addr {
        SocksAddr::IPv4(octets, port) => eggress_core::TargetAddr {
            host: eggress_core::TargetHost::Ip(std::net::IpAddr::V4(std::net::Ipv4Addr::from(
                *octets,
            ))),
            port: *port,
        },
        SocksAddr::IPv6(octets, port) => eggress_core::TargetAddr {
            host: eggress_core::TargetHost::Ip(std::net::IpAddr::V6(std::net::Ipv6Addr::from(
                *octets,
            ))),
            port: *port,
        },
        SocksAddr::Domain(domain, port) => eggress_core::TargetAddr {
            host: eggress_core::TargetHost::Domain(domain.clone()),
            port: *port,
        },
    }
}

fn target_to_socks(
    host: &str,
    port: u16,
) -> Result<eggress_protocol_socks::socks5::server::SocksAddr, EggressError> {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    if port == 0 {
        return Err(EggressError::Config(
            "udp target port must be non-zero".to_string(),
        ));
    }
    if host.is_empty() {
        return Err(EggressError::Config(
            "udp target host must not be empty".to_string(),
        ));
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let socks = match ip {
            std::net::IpAddr::V4(v4) => SocksAddr::IPv4(v4.octets(), port),
            std::net::IpAddr::V6(v6) => SocksAddr::IPv6(v6.octets(), port),
        };
        // Shape validation only (multicast/broadcast/unspecified/port-zero).
        // Private and loopback targets are allowed for listener-free outbound:
        // the caller explicitly selected the destination, unlike inbound
        // listener policy which gates private egress.
        eggress_udp::security::validate_standalone_target(&socks, true)
            .map_err(|e| EggressError::Config(format!("invalid udp target: {e}")))?;
        return Ok(socks);
    }
    if host.len() > 255 {
        return Err(EggressError::Config(
            "udp target domain too long".to_string(),
        ));
    }
    let socks = SocksAddr::Domain(host.to_string(), port);
    eggress_udp::security::validate_standalone_target(&socks, true)
        .map_err(|e| EggressError::Config(format!("invalid udp target: {e}")))?;
    Ok(socks)
}

/// Resolve a fixed UDP target to a connected `SocketAddr`.
///
/// IP literals resolve directly; domains go through DNS. Failures are
/// classified as `Runtime` with a `dns` prefix so callers can distinguish
/// resolution problems from validation or transport errors without leaking
/// credentials (the target host is not secret).
async fn resolve_udp_target(
    target: &eggress_protocol_socks::socks5::server::SocksAddr,
    host: &str,
    port: u16,
) -> Result<std::net::SocketAddr, EggressError> {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    match target {
        SocksAddr::IPv4(octets, _) => Ok(std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::from(*octets)),
            port,
        )),
        SocksAddr::IPv6(octets, _) => Ok(std::net::SocketAddr::new(
            std::net::IpAddr::V6(std::net::Ipv6Addr::from(*octets)),
            port,
        )),
        SocksAddr::Domain(_, _) => {
            let lookup = format!("{host}:{port}");
            let mut addrs = tokio::net::lookup_host(&lookup).await.map_err(|e| {
                EggressError::Runtime(format!("dns resolution failed for {host}:{port}: {e}"))
            })?;
            addrs.next().ok_or_else(|| {
                EggressError::Runtime(format!(
                    "dns resolution failed for {host}:{port}: no addresses"
                ))
            })
        }
    }
}

/// Map SOCKS5 upstream establishment failures to stable embed errors.
///
/// Authentication and protocol errors never include hop credentials; only
/// the redacted reason label and transport context are surfaced.
fn map_socks5_upstream_error(e: eggress_udp::upstream_socks5::UdpUpstreamError) -> EggressError {
    use eggress_udp::upstream_socks5::UdpUpstreamError;
    match e {
        UdpUpstreamError::UnsupportedProtocol => EggressError::UnsupportedFeature {
            feature: "socks5".to_string(),
            message: "upstream chain cannot carry UDP".to_string(),
        },
        UdpUpstreamError::UnsupportedMultiHop => EggressError::UnsupportedFeature {
            feature: "multi-hop".to_string(),
            message: "multi-hop UDP chains are not supported in OutboundConnector::associate_udp"
                .to_string(),
        },
        UdpUpstreamError::Timeout => {
            EggressError::Runtime("udp upstream handshake timed out".to_string())
        }
        UdpUpstreamError::CredentialTooLong | UdpUpstreamError::DomainTooLong => {
            EggressError::Config(e.to_string())
        }
        UdpUpstreamError::TcpConnect(inner) => {
            EggressError::Runtime(format!("udp upstream TCP connect failed: {inner}"))
        }
        UdpUpstreamError::SocksMethodRejected
        | UdpUpstreamError::SocksAuthFailed
        | UdpUpstreamError::SocksAssociateRejected(_) => {
            EggressError::Runtime(format!("udp upstream SOCKS5 handshake failed: {e}"))
        }
        UdpUpstreamError::MalformedSocksReply
        | UdpUpstreamError::UdpRelayAddressInvalid
        | UdpUpstreamError::Io(_) => EggressError::Runtime(e.to_string()),
    }
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
    let mut addresses = tokio::net::lookup_host(&lookup).await.ok()?;
    addresses.next()
}

/// A native outbound connector that executes the chain engine directly.
///
/// This compiles routing/upstream state from a TOML config and provides
/// methods to open TCP connections through the configured proxy chain
/// without starting a listener service.
pub struct OutboundConnector {
    runtime_config: Option<Arc<eggress_config::compile::RuntimeConfig>>,
    chain_executor: eggress_core::chain::ChainExecutor,
    direct: bool,
    udp_live: Arc<AtomicU64>,
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

        #[cfg(feature = "ssh")]
        let chain_executor = eggress_server::build_chain_executor(None, None, None);
        #[cfg(not(feature = "ssh"))]
        let chain_executor = eggress_server::build_chain_executor(None, None);

        Ok(Self {
            runtime_config: Some(Arc::new(runtime_config)),
            chain_executor,
            direct: false,
            udp_live: Arc::new(AtomicU64::new(0)),
        })
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
    pub fn from_pproxy_uri(uri: &str) -> Result<Self, EggressError> {
        let redacted_expr = redact_pproxy_expression(uri);
        let chain = eggress_pproxy_compat::uri::parse_pproxy_chain(uri)
            .map_err(|e| map_compat_parse_error(uri, &redacted_expr, e))?;
        if chain.hops.len() == 1 && chain.hops[0].scheme == "direct" {
            #[cfg(feature = "ssh")]
            let executor = eggress_server::build_chain_executor(None, None, None);
            #[cfg(not(feature = "ssh"))]
            let executor = eggress_server::build_chain_executor(None, None);
            return Ok(Self {
                runtime_config: None,
                chain_executor: executor,
                direct: true,
                udp_live: Arc::new(AtomicU64::new(0)),
            });
        }
        // Direct native compilation: typed `PproxyChain` -> native
        // `ProxyChainSpec` with no TOML serialize/parse round trip. Validation
        // (backward, unsupported hops, plugins, local-bind, schemes) lives in
        // the compatibility crate so direct and TOML paths agree.
        let native_chain = eggress_pproxy_compat::translate::compile_chain_to_native(&chain)
            .map_err(|e| map_compat_translate_error(&chain, uri, &redacted_expr, e))?;
        if native_chain.hops.is_empty() {
            return Err(EggressError::Config(format!(
                "pproxy chain '{}' produced an empty native chain",
                chain.redacted_display()
            )));
        }
        let upstream = eggress_config::compile::UpstreamConfig {
            id: "pproxy-upstream-0".to_string(),
            chain: native_chain,
            health: Default::default(),
            h2: None,
        };
        let runtime_config = eggress_config::compile::RuntimeConfig {
            process: Default::default(),
            timeouts: Default::default(),
            listeners: Vec::new(),
            upstreams: vec![upstream],
            groups: Vec::new(),
            rules: Vec::new(),
            default_action: eggress_routing::RouteActionSpec::Direct,
            admin: None,
            reverse_servers: Vec::new(),
            reverse_clients: Vec::new(),
        };
        #[cfg(feature = "ssh")]
        let chain_executor = eggress_server::build_chain_executor(None, None, None);
        #[cfg(not(feature = "ssh"))]
        let chain_executor = eggress_server::build_chain_executor(None, None);
        Ok(Self {
            runtime_config: Some(std::sync::Arc::new(runtime_config)),
            chain_executor,
            direct: false,
            udp_live: Arc::new(AtomicU64::new(0)),
        })
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

        if self.direct {
            let stream = eggress_core::connector::DirectConnector
                .connect_with_options(&target, &eggress_core::connector::ConnectOptions::default())
                .await
                .map_err(|e| EggressError::Runtime(e.to_string()))?;
            return Ok((
                stream,
                OutboundInfo {
                    local_addr: None,
                    peer_addr: None,
                    hop_count: 0,
                },
            ));
        }

        let runtime_config = self.runtime_config.as_ref().ok_or_else(|| {
            EggressError::Runtime("outbound runtime configuration is unavailable".to_string())
        })?;
        let upstream = &runtime_config.upstreams[0];
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
    /// [`EggressError::UnsupportedFeature`] rather than silent direct
    /// fallback. Private and loopback targets are allowed because the caller
    /// explicitly selected the destination.
    pub async fn associate_udp(
        &self,
        target_host: &str,
        target_port: u16,
    ) -> Result<UdpAssociation, EggressError> {
        let target_socks = target_to_socks(target_host, target_port)?;

        if self.direct {
            let resolved = resolve_udp_target(&target_socks, target_host, target_port).await?;
            let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
                .await
                .map_err(|e| EggressError::Runtime(format!("udp bind failed: {e}")))?;
            socket
                .connect(resolved)
                .await
                .map_err(|e| EggressError::Runtime(format!("udp connect failed: {e}")))?;
            return Ok(UdpAssociation::new_direct(
                socket,
                target_socks,
                self.udp_live.clone(),
            ));
        }

        let runtime_config = self.runtime_config.as_ref().ok_or_else(|| {
            EggressError::Runtime("outbound runtime configuration is unavailable".to_string())
        })?;
        let upstream = runtime_config.upstreams.first().ok_or_else(|| {
            EggressError::Config("no upstreams configured; cannot associate UDP".to_string())
        })?;
        let chain = &upstream.chain;

        match eggress_udp::udp_capability(chain) {
            eggress_udp::UdpRelayCapability::SupportedSocks5 => {
                let hop = chain.hops.first().ok_or_else(|| {
                    EggressError::Config("upstream chain is empty".to_string())
                })?;
                let assoc = eggress_udp::upstream_socks5::open_socks5_udp_upstream(
                    eggress_udp::upstream_socks5::Socks5UdpUpstreamConfig {
                        upstream_id: eggress_core::UpstreamId::new(upstream.id.as_str()),
                        hop: hop.clone(),
                        connect_timeout: OUTBOUND_UDP_CONNECT_TIMEOUT,
                        udp_bind: "127.0.0.1:0".parse().expect("loopback bind parses"),
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
            eggress_udp::UdpRelayCapability::SupportedComposed => Err(EggressError::UnsupportedFeature {
                feature: "composed-udp".to_string(),
                message: "listener-free UDP supports direct and single-hop SOCKS5 only; composed multi-hop UDP chains are not supported in OutboundConnector::associate_udp".to_string(),
            }),
            eggress_udp::UdpRelayCapability::UnsupportedProtocol { protocol } => {
                Err(EggressError::UnsupportedFeature {
                    feature: protocol.clone(),
                    message: format!(
                        "upstream protocol '{protocol}' cannot carry UDP in OutboundConnector::associate_udp; use direct or single-hop SOCKS5"
                    ),
                })
            }
            eggress_udp::UdpRelayCapability::UnsupportedMultiHop => {
                Err(EggressError::UnsupportedFeature {
                    feature: "multi-hop".to_string(),
                    message: "listener-free UDP supports direct and single-hop SOCKS5 only; multi-hop UDP chains are not supported in OutboundConnector::associate_udp".to_string(),
                })
            }
            // Covers Shadowsocks UDP (feature-gated in eggress-udp) and any
            // future UDP-capable modes: explicitly unsupported in the
            // listener-free surface rather than silent fallback.
            _ => Err(EggressError::UnsupportedFeature {
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
    pub async fn associate_udp_timeout(
        &self,
        target_host: &str,
        target_port: u16,
        timeout: Duration,
    ) -> Result<UdpAssociation, EggressError> {
        tokio::time::timeout(timeout, self.associate_udp(target_host, target_port))
            .await
            .map_err(|_| EggressError::Runtime("udp association timed out".to_string()))?
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
        self.runtime_config
            .as_ref()
            .map_or(0, |config| config.upstreams.len())
    }

    /// Validate that the config is usable for outbound connections.
    ///
    /// Returns the number of hops in the first upstream's chain.
    ///
    /// Parsing/validation/compilation goes through the shared
    /// [`crate::parse_validate_compile`] boundary exactly once.
    pub fn validate_outbound_config(config_toml: &str) -> Result<usize, EggressError> {
        let runtime_config =
            crate::parse_validate_compile(config_toml).map_err(EggressError::Config)?;

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

/// Redact credentials in a pproxy remote expression without requiring it
/// to parse successfully.
///
/// Valid hops use the typed `PproxyUri::redacted_display` so bind addresses
/// and plugin names survive; unparseable segments fall back to aggressive
/// syntax-local redaction. The exact rendering is not contractual; absence
/// of secrets is.
#[cfg(feature = "pproxy-compat")]
fn redact_pproxy_expression(input: &str) -> String {
    split_redaction_hops(input)
        .iter()
        .map(|segment| redact_pproxy_hop(segment))
        .collect::<Vec<_>>()
        .join("__")
}

/// Split a pproxy expression on `__` while ignoring separators inside
/// bracketed IPv6 literals and brace-delimited fixed targets.
/// Never fails; unmatched brackets are treated literally.
#[cfg(feature = "pproxy-compat")]
fn split_redaction_hops(input: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut bracket = 0u32;
    let mut brace = 0u32;
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] as char {
            '[' => bracket += 1,
            ']' => bracket = bracket.saturating_sub(1),
            '{' => brace += 1,
            '}' => brace = brace.saturating_sub(1),
            '_' if i + 1 < bytes.len() && bytes[i + 1] == b'_' && bracket == 0 && brace == 0 => {
                out.push(&input[start..i]);
                i += 1;
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&input[start..]);
    out
}

#[cfg(feature = "pproxy-compat")]
fn redact_pproxy_hop(segment: &str) -> String {
    if segment.is_empty() {
        return String::new();
    }
    if let Ok(parsed) = eggress_pproxy_compat::uri::parse_pproxy_uri(segment) {
        return parsed.redacted_display();
    }
    fallback_redact_hop(segment)
}

/// Aggressive fallback for hops that do not parse: hide anything that
/// could be userinfo and any `#` auth fragment. Over-redaction is
/// acceptable here; leakage is not.
#[cfg(feature = "pproxy-compat")]
fn fallback_redact_hop(segment: &str) -> String {
    let (before_hash, has_fragment) = match segment.find('#') {
        Some(pos) => (&segment[..pos], true),
        None => (segment, false),
    };
    let frag_suffix = if has_fragment { "#****" } else { "" };
    if before_hash.starts_with("unix://") {
        return format!("unix://****{frag_suffix}");
    }
    let Some(scheme_end) = before_hash.find("://") else {
        if let Some(at) = find_last_at_outside_brackets(before_hash) {
            return format!("****:****@{}{frag_suffix}", &before_hash[at + 1..]);
        }
        return format!("{before_hash}{frag_suffix}");
    };
    let scheme = &before_hash[..scheme_end];
    let after = &before_hash[scheme_end + 3..];
    if let Some(at) = find_last_at_outside_brackets(after) {
        format!("{}://****:****@{}{frag_suffix}", scheme, &after[at + 1..])
    } else {
        format!("{before_hash}{frag_suffix}")
    }
}

#[cfg(feature = "pproxy-compat")]
fn find_last_at_outside_brackets(s: &str) -> Option<usize> {
    let mut last = None;
    let mut depth = 0u32;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '@' if depth == 0 => last = Some(i),
            _ => {}
        }
    }
    last
}

/// Redact `scheme://...@...` userinfo occurrences embedded in free-form
/// diagnostic text, plus `#` auth fragments that carry credentials.
#[cfg(feature = "pproxy-compat")]
fn redact_credentials_in_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("://") {
        out.push_str(&rest[..pos + 3]);
        rest = &rest[pos + 3..];
        let mut token_end = rest.len();
        for (i, c) in rest.char_indices() {
            if c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '(' | ')') {
                token_end = i;
                break;
            }
        }
        let (token, remainder) = (&rest[..token_end], &rest[token_end..]);
        let (before_hash, fragment) = match token.find('#') {
            Some(p) => (&token[..p], Some(&token[p..])),
            None => (token, None),
        };
        if let Some(at) = find_last_at_outside_brackets(before_hash) {
            out.push_str("****:****@");
            out.push_str(&before_hash[at + 1..]);
        } else {
            out.push_str(before_hash);
        }
        if let Some(frag) = fragment {
            if frag.contains(':') || frag.contains('@') {
                out.push_str("#****");
            } else {
                out.push_str(frag);
            }
        }
        rest = remainder;
    }
    out.push_str(rest);
    out
}

/// Percent-encode mirroring the compatibility translator so scrubbing
/// catches credentials that reappear in generated config URIs.
#[cfg(feature = "pproxy-compat")]
fn percent_encode_for_scrub(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(feature = "pproxy-compat")]
fn chain_credential_terms(chain: &eggress_pproxy_compat::uri::PproxyChain) -> Vec<String> {
    let mut terms = Vec::new();
    for hop in &chain.hops {
        for value in [&hop.username, &hop.password].into_iter().flatten() {
            if !value.is_empty() {
                terms.push(value.clone());
            }
        }
        if let Some(fragment) = &hop.auth_fragment {
            if !fragment.is_empty() {
                terms.push(fragment.clone());
                if let Some((user, pass)) = fragment.split_once(':') {
                    if !user.is_empty() {
                        terms.push(user.to_string());
                    }
                    if !pass.is_empty() {
                        terms.push(pass.to_string());
                    }
                }
            }
        }
    }
    terms.sort_by_key(|term| std::cmp::Reverse(term.len()));
    terms
}

/// Scrub a diagnostic message of the original expression and every
/// credential term carried by the parsed chain (raw and percent-encoded),
/// plus any generic `://...@` userinfo that remains.
#[cfg(feature = "pproxy-compat")]
fn scrub_message_with_chain(
    chain: &eggress_pproxy_compat::uri::PproxyChain,
    original_uri: &str,
    redacted_expr: &str,
    message: String,
) -> String {
    let mut msg = message.replace(original_uri, redacted_expr);
    for term in chain_credential_terms(chain) {
        if !term.is_empty() {
            msg = msg.replace(term.as_str(), "****");
            let encoded = percent_encode_for_scrub(&term);
            if encoded != term {
                msg = msg.replace(encoded.as_str(), "****");
            }
        }
    }
    redact_credentials_in_text(&msg)
}

#[cfg(feature = "pproxy-compat")]
fn map_compat_parse_error(
    uri: &str,
    redacted_expr: &str,
    error: eggress_pproxy_compat::CompatError,
) -> EggressError {
    let raw = error.to_string();
    let mut detail = raw.replace(uri, redacted_expr);
    detail = redact_credentials_in_text(&detail);
    let message = format!("invalid pproxy chain '{redacted_expr}': {detail}");
    match error {
        eggress_pproxy_compat::CompatError::UnsupportedProtocol(protocol) => {
            EggressError::UnsupportedFeature {
                feature: protocol,
                message,
            }
        }
        eggress_pproxy_compat::CompatError::UnsupportedFeature { feature, .. } => {
            EggressError::UnsupportedFeature {
                feature: feature.to_string(),
                message,
            }
        }
        _ => EggressError::Config(message),
    }
}

#[cfg(feature = "pproxy-compat")]
fn map_compat_translate_error(
    chain: &eggress_pproxy_compat::uri::PproxyChain,
    uri: &str,
    redacted_expr: &str,
    error: eggress_pproxy_compat::CompatError,
) -> EggressError {
    let detail = scrub_message_with_chain(chain, uri, redacted_expr, error.to_string());
    let message = format!(
        "pproxy chain '{}' failed translation: {}",
        chain.redacted_display(),
        detail
    );
    match error {
        eggress_pproxy_compat::CompatError::UnsupportedProtocol(protocol) => {
            EggressError::UnsupportedFeature {
                feature: protocol,
                message,
            }
        }
        eggress_pproxy_compat::CompatError::UnsupportedFeature { feature, .. } => {
            EggressError::UnsupportedFeature {
                feature: feature.to_string(),
                message,
            }
        }
        _ => EggressError::Config(message),
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

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_single_http() {
        let connector =
            OutboundConnector::from_pproxy_uri("http://127.0.0.1:8080").expect("single HTTP");
        let runtime = connector
            .runtime_config
            .as_ref()
            .expect("single-hop connector has runtime config");
        assert_eq!(runtime.upstreams.len(), 1);
        assert_eq!(runtime.upstreams[0].chain.hops.len(), 1);
        assert!(
            runtime.upstreams[0].chain.hops[0]
                .protocols
                .contains(&eggress_uri::ProtocolSpec::Http),
            "expected HTTP hop, got {:?}",
            runtime.upstreams[0].chain.hops[0].protocols
        );
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_two_hop_chain() {
        let connector =
            OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080__http://127.0.0.1:8080")
                .expect("two-hop chain should construct");
        let runtime = connector
            .runtime_config
            .as_ref()
            .expect("chained connector has runtime config");
        assert_eq!(runtime.upstreams.len(), 1);
        let hops = &runtime.upstreams[0].chain.hops;
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
        assert!(!connector.direct);
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_three_hop_chain() {
        let connector = OutboundConnector::from_pproxy_uri(
            "socks5://127.0.0.1:1080__http://127.0.0.1:8080__socks4://127.0.0.1:1081",
        )
        .expect("three-hop chain should construct");
        let runtime = connector.runtime_config.as_ref().expect("runtime config");
        let hops = &runtime.upstreams[0].chain.hops;
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
        assert!(connector.direct);
        assert!(connector.runtime_config.is_none());
        assert_eq!(connector.upstream_count(), 0);
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
        let runtime = connector.runtime_config.as_ref().expect("runtime config");
        let hops = &runtime.upstreams[0].chain.hops;
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

    async fn start_raw_udp_echo() -> std::net::SocketAddr {
        eggress_udp::testkit::start_udp_echo_server().await
    }

    #[cfg(feature = "pproxy-compat")]
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

    #[cfg(feature = "pproxy-compat")]
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

    #[cfg(feature = "pproxy-compat")]
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

    #[cfg(feature = "pproxy-compat")]
    #[tokio::test]
    async fn outbound_udp_unsupported_chain_fails_structured() {
        let connector = OutboundConnector::from_pproxy_uri("http://127.0.0.1:8080").unwrap();
        let err = connector.associate_udp("127.0.0.1", 53).await.unwrap_err();
        match err {
            EggressError::UnsupportedFeature { feature, .. } => {
                assert!(!feature.is_empty());
            }
            other => panic!("expected UnsupportedFeature, got {other:?}"),
        }
    }

    #[cfg(feature = "pproxy-compat")]
    #[tokio::test]
    async fn outbound_udp_multihop_fails_structured() {
        let connector =
            OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080__http://127.0.0.1:8080")
                .unwrap();
        let err = connector.associate_udp("127.0.0.1", 53).await.unwrap_err();
        assert!(matches!(err, EggressError::UnsupportedFeature { .. }));
    }

    #[cfg(feature = "pproxy-compat")]
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

    #[cfg(feature = "pproxy-compat")]
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

    #[cfg(feature = "pproxy-compat")]
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

    #[cfg(feature = "pproxy-compat")]
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

    #[cfg(feature = "pproxy-compat")]
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

    #[cfg(feature = "pproxy-compat")]
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

    #[cfg(feature = "pproxy-compat")]
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
