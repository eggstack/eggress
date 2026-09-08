//! Inbound accept: authentication, detection, and protocol handshakes.
//!
//! Entry points (`accept`, `accept_with_fixed_target[_for_peer]`)
//! live here with auth/session types; protocol-specific handlers
//! (`handlers`), HTTP-forward parsing (`forward`), detection
//! (`detect`), and the prefixed stream (`prefixed`) are submodules.

use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

static AUTH_CACHE_EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

use eggress_core::BoxStream;
use eggress_core::{ClientIdentity, ProtocolId, TargetAddr};
use tokio::io::AsyncReadExt;
use zeroize::Zeroize;

pub(crate) mod detect;
pub(crate) mod forward;
pub(crate) mod handlers;
pub(crate) mod prefixed;
#[cfg(test)]
mod tests;

pub(crate) use detect::{detect_http_method, DetectResult};
#[cfg(test)]
pub(crate) use forward::read_http_head;
pub(crate) use handlers::{accept_http, accept_socks4, accept_socks5};
pub(crate) use prefixed::PrefixedStream;

/// Authentication policy for inbound connections.
/// Bounded compatibility authentication state keyed by source IP.
///
/// This intentionally lives in the server crate but is only constructed by
/// the pproxy compatibility runtime. Native Eggress listeners continue to
/// authenticate every connection independently.
pub struct AuthReuseCache {
    timeout: Duration,
    entries: Mutex<HashMap<IpAddr, AuthReuseEntry>>,
    max_entries: usize,
    /// Nanosecond timestamp of the last full expiration sweep. Used to bound
    /// how often `record` may run an O(n) `retain` under the cache lock;
    /// expired entries are otherwise evicted lazily on `lookup`.
    last_sweep_nanos: AtomicU64,
}

struct AuthReuseEntry {
    identity: ClientIdentity,
    last_authenticated: Instant,
}

impl AuthReuseCache {
    pub const DEFAULT_MAX_ENTRIES: usize = 4096;

    /// Minimum gap between full expiration sweeps inside `record`. Lookup-time
    /// expiration still runs on every call, so a long gap only delays the
    /// reclaim of entries that are never looked up again.
    const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            entries: Mutex::new(HashMap::new()),
            max_entries: Self::DEFAULT_MAX_ENTRIES,
            last_sweep_nanos: AtomicU64::new(0),
        }
    }

    fn lock_entries(&self) -> MutexGuard<'_, HashMap<IpAddr, AuthReuseEntry>> {
        self.entries.lock().unwrap_or_else(|error| {
            tracing::warn!("auth reuse cache was poisoned; clearing it: {error}");
            let mut entries = error.into_inner();
            entries.clear();
            self.entries.clear_poison();
            entries
        })
    }

    pub fn lookup(&self, peer_ip: IpAddr) -> Option<ClientIdentity> {
        let mut entries = self.lock_entries();
        let entry = entries.get(&peer_ip)?;
        if Instant::now().duration_since(entry.last_authenticated) > self.timeout {
            entries.remove(&peer_ip);
            return None;
        }
        Some(entry.identity.clone())
    }

    pub fn record(&self, peer_ip: IpAddr, identity: ClientIdentity) {
        let mut entries = self.lock_entries();
        let now = Instant::now();
        // Expired entries are removed lazily by `lookup`; only pay for a full
        // sweep once the cache is actually at capacity *and* a full
        // sweep has not run in the last SWEEP_INTERVAL. This caps the
        // tail-latency hit of an O(n) `retain` under the cache lock. The
        // interval check is evaluated while the lock is held so the sweep
        // schedule is time-based rather than lock-contention dependent.
        if entries.len() >= self.max_entries {
            let now_nanos = u64::try_from(
                now.checked_duration_since(*AUTH_CACHE_EPOCH)
                    .unwrap_or(Duration::ZERO)
                    .as_nanos(),
            )
            .unwrap_or(u64::MAX);
            let last_sweep = self.last_sweep_nanos.load(Ordering::Acquire);
            let interval_nanos = u64::try_from(Self::SWEEP_INTERVAL.as_nanos()).unwrap_or(u64::MAX);
            if now_nanos.saturating_sub(last_sweep) >= interval_nanos {
                self.last_sweep_nanos.store(now_nanos, Ordering::Release);
                entries.retain(|_, entry| {
                    now.duration_since(entry.last_authenticated) <= self.timeout
                });
            }
        }
        if entries.len() >= self.max_entries && !entries.contains_key(&peer_ip) {
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_authenticated)
                .map(|(ip, _)| *ip)
            {
                entries.remove(&oldest);
            }
        }
        entries.insert(
            peer_ip,
            AuthReuseEntry {
                identity,
                last_authenticated: now,
            },
        );
    }

    pub fn len(&self) -> usize {
        self.lock_entries().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone)]
