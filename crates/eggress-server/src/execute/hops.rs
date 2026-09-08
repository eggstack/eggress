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
            let result = if target.port == 0 {
                sessions.open_unix_channel(key, stream, &target_host).await
            } else {
                sessions
                    .open_tcp_channel(key, stream, &target_host, target.port)
                    .await
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

pub(crate) struct H2HopHandler;

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
            let (send_stream, recv_stream, guard) =
                eggress_protocol_http::h2_connect_client_pooled(
                    stream,
                    &target_clone,
                    auth_ref,
                    &pool_key,
                )
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

            let h2_write = eggress_protocol_http::H2StreamWrite::new(send_stream);
            let h2_read = eggress_protocol_http::H2StreamRead::new(recv_stream);

            let pooled = PooledH2Stream {
                inner: tokio::io::join(h2_read, h2_write),
                _guard: guard,
            };
            Ok(Box::new(pooled) as BoxStream)
        })
    }
}

pub(crate) fn target_to_socks_addr(
    target: &TargetAddr,
) -> eggress_protocol_socks::socks5::server::SocksAddr {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => SocksAddr::IPv4(ip.octets(), target.port),
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => SocksAddr::IPv6(ip.octets(), target.port),
        TargetHost::Domain(d) => SocksAddr::Domain(d.clone(), target.port),
    }
}
