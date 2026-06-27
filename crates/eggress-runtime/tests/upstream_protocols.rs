use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::Once;

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

async fn start_socks5_tcp_proxy() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => continue,
            };
            tokio::spawn(async move {
                // SOCKS5 handshake: version + nmethods + methods
                let mut header = [0u8; 2];
                if stream.read_exact(&mut header).await.is_err() {
                    return;
                }
                let nmethods = header[1] as usize;
                let mut methods = vec![0u8; nmethods];
                if stream.read_exact(&mut methods).await.is_err() {
                    return;
                }
                // Select no-auth
                if stream.write_all(&[0x05, 0x00]).await.is_err() {
                    return;
                }

                // CONNECT request: version + cmd + rsv + atyp + addr + port
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
                    0x04 => {
                        let mut addr = [0u8; 16];
                        if stream.read_exact(&mut addr).await.is_err() {
                            return;
                        }
                        let port = stream.read_u16().await.unwrap_or(0);
                        format!(
                            "[{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}]:{}",
                            addr[0],
                            addr[1],
                            addr[2],
                            addr[3],
                            addr[4],
                            addr[5],
                            addr[6],
                            addr[7],
                            port
                        )
                    }
                    _ => return,
                };

                // Connect to target
                let target = match tokio::net::TcpStream::connect(&target_addr).await {
                    Ok(t) => t,
                    Err(_) => {
                        let _ = stream
                            .write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                            .await;
                        return;
                    }
                };

                // Send success reply
                if stream
                    .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await
                    .is_err()
                {
                    return;
                }

                // Bidirectional relay
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

async fn start_socks4_tcp_proxy() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => continue,
            };
            tokio::spawn(async move {
                // SOCKS4 connect request:
                // version(1) + cmd(1) + port(2) + ip(4) + userid(null-terminated)
                let mut header = [0u8; 8];
                if stream.read_exact(&mut header).await.is_err() {
                    return;
                }
                let version = header[0];
                let cmd = header[1];
                if version != 0x04 || cmd != 0x01 {
                    // Not SOCKS4 CONNECT
                    return;
                }
                let port = u16::from_be_bytes([header[2], header[3]]);
                let ip = [header[4], header[5], header[6], header[7]];

                // Read null-terminated userid
                let mut userid = Vec::new();
                loop {
                    let mut byte = [0u8; 1];
                    if stream.read_exact(&mut byte).await.is_err() {
                        return;
                    }
                    if byte[0] == 0 {
                        break;
                    }
                    userid.push(byte[0]);
                }
                let _ = userid;

                let target_addr = format!("{}.{}.{}.{}:{}", ip[0], ip[1], ip[2], ip[3], port);

                // Connect to target
                let target = match tokio::net::TcpStream::connect(&target_addr).await {
                    Ok(t) => t,
                    Err(_) => {
                        // SOCKS4 failure reply: vn=0, cd=91, port=0, ip=0.0.0.0
                        let _ = stream.write_all(&[0x00, 0x5B, 0, 0, 0, 0, 0, 0]).await;
                        return;
                    }
                };

                // SOCKS4 success reply: vn=0, cd=90, port=0, ip=0.0.0.0
                if stream
                    .write_all(&[0x00, 0x5A, 0, 0, 0, 0, 0, 0])
                    .await
                    .is_err()
                {
                    return;
                }

                // Bidirectional relay
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

async fn start_http_connect_proxy() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => continue,
            };
            tokio::spawn(async move {
                // Read HTTP request line
                let mut request = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => return,
                        Ok(n) => {
                            request.extend_from_slice(&buf[..n]);
                            if request.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => return,
                    }
                }

                let request_str = String::from_utf8_lossy(&request);
                let first_line = request_str.lines().next().unwrap_or("");
                let parts: Vec<&str> = first_line.split_whitespace().collect();

                if parts.len() < 2 || parts[0] != "CONNECT" {
                    let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                    return;
                }

                let target_addr = parts[1].to_string();

                // Connect to target
                let target = match tokio::net::TcpStream::connect(&target_addr).await {
                    Ok(t) => t,
                    Err(_) => {
                        let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                        return;
                    }
                };

                // Send 200 Connection Established
                if stream
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .await
                    .is_err()
                {
                    return;
                }

                // Bidirectional relay
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn socks5_upstream_routes_tcp_echo() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let upstream_addr = start_socks5_tcp_proxy().await;

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

    // Connect to eggress SOCKS5 listener
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
    stream.write_all(b"hello-upstream").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"hello-upstream");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn socks4_upstream_routes_tcp_echo() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let upstream_addr = start_socks4_tcp_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "socks4-up"
uri = "socks4://127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["socks4-up"]
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

    // Connect to eggress SOCKS5 listener
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
        "SOCKS5 CONNECT should succeed via SOCKS4 upstream"
    );

    // Send data through the proxied connection
    stream.write_all(b"socks4-test").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"socks4-test");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_connect_upstream_routes_tcp_echo() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let upstream_addr = start_http_connect_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "http-up"
uri = "http://127.0.0.1:{upstream_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["http-up"]
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

    // Connect to eggress SOCKS5 listener
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
        "SOCKS5 CONNECT should succeed via HTTP CONNECT upstream"
    );

    // Send data through the proxied connection
    stream.write_all(b"http-connect-test").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"http-connect-test");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

// Shadowsocks upstream TCP tests moved to tests/shadowsocks_tcp.rs

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_upstream_with_udp_listener_rejected_at_config() {
    install_crypto();
    // Config validation rejects HTTP upstream + UDP listener at config time
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
        "HTTP upstream with UDP listener should be rejected at config validation"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn socks4_upstream_with_udp_listener_rejected_at_config() {
    install_crypto();
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
        "SOCKS4 upstream with UDP listener should be rejected at config validation"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_upstream_with_udp_listener_accepted() {
    install_crypto();
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trojan_upstream_with_udp_listener_rejected_at_config() {
    install_crypto();
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
        "Trojan upstream with UDP listener should be rejected at config validation"
    );
}
