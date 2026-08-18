//! Reverse proxy interop tests against the pproxy 2.7.9 oracle.
//!
//! Two flavours of test exist:
//!
//! * **Un-gated** (`#[test]`): pure Rust end-to-end tests that exercise
//!   the eggress native `ReverseServer`/`ReverseClient` and the bounded
//!   `PproxyBackwardClient`/`PproxyBackwardServer` adapters against a
//!   loopback echo fixture. They run in any environment and verify the
//!   wire format, reconnect logic, framing mode, and lifecycle behave
//!   as documented.
//!
//! * **Gated** (`#[ignore]`): require the canonical pproxy 2.7.9 oracle
//!   interpreter at `$EGRESS_PPROXY_PYTHON` (or `$EGRESS_ORACLE_PYTHON`),
//!   and the `EGRESS_REQUIRE_REVERSE_INTEROP=1` gate. Run with:
//!   ```text
//!   EGRESS_PPROXY_PYTHON=/path/to/pproxy-2.7.9-oracle/bin/python \
//!     EGRESS_REQUIRE_REVERSE_INTEROP=1 \
//!     cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1
//!   ```
//!
//! The gated tests exercise real payload-level interop against the
//! pinned pproxy interpreter in both directions: pproxy-as-worker
//! dialing an Eggress pproxy-compatibility listener and
//! Eggress-as-worker dialing a pproxy backward listener. Each test
//! captures the pproxy child via a RAII guard, allocates ports from the
//! shared testkit helper, and verifies byte-for-byte equality through
//! a local echo target. No port-range scanning, no path lookups; each
//! pproxy child is bound to the explicit port the test chose.
//!
//! The capability manifest points at this file as the source of
//! `strict_phase = "5"` reverse/backward evidence for the
//! `pproxy_capability_manifest.toml` record set.

#![allow(clippy::zombie_processes)] // Gated tests intentionally spawn pproxy and kill it via drop.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use eggress_protocol_reverse::client::{ReverseClient, ReverseClientConfig, TargetResolution};
use eggress_protocol_reverse::compat_pproxy::{
    PproxyBackwardClient, PproxyBackwardClientConfig, PproxyBackwardFraming, PproxyBackwardServer,
    PproxyBackwardServerConfig,
};
use eggress_protocol_reverse::metrics::ReverseMetrics;
use eggress_protocol_reverse::server::{ReverseServer, ReverseServerConfig};
use eggress_testkit::differential::{
    assert_port_ready, find_oracle_python, ProcessGuard, PINNED_PPROXY_VERSION,
};
use eggress_testkit::fixtures::TcpEchoServer;
use eggress_uri::ProxyChainSpec;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn require_reverse_interop() -> String {
    if std::env::var("EGRESS_REQUIRE_REVERSE_INTEROP").is_err() {
        panic!(
            "EGRESS_REQUIRE_REVERSE_INTEROP not set; gated reverse interop test requires it.\n\
             Run with: EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test ... -- --ignored"
        );
    }
    let python = find_oracle_python(true);
    // find_oracle_python(true) already panics on resolution failure, so we
    // can safely use the returned path here.
    if std::process::Command::new(&python)
        .args(["-c", "import pproxy; print(pproxy.__file__)"])
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        panic!(
            "no pproxy=={} oracle interpreter available; set {} or {}",
            PINNED_PPROXY_VERSION,
            eggress_testkit::differential::ORACLE_PYTHON_VAR,
            eggress_testkit::differential::LEGACY_PYTHON_VAR,
        );
    }
    python
}

// ===========================================================================
// Un-gated tests — eggress reverse wire format and lifecycle
// ===========================================================================

#[tokio::test]
async fn reverse_eggress_self_interop_loopback() {
    let control_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);

    let target_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target_listener.local_addr().unwrap();

    let echo_task = tokio::spawn(async move {
        if let Ok((mut stream, _)) = target_listener.accept().await {
            let mut buf = [0u8; 64];
            if let Ok(n) = stream.read(&mut buf).await {
                let _ = stream.write_all(&buf[..n]).await;
            }
        }
    });

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

    tokio::time::sleep(Duration::from_secs(2)).await;

    let server_snap = server_metrics.snapshot();
    assert!(
        server_snap.control_connections_accepted_total >= 1
            || server_snap.control_connections_rejected_total >= 1,
        "expected server to record a control connection: {:?}",
        server_snap,
    );

    let client_snap = client_metrics.snapshot();
    assert!(
        client_snap.control_reconnects_total >= 1 || client_snap.streams_opened_total >= 1,
        "expected client to record a control connection attempt: {:?}",
        client_snap
    );

    client_cancel.cancel();
    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), echo_task).await;
}

