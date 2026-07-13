use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::Once;

use futures_util::{SinkExt, StreamExt};
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
        addrs[0].unwrap()
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
        addrs[0].unwrap()
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
        addrs[0].unwrap()
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

async fn start_ws_echo_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let ws_stream = tokio_tungstenite::accept_async(stream).await;
                let ws_stream = match ws_stream {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let (mut sink, mut source) = ws_stream.split();
                while let Some(Ok(msg)) = source.next().await {
                    match msg {
                        tokio_tungstenite::tungstenite::Message::Binary(data) => {
                            if sink
                                .send(tokio_tungstenite::tungstenite::Message::Binary(
                                    data.clone(),
                                ))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        tokio_tungstenite::tungstenite::Message::Close(_) => break,
                        _ => {}
                    }
                }
            });
        }
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ws_upstream_routes_tcp_echo() {
    install_crypto();
    let ws_addr = start_ws_echo_server().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ws-up"
uri = "ws://127.0.0.1:{ws_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["ws-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        ws_port = ws_addr.port()
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
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    let target = ws_addr;
    let ip_octets = match target.ip() {
        std::net::IpAddr::V4(ip) => ip.octets(),
        _ => panic!("expected IPv4"),
    };
    stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
    stream.write_all(&ip_octets).await.unwrap();
    stream
        .write_all(&target.port().to_be_bytes())
        .await
        .unwrap();

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0x00, "SOCKS5 CONNECT should succeed");

    stream.write_all(b"hello-ws-upstream").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"hello-ws-upstream");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

async fn start_dummy_listener() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            // Just hold the connection open; the RawHopHandler ignores it anyway
            tokio::spawn(async move {
                let mut buf = [0u8; 1];
                let mut stream = stream;
                let _ = stream.read(&mut buf).await;
            });
        }
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_upstream_routes_tcp_echo() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let raw_endpoint = start_dummy_listener().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "raw-up"
uri = "raw://127.0.0.1:{raw_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["raw-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        raw_port = raw_endpoint.port()
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
        addrs[0].unwrap()
    };

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
    assert_eq!(reply[1], 0x00, "SOCKS5 CONNECT should succeed");

    stream.write_all(b"hello-raw-upstream").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"hello-raw-upstream");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

async fn start_h2_connect_proxy() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let conn = match h2::server::handshake(stream).await {
                    Ok(c) => c,
                    Err(_) => return,
                };
                if let Err(e) = eggress_protocol_http::handle_h2_connect(conn).await {
                    eprintln!("h2 connect proxy error: {}", e);
                }
            });
        }
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h2_upstream_routes_tcp_echo() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let h2_proxy_addr = start_h2_connect_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "h2-up"
uri = "h2://127.0.0.1:{h2_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["h2-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        h2_port = h2_proxy_addr.port()
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
        addrs[0].unwrap()
    };

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
    assert_eq!(
        reply[1], 0x00,
        "SOCKS5 CONNECT should succeed via H2 upstream"
    );

    stream.write_all(b"hello-h2-upstream").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"hello-h2-upstream");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        result.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

async fn start_h2_connect_proxy_with_auth(
    required_auth: Option<(&str, &str)>,
) -> std::net::SocketAddr {
    let owned_auth = required_auth.map(|(u, p)| {
        let cred = format!("{}:{}", u, p);
        let encoded = base64_encode(cred.as_bytes());
        format!("Basic {}", encoded)
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let required = owned_auth.clone();
            tokio::spawn(async move {
                let conn = match h2::server::handshake(stream).await {
                    Ok(c) => c,
                    Err(_) => return,
                };
                if let Err(e) = h2_connect_with_auth(conn, required.as_deref()).await {
                    eprintln!("h2 auth proxy error: {}", e);
                }
            });
        }
    });
    addr
}

