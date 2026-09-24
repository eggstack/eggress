//! Upstream hop handlers: one `HopHandler` per proxied protocol.
//!
//! Constructed by `build_chain_executor`; each handler owns its
//! protocol-specific handshake while sharing the boxed-stream shape.

use eggress_core::chain::HopHandler;
use eggress_core::BoxStream;
use eggress_core::{TargetAddr, TargetHost};
use std::pin::Pin;
use std::task::{ready, Context, Poll};

pub(crate) type HandshakeFuture<'a> = std::pin::Pin<
    Box<
        dyn std::future::Future<
                Output = Result<BoxStream, Box<dyn std::error::Error + Send + Sync>>,
            > + Send
            + 'a,
    >,
>;

/// Adapts an origin-form request into the absolute-form request expected by
/// pproxy's `httponly` upstream mode. The adapter is deliberately limited to
/// the request headers; bodies are passed through unchanged.
pub(crate) struct HttpOnlyStream {
    pub(crate) inner: BoxStream,
    pub(crate) target: TargetAddr,
    pub(crate) pending: Vec<u8>,
    pub(crate) rewritten: bool,
}

/// Cap on bytes buffered while the httponly upstream is stalled. Beyond this,
/// `poll_write` exerts backpressure instead of growing memory without bound.
pub(crate) const HTTPONLY_MAX_BUFFERED: usize = 64 * 1024;

impl tokio::io::AsyncRead for HttpOnlyStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for HttpOnlyStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        // Drain buffered bytes into the upstream first; if it is stalled,
        // propagating `Pending` from the flush engages relay backpressure
        // instead of buffering without bound.
        if !self.pending.is_empty() {
            ready!(self.as_mut().poll_flush(cx))?;
        }
        let room = HTTPONLY_MAX_BUFFERED.saturating_sub(self.pending.len());
        if room == 0 && !data.is_empty() {
            // Backpressure: pending is full after flush attempt, signal
            // Pending so the caller (which must use `write_all`/retry loop)
            // waits for the waker instead of busy-spinning on Ok(0).
            return Poll::Pending;
        }
        let accepted = data.len().min(room);
        self.pending.extend_from_slice(&data[..accepted]);
        // A short write is valid `AsyncWrite` behavior: callers retry with
        // the remainder once the buffered bytes have drained. Callers must
        // use `write_all` or explicit retry; a single `write` may return
        // short and the tail must be retried by the caller.
        Poll::Ready(Ok(accepted))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if !self.rewritten
            && !self.pending.is_empty()
            && self.pending.windows(4).any(|window| window == b"\r\n\r\n")
        {
            self.pending = rewrite_request_head(&self.pending, &self.target);
            self.rewritten = true;
        }
        // The stream type is Unpin, so plain field access keeps the borrows
        // of `inner` and `pending` disjoint inside the drain loop.
        let this = self.get_mut();
        while !this.pending.is_empty() {
            match Pin::new(&mut this.inner).poll_write(cx, &this.pending) {
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "httponly upstream accepted zero bytes",
                    )));
                }
                Poll::Ready(Ok(n)) => {
                    this.pending.drain(..n);
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        ready!(self.as_mut().poll_flush(cx))?;
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Rewrites an origin-form request head into the absolute-form request
/// expected by pproxy's `httponly` upstream mode. Only the request line
/// changes; every other byte — including all header line terminators and
/// the body — is preserved exactly as received. Input without a complete,
/// rewritable request head is returned unchanged.
pub(crate) fn rewrite_request_head(data: &[u8], target: &TargetAddr) -> Vec<u8> {
    let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") else {
        // Head not complete yet; leave everything untouched.
        return data.to_vec();
    };
    let end = pos + 4;
    let head = &data[..end];
    let mut rewritten = Vec::with_capacity(data.len() + 32);
    if let Some(nl) = head.iter().position(|b| *b == b'\n') {
        let raw_first = &head[..nl];
        let first = raw_first.strip_suffix(b"\r").unwrap_or(raw_first);
        if let Some(space) = first.iter().position(|b| *b == b' ') {
            if let Some(second) = first[space + 1..].iter().position(|b| *b == b' ') {
                let method = &first[..space];
                let path = &first[space + 1..space + 1 + second];
                if path.starts_with(b"/") {
                    rewritten.extend_from_slice(method);
                    rewritten.extend_from_slice(b" http://");
                    rewritten.extend_from_slice(target.to_string().as_bytes());
                    rewritten.extend_from_slice(path);
                    rewritten.extend_from_slice(&first[space + 1 + second..]);
                    // Restore the original request-line terminator, then
                    // copy every remaining header byte verbatim; only the
                    // request line itself changes.
                    if raw_first.len() != first.len() {
                        rewritten.push(b'\r');
                    }
                    rewritten.extend_from_slice(&head[nl..]);
                }
            }
        }
    }
    if rewritten.is_empty() {
        rewritten.extend_from_slice(head);
    }
    rewritten.extend_from_slice(&data[end..]);
    rewritten
}