#[tokio::test]
async fn reverse_payload_byte_equality_eggress_loopback() {
    let control_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);

    let external_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let external_addr = external_listener.local_addr().unwrap();
    drop(external_listener);

    let target_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target_listener.local_addr().unwrap();

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

    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut stream = tokio::net::TcpStream::connect(external_addr).await.unwrap();
    let payload: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
    stream.write_all(&payload).await.unwrap();

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
    let auth = "admin:super-secret-p@ssw0rd";
    let redacted = eggress_protocol_reverse::redact_auth(auth);
    assert!(!redacted.contains("super-secret-p@ssw0rd"));
    assert!(redacted.contains("admin"));
    assert!(redacted.contains("****"));
}

#[tokio::test]
async fn reverse_compat_pproxy_raw_byte_pipe_relays_byte_for_byte() {
    let echo = TcpEchoServer::start().await;
    let control_addr = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap();
    let external_addr = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap();
    let server = PproxyBackwardServer::new(PproxyBackwardServerConfig {
        control_bind: control_addr,
        external_bind: external_addr,
        auth: b"alice:wonderland".to_vec(),
        read_timeout_ms: 2_000,
        client_framing: PproxyBackwardFraming::Raw,
        ..Default::default()
    });
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(server.run());

    let client = PproxyBackwardClient::new(
        PproxyBackwardClientConfig {
            server_addr: control_addr,
            auth: b"alice:wonderland".to_vec(),
            reconnect_initial_ms: 1,
            reconnect_max_ms: 5,
            server_framing: PproxyBackwardFraming::Raw,
            ..Default::default()
        },
        Arc::new(FixedAddrResolver::new(echo.addr())),
    );
    let client_cancel = client.cancel_token();
    let client_handle = tokio::spawn(async move { client.run().await });

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut external = loop {
        match tokio::net::TcpStream::connect(external_addr).await {
            Ok(stream) => break stream,
            Err(error) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(5)).await;
                let _ = error;
            }
            Err(error) => panic!("backward external listener did not start: {error}"),
        }
    };

    let payload: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
    external.write_all(&payload).await.unwrap();

    let mut received = Vec::with_capacity(payload.len());
    let read_fut = async {
        let mut chunk = vec![0u8; payload.len()];
        external.read_exact(&mut chunk).await?;
        Ok::<_, std::io::Error>(chunk)
    };
    let chunk = tokio::time::timeout(Duration::from_secs(3), read_fut)
        .await
        .expect("read timed out")
        .expect("read failed");
    received.extend_from_slice(&chunk);

    assert_eq!(received, payload, "echo server returned different bytes than sent");

    client_cancel.cancel();
    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn reverse_compat_pproxy_socks5_server_mode_negotiates_and_relays() {
    // Verifies the new `Socks5` worker framing matches a SOCKS5 server
    // pattern end-to-end against the eggress compatibility server, without
    // needing the pproxy oracle. The worker runs SOCKS5 server mode and the
    // server speaks SOCKS5 client framing toward it, then the server pairs
    // the worker's channel with a fresh external client.
    let local_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = local_listener.local_addr().unwrap();
    let echo_task = tokio::spawn(async move {
        if let Ok((mut s, _)) = local_listener.accept().await {
            let mut buf = vec![0u8; 4096];
            loop {
                match s.read(&mut buf).await {
                    Ok(0) => return,
                    Ok(n) => {
                        if s.write_all(&buf[..n]).await.is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        }
    });

    let control_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);
    let external_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let external_addr = external_listener.local_addr().unwrap();
    drop(external_listener);

    let server = PproxyBackwardServer::new(PproxyBackwardServerConfig {
        control_bind: control_addr,
        external_bind: external_addr,
        auth: Vec::new(),
        read_timeout_ms: 2_000,
        client_framing: PproxyBackwardFraming::Socks5,
        socks5_target: Some((local_addr.ip().to_string(), local_addr.port())),
        ..Default::default()
    });
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(server.run());

    let worker = PproxyBackwardClient::new(
        PproxyBackwardClientConfig {
            server_addr: control_addr,
            server_chain: None,
            auth: b"".to_vec(),
            reconnect_initial_ms: 1,
            reconnect_max_ms: 5,
            server_framing: PproxyBackwardFraming::Socks5,
            ..Default::default()
        },
        Arc::new(NoopResolver),
    );
    let worker_cancel = worker.cancel_token();
    let worker_handle = tokio::spawn(async move { worker.run().await });

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut external = loop {
        match tokio::net::TcpStream::connect(external_addr).await {
            Ok(stream) => break stream,
            Err(error) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(5)).await;
                let _ = error;
            }
            Err(error) => panic!("backward external listener did not start: {error}"),
        }
    };

    let payload: Vec<u8> = (0..=255u8).cycle().take(512).collect();
    external.write_all(&payload).await.unwrap();

    let mut received = Vec::with_capacity(payload.len());
    let read_fut = async {
        let mut chunk = vec![0u8; payload.len()];
        external.read_exact(&mut chunk).await?;
        Ok::<_, std::io::Error>(chunk)
    };
    let chunk = tokio::time::timeout(Duration::from_secs(3), read_fut)
        .await
        .expect("read timed out")
        .expect("read failed");
    received.extend_from_slice(&chunk);

    assert_eq!(received, payload, "SOCKS5 worker did not relay payload byte-for-byte");

    worker_cancel.cancel();
    server_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), worker_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), echo_task).await;
}

