use std::io::Write;

use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        addrs[0]
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
        addrs[0]
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
