pub mod accept;
#[cfg(feature = "extended")]
pub mod advanced;
pub mod error;
pub mod execute;
pub mod listener;
pub mod reply;

use std::sync::Arc;
use std::time::Duration;

pub use accept::AcceptedSession;
pub use accept::AuthReuseCache;
pub use error::SessionOpenError;
pub use execute::{build_chain_executor, FailureCategory, SessionReport};

use eggress_routing::RouteService;

/// Trait for recording session metrics. Implemented by external crates.
pub trait SessionMetrics: Send + Sync {
    fn record_session_start(&self);
    fn record_session(&self, report: &SessionReport);
    fn record_route_decision(&self, rule: &str, action: &str, outcome: &str);
    fn record_upstream_open(&self, protocol: &str, outcome: &str);
    fn record_upstream_failure(&self, protocol: &str, reason: &str);
    fn record_auth_failure(&self);
    fn record_platform_capability_check_failure(&self) {}
    fn record_unix_listener_connection_accepted(&self) {}
    fn record_reload(&self, _success: bool) {}
    fn set_config_generation(&self, _generation: u64) {}
    fn record_udp_association_created(&self) {}
    fn render_prometheus(&self) -> String {
        String::new()
    }
}

/// No-op implementation of SessionMetrics for builds without operations support.
pub struct NoopMetrics;

impl SessionMetrics for NoopMetrics {
    fn record_session_start(&self) {}
    fn record_session(&self, _report: &SessionReport) {}
    fn record_route_decision(&self, _rule: &str, _action: &str, _outcome: &str) {}
    fn record_upstream_open(&self, _protocol: &str, _outcome: &str) {}
    fn record_upstream_failure(&self, _protocol: &str, _reason: &str) {}
    fn record_auth_failure(&self) {}
}

/// Handle returned by UdpService::create_association.
pub struct UdpAssociationHandle {
    pub id: eggress_udp::assoc::UdpAssociationId,
    pub relay_addr: std::net::SocketAddr,
    pub cancel: tokio_util::sync::CancellationToken,
}