// --- Static resolvers used by the un-gated tests ---

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

struct FixedAddrResolver(SocketAddr);

impl FixedAddrResolver {
    fn new(addr: SocketAddr) -> Self {
        Self(addr)
    }
}

impl eggress_protocol_reverse::client::TargetResolver for FixedAddrResolver {
    fn resolve(&self) -> TargetResolution {
        TargetResolution::Connect {
            host: self.0.ip().to_string(),
            port: self.0.port(),
        }
    }
}

struct NoopResolver;

impl eggress_protocol_reverse::client::TargetResolver for NoopResolver {
    fn resolve(&self) -> TargetResolution {
        TargetResolution::Reject {
            reason: "socks5 mode derives target from CONNECT".into(),
        }
    }
}

// ===========================================================================
// Gated tests — real pproxy 2.7.9 oracle interop
// ===========================================================================
//
// Each test below:
//   * Resolves the canonical pproxy 2.7.9 oracle interpreter with
//     `find_oracle_python(true)`. This panics when the gate is enabled
//     but no validated interpreter is available, instead of silently
//     falling back to `python3`.
//   * Allocates explicit loopback ports through `eggress_testkit::get_free_port`.
//   * Spawns the oracle via `$python -m pproxy ...` with the chosen
//     ports. The child is captured in a `ProcessGuard` so a test
//     failure or normal completion always releases it deterministically.
//   * Verifies byte-for-byte equality through a local TCP echo fixture.
//   * Keeps cancellation tokens live for the Eggress worker and the
//     listener so no detached reconnect task leaks past test end.