pub(crate) struct HttpOnlyHopHandler;

impl HopHandler for HttpOnlyHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::HttpOnly
    }
    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        _hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        let target = target.clone();
        Box::pin(async move {
            Ok(Box::new(HttpOnlyStream {
                inner: stream,
                target,
                pending: Vec::new(),
                rewritten: false,
            }) as BoxStream)
        })
    }
}

pub(crate) struct HttpHopHandler;

impl HopHandler for HttpHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Http
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        let auth = hop
            .credentials
            .as_ref()
            .map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            eggress_protocol_http::http_connect(stream, target, auth, &Default::default())
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

pub(crate) struct Socks5HopHandler;

impl HopHandler for Socks5HopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Socks5
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        let socks_addr = target_to_socks_addr(target);
        let auth = hop
            .credentials
            .as_ref()
            .map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            eggress_protocol_socks::socks5::client::socks5_connect(stream, &socks_addr, auth)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

pub(crate) struct Socks4HopHandler;

impl HopHandler for Socks4HopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Socks4
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        let user_id = hop.credentials.as_ref().map(|c| c.username.as_str());
        Box::pin(async move {
            eggress_protocol_socks::socks4_connect(stream, target, user_id)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

#[cfg(feature = "extended")]
pub(crate) struct ShadowsocksHopHandler {
    pub(crate) metrics: Option<std::sync::Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>>,
}

#[cfg(feature = "extended")]
impl HopHandler for ShadowsocksHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Shadowsocks
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        let metrics = self.metrics.clone();
        Box::pin(async move {
            let creds = hop.credentials.as_ref().ok_or_else(|| {
                Box::new(eggress_protocol_shadowsocks::ShadowsocksError::Other(
                    "shadowsocks requires credentials (method:password)".to_string(),
                )) as Box<dyn std::error::Error + Send + Sync>
            })?;

            match eggress_protocol_shadowsocks::CipherMethod::parse_method(&creds.username) {
                Ok(method) => eggress_protocol_shadowsocks::shadowsocks_connect(
                    stream,
                    target,
                    method,
                    &creds.password,
                    metrics,
                )
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
                Err(modern_error) => {
                    #[cfg(feature = "legacy-crypto")]
                    if let Ok(legacy_method) =
                        eggress_protocol_shadowsocks::legacy::LegacyMethod::parse(&creds.username)
                    {
                        return eggress_protocol_shadowsocks::legacy::legacy_connect(
                            stream,
                            target,
                            legacy_method,
                            creds.password.as_bytes(),
                        )
                        .await
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>);
                    }
                    if let Some(m) = metrics.as_ref() {
                        m.record_tcp_unsupported_method_reject();
                    }
                    Err(Box::new(modern_error) as Box<dyn std::error::Error + Send + Sync>)
                }
            }
        })
    }
}

#[cfg(feature = "pproxy-legacy")]
pub(crate) struct ShadowsocksRHopHandler;