/// Trait for UDP association services. Implemented by the runtime crate.
pub trait UdpService: Send + Sync {
    fn create_association(
        &self,
        listener: &str,
        client_tcp_peer: std::net::SocketAddr,
        identity: eggress_core::ClientIdentity,
        generation: u64,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<UdpAssociationHandle, eggress_udp::error::UdpError>,
                > + Send
                + 'static,
        >,
    >;
    fn is_enabled(&self) -> bool;
    fn active_count(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = usize> + Send + 'static>>;
}

/// Context propagated from the listener into routing decisions.
#[derive(Clone, Default)]
pub struct ConnectionContext {
    pub source: Option<std::net::SocketAddr>,
    pub listener: String,
    pub generation: u64,
}

/// Configuration for a single connection.
#[derive(Clone)]
pub struct ConnectionConfig {
    pub routing: Arc<dyn RouteService>,
    pub context: ConnectionContext,
    pub handshake_timeout: Duration,
    pub connect_timeout: Duration,
    pub protocols: Arc<[eggress_core::ProtocolId]>,
    pub authentication: accept::InboundAuthentication,
    pub metrics: Option<Arc<dyn SessionMetrics>>,
    pub udp: Option<Arc<dyn UdpService>>,
    /// Optional TLS client config override for upstream connections (e.g., Trojan).
    /// When `None`, the chain executor builds a config with system root CAs.
    /// Intended for test-only use (e.g., insecure TLS for self-signed certs).
    pub tls_client_config: Option<Arc<rustls::ClientConfig>>,
    pub shadowsocks: Option<accept::InboundShadowsocksConfig>,
    /// Optional Trojan inbound configuration for password verification on Trojan listeners.
    pub trojan: Option<accept::InboundTrojanConfig>,
    pub fixed_target: Option<eggress_core::TargetAddr>,
    pub local_bind: Option<String>,
    /// Shared optional SSH session cache for compatibility upstreams.
    #[cfg(feature = "ssh")]
    pub ssh_sessions: Option<Arc<eggress_transport_ssh::SshSessionCache>>,
    /// Optional Shadowsocks-specific metrics for observability.
    #[cfg(feature = "extended")]
    pub shadowsocks_metrics: Option<Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>>,
    #[cfg(not(feature = "extended"))]
    pub shadowsocks_metrics: Option<()>,
}

/// Handle a single inbound connection.
///
/// Every non-panicking return from this function goes through exactly one
/// terminal metrics finalization: after `record_session_start()`, exactly
/// one `record_session()` call is made before returning.
pub async fn serve_connection(
    client: eggress_core::BoxStream,
    config: ConnectionConfig,
) -> SessionReport {
    // Buffer reads so protocol handshakes that consume the head
    // incrementally do not issue one syscall per byte. Unconsumed
    // prefetch stays available to later reads on the same stream.
    let client: eggress_core::BoxStream = Box::new(tokio::io::BufReader::new(client));

    if let Some(metrics) = &config.metrics {
        metrics.record_session_start();
    }

    let accepted = tokio::time::timeout(
        config.handshake_timeout,
        accept::accept_with_fixed_target_for_peer(
            client,
            &config.protocols,
            &config.authentication,
            config.shadowsocks.as_ref(),
            config.shadowsocks_metrics.as_ref(),
            config.trojan.as_ref(),
            config.fixed_target.as_ref(),
            config.context.source.map(|peer| peer.ip()),
        ),
    )
    .await;

    let report = match accepted {
        Ok(Ok(session)) => execute::execute(session, &config).await,
        Ok(Err(accept::AcceptError::AuthenticationFailed)) => {
            if let Some(metrics) = &config.metrics {
                metrics.record_auth_failure();
            }
            SessionReport {
                protocol: None,
                target: None,
                route: "unknown".to_string(),
                bytes_upstream: 0,
                bytes_downstream: 0,
                outcome: execute::SessionOutcome::AuthenticationFailed,
                failure: Some(execute::FailureCategory::Authentication),
                rule_id: None,
                upstream_group: None,
                upstream_id: None,
                selection_reason: None,
            }
        }
        Ok(Err(_)) => SessionReport {
            protocol: None,
            target: None,
            route: "unknown".to_string(),
            bytes_upstream: 0,
            bytes_downstream: 0,
            outcome: execute::SessionOutcome::ClientProtocolError,
            failure: Some(execute::FailureCategory::Protocol),
            rule_id: None,
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
        },
        Err(_) => SessionReport {
            protocol: None,
            target: None,
            route: "unknown".to_string(),
            bytes_upstream: 0,
            bytes_downstream: 0,
            outcome: execute::SessionOutcome::HandshakeTimedOut,
            failure: Some(execute::FailureCategory::HandshakeTimeout),
            rule_id: None,
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
        },
    };

    if let Some(metrics) = &config.metrics {
        metrics.record_session(&report);
    }

    #[cfg(feature = "extended")]
    if let Some(ss_metrics) = &config.shadowsocks_metrics {
        if report.protocol.as_deref() == Some("shadowsocks") {
            ss_metrics.record_tcp_session_closed();
            ss_metrics.record_tcp_flow_close();
        }
    }

    tracing::info!(
        outcome = ?report.outcome,
        failure = ?report.failure,
        protocol = ?report.protocol,
        target = ?report.target,
        route = %report.route,
        rule = ?report.rule_id,
        upstream_group = ?report.upstream_group,
        upstream = ?report.upstream_id,
        selection_reason = ?report.selection_reason,
        bytes_upstream = report.bytes_upstream,
        bytes_downstream = report.bytes_downstream,
        "connection completed",
    );

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_routing::{RouteActionSpec, Router};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    fn all_protocols() -> Arc<[eggress_core::ProtocolId]> {
        Arc::from([
            eggress_core::ProtocolId::Http,
            eggress_core::ProtocolId::Socks4,
            eggress_core::ProtocolId::Socks5,
            eggress_core::ProtocolId::Http2,
            eggress_core::ProtocolId::WebSocket,
            eggress_core::ProtocolId::Raw,
        ])
    }

    fn direct_routing() -> Arc<dyn RouteService> {
        Arc::new(Router::new(vec![], RouteActionSpec::Direct))
    }

    fn test_config(routing: Arc<dyn RouteService>) -> ConnectionConfig {
        ConnectionConfig {
            routing,
            context: ConnectionContext::default(),
            handshake_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(10),
            protocols: all_protocols(),
            authentication: accept::InboundAuthentication::None,
            metrics: None,
            udp: None,
            tls_client_config: None,
            shadowsocks: None,
            shadowsocks_metrics: None,
            trojan: None,
            fixed_target: None,
            local_bind: None,
        }
    }

    #[tokio::test]
    async fn test_serve_connection_socks5_direct() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(direct_routing());
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
        match echo_addr.ip() {
            std::net::IpAddr::V4(ip) => {
                stream.write_all(&ip.octets()).await.unwrap();
            }
            std::net::IpAddr::V6(ip) => {
                stream.write_all(&ip.octets()).await.unwrap();
            }
        }
        stream
            .write_all(&echo_addr.port().to_be_bytes())
            .await
            .unwrap();

        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x00);

        stream.write_all(b"hello").await.unwrap();
        stream.shutdown().await.unwrap();

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));

        echo_jh.abort();
    }

    #[tokio::test]
    async fn test_serve_connection_http_connect_direct() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let _proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(direct_routing());
            serve_connection(boxed, config).await
        });

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

        echo_jh.abort();
    }

    async fn start_echo_origin() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let jh = tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    use tokio::io::AsyncWriteExt;

                    let mut head = Vec::new();
                    let mut tmp = [0u8; 1];
                    loop {
                        if stream.read(&mut tmp).await.unwrap_or(0) == 0 {
                            return;
                        }
                        head.push(tmp[0]);
                        if head.len() >= 4 && &head[head.len() - 4..] == b"\r\n\r\n" {
                            break;
                        }
                    }

                    let head_str = String::from_utf8_lossy(&head);
                    let mut content_length: Option<u64> = None;
                    let mut is_chunked = false;
                    for line in head_str.lines() {
                        if let Some((name, value)) = line.split_once(':') {
                            if name.eq_ignore_ascii_case("Content-Length") {
                                content_length = match value.trim().parse() {
                                    Ok(length) => Some(length),
                                    Err(_) => return,
                                };
                            } else if name.eq_ignore_ascii_case("Transfer-Encoding")
                                && value.trim().eq_ignore_ascii_case("chunked")
                            {
                                is_chunked = true;
                            }
                        }
                    }

                    let body = match (content_length, is_chunked) {
                        (Some(len), _) => {
                            let mut body = vec![0u8; len as usize];
                            let mut off = 0;
                            while off < body.len() {
                                let n = stream.read(&mut body[off..]).await.unwrap_or(0);
                                if n == 0 {
                                    break;
                                }
                                off += n;
                            }
                            body
                        }
                        (None, true) => {
                            let mut body = Vec::new();
                            loop {
                                let mut size_line = Vec::new();
                                loop {
                                    let n = stream.read(&mut tmp).await.unwrap_or(0);
                                    if n == 0 {
                                        return;
                                    }
                                    size_line.push(tmp[0]);
                                    if size_line.len() >= 2
                                        && &size_line[size_line.len() - 2..] == b"\r\n"
                                    {
                                        break;
                                    }
                                }
                                let size_str =
                                    String::from_utf8_lossy(&size_line[..size_line.len() - 2]);
                                let chunk_size = match usize::from_str_radix(size_str.trim(), 16) {
                                    Ok(size) => size,
                                    Err(_) => return,
                                };
                                if chunk_size == 0 {
                                    let mut trail = [0u8; 2];
                                    let _ = stream.read_exact(&mut trail).await;
                                    break;
                                }
                                let mut chunk = vec![0u8; chunk_size];
                                let mut off = 0;
                                while off < chunk.len() {
                                    let n = stream.read(&mut chunk[off..]).await.unwrap_or(0);
                                    if n == 0 {
                                        return;
                                    }
                                    off += n;
                                }
                                body.extend_from_slice(&chunk);
                                let mut trail = [0u8; 2];
                                let _ = stream.read_exact(&mut trail).await;
                            }
                            body
                        }
                        _ => Vec::new(),
                    };

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.write_all(&body).await;
                    let _ = stream.shutdown().await;
                });
            }
        });

        (addr, jh)
    }

    #[tokio::test]
    async fn test_http_forward_post_content_length() {
        let (origin_addr, origin_jh) = start_echo_origin().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(direct_routing());
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let body = b"hello world";
        let request = format!(
            "POST http://{}:{} HTTP/1.1\r\nHost: {}:{}\r\nContent-Length: {}\r\n\r\n",
            origin_addr.ip(),
            origin_addr.port(),
            origin_addr.ip(),
            origin_addr.port(),
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response);
        assert!(
            response_str.ends_with("hello world"),
            "body not echoed: {response_str}"
        );

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));

        origin_jh.abort();
    }

    #[tokio::test]
    async fn test_http_forward_post_chunked() {
        let (origin_addr, origin_jh) = start_echo_origin().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(direct_routing());
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let body = b"chunked body";
        let request = format!(
            "POST http://{}:{} HTTP/1.1\r\nHost: {}:{}\r\nTransfer-Encoding: chunked\r\n\r\n",
            origin_addr.ip(),
            origin_addr.port(),
            origin_addr.ip(),
            origin_addr.port()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        stream
            .write_all(format!("{:x}\r\n", body.len()).as_bytes())
            .await
            .unwrap();
        stream.write_all(body).await.unwrap();
        stream.write_all(b"\r\n").await.unwrap();
        stream.write_all(b"0\r\n\r\n").await.unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response);
        assert!(
            response_str.ends_with("chunked body"),
            "body not echoed: {response_str}"
        );

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));

        origin_jh.abort();
    }

    #[tokio::test]
    async fn test_http_forward_get_no_body() {
        let (origin_addr, origin_jh) = start_echo_origin().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(direct_routing());
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let request = format!(
            "GET http://{}:{}/ HTTP/1.1\r\nHost: {}:{}\r\n\r\n",
            origin_addr.ip(),
            origin_addr.port(),
            origin_addr.ip(),
            origin_addr.port()
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response);
        assert!(
            response_str.contains("200 OK"),
            "expected 200, got: {response_str}"
        );
        let body_start = response_str.find("\r\n\r\n").unwrap() + 4;
        let body = &response_str[body_start..];
        assert!(body.is_empty(), "expected empty body for GET, got: {body}");

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));

        origin_jh.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn test_handshake_timeout_no_bytes() {
        let (_client_stream, server_stream) = tokio::io::duplex(1024);
        let boxed: eggress_core::BoxStream = Box::new(server_stream);
        let config = test_config(direct_routing());

        let task = tokio::spawn(serve_connection(boxed, config));

        tokio::time::advance(Duration::from_secs(6)).await;

        let report = task.await.unwrap();
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::HandshakeTimedOut
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn test_handshake_timeout_partial_http() {
        let (mut client_stream, server_stream) = tokio::io::duplex(1024);
        let boxed: eggress_core::BoxStream = Box::new(server_stream);
        let config = test_config(direct_routing());

        let task = tokio::spawn(serve_connection(boxed, config));

        client_stream.write_all(b"CON").await.unwrap();
        tokio::time::advance(Duration::from_secs(6)).await;

        let report = task.await.unwrap();
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::HandshakeTimedOut
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn test_handshake_timeout_partial_socks5() {
        let (mut client_stream, server_stream) = tokio::io::duplex(1024);
        let boxed: eggress_core::BoxStream = Box::new(server_stream);
        let config = test_config(direct_routing());

        let task = tokio::spawn(serve_connection(boxed, config));

        client_stream.write_all(&[0x05]).await.unwrap();
        tokio::time::advance(Duration::from_secs(6)).await;

        let report = task.await.unwrap();
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::HandshakeTimedOut
        ));
    }

    #[tokio::test]
    async fn test_handshake_completes_before_timeout() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(direct_routing());
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
        match echo_addr.ip() {
            std::net::IpAddr::V4(ip) => {
                stream.write_all(&ip.octets()).await.unwrap();
            }
            std::net::IpAddr::V6(ip) => {
                stream.write_all(&ip.octets()).await.unwrap();
            }
        }
        stream
            .write_all(&echo_addr.port().to_be_bytes())
            .await
            .unwrap();

        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x00);

        stream.write_all(b"hello").await.unwrap();
        stream.shutdown().await.unwrap();

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));

        echo_jh.abort();
    }

    #[tokio::test]
    async fn test_http_forward_get_reports_nonzero_bytes() {
        let (origin_addr, origin_jh) = start_echo_origin().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(direct_routing());
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let request = format!(
            "GET http://{}:{}/ HTTP/1.1\r\nHost: {}:{}\r\n\r\n",
            origin_addr.ip(),
            origin_addr.port(),
            origin_addr.ip(),
            origin_addr.port()
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));
        assert!(
            report.bytes_upstream > 0,
            "upstream bytes should be nonzero"
        );
        assert!(
            report.bytes_downstream > 0,
            "downstream bytes should be nonzero"
        );

        origin_jh.abort();
    }

    #[tokio::test]
    async fn test_http_forward_post_reports_body_bytes() {
        let (origin_addr, origin_jh) = start_echo_origin().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(direct_routing());
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let body = b"hello world";
        let request = format!(
            "POST http://{}:{} HTTP/1.1\r\nHost: {}:{}\r\nContent-Length: {}\r\n\r\n",
            origin_addr.ip(),
            origin_addr.port(),
            origin_addr.ip(),
            origin_addr.port(),
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));
        assert!(
            report.bytes_upstream > body.len() as u64,
            "upstream bytes ({}) should exceed body length ({})",
            report.bytes_upstream,
            body.len()
        );
        assert!(report.bytes_downstream > 0);

        origin_jh.abort();
    }

    #[tokio::test]
    async fn test_successful_session_has_no_failure() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(direct_routing());
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
        match echo_addr.ip() {
            std::net::IpAddr::V4(ip) => {
                stream.write_all(&ip.octets()).await.unwrap();
            }
            std::net::IpAddr::V6(ip) => {
                stream.write_all(&ip.octets()).await.unwrap();
            }
        }
        stream
            .write_all(&echo_addr.port().to_be_bytes())
            .await
            .unwrap();

        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x00);

        stream.write_all(b"hello").await.unwrap();
        stream.shutdown().await.unwrap();

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));
        assert_eq!(report.failure, None);

        echo_jh.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn test_handshake_timeout_maps_to_failure_category() {
        let (_client_stream, server_stream) = tokio::io::duplex(1024);
        let boxed: eggress_core::BoxStream = Box::new(server_stream);
        let config = test_config(direct_routing());

        let task = tokio::spawn(serve_connection(boxed, config));

        tokio::time::advance(Duration::from_secs(6)).await;

        let report = task.await.unwrap();
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::HandshakeTimedOut
        ));
        assert_eq!(
            report.failure,
            Some(execute::FailureCategory::HandshakeTimeout)
        );
    }

    #[tokio::test]
    async fn test_failure_category_from_session_open_error_dns() {
        let error = SessionOpenError::Dns;
        let category = execute::FailureCategory::from(&error);
        assert_eq!(category, execute::FailureCategory::Dns);
    }

    #[tokio::test]
    async fn test_failure_category_from_session_open_error_refused() {
        let error = SessionOpenError::Refused;
        let category = execute::FailureCategory::from(&error);
        assert_eq!(category, execute::FailureCategory::ConnectionRefused);
    }

    #[tokio::test]
    async fn test_failure_category_from_session_open_error_network_unreachable() {
        let error = SessionOpenError::NetworkUnreachable;
        let category = execute::FailureCategory::from(&error);
        assert_eq!(category, execute::FailureCategory::NetworkUnreachable);
    }

    #[tokio::test]
    async fn test_failure_category_from_session_open_error_host_unreachable() {
        let error = SessionOpenError::HostUnreachable;
        let category = execute::FailureCategory::from(&error);
        assert_eq!(category, execute::FailureCategory::HostUnreachable);
    }

    #[tokio::test]
    async fn test_failure_category_from_session_open_error_timeout() {
        let error = SessionOpenError::Timeout;
        let category = execute::FailureCategory::from(&error);
        assert_eq!(category, execute::FailureCategory::RouteTimeout);
    }

    #[tokio::test]
    async fn test_failure_category_from_session_open_error_upstream_auth() {
        let error = SessionOpenError::UpstreamAuthentication;
        let category = execute::FailureCategory::from(&error);
        assert_eq!(category, execute::FailureCategory::UpstreamAuthentication);
    }

    #[tokio::test]
    async fn test_failure_category_from_io_error_connection_refused() {
        let error = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let category = execute::FailureCategory::from_io_error(&error);
        assert_eq!(category, execute::FailureCategory::ConnectionRefused);
    }

    #[tokio::test]
    async fn test_failure_category_from_io_error_connection_reset() {
        let error = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
        let category = execute::FailureCategory::from_io_error(&error);
        assert_eq!(category, execute::FailureCategory::Relay);
    }

    #[tokio::test]
    async fn test_failure_category_from_io_error_timeout() {
        let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
        let category = execute::FailureCategory::from_io_error(&error);
        assert_eq!(category, execute::FailureCategory::Relay);
    }

    #[tokio::test]
    async fn test_route_failure_maps_to_dns_category() {
        let report = execute::SessionReport::open_failed(
            SessionOpenError::Dns,
            Some("socks5".to_string()),
            Some("example.com:443".to_string()),
            "direct".to_string(),
        );
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::RouteFailed
        ));
        assert_eq!(report.failure, Some(execute::FailureCategory::Dns));
    }

    #[tokio::test]
    async fn test_route_failure_maps_to_connection_refused_category() {
        let report = execute::SessionReport::open_failed(
            SessionOpenError::Refused,
            Some("http".to_string()),
            Some("10.0.0.1:80".to_string()),
            "chain(2)".to_string(),
        );
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::RouteFailed
        ));
        assert_eq!(
            report.failure,
            Some(execute::FailureCategory::ConnectionRefused)
        );
    }

    #[tokio::test]
    async fn test_completed_session_has_no_failure() {
        let report = execute::SessionReport::completed(
            Some("socks5".to_string()),
            Some("example.com:443".to_string()),
            "direct".to_string(),
            1024,
            2048,
        );
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));
        assert_eq!(report.failure, None);
        assert_eq!(report.bytes_upstream, 1024);
        assert_eq!(report.bytes_downstream, 2048);
    }

    #[tokio::test]
    async fn test_cancelled_session_has_cancelled_failure() {
        let report = execute::SessionReport::cancelled(
            Some("http".to_string()),
            Some("example.com:80".to_string()),
            "direct".to_string(),
        );
        assert!(matches!(report.outcome, execute::SessionOutcome::Cancelled));
        assert_eq!(report.failure, Some(execute::FailureCategory::Cancelled));
    }

    #[tokio::test]
    async fn test_authentication_failure_maps_to_failure_category() {
        let auth = accept::InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "secret".to_string(),
        };

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let mut cfg = test_config(direct_routing());
            cfg.authentication = auth;
            let config = cfg;
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream.write_all(&[0x05, 0x02, 0x00, 0x02]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x02]);

        stream
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r', 0x05])
            .await
            .unwrap();
        stream.write_all(b"wrong").await.unwrap();
        let mut auth_resp = [0u8; 2];
        stream.read_exact(&mut auth_resp).await.unwrap();
        assert_eq!(auth_resp, [0x01, 0x01]);

        let report = proxy_jh.await.unwrap();
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::AuthenticationFailed
        ));
        assert_eq!(
            report.failure,
            Some(execute::FailureCategory::Authentication)
        );
    }

    #[tokio::test]
    async fn test_reject_route_returns_403_for_http() {
        let rules = vec![eggress_routing::CompiledRule {
            id: eggress_routing::RuleId(std::sync::Arc::from("block")),
            matcher: eggress_routing::MatchExpr::Any,
            action: eggress_routing::RouteActionSpec::Reject(
                eggress_core::RejectReason::AccessDenied,
            ),
        }];
        let routing: Arc<dyn RouteService> =
            Arc::new(Router::new(rules, eggress_routing::RouteActionSpec::Direct));

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let _proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(routing);
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let request = "GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n";
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response);
        assert!(
            response_str.contains("403"),
            "expected 403, got: {response_str}"
        );
    }

    #[tokio::test]
    async fn test_source_cidr_matching_with_real_peer() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let rules = vec![eggress_routing::CompiledRule {
            id: eggress_routing::RuleId(std::sync::Arc::from("allow-localhost")),
            matcher: eggress_routing::MatchExpr::SourceCidr("127.0.0.0/8".parse().unwrap()),
            action: eggress_routing::RouteActionSpec::Direct,
        }];
        let routing: Arc<dyn RouteService> =
            Arc::new(Router::new(rules, eggress_routing::RouteActionSpec::Direct));

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, peer) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let mut cfg = test_config(routing.clone());
            cfg.context = ConnectionContext {
                source: Some(peer),
                listener: "test-listener".to_string(),
                generation: 0,
            };
            let config = cfg;
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
        match echo_addr.ip() {
            std::net::IpAddr::V4(ip) => {
                stream.write_all(&ip.octets()).await.unwrap();
            }
            std::net::IpAddr::V6(ip) => {
                stream.write_all(&ip.octets()).await.unwrap();
            }
        }
        stream
            .write_all(&echo_addr.port().to_be_bytes())
            .await
            .unwrap();

        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x00);

        stream.write_all(b"hello").await.unwrap();
        stream.shutdown().await.unwrap();

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));

        echo_jh.abort();
    }

    #[tokio::test]
    async fn test_http_expectation_is_rejected_with_417_and_connection_close() {
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = test_config(direct_routing());
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream
            .write_all(
                b"POST http://127.0.0.1:1/ HTTP/1.1\r\nHost: 127.0.0.1:1\r\nExpect: 100-continue\r\nContent-Length: 1048576\r\n\r\n",
            )
            .await
            .unwrap();

        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(1), stream.read_to_end(&mut response))
            .await
            .expect("expectation rejection must be bounded")
            .unwrap();
        assert!(String::from_utf8_lossy(&response).starts_with("HTTP/1.1 417 Expectation Failed"));

        let report = proxy_jh.await.unwrap();
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::ClientProtocolError
        ));
    }

    #[tokio::test]
    async fn test_http_body_upload_is_not_limited_by_connect_timeout() {
        let origin_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin_addr = origin_listener.local_addr().unwrap();
        let origin_jh = tokio::spawn(async move {
            let (stream, _) = origin_listener.accept().await.unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = Vec::new();
            loop {
                line.clear();
                reader.read_until(b'\n', &mut line).await.unwrap();
                if line == b"\r\n" {
                    break;
                }
            }
            let mut body = [0u8; 16];
            reader.read_exact(&mut body).await.unwrap();
            let mut stream = reader.into_inner();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
        });

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let mut config = test_config(direct_routing());
            config.connect_timeout = Duration::from_millis(50);
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let request = format!(
            "POST http://{}:{}/ HTTP/1.1\r\nHost: {}:{}\r\nContent-Length: 16\r\n\r\n",
            origin_addr.ip(),
            origin_addr.port(),
            origin_addr.ip(),
            origin_addr.port()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        stream.write_all(b"delayed-body-123").await.unwrap();

        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(1), stream.read_to_end(&mut response))
            .await
            .expect("body upload must not inherit the connect timeout")
            .unwrap();
        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));

        origin_jh.abort();
    }
}