/// Scenario A: pproxy backward worker -> Eggress pproxy-compatibility
/// listening endpoint, payload-level interop.
///
/// pproxy runs `-l socks5+in://control#auth -r direct` to dial into the
/// Eggress control port and act as the SOCKS5 server on the channel.
/// Eggress runs `PproxyBackwardServer` with `Raw` framing so the external
/// client (raw TCP) can speak SOCKS5 directly to the pproxy worker via
/// the paired channel.
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gated_pproxy_backward_worker_to_eggress_listener_payload() {
    let python = require_reverse_interop();
    let echo = TcpEchoServer::start().await;
    let control_port = eggress_testkit::get_free_port().await;
    let control_addr: SocketAddr = format!("127.0.0.1:{control_port}").parse().unwrap();
    let external_port = eggress_testkit::get_free_port().await;
    let external_addr: SocketAddr = format!("127.0.0.1:{external_port}").parse().unwrap();
    let keepalive_port = eggress_testkit::get_free_port().await;

    let auth = "alice:wonderland";

    let server = PproxyBackwardServer::new(PproxyBackwardServerConfig {
        control_bind: control_addr,
        external_bind: external_addr,
        auth: auth.as_bytes().to_vec(),
        read_timeout_ms: 6_000,
        client_framing: PproxyBackwardFraming::Raw,
        ..Default::default()
    });
    let server_cancel = server.cancel_token();
    let server_handle = tokio::spawn(server.run());

    let mut pproxy_guard = spawn_pproxy_worker(&python, control_port, auth, keepalive_port);
    assert_port_ready(control_port, Duration::from_secs(5)).await;

    // Let the pproxy worker dial control_addr and send auth.
    tokio::time::sleep(Duration::from_millis(250)).await;

    let payload: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
    let result = drive_external_socks5_to_pproxy_worker(external_addr, echo.addr(), &payload).await;
    if result.is_err() {
        let stderr = pproxy_guard.drain_stderr();
        eprintln!("DBG pproxy stderr: {stderr}");
    }
    result.expect("payload roundtrip failed");

    server_cancel.cancel();
    pproxy_guard.kill();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}

/// Scenario B: Eggress backward worker -> pproxy listener endpoint,
/// payload-level interop.
///
/// pproxy runs `-l http://:external -r socks5+in://control#auth`. The
/// external HTTP client sends `CONNECT echo_addr`, pproxy obtains the
/// target from the request, then performs the SOCKS5 negotiation with
/// the Eggress worker via the channel. Eggress runs
/// `PproxyBackwardClient` with `Socks5` framing so it acts as the
/// SOCKS5 server that pproxy's listener expects after auth.
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gated_eggress_backward_worker_to_pproxy_listener_payload() {
    let python = require_reverse_interop();
    let echo = TcpEchoServer::start().await;
    let control_port = eggress_testkit::get_free_port().await;
    let control_addr: SocketAddr = format!("127.0.0.1:{control_port}").parse().unwrap();
    let external_port = eggress_testkit::get_free_port().await;
    let external_addr: SocketAddr = format!("127.0.0.1:{external_port}").parse().unwrap();

    let auth = "";

    let _pproxy_guard =
        spawn_pproxy_listener(&python, control_port, external_port, auth);
    assert_port_ready(external_port, Duration::from_secs(5)).await;
    let _ = control_addr; // re-used by the spawned Socks5 worker below

    let worker = PproxyBackwardClient::new(
        PproxyBackwardClientConfig {
            server_addr: control_addr,
            server_chain: None,
            auth: Vec::new(),
            reconnect_initial_ms: 50,
            reconnect_max_ms: 200,
            read_timeout_ms: 6_000,
            target_connect_timeout_ms: 4_000,
            server_framing: PproxyBackwardFraming::Socks5,
        },
        Arc::new(NoopResolver),
    );
    let worker_cancel = worker.cancel_token();
    let worker_handle = tokio::spawn(async move { worker.run().await });

    tokio::time::sleep(Duration::from_millis(300)).await;

    let payload: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
    drive_external_http_connect_to_pproxy_listener(external_addr, echo.addr(), &payload)
        .await
        .expect("payload roundtrip failed");

    eprintln!("DBG test B: echo connection_count = {}", echo.connection_count().load(std::sync::atomic::Ordering::Relaxed));
    worker_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), worker_handle).await;
}

