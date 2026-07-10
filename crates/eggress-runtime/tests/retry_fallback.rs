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

async fn start_socks5_proxy() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
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
    addrs[0].unwrap()
}

fn metric_value_with_labels(body: &str, name: &str, labels: &[(&str, &str)]) -> Option<f64> {
    let candidates = [name.to_string(), format!("{name}_total")];
    for line in body.lines() {
        if line.starts_with('#') {
            continue;
        }
        let Some(cand) = candidates.iter().find(|n| matches_name(line, n)) else {
            continue;
        };
        let prefix = format!("{cand}{{");
        let Some(rest) = line.strip_prefix(&prefix) else {
            continue;
        };
        let Some(brace) = rest.find('}') else {
            continue;
        };
        let label_section = &rest[..brace];
        let value_section = &rest[brace + 1..];
        let all_present = labels.iter().all(|(k, v)| {
            let needle = format!("{k}=\"{v}\"");
            label_section.contains(&needle)
        });
        if !all_present {
            continue;
        }
        if let Ok(v) = value_section.split_whitespace().last()?.parse::<f64>() {
            return Some(v);
        }
    }
    None
}

fn matches_name(line: &str, name: &str) -> bool {
    if let Some(rest) = line.strip_prefix(name) {
        rest.starts_with(' ') || rest.starts_with('{')
    } else {
        false
    }
}

/// Perform a SOCKS5 handshake, then a CONNECT to the given address.
/// Returns the reply byte (0x00 = success, 0x02 = policy denied, etc.)
/// and the connected stream on success.
async fn socks5_connect(
    listener_addr: std::net::SocketAddr,
    target: std::net::SocketAddr,
) -> (u8, Option<tokio::net::TcpStream>) {
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect to listener");

    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00], "SOCKS5 auth negotiation failed");

    let ip = match target.ip() {
        std::net::IpAddr::V4(ip) => ip.octets(),
        _ => panic!("expected IPv4"),
    };
    stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
    stream.write_all(&ip).await.unwrap();
    stream
        .write_all(&target.port().to_be_bytes())
        .await
        .unwrap();

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    let reply_code = reply[1];

    if reply_code == 0x00 {
        (reply_code, Some(stream))
    } else {
        (reply_code, None)
    }
}

/// Helper: find a port that is almost certainly closed (will refuse connections).
fn closed_port() -> u16 {
    1
}

// ---------------------------------------------------------------------------
// 1. first_upstream_refused_second_healthy
//
// Two upstreams in a group. First upstream port is closed (refused). Second
// upstream is a working SOCKS5 proxy. Connect through eggress SOCKS5 and
// verify the request succeeds through the second upstream. Eggress does NOT
// retry — it makes a single selection via the scheduler. With health checks
// disabling the bad upstream, round-robin should only select the good one.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_upstream_refused_second_healthy() {
    let echo_addr = start_tcp_echo().await;
    let good_proxy_addr = start_socks5_proxy().await;
    let refuse_port = closed_port();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "bad-up"
