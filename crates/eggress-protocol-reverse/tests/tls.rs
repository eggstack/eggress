//! Native reverse TLS/mTLS integration tests.
//!
//! Covers Phase 4 Workstream 2 acceptance: server-authenticated TLS, mutual
//! TLS policy, SNI verification, reconnect after transient failure, and
//! cancellation during handshakes. Plaintext behavior is covered by the
//! existing `integration.rs` suite.

use eggress_protocol_reverse::client::{ReverseClient, ReverseClientConfig};
use eggress_protocol_reverse::server::{ReverseServer, ReverseServerConfig};
use eggress_protocol_reverse::tls::{ReverseClientTlsConfig, ReverseServerTlsConfig};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::sleep;

fn init_crypto() {
    eggress_transport_tls::install_default_crypto_provider();
}

fn cert_for(names: Vec<String>) -> (String, String) {
    let params = rcgen::CertificateParams::new(names).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    (cert.pem(), key.serialize_pem())
}

async fn free_port() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    drop(l);
    addr
}

async fn start_echo_target() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let (mut r, mut w) = s.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    addr
}

async fn wait_for_tcp(addr: SocketAddr) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "listener did not become ready: {addr}"
        );
        sleep(Duration::from_millis(10)).await;
    }
}

async fn echo_via_external(external: SocketAddr, payload: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(mut s) = tokio::net::TcpStream::connect(external).await {
            s.write_all(payload).await.unwrap();
            s.shutdown().await.unwrap();
            let mut out = Vec::new();
            // Read until EOF with a bounded timeout so a missing relay
            // fails fast instead of hanging the test.
            if let Ok(Ok(_)) =
                tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut out)).await
            {
                if !out.is_empty() {
                    return out;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "external relay did not produce echo for {external}"
        );
        sleep(Duration::from_millis(50)).await;
    }
}

fn server_tls(cert: &str, key: &str) -> ReverseServerTlsConfig {
    ReverseServerTlsConfig {
        cert_pem: cert.as_bytes().to_vec(),
        key_pem: key.as_bytes().to_vec(),
        client_ca_pem: None,
        require_client_cert: false,
    }
}

fn client_tls(ca: &str, server_name: &str) -> ReverseClientTlsConfig {
    ReverseClientTlsConfig {
        ca_pem: Some(ca.as_bytes().to_vec()),
        server_name: server_name.to_string(),
        client_cert_pem: None,
        client_key_pem: None,
    }
}

#[tokio::test]
async fn reverse_tls_success_with_trusted_cert() {
    init_crypto();
    let (cert, key) = cert_for(vec!["localhost".to_string()]);
    let echo = start_echo_target().await;
    let control = free_port().await;
    let external = free_port().await;

    let server_cfg = ReverseServerConfig {
        control_bind: control,
        external_bind: Some(external),
        tls: Some(server_tls(&cert, &key)),
        ..Default::default()
    };
    let server = ReverseServer::new(server_cfg);
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move { server.run().await.unwrap() });
    wait_for_tcp(control).await;

    let client_cfg = ReverseClientConfig {
        server_addr: control,
        default_target_host: Some("127.0.0.1".to_string()),
        default_target_port: Some(echo.port()),
        read_timeout_ms: 5_000,
        tls: Some(client_tls(&cert, "localhost")),
        ..Default::default()
    };
    let mut client = ReverseClient::new(client_cfg);
    // Route engine resolver would be injected in production; the default
    // resolver already points at the echo target via default_target_*.
    let _ = &mut client;
    let client_cancel = client.cancel_token();
    let client_handle = tokio::spawn(async move { client.run().await.unwrap() });

    let out = echo_via_external(external, b"tls hello").await;
    assert_eq!(out, b"tls hello");

    client_cancel.cancel();
    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
}

