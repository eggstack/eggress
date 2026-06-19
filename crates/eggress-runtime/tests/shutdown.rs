use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

/// Start a TCP server that accepts connections and holds them open indefinitely.
async fn start_slow_backend() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let jh = tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            // Hold the connection open by sleeping forever
            tokio::spawn(async move {
                let (mut rd, mut wr) = stream.into_split();
                let _ = tokio::io::copy(&mut rd, &mut wr).await;
            });
        }
    });
    (addr, jh)
}

/// Perform a minimal SOCKS5 handshake (no auth) targeting an IPv4 address.
async fn socks5_handshake(
    stream: &mut tokio::net::TcpStream,
    target: std::net::SocketAddr,
) -> std::io::Result<()> {
    // Method negotiation: version=5, 1 method, NO AUTH
    stream.write_all(&[0x05, 0x01, 0x00]).await?;
    // Read response: version=5, selected method
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await?;
    if resp[0] != 0x05 || resp[1] != 0x00 {
        return Err(std::io::Error::other("SOCKS5 method negotiation failed"));
    }
    // CONNECT request: version=5, cmd=CONNECT, rsv=0, atyp=IPv4, addr, port
    let octets = match target.ip() {
        std::net::IpAddr::V4(v4) => v4.octets(),
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "only IPv4 supported",
            ))
        }
    };
    let port = target.port().to_be_bytes();
    stream
        .write_all(&[
            0x05, 0x01, 0x00, 0x01, octets[0], octets[1], octets[2], octets[3],
        ])
        .await?;
    stream.write_all(&port).await?;
    // Read reply header
    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await?;
    if reply[1] != 0x00 {
        return Err(std::io::Error::other(format!(
            "SOCKS5 connect failed: {:#04x}",
            reply[1]
        )));
    }
    Ok(())
}

#[tokio::test]
async fn readiness_transitions_to_false_on_shutdown() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed), "should be ready");

    // Trigger shutdown
    token.cancel();
    jh.await.ok();

    // Readiness should be false after shutdown
    assert!(
        !state.readiness.load(Ordering::Relaxed),
        "readiness should be false after shutdown"
    );
}

#[tokio::test]
async fn shutdown_drains_active_connections() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed));

    // Trigger shutdown (should drain within shutdown_grace of 30s)
    let start = std::time::Instant::now();
    token.cancel();
    jh.await.ok();
    let elapsed = start.elapsed();

    // Shutdown should complete well within the 30s grace period
    assert!(
        elapsed < Duration::from_secs(10),
        "shutdown took too long: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn shutdown_generation_remains_consistent() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let gen_before = state.generation.load(Ordering::Relaxed);
    assert_eq!(gen_before, 0);

    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Trigger shutdown
    token.cancel();
    jh.await.ok();

    // Generation should not change during shutdown
    let gen_after = state.generation.load(Ordering::Relaxed);
    assert_eq!(
        gen_before, gen_after,
        "generation should not change during shutdown"
    );
}

#[tokio::test]
async fn shutdown_active_connections_returns_to_zero() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    assert_eq!(state.active_connections.load(Ordering::Relaxed), 0);

    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Trigger shutdown
    token.cancel();
    jh.await.ok();

    assert_eq!(
        state.active_connections.load(Ordering::Relaxed),
        0,
        "active connections should be zero after shutdown"
    );
}

#[tokio::test]
async fn shutdown_stops_accepting_new_connections() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed));

    // Trigger shutdown
    token.cancel();
    jh.await.ok();

    // Active connections should be zero
    assert_eq!(
        state.active_connections.load(Ordering::Relaxed),
        0,
        "active connections should be zero after shutdown"
    );
}

#[tokio::test]
async fn shutdown_completes_within_grace_period() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed));

    // Trigger shutdown — with zero active connections it should finish instantly
    let start = std::time::Instant::now();
    token.cancel();
    jh.await.ok();
    let elapsed = start.elapsed();

    // With no connections to drain, shutdown should complete in under 2s
    assert!(
        elapsed < Duration::from_secs(2),
        "empty shutdown took too long: {:?}",
        elapsed
    );
    assert!(!state.readiness.load(Ordering::Relaxed));
}

#[tokio::test]
async fn shutdown_force_cancels_after_deadline() {
    let (backend_addr, _backend_jh) = start_slow_backend().await;

    let config = r#"
version = 1

[process]
shutdown_grace = "2s"

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[rules]]
id = "route-all"
any = true
direct = true
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..100 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed), "should be ready");

    // Get the listener address
    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        assert!(!addrs.is_empty(), "should have at least one listener");
        addrs[0]
    };

    // Connect through SOCKS5 to the slow backend
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("failed to connect to listener");
    socks5_handshake(&mut stream, backend_addr)
        .await
        .expect("SOCS5 handshake failed");

    // Verify active connection is tracked
    tokio::time::sleep(Duration::from_millis(100)).await;
    let active = state.active_connections.load(Ordering::Relaxed);
    assert!(
        active >= 1,
        "should have at least 1 active connection, got {active}"
    );

    // Trigger shutdown — the 2s grace period should forcibly cancel the connection
    let start = std::time::Instant::now();
    token.cancel();
    jh.await.ok();
    let elapsed = start.elapsed();

    // Shutdown should complete within grace period + margin, not hang forever
    assert!(
        elapsed < Duration::from_secs(6),
        "shutdown took too long with active connection: {:?}",
        elapsed
    );

    // Active connections should be zero
    assert_eq!(
        state.active_connections.load(Ordering::Relaxed),
        0,
        "active connections should be zero after forced shutdown"
    );

    // The client stream should be dead
    let mut buf = [0u8; 1];
    let result = tokio::time::timeout(Duration::from_millis(500), stream.read(&mut buf)).await;
    assert!(
        result.is_err() || matches!(result, Ok(Ok(0))),
        "client stream should be dead after forced shutdown"
    );
}
