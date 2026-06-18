pub mod accept;
pub mod error;
pub mod execute;
pub mod reply;

use std::sync::Arc;
use std::time::Duration;

pub use accept::AcceptedSession;
pub use error::SessionOpenError;
pub use execute::{FailureCategory, SessionReport};

use eggress_routing::RouteService;

/// Configuration for a single connection.
pub struct ConnectionConfig {
    pub routing: Arc<dyn RouteService>,
    pub handshake_timeout: Duration,
    pub connect_timeout: Duration,
    pub protocols: Arc<[eggress_core::ProtocolId]>,
    pub authentication: accept::InboundAuthentication,
}

/// Handle a single inbound connection.
pub async fn serve_connection(
    client: eggress_core::BoxStream,
    config: ConnectionConfig,
) -> SessionReport {
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
                route: "unknown".to_string(),
                bytes_upstream: 0,
                bytes_downstream: 0,
                outcome: execute::SessionOutcome::AuthenticationFailed,
                failure: Some(execute::FailureCategory::Authentication),
                rule_id: None,
                upstream_group: None,
                upstream_id: None,
            };
        }
        Ok(Err(_)) => {
            return SessionReport {
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
            };
        }
        Err(_) => {
            return SessionReport {
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
            };
        }
    };

    let report = execute::execute(session, &config).await;

    tracing::info!(
        outcome = ?report.outcome,
        failure = ?report.failure,
        protocol = ?report.protocol,
        target = ?report.target,
        route = %report.route,
        rule = ?report.rule_id,
        upstream_group = ?report.upstream_group,
        upstream = ?report.upstream_id,
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn all_protocols() -> Arc<[eggress_core::ProtocolId]> {
        Arc::from([
            eggress_core::ProtocolId::Http,
            eggress_core::ProtocolId::Socks4,
            eggress_core::ProtocolId::Socks5,
        ])
    }

    fn direct_routing() -> Arc<dyn RouteService> {
        Arc::new(Router::new(vec![], RouteActionSpec::Direct))
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
                routing: direct_routing(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
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
    async fn test_serve_connection_http_connect_direct() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let _proxy_jh = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let config = ConnectionConfig {
                routing: direct_routing(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
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
                                content_length = value.trim().parse().ok();
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
                                let chunk_size =
                                    usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
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
            let config = ConnectionConfig {
                routing: direct_routing(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
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
                routing: direct_routing(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
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
            let config = ConnectionConfig {
                routing: direct_routing(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
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
            routing: direct_routing(),
            handshake_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(10),
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
            routing: direct_routing(),
            handshake_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(10),
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
            routing: direct_routing(),
            handshake_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(10),
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
                routing: direct_routing(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
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
                routing: direct_routing(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
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
                routing: direct_routing(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
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
            let config = ConnectionConfig {
                routing: direct_routing(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
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
        assert_eq!(report.failure, None);

        echo_jh.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn test_handshake_timeout_maps_to_failure_category() {
        let (_client_stream, server_stream) = tokio::io::duplex(1024);
        let boxed: eggress_core::BoxStream = Box::new(server_stream);
        let config = ConnectionConfig {
            routing: direct_routing(),
            handshake_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(10),
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
    async fn test_cancelled_session_has_no_failure() {
        let report = execute::SessionReport::cancelled(
            Some("http".to_string()),
            Some("example.com:80".to_string()),
            "direct".to_string(),
        );
        assert!(matches!(report.outcome, execute::SessionOutcome::Cancelled));
        assert_eq!(report.failure, None);
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
            let config = ConnectionConfig {
                routing: direct_routing(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
                protocols: all_protocols(),
                authentication: auth,
            };
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
            let config = ConnectionConfig {
                routing,
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
                protocols: all_protocols(),
                authentication: accept::InboundAuthentication::None,
            };
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
}