#[tokio::test]
async fn reverse_tls_client_rejects_untrusted_server() {
    init_crypto();
    let (cert, key) = cert_for(vec!["localhost".to_string()]);
    let control = free_port().await;

    let server_cfg = ReverseServerConfig {
        control_bind: control,
        external_bind: None,
        tls: Some(server_tls(&cert, &key)),
        ..Default::default()
    };
    let server = ReverseServer::new(server_cfg);
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move { server.run().await.unwrap() });
    wait_for_tcp(control).await;

    // Client uses system roots only: self-signed server must be rejected.
    // Build the TLS client config directly and attempt one handshake.
    let tls = ReverseClientTlsConfig {
        ca_pem: None,
        server_name: "localhost".to_string(),
        client_cert_pem: None,
        client_key_pem: None,
    };
    let client_cfg = tls.build_client_config().unwrap();
    let tcp = tokio::net::TcpStream::connect(control).await.unwrap();
    let boxed: eggress_core::BoxStream = Box::new(tcp);
    let result = eggress_transport_tls::tls_connect(boxed, client_cfg, "localhost").await;
    assert!(
        result.is_err(),
        "untrusted self-signed server must be rejected"
    );

    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
}

#[tokio::test]
async fn reverse_tls_sni_mismatch_rejected() {
    init_crypto();
    let (cert, key) = cert_for(vec!["localhost".to_string()]);
    let control = free_port().await;

    let server_cfg = ReverseServerConfig {
        control_bind: control,
        external_bind: None,
        tls: Some(server_tls(&cert, &key)),
        ..Default::default()
    };
    let server = ReverseServer::new(server_cfg);
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move { server.run().await.unwrap() });
    wait_for_tcp(control).await;

    // Client trusts the cert but uses the wrong SNI/name.
    let tls = ReverseClientTlsConfig {
        ca_pem: Some(cert.as_bytes().to_vec()),
        server_name: "wrong.example.com".to_string(),
        client_cert_pem: None,
        client_key_pem: None,
    };
    let client_cfg = tls.build_client_config().unwrap();
    let tcp = tokio::net::TcpStream::connect(control).await.unwrap();
    let boxed: eggress_core::BoxStream = Box::new(tcp);
    let result = eggress_transport_tls::tls_connect(boxed, client_cfg, "wrong.example.com").await;
    assert!(result.is_err(), "SNI mismatch must be rejected");

    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
}

#[tokio::test]
async fn reverse_mtls_accepts_trusted_client() {
    init_crypto();
    let (server_cert, server_key) = cert_for(vec!["localhost".to_string()]);
    let (client_cert, client_key) = cert_for(vec!["client".to_string()]);
    let echo = start_echo_target().await;
    let control = free_port().await;
    let external = free_port().await;

    let server_cfg = ReverseServerConfig {
        control_bind: control,
        external_bind: Some(external),
        tls: Some(ReverseServerTlsConfig {
            cert_pem: server_cert.as_bytes().to_vec(),
            key_pem: server_key.as_bytes().to_vec(),
            client_ca_pem: Some(client_cert.as_bytes().to_vec()),
            require_client_cert: true,
        }),
        ..Default::default()
    };
    let server = ReverseServer::new(server_cfg);
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move { server.run().await.unwrap() });
    wait_for_tcp(control).await;

    let client_cfg = ReverseClientConfig {
        server_addr: control,
        default_target_host: Some("127.0.0.1".to_string()),
        default_target_port: Some(echo.port()),
        read_timeout_ms: 5_000,
        tls: Some(ReverseClientTlsConfig {
            ca_pem: Some(server_cert.as_bytes().to_vec()),
            server_name: "localhost".to_string(),
            client_cert_pem: Some(client_cert.as_bytes().to_vec()),
            client_key_pem: Some(client_key.as_bytes().to_vec()),
        }),
        ..Default::default()
    };
    let client = ReverseClient::new(client_cfg);
    let client_cancel = client.cancel_token();
    let client_handle = tokio::spawn(async move { client.run().await.unwrap() });

    let out = echo_via_external(external, b"mtls ok").await;
    assert_eq!(out, b"mtls ok");

    client_cancel.cancel();
    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
}

