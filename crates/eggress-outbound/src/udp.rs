//! Private UDP association lifecycle for listener-free outbound chains.
//!
//! Owns [`UdpAssociation`], direct/SOCKS5 send/recv, resolution/conversion,
//! and accounting. Capability is frozen: direct plus the currently supported
//! SOCKS5 form; no pooling, no listener-backed implementation, no multi-hop
//! expansion.

#[cfg(feature = "udp")]
use std::sync::atomic::AtomicBool;
#[cfg(feature = "udp")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "udp")]
use std::sync::Arc;
#[cfg(feature = "udp")]
use std::time::Duration;

#[cfg(feature = "udp")]
use crate::OutboundError;

#[cfg(feature = "udp")]
use super::connector::OUTBOUND_MAX_DATAGRAM_SIZE;

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
    pub(crate) fn new_direct(
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

    pub(crate) fn new_socks5(
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

    pub(crate) fn ensure_open(&self) -> Result<(), OutboundError> {
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
pub(crate) fn socks_to_target(
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
pub(crate) fn target_to_socks(
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
pub(crate) async fn resolve_udp_target(
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
pub(crate) fn wildcard_bind_for_resolved(resolved: &std::net::SocketAddr) -> std::net::SocketAddr {
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
pub(crate) fn map_socks5_upstream_error(
    e: eggress_udp::upstream_socks5::UdpUpstreamError,
) -> OutboundError {
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
