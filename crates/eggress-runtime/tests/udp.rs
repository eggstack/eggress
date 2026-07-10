use std::io::Write;

use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

/// Perform a SOCKS5 handshake (no auth) and send a UDP ASSOCIATE command.
/// Returns the relay address from the server reply.
async fn socks5_udp_associate(stream: &mut tokio::net::TcpStream) -> std::io::Result<[u8; 10]> {
    stream.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await?;
    assert_eq!(resp, [0x05, 0x00]);

    // UDP ASSOCIATE: version=5, cmd=3, rsv=0, atyp=1 (IPv4), addr=0.0.0.0, port=0
    stream
        .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
        .await?;
    stream.write_all(&0u16.to_be_bytes()).await?;

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await?;
    Ok(reply)
}

fn ipv4_socks5_packet(target: [u8; 4], port: u16, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00, 0x01];
    pkt.extend_from_slice(&target);
    pkt.extend_from_slice(&port.to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

async fn start_udp_echo() -> std::net::SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 65535];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let _ = socket.send_to(&buf[..n], peer).await;
        }
    });
    addr
}

#[tokio::test]
async fn udp_disabled_returns_socks_failure() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        state.readiness.load(std::sync::atomic::Ordering::Relaxed),
        "should be ready"
    );

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(
        reply[1], 0x02,
        "should get REP_NOT_ALLOWED when UDP is disabled"
    );

    drop(stream);
    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn tcp_control_close_after_udp_associate() {
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

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        state.readiness.load(std::sync::atomic::Ordering::Relaxed),
        "should be ready"
    );

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate should succeed");
    assert_eq!(reply[0], 0x05, "should be SOCKS5 version");
    assert_eq!(reply[1], 0x00, "should get success reply");
    assert_eq!(reply[2], 0x00, "reserved byte should be zero");
    assert_eq!(reply[3], 0x01, "should be IPv4 address type");

    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let active = state
        .active_connections
        .load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        active, 0,
        "active connections should be zero after TCP close"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn udp_bind_conflict_aborts_startup() {
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = held.local_addr().unwrap();
    let bind_addr = addr.to_string();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "{bind_addr}"
protocols = ["socks5"]
"#,
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap().to_string();

    let mut supervisor = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let state = supervisor.state().clone();

    let result = tokio::task::spawn_blocking(move || supervisor.run())
        .await
        .expect("spawn_blocking failed");

    assert!(
        matches!(
            result,
            Err(eggress_runtime::RuntimeError::ListenerBind { .. })
        ),
        "expected ListenerBind error, got {result:?}"
    );
    assert!(
        !state.readiness.load(std::sync::atomic::Ordering::Relaxed),
        "readiness should remain false when bind fails"
    );

    drop(held);
}

#[test]
fn udp_listener_topology_change_rejected_on_reload() {
    let config1 = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[listeners]]
name = "socks-extra"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path1)
            .unwrap();
        f.write_all(config2.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Rejected { reason } => {
            assert!(
                reason.contains("listener count") || reason.contains("restart required"),
                "reason should mention topology change: {}",
                reason
            );
        }
        other => panic!("expected Rejected, got {:?}", other),
    }
}

#[test]
fn udp_config_reload_changes_generation() {
    let config1 = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[rules]]
id = "block-all"
any = true
reject = "blocked"
"#;
    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    assert_eq!(sup.state().generation(), 0);

    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path1)
            .unwrap();
        f.write_all(config2.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Applied {
            generation,
            upstreams,
        } => {
            assert_eq!(generation, 1);
            assert_eq!(upstreams, 0);
        }
        other => panic!("expected Applied, got {:?}", other),
    }
}

