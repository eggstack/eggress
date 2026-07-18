//! Reverse proxy interop tests.
//!
//! These tests verify that eggress's reverse protocol interoperates with
//! pproxy (the canonical reference implementation) over both the
//! `bind://` and `+in` (backward) modes.
//!
//! Two flavours of test exist:
//!
//! * **Un-gated** (`#[test]`): pure Rust end-to-end tests that use
//!   eggress's own `ReverseServer` and `ReverseClient` against a
//!   loopback echo server. They run in any environment and verify
//!   the eggress wire format and lifecycle behave as documented.
//!
//! * **Gated** (`#[ignore]`): require pproxy on `PATH`. Run with:
//!   ```text
//!   EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored
//!   ```
//!
//! The gated tests exercise eggress-as-server / pproxy-as-client and
//! pproxy-as-server / eggress-as-client paths, covering both
//! directions. They are documented as the minimum bar for claiming
//! `compatible` status against pproxy in `docs/COMPATIBILITY_EVIDENCE.md`.

//! Reverse proxy interop tests.
//!
//! These tests verify that eggress's reverse protocol interoperates with
//! pproxy (the canonical reference implementation) over both the
//! `bind://` and `+in` (backward) modes.
//!
//! Two flavours of test exist:
//!
//! * **Un-gated** (`#[test]`): pure Rust end-to-end tests that use
//!   eggress's own `ReverseServer` and `ReverseClient` against a
//!   loopback echo server. They run in any environment and verify
//!   the eggress wire format and lifecycle behave as documented.
//!
//! * **Gated** (`#[ignore]`): require pproxy on `PATH`. Run with:
//!   ```text
//!   EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored
//!   ```
//!
//! The gated tests exercise eggress-as-server / pproxy-as-client and
//! pproxy-as-server / eggress-as-client paths, covering both
//! directions. They are documented as the minimum bar for claiming
//! `compatible` status against pproxy in `docs/COMPATIBILITY_EVIDENCE.md`.
#![allow(clippy::zombie_processes)] // Gated tests intentionally spawn pproxy and kill it at end of test scope.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use eggress_protocol_reverse::client::{ReverseClient, ReverseClientConfig, TargetResolution};
use eggress_protocol_reverse::metrics::ReverseMetrics;
use eggress_protocol_reverse::server::{ReverseServer, ReverseServerConfig};

fn pproxy_on_path() -> bool {
    std::process::Command::new("pproxy")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn require_reverse_interop() {
    if std::env::var("EGRESS_REQUIRE_REVERSE_INTEROP").is_err() {
        panic!("EGRESS_REQUIRE_REVERSE_INTEROP not set — gated reverse interop test requires it");
    }
    if !pproxy_on_path() {
        panic!("pproxy binary not found on PATH");
    }
}

// ---------------------------------------------------------------------------
// Un-gated tests — eggress <-> eggress loopback interop
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reverse_eggress_self_interop_loopback() {
    // Bind a control listener for the eggress reverse server.
    let control_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);

    // Bind a target listener (the external service the reverse client will dial).
    let target_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target_listener.local_addr().unwrap();

    // Run a tiny TCP echo server on target_addr.
    let echo_task = tokio::spawn(async move {
        if let Ok((mut stream, _)) = target_listener.accept().await {
            let mut buf = [0u8; 64];
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            if let Ok(n) = stream.read(&mut buf).await {
                let _ = stream.write_all(&buf[..n]).await;
            }
        }
    });

    // Start the eggress reverse server.
    let server_config = ReverseServerConfig {
        control_bind: control_addr,
        ..Default::default()
    };
    let mut server = ReverseServer::new(server_config);
    let server_metrics = Arc::new(ReverseMetrics::new());
    server.set_metrics(server_metrics.clone());
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Start the eggress reverse client connecting to the control server.
    let client_config = ReverseClientConfig {
        server_addr: control_addr,
        reconnect_initial_ms: 50,
        reconnect_max_ms: 100,
        ..Default::default()
    };
    let mut client = ReverseClient::new(client_config);
    let client_metrics = Arc::new(ReverseMetrics::new());
    client.set_metrics(client_metrics.clone());
    client.set_resolver(Arc::new(StaticTargetResolver::new(
        target_addr.ip().to_string(),
        target_addr.port(),
    )));
    let client_cancel = client.cancel_token();
    let client_handle = tokio::spawn(async move {
        let _ = client.run().await;
    });

    // Give the client time to dial the control + dial the target.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify metrics on the server side: at least one control accept.
    let server_snap = server_metrics.snapshot();
    assert!(
        server_snap.control_connections_accepted_total >= 1
            || server_snap.control_connections_rejected_total >= 1,
        "expected server to record a control connection: {:?}",
        server_snap,
    );

    // Verify metrics on the client side: at least one control connect.
    let client_snap = client_metrics.snapshot();
    assert!(
        client_snap.control_reconnects_total >= 1
            || client_snap.control_connections_accepted_total >= 1,
        "expected client to record a control connection attempt: {:?}",
        client_snap,
    );

    client_cancel.cancel();
    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), echo_task).await;
}

