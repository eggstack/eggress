//! Native outbound connector for proxy chains.
//!
//! This module provides [`OutboundConnector`], which executes the chain engine
//! directly to open TCP connections through a configured proxy chain without
//! starting a listener service. It is the listener-free Rust dependency;
//! `eggress-embed` re-exports this API as a full-service facade.

#[cfg(feature = "udp")]
use std::sync::atomic::AtomicBool;
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

/// Stable failure category for listener-free TCP establishment.
///
/// Protocol-neutral and general-purpose: embedding consumers match on this
/// plus [`OutboundConnectError::stage`]/[`OutboundConnectError::hop_index`]
/// to implement their own routing policy without parsing display strings.
/// The enum is [`non_exhaustive`](https://doc.rust-lang.org/reference/attributes/type_system.html)
/// so new categories can be added without breaking downstream matches.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundConnectErrorKind {
    Timeout,
    Dns,
    ConnectionRefused,
    NetworkUnreachable,
    HostUnreachable,
    Authentication,
    Tls,
    Protocol,
    Policy,
    Other,
}

impl std::fmt::Display for OutboundConnectErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Timeout => "timeout",
            Self::Dns => "dns",
            Self::ConnectionRefused => "connection_refused",
            Self::NetworkUnreachable => "network_unreachable",
            Self::HostUnreachable => "host_unreachable",
            Self::Authentication => "authentication",
            Self::Tls => "tls",
            Self::Protocol => "protocol",
            Self::Policy => "policy",
            Self::Other => "other",
        };
        f.write_str(label)
    }
}

/// Route stage where a listener-free TCP establishment failed.
///
/// Distinguishes transport failures (opening TCP to a hop) from proxy
/// protocol handshake failures (the proxy accepted TCP but reported a
/// destination/authentication problem), plus the caller-supplied outer
/// deadline. Consumers combine `kind` + `stage` + `hop_index` into their own
/// policy; Eggress never retries or falls back on their behalf.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundConnectStage {
    DirectConnect,
    HopConnect,
    HopHandshake,
    Deadline,
}

impl std::fmt::Display for OutboundConnectStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::DirectConnect => "direct_connect",
            Self::HopConnect => "hop_connect",
            Self::HopHandshake => "hop_handshake",
            Self::Deadline => "deadline",
        };
        f.write_str(label)
    }
}

/// Typed failure for listener-free TCP establishment.
///
/// Returned by
/// [`OutboundConnector::connect_tcp_detailed`] and
/// [`OutboundConnector::connect_tcp_timeout_detailed`]. The struct uses
/// private fields with accessors so metadata can be extended without forcing
/// exhaustive matches on internal details.
///
/// `Display` and `Debug` are bounded, redacted, and safe to log: they carry
/// only kind/stage/hop/protocol facts, never proxy credentials, passwords,
/// auth headers, full credential-bearing URIs, or configuration snippets.
/// Callers already know the target they requested; it is not echoed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundConnectError {
    kind: OutboundConnectErrorKind,
    stage: OutboundConnectStage,
    hop_index: Option<usize>,
    protocol: Option<String>,
    message: String,
}

impl OutboundConnectError {
    fn new(
        kind: OutboundConnectErrorKind,
        stage: OutboundConnectStage,
        hop_index: Option<usize>,
        protocol: Option<String>,
    ) -> Self {
        let message = match (&hop_index, &protocol) {
            (Some(hop), Some(proto)) => {
                format!("outbound connection failed: kind={kind} stage={stage} hop={hop} protocol={proto}")
            }
            (Some(hop), None) => {
                format!("outbound connection failed: kind={kind} stage={stage} hop={hop}")
            }
            (None, Some(proto)) => {
                format!("outbound connection failed: kind={kind} stage={stage} protocol={proto}")
            }
            (None, None) => {
                format!("outbound connection failed: kind={kind} stage={stage}")
            }
        };
        Self {
            kind,
            stage,
            hop_index,
            protocol,
            message,
        }
    }

    /// Stable failure category for routing policy.
    pub fn kind(&self) -> OutboundConnectErrorKind {
        self.kind
    }

    /// Route stage where establishment failed.
    pub fn stage(&self) -> OutboundConnectStage {
        self.stage
    }