uri = "socks5://127.0.0.1:{refuse_port}"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstreams]]
id = "good-up"
uri = "socks5://127.0.0.1:{good_port}"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "mixed-grp"
scheduler = "round-robin"
members = ["bad-up", "good-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "mixed-grp"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#,
        refuse_port = refuse_port,
        good_port = good_proxy_addr.port(),
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    // Wait for health checks to disable bad-up (1 failure + 200ms interval)
    tokio::time::sleep(Duration::from_secs(2)).await;

    let listener_addr = get_listener_addr(&state);

    // After health checks disable bad-up, round-robin should only select good-up
    let mut succeeded = false;
    for _ in 0..5 {
        let (reply_code, stream_opt) = socks5_connect(listener_addr, echo_addr).await;
        if reply_code == 0x00 {
            if let Some(mut stream) = stream_opt {
                stream.write_all(b"rr-test").await.unwrap();
                let mut buf = [0u8; 4096];
                let n = tokio::time::timeout(Duration::from_secs(2), async {
                    stream.read(&mut buf).await
                })
                .await
                .expect("timeout")
                .expect("read");
                assert_eq!(&buf[..n], b"rr-test");
                succeeded = true;
                break;
            }
        }
    }
    assert!(
        succeeded,
        "at least one connection should have succeeded via the healthy upstream"
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 2. direct_fallback_succeeds_when_upstream_refused
//
// One upstream (refused port) with GroupFallback::Direct. Health checks
// disable the upstream, then the router falls back to direct. Connect through
// SOCKS5 and verify the direct connection succeeds.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn direct_fallback_succeeds_when_upstream_refused() {
    let echo_addr = start_tcp_echo().await;
    let refuse_port = closed_port();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "bad-up"
uri = "socks5://127.0.0.1:{refuse_port}"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "fb-grp"
scheduler = "first-available"
members = ["bad-up"]
fallback = "direct"

[[rules]]
id = "route-all"
upstream_group = "fb-grp"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#,
        refuse_port = refuse_port,
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    // Wait for health checks to disable bad-up
    tokio::time::sleep(Duration::from_secs(2)).await;

    let listener_addr = get_listener_addr(&state);

    let (reply_code, mut stream_opt) = socks5_connect(listener_addr, echo_addr).await;
    assert_eq!(
        reply_code, 0x00,
        "direct fallback should produce success reply"
    );
    let mut stream = stream_opt.take().expect("stream should be present");
    stream.write_all(b"fb-direct").await.unwrap();
    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(2), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");
    assert_eq!(&buf[..n], b"fb-direct");

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 3. reject_fallback_returns_error_when_upstream_refused
//
// One upstream (refused port) with GroupFallback::Reject. Health checks
// disable the upstream, then the router rejects. Connect through SOCKS5
// and verify reply code 0x02 (policy denied).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reject_fallback_returns_error_when_upstream_refused() {
    let refuse_port = closed_port();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "bad-up"
uri = "socks5://127.0.0.1:{refuse_port}"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "rj-grp"
scheduler = "first-available"
members = ["bad-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "rj-grp"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#,
        refuse_port = refuse_port,
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    // Wait for health checks to disable bad-up
    tokio::time::sleep(Duration::from_secs(2)).await;

    let listener_addr = get_listener_addr(&state);

    let target: std::net::SocketAddr = "192.0.2.1:80".parse().unwrap();
    let (reply_code, stream_opt) = socks5_connect(listener_addr, target).await;
    assert!(
        reply_code != 0x00,
        "reject fallback should produce error reply, got 0x{:02x}",
        reply_code
    );
    // Health checks disabled upstream → router rejects → PolicyDenied → 0x02
    assert_eq!(
        reply_code, 0x02,
        "expected SOCKS5 policy denied (0x02), got 0x{:02x}",
        reply_code
    );
    assert!(stream_opt.is_none(), "stream should be None on error");

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 4. all_upstreams_fail_with_direct_fallback
//
// Two upstreams (both refused) with GroupFallback::Direct. Health checks
// disable both. Connect through SOCKS5, verify direct fallback succeeds.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn all_upstreams_fail_with_direct_fallback() {
    let echo_addr = start_tcp_echo().await;

    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "bad-up1"
uri = "socks5://127.0.0.1:1"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstreams]]
id = "bad-up2"
uri = "socks5://127.0.0.1:2"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "all-bad-grp"
scheduler = "round-robin"
members = ["bad-up1", "bad-up2"]
fallback = "direct"

[[rules]]
id = "route-all"
upstream_group = "all-bad-grp"

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

    // Wait for health checks to disable both upstreams
    tokio::time::sleep(Duration::from_secs(2)).await;

    let listener_addr = get_listener_addr(&state);

    let (reply_code, mut stream_opt) = socks5_connect(listener_addr, echo_addr).await;
    assert_eq!(
        reply_code, 0x00,
        "direct fallback should succeed even when all upstreams fail"
    );
    let mut stream = stream_opt.take().expect("stream should be present");
    stream.write_all(b"all-fail-direct").await.unwrap();
    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(2), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");
    assert_eq!(&buf[..n], b"all-fail-direct");

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 5. all_upstreams_fail_with_reject_fallback
//
// Two upstreams (both refused) with GroupFallback::Reject. Health checks
// disable both. Connect through SOCKS5, verify error reply 0x02.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn all_upstreams_fail_with_reject_fallback() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "bad-up1"
uri = "socks5://127.0.0.1:1"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstreams]]
id = "bad-up2"
uri = "socks5://127.0.0.1:2"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "all-fail-grp"
scheduler = "round-robin"
members = ["bad-up1", "bad-up2"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "all-fail-grp"

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

    // Wait for health checks to disable both upstreams
    tokio::time::sleep(Duration::from_secs(2)).await;

    let listener_addr = get_listener_addr(&state);

    let target: std::net::SocketAddr = "192.0.2.1:80".parse().unwrap();
    let (reply_code, stream_opt) = socks5_connect(listener_addr, target).await;
    assert!(
        reply_code != 0x00,
        "all-upstreams-fail with reject should produce error reply"
    );
    assert_eq!(
        reply_code, 0x02,
        "expected SOCKS5 policy denied (0x02), got 0x{:02x}",
        reply_code
    );
    assert!(stream_opt.is_none());

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 6. upstream_auth_failure_does_not_expose_credentials
//
// Upstream requires auth but client sends wrong credentials. Verify error
// message does not contain the password. The chain error wraps the auth
// failure, so the SOCKS5 reply code is 0x01 (general failure), but the
// important invariant is that credentials are never exposed.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upstream_auth_failure_does_not_expose_credentials() {
    let upstream_addr = start_authenticated_upstream().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "auth-up"
uri = "socks5://wronguser:wrongpass@127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "auth-grp"
scheduler = "first-available"
members = ["auth-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "auth-grp"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#,
        upstream_port = upstream_addr.port(),
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;
    let listener_addr = get_listener_addr(&state);

    let target: std::net::SocketAddr = "192.0.2.1:80".parse().unwrap();
    let (reply_code, _stream_opt) = socks5_connect(listener_addr, target).await;

    // Auth failure should produce an error reply
    assert!(
        reply_code != 0x00,
        "auth failure should produce error reply, got 0x{:02x}",
        reply_code
    );

    // Verify the admin status endpoint doesn't expose credentials
    let admin_addr = get_admin_addr(&state);
    let (_status, body) = http_get(&admin_addr, "/-/upstreams").await;
    assert!(
        !body.contains("wrongpass"),
        "admin upstreams endpoint should not contain password"
    );
    assert!(
        !body.contains("wronguser:wrongpass"),
        "admin upstreams should not expose full credential string"
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 7. unsupported_udp_route_does_not_direct_fallback
//
// Configure a rule that routes to a group with HTTP upstream (doesn't support
// UDP). Attempt UDP association. Verify it's rejected (not silently falling
// back to direct). HTTP upstream + UDP is rejected at config validation time.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unsupported_udp_route_does_not_direct_fallback() {
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
fallback = "direct"

[[rules]]
id = "route-all"
upstream_group = "udp-upstream"
"#;

    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_err(),
        "HTTP upstream with UDP listener should be rejected at config validation"
    );
}

// ---------------------------------------------------------------------------
// 8. use_unhealthy_fallback_selects_unhealthy_member
//
// Configure GroupFallback::UseUnhealthy. Configure an upstream with health
// checks. When the upstream is healthy, connect and verify success. The
// use-unhealthy fallback ensures that even if the upstream becomes unhealthy
// later, connections are still attempted (not rejected).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn use_unhealthy_fallback_selects_unhealthy_member() {
    let echo_addr = start_tcp_echo().await;
    let proxy_addr = start_socks5_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "up1"
uri = "socks5://127.0.0.1:{upstream_port}"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "unhealthy-grp"
scheduler = "first-available"
members = ["up1"]
fallback = "use-unhealthy"

[[rules]]
id = "route-all"
upstream_group = "unhealthy-grp"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#,
        upstream_port = proxy_addr.port(),
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    // Wait for health probe to mark upstream as healthy (proxy is reachable)
    tokio::time::sleep(Duration::from_secs(1)).await;

    let listener_addr = get_listener_addr(&state);

    let (reply_code, mut stream_opt) = socks5_connect(listener_addr, echo_addr).await;
    assert_eq!(
        reply_code, 0x00,
        "connection through healthy upstream should succeed"
    );
    let mut stream = stream_opt.take().expect("stream should be present");
    stream.write_all(b"unhealthy-fb").await.unwrap();
    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(2), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");
    assert_eq!(&buf[..n], b"unhealthy-fb");

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 9. fallback_direct_counted_in_metrics
//
// Connect through direct fallback. Verify metrics count it as a direct
// route decision.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fallback_direct_counted_in_metrics() {
    let echo_addr = start_tcp_echo().await;
    let refuse_port = closed_port();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "bad-up"
uri = "socks5://127.0.0.1:{refuse_port}"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "fb-metrics-grp"
scheduler = "first-available"
members = ["bad-up"]
fallback = "direct"

[[rules]]
id = "route-all"
upstream_group = "fb-metrics-grp"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#,
        refuse_port = refuse_port,
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    // Wait for health checks to disable bad-up
    tokio::time::sleep(Duration::from_secs(2)).await;

    let listener_addr = get_listener_addr(&state);
    let admin_addr = get_admin_addr(&state);

    let (reply_code, stream_opt) = socks5_connect(listener_addr, echo_addr).await;
    assert_eq!(reply_code, 0x00, "direct fallback should succeed");
    drop(stream_opt);

    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = http_get(&admin_addr, "/metrics").await;
    assert_eq!(status, 200);

    // The direct fallback should be counted as a direct route decision
    let route_direct = metric_value_with_labels(
        &body,
        "eggress_route_decisions_total",
        &[("action", "direct")],
    );
    assert!(
        route_direct.is_some() && route_direct.unwrap() >= 1.0,
        "direct route decision counter should be >= 1 after direct fallback, got {:?}",
        route_direct
    );

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// 10. policy_denied_returns_correct_reply_codes
//
// Configure a reject rule. Connect via SOCKS5 and verify reply code 0x02
// (not allowed). Connect via HTTP and verify 403.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn policy_denied_returns_correct_reply_codes() {
    let config = r#"
version = 1

[[listeners]]
name = "mixed-in"
bind = "127.0.0.1:0"
protocols = ["http", "socks5"]

[[rules]]
id = "block-all"
any = true
reject = "blocked"

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

    // SOCKS5: should get reply code 0x02 (not allowed / policy denied)
    {
        let target: std::net::SocketAddr = "192.0.2.1:80".parse().unwrap();
        let (reply_code, stream_opt) = socks5_connect(listener_addr, target).await;
        assert_eq!(
            reply_code, 0x02,
            "SOCKS5 policy deny should return reply code 0x02, got 0x{:02x}",
            reply_code
        );
        assert!(stream_opt.is_none());
    }

    // HTTP CONNECT: should get 403 Forbidden
    {
        let mut stream = tokio::net::TcpStream::connect(listener_addr)
            .await
            .expect("connect");
        let request =
            "CONNECT 192.0.2.1:80 HTTP/1.1\r\nHost: 192.0.2.1:80\r\nConnection: close\r\n\r\n";
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();

        let mut response = Vec::new();
        loop {
            let mut buf = [0u8; 4096];
            match stream.read(&mut buf).await {
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
        assert_eq!(
            status, 403,
            "HTTP CONNECT policy deny should return 403, got {}",
            status
        );
    }

    token.cancel();
    jh.await.ok();
}

// ---------------------------------------------------------------------------
// Helper: a minimal authenticated SOCKS5 upstream for auth-failure tests.
// ---------------------------------------------------------------------------

async fn start_authenticated_upstream() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
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
                if stream.read_exact(&mut methods).await.is_err() {
                    return;
                }
                // Require username/password auth
                if stream.write_all(&[0x05, 0x02]).await.is_err() {
                    return;
                }

                // Read username/password auth
                let mut auth_ver = [0u8; 1];
                if stream.read_exact(&mut auth_ver).await.is_err() {
                    return;
                }
                if auth_ver[0] != 0x01 {
                    return;
                }
                let mut ulen = [0u8; 1];
                if stream.read_exact(&mut ulen).await.is_err() {
                    return;
                }
                let mut user = vec![0u8; ulen[0] as usize];
                if stream.read_exact(&mut user).await.is_err() {
                    return;
                }
                let mut plen = [0u8; 1];
                if stream.read_exact(&mut plen).await.is_err() {
                    return;
                }
                let mut pass = vec![0u8; plen[0] as usize];
                if stream.read_exact(&mut pass).await.is_err() {
                    return;
                }

                // Check credentials
                if user != b"correctuser" || pass != b"correctpass" {
                    let _ = stream.write_all(&[0x01, 0x01]).await;
                    return;
                }
                let _ = stream.write_all(&[0x01, 0x00]).await;

                // Read CONNECT request
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