pub enum InboundAuthentication {
    None,
    UsernamePassword {
        username: String,
        password: String,
    },
    UsernamePasswordWithReuse {
        username: String,
        password: String,
        reuse: Arc<AuthReuseCache>,
    },
}

impl Drop for InboundAuthentication {
    fn drop(&mut self) {
        match self {
            Self::None => {}
            Self::UsernamePassword { password, .. }
            | Self::UsernamePasswordWithReuse { password, .. } => password.zeroize(),
        }
    }
}

impl fmt::Debug for InboundAuthentication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InboundAuthentication::None => write!(f, "InboundAuthentication::None"),
            InboundAuthentication::UsernamePassword { .. } => {
                write!(f, "InboundAuthentication::UsernamePassword {{ .. }}")
            }
            InboundAuthentication::UsernamePasswordWithReuse { .. } => write!(
                f,
                "InboundAuthentication::UsernamePasswordWithReuse {{ .. }}"
            ),
        }
    }
}

impl fmt::Display for InboundAuthentication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InboundAuthentication::None => write!(f, "none"),
            InboundAuthentication::UsernamePassword { .. } => write!(f, "username/password"),
            InboundAuthentication::UsernamePasswordWithReuse { .. } => {
                write!(f, "username/password with IP reuse")
            }
        }
    }
}

pub(crate) fn auth_credentials(
    auth: &InboundAuthentication,
) -> Option<(&str, &str, Option<&AuthReuseCache>)> {
    match auth {
        InboundAuthentication::None => None,
        InboundAuthentication::UsernamePassword { username, password } => {
            Some((username, password, None))
        }
        InboundAuthentication::UsernamePasswordWithReuse {
            username,
            password,
            reuse,
        } => Some((username, password, Some(reuse))),
    }
}

pub(crate) fn cached_identity(
    auth: &InboundAuthentication,
    peer_ip: Option<IpAddr>,
) -> Option<ClientIdentity> {
    let (_, _, reuse) = auth_credentials(auth)?;
    peer_ip.and_then(|ip| reuse.and_then(|cache| cache.lookup(ip)))
}

pub(crate) fn record_authenticated(
    auth: &InboundAuthentication,
    peer_ip: Option<IpAddr>,
    identity: &ClientIdentity,
) {
    let Some((_, _, Some(cache))) = auth_credentials(auth) else {
        return;
    };
    if let Some(ip) = peer_ip {
        cache.record(ip, identity.clone());
    }
}

/// Error type for accept operations.
#[derive(Debug, thiserror::Error)]

pub enum AcceptError {
    #[error("protocol error")]
    Protocol(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("authentication failed")]
    AuthenticationFailed,
}

impl From<Box<dyn std::error::Error + Send + Sync>> for AcceptError {
    fn from(e: Box<dyn std::error::Error + Send + Sync>) -> Self {
        AcceptError::Protocol(e)
    }
}

/// The result of accepting an inbound connection.
pub enum AcceptedSession {
    Tunnel(PendingTunnel),
    HttpForward(PendingHttpForward),
    UdpAssociate(PendingUdpAssociate),
    Echo(BoxStream),
}