#[cfg(feature = "pproxy-legacy")]
impl HopHandler for ShadowsocksRHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::ShadowsocksR
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        Box::pin(async move {
            let plugins = eggress_protocol_shadowsocks::compat::plugin::parse_plugins(&hop.plugins)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            eggress_protocol_shadowsocks::compat::ssr::ssr_connect(
                stream,
                target,
                &eggress_protocol_shadowsocks::compat::ssr::SsrConfig {
                    auth_prefix: hop.auth_prefix.as_deref().map(str::as_bytes).map(Vec::from),
                    plugins,
                },
            )
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

#[cfg(feature = "extended")]
pub(crate) struct TrojanHopHandler {
    pub(crate) tls_config: Option<std::sync::Arc<rustls::ClientConfig>>,
    pub(crate) insecure_tls_config: Option<std::sync::Arc<rustls::ClientConfig>>,
    pub(crate) tls_override: Option<std::sync::Arc<rustls::ClientConfig>>,
}

#[cfg(feature = "extended")]
impl HopHandler for TrojanHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Trojan
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        let tls_config = self.tls_config.clone();
        let insecure_tls_config = self.insecure_tls_config.clone();
        let tls_override = self.tls_override.clone();
        let insecure = hop.insecure;
        let password = hop.credentials.as_ref().map(|c| c.password.clone());
        let server_name = hop
            .server_name
            .clone()
            .unwrap_or_else(|| hop.endpoint.host.clone());
        Box::pin(async move {
            let password = password.ok_or_else(|| {
                Box::new(eggress_protocol_trojan::TrojanError::Protocol(
                    "trojan requires credentials (password)".to_string(),
                )) as Box<dyn std::error::Error + Send + Sync>
            })?;

            let chosen = if insecure {
                if let Some(ovr) = tls_override.clone() {
                    // Global override is already insecure in tests that set it;
                    // reuse it for per-hop insecure when available.
                    Some(ovr)
                } else {
                    insecure_tls_config.clone().or(tls_config.clone())
                }
            } else {
                tls_config.clone()
            };

            eggress_protocol_trojan::trojan_connect(stream, target, &password, &server_name, chosen)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

#[cfg(feature = "extended")]
pub(crate) struct WebSocketHopHandler;

#[cfg(feature = "extended")]
impl HopHandler for WebSocketHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::WebSocket
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        _target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        let use_tls = hop.tls;
        let scheme = if use_tls { "wss" } else { "ws" };
        let url = format!("{}://{}:{}", scheme, hop.endpoint.host, hop.endpoint.port);
        Box::pin(async move {
            let client = eggress_protocol_websocket::WebSocketTunnelClient::with_default_config();
            client
                .connect_over_stream(&url, stream)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

pub(crate) struct RawHopHandler;

impl HopHandler for RawHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Raw
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        _target: &'a TargetAddr,
        _hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        Box::pin(async move { Ok(stream) })
    }
}

#[cfg(feature = "ssh")]
pub(crate) struct SshHopHandler {
    pub(crate) sessions: std::sync::Arc<eggress_transport_ssh::SshSessionCache>,
}

#[cfg(feature = "ssh")]
impl HopHandler for SshHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Ssh
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        hop_index: usize,
    ) -> HandshakeFuture<'a> {
        let sessions = self.sessions.clone();
        let target = target.clone();
        let endpoint = hop.endpoint.clone();
        let credentials = hop.credentials.clone();
        Box::pin(async move {
            let credentials = credentials.ok_or_else(|| {
                Box::new(eggress_transport_ssh::SshTransportError::MissingUsername)
                    as Box<dyn std::error::Error + Send + Sync>
            })?;
            if credentials.username.is_empty() {
                return Err(
                    Box::new(eggress_transport_ssh::SshTransportError::MissingUsername)
                        as Box<dyn std::error::Error + Send + Sync>,
                );
            }
            let auth = if let Some(path) = credentials.password.strip_prefix(':') {
                if path.is_empty() {
                    return Err(Box::new(
                        eggress_transport_ssh::SshTransportError::EmptyPrivateKeyPath,
                    )
                        as Box<dyn std::error::Error + Send + Sync>);
                }
                eggress_transport_ssh::SshAuth::PrivateKey(path.to_string())
            } else {
                eggress_transport_ssh::SshAuth::Password(credentials.password)
            };
            let key = eggress_transport_ssh::SshSessionKey {
                host: endpoint.host,
                port: endpoint.port,
                username: credentials.username,
                auth,
                hop_index,
            };
            // Format once (O-03); previously each branch formatted separately.
            let target_host = target.host.to_string();
            let cached_allowed = hop_index == 0 && hop.local_bind.is_none();
            let result = match (target.port == 0, cached_allowed) {
                (true, true) => sessions.open_unix_channel(key, stream, &target_host).await,
                (false, true) => {
                    sessions
                        .open_tcp_channel(key, stream, &target_host, target.port)
                        .await
                }
                (true, false) => {
                    sessions
                        .open_unix_channel_fresh(key, stream, &target_host)
                        .await
                }
                (false, false) => {
                    sessions
                        .open_tcp_channel_fresh(key, stream, &target_host, target.port)
                        .await
                }
            };
            result.map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

pub(crate) struct UnixHopHandler;

impl HopHandler for UnixHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Unix
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        _target: &'a TargetAddr,
        _hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        Box::pin(async move { Ok(stream) })
    }
}

pub(crate) struct H2HopHandler {
    pub(crate) pool_registry: std::sync::Arc<eggress_protocol_http::H2PoolRegistry>,
}

#[cfg(feature = "quic")]
pub(crate) struct QuicHopHandler;

#[cfg(feature = "quic")]
impl HopHandler for QuicHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Quic
    }

    fn open<'a>(
        &'a self,
        endpoint: &'a eggress_uri::EndpointSpec,
        hop: &'a eggress_uri::ProxyHopSpec,
        _target: &'a TargetAddr,
    ) -> Option<HandshakeFuture<'a>> {
        let endpoint = endpoint.clone();
        let server_name = hop
            .server_name
            .clone()
            .unwrap_or_else(|| endpoint.host.clone());
        Some(Box::pin(async move {
            let client = eggress_transport_quic::QuicClient::connect(
                &endpoint.host,
                endpoint.port,
                eggress_transport_quic::QuicClientConfig {
                    server_name,
                    insecure: hop.insecure,
                    alpn_protocols: Vec::new(),
                    ..Default::default()
                },
            )
            .await?;
            client
                .open_stream()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }))
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        _target: &'a TargetAddr,
        _hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        Box::pin(async move { Ok(stream) })
    }
}