/// Scenario C: Eggress backward worker reaches a pproxy listener through
/// one HTTP CONNECT jump. The jump uses a local `HttpConnectUpstream`
/// fixture so no public internet is involved. End-to-end byte equality is
/// verified through the HTTP-jumped channel.
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gated_eggress_backward_worker_pproxy_http_jump_payload() {
    use eggress_testkit::fixtures::HttpConnectUpstream;
    use eggress_uri::{EndpointSpec, ProtocolSpec, ProxyHopSpec};

    let python = require_reverse_interop();
    let echo = TcpEchoServer::start().await;

    let jump = HttpConnectUpstream::start().await;
    let control_port = eggress_testkit::get_free_port().await;
    let control_addr: SocketAddr = format!("127.0.0.1:{control_port}").parse().unwrap();
    let external_port = eggress_testkit::get_free_port().await;
    let external_addr: SocketAddr = format!("127.0.0.1:{external_port}").parse().unwrap();

    let auth = "";
    let _pproxy_guard =
        spawn_pproxy_listener(&python, control_port, external_port, auth);
    assert_port_ready(external_port, Duration::from_secs(5)).await;

    // chain order: [target (control), jump (HTTP CONNECT)] — see
    // PproxyBackwardClient::connect_control
    let chain = ProxyChainSpec {
        hops: vec![
            ProxyHopSpec {
                protocols: vec![ProtocolSpec::Socks5],
                endpoint: EndpointSpec {
                    host: control_addr.ip().to_string(),
                    port: control_addr.port(),
                },
                credentials: None,
                rule: None,
                local_bind: None,
                plugins: Vec::new(),
                auth_prefix: None,
                tls: false,
                server_name: None,
                insecure: false,
            },
            ProxyHopSpec {
                protocols: vec![ProtocolSpec::Http],
                endpoint: EndpointSpec {
                    host: jump.addr().ip().to_string(),
                    port: jump.addr().port(),
                },
                credentials: None,
                rule: None,
                local_bind: None,
                plugins: Vec::new(),
                auth_prefix: None,
                tls: false,
                server_name: None,
                insecure: false,
            },
        ],
    };

    let worker = PproxyBackwardClient::new(
        PproxyBackwardClientConfig {
            server_addr: control_addr,
            server_chain: Some(chain),
            auth: Vec::new(),
            reconnect_initial_ms: 50,
            reconnect_max_ms: 200,
            read_timeout_ms: 6_000,
            target_connect_timeout_ms: 4_000,
            server_framing: PproxyBackwardFraming::Socks5,
        },
        Arc::new(NoopResolver),
    );
    let worker_cancel = worker.cancel_token();
    let worker_handle = tokio::spawn(async move { worker.run().await });

    tokio::time::sleep(Duration::from_millis(400)).await;

    let payload: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
    drive_external_http_connect_to_pproxy_listener(external_addr, echo.addr(), &payload)
        .await
        .expect("payload roundtrip failed");

    worker_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), worker_handle).await;
}

/// Scenario D: forced disconnect on the pproxy listener side, followed by
/// Eggress worker reconnect + second payload roundtrip.
///
/// The pproxy listener is started, relays a first payload, then is killed.
/// Within the bounded retry window the Eggress worker reconnects, the
/// listener is restarted on the same ports, and a second payload
/// roundtrip succeeds. Cancellation tokens are kept alive across the
/// relaunch so no detached reconnect task leaks.
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gated_eggress_backward_worker_pproxy_disconnect_reconnect() {
    let python = require_reverse_interop();
    let echo = TcpEchoServer::start().await;

    let control_port = eggress_testkit::get_free_port().await;
    let control_addr: SocketAddr = format!("127.0.0.1:{control_port}").parse().unwrap();
    let external_port = eggress_testkit::get_free_port().await;
    let external_addr: SocketAddr = format!("127.0.0.1:{external_port}").parse().unwrap();
    let auth = "";

    let mut pproxy_guard =
        spawn_pproxy_listener(&python, control_port, external_port, auth);
    assert_port_ready(external_port, Duration::from_secs(5)).await;

    let worker = PproxyBackwardClient::new(
        PproxyBackwardClientConfig {
            server_addr: control_addr,
            server_chain: None,
            auth: Vec::new(),
            reconnect_initial_ms: 50,
            reconnect_max_ms: 200,
            read_timeout_ms: 6_000,
            target_connect_timeout_ms: 4_000,
            server_framing: PproxyBackwardFraming::Socks5,
        },
        Arc::new(NoopResolver),
    );
    let worker_cancel = worker.cancel_token();
    let worker_handle = tokio::spawn(async move { worker.run().await });

    tokio::time::sleep(Duration::from_millis(300)).await;

    let payload: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
    drive_external_http_connect_to_pproxy_listener(external_addr, echo.addr(), &payload)
        .await
        .expect("first payload roundtrip failed");

    // Forced disconnect: kill the listener process without cancelling the
    // Eggress worker.
    pproxy_guard.kill();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Relaunch the listener on the same explicit ports.
    let _pproxy_guard = spawn_pproxy_listener_explicit(&python, control_port, external_port, auth);
    assert_port_ready(external_port, Duration::from_secs(5)).await;

    let second_payload: Vec<u8> = payload.iter().rev().copied().collect();
    drive_external_http_connect_to_pproxy_listener(external_addr, echo.addr(), &second_payload)
        .await
        .expect("second payload roundtrip after reconnect failed");

    worker_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), worker_handle).await;
}