/// A pending tunnel connection (HTTP CONNECT, SOCKS4, SOCKS5).
/// Success reply has NOT been sent yet.
pub struct PendingTunnel {
    pub target: TargetAddr,
    pub client: BoxStream,
    pub protocol: TunnelProtocol,
    pub reply_context: ReplyContext,
    pub identity: ClientIdentity,
}

/// A pending HTTP forward-proxy request.
pub struct PendingHttpForward {
    pub target: TargetAddr,
    pub client: BoxStream,
    pub request: eggress_protocol_http::forward::ForwardRequest,
    pub identity: ClientIdentity,
}

/// A pending SOCKS5 UDP ASSOCIATE session.
pub struct PendingUdpAssociate {
    pub client: BoxStream,
    pub protocol: TunnelProtocol,
    pub identity: ClientIdentity,
    pub client_hint: Option<TargetAddr>,
}

/// Which tunnel protocol was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelProtocol {
    HttpConnect,
    Http2,
    Http3,
    WebSocket,
    Socks4,
    Socks5,
    Shadowsocks,
    ShadowsocksR,
    Trojan,
    Raw,
}

/// Information needed to send a protocol-specific reply later.
pub enum ReplyContext {
    Http,
    Http2,
    Http3,
    WebSocket,
    Socks4,
    Socks5,
    Shadowsocks,
    Trojan,
    Raw,
}

/// Configuration for Shadowsocks inbound listener.
#[derive(Clone)]
pub struct InboundShadowsocksConfig {
    pub method: String,
    pub password: String,
    #[cfg(feature = "pproxy-legacy")]
    pub auth_prefix: Option<Vec<u8>>,
    #[cfg(feature = "pproxy-legacy")]
    pub plugins: Vec<String>,
}

impl Drop for InboundShadowsocksConfig {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

impl fmt::Debug for InboundShadowsocksConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("InboundShadowsocksConfig");
        ds.field("method", &self.method);
        ds.field("password", &"***");
        #[cfg(feature = "pproxy-legacy")]
        {
            ds.field("auth_prefix", &self.auth_prefix);
            ds.field("plugins", &self.plugins);
        }
        ds.finish()
    }
}

/// Configuration for Trojan inbound listener.
#[derive(Clone)]
pub struct InboundTrojanConfig {
    pub password: String,
    /// Optional fallback target for auth-failed connections.
    /// When set, connections with invalid Trojan passwords are relayed to this
    /// target instead of being rejected (matches pproxy's chaining behavior).
    pub fallback: Option<String>,
}

impl Drop for InboundTrojanConfig {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

impl fmt::Debug for InboundTrojanConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InboundTrojanConfig")
            .field("password", &"***")
            .field("fallback", &self.fallback)
            .finish()
    }
}

/// Read the first byte from the stream, detect the protocol, perform the
/// handshake parsing, and return an `AcceptedSession` **without** opening
/// any outbound connection or sending any success/failure reply.
pub async fn accept(
    client: BoxStream,
    protocols: &[ProtocolId],
    auth: &InboundAuthentication,
    shadowsocks_config: Option<&InboundShadowsocksConfig>,
    #[cfg(feature = "extended")] shadowsocks_metrics: Option<
        &std::sync::Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>,
    >,
    #[cfg(not(feature = "extended"))] shadowsocks_metrics: Option<&()>,
    trojan_config: Option<&InboundTrojanConfig>,
) -> Result<AcceptedSession, AcceptError> {
    #[cfg(not(feature = "extended"))]
    let _ = (shadowsocks_config, shadowsocks_metrics, trojan_config);
    accept_with_fixed_target(
        client,
        protocols,
        auth,
        shadowsocks_config,
        shadowsocks_metrics,
        trojan_config,
        None,
    )
    .await
}

