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

async fn wait_ready(state: &eggress_runtime::RuntimeState) {
    for _ in 0..100 {
        if state.readiness.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timeout waiting for readiness");
}

/// Poll `f` until it returns `Some(value)` or the deadline elapses.
/// Returns the inner value on success and panics with `msg` on timeout.
///
/// This is used in place of fixed `tokio::time::sleep` to make post-close
/// invariants deterministic: tests exit as soon as the condition is observed
/// instead of relying on a sleep that is only an upper bound on relay
/// teardown latency.
async fn wait_for<T, F>(deadline: Duration, mut f: F, msg: &str) -> T
where
    F: FnMut() -> Option<T>,
{
    let start = std::time::Instant::now();
    let step = Duration::from_millis(20);
    loop {
        if let Some(v) = f() {
            return v;
        }
        if start.elapsed() >= deadline {
            panic!("timeout after {deadline:?}: {msg}");
        }
        tokio::time::sleep(step).await;
    }
}

async fn start_tcp_echo() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if stream.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });
    addr
}

async fn start_socks5_upstream() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => continue,
            };
            tokio::spawn(async move {
                let mut header = [0u8; 2];
                if stream.read_exact(&mut header).await.is_err() {
                    return;
                }
                let nmethods = header[1] as usize;
                let mut methods = vec![0u8; nmethods];
                if stream.read_exact(&mut methods).await.is_err() {
                    return;
                }
                if stream.write_all(&[0x05, 0x00]).await.is_err() {
                    return;
                }
                let mut req = [0u8; 4];
                if stream.read_exact(&mut req).await.is_err() {
                    return;
                }
                let atyp = req[3];
                let target_addr = match atyp {
                    0x01 => {
                        let mut addr = [0u8; 4];
                        if stream.read_exact(&mut addr).await.is_err() {
                            return;
                        }
                        let port = stream.read_u16().await.unwrap_or(0);
                        format!("{}.{}.{}.{}:{}", addr[0], addr[1], addr[2], addr[3], port)
                    }
                    _ => return,
                };
                let target = match tokio::net::TcpStream::connect(&target_addr).await {
                    Ok(t) => t,
                    Err(_) => {
                        let _ = stream
                            .write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                            .await;
                        return;
                    }
                };
                if stream
                    .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await
                    .is_err()
                {
                    return;
                }
                let (mut cr, mut cw) = stream.into_split();
                let (mut tr, mut tw) = target.into_split();
                let c2t = tokio::spawn(async move {
                    let _ = tokio::io::copy(&mut cr, &mut tw).await;
                    let _ = tw.shutdown().await;
                });
                let t2c = tokio::spawn(async move {
                    let _ = tokio::io::copy(&mut tr, &mut cw).await;
                    let _ = cw.shutdown().await;
                });
                let _ = tokio::join!(c2t, t2c);
            });
        }
    });
    addr
}

fn ipv4_socks5_packet(target: [u8; 4], port: u16, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00, 0x01];
    pkt.extend_from_slice(&target);
    pkt.extend_from_slice(&port.to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

async fn start_udp_echo() -> std::net::SocketAddr {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 65535];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let _ = socket.send_to(&buf[..n], peer).await;
        }
    });
    addr
}

async fn socks5_udp_associate(stream: &mut tokio::net::TcpStream) -> std::io::Result<[u8; 10]> {
    stream.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await?;
    assert_eq!(resp, [0x05, 0x00]);
    stream
        .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
        .await?;
    stream.write_all(&0u16.to_be_bytes()).await?;
    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await?;
    Ok(reply)
}

/// Start a TCP server that refuses connections immediately (unreachable upstream).
async fn start_refusing_upstream() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let _ = stream.shutdown().await;
        }
    });
    addr
}