/// Tests proving that session metrics are structurally balanced: one
/// `record_session_start()` followed by exactly one `record_session()` for
/// every path through `serve_connection()`.
#[cfg(test)]
mod metrics_lifecycle_tests {
    use super::*;
    use eggress_routing::{RouteActionSpec, Router};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A test-only metrics implementation that counts calls.
    struct RecordingMetrics {
        starts: AtomicUsize,
        terminals: AtomicUsize,
        auth_failures: AtomicUsize,
        terminal_reports: Mutex<Vec<execute::SessionOutcome>>,
    }

    impl RecordingMetrics {
        fn new() -> Self {
            Self {
                starts: AtomicUsize::new(0),
                terminals: AtomicUsize::new(0),
                auth_failures: AtomicUsize::new(0),
                terminal_reports: Mutex::new(Vec::new()),
            }
        }
    }

    impl SessionMetrics for RecordingMetrics {
        fn record_session_start(&self) {
            self.starts.fetch_add(1, Ordering::SeqCst);
        }
        fn record_session(&self, report: &SessionReport) {
            self.terminals.fetch_add(1, Ordering::SeqCst);
            self.terminal_reports
                .lock()
                .unwrap()
                .push(std::mem::replace(
                    &mut report.outcome.clone_outcome(),
                    execute::SessionOutcome::Completed,
                ));
        }
        fn record_auth_failure(&self) {
            self.auth_failures.fetch_add(1, Ordering::SeqCst);
        }
        fn record_route_decision(&self, _: &str, _: &str, _: &str) {}
        fn record_upstream_open(&self, _: &str, _: &str) {}
        fn record_upstream_failure(&self, _: &str, _: &str) {}
    }