#[cfg(feature = "quic")]
pub(crate) struct H3HopHandler;

#[cfg(feature = "quic")]
impl HopHandler for H3HopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Http3
    }

    fn open<'a>(
        &'a self,
        endpoint: &'a eggress_uri::EndpointSpec,
        hop: &'a eggress_uri::ProxyHopSpec,
        target: &'a TargetAddr,
    ) -> Option<HandshakeFuture<'a>> {
        let endpoint = endpoint.clone();
        let target = target.clone();
        let server_name = hop
            .server_name
            .clone()
            .unwrap_or_else(|| endpoint.host.clone());
        let authorization = hop
            .credentials
            .as_ref()
            .map(|credentials| (credentials.username.clone(), credentials.password.clone()));
        Some(Box::pin(async move {
            let client = eggress_transport_quic::QuicClient::connect(
                &endpoint.host,
                endpoint.port,
                eggress_transport_quic::QuicClientConfig {
                    server_name,
                    insecure: hop.insecure,
                    alpn_protocols: vec![b"h3".to_vec()],
                    ..Default::default()
                },
            )
            .await?;
            eggress_protocol_h3::H3Client::new(client, authorization)
                .connect(&target)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }))
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        _target: &'a TargetAddr,
        _hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> HandshakeFuture<'a> {
        Box::pin(async move { Ok(stream) })
    }
}

/// Wrapper that holds an H2PoolGuard alongside the bidirectional stream,
/// ensuring the pooled connection is released back to the pool only when
/// the stream is dropped.
pub(crate) struct PooledH2Stream {
    inner:
        tokio::io::Join<eggress_protocol_http::H2StreamRead, eggress_protocol_http::H2StreamWrite>,
    pub(crate) _guard: eggress_protocol_http::H2PoolGuard,
}

pub(crate) struct UnpooledH2Stream {
    inner:
        tokio::io::Join<eggress_protocol_http::H2StreamRead, eggress_protocol_http::H2StreamWrite>,
    driver: tokio::task::JoinHandle<Result<(), h2::Error>>,
}

impl Drop for UnpooledH2Stream {
    fn drop(&mut self) {
        self.driver.abort();
    }
}