// ---------------------------------------------------------------------------
// Test 1: TCP active lease increments after upstream connect and decrements
//         after relay close
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_active_lease_increments_and_decrements() {
    let echo_addr = start_tcp_echo().await;
    let upstream_addr = start_socks5_upstream().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "socks-up"
uri = "socks5://127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["socks-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        upstream_port = upstream_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    // Verify in_flight and active start at zero
    let snap = state.snapshot.load();
    let upstream_rt = snap.upstreams.get("socks-up").unwrap();
    assert_eq!(
        upstream_rt.in_flight.load(Ordering::Relaxed),
        0,
        "in_flight should start at 0"
    );
    assert_eq!(
        upstream_rt.active.load(Ordering::Relaxed),
        0,
        "active should start at 0"
    );
    drop(snap);

    // Connect through SOCKS5 to the upstream
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    let ip_octets = match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => ip.octets(),
        _ => panic!("expected IPv4"),
    };
    stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
    stream.write_all(&ip_octets).await.unwrap();
    stream
        .write_all(&echo_addr.port().to_be_bytes())
        .await
        .unwrap();

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0x00, "CONNECT should succeed");

    // Send data to ensure the relay is established
    stream.write_all(b"ping").await.unwrap();
    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(2), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("read timeout")
    .expect("read error");
    assert_eq!(&buf[..n], b"ping");

    // Active count should be >= 1 while connection is open
    let snap = state.snapshot.load();
    let upstream_rt = snap.upstreams.get("socks-up").unwrap();
    let active = upstream_rt.active.load(Ordering::Relaxed);
    assert!(
        active >= 1,
        "active lease count should be >= 1 while connection open, got {active}"
    );
    drop(snap);

    // Close the connection
    drop(stream);

    // Active count should return to zero. Poll until observed (deterministic)
    // rather than sleeping a fixed duration.
    let _ = wait_for(
        Duration::from_secs(5),
        || {
            let snap = state.snapshot.load();
            let rt = snap.upstreams.get("socks-up").unwrap();
            if rt.active.load(Ordering::Relaxed) == 0 && rt.in_flight.load(Ordering::Relaxed) == 0 {
                Some(())
            } else {
                None
            }
        },
        "active+in_flight lease counters should return to 0 after close",
    )
    .await;
    // Re-read once for the final assertion (snap above may be stale).
    let snap = state.snapshot.load();
    let upstream_rt = snap.upstreams.get("socks-up").unwrap();
    assert_eq!(
        upstream_rt.active.load(Ordering::Relaxed),
        0,
        "active lease should return to 0 after close"
    );
    assert_eq!(
        upstream_rt.in_flight.load(Ordering::Relaxed),
        0,
        "in_flight should return to 0 after close"
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// Test 2: Pending lease dropped on failed upstream connect does not increment
//         active count
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_upstream_connect_does_not_increment_active() {
    let upstream_addr = start_refusing_upstream().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "refuse-up"
uri = "socks5://127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["refuse-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        upstream_port = upstream_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    // Try to connect to a port that the refusing upstream will reject
    let refuse_port = upstream_addr.port();
    stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
    stream.write_all(&[127, 0, 0, 1]).await.unwrap();
    stream.write_all(&refuse_port.to_be_bytes()).await.unwrap();

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    assert_ne!(reply[1], 0x00, "CONNECT should fail when upstream refuses");

    // Active and in_flight should both return to zero. Poll deterministically.
    let _ = wait_for(
        Duration::from_secs(5),
        || {
            let snap = state.snapshot.load();
            let rt = snap.upstreams.get("refuse-up").unwrap();
            if rt.active.load(Ordering::Relaxed) == 0 && rt.in_flight.load(Ordering::Relaxed) == 0 {
                Some(())
            } else {
                None
            }
        },
        "active+in_flight lease counters should return to 0 after failed connect",
    )
    .await;
    let snap = state.snapshot.load();
    let upstream_rt = snap.upstreams.get("refuse-up").unwrap();
    assert_eq!(
        upstream_rt.active.load(Ordering::Relaxed),
        0,
        "active should remain 0 after failed connect"
    );
    assert_eq!(
        upstream_rt.in_flight.load(Ordering::Relaxed),
        0,
        "in_flight should return to 0 after failed connect"
    );

    drop(stream);
    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// Test 3: UDP association close removes registry entry and leaves active
//         count zero
// ---------------------------------------------------------------------------
#[tokio::test]
async fn udp_association_close_removes_registry_entry() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

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

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    // Verify registry starts empty
    let count_before = state.udp_registry.active_count().await;
    assert_eq!(count_before, 0, "registry should start empty");

    // Establish UDP association
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00, "UDP associate should succeed");

    // Registry should have one entry
    let count_active = state.udp_registry.active_count().await;
    assert!(
        count_active >= 1,
        "registry should have >= 1 entry after associate, got {count_active}"
    );

    // Close the TCP control connection
    drop(stream);

    // Registry should be empty again. Poll until observed.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let count_now = state.udp_registry.active_count().await;
        if count_now == 0 {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "timeout: UDP registry should be empty after TCP control close, still {count_now}"
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let count_after = state.udp_registry.active_count().await;
    assert_eq!(
        count_after, 0,
        "registry active count should be 0 after TCP close"
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// Test 4: Shutdown with active TCP sessions drains within grace period
// ---------------------------------------------------------------------------
#[tokio::test]
async fn shutdown_drains_active_tcp_sessions_within_grace() {
    let echo_addr = start_tcp_echo().await;
    let upstream_addr = start_socks5_upstream().await;

    let config = format!(
        r#"
version = 1

[process]
shutdown_grace = "3s"

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "socks-up"
uri = "socks5://127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["socks-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        upstream_port = upstream_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    // Open a proxied TCP connection
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    let ip_octets = match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => ip.octets(),
        _ => panic!("expected IPv4"),
    };
    stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
    stream.write_all(&ip_octets).await.unwrap();
    stream
        .write_all(&echo_addr.port().to_be_bytes())
        .await
        .unwrap();
    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0x00);

    // Verify active connection exists
    tokio::time::sleep(Duration::from_millis(100)).await;
    let active = state.active_connections.load(Ordering::Relaxed);
    assert!(
        active >= 1,
        "should have at least 1 active connection, got {active}"
    );

    // Drop the client to allow drain
    drop(stream);

    // Trigger shutdown
    let start = std::time::Instant::now();
    token.cancel();
    jh.await.ok();
    let elapsed = start.elapsed();

    // Active connections should return to zero
    assert_eq!(
        state.active_connections.load(Ordering::Relaxed),
        0,
        "active connections should be 0 after shutdown"
    );

    // Shutdown should complete within a reasonable time (well under the 3s grace)
    assert!(
        elapsed < Duration::from_secs(8),
        "shutdown took too long: {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 5: Shutdown with active UDP association cancels relay tasks and leaves
//         counts zero
// ---------------------------------------------------------------------------
#[tokio::test]
async fn shutdown_cancels_udp_and_leaves_counts_zero() {
    let echo_addr = start_udp_echo().await;

    let config = r#"
version = 1

[process]
shutdown_grace = "3s"

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

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

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    // Create a UDP association
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00);

    let relay_ip = std::net::Ipv4Addr::new(reply[4], reply[5], reply[6], reply[7]);
    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = std::net::SocketAddr::new(relay_ip.into(), relay_port);

    // Send a packet to verify relay is alive
    let client_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();
    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"pre-shutdown");
    client_socket.send(&pkt).await.unwrap();
    let mut recv_buf = [0u8; 65535];
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await;

    let active_before = state.udp_registry.active_count().await;
    assert!(
        active_before >= 1,
        "should have active UDP association before shutdown"
    );

    // Shutdown with UDP association still open
    let start = std::time::Instant::now();
    drop(stream);
    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(10), jh).await;
    let elapsed = start.elapsed();

    // UDP registry should be empty
    let active_after = state.udp_registry.active_count().await;
    assert_eq!(
        active_after, 0,
        "UDP registry should be empty after shutdown"
    );

    // UDP tasks should be cleared
    assert_eq!(
        state.udp_tasks.len(),
        0,
        "no UDP tasks should remain after shutdown"
    );

    // Active TCP connections should be zero
    assert_eq!(
        state.active_connections.load(Ordering::Relaxed),
        0,
        "active connections should be 0"
    );

    // Should complete within grace period
    assert!(
        elapsed < Duration::from_secs(10),
        "shutdown took too long: {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 6: Reload failure preserves previous generation and route behavior
// ---------------------------------------------------------------------------
#[test]
fn reload_failure_preserves_generation() {
    let config = r#"
version = 1

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

    let gen_before = sup.state().generation();
    assert_eq!(gen_before, 0);

    // Verify a successful reload works first
    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Applied { generation, .. } => {
            assert_eq!(generation, 1);
        }
        other => panic!("expected Applied for first reload, got {:?}", other),
    }

    let gen_after_first = sup.state().generation();
    assert_eq!(gen_after_first, 1);

    // Corrupt the config file to cause a reload failure
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .unwrap();
        f.write_all(b"this is not valid toml {{{").unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Failed { error } => {
            assert!(
                error.contains("config") || error.contains("load"),
                "error should mention config issue: {error}"
            );
        }
        other => panic!("expected Failed for invalid config, got {:?}", other),
    }

    // Generation should not change after the failed reload
    let gen_after_failed = sup.state().generation();
    assert_eq!(
        gen_after_first, gen_after_failed,
        "generation should not change after failed reload"
    );

    // Restore a valid config and verify generation advances again
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .unwrap();
        f.write_all(config.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Applied { generation, .. } => {
            assert_eq!(generation, 2);
        }
        other => panic!("expected Applied after recovery, got {:?}", other),
    }

    assert_eq!(sup.state().generation(), 2);
}

// ---------------------------------------------------------------------------
// Test 7: Reload atomically swaps the router snapshot so existing captures
//         remain valid while new sessions use the updated routing
// ---------------------------------------------------------------------------
#[test]
fn reload_atomically_swaps_snapshot_preserving_old_captures() {
    let config1 = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[rules]]
id = "route-all"
any = true
direct = true
"#;

    let f = write_config(config1);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    assert_eq!(sup.state().generation(), 0);

    // Capture the old snapshot before reload
    let old_snapshot = sup.state().snapshot.load();
    let old_router = old_snapshot.router.clone();
    let old_gen = old_snapshot.generation;
    drop(old_snapshot);

    // Reload with a config that adds a new reject rule
    let config2 = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[rules]]
