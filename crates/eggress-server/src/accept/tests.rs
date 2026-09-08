//! Accept-path regression tests (moved verbatim with the split).

use super::*;
use eggress_core::TargetHost;

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