    impl execute::SessionOutcome {
        fn clone_outcome(&self) -> Self {
            match self {
                Self::Completed => Self::Completed,
                Self::ClientProtocolError => Self::ClientProtocolError,
                Self::AuthenticationFailed => Self::AuthenticationFailed,
                Self::HandshakeTimedOut => Self::HandshakeTimedOut,
                Self::RouteFailed => Self::RouteFailed,
                Self::RelayFailed => Self::RelayFailed,
                Self::Cancelled => Self::Cancelled,
            }
        }
    }

    fn test_direct_routing() -> Arc<dyn RouteService> {
        Arc::new(Router::new(vec![], RouteActionSpec::Direct))
    }

    fn test_all_protocols() -> Arc<[eggress_core::ProtocolId]> {
        Arc::from([
            eggress_core::ProtocolId::Http,
            eggress_core::ProtocolId::Socks4,
            eggress_core::ProtocolId::Socks5,
            eggress_core::ProtocolId::Http2,
            eggress_core::ProtocolId::WebSocket,
            eggress_core::ProtocolId::Raw,
        ])
    }

    fn metrics_config(metrics: Arc<RecordingMetrics>) -> ConnectionConfig {
        ConnectionConfig {
            routing: test_direct_routing(),
            context: ConnectionContext::default(),
            handshake_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(10),
            protocols: test_all_protocols(),
            authentication: accept::InboundAuthentication::None,
            metrics: Some(metrics),
            udp: None,
            tls_client_config: None,
            shadowsocks: None,
            shadowsocks_metrics: None,
            trojan: None,
            fixed_target: None,
            local_bind: None,
        }
    }