impl tokio::io::AsyncRead for UnpooledH2Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
impl tokio::io::AsyncWrite for UnpooledH2Stream {
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

impl tokio::io::AsyncRead for PooledH2Stream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for PooledH2Stream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl HopHandler for H2HopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Http2
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        hop_index: usize,
    ) -> HandshakeFuture<'a> {
        let endpoint_host = hop.endpoint.host.clone();
        let endpoint_port = hop.endpoint.port;
        let local_bind = hop.local_bind.is_some();
        let insecure = hop.insecure;
        let auth = hop
            .credentials
            .as_ref()
            .map(|c| (c.username.clone(), c.password.clone()));
        let target_clone = target.clone();
        let pool_key = eggress_protocol_http::H2PoolKey::with_hop_index(
            &endpoint_host,
            endpoint_port,
            hop.tls,
            hop.server_name.as_deref(),
            auth.as_ref().map(|(u, p)| (u.as_str(), p.as_str())),
            hop_index,
        );
        Box::pin(async move {
            let stream: BoxStream = stream;

            let auth_ref = auth.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));
            let pool_eligible = hop_index == 0 && !local_bind && !insecure;
            if pool_eligible {
                let (send_stream, recv_stream, guard) =
                    eggress_protocol_http::h2_connect_client_pooled_in_registry(
                        &self.pool_registry,
                        stream,
                        &target_clone,
                        auth_ref,
                        &pool_key,
                    )
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                let inner = tokio::io::join(
                    eggress_protocol_http::H2StreamRead::new(recv_stream),
                    eggress_protocol_http::H2StreamWrite::new(send_stream),
                );
                Ok(Box::new(PooledH2Stream {
                    inner,
                    _guard: guard,
                }) as BoxStream)
            } else {
                let (send_stream, recv_stream, driver) =
                    eggress_protocol_http::h2_connect_client(stream, &target_clone, auth_ref)
                        .await
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                let inner = tokio::io::join(
                    eggress_protocol_http::H2StreamRead::new(recv_stream),
                    eggress_protocol_http::H2StreamWrite::new(send_stream),
                );
                Ok(Box::new(UnpooledH2Stream { inner, driver }) as BoxStream)
            }
        })
    }
}

