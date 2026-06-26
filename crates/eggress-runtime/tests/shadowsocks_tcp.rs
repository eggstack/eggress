use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::Once;
use std::time::Duration;

use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

static INIT: Once = Once::new();

fn install_crypto() {
    INIT.call_once(|| {
        eggress_transport_tls::install_default_crypto_provider();
    });
}

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
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
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

async fn start_shadowsocks_tcp_proxy(
    password: &str,
    method: eggress_protocol_shadowsocks::CipherMethod,
) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let password = password.to_string();
    tokio::spawn(async move {
        let _ = eggress_protocol_shadowsocks::server::run_shadowsocks_server(
            &listener, &password, method,
        )
        .await;
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_upstream_routes_tcp_echo() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let ss_addr = start_shadowsocks_tcp_proxy(
        "test-secret",
        eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
    )
    .await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "shadowsocks://aes-256-gcm:test-secret@127.0.0.1:{ss_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["ss-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        ss_port = ss_addr.port()
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

    // SOCKS5 handshake
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    // SOCKS5 CONNECT to echo server
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
    assert_eq!(reply[1], 0x00, "SOCKS5 CONNECT should succeed");

    // Send data through the proxied connection
    stream.write_all(b"hello-shadowsocks").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"hello-shadowsocks");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_upstream_wrong_password_fails() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let ss_addr = start_shadowsocks_tcp_proxy(
        "correct-password",
        eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
    )
    .await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "shadowsocks://aes-256-gcm:wrong-password@127.0.0.1:{ss_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["ss-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        ss_port = ss_addr.port()
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

    // SOCKS5 handshake
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    // SOCKS5 CONNECT to echo server
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
    assert_eq!(reply[1], 0x00, "SOCKS5 CONNECT should succeed");

    // Send data through the proxied connection - SS proxy will reject wrong password
    stream.write_all(b"hello").await.unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).await?;
        Ok::<_, std::io::Error>(n)
    })
    .await;

    // Connection should fail - either timeout, error, or zero bytes
    match result {
        Ok(Ok(0)) => {}  // zero bytes = connection closed
        Ok(Err(_)) => {} // read error
        Err(_) => {}     // timeout = connection hung
        Ok(Ok(_)) => {
            // If we got data back, the wrong password somehow worked - fail
            panic!("expected connection failure with wrong password, but got data back");
        }
    }

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_upstream_all_methods() {
    install_crypto();
    let methods = [
        (
            "aes-128-gcm",
            eggress_protocol_shadowsocks::CipherMethod::Aes128Gcm,
        ),
        (
            "aes-256-gcm",
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
        ),
        (
            "chacha20-ietf-poly1305",
            eggress_protocol_shadowsocks::CipherMethod::ChaCha20IetfPoly1305,
        ),
    ];

    for (method_name, method) in methods {
        let echo_addr = start_tcp_echo().await;
        let ss_addr = start_shadowsocks_tcp_proxy("test-secret", method).await;

        let config = format!(
            r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "shadowsocks://{method}:test-secret@127.0.0.1:{ss_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["ss-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
            method = method_name,
            ss_port = ss_addr.port()
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
        let _ = listener_addr;

        let mut stream = tokio::net::TcpStream::connect(listener_addr)
            .await
            .expect("connect");

        // SOCKS5 handshake
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut resp = [0u8; 2];
        stream.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp, [0x05, 0x00]);

        // SOCKS5 CONNECT to echo server
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
        assert_eq!(
            reply[1], 0x00,
            "SOCKS5 CONNECT should succeed with method {}",
            method_name
        );

        let payload = format!("hello-{}", method_name);
        stream.write_all(payload.as_bytes()).await.unwrap();

        let mut buf = [0u8; 4096];
        let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            stream.read(&mut buf).await
        })
        .await
        .expect("timeout")
        .expect("read");

        assert_eq!(&buf[..n], payload.as_bytes());

        drop(stream);
        token.cancel();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
        assert!(
            result.is_ok(),
            "shutdown should complete within timeout for method {}",
            method_name
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_upstream_unsupported_method_rejected() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "shadowsocks://rc4-md5:pass@127.0.0.1:{port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["ss-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        port = echo_addr.port()
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

    // Connect to eggress SOCKS5 listener
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    // SOCKS5 handshake
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    // SOCKS5 CONNECT to echo server - should fail due to unsupported method
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
    // CONNECT should fail because the upstream handshake fails
    assert_ne!(
        reply[1], 0x00,
        "SOCKS5 CONNECT should fail with unsupported method"
    );

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_upstream_direct_route_bypasses_ss() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;

    // Config with a direct-action rule — traffic routes through eggress SOCKS5
    // but connects directly to the target, never touching Shadowsocks.
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[rules]]
id = "route-direct"
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

    // Connect through eggress SOCKS5 with a direct route (no upstream)
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    // SOCKS5 handshake
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    // SOCKS5 CONNECT to echo server
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
    assert_eq!(
        reply[1], 0x00,
        "SOCKS5 CONNECT should succeed via direct route"
    );

    // Send data — should echo back without any Shadowsocks encryption
    stream.write_all(b"direct-route-test").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"direct-route-test");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_connect_inbound_routes_tcp_echo() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let ss_addr = start_shadowsocks_tcp_proxy(
        "test-secret",
        eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
    )
    .await;

    // Listener uses HTTP protocol (not SOCKS5)
    let config = format!(
        r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "ss-up"
uri = "shadowsocks://aes-256-gcm:test-secret@127.0.0.1:{ss_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["ss-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        ss_port = ss_addr.port()
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

    // Connect to eggress HTTP listener
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    // HTTP CONNECT handshake
    let connect_req = format!(
        "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n\r\n",
        echo_addr.ip(),
        echo_addr.port(),
        echo_addr.ip(),
        echo_addr.port()
    );
    stream.write_all(connect_req.as_bytes()).await.unwrap();

    // Read HTTP response — expect 200 Connection Established
    let mut response = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf))
            .await
            .expect("timeout reading HTTP response")
            .expect("read error");
        response.extend_from_slice(&buf[..n]);
        if response.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let response_str = String::from_utf8_lossy(&response);
    assert!(
        response_str.starts_with("HTTP/1.1 200"),
        "HTTP CONNECT should succeed, got: {}",
        response_str
    );

    // Send data through the tunneled connection
    stream.write_all(b"http-ss-hello").await.unwrap();

    let n = tokio::time::timeout(Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"http-ss-hello");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