    #[tokio::test]
    async fn metrics_balanced_after_successful_session() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
        let metrics = Arc::new(RecordingMetrics::new());
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let m = metrics.clone();
        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = metrics_config(m);
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
        match echo_addr.ip() {
            std::net::IpAddr::V4(ip) => stream.write_all(&ip.octets()).await.unwrap(),
            std::net::IpAddr::V6(ip) => stream.write_all(&ip.octets()).await.unwrap(),
        }
        stream
            .write_all(&echo_addr.port().to_be_bytes())
            .await
            .unwrap();

        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        stream.write_all(b"hello").await.unwrap();
        stream.shutdown().await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));

        let m = &*metrics;
        assert_eq!(m.starts.load(Ordering::SeqCst), 1, "expected 1 start");
        assert_eq!(m.terminals.load(Ordering::SeqCst), 1, "expected 1 terminal");
        assert_eq!(
            m.auth_failures.load(Ordering::SeqCst),
            0,
            "no auth failures"
        );
        echo_jh.abort();
    }

    #[tokio::test]
    async fn metrics_balanced_after_auth_failure() {
        let auth = accept::InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "secret".to_string(),
        };
        let metrics = Arc::new(RecordingMetrics::new());
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let m = metrics.clone();
        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let mut cfg = metrics_config(m);
            cfg.authentication = auth;
            serve_connection(boxed, cfg).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream.write_all(&[0x05, 0x02, 0x00, 0x02]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x02]);

        stream
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r', 0x05])
            .await
            .unwrap();
        stream.write_all(b"wrong").await.unwrap();
        let mut auth_resp = [0u8; 2];
        stream.read_exact(&mut auth_resp).await.unwrap();

        let report = proxy_jh.await.unwrap();
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::AuthenticationFailed
        ));

        let m = &*metrics;
        assert_eq!(m.starts.load(Ordering::SeqCst), 1);
        assert_eq!(m.terminals.load(Ordering::SeqCst), 1);
        assert_eq!(m.auth_failures.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn metrics_balanced_after_protocol_error() {
        let metrics = Arc::new(RecordingMetrics::new());
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let m = metrics.clone();
        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            serve_connection(boxed, metrics_config(m)).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream.write_all(b"garbage data").await.unwrap();
        stream.shutdown().await.unwrap();

        let report = proxy_jh.await.unwrap();
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::ClientProtocolError
        ));

        let m = &*metrics;
        assert_eq!(m.starts.load(Ordering::SeqCst), 1);
        assert_eq!(m.terminals.load(Ordering::SeqCst), 1);
        assert_eq!(m.auth_failures.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn metrics_balanced_after_handshake_timeout() {
        let metrics = Arc::new(RecordingMetrics::new());
        let (_client_stream, server_stream) = tokio::io::duplex(1024);
        let boxed: eggress_core::BoxStream = Box::new(server_stream);
        let task = tokio::spawn(serve_connection(boxed, metrics_config(metrics.clone())));
        tokio::time::advance(Duration::from_secs(6)).await;

        let report = task.await.unwrap();
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::HandshakeTimedOut
        ));

        let m = &*metrics;
        assert_eq!(m.starts.load(Ordering::SeqCst), 1);
        assert_eq!(m.terminals.load(Ordering::SeqCst), 1);
        assert_eq!(m.auth_failures.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn no_double_finalization_for_route_failure() {
        let rules = vec![eggress_routing::CompiledRule {
            id: eggress_routing::RuleId(std::sync::Arc::from("block")),
            matcher: eggress_routing::MatchExpr::Any,
            action: eggress_routing::RouteActionSpec::Reject(
                eggress_core::RejectReason::AccessDenied,
            ),
        }];
        let routing: Arc<dyn eggress_routing::RouteService> =
            Arc::new(Router::new(rules, eggress_routing::RouteActionSpec::Direct));
        let metrics = Arc::new(RecordingMetrics::new());
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let m = metrics.clone();
        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let mut cfg = metrics_config(m);
            cfg.routing = routing;
            serve_connection(boxed, cfg).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let request = "GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n";
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();

        let report = proxy_jh.await.unwrap();
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::RouteFailed | execute::SessionOutcome::ClientProtocolError
        ));

        let m = &*metrics;
        assert_eq!(m.starts.load(Ordering::SeqCst), 1);
        assert_eq!(m.terminals.load(Ordering::SeqCst), 1);
    }
}