// ---------------------------------------------------------------------------
// pproxy process helpers
// ---------------------------------------------------------------------------

fn spawn_pproxy_listener(
    python: &str,
    control_port: u16,
    external_port: u16,
    auth: &str,
) -> ProcessGuard {
    let listen = format!("http://127.0.0.1:{external_port}");
    // No `#auth` fragment so pproxy uses the SOCKS5 NO AUTH method
    // (matching the Eggress compatibility `PproxyBackwardClient`
    // `Socks5` framing). With `#user:pass` pproxy would offer
    // USERNAME/PASSWORD (`\x05\x01\x02`) and the Eggress-side
    // server would refuse with `\x05\xff`.
    let auth_segment = if auth.is_empty() {
        String::new()
    } else {
        format!("#{auth}")
    };
    let remote = format!("socks5+in://127.0.0.1:{control_port}{auth_segment}");
    let args = vec!["-l", listen.as_str(), "-r", remote.as_str()];
    let child = std::process::Command::new(python)
        .args(["-m", "pproxy"])
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn pproxy listener");
    ProcessGuard::new(child)
}

fn spawn_pproxy_listener_explicit(
    python: &str,
    control_port: u16,
    external_port: u16,
    auth: &str,
) -> ProcessGuard {
    let listen = format!("http://127.0.0.1:{external_port}");
    let auth_segment = if auth.is_empty() {
        String::new()
    } else {
        format!("#{auth}")
    };
    let remote = format!("socks5+in://127.0.0.1:{control_port}{auth_segment}");
    let args = vec!["-l", listen.as_str(), "-r", remote.as_str()];
    let child = std::process::Command::new(python)
        .args(["-m", "pproxy"])
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn pproxy listener");
    ProcessGuard::new(child)
}

fn spawn_pproxy_worker(
    python: &str,
    control_port: u16,
    auth: &str,
    keepalive_port: u16,
) -> ProcessGuard {
    // pproxy worker side of a backward/reverse proxy. The `-l socks5+in://`
    // URI tells pproxy to dial out to `control_port` and act as a SOCKS5
    // server on that channel. pproxy 2.7.9 has a CLI bug where
    // `ProxyBackward` lacks a `sockets` attribute, causing
    // `print_server_started` to fail and the process to exit because the
    // `servers` list ends up empty. Adding a second `-l` with a real TCP
    // listener keeps pproxy alive long enough to perform the dial-out.
    let keepalive = format!("socks5://127.0.0.1:{keepalive_port}");
    let listen = format!("socks5+in://127.0.0.1:{control_port}#{auth}");
    let args = vec![
        "-l",
        keepalive.as_str(),
        "-l",
        listen.as_str(),
        "-r",
        "direct",
    ];
    let child = std::process::Command::new(python)
        .args(["-m", "pproxy"])
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn pproxy worker");
    ProcessGuard::new(child)
}

// ---------------------------------------------------------------------------
// External client drivers
// ---------------------------------------------------------------------------