    /// Failing hop index for chained failures, if applicable.
    pub fn hop_index(&self) -> Option<usize> {
        self.hop_index
    }

    /// Bounded protocol label for handshake failures, if applicable.
    ///
    /// Derived from the chain's protocol identifier (lowercased, e.g.
    /// `"http"`, `"socks5"`, `"tls"`); never contains credentials.
    pub fn protocol(&self) -> Option<&str> {
        self.protocol.as_deref()
    }
}

impl std::fmt::Display for OutboundConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OutboundConnectError {}

/// Internal classified failure retaining both public representations.
///
/// `error` is the detailed typed surface; `compat_message` is the exact
/// sanitized string legacy `connect_tcp()`/`connect_tcp_timeout()` return
/// inside `OutboundError::Runtime`, preserving behavioral compatibility.
struct ClassifiedFailure {
    error: OutboundConnectError,
    compat_message: String,
}

fn map_classified_kind(kind: crate::classify::ClassifiedKind) -> OutboundConnectErrorKind {
    match kind {
        crate::classify::ClassifiedKind::Timeout => OutboundConnectErrorKind::Timeout,
        crate::classify::ClassifiedKind::Dns => OutboundConnectErrorKind::Dns,
        crate::classify::ClassifiedKind::Refused => OutboundConnectErrorKind::ConnectionRefused,
        crate::classify::ClassifiedKind::NetworkUnreachable => {
            OutboundConnectErrorKind::NetworkUnreachable
        }
        crate::classify::ClassifiedKind::HostUnreachable => {
            OutboundConnectErrorKind::HostUnreachable
        }
        crate::classify::ClassifiedKind::Auth => OutboundConnectErrorKind::Authentication,
        crate::classify::ClassifiedKind::Tls => OutboundConnectErrorKind::Tls,
        crate::classify::ClassifiedKind::Protocol => OutboundConnectErrorKind::Protocol,
        crate::classify::ClassifiedKind::Policy => OutboundConnectErrorKind::Policy,
        crate::classify::ClassifiedKind::Other => OutboundConnectErrorKind::Other,
    }
}