#[tokio::test]
async fn reverse_mtls_rejects_client_without_cert() {
    init_crypto();
    let (server_cert, server_key) = cert_for(vec!["localhost".to_string()]);
    let (client_ca, _) = cert_for(vec!["client-ca".to_string()]);
    let control = free_port().await;

    let server_cfg = ReverseServerConfig {
        control_bind: control,
        external_bind: None,
        tls: Some(ReverseServerTlsConfig {
            cert_pem: server_cert.as_bytes().to_vec(),
            key_pem: server_key.as_bytes().to_vec(),
            client_ca_pem: Some(client_ca.as_bytes().to_vec()),
            require_client_cert: true,
        }),
        ..Default::default()
    };
    let server = ReverseServer::new(server_cfg);
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move { server.run().await.unwrap() });
    wait_for_tcp(control).await;

    // Client presents no certificate although the server requires one.
    let tls = ReverseClientTlsConfig {
        ca_pem: Some(server_cert.as_bytes().to_vec()),
        server_name: "localhost".to_string(),
        client_cert_pem: None,
        client_key_pem: None,
    };
    let client_cfg = tls.build_client_config().unwrap();
    let tcp = tokio::net::TcpStream::connect(control).await.unwrap();
    let boxed: eggress_core::BoxStream = Box::new(tcp);
    let result = eggress_transport_tls::tls_connect(boxed, client_cfg, "localhost").await;
    // The client-side handshake may succeed (server requests but does not
    // fail the ClientHello); the server side then closes on missing cert.
    // Either a client handshake error or a subsequent auth/read failure
    // proves the policy. If the handshake succeeds, the server must still
    // have rejected: attempt the reverse auth read which should fail.
    if let Ok(mut s) = result {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        // Send auth; server should have already closed the TLS session.
        let _ = s.write_all(b"user:pass\n").await;
        let mut buf = [0u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(3), s.read(&mut buf)).await;
        // EOF (0 bytes) or error both prove rejection; a successful accept
        // byte would mean mTLS was not enforced.
        match read {
            Ok(Ok(0)) => {}
            Ok(Ok(_)) => panic!("mTLS server accepted a client without a certificate"),
            Ok(Err(_)) => {}
            Err(_) => {}
        }
    }

    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
}

#[tokio::test]
async fn reverse_mtls_rejects_untrusted_client_ca() {
    init_crypto();
    let (server_cert, server_key) = cert_for(vec!["localhost".to_string()]);
    let (trusted_client, _) = cert_for(vec!["trusted".to_string()]);
    let (untrusted_client, untrusted_key) = cert_for(vec!["untrusted".to_string()]);
    let _ = trusted_client;
    let control = free_port().await;

    let server_cfg = ReverseServerConfig {
        control_bind: control,
        external_bind: None,
        tls: Some(ReverseServerTlsConfig {
            cert_pem: server_cert.as_bytes().to_vec(),
            key_pem: server_key.as_bytes().to_vec(),
            // Server trusts only `trusted_client` as a CA.
            client_ca_pem: Some(cert_for(vec!["trusted".to_string()]).0.as_bytes().to_vec()),
            require_client_cert: true,
        }),
        ..Default::default()
    };
    let server = ReverseServer::new(server_cfg);
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move { server.run().await.unwrap() });
    wait_for_tcp(control).await;

    let tls = ReverseClientTlsConfig {
        ca_pem: Some(server_cert.as_bytes().to_vec()),
        server_name: "localhost".to_string(),
        client_cert_pem: Some(untrusted_client.as_bytes().to_vec()),
        client_key_pem: Some(untrusted_key.as_bytes().to_vec()),
    };
    let client_cfg = tls.build_client_config().unwrap();
    let tcp = tokio::net::TcpStream::connect(control).await.unwrap();
    let boxed: eggress_core::BoxStream = Box::new(tcp);
    let result = eggress_transport_tls::tls_connect(boxed, client_cfg, "localhost").await;
    // Untrusted client CA must fail either at handshake or at first read.
    if let Ok(mut s) = result {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let _ = s.write_all(b"user:pass\n").await;
        let mut buf = [0u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(3), s.read(&mut buf)).await;
        match read {
            Ok(Ok(0)) => {}
            Ok(Ok(_)) => panic!("mTLS server accepted a client from an untrusted CA"),
            Ok(Err(_)) => {}
            Err(_) => {}
        }
    }

    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
}