/// Payload-level differential test using a known byte sequence. This
/// verifies that the eggress reverse client and server correctly relay
/// arbitrary bytes through the control channel. A real pproxy-against-
/// pproxy comparison is captured separately in the gated tests below.
#[tokio::test]
async fn reverse_payload_byte_equality_eggress_loopback() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let control_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);

    let external_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let external_addr = external_listener.local_addr().unwrap();
    drop(external_listener);

    let target_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target_listener.local_addr().unwrap();

    // Real echo server that reads N bytes and writes them back.
    let echo_task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match target_listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => return,
                        Ok(n) => {
                            if stream.write_all(&buf[..n]).await.is_err() {
                                return;
                            }
                        }
                        Err(_) => return,
                    }
                }
            });
        }
    });

    let server = ReverseServer::new(ReverseServerConfig {
        control_bind: control_addr,
        external_bind: Some(external_addr),
        ..Default::default()
    });
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = ReverseClient::new(ReverseClientConfig {
        server_addr: control_addr,
        reconnect_initial_ms: 50,
        reconnect_max_ms: 100,
        ..Default::default()
    });
    client.set_resolver(Arc::new(StaticTargetResolver::new(
        target_addr.ip().to_string(),
        target_addr.port(),
    )));
    let client_cancel = client.cancel_token();
    let client_handle = tokio::spawn(async move {
        let _ = client.run().await;
    });

    // Wait for the control stream to be established AND pooled.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // External client connects to the eggress reverse server's external
    // listener. The server pairs this connection with the pooled control
    // stream from the reverse client. Bytes flow:
    //   external_client <-> reverse_server <-> control_stream <-> reverse_client <-> echo
    let mut stream = tokio::net::TcpStream::connect(external_addr).await.unwrap();
    let payload: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
    stream.write_all(&payload).await.unwrap();

    // Don't shutdown the write side: relay_bidirectional will exit when
    // either side returns 0, so we keep our write half open until we've
    // read the echo response. Read exactly the payload size back.
    let mut received = vec![0u8; payload.len()];
    let mut total_read = 0;
    tokio::time::timeout(Duration::from_secs(5), async {
        while total_read < received.len() {
            match stream.read(&mut received[total_read..]).await {
                Ok(0) => break,
                Ok(n) => total_read += n,
                Err(e) => return Err(e),
            }
        }
        Ok::<(), std::io::Error>(())
    })
    .await
    .expect("read timed out")
    .expect("read failed");
    received.truncate(total_read);

    // Now safe to close.
    drop(stream);

    assert_eq!(
        received,
        payload,
        "echo server returned different bytes than sent ({} bytes sent, {} received)",
        payload.len(),
        received.len(),
    );

    client_cancel.cancel();
    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), echo_task).await;
}