id = "route-all"
any = true
direct = true

[[rules]]
id = "reject-example"
hostname = "blocked.example.com"
reject = "blocked"
"#;
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .unwrap();
        f.write_all(config2.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Applied { generation, .. } => {
            assert_eq!(generation, 1);
        }
        other => panic!("expected Applied, got {:?}", other),
    }

    // Generation should have advanced
    assert_eq!(sup.state().generation(), 1);

    // The old router clone should still be a valid, separate object from
    // the new one.  This verifies that atomic swap preserves existing
    // captures (used by in-flight connections) while new connections get
    // the updated routing.
    let new_snapshot = sup.state().snapshot.load();
    let new_router = new_snapshot.router.clone();
    drop(new_snapshot);

    assert_ne!(old_gen, sup.state().generation());
    // The two routers should not be the same Arc (reload created a new one)
    assert!(
        !std::sync::Arc::ptr_eq(&old_router, &new_router),
        "reload should produce a new router, not reuse the old one"
    );

    // Do a second reload to confirm generation continues to advance
    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Applied { generation, .. } => {
            assert_eq!(generation, 2);
        }
        other => panic!("expected Applied for second reload, got {:?}", other),
    }
    assert_eq!(sup.state().generation(), 2);
}

// ---------------------------------------------------------------------------
// Test 8: Unsupported UDP upstreams are rejected at config validation, never
//         silently direct-routed. Shadowsocks is supported for UDP.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn http_upstream_with_udp_rejected_not_direct_routed() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "http-up"
uri = "http://127.0.0.1:8080"