#[tokio::test]
async fn udp_echo_through_socks5_relay() {
    let echo_addr = start_udp_echo().await;

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

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        state.readiness.load(std::sync::atomic::Ordering::Relaxed),
        "should be ready"
    );

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate should succeed");
    assert_eq!(reply[0], 0x05, "should be SOCKS5 version");
    assert_eq!(reply[1], 0x00, "should get success reply");

    let relay_ip = std::net::Ipv4Addr::new(reply[4], reply[5], reply[6], reply[7]);
    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = std::net::SocketAddr::new(relay_ip.into(), relay_port);

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"hello e2e");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("recv should not timeout")
    .expect("recv should succeed");

    assert!(
        n > 4,
        "response should be at least SOCKS5 header, got {n} bytes"
    );
    assert_eq!(recv_buf[0], 0x00, "RSV byte 0 should be zero");
    assert_eq!(recv_buf[1], 0x00, "RSV byte 1 should be zero");
    assert_eq!(recv_buf[2], 0x00, "FRAG should be zero");

    let payload = &recv_buf[10..n];
    assert_eq!(payload, b"hello e2e", "payload should match echo input");

    drop(stream);
    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn tcp_close_removes_from_udp_registry() {
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

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(std::sync::atomic::Ordering::Relaxed));

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00);

    let active_before = state.udp_registry.active_count().await;
    assert!(active_before > 0, "should have active UDP association");

    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let active_after = state.udp_registry.active_count().await;
    assert_eq!(
        active_after, 0,
        "UDP registry active count should be zero after TCP close"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn shutdown_removes_from_udp_registry() {
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

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(std::sync::atomic::Ordering::Relaxed));

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let _reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");

    let active_before = state.udp_registry.active_count().await;
    assert!(active_before > 0);

    drop(stream);
    token.cancel();
    jh.await.ok();

    let active_after = state.udp_registry.active_count().await;
    assert_eq!(
        active_after, 0,
        "UDP registry active count should be zero after shutdown"
    );
}

#[tokio::test]
async fn idle_timeout_removes_from_udp_registry() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.udp]
enabled = true
idle_timeout = "200ms"

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

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(std::sync::atomic::Ordering::Relaxed));

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let _reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");

    let active_before = state.udp_registry.active_count().await;
    assert!(active_before > 0, "should have active UDP association");

    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    let active_after = state.udp_registry.active_count().await;
    assert_eq!(
        active_after, 0,
        "UDP registry active count should be zero after idle timeout"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn udp_relay_task_exits_on_tcp_close() {
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

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(std::sync::atomic::Ordering::Relaxed));

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00);

    let active_before = state.udp_registry.active_count().await;
    assert!(active_before > 0);

    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let active_after = state.udp_registry.active_count().await;
    assert_eq!(active_after, 0);

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn shutdown_waits_for_udp_task_completion() {
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

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(std::sync::atomic::Ordering::Relaxed));

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let _reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");

    let start = std::time::Instant::now();
    drop(stream);
    token.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "shutdown should complete within grace period, took {elapsed:?}"
    );
}

#[tokio::test]
async fn no_stale_associations_after_shutdown() {
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

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(std::sync::atomic::Ordering::Relaxed));

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    for _ in 0..3 {
        let mut stream = tokio::net::TcpStream::connect(listener_addr)
            .await
            .expect("connect");
        let _reply = socks5_udp_associate(&mut stream)
            .await
            .expect("udp associate");
    }

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    token.cancel();
    jh.await.ok();

    let active = state.udp_registry.active_count().await;
    assert_eq!(
        active, 0,
        "no stale associations should remain after shutdown"
    );
}

#[tokio::test]
async fn no_udp_task_leak_after_shutdown() {
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

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(std::sync::atomic::Ordering::Relaxed));

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    for _ in 0..5 {
        let mut stream = tokio::net::TcpStream::connect(listener_addr)
            .await
            .expect("connect");
        let _reply = socks5_udp_associate(&mut stream)
            .await
            .expect("udp associate");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    token.cancel();
    jh.await.ok();

    assert_eq!(
        state.udp_tasks.len(),
        0,
        "no UDP tasks should remain after shutdown"
    );
}