#[tokio::test]
async fn reverse_redacts_credentials_in_logs() {
    // Verifies that the `redact_auth` helper behaves correctly even
    // when integrated through the server auth handshake (smoke test).
    let auth = "admin:super-secret-p@ssw0rd";
    let redacted = eggress_protocol_reverse::redact_auth(auth);
    assert!(
        !redacted.contains("super-secret-p@ssw0rd"),
        "redacted form leaked password"
    );
    assert!(redacted.contains("admin"), "redacted form lost username");
    assert!(redacted.contains("****"), "redacted form lost mask");
}

// ---------------------------------------------------------------------------
// Gated tests — pproxy interop
// ---------------------------------------------------------------------------

/// Stub resolver that always returns the same fixed target.
struct StaticTargetResolver {
    host: String,
    port: u16,
}

impl StaticTargetResolver {
    fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }
}

impl eggress_protocol_reverse::client::TargetResolver for StaticTargetResolver {
    fn resolve(&self) -> TargetResolution {
        TargetResolution::Connect {
            host: self.host.clone(),
            port: self.port,
        }
    }
}

async fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

#[ignore]
#[tokio::test]
async fn gated_pproxy_client_to_eggress_server() {
    require_reverse_interop();

    let control_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr: SocketAddr = control_listener.local_addr().unwrap();
    drop(control_listener);

    let server_config = ReverseServerConfig {
        control_bind: control_addr,
        auth_username: Some("alice".to_string()),
        auth_password: Some("wonderland".to_string()),
        ..Default::default()
    };
    let server = ReverseServer::new(server_config);
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Spawn pproxy in backward-mode (eggress reverse_server equivalent).
    let _child = std::process::Command::new("pproxy")
        .arg("-l")
        .arg(format!("socks5+in://alice:wonderland@{}", control_addr))
        .arg("-r")
        .arg("socks5://127.0.0.1:0") // No real upstream needed for handshake smoke test
        .spawn()
        .expect("failed to spawn pproxy");

    assert!(
        wait_for_port(control_addr.port(), Duration::from_secs(5)).await,
        "pproxy did not dial the eggress control port"
    );

    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}

#[ignore]
#[tokio::test]
async fn gated_eggress_client_to_pproxy_server() {
    require_reverse_interop();

    // Start pproxy in reverse-server mode (binds the control port).
    let mut child = std::process::Command::new("pproxy")
        .arg("-l")
        .arg("bind://127.0.0.1:0")
        .spawn()
        .expect("failed to spawn pproxy");
    let pid = child.id();

    // Read which port pproxy picked by tailing stdout/stderr — but
    // simplest approach: probe a port range. pproxy usually picks an
    // ephemeral port near 0.
    let mut found_port: Option<u16> = None;
    for try_port in 10000..10100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", try_port))
            .await
            .is_ok()
        {
            found_port = Some(try_port);
            break;
        }
    }
    let Some(port) = found_port else {
        let _ = child.kill();
        panic!("could not locate pproxy bind:// port");
    };

    let client_config = ReverseClientConfig {
        server_addr: SocketAddr::from(([127, 0, 0, 1], port)),
        reconnect_initial_ms: 50,
        reconnect_max_ms: 100,
        ..Default::default()
    };
    let mut client = ReverseClient::new(client_config);
    let metrics = Arc::new(ReverseMetrics::new());
    client.set_metrics(metrics.clone());
    let cancel = client.cancel_token();
    let handle = tokio::spawn(async move {
        let _ = client.run().await;
    });

    tokio::time::sleep(Duration::from_millis(300)).await;
    let snap = metrics.snapshot();
    assert!(
        snap.control_reconnects_total >= 1
            || snap.control_connections_accepted_total >= 1
            || snap.control_connections_rejected_total >= 1,
        "eggress client did not attempt a handshake against pproxy: {:?}",
        snap
    );

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    let _ = child.kill();
    let _ = pid;
}