fn metric_value_with_labels(body: &str, name: &str, labels: &[(&str, &str)]) -> Option<f64> {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if !matches_name(trimmed, name) {
            continue;
        }
        let all_match = labels.iter().all(|(k, v)| {
            let pattern = format!("{k}=\"{v}\"");
            trimmed.contains(&pattern)
        });
        if all_match {
            return line_value(trimmed);
        }
    }
    None
}

fn matches_name(line: &str, name: &str) -> bool {
    line.starts_with(name)
        && (line.as_bytes().get(name.len()) == Some(&b'{')
            || line.as_bytes().get(name.len()) == Some(&b' ')
            || line.len() == name.len())
}

fn line_value(line: &str) -> Option<f64> {
    line.split_whitespace().last()?.parse::<f64>().ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_upstream_metrics_increment() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let ss_addr = start_shadowsocks_tcp_proxy(
        "test-secret",
        eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
    )
    .await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "shadowsocks://aes-256-gcm:test-secret@127.0.0.1:{ss_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["ss-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#,
        ss_port = ss_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    // Wait for admin to be bound (it may not be ready when readiness fires)
    let admin_addr = {
        let mut addr = None;
        for _ in 0..100 {
            if let Some(a) = *state.admin_local_addr.lock().unwrap() {
                addr = Some(a.to_string());
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        addr.expect("admin should have bound within 5s")
    };

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    // Connect through SOCKS5 -> Shadowsocks -> echo
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    // SOCKS5 handshake
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    // SOCKS5 CONNECT to echo server
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

    // Send data through the connection
    stream.write_all(b"metrics-test").await.unwrap();
    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");
    assert_eq!(&buf[..n], b"metrics-test");

    // Close the connection and let metrics flush
    drop(stream);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Fetch metrics with a raw TCP request and explicit timeout
    let metrics_body = {
        let mut stream = tokio::net::TcpStream::connect(&admin_addr)
            .await
            .expect("connect to admin");
        let req = b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
        tokio::io::AsyncWriteExt::write_all(&mut stream, req)
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::flush(&mut stream).await.unwrap();

        let mut resp = Vec::new();
        let read_result = tokio::time::timeout(Duration::from_secs(5), async {
            let mut buf = [0u8; 8192];
            loop {
                match tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
                    Ok(0) => break,
                    Ok(n) => resp.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
        })
        .await;
        assert!(read_result.is_ok(), "timeout reading metrics");
        String::from_utf8_lossy(&resp).to_string()
    };

    let upstream_open = metric_value_with_labels(
        &metrics_body,
        "eggress_upstream_open_total_total",
        &[("protocol", "shadowsocks"), ("outcome", "success")],
    );
    assert!(
        upstream_open.is_some() && upstream_open.unwrap() >= 1.0,
        "eggress_upstream_open_total_total{{protocol=\"shadowsocks\",outcome=\"success\"}} should be >= 1 after SS relay, got {upstream_open:?}"
    );

    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}