#[tokio::test]
async fn configured_advertise_ip_appears_in_socks5_reply() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.udp]
enabled = true
advertise = "10.0.0.1"

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

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(std::sync::atomic::Ordering::Relaxed));

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00);

    let relay_ip = std::net::Ipv4Addr::new(reply[4], reply[5], reply[6], reply[7]);
    assert_eq!(
        relay_ip,
        std::net::Ipv4Addr::new(10, 0, 0, 1),
        "advertised IP should appear in SOCKS5 reply"
    );

    drop(stream);
    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn udp_echo_increments_metrics_counters() {
    let echo_addr = start_udp_echo().await;

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

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(std::sync::atomic::Ordering::Relaxed));

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

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

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"metrics");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("recv timeout")
    .expect("recv failed");
    assert!(n > 0);

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let (_status, _body) = http_get_local(&admin_addr, "/metrics").await;

    let (_status, body) = http_get_local(&admin_addr, "/-/udp").await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        json["associations_active"].as_i64().unwrap() >= 1,
        "should have at least 1 active association"
    );

    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let (_status, _body) = http_get_local(&admin_addr, "/metrics").await;

    let (_status, body) = http_get_local(&admin_addr, "/-/udp").await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        json["associations_active"].as_i64().unwrap(),
        0,
        "active count should be zero after TCP close"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn metrics_do_not_expose_client_or_target_addresses() {
    let echo_addr = start_udp_echo().await;

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

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(std::sync::atomic::Ordering::Relaxed));

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");

    let relay_ip = std::net::Ipv4Addr::new(reply[4], reply[5], reply[6], reply[7]);
    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = std::net::SocketAddr::new(relay_ip.into(), relay_port);

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"privacy");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("recv timeout")
    .expect("recv failed");

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let (_status, body) = http_get_local(&admin_addr, "/metrics").await;

    let client_addr = client_socket.local_addr().unwrap().to_string();
    let target_addr = echo_addr.to_string();

    assert!(
        !body.contains(&client_addr),
        "metrics should not contain client address {client_addr}"
    );
    assert!(
        !body.contains(&target_addr),
        "metrics should not contain target address {target_addr}"
    );

    drop(stream);
    token.cancel();
    jh.await.ok();
}

// ── Standalone UDP relay integration tests ──────────────────────────────
//
// These tests exercise the standalone pproxy-compatible UDP relay directly
// (without a SOCKS5 TCP control channel), validating the cases required by
// the Phase 23 plan item 23.11.

use std::sync::Arc;

use eggress_udp::codec::decode_packet;
use eggress_udp::limits::UdpLimits;
use eggress_udp::metrics::UdpMetrics;
use eggress_udp::standalone::{standalone_udp_relay, StandaloneUdpConfig};
use tokio_util::sync::CancellationToken;

fn direct_router() -> Arc<dyn eggress_routing::RouteService> {
    Arc::new(eggress_routing::Router::new(
        vec![],
        eggress_routing::RouteActionSpec::Direct,
    ))
}

fn standalone_config(routing: Arc<dyn eggress_routing::RouteService>) -> StandaloneUdpConfig {
    StandaloneUdpConfig {
        routing,
        udp_metrics: Arc::new(UdpMetrics::new()),
        limits: UdpLimits::default(),
        listener: "test-standalone".to_string(),
        generation: 1,
        allow_private_egress: true,
    }
}

fn standalone_config_with_limits(
    routing: Arc<dyn eggress_routing::RouteService>,
    limits: UdpLimits,
) -> StandaloneUdpConfig {
    StandaloneUdpConfig {
        routing,
        udp_metrics: Arc::new(UdpMetrics::new()),
        limits,
        listener: "test-standalone".to_string(),
        generation: 1,
        allow_private_egress: true,
    }
}