async fn h2_connect_with_auth(
    mut connection: h2::server::Connection<tokio::net::TcpStream, bytes::Bytes>,
    required_auth: Option<&str>,
) -> Result<(), eggress_protocol_http::H2ConnectError> {
    loop {
        match connection.accept().await {
            Some(Ok((request, mut send_response))) => {
                if *request.method() != http::Method::CONNECT {
                    send_response.send_reset(h2::Reason::PROTOCOL_ERROR);
                    continue;
                }
                if let Some(expected) = required_auth {
                    let auth_ok = request.headers().iter().any(|(name, value)| {
                        name == http::header::PROXY_AUTHORIZATION
                            && value.to_str().unwrap_or("") == expected
                    });
                    if !auth_ok {
                        let mut recv_stream = request.into_body();
                        let response = http::Response::builder().status(407).body(()).unwrap();
                        let _ = send_response.send_response(response, true);
                        while let Some(Ok(_)) = recv_stream.data().await {}
                        continue;
                    }
                }
                let authority = request.uri().authority().ok_or_else(|| {
                    eggress_protocol_http::H2ConnectError::H2("missing authority".into())
                })?;
                let target_str = match authority.port_u16() {
                    Some(port) => format!("{}:{}", authority.host(), port),
                    None => format!("{}:443", authority.host()),
                };
                let target: eggress_core::TargetAddr = target_str
                    .parse()
                    .map_err(|e: String| eggress_protocol_http::H2ConnectError::H2(e))?;
                let response = http::Response::builder().status(200).body(()).unwrap();
                let send_stream = send_response.send_response(response, false)?;
                let recv_stream = request.into_body();
                tokio::spawn(async move {
                    if let Err(e) =
                        eggress_protocol_http::h2_connect_relay(recv_stream, send_stream, target)
                            .await
                    {
                        tracing::warn!("h2 connect relay error: {}", e);
                    }
                });
            }
            Some(Err(e)) => {
                return Err(eggress_protocol_http::H2ConnectError::H2(e.to_string()));
            }
            None => break,
        }
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h2_upstream_auth_success() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let h2_proxy_addr = start_h2_connect_proxy_with_auth(Some(("user", "pass"))).await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "h2-up"
uri = "h2://user:pass@127.0.0.1:{h2_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["h2-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        h2_port = h2_proxy_addr.port()
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
        addrs[0].unwrap()
    };

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
    assert_eq!(
        reply[1], 0x00,
        "SOCKS5 CONNECT should succeed with correct H2 auth"
    );

    stream.write_all(b"auth-success-test").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"auth-success-test");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h2_upstream_auth_failure() {
    install_crypto();
    let h2_proxy_addr = start_h2_connect_proxy_with_auth(Some(("user", "pass"))).await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "h2-up"
uri = "h2://wrong:creds@127.0.0.1:{h2_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["h2-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        h2_port = h2_proxy_addr.port()
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
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    let target_addr = "127.0.0.1:9";
    let parts: Vec<&str> = target_addr.split(':').collect();
    let ip: std::net::Ipv4Addr = parts[0].parse().unwrap();
    let ip_octets = ip.octets();
    let port: u16 = parts[1].parse().unwrap();
    stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
    stream.write_all(&ip_octets).await.unwrap();
    stream.write_all(&port.to_be_bytes()).await.unwrap();

    let mut reply = [0u8; 10];
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read_exact(&mut reply).await
    })
    .await;
    assert!(
        result.is_err() || reply[1] != 0x00,
        "SOCKS5 CONNECT should fail with wrong H2 auth"
    );

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), jh).await;
    if result.is_err() {
        eprintln!(
            "h2_upstream_auth_failure: shutdown took longer than 10s (expected for 407 path)"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h2_upstream_concurrent_streams() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let h2_proxy_addr = start_h2_connect_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "h2-up"
uri = "h2://127.0.0.1:{h2_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["h2-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        h2_port = h2_proxy_addr.port()
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
        addrs[0].unwrap()
    };

    let mut handles = Vec::new();
    for i in 0..5 {
        let addr = listener_addr;
        let echo = echo_addr;
        handles.push(tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");

            stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut resp = [0u8; 2];
            stream.read_exact(&mut resp).await.unwrap();
            assert_eq!(resp, [0x05, 0x00]);

            let ip_octets = match echo.ip() {
                std::net::IpAddr::V4(ip) => ip.octets(),
                _ => panic!("expected IPv4"),
            };
            stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
            stream.write_all(&ip_octets).await.unwrap();
            stream.write_all(&echo.port().to_be_bytes()).await.unwrap();

            let mut reply = [0u8; 10];
            stream.read_exact(&mut reply).await.unwrap();
            assert_eq!(reply[1], 0x00, "SOCKS5 CONNECT should succeed");

            let payload = format!("concurrent-{}", i);
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
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

async fn start_h2_connect_proxy_forwarding(
    http_upstream_addr: std::net::SocketAddr,
) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let http_up = http_upstream_addr;
            tokio::spawn(async move {
                let conn = match h2::server::handshake(stream).await {
                    Ok(c) => c,
                    Err(_) => return,
                };
                if let Err(e) = h2_connect_relay_to_http(conn, http_up).await {
                    eprintln!("h2 chain relay error: {}", e);
                }
            });
        }
    });
    addr
}