#[tokio::test]
async fn reverse_tls_reconnect_after_transient_failure() {
    init_crypto();
    let (cert, key) = cert_for(vec!["localhost".to_string()]);
    let echo = start_echo_target().await;
    let control = free_port().await;
    let external = free_port().await;

    // Client starts before any server is listening: it must keep retrying
    // with backoff rather than exiting.
    let client_cfg = ReverseClientConfig {
        server_addr: control,
        default_target_host: Some("127.0.0.1".to_string()),
        default_target_port: Some(echo.port()),
        reconnect_initial_ms: 50,
        reconnect_max_ms: 200,
        read_timeout_ms: 5_000,
        tls: Some(client_tls(&cert, "localhost")),
        ..Default::default()
    };
    let client = ReverseClient::new(client_cfg);
    let client_cancel = client.cancel_token();
    let client_handle = tokio::spawn(async move { client.run().await.unwrap() });

    // No server yet: external relay cannot succeed. Start the valid server
    // after a short outage and prove the waiting client connects.
    sleep(Duration::from_millis(200)).await;
    let server_cfg = ReverseServerConfig {
        control_bind: control,
        external_bind: Some(external),
        tls: Some(server_tls(&cert, &key)),
        ..Default::default()
    };
    let server = ReverseServer::new(server_cfg);
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move { server.run().await.unwrap() });

    let out = echo_via_external(external, b"reconnect").await;
    assert_eq!(out, b"reconnect");

    client_cancel.cancel();
    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
}

#[tokio::test]
async fn reverse_tls_shutdown_interrupts_handshake() {
    init_crypto();
    // Stall TCP acceptor: accepts control connections but never completes a
    // TLS handshake. Client cancellation must still shut down promptly.
    let stall = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stall_addr = stall.local_addr().unwrap();
    let stall_handle = tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = stall.accept().await else {
                break;
            };
            tokio::spawn(async move {
                // Hold the connection open without speaking TLS.
                let mut buf = [0u8; 1];
                let _ = s.read(&mut buf).await;
            });
        }
    });

    let (_cert, _key) = cert_for(vec!["localhost".to_string()]);
    // Use a self-signed cert the stall will never present; the client will
    // block in the handshake until cancelled.
    let (real_cert, _) = cert_for(vec!["localhost".to_string()]);
    let client_cfg = ReverseClientConfig {
        server_addr: stall_addr,
        default_target_host: Some("127.0.0.1".to_string()),
        default_target_port: Some(9),
        reconnect_initial_ms: 50,
        reconnect_max_ms: 100,
        tls: Some(client_tls(&real_cert, "localhost")),
        ..Default::default()
    };
    let client = ReverseClient::new(client_cfg);
    let cancel = client.cancel_token();
    let handle = tokio::spawn(async move { client.run().await.unwrap() });
    sleep(Duration::from_millis(200)).await;
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(result.is_ok(), "cancellation must interrupt TLS handshake");
    stall_handle.abort();
}

#[test]
fn reverse_tls_debug_redacts_secrets() {
    let server_cfg = ReverseServerConfig {
        auth_username: Some("user".to_string()),
        auth_password: Some("supersecret".to_string()),
        tls: Some(ReverseServerTlsConfig {
            cert_pem: b"cert".to_vec(),
            key_pem: b"private-key-material".to_vec(),
            client_ca_pem: None,
            require_client_cert: false,
        }),
        ..Default::default()
    };
    let rendered = format!("{server_cfg:?}");
    assert!(!rendered.contains("supersecret"));
    assert!(!rendered.contains("private-key-material"));

    let client_cfg = ReverseClientConfig {
        auth_username: Some("user".to_string()),
        auth_password: Some("anothersecret".to_string()),
        tls: Some(ReverseClientTlsConfig {
            ca_pem: None,
            server_name: "localhost".to_string(),
            client_cert_pem: None,
            client_key_pem: Some(b"client-private".to_vec()),
        }),
        ..Default::default()
    };
    let rendered = format!("{client_cfg:?}");
    assert!(!rendered.contains("anothersecret"));
    assert!(!rendered.contains("client-private"));
}