pub fn target_to_socks_addr(
    target: &TargetAddr,
) -> eggress_protocol_socks::socks5::server::SocksAddr {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => SocksAddr::IPv4(ip.octets(), target.port),
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => SocksAddr::IPv6(ip.octets(), target.port),
        TargetHost::Domain(d) => SocksAddr::Domain(d.clone(), target.port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::{TargetAddr, TargetHost};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn h2_pair(
        handshakes: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> (BoxStream, tokio::task::JoinHandle<()>) {
        let (client, server) = tokio::io::duplex(16 * 1024);
        let task = tokio::spawn(async move {
            let Ok(mut connection) = h2::server::handshake(server).await else {
                return;
            };
            handshakes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            while let Some(Ok((_request, mut response))) = connection.accept().await {
                if response
                    .send_response(
                        http::Response::builder().status(200).body(()).unwrap(),
                        false,
                    )
                    .is_err()
                {
                    break;
                }
            }
        });
        (Box::new(client), task)
    }

    fn h2_hop() -> eggress_uri::ProxyHopSpec {
        eggress_uri::parse_proxy_chain("h2://127.0.0.1:443")
            .unwrap()
            .hops
            .into_iter()
            .next()
            .unwrap()
    }

    async fn run_h2_handshake(
        handler: &H2HopHandler,
        stream: BoxStream,
        hop: &eggress_uri::ProxyHopSpec,
        hop_index: usize,
    ) {
        let target: TargetAddr = "target.example:443".parse().unwrap();
        let connected = handler
            .handshake(stream, &target, hop, hop_index)
            .await
            .expect("H2 CONNECT succeeds");
        drop(connected);
    }

    #[tokio::test]
    async fn h2_hop_zero_same_policy_reuses_physical_connection() {
        let handshakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let handler = H2HopHandler {
            pool_registry: std::sync::Arc::new(eggress_protocol_http::H2PoolRegistry::new()),
        };
        let hop = h2_hop();
        for _ in 0..2 {
            let (stream, _server) = h2_pair(handshakes.clone()).await;
            run_h2_handshake(&handler, stream, &hop, 0).await;
        }
        assert_eq!(handshakes.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn h2_hop_zero_local_bind_is_not_pooled() {
        let handshakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let handler = H2HopHandler {
            pool_registry: std::sync::Arc::new(eggress_protocol_http::H2PoolRegistry::new()),
        };
        let mut hop = h2_hop();
        hop.local_bind = Some("127.0.0.1:12345".into());
        for _ in 0..2 {
            let (stream, _server) = h2_pair(handshakes.clone()).await;
            run_h2_handshake(&handler, stream, &hop, 0).await;
        }
        assert_eq!(handshakes.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn h2_hop_zero_insecure_is_not_pooled() {
        let handshakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let handler = H2HopHandler {
            pool_registry: std::sync::Arc::new(eggress_protocol_http::H2PoolRegistry::new()),
        };
        let mut hop = h2_hop();
        hop.insecure = true;
        for _ in 0..2 {
            let (stream, _server) = h2_pair(handshakes.clone()).await;
            run_h2_handshake(&handler, stream, &hop, 0).await;
        }
        assert_eq!(handshakes.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn nested_h2_does_not_cross_reuse_prefixes() {
        let prefix_a = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let prefix_b = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let handler = H2HopHandler {
            pool_registry: std::sync::Arc::new(eggress_protocol_http::H2PoolRegistry::new()),
        };
        let hop = h2_hop();
        let (stream_a, _server_a) = h2_pair(prefix_a.clone()).await;
        run_h2_handshake(&handler, stream_a, &hop, 1).await;
        let (stream_b, _server_b) = h2_pair(prefix_b.clone()).await;
        run_h2_handshake(&handler, stream_b, &hop, 1).await;

        assert_eq!(prefix_a.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(prefix_b.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    fn http_only_target() -> TargetAddr {
        TargetAddr {
            host: TargetHost::Domain("target.example".into()),
            port: 8080,
        }
    }

    #[test]
    fn httponly_rewrite_preserves_header_terminators() {
        let request = b"GET /path HTTP/1.1\r\nHost: example.com\r\nX-Foo: bar\r\n\r\nbody";
        let rewritten = rewrite_request_head(request, &http_only_target());
        assert_eq!(
            std::str::from_utf8(&rewritten).unwrap(),
            "GET http://target.example:8080/path HTTP/1.1\r\n\
             Host: example.com\r\n\
             X-Foo: bar\r\n\
             \r\n\
             body"
        );
    }

    #[test]
    fn httponly_rewrite_preserves_mixed_line_endings() {
        // Request line terminated by bare LF while the head ends with CRLF.
        let request = b"GET /path HTTP/1.1\nHost: example.com\r\nX-Foo: bar\r\n\r\n";
        let rewritten = rewrite_request_head(request, &http_only_target());
        assert_eq!(
            std::str::from_utf8(&rewritten).unwrap(),
            "GET http://target.example:8080/path HTTP/1.1\n\
             Host: example.com\r\n\
             X-Foo: bar\r\n\
             \r\n"
        );
    }

    #[test]
    fn httponly_rewrite_waits_for_complete_head() {
        // No `\r\n\r\n` terminator yet — unchanged (flush retries later).
        let partial = b"GET /path HTTP/1.1\r\nHost: example.com\r\n".as_slice();
        assert_eq!(rewrite_request_head(partial, &http_only_target()), partial);
    }

    #[test]
    fn httponly_rewrite_leaves_absolute_form_and_incomplete_heads_alone() {
        // Absolute-form request line is not origin-form; unchanged.
        let absolute =
            b"GET http://example.com/path HTTP/1.1\r\nHost: example.com\r\n\r\n".as_slice();
        assert_eq!(
            rewrite_request_head(absolute, &http_only_target()),
            absolute
        );
        // No complete head yet; unchanged (flush will retry later).
        let partial = b"GET /path HTTP/1.1\r\nHost: example.com\r\n".as_slice();
        assert_eq!(rewrite_request_head(partial, &http_only_target()), partial);
        // Empty input stays empty.
        assert!(rewrite_request_head(b"", &http_only_target()).is_empty());
    }

    #[tokio::test]
    async fn httponly_stream_rewrites_once_and_drains_on_shutdown() {
        // A tiny duplex buffer forces the flush loop across multiple polls;
        // the remaining bytes still contain `\r\n\r\n`, which a second
        // rewrite pass would mangle ("X-Foo: /bar baz" looks like an
        // origin-form request line to the rewriter).
        let (mut peer, inner) = tokio::io::duplex(16);
        let mut stream = HttpOnlyStream {
            inner: Box::new(inner),
            target: http_only_target(),
            pending: Vec::new(),
            rewritten: false,
        };
        let request =
            b"GET /path HTTP/1.1\r\nHost: example.com\r\nX-Foo: /bar baz\r\n\r\ntail".as_slice();
        let expected =
            b"GET http://target.example:8080/path HTTP/1.1\r\nHost: example.com\r\nX-Foo: /bar baz\r\n\r\ntail";
        let reader = tokio::spawn(async move {
            let mut received = Vec::new();
            peer.read_to_end(&mut received).await.unwrap();
            received
        });
        stream.write_all(request).await.unwrap();
        stream.shutdown().await.unwrap();
        let received = reader.await.unwrap();
        assert_eq!(received, expected);
    }
}