fn standalone_socks5_packet(target: std::net::SocketAddr, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00];
    match target {
        std::net::SocketAddr::V4(addr) => {
            pkt.push(0x01);
            pkt.extend_from_slice(&addr.ip().octets());
        }
        std::net::SocketAddr::V6(addr) => {
            pkt.push(0x04);
            pkt.extend_from_slice(&addr.ip().octets());
        }
    }
    pkt.extend_from_slice(&target.port().to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

fn standalone_socks5_packet_frag(
    target: std::net::SocketAddr,
    frag: u8,
    payload: &[u8],
) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, frag];
    match target {
        std::net::SocketAddr::V4(addr) => {
            pkt.push(0x01);
            pkt.extend_from_slice(&addr.ip().octets());
        }
        std::net::SocketAddr::V6(addr) => {
            pkt.push(0x04);
            pkt.extend_from_slice(&addr.ip().octets());
        }
    }
    pkt.extend_from_slice(&target.port().to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

fn standalone_socks5_packet_domain(host: &str, port: u16, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00, 0x03];
    pkt.push(host.len() as u8);
    pkt.extend_from_slice(host.as_bytes());
    pkt.extend_from_slice(&port.to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

async fn start_standalone_relay(
    routing: Arc<dyn eggress_routing::RouteService>,
) -> (
    std::net::SocketAddr,
    Arc<UdpMetrics>,
    CancellationToken,
    tokio::task::JoinHandle<Result<(), eggress_udp::error::UdpError>>,
) {
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();
    let metrics = Arc::new(UdpMetrics::new());
    let mut config = standalone_config(routing);
    config.udp_metrics = metrics.clone();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    (relay_addr, metrics, cancel, handle)
}

#[tokio::test]
async fn standalone_direct_echo() {
    let echo_addr = start_udp_echo().await;
    let (relay_addr, _metrics, cancel, handle) = start_standalone_relay(direct_router()).await;

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = standalone_socks5_packet(echo_addr, b"standalone-echo");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("recv should not timeout")
    .expect("recv should succeed");

    let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
    assert_eq!(resp.payload, b"standalone-echo");

    cancel.cancel();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_malformed_short_datagram() {
    let (relay_addr, metrics, cancel, handle) = start_standalone_relay(direct_router()).await;

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    // Too short for SOCKS5 UDP header (need at least 4 bytes)
    client_socket.send(&[0x00, 0x01]).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert_eq!(
        metrics
            .standalone_malformed_datagrams
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "should record one malformed datagram"
    );

    // No response should be received
    let result = tokio::time::timeout(std::time::Duration::from_millis(200), async {
        let mut buf = [0u8; 65535];
        client_socket.recv(&mut buf).await
    })
    .await;
    assert!(
        result.is_err(),
        "should not receive any response to malformed datagram"
    );

    cancel.cancel();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_nonzero_frag_dropped() {
    let echo_addr = start_udp_echo().await;
    let (relay_addr, _metrics, cancel, handle) = start_standalone_relay(direct_router()).await;

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    // FRAG=1 should be silently dropped
    let pkt = standalone_socks5_packet_frag(echo_addr, 1, b"frag-test");
    client_socket.send(&pkt).await.unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_millis(200), async {
        let mut buf = [0u8; 65535];
        client_socket.recv(&mut buf).await
    })
    .await;
    assert!(
        result.is_err(),
        "should not receive response to FRAG=1 datagram"
    );

    cancel.cancel();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_two_clients_same_listener() {
    let echo_addr = start_udp_echo().await;
    let (relay_addr, _metrics, cancel, handle) = start_standalone_relay(direct_router()).await;

    let client_a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_a.connect(relay_addr).await.unwrap();

    let client_b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_b.connect(relay_addr).await.unwrap();

    let pkt_a = standalone_socks5_packet(echo_addr, b"client-a");
    client_a.send(&pkt_a).await.unwrap();

    let pkt_b = standalone_socks5_packet(echo_addr, b"client-b");
    client_b.send(&pkt_b).await.unwrap();

    let mut buf = [0u8; 65535];
    let n_a = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_a.recv(&mut buf).await
    })
    .await
    .expect("client a recv timeout")
    .expect("client a recv failed");
    let resp_a = decode_packet(&buf[..n_a], &UdpLimits::default()).unwrap();
    assert_eq!(resp_a.payload, b"client-a");

    let n_b = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_b.recv(&mut buf).await
    })
    .await
    .expect("client b recv timeout")
    .expect("client b recv failed");
    let resp_b = decode_packet(&buf[..n_b], &UdpLimits::default()).unwrap();
    assert_eq!(resp_b.payload, b"client-b");

    cancel.cancel();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_two_targets_from_one_client() {
    let echo_addr_a = start_udp_echo().await;
    let echo_addr_b = start_udp_echo().await;
    let (relay_addr, _metrics, cancel, handle) = start_standalone_relay(direct_router()).await;

    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.connect(relay_addr).await.unwrap();

    let pkt_a = standalone_socks5_packet(echo_addr_a, b"target-a");
    client.send(&pkt_a).await.unwrap();

    let pkt_b = standalone_socks5_packet(echo_addr_b, b"target-b");
    client.send(&pkt_b).await.unwrap();

    let mut buf = [0u8; 65535];

    // Receive response from target A
    let n_a = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client.recv(&mut buf).await
    })
    .await
    .expect("target a recv timeout")
    .expect("target a recv failed");
    let resp_a = decode_packet(&buf[..n_a], &UdpLimits::default()).unwrap();
    assert_eq!(resp_a.payload, b"target-a");

    // Receive response from target B
    let n_b = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client.recv(&mut buf).await
    })
    .await
    .expect("target b recv timeout")
    .expect("target b recv failed");
    let resp_b = decode_packet(&buf[..n_b], &UdpLimits::default()).unwrap();
    assert_eq!(resp_b.payload, b"target-b");

    cancel.cancel();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_domain_target() {
    let echo_addr = start_udp_echo().await;
    let (relay_addr, _metrics, cancel, handle) = start_standalone_relay(direct_router()).await;

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    // Use "127.0.0.1" as a domain name (SOCKS5 ATYP=0x03) to test domain resolution
    let pkt = standalone_socks5_packet_domain("127.0.0.1", echo_addr.port(), b"domain-test");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("recv timeout")
    .expect("recv failed");

    let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
    assert_eq!(resp.payload, b"domain-test");

    cancel.cancel();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_oversized_datagram_handled() {
    let (relay_addr, _metrics, cancel, handle) = start_standalone_relay(direct_router()).await;

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    // Build a packet that exceeds the default max_datagram_size (65535)
    let mut pkt = vec![0x00, 0x00, 0x00, 0x01];
    pkt.extend_from_slice(&[127, 0, 0, 1]);
    pkt.extend_from_slice(&8080u16.to_be_bytes());
    pkt.extend_from_slice(&vec![0xAA; 70000]); // Exceeds 65535

    // Should either be dropped (too large for buffer) or silently fail
    let _ = client_socket.send(&pkt).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    cancel.cancel();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_route_reject_drops_packet() {
    let rules = vec![eggress_routing::CompiledRule {
        id: eggress_routing::RuleId(std::sync::Arc::from("reject-all")),
        matcher: eggress_routing::MatchExpr::Any,
        action: eggress_routing::RouteActionSpec::Reject(eggress_core::RejectReason::AccessDenied),
    }];
    let routing: Arc<dyn eggress_routing::RouteService> = Arc::new(eggress_routing::Router::new(
        rules,
        eggress_routing::RouteActionSpec::Direct,
    ));

    let echo_addr = start_udp_echo().await;
    let (relay_addr, metrics, cancel, handle) = start_standalone_relay(routing).await;

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = standalone_socks5_packet(echo_addr, b"should-be-dropped");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert_eq!(
        metrics
            .standalone_rejected_datagrams
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "should record one rejected datagram"
    );

    cancel.cancel();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_per_client_target_limit() {
    let (_relay_addr, _metrics, cancel, handle) = start_standalone_relay(direct_router()).await;

    let limits = UdpLimits {
        max_targets_per_association: 1,
        ..UdpLimits::default()
    };
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr_limits = relay_socket.local_addr().unwrap();
    let metrics_limits = Arc::new(UdpMetrics::new());
    let mut config = standalone_config_with_limits(direct_router(), limits);
    config.udp_metrics = metrics_limits.clone();
    let cancel_limits = CancellationToken::new();

    let relay_cancel = cancel_limits.clone();
    let relay_sock = relay_socket.clone();
    let handle_limits =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr_limits).await.unwrap();

    // First packet to target port 8081 should succeed
    let pkt1 = standalone_socks5_packet(
        std::net::SocketAddr::new(std::net::Ipv4Addr::new(127, 0, 0, 1).into(), 8081),
        b"first",
    );
    client_socket.send(&pkt1).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Second packet to different target should be rejected (limit = 1)
    let pkt2 = standalone_socks5_packet(
        std::net::SocketAddr::new(std::net::Ipv4Addr::new(127, 0, 0, 1).into(), 8082),
        b"second",
    );
    client_socket.send(&pkt2).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        metrics_limits
            .standalone_rejected_datagrams
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "should reject second target when limit is 1"
    );

    cancel_limits.cancel();
    handle_limits.await.unwrap().unwrap();

    // Cleanup the original relay
    cancel.cancel();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_flow_reuse_allows_same_target() {
    let echo_addr = start_udp_echo().await;
    let (relay_addr, metrics, cancel, handle) = start_standalone_relay(direct_router()).await;

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    // Send two packets to same target - both should succeed (flow reuse)
    let pkt1 = standalone_socks5_packet(echo_addr, b"reuse1");
    client_socket.send(&pkt1).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("first recv timeout")
    .expect("first recv failed");

    let pkt2 = standalone_socks5_packet(echo_addr, b"reuse2");
    client_socket.send(&pkt2).await.unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("second recv timeout")
    .expect("second recv failed");

    // No rejections - flow reuse is allowed for same target
    assert_eq!(
        metrics
            .standalone_rejected_datagrams
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );

    cancel.cancel();
    handle.await.unwrap().unwrap();
}

async fn http_get_local(addr: &str, path: &str) -> (u16, String) {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::flush(&mut stream).await.unwrap();

    let mut response = Vec::new();
    loop {
        let mut buf = [0u8; 4096];
        match tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    let response = String::from_utf8_lossy(&response);
    let status_line = response.lines().next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let body = response.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}