/// Negative tests for lean (common-only) build.
///
/// These tests verify that excluded protocol capabilities fail clearly
/// at the accept boundary, never silently degrading. They run only when
/// the `extended` feature is disabled (lean build).
#[cfg(all(test, not(feature = "extended")))]
mod lean_negative_tests {
    use super::*;
    use eggress_core::ProtocolId;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn lean_rejects_shadowsocks_accept() {
        use eggress_routing::{RouteActionSpec, Router};

        let routing: Arc<dyn eggress_routing::RouteService> =
            Arc::new(Router::new(vec![], RouteActionSpec::Direct));
        let protocols: Arc<[ProtocolId]> = Arc::from([ProtocolId::Shadowsocks]);
        let (_client, server) = tokio::io::duplex(1024);
        let boxed: eggress_core::BoxStream = Box::new(server);

        let config = ConnectionConfig {
            routing,
            context: ConnectionContext::default(),
            handshake_timeout: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(5),
            protocols,
            authentication: accept::InboundAuthentication::None,
            metrics: None,
            udp: None,
            tls_client_config: None,
            shadowsocks: Some(accept::InboundShadowsocksConfig {
                method: "aes-256-gcm".to_string(),
                password: "test-password".to_string(),
            }),
            shadowsocks_metrics: None,
            trojan: None,
            fixed_target: None,
            local_bind: None,
        };

        let report = serve_connection(boxed, config).await;
        // In lean build, shadowsocks accept should fail with a protocol error
        // because the feature is not included.
        assert!(
            matches!(
                report.outcome,
                execute::SessionOutcome::ClientProtocolError
                    | execute::SessionOutcome::HandshakeTimedOut
            ),
            "shadowsocks should fail in lean build, got: {:?}",
            report.outcome
        );
    }

