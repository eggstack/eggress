//! Reverse proxy soak tests.
//!
//! These tests exercise the reverse proxy under sustained load to detect
//! resource leaks, stuck tasks, and cleanup issues. They are gated behind
//! `EGRESS_REQUIRE_SOAK=1` and intended to run with `--test-threads=1`.
//!
//! ```text
//! EGRESS_REQUIRE_SOAK=1 cargo test -p eggress-runtime --test reverse_soak -- --ignored --test-threads=1
//! ```

use std::sync::Arc;
use std::time::Duration;

use eggress_protocol_reverse::client::{ReverseClient, ReverseClientConfig, TargetResolution};
use eggress_protocol_reverse::server::{ReverseServer, ReverseServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn require_soak() {
    if std::env::var("EGRESS_REQUIRE_SOAK").unwrap_or_default() != "1" {
        panic!("EGRESS_REQUIRE_SOAK not set — gated soak test requires it");
    }
}

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

async fn start_echo_server() -> (tokio::task::JoinHandle<()>, std::net::SocketAddr) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
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
    (handle, addr)
}

/// Spawn a reverse server + client pair. Returns (server_handle, client_handle,
/// echo_handle, external_addr, server_cancel, client_cancel).
async fn start_reverse_pair(
    auth_password: Option<&str>,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
    std::net::SocketAddr,
    tokio_util::sync::CancellationToken,
    tokio_util::sync::CancellationToken,
) {
    let control_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);

    let external_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let external_addr = external_listener.local_addr().unwrap();
    drop(external_listener);

    let (echo_handle, echo_addr) = start_echo_server().await;

    let server_config = ReverseServerConfig {
        control_bind: control_addr,
        external_bind: Some(external_addr),
        auth_password: auth_password.map(|s| s.to_string()),
        auth_username: auth_password.map(|_| "user".to_string()),
        ..Default::default()
    };
    let server = ReverseServer::new(server_config);
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let client_config = ReverseClientConfig {
        server_addr: control_addr,
        auth_password: auth_password.map(|s| s.to_string()),
        auth_username: auth_password.map(|_| "user".to_string()),
        reconnect_initial_ms: 50,
        reconnect_max_ms: 100,
        ..Default::default()
    };
    let mut client = ReverseClient::new(client_config);
    client.set_resolver(Arc::new(StaticTargetResolver::new(
        echo_addr.ip().to_string(),
        echo_addr.port(),
    )));
    let client_cancel = client.cancel_token();
    let client_handle = tokio::spawn(async move {
        let _ = client.run().await;
    });

    tokio::time::sleep(Duration::from_millis(300)).await;

    (
        server_handle,
        client_handle,
        echo_handle,
        external_addr,
        server_cancel,
        client_cancel,
    )
}

/// Connect to the external port, send `payload`, and read it back. Returns the
/// number of bytes echoed correctly.
async fn echo_roundtrip(
    addr: std::net::SocketAddr,
    payload: &[u8],
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = tokio::net::TcpStream::connect(addr).await?;
    stream.write_all(payload).await?;
    let mut received = vec![0u8; payload.len()];
    let mut total = 0;
    tokio::time::timeout(Duration::from_secs(5), async {
        while total < received.len() {
            match stream.read(&mut received[total..]).await {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(_) => break,
            }
        }
    })
    .await
    .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
        "echo roundtrip timed out".into()
    })?;
    received.truncate(total);
    if received == payload {
        Ok(payload.len())
    } else {
        Ok(0)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn performance_reverse_soak() {
    require_soak();

    let (server_handle, client_handle, echo_handle, external_addr, server_cancel, client_cancel) =
        start_reverse_pair(Some("testpassword")).await;

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut connections: u64 = 0;
    let mut errors: u64 = 0;

    while std::time::Instant::now() < deadline {
        let payload: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
        match echo_roundtrip(external_addr, &payload).await {
            Ok(n) if n == payload.len() => connections += 1,
            _ => errors += 1,
        }
    }

    assert!(
        connections > 0,
        "should complete at least one connection (got {connections} ok, {errors} errors)"
    );

    client_cancel.cancel();
    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(3), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), echo_handle).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn performance_reverse_reconnect_churn() {
    require_soak();

    let (server_handle, client_handle, echo_handle, external_addr, server_cancel, client_cancel) =
        start_reverse_pair(Some("testpassword")).await;

    let mut successes: u32 = 0;
    let mut errors: u32 = 0;

    for i in 0..20 {
        let payload: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
        match echo_roundtrip(external_addr, &payload).await {
            Ok(n) if n == payload.len() => successes += 1,
            _ => {
                errors += 1;
                eprintln!("reconnect_churn iteration {i} failed");
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(
        successes, 20,
        "all 20 reconnect iterations should succeed (got {successes} ok, {errors} errors)"
    );

    client_cancel.cancel();
    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(3), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), echo_handle).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn performance_reverse_auth_failure_churn() {
    require_soak();

    let control_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);

    let server_config = ReverseServerConfig {
        control_bind: control_addr,
        auth_password: Some("correct_password".to_string()),
        auth_username: Some("user".to_string()),
        ..Default::default()
    };
    let server = ReverseServer::new(server_config);
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut auth_failures: u32 = 0;
    let mut other_errors: u32 = 0;

    for i in 0..10 {
        match tokio::net::TcpStream::connect(control_addr).await {
            Ok(mut stream) => {
                let result = eggress_protocol_reverse::client_auth_handshake(
                    &mut stream,
                    "user",
                    "wrong_password",
                )
                .await;
                match result {
                    Err(_) => auth_failures += 1,
                    Ok(_) => {
                        other_errors += 1;
                        eprintln!("auth_failure_churn iteration {i}: expected failure but got Ok");
                    }
                }
            }
            Err(_) => {
                other_errors += 1;
                eprintln!("auth_failure_churn iteration {i}: TCP connect failed");
            }
        }
    }

    assert_eq!(
        auth_failures, 10,
        "all 10 auth attempts should fail (got {auth_failures} auth failures, {other_errors} other errors)"
    );
    assert_eq!(
        other_errors, 0,
        "should have no other errors (got {other_errors})"
    );

    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
}