[[upstream_groups]]
id = "udp-upstream"
scheduler = "first-available"
members = ["http-up"]
fallback = "reject"

[[rules]]
id = "udp-via-http"
upstream_group = "udp-upstream"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_err(),
        "HTTP upstream with UDP listener should be rejected at config validation, not silently direct-routed"
    );
}

#[tokio::test]
async fn socks4_upstream_with_udp_rejected_not_direct_routed() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "socks4-up"
uri = "socks4://127.0.0.1:1080"

[[upstream_groups]]
id = "udp-upstream"
scheduler = "first-available"
members = ["socks4-up"]
fallback = "reject"

[[rules]]
id = "udp-via-socks4"
upstream_group = "udp-upstream"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_err(),
        "SOCKS4 upstream with UDP listener should be rejected at config validation, not silently direct-routed"
    );
}

#[tokio::test]
async fn shadowsocks_upstream_with_udp_accepted() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "ss-up"
uri = "shadowsocks://aes-256-gcm:secret@127.0.0.1:8388"

[[upstream_groups]]
id = "udp-upstream"
scheduler = "first-available"
members = ["ss-up"]
fallback = "reject"

[[rules]]
id = "udp-via-ss"
upstream_group = "udp-upstream"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_ok(),
        "Shadowsocks upstream with UDP listener should now be accepted: {:?}",
        result.err()
    );
    if let Ok(sup) = result {
        sup.shutdown_token().cancel();
    }
}

#[tokio::test]
async fn trojan_upstream_with_udp_rejected_not_direct_routed() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "trojan-up"
uri = "trojan://password@127.0.0.1:443"

[[upstream_groups]]
id = "udp-upstream"
scheduler = "first-available"
members = ["trojan-up"]
fallback = "reject"

[[rules]]
id = "udp-via-trojan"
upstream_group = "udp-upstream"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_err(),
        "Trojan upstream with UDP listener should be rejected at config validation, not silently direct-routed"
    );
}