/// Drive an external SOCKS5 client through the queued pproxy worker
/// channel. The Eggress server side has `Raw` framing, so the external
/// client's bytes are forwarded directly to the pproxy worker (which
/// implements the SOCKS5 server).
async fn drive_external_socks5_to_pproxy_worker(
    external_addr: SocketAddr,
    echo_addr: SocketAddr,
    payload: &[u8],
) -> Result<(), String> {
    let mut external = tokio::net::TcpStream::connect(external_addr)
        .await
        .map_err(|e| format!("connect external: {e}"))?;
    external
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| format!("write SOCKS5 hello: {e}"))?;
    external
        .flush()
        .await
        .map_err(|e| format!("flush: {e}"))?;
    let mut methods = [0u8; 2];
    external
        .read_exact(&mut methods)
        .await
        .map_err(|e| format!("read methods: {e}"))?;
    if methods != [0x05, 0x00] {
        return Err(format!("unexpected SOCKS5 selection: {methods:?}"));
    }

    let octets = match echo_addr.ip() {
        std::net::IpAddr::V4(v4) => v4.octets().to_vec(),
        std::net::IpAddr::V6(_) => return Err("IPv6 echo not supported".into()),
    };
    let mut connect = vec![0x05, 0x01, 0x00, 0x01];
    connect.extend_from_slice(&octets);
    connect.extend_from_slice(&echo_addr.port().to_be_bytes());
    external
        .write_all(&connect)
        .await
        .map_err(|e| format!("write SOCKS5 CONNECT: {e}"))?;
    external
        .flush()
        .await
        .map_err(|e| format!("flush: {e}"))?;
    let mut reply = [0u8; 10];
    external
        .read_exact(&mut reply)
        .await
        .map_err(|e| format!("read SOCKS5 reply: {e}"))?;
    if reply[0] != 0x05 || reply[1] != 0x00 {
        return Err(format!("SOCKS5 CONNECT reply not success: {reply:?}"));
    }

    external
        .write_all(payload)
        .await
        .map_err(|e| format!("write payload: {e}"))?;
    external
        .shutdown()
        .await
        .map_err(|e| format!("shutdown: {e}"))?;
    let mut echoed = vec![0u8; payload.len()];
    external
        .read_exact(&mut echoed)
        .await
        .map_err(|e| format!("read echo: {e}"))?;
    drop(external);
    if echoed != payload {
        return Err(format!(
            "payload mismatch: sent {} bytes, received {}",
            payload.len(),
            echoed.len()
        ));
    }
    Ok(())
}

/// Drive an external HTTP client through the pproxy HTTP listener:
///   1. Open TCP to the listener.
///   2. Send `CONNECT <echo> HTTP/1.1`.
///   3. Read 200 Connection Established.
///   4. Stream the payload and assert byte equality.
///
/// pproxy performs the SOCKS5 negotiation with the Eggress worker
/// internally as part of forwarding the CONNECT target, so the external
/// client never speaks SOCKS5 over the established tunnel.
async fn drive_external_http_connect_to_pproxy_listener(
    external_addr: SocketAddr,
    echo_addr: SocketAddr,
    payload: &[u8],
) -> Result<(), String> {
    let mut external = tokio::net::TcpStream::connect(external_addr)
        .await
        .map_err(|e| format!("connect pproxy listener: {e}"))?;

    let connect_line = format!(
        "CONNECT {echo_ip}:{echo_port} HTTP/1.1\r\nHost: {echo_ip}:{echo_port}\r\n\r\n",
        echo_ip = echo_addr.ip(),
        echo_port = echo_addr.port(),
    );
    external
        .write_all(connect_line.as_bytes())
        .await
        .map_err(|e| format!("write CONNECT: {e}"))?;
    external
        .flush()
        .await
        .map_err(|e| format!("flush CONNECT: {e}"))?;

    let mut response = Vec::new();
    let mut buf = [0u8; 256];
    while !response.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = external
            .read(&mut buf)
            .await
            .map_err(|e| format!("read HTTP response: {e}"))?;
        if n == 0 {
            return Err(format!(
                "EOF reading HTTP response: {:?}",
                String::from_utf8_lossy(&response)
            ));
        }
        response.extend_from_slice(&buf[..n]);
        if response.len() > 4096 {
            return Err("HTTP response too large".into());
        }
    }
    let response_text = String::from_utf8_lossy(&response);
    if !response_text.starts_with("HTTP/1.1 200") {
        return Err(format!("non-200 HTTP response: {response_text}"));
    }

    external
        .write_all(payload)
        .await
        .map_err(|e| format!("write payload: {e}"))?;
    let mut echoed = vec![0u8; payload.len()];
    external
        .read_exact(&mut echoed)
        .await
        .map_err(|e| format!("read echo: {e}"))?;
    drop(external);
    if echoed != payload {
        return Err(format!(
            "payload mismatch: sent {} bytes, received {}",
            payload.len(),
            echoed.len()
        ));
    }
    Ok(())
}
