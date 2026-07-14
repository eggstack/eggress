//! HTTP proxy protocol implementation.
//!
//! This crate provides HTTP/1.1 proxy protocol handlers including
//! CONNECT tunneling and ordinary HTTP forwarding.

pub mod connect;
pub mod detect;
pub mod error;
pub mod forward;
pub mod h2_connect;

pub use connect::{
    handle_connect, http_connect, validate_credentials, ConnectRequest, HttpConnectLimits,
};
pub use detect::HttpDetector;
pub use error::HttpError;
pub use forward::{
    build_origin_request, copy_request_body, determine_request_body_kind, filter_hop_by_hop,
    forward_request, forward_request_stream, forward_response, BodyCopyLimits, BodyCopyReport,
    ForwardRequest, ForwardResponse, ForwardResponseReport, ForwardResult, RequestBodyKind,
};
pub use h2_connect::{
    h2_connect_client, h2_connect_client_pooled, h2_connect_relay, handle_h2_connect,
    H2ConnectError, H2PoolGuard, H2PoolKey, H2PoolRegistry, H2PoolStats, H2ProtocolMetrics,
    H2StreamRead, H2StreamWrite, H2_POOL_REGISTRY, H2_PROTOCOL_METRICS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::detect::{DetectResult, ProtocolDetector};
    use eggress_core::{BoxStream, TargetAddr, TargetHost};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_http_detector_identifies_http() {
        let detector = HttpDetector;
        assert_eq!(detector.id(), eggress_core::ProtocolId::Http);
        assert_eq!(
            detector.detect(b"GET / HTTP/1.1\r\n"),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[test]
    fn test_http_detector_identifies_connect() {
        let detector = HttpDetector;
        assert_eq!(
            detector.detect(b"CONNECT example.com:443 HTTP/1.1\r\n"),
            DetectResult::Match { confidence: 100 }
        );
    }

    // ===== Integration tests for HTTP CONNECT =====

    #[tokio::test]
    async fn test_connect_to_echo_server() {
        let (addr, jh) = eggress_testkit::start_echo_server().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (request, stream) = connect::handle_connect(boxed, false, None).await.unwrap();
            assert_eq!(
                request.target,
                TargetAddr {
                    host: TargetHost::Ip(addr.ip()),
                    port: addr.port(),
                }
            );

            // Connect to the target
            let target_stream = tokio::net::TcpStream::connect((addr.ip(), addr.port()))
                .await
                .unwrap();
            let (mut cr, mut cw) = tokio::io::split(stream);
            let (mut tr, mut tw) = tokio::io::split(target_stream);
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut cr, &mut tw).await;
            });
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut tr, &mut cw).await;
            });
        });

        let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip(addr.ip()),
            port: addr.port(),
        };
        let mut conn = connect::http_connect(boxed, &target, None, &Default::default())
            .await
            .unwrap();

        // Verify stream works after CONNECT
        conn.write_all(b"hello connect").await.unwrap();
        conn.shutdown().await.unwrap();

        let mut buf = [0u8; 13];
        conn.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello connect");

        server_jh.await.unwrap();
        jh.abort();
    }

    #[tokio::test]
    async fn test_connect_with_basic_auth() {
        let (addr, jh) = eggress_testkit::start_echo_server().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (request, stream) = connect::handle_connect(boxed, true, Some(("user", "pass")))
                .await
                .unwrap();
            assert_eq!(request.proxy_auth, Some(("user".into(), "pass".into())));

            let target_stream = tokio::net::TcpStream::connect((addr.ip(), addr.port()))
                .await
                .unwrap();
            let (mut cr, mut cw) = tokio::io::split(stream);
            let (mut tr, mut tw) = tokio::io::split(target_stream);
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut cr, &mut tw).await;
            });
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut tr, &mut cw).await;
            });
        });

        let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip(addr.ip()),
            port: addr.port(),
        };
        let mut conn =
            connect::http_connect(boxed, &target, Some(("user", "pass")), &Default::default())
                .await
                .unwrap();

        conn.write_all(b"auth test").await.unwrap();
        conn.shutdown().await.unwrap();

        let mut buf = [0u8; 9];
        conn.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"auth test");

        server_jh.await.unwrap();
        jh.abort();
    }

    #[tokio::test]
    async fn test_connect_auth_required() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = connect::handle_connect(boxed, true, Some(("user", "pass"))).await;
            assert!(result.is_err());
        });

        let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let result = connect::http_connect(boxed, &target, None, &Default::default()).await;
        assert!(result.is_err());
        match result {
            Err(HttpError::AuthRequired) => {}
            _ => panic!("expected AuthRequired error"),
        }

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_connect_with_domain_target() {
        let (addr, jh) = eggress_testkit::start_echo_server().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (request, stream) = connect::handle_connect(boxed, false, None).await.unwrap();
            assert_eq!(
                request.target,
                TargetAddr {
                    host: TargetHost::Domain("example.com".to_string()),
                    port: 443,
                }
            );

            let target_stream = tokio::net::TcpStream::connect((addr.ip(), addr.port()))
                .await
                .unwrap();
            let (mut cr, mut cw) = tokio::io::split(stream);
            let (mut tr, mut tw) = tokio::io::split(target_stream);
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut cr, &mut tw).await;
            });
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut tr, &mut cw).await;
            });
        });

        let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };
        let mut conn = connect::http_connect(boxed, &target, None, &Default::default())
            .await
            .unwrap();

        conn.write_all(b"domain test").await.unwrap();
        conn.shutdown().await.unwrap();

        let mut buf = [0u8; 11];
        conn.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"domain test");

        server_jh.await.unwrap();
        jh.abort();
    }

    #[tokio::test]
    async fn test_connect_with_ipv6_target() {
        let (addr, jh) = eggress_testkit::start_echo_server().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (request, stream) = connect::handle_connect(boxed, false, None).await.unwrap();
            assert!(matches!(
                request.target.host,
                TargetHost::Ip(std::net::IpAddr::V6(_))
            ));

            let target_stream = tokio::net::TcpStream::connect((addr.ip(), addr.port()))
                .await
                .unwrap();
            let (mut cr, mut cw) = tokio::io::split(stream);
            let (mut tr, mut tw) = tokio::io::split(target_stream);
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut cr, &mut tw).await;
            });
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut tr, &mut cw).await;
            });
        });

        let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("::1".parse().unwrap()),
            port: addr.port(),
        };
        let mut conn = connect::http_connect(boxed, &target, None, &Default::default())
            .await
            .unwrap();

        conn.write_all(b"ipv6 test").await.unwrap();
        conn.shutdown().await.unwrap();

        let mut buf = [0u8; 9];
        conn.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ipv6 test");

        server_jh.await.unwrap();
        jh.abort();
    }

    #[tokio::test]
    async fn test_connect_invalid_method_rejection() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = connect::handle_connect(boxed, false, None).await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        // Send a GET request instead of CONNECT
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_connect_header_size_limit() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = connect::handle_connect(boxed, false, None).await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        // Send a CONNECT with very long headers
        let mut request = b"CONNECT example.com:443 HTTP/1.1\r\n".to_vec();
        for _ in 0..1000 {
            request.extend_from_slice(b"X-Long-Header: ");
            request.extend(&[b'A'; 100]);
            request.extend_from_slice(b"\r\n");
        }
        request.extend_from_slice(b"\r\n");
        stream.write_all(&request).await.unwrap();

        server_jh.await.unwrap();
    }

    // ===== Integration tests for HTTP Forwarding =====

    #[tokio::test]
    async fn test_forward_get_absolute_form() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (request, _stream) = forward::forward_request(boxed).await.unwrap();
            assert_eq!(request.method, "GET");
            assert_eq!(request.path, "/index.html");
            assert_eq!(
                request.target,
                TargetAddr {
                    host: TargetHost::Domain("example.com".to_string()),
                    port: 80,
                }
            );
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream
            .write_all(b"GET http://example.com/index.html HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_forward_removes_proxy_auth() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (request, _stream) = forward::forward_request(boxed).await.unwrap();
            // Proxy-Authorization should be removed
            assert!(!request
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("Proxy-Authorization")));
            // Other headers should remain
            assert!(request
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("Authorization")));
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream
            .write_all(
                b"GET http://example.com/ HTTP/1.1\r\n\
                  Host: example.com\r\n\
                  Authorization: Bearer token123\r\n\
                  Proxy-Authorization: Basic dXNlcjpwYXNz\r\n\
                  \r\n",
            )
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_forward_origin_form_conversion() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (request, _stream) = forward::forward_request(boxed).await.unwrap();
            // The path should be origin-form (just the path, not the full URI)
            assert_eq!(request.path, "/api/data");
            assert_eq!(
                request.target.host,
                TargetHost::Domain("api.example.com".to_string())
            );
            assert_eq!(request.target.port, 8080);
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream
            .write_all(
                b"POST http://api.example.com:8080/api/data HTTP/1.1\r\n\
                  Host: api.example.com:8080\r\n\
                  Content-Length: 11\r\n\
                  \r\n\
                  hello world",
            )
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_forward_post_with_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (request, _stream) = forward::forward_request(boxed).await.unwrap();
            assert_eq!(request.method, "POST");
            assert!(request.has_body);
            assert_eq!(request.content_length, Some(11));
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        stream
            .write_all(
                b"POST http://example.com/api HTTP/1.1\r\n\
                  Host: example.com\r\n\
                  Content-Length: 11\r\n\
                  \r\n\
                  hello world",
            )
            .await
            .unwrap();

        server_jh.await.unwrap();
    }
}
