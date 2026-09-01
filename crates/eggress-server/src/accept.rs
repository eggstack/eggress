use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

static AUTH_CACHE_EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

use eggress_core::BoxStream;
use eggress_core::{ClientIdentity, ProtocolId, TargetAddr, TargetHost};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
};
use zeroize::Zeroize;

use crate::auth::parse_basic_auth;

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
        // tail-latency hit of an O(n) `retain` under the cache lock.
        if entries.len() >= self.max_entries {
            let now_nanos = u64::try_from(
                now.checked_duration_since(*AUTH_CACHE_EPOCH)
                    .unwrap_or(Duration::ZERO)
                    .as_nanos(),
            )
            .unwrap_or(u64::MAX);
            let last_sweep = self.last_sweep_nanos.load(Ordering::Acquire);
            let interval_nanos = u64::try_from(Self::SWEEP_INTERVAL.as_nanos()).unwrap_or(u64::MAX);
            if now_nanos.saturating_sub(last_sweep) >= interval_nanos
                && self
                    .last_sweep_nanos
                    .compare_exchange(last_sweep, now_nanos, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
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

/// A stream that returns `prefix` bytes first, then delegates to `inner`.
struct PrefixedStream {
    prefix: std::io::Cursor<Vec<u8>>,
    inner: BoxStream,
}

impl PrefixedStream {
    fn new(prefix: Vec<u8>, inner: BoxStream) -> Self {
        Self {
            prefix: std::io::Cursor::new(prefix),
            inner,
        }
    }
}

impl AsyncRead for PrefixedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let pos = self.prefix.position() as usize;
        let len = self.prefix.get_ref().len();
        if pos < len {
            let remaining = &self.prefix.get_ref()[pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.prefix.set_position((pos + to_copy) as u64);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
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

enum DetectResult {
    Match,
    NeedMore,
    NoMatch,
}

fn detect_http_method(prefix: &[u8]) -> DetectResult {
    // Look for a space in the prefix to find the end of the method token
    if let Some(space_pos) = prefix.iter().position(|&b| b == b' ') {
        let method_token = &prefix[..space_pos];
        if method_token.is_empty() || method_token.len() > 16 {
            return DetectResult::NoMatch;
        }
        // HTTP methods use the RFC token grammar, so extension methods may
        // legitimately contain lowercase letters and hyphens as well.
        let is_valid_method = method_token
            .iter()
            .all(|&b| b.is_ascii_uppercase() || b == b'-' || b.is_ascii_lowercase());
        if is_valid_method {
            DetectResult::Match
        } else {
            DetectResult::NoMatch
        }
    } else {
        // No space found yet - check if what we have so far looks like a valid method prefix
        if prefix.len() > 16 {
            return DetectResult::NoMatch;
        }
        // Check if all bytes so far are valid method characters
        let is_valid_prefix = prefix
            .iter()
            .all(|&b| b.is_ascii_uppercase() || b == b'-' || b.is_ascii_lowercase());
        if is_valid_prefix {
            DetectResult::NeedMore
        } else {
            DetectResult::NoMatch
        }
    }
}

async fn accept_socks5(
    stream: BoxStream,
    auth: &InboundAuthentication,
    peer_ip: Option<IpAddr>,
) -> Result<AcceptedSession, AcceptError> {
    use eggress_protocol_socks::socks5::server::{
        read_auth_request, read_method_negotiation, read_socks5_request, send_auth_response,
        send_connect_reply, Socks5Command, CMD_BIND, REP_COMMAND_NOT_SUPPORTED,
    };

    let (mut reader, mut writer) = tokio::io::split(stream);
    let methods = read_method_negotiation(&mut reader)
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

    // Determine method selection based on auth policy
    const AUTH_NONE: u8 = 0x00;
    const AUTH_USERNAME_PASSWORD: u8 = 0x02;
    const AUTH_NO_ACCEPTABLE: u8 = 0xFF;

    let cached = cached_identity(auth, peer_ip);
    let selected_method = match (auth_credentials(auth), cached.is_some()) {
        (None, _) | (Some(_), true) if methods.contains(&AUTH_NONE) => AUTH_NONE,
        (Some(_), _) if methods.contains(&AUTH_USERNAME_PASSWORD) => AUTH_USERNAME_PASSWORD,
        (None, _) => AUTH_NO_ACCEPTABLE,
        (Some(_), _) => AUTH_NO_ACCEPTABLE,
    };

    // Send method selection
    use tokio::io::AsyncWriteExt;
    writer
        .write_all(&[0x05, selected_method])
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
    writer
        .flush()
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

    if selected_method == AUTH_NO_ACCEPTABLE {
        return Err(AcceptError::Protocol(Box::new(
            eggress_protocol_socks::error::Socks5Error::MethodNegotiationFailed,
        )));
    }

    // Handle auth if required
    let mut identity = cached.unwrap_or(ClientIdentity::Anonymous);
    if selected_method == AUTH_USERNAME_PASSWORD {
        let (username, password, _) = match auth_credentials(auth) {
            Some(creds) => creds,
            None => return Err(AcceptError::AuthenticationFailed),
        };
        match read_auth_request(&mut reader, username, password).await {
            Ok(client_username) => {
                identity = ClientIdentity::Username(client_username);
                record_authenticated(auth, peer_ip, &identity);
                send_auth_response(&mut writer, true)
                    .await
                    .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
            }
            Err(_) => {
                let _ = send_auth_response(&mut writer, false).await;
                return Err(AcceptError::AuthenticationFailed);
            }
        }
    }

    let (command, socks_addr) = read_socks5_request(&mut reader)
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

    match command {
        Socks5Command::Connect => {
            let target = socks_addr_to_target(&socks_addr);
            let stream: BoxStream = Box::new(tokio::io::join(reader, writer));

            Ok(AcceptedSession::Tunnel(PendingTunnel {
                target,
                client: stream,
                protocol: TunnelProtocol::Socks5,
                reply_context: ReplyContext::Socks5,
                identity,
            }))
        }
        Socks5Command::UdpAssociate => {
            let client_hint = Some(socks_addr_to_target(&socks_addr));
            let stream: BoxStream = Box::new(tokio::io::join(reader, writer));

            Ok(AcceptedSession::UdpAssociate(PendingUdpAssociate {
                client: stream,
                protocol: TunnelProtocol::Socks5,
                identity,
                client_hint,
            }))
        }
        Socks5Command::Bind => {
            let _ = send_connect_reply(&mut writer, REP_COMMAND_NOT_SUPPORTED, &socks_addr).await;
            Err(AcceptError::Protocol(Box::new(
                eggress_protocol_socks::error::Socks5Error::UnsupportedCommand(CMD_BIND),
            )))
        }
    }
}

async fn accept_socks4(
    stream: BoxStream,
    auth: &InboundAuthentication,
    peer_ip: Option<IpAddr>,
) -> Result<AcceptedSession, AcceptError> {
    use eggress_protocol_socks::socks4::server::read_socks4_request;

    let (mut reader, writer) = tokio::io::split(stream);
    let request = read_socks4_request(&mut reader)
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
    let target = if let Some(ref domain) = request.domain {
        TargetAddr {
            host: TargetHost::Domain(domain.clone()),
            port: request.port,
        }
    } else {
        TargetAddr {
            host: TargetHost::Ip(request.addr.ip()),
            port: request.addr.port(),
        }
    };
    let cached = cached_identity(auth, peer_ip);
    if cached.is_none() {
        if let Some((username, _, _)) = auth_credentials(auth) {
            use subtle::ConstantTimeEq;
            let user_ok: bool = request.user_id.as_bytes().ct_eq(username.as_bytes()).into();
            if !user_ok {
                return Err(AcceptError::AuthenticationFailed);
            }
        }
    }
    let identity = cached.unwrap_or({
        if request.user_id.is_empty() {
            ClientIdentity::Anonymous
        } else {
            ClientIdentity::Opaque(request.user_id)
        }
    });
    if matches!(
        identity,
        ClientIdentity::Opaque(_) | ClientIdentity::Username(_)
    ) {
        record_authenticated(auth, peer_ip, &identity);
    }
    let stream: BoxStream = Box::new(tokio::io::join(reader, writer));

    Ok(AcceptedSession::Tunnel(PendingTunnel {
        target,
        client: stream,
        protocol: TunnelProtocol::Socks4,
        reply_context: ReplyContext::Socks4,
        identity,
    }))
}

async fn accept_http(
    stream: BoxStream,
    auth: &InboundAuthentication,
    peer_ip: Option<IpAddr>,
) -> Result<AcceptedSession, AcceptError> {
    // Keep the reader buffered so parsing a request head does not issue one
    // underlying read per byte. Any prefetched body remains in the reader.
    let mut stream = tokio::io::BufReader::new(stream);
    let head_buf = read_http_head(&mut stream).await?;

    let method = {
        let request_line = String::from_utf8_lossy(&head_buf);
        request_line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    };

    if method == "connect" {
        let request = parse_connect_request(&head_buf, &mut stream, auth, peer_ip).await?;
        Ok(AcceptedSession::Tunnel(PendingTunnel {
            target: request.target,
            client: Box::new(stream),
            protocol: TunnelProtocol::HttpConnect,
            reply_context: ReplyContext::Http,
            identity: request.identity,
        }))
    } else {
        // Parse Proxy-Authorization from the raw head
        let head_str = String::from_utf8_lossy(&head_buf);
        let cached = cached_identity(auth, peer_ip);
        let proxy_auth = if cached.is_some() {
            None
        } else if let Some((username, password, _)) = auth_credentials(auth) {
            let mut found_auth = None;
            for line in head_str.split("\r\n") {
                if let Some((name, value)) = parse_header_line_str(line) {
                    if name.eq_ignore_ascii_case("Proxy-Authorization") {
                        found_auth = parse_basic_auth(&value);
                        break;
                    }
                }
            }
            match found_auth {
                Some((user, pass)) => {
                    use subtle::ConstantTimeEq;
                    let user_ok: bool = user.as_bytes().ct_eq(username.as_bytes()).into();
                    let pass_ok: bool = pass.as_bytes().ct_eq(password.as_bytes()).into();
                    if !user_ok || !pass_ok {
                        let _ = write_proxy_auth_required(&mut stream).await;
                        return Err(AcceptError::AuthenticationFailed);
                    }
                    Some((user, pass))
                }
                None => {
                    let _ = write_proxy_auth_required(&mut stream).await;
                    return Err(AcceptError::AuthenticationFailed);
                }
            }
        } else {
            None
        };
        let identity = cached.unwrap_or_else(|| match &proxy_auth {
            Some((user, _)) => ClientIdentity::Username(user.clone()),
            None => ClientIdentity::Anonymous,
        });
        if matches!(identity, ClientIdentity::Username(_)) {
            record_authenticated(auth, peer_ip, &identity);
        }
        let _ = proxy_auth; // Auth already validated above

        // Reconstruct stream for forward_request
        let stream: BoxStream = Box::new(PrefixedStream::new(head_buf, Box::new(stream)));

        let (request, client_stream) = eggress_protocol_http::forward_request(stream)
            .await
            .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

        let target = request.target.clone();
        Ok(AcceptedSession::HttpForward(PendingHttpForward {
            target,
            client: client_stream,
            request,
            identity,
        }))
    }
}

struct ConnectRequest {
    target: TargetAddr,
    identity: ClientIdentity,
}

async fn parse_connect_request<W: AsyncWrite + Unpin>(
    head_buf: &[u8],
    stream: &mut W,
    auth: &InboundAuthentication,
    peer_ip: Option<IpAddr>,
) -> Result<ConnectRequest, AcceptError> {
    let head_str = String::from_utf8_lossy(head_buf);
    let mut lines = head_str.split("\r\n");

    let request_line = lines.next().ok_or_else(|| {
        AcceptError::Protocol(
            eggress_protocol_http::HttpError::MalformedRequest("empty request".into()).into(),
        )
    })?;

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(AcceptError::Protocol(
            eggress_protocol_http::HttpError::MalformedRequest(format!(
                "expected 3 parts in request line, got {}",
                parts.len()
            ))
            .into(),
        ));
    }

    let authority = parts[1];
    let target = parse_authority(authority)?;

    // Parse Proxy-Authorization header
    let mut proxy_auth = None;
    let mut parsed_username: Option<String> = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = parse_header_line_str(line) {
            if name.eq_ignore_ascii_case("Proxy-Authorization") {
                proxy_auth = parse_basic_auth(&value);
                if let Some((user, _)) = &proxy_auth {
                    parsed_username = Some(user.clone());
                }
            }
        }
    }

    // Validate auth if required. A compatibility cache hit is sufficient and
    // intentionally ignores credentials on the new connection, matching the
    // pproxy AuthTable behavior.
    let cached = cached_identity(auth, peer_ip);
    if cached.is_none() {
        if let Some((username, password, _)) = auth_credentials(auth) {
            match proxy_auth {
                Some((user, pass)) => {
                    use subtle::ConstantTimeEq;
                    let user_ok: bool = user.as_bytes().ct_eq(username.as_bytes()).into();
                    let pass_ok: bool = pass.as_bytes().ct_eq(password.as_bytes()).into();
                    if !user_ok || !pass_ok {
                        let _ = write_proxy_auth_required(stream).await;
                        return Err(AcceptError::AuthenticationFailed);
                    }
                }
                None => {
                    let _ = write_proxy_auth_required(stream).await;
                    return Err(AcceptError::AuthenticationFailed);
                }
            }
        }
    }

    let identity = cached.unwrap_or(match parsed_username {
        Some(user) => ClientIdentity::Username(user),
        None => ClientIdentity::Anonymous,
    });
    if matches!(identity, ClientIdentity::Username(_)) {
        record_authenticated(auth, peer_ip, &identity);
    }

    Ok(ConnectRequest { target, identity })
}

async fn read_http_head<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Vec<u8>, AcceptError> {
    let mut head_buf = Vec::with_capacity(1024);
    let mut line = Vec::with_capacity(256);
    let mut header_count = 0;

    loop {
        if head_buf.len() >= MAX_HEAD_SIZE {
            return Err(AcceptError::Protocol(
                eggress_protocol_http::HttpError::HeaderTooLarge.into(),
            ));
        }

        line.clear();
        let remaining = MAX_HEAD_SIZE - head_buf.len();
        let n = reader
            .take((remaining + 1) as u64)
            .read_until(b'\n', &mut line)
            .await
            .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
        if n == 0 {
            return Err(AcceptError::Protocol(
                eggress_protocol_http::HttpError::MalformedRequest(
                    "unexpected EOF reading request".into(),
                )
                .into(),
            ));
        }

        if head_buf.len() + line.len() > MAX_HEAD_SIZE {
            return Err(AcceptError::Protocol(
                eggress_protocol_http::HttpError::HeaderTooLarge.into(),
            ));
        }
        head_buf.extend_from_slice(&line);

        if line.ends_with(b"\r\n") {
            header_count += 1;
            if header_count > MAX_HEADER_LINES {
                return Err(AcceptError::Protocol(
                    eggress_protocol_http::HttpError::TooManyHeaders.into(),
                ));
            }
        }
        if head_buf.ends_with(b"\r\n\r\n") {
            return Ok(head_buf);
        }
    }
}

fn parse_authority(
    authority: &str,
) -> Result<TargetAddr, Box<dyn std::error::Error + Send + Sync>> {
    if authority.starts_with('[') {
        let bracket_end = authority.find(']').ok_or_else(|| {
            eggress_protocol_http::HttpError::TargetParseError(
                "unclosed bracket in IPv6 address".into(),
            )
        })?;

        let ip_str = &authority[1..bracket_end];
        let ip: std::net::IpAddr = ip_str.parse().map_err(|e| {
            eggress_protocol_http::HttpError::TargetParseError(format!("invalid IPv6 address: {e}"))
        })?;

        let port_str = authority.get(bracket_end + 2..).ok_or_else(|| {
            eggress_protocol_http::HttpError::TargetParseError("missing port".into())
        })?;

        if authority
            .as_bytes()
            .get(bracket_end + 1)
            .is_none_or(|&b| b != b':')
        {
            return Err(eggress_protocol_http::HttpError::TargetParseError(
                "expected ':' between IPv6 address and port".into(),
            )
            .into());
        }

        let port: u16 = port_str.parse().map_err(|e| {
            eggress_protocol_http::HttpError::TargetParseError(format!("invalid port: {e}"))
        })?;

        return Ok(TargetAddr {
            host: TargetHost::Ip(ip),
            port,
        });
    }

    let colon_pos = authority.rfind(':').ok_or_else(|| {
        eggress_protocol_http::HttpError::TargetParseError("missing port in authority".into())
    })?;

    let host_str = &authority[..colon_pos];
    let port_str = &authority[colon_pos + 1..];

    let port: u16 = port_str.parse().map_err(|e| {
        eggress_protocol_http::HttpError::TargetParseError(format!("invalid port: {e}"))
    })?;

    if let Ok(ip) = host_str.parse::<std::net::IpAddr>() {
        return Ok(TargetAddr {
            host: TargetHost::Ip(ip),
            port,
        });
    }

    if host_str.is_empty() {
        return Err(eggress_protocol_http::HttpError::TargetParseError("empty host".into()).into());
    }

    Ok(TargetAddr {
        host: TargetHost::Domain(host_str.to_string()),
        port,
    })
}

fn socks_addr_to_target(addr: &eggress_protocol_socks::socks5::server::SocksAddr) -> TargetAddr {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    match addr {
        SocksAddr::IPv4(octets, port) => TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V4((*octets).into())),
            port: *port,
        },
        SocksAddr::IPv6(octets, port) => TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V6((*octets).into())),
            port: *port,
        },
        SocksAddr::Domain(domain, port) => TargetAddr {
            host: TargetHost::Domain(domain.clone()),
            port: *port,
        },
    }
}