    #[tokio::test]
    async fn lean_rejects_trojan_accept() {
        use eggress_routing::{RouteActionSpec, Router};

        let routing: Arc<dyn eggress_routing::RouteService> =
            Arc::new(Router::new(vec![], RouteActionSpec::Direct));
        let protocols: Arc<[ProtocolId]> = Arc::from([ProtocolId::Trojan]);
        let (_client, server) = tokio::io::duplex(1024);
        let boxed: eggress_core::BoxStream = Box::new(server);

        let config = ConnectionConfig {
            routing,
            context: ConnectionContext::default(),
            handshake_timeout: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(5),
            protocols,
            authentication: accept::InboundAuthentication::None,
            metrics: None,
            udp: None,
            tls_client_config: None,
            shadowsocks: None,
            shadowsocks_metrics: None,
            trojan: Some(accept::InboundTrojanConfig {
                password: "test-password".to_string(),
                fallback: None,
            }),
            fixed_target: None,
            local_bind: None,
        };

        let report = serve_connection(boxed, config).await;
        // In lean build, trojan accept should fail with a protocol error
        // because the feature is not included.
        assert!(
            matches!(
                report.outcome,
                execute::SessionOutcome::ClientProtocolError
                    | execute::SessionOutcome::HandshakeTimedOut
            ),
            "trojan should fail in lean build, got: {:?}",
            report.outcome
        );
    }

