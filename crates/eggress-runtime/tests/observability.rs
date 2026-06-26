use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;

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

async fn http_get(addr: &str, path: &str) -> (u16, String) {
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

async fn http_post(addr: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
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

async fn start_tcp_echo() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
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

fn get_admin_addr(state: &eggress_runtime::RuntimeState) -> String {
    state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string()
}

fn get_listener_addr(state: &eggress_runtime::RuntimeState) -> std::net::SocketAddr {
    let addrs = state.listener_addrs.lock().unwrap();
    addrs[0]
}

/// Start a SOCKS5 upstream proxy that handles TCP CONNECT.
async fn start_socks5_tcp_upstream() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut header = [0u8; 2];
                if stream.read_exact(&mut header).await.is_err() {
                    return;
                }
                let nmethods = header[1] as usize;
                let mut methods = vec![0u8; nmethods];
                let _ = stream.read_exact(&mut methods).await;
                let _ = stream.write_all(&[0x05, 0x00]).await;

                let mut req = [0u8; 4];
                if stream.read_exact(&mut req).await.is_err() {
                    return;
                }
                let cmd = req[1];
                let atyp = req[3];

                if cmd != 0x01 {
                    let _ = stream
                        .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                        .await;
                    return;
                }

                let target_addr = match atyp {
                    0x01 => {
                        let mut addr = [0u8; 4];
                        if stream.read_exact(&mut addr).await.is_err() {
                            return;
                        }
                        let port = stream.read_u16().await.unwrap_or(0);
                        format!("{}.{}.{}.{}:{}", addr[0], addr[1], addr[2], addr[3], port)
                    }
                    0x03 => {
                        let len = stream.read_u8().await.unwrap_or(0) as usize;
                        let mut domain = vec![0u8; len];
                        if stream.read_exact(&mut domain).await.is_err() {
                            return;
                        }
                        let port = stream.read_u16().await.unwrap_or(0);
                        let domain = String::from_utf8_lossy(&domain);
                        format!("{}:{}", domain, port)
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

                let _ = stream
                    .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await;

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

// ---------------------------------------------------------------------------
// 1. /metrics renders without panicking after a direct TCP session
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_renders_after_direct_tcp_session() {
    let echo_addr = start_tcp_echo().await;

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

    wait_ready(&state).await;
    let listener_addr = get_listener_addr(&state);
    let admin_addr = get_admin_addr(&state);

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    let ip = match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => ip.octets(),
        _ => panic!("expected IPv4"),
    };
    stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
    stream.write_all(&ip).await.unwrap();
    stream
        .write_all(&echo_addr.port().to_be_bytes())
        .await
        .unwrap();

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0x00, "CONNECT should succeed");

    stream.write_all(b"hello").await.unwrap();
    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(2), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");
    assert_eq!(&buf[..n], b"hello");

    drop(stream);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = http_get(&admin_addr, "/metrics").await;
    assert_eq!(status, 200);
    assert!(
        body.contains("eggress_connections_total"),
        "metrics should contain connections_total"
    );
    assert!(
        body.contains("eggress_connections_active"),
        "metrics should contain connections_active"
    );
    assert!(
        body.contains("eggress_bytes_upstream_total"),
        "metrics should contain bytes_upstream_total"
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 2. /metrics renders after UDP direct association
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_renders_after_udp_direct_association() {
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

    wait_ready(&state).await;
    let listener_addr = get_listener_addr(&state);
    let admin_addr = get_admin_addr(&state);

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

    let pkt = ipv4_socks5_packet(
        match echo_addr.ip() {
            std::net::IpAddr::V4(ip) => ip.octets(),
            _ => panic!("expected IPv4"),
        },
        echo_addr.port(),
        b"udp-test",
    );
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout")
    .expect("recv");
    assert!(n > 0, "should receive echo reply");

    let (status, body) = http_get(&admin_addr, "/metrics").await;
    assert_eq!(status, 200);
    assert!(
        body.contains("eggress_udp_associations_total"),
        "metrics should contain udp_associations_total"
    );
    assert!(
        body.contains("eggress_udp_packets_up_total"),
        "metrics should contain udp_packets_up_total"
    );

    drop(stream);
    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 3. /metrics renders after TCP traffic routed through an upstream group
//    (verifies upstream metrics appear in /metrics after upstream relay)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metrics_renders_after_upstream_relay() {
    let echo_addr = start_tcp_echo().await;
    let upstream_addr = start_socks5_tcp_upstream().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "socks5-up"
uri = "socks5://127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "upstream-grp"
scheduler = "first-available"
members = ["socks5-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "upstream-grp"

[admin]
bind = "127.0.0.1:0"
enabled = true
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
    let listener_addr = get_listener_addr(&state);
    let admin_addr = get_admin_addr(&state);

    // Connect through eggress SOCKS5 -> upstream SOCKS5 -> TCP echo
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    let ip = match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => ip.octets(),
        _ => panic!("expected IPv4"),
    };
    stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
    stream.write_all(&ip).await.unwrap();
    stream
        .write_all(&echo_addr.port().to_be_bytes())
        .await
        .unwrap();

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0x00, "CONNECT through upstream should succeed");

    stream.write_all(b"upstream-hello").await.unwrap();
    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");
    assert_eq!(&buf[..n], b"upstream-hello");

    drop(stream);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = http_get(&admin_addr, "/metrics").await;
    assert_eq!(status, 200);
    assert!(
        body.contains("eggress_upstream_open_total"),
        "metrics should contain upstream_open_total after upstream relay"
    );
    assert!(
        body.contains("eggress_route_decisions_total"),
        "metrics should contain route_decisions_total"
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 4. /metrics renders with upstream group configured for UDP
//    Verifies upstream metrics fields are present in /metrics after UDP
//    association with an upstream group in the routing config.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_renders_with_upstream_group_for_udp() {
    let upstream_addr = start_socks5_tcp_upstream().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "socks5-up"
uri = "socks5://127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "upstream-grp"
scheduler = "first-available"
members = ["socks5-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "upstream-grp"

[admin]
bind = "127.0.0.1:0"
enabled = true
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
    let listener_addr = get_listener_addr(&state);
    let admin_addr = get_admin_addr(&state);

    // Create a UDP association
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00);

    // Verify /metrics renders correctly with upstream group configured
    let (status, body) = http_get(&admin_addr, "/metrics").await;
    assert_eq!(status, 200);
    assert!(
        body.contains("eggress_udp_associations_total"),
        "metrics should contain udp_associations_total"
    );
    assert!(
        body.contains("eggress_upstream_open_total"),
        "metrics should contain upstream_open_total"
    );
    assert!(
        body.contains("eggress_route_decisions_total"),
        "metrics should contain route_decisions_total"
    );

    // Verify upstreams endpoint shows the configured upstream
    let (status, body) = http_get(&admin_addr, "/-/upstreams").await;
    assert_eq!(status, 200);
    assert!(
        body.contains("socks5-up"),
        "upstreams should contain socks5-up"
    );

    drop(stream);
    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 5. Route decision counters increment for direct connections
// ---------------------------------------------------------------------------

#[tokio::test]
async fn route_decision_counters_increment() {
    let echo_addr = start_tcp_echo().await;

    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[rules]]
id = "direct-route"
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

    wait_ready(&state).await;
    let listener_addr = get_listener_addr(&state);
    let admin_addr = get_admin_addr(&state);

    // --- Direct TCP connection (to echo server, matches direct-route) ---
    {
        let mut stream = tokio::net::TcpStream::connect(listener_addr)
            .await
            .expect("connect");
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut resp = [0u8; 2];
        stream.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp, [0x05, 0x00]);

        let ip = match echo_addr.ip() {
            std::net::IpAddr::V4(ip) => ip.octets(),
            _ => panic!("expected IPv4"),
        };
        stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
        stream.write_all(&ip).await.unwrap();
        stream
            .write_all(&echo_addr.port().to_be_bytes())
            .await
            .unwrap();
        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[1], 0x00);

        stream.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4096];
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            stream.read(&mut buf).await
        })
        .await;
        drop(stream);
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify route decision counters in metrics
    let (status, body) = http_get(&admin_addr, "/metrics").await;
    assert_eq!(status, 200);
    assert!(
        body.contains("eggress_route_decisions_total"),
        "metrics should contain route decisions"
    );
    assert!(
        body.contains("action=\"direct\""),
        "should have direct route decision"
    );
    assert!(
        body.contains("outcome=\"selected\""),
        "should have 'selected' outcome in route decisions"
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 6. UDP active gauges return to zero after close
// ---------------------------------------------------------------------------

#[tokio::test]
async fn udp_active_gauges_return_to_zero_after_close() {
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

    wait_ready(&state).await;
    let listener_addr = get_listener_addr(&state);
    let admin_addr = get_admin_addr(&state);

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00);

    // Verify active association gauge is > 0
    let (status, body) = http_get(&admin_addr, "/-/udp").await;
    assert_eq!(status, 200);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        json["associations_active"].as_i64().unwrap() >= 1,
        "should have at least 1 active association"
    );

    // Close TCP control connection
    drop(stream);
    tokio::time::sleep(Duration::from_secs(1)).await;

    // The /-/udp endpoint reads the MetricsRegistry gauge directly which may
    // not reflect the UdpMetrics atomic. Check /metrics instead which triggers
    // the bridge sync from UdpMetrics.
    let (status, body) = http_get(&admin_addr, "/metrics").await;
    assert_eq!(status, 200);

    // After the bridge sync in render_prometheus, the udp_associations_active
    // gauge reflects the live UdpMetrics count which should be 0 after close.
    let mut found_active_zero = false;
    for line in body.lines() {
        if line.contains("eggress_udp_associations_active") && !line.starts_with('#') {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(val) = parts.last() {
                if let Ok(n) = val.parse::<f64>() {
                    assert_eq!(
                        n, 0.0,
                        "udp associations active gauge should be 0 after close"
                    );
                    found_active_zero = true;
                }
            }
        }
    }
    assert!(
        found_active_zero,
        "should find udp_associations_active in metrics"
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 7. No client IP, target host, username, password, or payload appears as
//    metric labels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_no_secrets_in_labels() {
    let echo_addr = start_tcp_echo().await;

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

    wait_ready(&state).await;
    let listener_addr = get_listener_addr(&state);
    let admin_addr = get_admin_addr(&state);

    // Connect through proxy with known target
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    let ip = match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => ip.octets(),
        _ => panic!("expected IPv4"),
    };
    stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
    stream.write_all(&ip).await.unwrap();
    stream
        .write_all(&echo_addr.port().to_be_bytes())
        .await
        .unwrap();
    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0x00);

    let client_addr = stream.local_addr().unwrap().to_string();
    stream.write_all(b"secret-payload-data").await.unwrap();
    let mut buf = [0u8; 4096];
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        stream.read(&mut buf).await
    })
    .await;
    drop(stream);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = http_get(&admin_addr, "/metrics").await;
    assert_eq!(status, 200);

    // Client IP addresses should not appear
    assert!(
        !body.contains(&client_addr),
        "metrics should not contain client address {client_addr}"
    );

    // Target address should not appear
    assert!(
        !body.contains(&echo_addr.to_string()),
        "metrics should not contain target address {}",
        echo_addr
    );

    // Payload should not appear
    assert!(
        !body.contains("secret-payload-data"),
        "metrics should not contain payload"
    );

    // No sensitive keywords in metric labels
    assert!(
        !body.contains("password"),
        "metrics should not contain 'password'"
    );
    assert!(
        !body.contains("secret"),
        "metrics should not contain 'secret'"
    );
    assert!(
        !body.contains("token"),
        "metrics should not contain 'token'"
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 8. Admin upstream endpoint redacts credentials
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_upstreams_redact_credentials() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "shadowsocks://aes-256-gcm:supersecret@127.0.0.1:8388"

[[upstreams]]
id = "socks5-up"
uri = "socks5://admin:hunter2@127.0.0.1:1080"

[[upstream_groups]]
id = "mixed-grp"
scheduler = "first-available"
members = ["ss-up", "socks5-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "mixed-grp"

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

    wait_ready(&state).await;
    let admin_addr = get_admin_addr(&state);

    let (status, body) = http_get(&admin_addr, "/-/upstreams").await;
    assert_eq!(status, 200);

    // Credentials should never appear in upstream listing
    assert!(
        !body.contains("supersecret"),
        "upstreams should not contain password 'supersecret'"
    );
    assert!(
        !body.contains("hunter2"),
        "upstreams should not contain password 'hunter2'"
    );
    assert!(
        !body.contains("aes-256-gcm:supersecret"),
        "upstreams should not contain full credential string"
    );
    assert!(
        !body.contains("admin:hunter2"),
        "upstreams should not contain full credential string"
    );

    // IDs should still be visible
    assert!(
        body.contains("ss-up"),
        "upstreams should contain upstream id 'ss-up'"
    );
    assert!(
        body.contains("socks5-up"),
        "upstreams should contain upstream id 'socks5-up'"
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 9. Admin route-explain does not expose secrets
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_route_explain_no_secrets() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "shadowsocks://aes-256-gcm:my_secret_password@127.0.0.1:8388"

[[upstream_groups]]
id = "secure-grp"
scheduler = "first-available"
members = ["ss-up"]
fallback = "reject"

[[rules]]
id = "route-to-upstream"
any = true
upstream_group = "secure-grp"

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

    wait_ready(&state).await;
    let admin_addr = get_admin_addr(&state);

    let body = r#"{"target":"example.com:443","listener":"socks-in","protocol":"socks5"}"#;
    let (status, resp_body) = http_post(&admin_addr, "/-/route-explain", body).await;
    assert_eq!(status, 200);

    // Passwords and secrets should never appear in route-explain output
    assert!(
        !resp_body.contains("my_secret_password"),
        "route-explain should not expose upstream password"
    );
    assert!(
        !resp_body.contains("aes-256-gcm:my_secret_password"),
        "route-explain should not expose full credential URI"
    );
    assert!(
        !resp_body.contains("secret"),
        "route-explain should not contain 'secret'"
    );

    // Rule ID should be visible
    assert!(
        resp_body.contains("route-to-upstream"),
        "route-explain should contain matched rule ID"
    );

    // Action should be visible
    let json: serde_json::Value = serde_json::from_str(&resp_body).unwrap();
    assert!(
        json.get("matched_rule").is_some(),
        "route-explain should have matched_rule field"
    );

    token.cancel();
    jh.await.ok();
}