pub async fn accept_with_fixed_target(
    client: BoxStream,
    protocols: &[ProtocolId],
    auth: &InboundAuthentication,
    shadowsocks_config: Option<&InboundShadowsocksConfig>,
    #[cfg(feature = "extended")] shadowsocks_metrics: Option<
        &std::sync::Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>,
    >,
    #[cfg(not(feature = "extended"))] shadowsocks_metrics: Option<&()>,
    trojan_config: Option<&InboundTrojanConfig>,
    fixed_target: Option<&TargetAddr>,
) -> Result<AcceptedSession, AcceptError> {
    accept_with_fixed_target_for_peer(
        client,
        protocols,
        auth,
        shadowsocks_config,
        shadowsocks_metrics,
        trojan_config,
        fixed_target,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn accept_with_fixed_target_for_peer(
    client: BoxStream,
    protocols: &[ProtocolId],
    auth: &InboundAuthentication,
    shadowsocks_config: Option<&InboundShadowsocksConfig>,
    #[cfg(feature = "extended")] shadowsocks_metrics: Option<
        &std::sync::Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>,
    >,
    #[cfg(not(feature = "extended"))] shadowsocks_metrics: Option<&()>,
    trojan_config: Option<&InboundTrojanConfig>,
    fixed_target: Option<&TargetAddr>,
    peer_ip: Option<IpAddr>,
) -> Result<AcceptedSession, AcceptError> {
    #[cfg(not(feature = "extended"))]
    let _ = (shadowsocks_config, shadowsocks_metrics, trojan_config);
    #[cfg(feature = "extended")]
    #[inline]
    fn shadows_metrics(
        m: Option<&std::sync::Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>>,
    ) -> Option<std::sync::Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>> {
        m.cloned()
    }
    let mut stream = client;
    if protocols.len() == 1 && protocols.contains(&ProtocolId::Echo) {
        return Ok(AcceptedSession::Echo(stream));
    }
    if protocols.len() == 1 && protocols.contains(&ProtocolId::Raw) {
        let target = fixed_target
            .cloned()
            .ok_or_else(|| AcceptError::Protocol("raw listener requires fixed_target".into()))?;
        return Ok(AcceptedSession::Tunnel(PendingTunnel {
            target,
            client: stream,
            protocol: TunnelProtocol::Raw,
            reply_context: ReplyContext::Raw,
            identity: ClientIdentity::Anonymous,
        }));
    }
    let mut first_byte = [0u8; 1];
    stream
        .read_exact(&mut first_byte)
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

    let has_socks5 = protocols.contains(&ProtocolId::Socks5);
    let has_socks4 = protocols.contains(&ProtocolId::Socks4);
    let has_http = protocols.contains(&ProtocolId::Http);

    // Check SOCKS5
    if first_byte[0] == 0x05 && has_socks5 {
        tracing::trace!(
            "detected protocol: socks5 (first_byte={:#04x})",
            first_byte[0]
        );
        let stream: BoxStream = Box::new(PrefixedStream::new(first_byte.to_vec(), stream));
        return accept_socks5(stream, auth, peer_ip).await;
    }

    // Check SOCKS4
    if first_byte[0] == 0x04 && has_socks4 {
        tracing::trace!(
            "detected protocol: socks4 (first_byte={:#04x})",
            first_byte[0]
        );
        let stream: BoxStream = Box::new(PrefixedStream::new(first_byte.to_vec(), stream));
        return accept_socks4(stream, auth, peer_ip).await;
    }

    // Try HTTP detection if HTTP is allowed
    if has_http {
        // Read more bytes to detect the HTTP method
        let mut prefix = vec![first_byte[0]];
        let mut buf = [0u8; 32];
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
        prefix.extend_from_slice(&buf[..n]);

        match detect_http_method(&prefix) {
            DetectResult::Match => {
                tracing::trace!(
                    "detected protocol: http (prefix={:?})",
                    &prefix[..prefix.len().min(16)]
                );
                let stream: BoxStream = Box::new(PrefixedStream::new(prefix, stream));
                return accept_http(stream, auth, peer_ip).await;
            }
            DetectResult::NeedMore => {
                // Read more bytes
                let mut more = [0u8; 32];
                let n = stream
                    .read(&mut more)
                    .await
                    .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
                prefix.extend_from_slice(&more[..n]);
                match detect_http_method(&prefix) {
                    DetectResult::Match => {
                        tracing::trace!(
                            "detected protocol: http (prefix={:?})",
                            &prefix[..prefix.len().min(16)]
                        );
                        let stream: BoxStream = Box::new(PrefixedStream::new(prefix, stream));
                        return accept_http(stream, auth, peer_ip).await;
                    }
                    DetectResult::NoMatch => {
                        return Err(AcceptError::Protocol(
                            "no matching protocol for listener".into(),
                        ));
                    }
                    DetectResult::NeedMore => {
                        return Err(AcceptError::Protocol(
                            "no matching protocol for listener".into(),
                        ));
                    }
                }
            }
            DetectResult::NoMatch => {
                return Err(AcceptError::Protocol(
                    "no matching protocol for listener".into(),
                ));
            }
        }
    }

    // Check if Shadowsocks is the only protocol (auto-detection not possible)
    #[cfg(feature = "extended")]
    if protocols.len() == 1 && protocols.contains(&ProtocolId::Shadowsocks) {
        if let Some(ss_config) = shadowsocks_config {
            let stream: BoxStream = Box::new(PrefixedStream::new(first_byte.to_vec(), stream));
            match eggress_protocol_shadowsocks::CipherMethod::parse_method(&ss_config.method) {
                Ok(method) => {
                    let (ss_stream, target_addr) =
                        eggress_protocol_shadowsocks::tcp::shadowsocks_accept(
                            stream,
                            &ss_config.password,
                            method,
                            shadows_metrics(shadowsocks_metrics),
                        )
                        .await
                        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

                    return Ok(AcceptedSession::Tunnel(PendingTunnel {
                        target: target_addr,
                        client: ss_stream,
                        protocol: TunnelProtocol::Shadowsocks,
                        reply_context: ReplyContext::Shadowsocks,
                        identity: ClientIdentity::Anonymous,
                    }));
                }
                Err(modern_error) => {
                    #[cfg(feature = "legacy-crypto")]
                    if let Ok(legacy_method) =
                        eggress_protocol_shadowsocks::legacy::LegacyMethod::parse(&ss_config.method)
                    {
                        let (ss_stream, target_addr) =
                            eggress_protocol_shadowsocks::legacy::legacy_accept(
                                stream,
                                legacy_method,
                                ss_config.password.as_bytes(),
                            )
                            .await
                            .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

                        return Ok(AcceptedSession::Tunnel(PendingTunnel {
                            target: target_addr,
                            client: ss_stream,
                            protocol: TunnelProtocol::Shadowsocks,
                            reply_context: ReplyContext::Shadowsocks,
                            identity: ClientIdentity::Anonymous,
                        }));
                    }
                    if let Some(m) = shadowsocks_metrics {
                        m.record_tcp_unsupported_method_reject();
                    }
                    return Err(AcceptError::Protocol(Box::new(modern_error)));
                }
            }
        }
        return Err(AcceptError::Protocol(
            "shadowsocks listener requires shadowsocks config".into(),
        ));
    }

    #[cfg(feature = "pproxy-legacy")]
    if protocols.len() == 1 && protocols.contains(&ProtocolId::ShadowsocksR) {
        let ssr_config = shadowsocks_config
            .filter(|config| config.method == "ssr")
            .ok_or_else(|| AcceptError::Protocol("SSR listener requires SSR config".into()))?;
        let plugins =
            eggress_protocol_shadowsocks::compat::plugin::parse_plugins(&ssr_config.plugins)
                .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
        let stream: BoxStream = Box::new(PrefixedStream::new(first_byte.to_vec(), stream));
        let (ss_stream, target_addr) = eggress_protocol_shadowsocks::compat::ssr::ssr_accept(
            stream,
            &eggress_protocol_shadowsocks::compat::ssr::SsrConfig {
                auth_prefix: ssr_config.auth_prefix.clone(),
                plugins,
            },
        )
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
        return Ok(AcceptedSession::Tunnel(PendingTunnel {
            target: target_addr,
            client: ss_stream,
            protocol: TunnelProtocol::ShadowsocksR,
            reply_context: ReplyContext::Shadowsocks,
            identity: ClientIdentity::Anonymous,
        }));
    }
    #[cfg(not(feature = "pproxy-legacy"))]
    if protocols.len() == 1 && protocols.contains(&ProtocolId::ShadowsocksR) {
        return Err(AcceptError::Protocol(
            "SSR compatibility support is not included in this build".into(),
        ));
    }
    #[cfg(not(feature = "extended"))]
    if protocols.len() == 1 && protocols.contains(&ProtocolId::Shadowsocks) {
        return Err(AcceptError::Protocol(
            "shadowsocks support not included in this build".into(),
        ));
    }

    // Check if Trojan is the only protocol (TLS termination already happened upstream)
    #[cfg(feature = "extended")]
    if protocols.len() == 1 && protocols.contains(&ProtocolId::Trojan) {
        if let Some(trojan_cfg) = trojan_config {
            use tokio::io::AsyncReadExt;

            // Read the 56-byte hash prefix to check password before consuming
            // the rest of the handshake. This enables fallback routing on auth
            // failure without consuming bytes needed by the fallback target.
            let mut hash_prefix = [0u8; 56];
            // The protocol detector already consumed the first hash byte.
            // Preserve it so password verification and the full Trojan parser
            // see the original 56-byte hash.
            hash_prefix[0] = first_byte[0];
            stream
                .read_exact(&mut hash_prefix[1..])
                .await
                .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

            let password_matches =
                eggress_protocol_trojan::trojan_check_password(&hash_prefix, &trojan_cfg.password);

            if password_matches {
                // Replay the 56-byte hash so trojan_accept reads the full handshake
                let prefixed = PrefixedStream::new(hash_prefix.to_vec(), stream);
                let boxed: BoxStream = Box::new(prefixed);
                let (trojan_stream, result) =
                    eggress_protocol_trojan::trojan_accept(boxed, &trojan_cfg.password)
                        .await
                        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

                return Ok(AcceptedSession::Tunnel(PendingTunnel {
                    target: result.target,
                    client: trojan_stream,
                    protocol: TunnelProtocol::Trojan,
                    reply_context: ReplyContext::Trojan,
                    identity: ClientIdentity::Anonymous,
                }));
            }

            // Password did not match — check for fallback routing
            if let Some(ref fallback_target) = trojan_cfg.fallback {
                // The password hash and its CRLF delimiter belong to the
                // rejected Trojan handshake, not to the fallback protocol.
                // Consume the delimiter and let the fallback see only the
                // application bytes that follow it.
                let mut delimiter = [0u8; 2];
                stream
                    .read_exact(&mut delimiter)
                    .await
                    .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
                if delimiter != *b"\r\n" {
                    tracing::warn!(?delimiter, "trojan fallback delimiter was not CRLF");
                }
                let target: TargetAddr = fallback_target.parse().map_err(|e: String| {
                    AcceptError::Protocol(format!("invalid trojan fallback address: {e}").into())
                })?;
                tracing::debug!("trojan auth failed, falling back to {}", fallback_target);
                return Ok(AcceptedSession::Tunnel(PendingTunnel {
                    target,
                    client: stream,
                    protocol: TunnelProtocol::Trojan,
                    reply_context: ReplyContext::Trojan,
                    identity: ClientIdentity::Anonymous,
                }));
            }

            return Err(AcceptError::AuthenticationFailed);
        }
        return Err(AcceptError::Protocol(
            "trojan listener requires trojan config".into(),
        ));
    }
    #[cfg(not(feature = "extended"))]
    if protocols.len() == 1 && protocols.contains(&ProtocolId::Trojan) {
        return Err(AcceptError::Protocol(
            "trojan support not included in this build".into(),
        ));
    }

    Err(AcceptError::Protocol(
        "no matching protocol for listener".into(),
    ))
}
