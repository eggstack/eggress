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
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timeout waiting for readiness");
}

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

async fn start_socks5_proxy() -> std::net::SocketAddr {
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

async fn start_http_proxy() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => continue,
            };
            tokio::spawn(async move {
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

                let target = match tokio::net::TcpStream::connect(&target_addr).await {
                    Ok(t) => t,
                    Err(_) => {
                        let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                        return;
                    }
                };

                if stream
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
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

async fn start_socks4_proxy() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => continue,
            };
            tokio::spawn(async move {
                let mut header = [0u8; 8];
                if stream.read_exact(&mut header).await.is_err() {
                    return;
                }
                let version = header[0];
                let cmd = header[1];
                if version != 0x04 || cmd != 0x01 {
                    return;
                }
                let port = u16::from_be_bytes([header[2], header[3]]);
                let ip = [header[4], header[5], header[6], header[7]];

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

                let target_addr = format!("{}.{}.{}.{}:{}", ip[0], ip[1], ip[2], ip[3], port);

                let target = match tokio::net::TcpStream::connect(&target_addr).await {
                    Ok(t) => t,
                    Err(_) => {
                        let _ = stream.write_all(&[0x00, 0x5B, 0, 0, 0, 0, 0, 0]).await;
                        return;
                    }
                };

                if stream
                    .write_all(&[0x00, 0x5A, 0, 0, 0, 0, 0, 0])
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

async fn socks5_connect(stream: &mut tokio::net::TcpStream, target: std::net::SocketAddr) -> bool {
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    if resp != [0x05, 0x00] {
        return false;
    }

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
    reply[1] == 0x00
}

// ---------------------------------------------------------------------------
// 1. socks5_to_http_chain: SOCKS5 inbound -> HTTP upstream -> echo target
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn socks5_to_http_chain() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let http_proxy_addr = start_http_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "http-up"
uri = "http://127.0.0.1:{http_port}"

[[upstream_groups]]
id = "chain"
scheduler = "first-available"
members = ["http-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "chain"
"#,
        http_port = http_proxy_addr.port()
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

    let ok = socks5_connect(&mut stream, echo_addr).await;
    assert!(ok, "SOCKS5 CONNECT should succeed via HTTP upstream");

    stream.write_all(b"socks5-http-hello").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf))
        .await
        .expect("timeout")
        .expect("read");
    assert_eq!(&buf[..n], b"socks5-http-hello");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

// ---------------------------------------------------------------------------
// 2. http_to_socks5_chain: HTTP CONNECT inbound -> SOCKS5 upstream -> echo target
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_to_socks5_chain() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let socks5_addr = start_socks5_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "socks-up"
uri = "socks5://127.0.0.1:{socks5_port}"

[[upstream_groups]]
id = "chain"
scheduler = "first-available"
members = ["socks-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "chain"
"#,
        socks5_port = socks5_addr.port()
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

    // HTTP CONNECT handshake
    let connect_req = format!(
        "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n\r\n",
        echo_addr.ip(),
        echo_addr.port(),
        echo_addr.ip(),
        echo_addr.port()
    );
    stream.write_all(connect_req.as_bytes()).await.unwrap();

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
        "HTTP CONNECT should succeed, got: {response_str}"
    );

    stream.write_all(b"http-socks5-hello").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf))
        .await
        .expect("timeout")
        .expect("read");
    assert_eq!(&buf[..n], b"http-socks5-hello");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

// ---------------------------------------------------------------------------
// 3. socks4_to_socks5_chain: SOCKS4 inbound -> SOCKS5 upstream -> echo target
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn socks4_to_socks5_chain() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let socks5_addr = start_socks5_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks4"]

[[upstreams]]
id = "socks-up"
uri = "socks5://127.0.0.1:{socks5_port}"

[[upstream_groups]]
id = "chain"
scheduler = "first-available"
members = ["socks-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "chain"
"#,
        socks5_port = socks5_addr.port()
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

    // SOCKS4 CONNECT request
    let ip_octets = match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => ip.octets(),
        _ => panic!("expected IPv4"),
    };
    stream.write_all(&[0x04, 0x01]).await.unwrap();
    stream
        .write_all(&echo_addr.port().to_be_bytes())
        .await
        .unwrap();
    stream.write_all(&ip_octets).await.unwrap();
    stream.write_all(b"test\x00").await.unwrap();

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(
        reply[1], 0x5A,
        "SOCKS4 CONNECT should succeed via SOCKS5 upstream"
    );

    stream.write_all(b"socks4-socks5-hello").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf))
        .await
        .expect("timeout")
        .expect("read");
    assert_eq!(&buf[..n], b"socks4-socks5-hello");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

