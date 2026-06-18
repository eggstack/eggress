pub mod accept;
pub mod error;
pub mod execute;
pub mod reply;

use std::time::Duration;

pub use accept::AcceptedSession;
pub use error::SessionOpenError;
pub use execute::SessionReport;

/// Configuration for a single connection.
pub struct ConnectionConfig {
    pub route: RouteConfig,
    pub handshake_timeout: Duration,
    pub protocols: std::sync::Arc<[eggress_core::ProtocolId]>,
    pub authentication: accept::InboundAuthentication,
}

/// How to route to the target.
pub enum RouteConfig {
    Direct,
    Chain(eggress_uri::ProxyChainSpec),
}

/// Handle a single inbound connection.
///
/// This is the main entry point. It:
/// 1. Detects the protocol
/// 2. Parses the inbound request (without sending replies)
/// 3. Opens the outbound route
/// 4. Sends protocol-specific success or failure
/// 5. Relays or forwards data
/// 6. Returns a session report
pub async fn serve_connection(
    client: eggress_core::BoxStream,
    config: ConnectionConfig,
) -> SessionReport {
    let route = match &config.route {
        RouteConfig::Direct => "direct".to_string(),
        RouteConfig::Chain(spec) => format!("chain({})", spec.hops.len()),
    };

    let accepted = tokio::time::timeout(
        config.handshake_timeout,
        accept::accept(client, &config.protocols, &config.authentication),
    )
    .await;

    let session = match accepted {
        Ok(Ok(session)) => session,
        Ok(Err(accept::AcceptError::AuthenticationFailed)) => {
            return SessionReport {
                protocol: None,
                target: None,
                route,
                bytes_upstream: 0,
                bytes_downstream: 0,
                outcome: execute::SessionOutcome::AuthenticationFailed,
            };
        }
        Ok(Err(_)) => {
            return SessionReport {
                protocol: None,
                target: None,
                route,
                bytes_upstream: 0,
                bytes_downstream: 0,
                outcome: execute::SessionOutcome::ClientProtocolError,
            };
        }
        Err(_) => {
            return SessionReport {
                protocol: None,
                target: None,
                route,
                bytes_upstream: 0,
                bytes_downstream: 0,
                outcome: execute::SessionOutcome::HandshakeTimedOut,
            };
        }
    };

    execute::execute(session, &config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn all_protocols() -> std::sync::Arc<[eggress_core::ProtocolId]> {
        std::sync::Arc::from([
            eggress_core::ProtocolId::Http,
            eggress_core::ProtocolId::Socks4,
            eggress_core::ProtocolId::Socks5,
        ])
    }

    #[tokio::test]
    async fn test_serve_connection_socks5_direct() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = ConnectionConfig {
                route: RouteConfig::Direct,
                handshake_timeout: Duration::from_secs(5),
                protocols: all_protocols(),
                authentication: accept::InboundAuthentication::None,
            };
            serve_connection(boxed, config).await
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        // Method negotiation
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // CONNECT request
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

        // Read connect reply
        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x00);

        // Send data and verify echo
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
            let config = ConnectionConfig {
                route: RouteConfig::Direct,
                handshake_timeout: Duration::from_secs(5),
                protocols: all_protocols(),
                authentication: accept::InboundAuthentication::None,
            };
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

    /// Start a simple HTTP origin server that reads the full request (headers + body)
    /// and echoes the body back in the response.
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

                    // Read headers
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
                                content_length = value.trim().parse().ok();
                            } else if name.eq_ignore_ascii_case("Transfer-Encoding")
                                && value.trim().eq_ignore_ascii_case("chunked")
                            {
                                is_chunked = true;
                            }
                        }
                    }

                    // Read body
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
                                let chunk_size =
                                    usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
                                if chunk_size == 0 {
                                    // Read trailing \r\n
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
                                // Read trailing \r\n after chunk data
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
            let config = ConnectionConfig {
                route: RouteConfig::Direct,
                handshake_timeout: Duration::from_secs(5),
                protocols: all_protocols(),
                authentication: accept::InboundAuthentication::None,
            };
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
            let config = ConnectionConfig {
                route: RouteConfig::Direct,
                handshake_timeout: Duration::from_secs(5),
                protocols: all_protocols(),
                authentication: accept::InboundAuthentication::None,
            };
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
        // Send one chunk
        stream
            .write_all(format!("{:x}\r\n", body.len()).as_bytes())
            .await
            .unwrap();
        stream.write_all(body).await.unwrap();
        stream.write_all(b"\r\n").await.unwrap();
        // Send zero chunk
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
            let config = ConnectionConfig {
                route: RouteConfig::Direct,
                handshake_timeout: Duration::from_secs(5),
                protocols: all_protocols(),
                authentication: accept::InboundAuthentication::None,
            };
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
        // GET response body should be empty (origin echoes empty body)
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
        let config = ConnectionConfig {
            route: RouteConfig::Direct,
            handshake_timeout: Duration::from_secs(5),
            protocols: all_protocols(),
            authentication: accept::InboundAuthentication::None,
        };

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
        let config = ConnectionConfig {
            route: RouteConfig::Direct,
            handshake_timeout: Duration::from_secs(5),
            protocols: all_protocols(),
            authentication: accept::InboundAuthentication::None,
        };

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
        let config = ConnectionConfig {
            route: RouteConfig::Direct,
            handshake_timeout: Duration::from_secs(5),
            protocols: all_protocols(),
            authentication: accept::InboundAuthentication::None,
        };

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
            let config = ConnectionConfig {
                route: RouteConfig::Direct,
                handshake_timeout: Duration::from_secs(5),
                protocols: all_protocols(),
                authentication: accept::InboundAuthentication::None,
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

    #[tokio::test]
    async fn test_http_forward_get_reports_nonzero_bytes() {
        let (origin_addr, origin_jh) = start_echo_origin().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = ConnectionConfig {
                route: RouteConfig::Direct,
                handshake_timeout: Duration::from_secs(5),
                protocols: all_protocols(),
                authentication: accept::InboundAuthentication::None,
            };
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
            let config = ConnectionConfig {
                route: RouteConfig::Direct,
                handshake_timeout: Duration::from_secs(5),
                protocols: all_protocols(),
                authentication: accept::InboundAuthentication::None,
            };
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
        // Upstream should include head + body
        assert!(
            report.bytes_upstream > body.len() as u64,
            "upstream bytes ({}) should exceed body length ({})",
            report.bytes_upstream,
            body.len()
        );
        assert!(report.bytes_downstream > 0);

        origin_jh.abort();
    }
}
