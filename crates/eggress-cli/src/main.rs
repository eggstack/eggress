use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use clap::Parser;
use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::connector::{Connector, DirectConnector};
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::relay::relay;
use eggress_core::{BoxStream, TargetAddr, TargetHost};
use eggress_uri::{parse_proxy_chain, CredentialSpec, ProxyChainSpec};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{fmt, EnvFilter};

/// Global connection counter for logging.
static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(1);

type HandshakeFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<BoxStream, Box<dyn std::error::Error + Send + Sync>>>
            + Send
            + 'a,
    >,
>;

/// Eggress - A multi-protocol TCP proxy
#[derive(Parser, Debug)]
#[command(name = "eggress", version, about = "A multi-protocol TCP proxy")]
struct Cli {
    /// Listener URIs (e.g., "http://0.0.0.0:8080", "socks5://0.0.0.0:1080")
    #[arg(short = 'l', long = "listen", value_name = "URI")]
    listeners: Vec<String>,

    /// Upstream proxy URIs (can be specified multiple times, chain with __)
    #[arg(short = 'r', long = "remote", value_name = "URI")]
    upstreams: Vec<String>,
}

fn init_logging() {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .compact()
        .init();
}

#[tokio::main]
async fn main() {
    init_logging();
    let args = Cli::parse();
    let cancel_token = CancellationToken::new();

    {
        let token = cancel_token.clone();
        tokio::spawn(async move {
            let ctrl_c = tokio::signal::ctrl_c();
            #[cfg(unix)]
            {
                let mut sigterm =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("failed to register SIGTERM handler");
                tokio::select! {
                    _ = ctrl_c => {}
                    _ = sigterm.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                ctrl_c.await.ok();
            }
            tracing::info!("shutdown signal received");
            token.cancel();
        });
    }

    let upstream_chain: Option<ProxyChainSpec> = if args.upstreams.is_empty() {
        None
    } else {
        let combined = args.upstreams.join("__");
        match parse_proxy_chain(&combined) {
            Ok(spec) => {
                tracing::info!("upstream chain: {}", eggress_uri::RedactedUri::new(&spec));
                Some(spec)
            }
            Err(e) => {
                eprintln!("invalid upstream URI: {e}");
                std::process::exit(1);
            }
        }
    };

    let listener_uris: Vec<String> = if args.listeners.is_empty() {
        vec!["http://127.0.0.1:8080".to_string()]
    } else {
        args.listeners
    };

    let mut handles = Vec::new();

    for uri in &listener_uris {
        let spec = match parse_proxy_chain(uri) {
            Ok(spec) => spec,
            Err(e) => {
                eprintln!("invalid listener URI '{uri}': {e}");
                std::process::exit(1);
            }
        };

        let first_hop = &spec.hops[0];
        let bind_addr: SocketAddr =
            format!("{}:{}", first_hop.endpoint.host, first_hop.endpoint.port)
                .parse()
                .unwrap_or_else(|e| {
                    eprintln!("invalid listener bind address '{uri}': {e}");
                    std::process::exit(1);
                });

        let protocols: Vec<eggress_core::ProtocolId> = first_hop
            .protocols
            .iter()
            .map(|p| match p {
                eggress_uri::ProtocolSpec::Http => "http",
                eggress_uri::ProtocolSpec::Socks4 => "socks4",
                eggress_uri::ProtocolSpec::Socks5 => "socks5",
            })
            .collect();

        let cancel = cancel_token.clone();
        let chain = upstream_chain.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = run_listener(bind_addr, protocols, chain, cancel).await {
                tracing::error!("listener error: {e}");
            }
        });
        handles.push(handle);
    }

    tracing::info!("eggress started, {} listener(s)", listener_uris.len());

    tokio::select! {
        _ = cancel_token.cancelled() => {
            tracing::info!("shutting down");
        }
        _ = async {
            for h in handles {
                let _ = h.await;
            }
        } => {}
    }
}

async fn run_listener(
    bind_addr: SocketAddr,
    protocols: Vec<eggress_core::ProtocolId>,
    upstream_chain: Option<ProxyChainSpec>,
    cancel_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = TcpListenerConfig {
        bind_addr,
        protocols,
        auth_required: false,
        handshake_timeout: Duration::from_secs(30),
        connection_limit: 1024,
    };

    let listener = TcpListener::new(&config, cancel_token.clone()).await?;
    let local_addr = listener.local_addr()?;
    tracing::info!("listening on {local_addr}");

    loop {
        let conn = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                if e.to_string().contains("listener cancelled") {
                    break;
                }
                tracing::error!("accept error: {e}");
                continue;
            }
        };

        let chain = upstream_chain.clone();
        let peer = conn.peer_addr;
        let listener = local_addr;
        let conn_id = CONNECTION_COUNTER.fetch_add(1, Ordering::Relaxed);

        tokio::spawn(async move {
            let started = Instant::now();
            let _span = tracing::info_span!(
                "conn",
                id = conn_id,
                peer = %peer,
                listener = %listener,
            );
            let _guard = _span.enter();

            if let Err(e) = handle_connection(conn.stream, chain).await {
                tracing::debug!("connection error: {e}");
            }

            tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "connection closed"
            );
        });
    }

    Ok(())
}