// ---------------------------------------------------------------------------
// 4. socks5_to_socks4_chain: SOCKS5 inbound -> SOCKS4 upstream -> echo target
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn socks5_to_socks4_chain() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let socks4_addr = start_socks4_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "socks4-up"
uri = "socks4://127.0.0.1:{socks4_port}"

[[upstream_groups]]
id = "chain"
scheduler = "first-available"
members = ["socks4-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "chain"
"#,
        socks4_port = socks4_addr.port()
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

    let ok = socks5_connect(&mut stream, echo_addr).await;
    assert!(ok, "SOCKS5 CONNECT should succeed via SOCKS4 upstream");

    stream.write_all(b"socks5-socks4-hello").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf))
        .await
        .expect("timeout")
        .expect("read");
    assert_eq!(&buf[..n], b"socks5-socks4-hello");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

// ---------------------------------------------------------------------------
// 5. socks5_to_trojan_chain: SOCKS5 inbound -> Trojan upstream -> echo target
//
// Uses a self-signed cert and an insecure TLS client config (test-only).
// The ServiceSupervisor is configured with `with_tls_client_config` to
// inject an insecure verifier so the self-signed cert is accepted.
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn socks5_to_trojan_chain() {
    use std::sync::Arc;

    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let expected_password = "test-trojan-password";

    // Generate a self-signed cert for the Trojan server
    let subject_alt_names = vec!["localhost".to_string()];
    let cert_params = rcgen::CertificateParams::new(subject_alt_names).expect("valid params");
    let cert_key = rcgen::KeyPair::generate().expect("key gen");
    let cert = cert_params
        .self_signed(&cert_key)
        .expect("self-signed cert");
    let cert_der = cert.der().clone();
    let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert_key.serialize_der());

    // Build TLS server config for the Trojan server
    let server_tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der.into())
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_tls_config));

    // Start a Trojan server that accepts TLS, parses the Trojan header,
    // and forwards to the target (the echo server).
    let trojan_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let trojan_addr = trojan_listener.local_addr().unwrap();
    let expected_hash = eggress_protocol_trojan::hash::password_hash(expected_password);

    tokio::spawn(async move {
        loop {
            let (stream, _) = match trojan_listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let acceptor = acceptor.clone();
            let expected_hash = expected_hash.clone();
            let echo_addr = echo_addr;
            tokio::spawn(async move {
                let tls_stream = match acceptor.accept(stream).await {
                    Ok(s) => s,
                    Err(_) => return,
                };

                // Read the Trojan header: hash(56) + CRLF + CMD(1) + ATYP + addr + port(2) + CRLF
                let mut buf = vec![0u8; 4096];
                let mut reader = tls_stream;
                let n = match reader.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                buf.truncate(n);

                // Verify the password hash
                if buf.len() < 68 {
                    return;
                }
                let received_hash = match std::str::from_utf8(&buf[..56]) {
                    Ok(h) => h,
                    Err(_) => return,
                };
                if received_hash != expected_hash {
                    return; // Wrong password — drop connection (Trojan behavior)
                }
                if &buf[56..58] != b"\r\n" {
                    return;
                }

                // Parse target address from the Trojan header
                let cmd = buf[58];
                if cmd != 0x01 {
                    return; // Only CONNECT supported
                }
                let atyp = buf[59];
                let (_target_ip, _target_port) = match atyp {
                    0x01 => {
                        // IPv4
                        if buf.len() < 66 {
                            return;
                        }
                        let ip = std::net::Ipv4Addr::new(buf[60], buf[61], buf[62], buf[63]);
                        let port = u16::from_be_bytes([buf[64], buf[65]]);
                        (std::net::IpAddr::V4(ip), port)
                    }
                    0x03 => {
                        // Domain
                        let len = buf[60] as usize;
                        if buf.len() < 61 + len + 2 {
                            return;
                        }
                        let _domain = match std::str::from_utf8(&buf[61..61 + len]) {
                            Ok(d) => d.to_string(),
                            Err(_) => return,
                        };
                        let port = u16::from_be_bytes([buf[61 + len], buf[61 + len + 1]]);
                        (std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), port)
                    }
                    _ => return,
                };

                // For this test, we connect directly to the echo server
                let target_addr = format!("{}:{}", echo_addr.ip(), echo_addr.port());
                let target = match tokio::net::TcpStream::connect(&target_addr).await {
                    Ok(t) => t,
                    Err(_) => return,
                };

                let (mut cr, mut cw) = tokio::io::split(reader);
                let (mut tr, mut tw) = tokio::io::split(target);
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

    // Build an insecure TLS client config (test-only — accepts any cert)
    let insecure_tls_config = eggress_transport_tls::TlsClientConfigBuilder::new()
        .with_insecure()
        .build()
        .unwrap();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "trojan-up"
uri = "trojan://x:{password}@127.0.0.1:{trojan_port}"

[[upstream_groups]]
id = "chain"
scheduler = "first-available"
members = ["trojan-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "chain"
"#,
        password = expected_password,
        trojan_port = trojan_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path)
        .unwrap()
        .with_tls_client_config(insecure_tls_config);
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

    let ok = socks5_connect(&mut stream, echo_addr).await;
    assert!(ok, "SOCKS5 CONNECT should succeed via Trojan upstream");

    stream.write_all(b"socks5-trojan-hello").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf))
        .await
        .expect("timeout")
        .expect("read");
    assert_eq!(&buf[..n], b"socks5-trojan-hello");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

// ---------------------------------------------------------------------------
// 6. three_hop_chain: SOCKS5 -> HTTP -> SOCKS5 -> echo target
//
//    eggress (socks5) -> http_proxy (hop1) -> socks5_proxy (hop2) -> echo
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn three_hop_chain() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;

    // Hop 2: SOCKS5 proxy that connects directly to the echo server
    let hop2_addr = start_socks5_proxy().await;

    // Hop 1: HTTP proxy that chains to hop2 via the multi-hop URI
    let hop1_addr = start_http_proxy().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "three-hop"
uri = "http://127.0.0.1:{http_port}__socks5://127.0.0.1:{socks5_port}"

[[upstream_groups]]
id = "chain"
scheduler = "first-available"
members = ["three-hop"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "chain"
"#,
        http_port = hop1_addr.port(),
        socks5_port = hop2_addr.port()
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

    let ok = socks5_connect(&mut stream, echo_addr).await;
    assert!(ok, "SOCKS5 CONNECT should succeed through 3-hop chain");

    stream.write_all(b"three-hop-hello").await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf))
        .await
        .expect("timeout")
        .expect("read");
    assert_eq!(&buf[..n], b"three-hop-hello");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

// ---------------------------------------------------------------------------
// 7. chain_metrics_lease_cleanup: leases return to zero after disconnect
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chain_metrics_lease_cleanup() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let upstream_addr = start_socks5_proxy().await;

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
id = "chain"
scheduler = "first-available"
members = ["socks-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "chain"
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

    // Verify leases start at zero
    let snap = state.snapshot.load();
    let upstream_rt = snap.upstreams.get("socks-up").unwrap();
    assert_eq!(
        upstream_rt.active.load(Ordering::Relaxed),
        0,
        "active should start at 0"
    );
    assert_eq!(
        upstream_rt.in_flight.load(Ordering::Relaxed),
        0,
        "in_flight should start at 0"
    );
    drop(snap);

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    // Connect through the chain
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let ok = socks5_connect(&mut stream, echo_addr).await;
    assert!(ok, "CONNECT should succeed");

    // Send data to ensure relay is established
    stream.write_all(b"lease-check").await.unwrap();
    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf))
        .await
        .expect("timeout")
        .expect("read");
    assert_eq!(&buf[..n], b"lease-check");

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

    // Poll until lease counters return to zero
    wait_for(
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
    let result = tokio::time::timeout(Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

// ---------------------------------------------------------------------------
// 8. unsupported_chain_combo_rejects: invalid chain combos rejected at config
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsupported_chain_combo_rejects_http_upstream_udp_listener() {
    install_crypto();
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
async fn unsupported_chain_combo_rejects_socks4_upstream_udp_listener() {
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
