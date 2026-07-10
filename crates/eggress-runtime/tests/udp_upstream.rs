use std::io::Write;
use std::sync::atomic::Ordering;

use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
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

#[tokio::test]
async fn shutdown_closes_udp_flows() {
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
        .expect("udp associate");
    assert_eq!(reply[1], 0x00, "udp associate should succeed");

    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = format!("127.0.0.1:{relay_port}");

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(&relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"shutdown-test");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout")
    .expect("recv");

    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test]
async fn metrics_expose_udp_counters() {
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00);

    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = format!("127.0.0.1:{relay_port}");

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(&relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"metrics-check");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout")
    .expect("recv");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (_status, body) = http_get_local(&admin_addr, "/-/udp").await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert!(
        json.get("associations_active").is_some(),
        "/-/udp should include associations_active"
    );
    assert!(
        json.get("target_flows_active").is_some(),
        "/-/udp should include target_flows_active"
    );

    let (_status, metrics_body) = http_get_local(&admin_addr, "/metrics").await;
    assert!(
        metrics_body.contains("eggress_udp_associations_total"),
        "/metrics should expose UDP association counter"
    );
    assert!(
        metrics_body.contains("eggress_udp_packets_up_total"),
        "/metrics should expose UDP packets up counter"
    );

    drop(stream);
    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn admin_udp_endpoint_safe() {
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let (_status, body) = http_get_local(&admin_addr, "/-/udp").await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");

    assert!(
        json.get("associations_active").is_some(),
        "should have associations_active"
    );
    assert!(
        json.get("target_flows_active").is_some(),
        "should have target_flows_active"
    );
    assert!(!body.contains("127.0.0.1"), "should not leak addresses");

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn direct_fallback_forwards_direct() {
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
        .expect("udp associate");
    assert_eq!(reply[1], 0x00);

    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = format!("127.0.0.1:{relay_port}");

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(&relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"fallback-direct");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout")
    .expect("recv");

    let resp = eggress_udp::codec::decode_packet(
        &recv_buf[..n],
        &eggress_udp::limits::UdpLimits::default(),
    )
    .unwrap();
    assert_eq!(resp.payload, b"fallback-direct");

    drop(stream);
    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn runtime_udp_via_configured_socks5_upstream_echoes() {
    // 1. Start a SOCKS5 UDP test server in echo mode
    let upstream = eggress_udp::testkit::Socks5UdpTestServer::start(
        eggress_udp::testkit::Socks5TestServerConfig {
            mode: eggress_udp::testkit::Socks5TestMode::Echo,
            relay_addr: None,
        },
    )
    .await
    .unwrap();

    // 2. Start a UDP echo server to act as the "target"
    let echo_addr = start_udp_echo().await;

    // 3. Write TOML config that routes UDP through the SOCKS5 upstream
    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "socks-up"
uri = "socks5://127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "udp-upstream"
scheduler = "first-available"
members = ["socks-up"]
fallback = "reject"

[[rules]]
id = "udp-via-socks"
upstream_group = "udp-upstream"

[rules.match]
all = [
  {{ transport = "udp" }}
]
"#,
        upstream_port = upstream.tcp_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // 4. Wait for readiness
    for _ in 0..100 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed), "should be ready");

    // 5. Connect to Eggress SOCKS5 and do UDP ASSOCIATE
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
    assert_eq!(reply[1], 0x00, "udp associate should succeed");

    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = format!("127.0.0.1:{relay_port}");

    // 6. Send a UDP datagram to the relay and assert echo
    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(&relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"runtime-upstream-test");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout waiting for response")
    .expect("recv");

    let resp = eggress_udp::codec::decode_packet(
        &recv_buf[..n],
        &eggress_udp::limits::UdpLimits::default(),
    )
    .unwrap();
    assert_eq!(resp.payload, b"runtime-upstream-test");

    // 7. Shutdown
    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test]
async fn runtime_authenticated_socks5_upstream_echoes() {
    let upstream = eggress_udp::testkit::Socks5UdpTestServer::start(
        eggress_udp::testkit::Socks5TestServerConfig {
            mode: eggress_udp::testkit::Socks5TestMode::EchoWithCredentials {
                username: "user".to_string(),
                password: "pass".to_string(),
            },
            relay_addr: None,
        },
    )
    .await
    .unwrap();

    let echo_addr = start_udp_echo().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "socks-auth"
uri = "socks5://user:pass@127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "udp-upstream"
scheduler = "first-available"
members = ["socks-auth"]
fallback = "reject"

[[rules]]
id = "udp-via-socks"
upstream_group = "udp-upstream"

[rules.match]
all = [
  {{ transport = "udp" }}
]
"#,
        upstream_port = upstream.tcp_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..100 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed), "should be ready");

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
    assert_eq!(reply[1], 0x00, "udp associate should succeed");

    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = format!("127.0.0.1:{relay_port}");

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(&relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"auth-upstream-test");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout waiting for response")
    .expect("recv");

    let resp = eggress_udp::codec::decode_packet(
        &recv_buf[..n],
        &eggress_udp::limits::UdpLimits::default(),
    )
    .unwrap();
    assert_eq!(resp.payload, b"auth-upstream-test");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test]