/// Normalize a chain protocol label for the public typed surface.
///
/// The chain executor reports labels like `"Http"`, `"Socks5"`, `"tls"`, or
/// `"Http+Socks5"`. Lowercasing keeps the stable contract predictable while
/// remaining bounded; the label originates from `ProtocolSpec` debug names
/// and never carries credentials.
fn normalize_protocol_label(raw: &str) -> String {
    raw.to_ascii_lowercase()
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
/// structured [`OutboundError::UnsupportedFeature`] rather than silent fallback.
///
/// Close is idempotent; dropping the association also releases sockets,
/// upstream control tasks, and the connector's live-association count.
#[cfg(feature = "udp")]
pub struct UdpAssociation {
    inner: Arc<UdpAssociationInner>,
}

#[cfg(feature = "udp")]
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

#[cfg(feature = "udp")]
struct UdpAssociationInner {
    kind: UdpAssociationKind,
    target: eggress_protocol_socks::socks5::server::SocksAddr,
    max_datagram_size: usize,
    closed: AtomicBool,
    cancel: tokio_util::sync::CancellationToken,
    live_count: Arc<AtomicU64>,
}

#[cfg(feature = "udp")]
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

#[cfg(feature = "udp")]
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

    fn ensure_open(&self) -> Result<(), OutboundError> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(OutboundError::Runtime(
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
    pub async fn send(&self, payload: &[u8]) -> Result<(), OutboundError> {
        self.ensure_open()?;
        eggress_udp::security::validate_datagram_size(payload.len(), self.inner.max_datagram_size)
            .map_err(|e| OutboundError::Config(e.to_string()))?;
        match &self.inner.kind {
            UdpAssociationKind::Direct { socket } => {
                tokio::select! {
                    result = socket.send(payload) => {
                        result.map(|_| ()).map_err(|e| OutboundError::Runtime(format!("udp send failed: {e}")))
                    }
                    _ = self.inner.cancel.cancelled() => {
                        Err(OutboundError::Runtime("udp association closed during send".to_string()))
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
                .map_err(|e| OutboundError::Config(format!("udp encode failed: {e}")))?;
                tokio::select! {
                    result = socket.send_to(&out, *relay_addr) => {
                        result.map(|_| ()).map_err(|e| OutboundError::Runtime(format!("udp upstream send failed: {e}")))
                    }
                    _ = self.inner.cancel.cancelled() => {
                        Err(OutboundError::Runtime("udp association closed during send".to_string()))
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
    pub async fn recv(&self, buf: &mut [u8]) -> Result<usize, OutboundError> {
        self.ensure_open()?;
        match &self.inner.kind {
            UdpAssociationKind::Direct { socket } => {
                tokio::select! {
                    result = socket.recv(buf) => {
                        result.map_err(|e| OutboundError::Runtime(format!("udp recv failed: {e}")))
                    }
                    _ = self.inner.cancel.cancelled() => {
                        Err(OutboundError::Runtime("udp association closed during recv".to_string()))
                    }
                }
            }
            UdpAssociationKind::Socks5 { socket, .. } => {
                let mut tmp = vec![0u8; OUTBOUND_MAX_DATAGRAM_SIZE];
                let (n, _) = tokio::select! {
                    result = socket.recv_from(&mut tmp) => {
                        result.map_err(|e| OutboundError::Runtime(format!("udp upstream recv failed: {e}")))?
                    }
                    _ = self.inner.cancel.cancelled() => {
                        return Err(OutboundError::Runtime("udp association closed during recv".to_string()));
                    }
                };
                let req = eggress_protocol_socks::socks5::udp_codec::decode_socks5_udp_datagram(
                    &tmp[..n],
                )
                .map_err(|e| OutboundError::Runtime(format!("udp upstream decode failed: {e}")))?;
                let payload = req.payload;
                if payload.len() > buf.len() {
                    return Err(OutboundError::Config(format!(
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
    ) -> Result<(), OutboundError> {
        tokio::time::timeout(timeout, self.send(payload))
            .await
            .map_err(|_| OutboundError::Runtime("udp send timed out".to_string()))?
    }

    /// Receive with a timeout. A timeout does not close the association and
    /// does not leak the underlying recv task.
    pub async fn recv_timeout(
        &self,
        buf: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, OutboundError> {
        tokio::time::timeout(timeout, self.recv(buf))
            .await
            .map_err(|_| OutboundError::Runtime("udp recv timed out".to_string()))?
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

#[cfg(feature = "udp")]
impl Drop for UdpAssociation {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(feature = "udp")]
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

#[cfg(feature = "udp")]
fn target_to_socks(
    host: &str,
    port: u16,
) -> Result<eggress_protocol_socks::socks5::server::SocksAddr, OutboundError> {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    if port == 0 {
        return Err(OutboundError::Config(
            "udp target port must be non-zero".to_string(),
        ));
    }
    if host.is_empty() {
        return Err(OutboundError::Config(
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
            .map_err(|e| OutboundError::Config(format!("invalid udp target: {e}")))?;
        return Ok(socks);
    }
    if host.len() > 255 {
        return Err(OutboundError::Config(
            "udp target domain too long".to_string(),
        ));
    }
    let socks = SocksAddr::Domain(host.to_string(), port);
    eggress_udp::security::validate_standalone_target(&socks, true)
        .map_err(|e| OutboundError::Config(format!("invalid udp target: {e}")))?;
    Ok(socks)
}

/// Resolve a fixed UDP target to a connected `SocketAddr`.
///
/// IP literals resolve directly; domains go through DNS. Failures are
/// classified as `Runtime` with a `dns` prefix so callers can distinguish
/// resolution problems from validation or transport errors without leaking
/// credentials (the target host is not secret).
#[cfg(feature = "udp")]
async fn resolve_udp_target(
    target: &eggress_protocol_socks::socks5::server::SocksAddr,
    host: &str,
    port: u16,
) -> Result<std::net::SocketAddr, OutboundError> {
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
            // Tuple form handles DNS without brittle `host:port` string
            // formatting. Current selection semantics (first address) are
            // preserved; no Happy-Eyeballs subsystem is introduced here.
            let mut addrs = tokio::net::lookup_host((host, port)).await.map_err(|e| {
                OutboundError::Runtime(format!("dns resolution failed for {host}:{port}: {e}"))
            })?;
            addrs.next().ok_or_else(|| {
                OutboundError::Runtime(format!(
                    "dns resolution failed for {host}:{port}: no addresses"
                ))
            })
        }
    }
}

/// Wildcard ephemeral bind matching a resolved destination family.
///
/// IPv4 destinations bind `0.0.0.0:0`; IPv6 destinations bind `[::]:0`.
/// Listener-free direct UDP never binds loopback; the destination is
/// caller-selected so a normal ephemeral wildcard source is correct.
#[cfg(feature = "udp")]
fn wildcard_bind_for_resolved(resolved: &std::net::SocketAddr) -> std::net::SocketAddr {
    match resolved {
        std::net::SocketAddr::V4(_) => "0.0.0.0:0".parse().expect("ipv4 wildcard parses"),
        std::net::SocketAddr::V6(_) => "[::]:0".parse().expect("ipv6 wildcard parses"),
    }
}

/// Map SOCKS5 upstream establishment failures to stable embed errors.
///
/// Authentication and protocol errors never include hop credentials; only
/// the redacted reason label and transport context are surfaced.
#[cfg(feature = "udp")]
fn map_socks5_upstream_error(e: eggress_udp::upstream_socks5::UdpUpstreamError) -> OutboundError {
    use eggress_udp::upstream_socks5::UdpUpstreamError;
    match e {
        UdpUpstreamError::UnsupportedProtocol => OutboundError::UnsupportedFeature {
            feature: "socks5".to_string(),
            message: "upstream chain cannot carry UDP".to_string(),
        },
        UdpUpstreamError::UnsupportedMultiHop => OutboundError::UnsupportedFeature {
            feature: "multi-hop".to_string(),
            message: "multi-hop UDP chains are not supported in OutboundConnector::associate_udp"
                .to_string(),
        },
        UdpUpstreamError::Timeout => {
            OutboundError::Runtime("udp upstream handshake timed out".to_string())
        }
        UdpUpstreamError::CredentialTooLong | UdpUpstreamError::DomainTooLong => {
            OutboundError::Config(e.to_string())
        }
        UdpUpstreamError::TcpConnect(inner) => {
            OutboundError::Runtime(format!("udp upstream TCP connect failed: {inner}"))
        }
        UdpUpstreamError::SocksMethodRejected
        | UdpUpstreamError::SocksAuthFailed
        | UdpUpstreamError::SocksAssociateRejected(_) => {
            OutboundError::Runtime(format!("udp upstream SOCKS5 handshake failed: {e}"))
        }
        UdpUpstreamError::MalformedSocksReply
        | UdpUpstreamError::UdpRelayAddressInvalid
        | UdpUpstreamError::Io(_) => OutboundError::Runtime(e.to_string()),
    }
}

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
) -> OutboundError {
    let raw = error.to_string();
    let mut detail = raw.replace(uri, redacted_expr);
    detail = redact_credentials_in_text(&detail);
    let message = format!("invalid pproxy chain '{redacted_expr}': {detail}");
    match error {
        eggress_pproxy_compat::CompatError::UnsupportedProtocol(protocol) => {
            OutboundError::UnsupportedFeature {
                feature: protocol,
                message,
            }
        }
        eggress_pproxy_compat::CompatError::UnsupportedFeature { feature, .. } => {
            OutboundError::UnsupportedFeature {
                feature: feature.to_string(),
                message,
            }
        }
        _ => OutboundError::Config(message),
    }
}

#[cfg(feature = "pproxy-compat")]
fn map_compat_translate_error(
    chain: &eggress_pproxy_compat::uri::PproxyChain,
    uri: &str,
    redacted_expr: &str,
    error: eggress_pproxy_compat::CompatError,
) -> OutboundError {
    let detail = scrub_message_with_chain(chain, uri, redacted_expr, error.to_string());
    let message = format!(
        "pproxy chain '{}' failed translation: {}",
        chain.redacted_display(),
        detail
    );
    match error {
        eggress_pproxy_compat::CompatError::UnsupportedProtocol(protocol) => {
            OutboundError::UnsupportedFeature {
                feature: protocol,
                message,
            }
        }
        eggress_pproxy_compat::CompatError::UnsupportedFeature { feature, .. } => {
            OutboundError::UnsupportedFeature {
                feature: feature.to_string(),
                message,
            }
        }
        _ => OutboundError::Config(message),
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