/// Handle an inbound HTTP request, distinguishing CONNECT from forward proxy.
///
/// Peeks at the method to determine whether this is a CONNECT tunnel
/// or an ordinary forward-proxy request.
async fn handle_http_request(
    client_stream: BoxStream,
) -> Result<(TargetAddr, BoxStream), Box<dyn std::error::Error + Send + Sync>> {
    // Read the request line to determine method
    let mut stream = client_stream;
    let mut head_buf = Vec::with_capacity(256);
    let mut temp = [0u8; 1];

    loop {
        if head_buf.len() >= 32 * 1024 {
            return Err(eggress_protocol_http::HttpError::HeaderTooLarge.into());
        }
        let n = stream.read(&mut temp).await?;
        if n == 0 {
            return Err(eggress_protocol_http::HttpError::MalformedRequest(
                "unexpected EOF".into(),
            )
            .into());
        }
        head_buf.push(temp[0]);
        if head_buf.len() >= 2 && &head_buf[head_buf.len() - 2..] == b"\r\n" {
            break;
        }
    }

    // Extract method before moving head_buf
    let method = {
        let request_line = String::from_utf8_lossy(&head_buf);
        request_line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    };

    // Reconstruct stream with the request line bytes prepended
    let stream: BoxStream = Box::new(PrefixedStream::new(head_buf, stream));

    if method == "connect" {
        // CONNECT tunnel: handshake returns the raw stream after 200
        let (request, stream) =
            eggress_protocol_http::connect::server::handle_connect(stream, false, None).await?;
        Ok((request.target, stream))
    } else {
        // Forward proxy: parse absolute-form request, connect upstream, forward
        let (request, mut client_stream) = eggress_protocol_http::forward_request(stream).await?;

        let target = request.target.clone();

        // Connect to the upstream target
        let connector = eggress_core::connector::DirectConnector;
        let mut upstream = connector
            .connect(&target)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Build origin-form request and send to upstream
        let origin_req = eggress_protocol_http::build_origin_request(&request);
        upstream.write_all(origin_req.as_bytes()).await?;
        upstream.flush().await?;

        // If there's a body, relay it from client to upstream
        if request.has_body {
            match (request.content_length, request.is_chunked) {
                (Some(len), _) => {
                    let mut remaining = len;
                    let mut buf = [0u8; 8192];
                    while remaining > 0 {
                        let to_read = (remaining as usize).min(buf.len());
                        let n = client_stream.read(&mut buf[..to_read]).await?;
                        if n == 0 {
                            break;
                        }
                        upstream.write_all(&buf[..n]).await?;
                        remaining -= n as u64;
                    }
                }
                (None, true) => {
                    // Chunked: relay chunks from client to upstream
                    // The body is already in the stream after the request head
                    // We need to relay it through
                    let mut buf = [0u8; 8192];
                    loop {
                        let n = client_stream.read(&mut buf).await?;
                        if n == 0 {
                            break;
                        }
                        upstream.write_all(&buf[..n]).await?;
                    }
                }
                (None, false) => {
                    // No body info, relay until client closes
                    let mut buf = [0u8; 8192];
                    loop {
                        let n = client_stream.read(&mut buf).await?;
                        if n == 0 {
                            break;
                        }
                        upstream.write_all(&buf[..n]).await?;
                    }
                }
            }
        }

        // Forward the upstream response back to the client
        eggress_protocol_http::forward_response(&mut upstream, &mut client_stream).await?;

        // Return a dummy target and a stream that will EOF (response already forwarded)
        // The relay phase is skipped for forward proxy responses
        // We use a special pattern: return the client stream wrapped so relay sees EOF
        Ok((target, Box::new(tokio::io::empty())))
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

impl tokio::io::AsyncRead for PrefixedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        // Serve from prefix first
        let pos = self.prefix.position() as usize;
        let len = self.prefix.get_ref().len();
        if pos < len {
            let remaining = &self.prefix.get_ref()[pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.prefix.set_position((pos + to_copy) as u64);
            return std::task::Poll::Ready(Ok(()));
        }
        // Then delegate to inner
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for PrefixedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

async fn handle_connection(
    stream: BoxStream,
    upstream_chain: Option<ProxyChainSpec>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Peek at first byte for protocol detection
    let mut first_byte = [0u8; 1];
    let mut stream = stream;
    stream.read_exact(&mut first_byte).await?;

    let proto = match first_byte[0] {
        0x05 => "socks5",
        0x04 => "socks4",
        _ => "http",
    };
    tracing::trace!(
        "detected protocol: {proto} (first_byte={:#04x})",
        first_byte[0]
    );

    // Reconstruct stream with the first byte prepended
    let client_stream: BoxStream = Box::new(PrefixedStream::new(first_byte.to_vec(), stream));

    let (target, client_stream) = match proto {
        "http" => handle_http_request(client_stream).await?,
        "socks5" => {
            use eggress_protocol_socks::socks5::server::{
                read_connect_request, read_method_negotiation, send_connect_reply,
                send_method_selection,
            };

            let (mut reader, mut writer) = tokio::io::split(client_stream);
            let methods = read_method_negotiation(&mut reader).await?;
            send_method_selection(&mut writer, &methods, None).await?;
            let socks_addr = read_connect_request(&mut reader).await?;

            let target = socks_addr_to_target(&socks_addr);
            let bind_addr =
                eggress_protocol_socks::socks5::server::SocksAddr::IPv4([0, 0, 0, 0], 0);
            send_connect_reply(&mut writer, 0x00, &bind_addr).await?;

            let stream: BoxStream = Box::new(tokio::io::join(reader, writer));
            (target, stream)
        }
        "socks4" => {
            use eggress_protocol_socks::socks4::server::{
                read_socks4_request, write_socks4_reply, Socks4Status,
            };
            use tokio::io::AsyncWriteExt;

            let (mut reader, mut writer) = tokio::io::split(client_stream);
            let request = read_socks4_request(&mut reader).await?;
            let target = request.addr;
            write_socks4_reply(
                &mut writer,
                Socks4Status::Granted,
                "0.0.0.0:0".parse().unwrap(),
            )
            .await?;
            writer.flush().await?;

            let stream: BoxStream = Box::new(tokio::io::join(reader, writer));
            (
                TargetAddr {
                    host: TargetHost::Ip(target.ip()),
                    port: target.port(),
                },
                stream,
            )
        }
        _ => {
            return Err("unsupported protocol".into());
        }
    };

    tracing::info!("connecting to {target}");

    let server_stream = if let Some(ref chain) = upstream_chain {
        let executor = build_chain_executor();
        executor
            .execute(&chain.hops, &target)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
    } else {
        let connector = DirectConnector;
        connector
            .connect(&target)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
    };

    let result = relay(client_stream, server_stream).await;
    tracing::debug!(
        "relay complete: upstream={}B downstream={}B reason={:?}",
        result.bytes_upstream,
        result.bytes_downstream,
        result.termination_reason
    );

    Ok(())
}

fn build_chain_executor() -> ChainExecutor {
    let handlers: Vec<Box<dyn HopHandler>> = vec![
        Box::new(HttpHopHandler),
        Box::new(Socks5HopHandler),
        Box::new(Socks4HopHandler),
    ];
    ChainExecutor::new(handlers)
}

struct HttpHopHandler;

impl HopHandler for HttpHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Http
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        credentials: Option<&'a CredentialSpec>,
    ) -> HandshakeFuture<'a> {
        let auth = credentials.map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            eggress_protocol_http::http_connect(stream, target, auth)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

struct Socks5HopHandler;

impl HopHandler for Socks5HopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Socks5
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        credentials: Option<&'a CredentialSpec>,
    ) -> HandshakeFuture<'a> {
        let socks_addr = target_to_socks_addr(target);
        let auth = credentials.map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            eggress_protocol_socks::socks5::client::socks5_connect(stream, &socks_addr, auth)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

struct Socks4HopHandler;

impl HopHandler for Socks4HopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Socks4
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        credentials: Option<&'a CredentialSpec>,
    ) -> HandshakeFuture<'a> {
        let user_id = credentials.map(|c| c.username.as_str());
        Box::pin(async move {
            eggress_protocol_socks::socks4_connect(stream, target, user_id)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

fn target_to_socks_addr(target: &TargetAddr) -> eggress_protocol_socks::socks5::server::SocksAddr {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => SocksAddr::IPv4(ip.octets(), target.port),
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => SocksAddr::IPv6(ip.octets(), target.port),
        TargetHost::Domain(d) => SocksAddr::Domain(d.clone(), target.port),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_http_proxy_end_to_end() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        drop(proxy_listener);

        let cancel = CancellationToken::new();
        let config = TcpListenerConfig {
            bind_addr: proxy_addr,
            protocols: vec!["http"],
            auth_required: false,
            handshake_timeout: Duration::from_secs(5),
            connection_limit: 10,
        };
        let listener = TcpListener::new(&config, cancel.clone()).await.unwrap();

        let proxy_jh = tokio::spawn(async move {
            loop {
                let conn = match listener.accept().await {
                    Ok(c) => c,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let _ = handle_connection(conn.stream, None).await;
                });
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let connect_req = format!(
            "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n\r\n",
            echo_addr.ip(),
            echo_addr.port(),
            echo_addr.ip(),
            echo_addr.port()
        );
        stream.write_all(connect_req.as_bytes()).await.unwrap();

        let mut response = vec![0u8; 1024];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("200"),
            "expected 200, got: {response_str}"
        );

        let header_end = response_str.find("\r\n\r\n").unwrap() + 4;
        let leftover = &response.as_slice()[header_end..n];

        stream.write_all(b"hello proxy").await.unwrap();
        stream.shutdown().await.unwrap();

        let mut buf = Vec::new();
        if !leftover.is_empty() {
            buf.extend_from_slice(leftover);
        }
        stream.read_to_end(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello proxy");

        cancel.cancel();
        let _ = proxy_jh.await;
        echo_jh.abort();
    }
}