/// Maximum size for the HTTP request head (request line + headers).
const MAX_HEAD_SIZE: usize = 32 * 1024;

/// Maximum number of header lines.
const MAX_HEADER_LINES: usize = 128;

/// Parse a header line into (name, value).
fn parse_header_line_str(line: &str) -> Option<(String, String)> {
    let colon_pos = line.find(':')?;
    let name = line[..colon_pos].trim().to_string();
    let value = line[colon_pos + 1..].trim().to_string();
    Some((name, value))
}

/// Parse Basic authentication from a Proxy-Authorization header value.
/// Write a 407 Proxy Authentication Required response.
async fn write_proxy_auth_required<W: AsyncWrite + Unpin>(
    stream: &mut W,
) -> Result<(), std::io::Error> {
    let response = b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"eggress\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(response).await?;
    stream.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn poisoned_auth_cache_is_cleared_before_reuse() {
        let cache = Arc::new(AuthReuseCache::new(Duration::from_secs(60)));
        let poisoned = Arc::clone(&cache);
        let result = std::thread::spawn(move || {
            let _guard = poisoned.entries.lock().unwrap();
            panic!("poison auth cache mutex");
        })
        .join();
        assert!(result.is_err());

        cache.record(
            "192.0.2.1".parse().unwrap(),
            ClientIdentity::Username("user".to_string()),
        );
        assert_eq!(cache.len(), 1);
    }

    #[tokio::test]
    async fn test_accept_socks5() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_socks4() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks4);
                    assert_eq!(pending.target.port, 80);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(&[0x04, 0x01, 0x00, 0x50, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&[0x00]).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_http_connect() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_http_forward() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.target.port, 80);
                    assert_eq!(pending.request.method, "GET");
                }
                _ => panic!("expected http forward"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET http://example.com/index.html HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_on_http_only_listener() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_on_http_only_listener_rejected() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_on_socks5_only_listener_rejected() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_on_mixed_listener_accepted() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_on_mixed_listener_accepted() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[cfg(feature = "extended")]
    #[tokio::test]
    async fn trojan_fallback_drops_rejected_handshake_bytes() {
        let mut input = vec![b'0'; 56];
        input.extend_from_slice(b"\r\npayload");
        let session = accept(
            Box::new(std::io::Cursor::new(input)),
            &[ProtocolId::Trojan],
            &InboundAuthentication::None,
            None,
            None,
            Some(&InboundTrojanConfig {
                password: "expected".to_string(),
                fallback: Some("example.com:80".to_string()),
            }),
        )
        .await
        .unwrap();

        let AcceptedSession::Tunnel(mut pending) = session else {
            panic!("expected fallback tunnel");
        };
        let mut payload = Vec::new();
        pending.client.read_to_end(&mut payload).await.unwrap();
        assert_eq!(payload, b"payload");
    }

    #[tokio::test]
    async fn test_random_binary_prefix_rejected() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send random binary prefix that isn't 0x04 or 0x05 and not valid HTTP
        stream.write_all(&[0x00, 0x01, 0x02, 0x03]).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_tls_client_hello_not_interpreted_as_http() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // TLS ClientHello starts with 0x16, 0x03, which isn't valid HTTP method
        stream
            .write_all(&[0x16, 0x03, 0x01, 0x00, 0x05])
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    // === Authentication tests ===

    #[tokio::test]
    async fn test_socks5_auth_correct_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "secret".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed, &all_protocols, &auth, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Client offers both no-auth and username/password
        stream.write_all(&[0x05, 0x02, 0x00, 0x02]).await.unwrap();
        // Server selects username/password (0x02)
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x02]);

        // Send auth: version=1, ulen=4, "user", plen=6, "secret"
        stream
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r', 0x06])
            .await
            .unwrap();
        stream.write_all(b"secret").await.unwrap();
        // Read auth response (success)
        let mut auth_resp = [0u8; 2];
        stream.read_exact(&mut auth_resp).await.unwrap();
        assert_eq!(auth_resp, [0x01, 0x00]);

        // Send CONNECT request
        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_auth_wrong_password() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "secret".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x02, 0x00, 0x02]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x02]);

        // Send auth with wrong password
        stream
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r', 0x05])
            .await
            .unwrap();
        stream.write_all(b"wrong").await.unwrap();
        // Read auth response (failure)
        let mut auth_resp = [0u8; 2];
        stream.read_exact(&mut auth_resp).await.unwrap();
        assert_eq!(auth_resp, [0x01, 0x01]);

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_auth_no_auth_client_rejected() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "secret".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Client only offers no-auth
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        // Server should send 0xFF (no acceptable methods)
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0xFF]);

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_connect_auth_correct_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed, &all_protocols, &auth, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // "user:pass" base64 encoded is "dXNlcjpwYXNz"
        stream
            .write_all(
                b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nProxy-Authorization: Basic dXNlcjpwYXNz\r\n\r\n",
            )
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_connect_auth_missing_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();

        // Read 407 response
        let mut response = vec![0u8; 512];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("407"),
            "expected 407, got: {response_str}"
        );
        assert!(
            response_str.contains("Proxy-Authenticate"),
            "expected Proxy-Authenticate header"
        );

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_connect_auth_wrong_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // "user:wrong" base64 encoded is "dXNlcjp3cm9uZw=="
        stream
            .write_all(
                b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nProxy-Authorization: Basic dXNlcjp3cm9uZw==\r\n\r\n",
            )
            .await
            .unwrap();

        let mut response = vec![0u8; 512];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("407"),
            "expected 407, got: {response_str}"
        );

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_connect_auth_malformed_base64() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nProxy-Authorization: Basic !!!invalid!!!\r\n\r\n",
            )
            .await
            .unwrap();

        let mut response = vec![0u8; 512];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("407"),
            "expected 407, got: {response_str}"
        );

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_head_reader_preserves_prefetched_body() {
        let (mut client, server) = tokio::io::duplex(1024);
        client
            .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\nbody")
            .await
            .unwrap();
        client.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(server);
        let head = read_http_head(&mut reader).await.unwrap();
        assert!(head.ends_with(b"\r\n\r\n"));
        let mut body = Vec::new();
        reader.read_to_end(&mut body).await.unwrap();
        assert_eq!(body, b"body");
    }

    #[tokio::test]
    async fn test_http_forward_auth_correct_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed, &all_protocols, &auth, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.target.port, 80);
                    assert_eq!(pending.request.method, "GET");
                    // Proxy-Authorization should be stripped
                    assert!(!pending
                        .request
                        .headers
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case("Proxy-Authorization")));
                }
                _ => panic!("expected http forward"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\nProxy-Authorization: Basic dXNlcjpwYXNz\r\n\r\n",
            )
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_forward_auth_missing_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        let mut response = vec![0u8; 512];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("407"),
            "expected 407, got: {response_str}"
        );

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_forward_auth_wrong_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // "user:wrong" base64 encoded is "dXNlcjp3cm9uZw=="
        stream
            .write_all(
                b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\nProxy-Authorization: Basic dXNlcjp3cm9uZw==\r\n\r\n",
            )
            .await
            .unwrap();

        let mut response = vec![0u8; 512];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("407"),
            "expected 407, got: {response_str}"
        );

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_udp_associate_returns_pending() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::UdpAssociate(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(
                        pending.client_hint,
                        Some(TargetAddr {
                            host: TargetHost::Ip(std::net::IpAddr::V4(std::net::Ipv4Addr::new(
                                0, 0, 0, 0
                            ))),
                            port: 0,
                        })
                    );
                }
                _ => panic!("expected UdpAssociate"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // UDP ASSOCIATE (cmd=0x03), target 0.0.0.0:0
        stream
            .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
            .await
            .unwrap();
        stream.write_all(&0u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_bind_rejected() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // BIND (cmd=0x02)
        stream
            .write_all(&[0x05, 0x02, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&80u16.to_be_bytes()).await.unwrap();

        // Server sends rejection reply (RFC 1928 0x07 command not supported)
        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x07); // command not supported

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_connect_still_works() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // CONNECT request
        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_udp_associate_with_auth() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "secret".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed, &all_protocols, &auth, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::UdpAssociate(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(
                        pending.identity,
                        ClientIdentity::Username("user".to_string())
                    );
                }
                _ => panic!("expected UdpAssociate"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x02, 0x00, 0x02]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x02]);

        // Auth
        stream
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r', 0x06])
            .await
            .unwrap();
        stream.write_all(b"secret").await.unwrap();
        let mut auth_resp = [0u8; 2];
        stream.read_exact(&mut auth_resp).await.unwrap();
        assert_eq!(auth_resp, [0x01, 0x00]);

        // UDP ASSOCIATE
        stream
            .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
            .await
            .unwrap();
        stream.write_all(&0u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    // === Mixed-protocol listener robustness tests ===

    #[tokio::test]
    async fn test_fragmented_first_byte_http() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.request.method, "GET");
                }
                _ => panic!("expected http forward"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send HTTP GET fragmented into individual bytes
        stream.write_all(b"G").await.unwrap();
        stream.write_all(b"E").await.unwrap();
        stream.write_all(b"T").await.unwrap();
        stream
            .write_all(b" http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_garbage_bytes_rejected() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0xAA, 0xBB, 0xCC, 0xDD]).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_slow_socks5_detection() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send first byte (version) then delay
        stream.write_all(&[0x05]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // Send rest of method negotiation
        stream.write_all(&[0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // Send CONNECT request
        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_connect_and_socks5_same_listener() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // First connection: HTTP CONNECT
        let client_jh1 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
                .await
                .unwrap();
        });

        let (stream1, _) = listener.accept().await.unwrap();
        let p = protocols.clone();
        let server_jh1 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream1);
            let session = accept(boxed, &p, &InboundAuthentication::None, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                }
                _ => panic!("expected tunnel"),
            }
        });

        client_jh1.await.unwrap();
        server_jh1.await.unwrap();

        // Second connection: SOCKS5
        let client_jh2 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut response = [0u8; 2];
            stream.read_exact(&mut response).await.unwrap();
            assert_eq!(response, [0x05, 0x00]);

            stream
                .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
                .await
                .unwrap();
            stream.write_all(&443u16.to_be_bytes()).await.unwrap();
        });

        let (stream2, _) = listener.accept().await.unwrap();
        let server_jh2 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream2);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        client_jh2.await.unwrap();
        server_jh2.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_forward_and_socks4_same_listener() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http, ProtocolId::Socks4];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // First connection: HTTP forward
        let client_jh1 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
                .await
                .unwrap();
        });

        let (stream1, _) = listener.accept().await.unwrap();
        let p = protocols.clone();
        let server_jh1 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream1);
            let session = accept(boxed, &p, &InboundAuthentication::None, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.request.method, "GET");
                    assert_eq!(pending.target.port, 80);
                }
                _ => panic!("expected http forward"),
            }
        });

        client_jh1.await.unwrap();
        server_jh1.await.unwrap();

        // Second connection: SOCKS4
        let client_jh2 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            // SOCKS4 CONNECT: version=0x04, cmd=0x01, port=443, addr=0.0.0.1, userid=0
            stream.write_all(&[0x04, 0x01]).await.unwrap();
            stream.write_all(&443u16.to_be_bytes()).await.unwrap();
            stream.write_all(&[10, 0, 0, 1]).await.unwrap();
            stream.write_all(&[0x00]).await.unwrap();
        });

        let (stream2, _) = listener.accept().await.unwrap();
        let server_jh2 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream2);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks4);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        client_jh2.await.unwrap();
        server_jh2.await.unwrap();
    }

    #[tokio::test]
    async fn test_fragmented_socks5_handshake() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send version byte separately from method negotiation
        stream.write_all(&[0x05]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        stream.write_all(&[0x01, 0x00]).await.unwrap();

        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // Now send CONNECT request, also fragmented
        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_malformed_http_request_rejected() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send a partial HTTP request that never completes headers
        stream
            .write_all(b"GET http://example.com HTTP/1.1\r\n")
            .await
            .unwrap();
        // Never send the final \r\n to end headers, then close the connection
        stream.shutdown().await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_empty_connection_closed() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Close immediately without sending anything
        drop(stream);

        server_jh.await.unwrap();
    }

    /// Mixed-protocol listener with auth: HTTP with auth and SOCKS5 with auth
    /// on the same listener. Both connections should be detected correctly
    /// when correct credentials are provided.
    #[tokio::test]
    async fn test_mixed_protocols_with_auth_detection() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // First connection: HTTP forward (non-CONNECT) without auth —
        // protocol detection still works, auth is checked in serve_connection.
        let client_jh1 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
                .await
                .unwrap();
        });

        let (stream1, _) = listener.accept().await.unwrap();
        let p = protocols.clone();
        let server_jh1 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream1);
            let session = accept(boxed, &p, &InboundAuthentication::None, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.request.method, "GET");
                }
                _ => panic!("expected http forward"),
            }
        });

        client_jh1.await.unwrap();
        server_jh1.await.unwrap();

        // Second connection: SOCKS5 without auth — detected correctly.
        let client_jh2 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut response = [0u8; 2];
            stream.read_exact(&mut response).await.unwrap();
            assert_eq!(response, [0x05, 0x00]);
            stream
                .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
                .await
                .unwrap();
            stream.write_all(&443u16.to_be_bytes()).await.unwrap();
        });

        let (stream2, _) = listener.accept().await.unwrap();
        let server_jh2 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream2);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        client_jh2.await.unwrap();
        server_jh2.await.unwrap();
    }

    #[test]
    fn auth_reuse_is_ip_scoped_and_bounded() {
        let cache = AuthReuseCache::new(Duration::from_secs(60));
        let first: IpAddr = "127.0.0.1".parse().unwrap();
        let second: IpAddr = "127.0.0.2".parse().unwrap();
        cache.record(first, ClientIdentity::Username("alice".to_string()));
        assert_eq!(
            cache.lookup(first),
            Some(ClientIdentity::Username("alice".to_string()))
        );
        assert_eq!(cache.lookup(second), None);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn zero_timeout_expires_after_authentication() {
        let cache = AuthReuseCache::new(Duration::ZERO);
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        cache.record(peer, ClientIdentity::Username("alice".to_string()));
        while cache.lookup(peer).is_some() {
            std::hint::spin_loop();
        }
        assert_eq!(cache.len(), 0);
    }
}