async fn runtime_http_upstream_drops_unsupported() {
    let upstream = eggress_udp::testkit::Socks5UdpTestServer::start(
        eggress_udp::testkit::Socks5TestServerConfig {
            mode: eggress_udp::testkit::Socks5TestMode::Echo,
            relay_addr: None,
        },
    )
    .await
    .unwrap();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "http-up"
uri = "http://127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "udp-upstream"
scheduler = "first-available"
members = ["http-up"]
fallback = "reject"

[[rules]]
id = "udp-via-http"
upstream_group = "udp-upstream"

[rules.match]
all = [
  {{ transport = "udp" }}
]
"#,
        upstream_port = upstream.tcp_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    // Config validation now rejects HTTP upstream + UDP listener at config time
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_err(),
        "HTTP upstream with UDP listener should be rejected at config validation"
    );
}

#[tokio::test]
async fn runtime_multi_hop_upstream_drops_unsupported() {
    let upstream1 = eggress_udp::testkit::Socks5UdpTestServer::start(
        eggress_udp::testkit::Socks5TestServerConfig {
            mode: eggress_udp::testkit::Socks5TestMode::Echo,
            relay_addr: None,
        },
    )
    .await
    .unwrap();

    let upstream2 = eggress_udp::testkit::Socks5UdpTestServer::start(
        eggress_udp::testkit::Socks5TestServerConfig {
            mode: eggress_udp::testkit::Socks5TestMode::Echo,
            relay_addr: None,
        },
    )
    .await
    .unwrap();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "multi-hop"
uri = "socks5://127.0.0.1:{port1}__socks5://127.0.0.1:{port2}"

[[upstream_groups]]
id = "udp-upstream"
scheduler = "first-available"
members = ["multi-hop"]
fallback = "reject"

[[rules]]
id = "udp-via-multi"
upstream_group = "udp-upstream"

[rules.match]
all = [
  {{ transport = "udp" }}
]
"#,
        port1 = upstream1.tcp_addr.port(),
        port2 = upstream2.tcp_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    // Config validation now rejects multi-hop upstream + UDP listener at config time
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_err(),
        "multi-hop upstream with UDP listener should be rejected at config validation"
    );
}

#[tokio::test]
async fn runtime_target_flow_idle_timeout_releases_upstream_gauge() {
    let upstream = eggress_udp::testkit::Socks5UdpTestServer::start(
        eggress_udp::testkit::Socks5TestServerConfig {
            mode: eggress_udp::testkit::Socks5TestMode::Echo,
            relay_addr: None,
        },
    )
    .await
    .unwrap();

    let echo_addr = start_udp_echo().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[listeners.udp]
target_idle_timeout = "150ms"

[[upstreams]]
id = "socks-up"
uri = "socks5://127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "udp-upstream"
scheduler = "first-available"
members = ["socks-up"]
fallback = "reject"

[[rules]]
id = "udp-via-socks"
upstream_group = "udp-upstream"

[rules.match]
all = [
  {{ transport = "udp" }}
]
"#,
        upstream_port = upstream.tcp_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..100 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed), "should be ready");

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
    assert_eq!(reply[1], 0x00, "udp associate should succeed");

    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = format!("127.0.0.1:{relay_port}");

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(&relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"idle-timeout-test");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout waiting for response")
    .expect("recv");

    let resp = eggress_udp::codec::decode_packet(
        &recv_buf[..n],
        &eggress_udp::limits::UdpLimits::default(),
    )
    .unwrap();
    assert_eq!(resp.payload, b"idle-timeout-test");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let flows_after_send = state
        .udp_metrics
        .target_flows_active
        .load(Ordering::Relaxed);
    assert_eq!(flows_after_send, 1, "should have one active target flow");

    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let flows_after_idle = state
        .udp_metrics
        .target_flows_active
        .load(Ordering::Relaxed);
    assert_eq!(
        flows_after_idle, 0,
        "target flow should be evicted after idle timeout"
    );

    drop(stream);
    token.cancel();
    jh.await.ok();
}