    #[tokio::test]
    async fn lean_rejects_websocket_accept() {
        use eggress_routing::{RouteActionSpec, Router};

        let routing: Arc<dyn eggress_routing::RouteService> =
            Arc::new(Router::new(vec![], RouteActionSpec::Direct));
        let protocols: Arc<[ProtocolId]> = Arc::from([ProtocolId::WebSocket]);
        let (mut client, server) = tokio::io::duplex(1024);
        let boxed: eggress_core::BoxStream = Box::new(server);
        let config = ConnectionConfig {
            routing,
            context: ConnectionContext::default(),
            handshake_timeout: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(5),
            protocols,
            authentication: accept::InboundAuthentication::None,
            metrics: None,
            udp: None,
            tls_client_config: None,
            shadowsocks: None,
            shadowsocks_metrics: None,
            trojan: None,
            fixed_target: None,
            local_bind: None,
        };
        client.write_all(&[0xff]).await.unwrap();

        let report = serve_connection(boxed, config).await;
        assert!(matches!(
            report.outcome,
            execute::SessionOutcome::ClientProtocolError
        ));
    }

    #[tokio::test]
    async fn lean_serves_http_and_socks_normally() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let routing: Arc<dyn eggress_routing::RouteService> = Arc::new(
                eggress_routing::Router::new(vec![], eggress_routing::RouteActionSpec::Direct),
            );
            let config = ConnectionConfig {
                routing,
                context: ConnectionContext::default(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
                protocols: Arc::from([ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5]),
                authentication: accept::InboundAuthentication::None,
                metrics: None,
                udp: None,
                tls_client_config: None,
                shadowsocks: None,
                shadowsocks_metrics: None,
                trojan: None,
                fixed_target: None,
                local_bind: None,
            };
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
        match echo_addr.ip() {
            std::net::IpAddr::V4(ip) => {
                stream.write_all(&ip.octets()).await.unwrap();
            }
            std::net::IpAddr::V6(ip) => {
                stream.write_all(&ip.octets()).await.unwrap();
            }
        }
        stream
            .write_all(&echo_addr.port().to_be_bytes())
            .await
            .unwrap();

        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x00);

        stream.write_all(b"hello").await.unwrap();
        stream.shutdown().await.unwrap();

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        let report = proxy_jh.await.unwrap();
        assert!(matches!(report.outcome, execute::SessionOutcome::Completed));

        echo_jh.abort();
    }
}