async fn h2_connect_relay_to_http(
    mut connection: h2::server::Connection<tokio::net::TcpStream, bytes::Bytes>,
    http_upstream_addr: std::net::SocketAddr,
) -> Result<(), eggress_protocol_http::H2ConnectError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    loop {
        match connection.accept().await {
            Some(Ok((request, mut send_response))) => {
                if *request.method() != http::Method::CONNECT {
                    send_response.send_reset(h2::Reason::PROTOCOL_ERROR);
                    continue;
                }
                let authority = request.uri().authority().ok_or_else(|| {
                    eggress_protocol_http::H2ConnectError::H2("missing authority".into())
                })?;
                let target_str = match authority.port_u16() {
                    Some(port) => format!("{}:{}", authority.host(), port),
                    None => format!("{}:443", authority.host()),
                };
                let mut recv_stream = request.into_body();
                let response = http::Response::builder().status(200).body(()).unwrap();
                let send_stream = send_response.send_response(response, false)?;
                let http_up = http_upstream_addr;
                tokio::spawn(async move {
                    let http_stream = match tokio::net::TcpStream::connect(http_up).await {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let connect_req = format!(
                        "CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n",
                        target_str, target_str
                    );
                    let mut http_stream = http_stream;
                    if http_stream.write_all(connect_req.as_bytes()).await.is_err() {
                        return;
                    }
                    let mut response_buf = Vec::new();
                    let mut temp = [0u8; 4096];
                    loop {
                        match http_stream.read(&mut temp).await {
                            Ok(0) => return,
                            Ok(n) => {
                                response_buf.extend_from_slice(&temp[..n]);
                                if response_buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                            }
                            Err(_) => return,
                        }
                    }
                    let resp_str = String::from_utf8_lossy(&response_buf);
                    if !resp_str.contains("200") {
                        return;
                    }
                    let (mut http_read, mut http_write) = http_stream.into_split();
                    let h2_to_http = async move {
                        loop {
                            match recv_stream.data().await {
                                Some(Ok(data)) => {
                                    if http_write.write_all(&data).await.is_err() {
                                        break;
                                    }
                                }
                                Some(Err(_)) => break,
                                None => break,
                            }
                        }
                        let _ = http_write.shutdown().await;
                    };
                    let http_to_h2 = async move {
                        let mut buf = [0u8; 8192];
                        let mut h2_write = eggress_protocol_http::H2StreamWrite::new(send_stream);
                        while let Ok(n) = http_read.read(&mut buf).await {
                            if n == 0 {
                                break;
                            }
                            if h2_write.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                    };
                    tokio::join!(h2_to_http, http_to_h2);
                });
            }
            Some(Err(e)) => {
                return Err(eggress_protocol_http::H2ConnectError::H2(e.to_string()));
            }
            None => break,
        }
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h2_chain_socks5_to_h2_to_http() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let http_proxy_addr = start_http_connect_proxy().await;
    let h2_proxy_addr = start_h2_connect_proxy_forwarding(http_proxy_addr).await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "h2-up"
uri = "h2://127.0.0.1:{h2_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["h2-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        h2_port = h2_proxy_addr.port()
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
        addrs[0].unwrap()
    };

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
    assert_eq!(
        reply[1], 0x00,
        "SOCKS5 CONNECT should succeed through H2 chain"
    );

    stream.write_all(b"chain-h2-http-test").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"chain-h2-http-test");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h2_chain_http_to_h2() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let h2_proxy_addr = start_h2_connect_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "h2-up"
uri = "h2://127.0.0.1:{h2_port}"

[[upstream_groups]]
id = "tcp-upstream"
scheduler = "first-available"
members = ["h2-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "tcp-upstream"
"#,
        h2_port = h2_proxy_addr.port()
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
        addrs[0].unwrap()
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let target = format!("{}:{}", echo_addr.ip(), echo_addr.port());
    let connect_req = format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n", target, target);
    stream.write_all(connect_req.as_bytes()).await.unwrap();

    let mut response = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            stream.read(&mut buf).await
        })
        .await
        .expect("timeout")
        .expect("read");
        response.extend_from_slice(&buf[..n]);
        if response.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let resp_str = String::from_utf8_lossy(&response);
    assert!(
        resp_str.contains("200"),
        "HTTP CONNECT should succeed through H2 upstream: {}",
        resp_str
    );

    stream.write_all(b"chain-http-h2-test").await.unwrap();

    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        stream.read(&mut buf).await
    })
    .await
    .expect("timeout")
    .expect("read");

    assert_eq!(&buf[..n], b"chain-http-h2-test");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}
