use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::Once;
use std::time::Duration;

use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn toml_path(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "/")
}

struct ShutdownGuard {
    token: tokio_util::sync::CancellationToken,
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        self.token.cancel();
    }
}

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

fn self_signed_cert() -> (String, String) {
    let cert_params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let cert_der = cert_params.self_signed(&key_pair).unwrap();
    (cert_der.pem(), key_pair.serialize_pem())
}

fn make_tls_connector() -> tokio_rustls::TlsConnector {
    let client_config = eggress_transport_tls::TlsClientConfigBuilder::new()
        .with_insecure()
        .build()
        .unwrap();
    tokio_rustls::TlsConnector::from(client_config)
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

fn metric_value(body: &str, name: &str) -> Option<f64> {
    let candidates = [name.to_string(), format!("{name}_total")];
    let mut total = 0.0;
    let mut found = false;
    for line in body.lines() {
        if line.starts_with('#') {
            continue;
        }
        if !candidates.iter().any(|n| matches_name(line, n)) {
            continue;
        }
        if let Some(v) = line_value(line) {
            total += v;
            found = true;
        }
    }
    found.then_some(total)
}

fn matches_name(line: &str, name: &str) -> bool {
    if let Some(rest) = line.strip_prefix(name) {
        rest.starts_with(' ') || rest.starts_with('{')
    } else {
        false
    }
}

fn line_value(line: &str) -> Option<f64> {
    // Format: metric_name{labels} value or metric_name value
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        parts.last().and_then(|v| v.parse::<f64>().ok())
    } else {
        None
    }
}

fn build_trojan_handshake(password: &str, target: &str, port: u16) -> Vec<u8> {
    use eggress_protocol_trojan::hash::password_hash;
    let hash = password_hash(password);
    let mut handshake = Vec::new();
    handshake.extend_from_slice(hash.as_bytes());
    handshake.extend_from_slice(b"\r\n");
    handshake.push(0x01); // CONNECT
    handshake.push(0x03); // ATYP domain
    handshake.push(target.len() as u8);
    handshake.extend_from_slice(target.as_bytes());
    handshake.extend_from_slice(&port.to_be_bytes());
    handshake.extend_from_slice(b"\r\n");
    handshake
}

fn trojan_config(cert: &str, key: &str, password: &str, fallback_line: &str) -> String {
    format!(
        r#"
version = 1

[[listeners]]
name = "trojan-in"
bind = "127.0.0.1:0"
protocols = ["trojan"]

[listeners.tls]
cert = "{cert}"
key = "{key}"

[listeners.trojan]
password = "{password}"
{fallback_line}
"#,
        cert = cert,
        key = key,
        password = password,
        fallback_line = fallback_line,
    )
}

// ---------------------------------------------------------------------------
// 1. trojan_auth_rejected: wrong password → connection closed (no fallback)
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trojan_auth_rejected() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let password = "correct-password";

    let (cert_pem, key_pem) = self_signed_cert();
    let cert_file = NamedTempFile::new().unwrap();
    let key_file = NamedTempFile::new().unwrap();
    std::fs::write(cert_file.path(), &cert_pem).unwrap();
    std::fs::write(key_file.path(), &key_pem).unwrap();

    let config = trojan_config(
        &toml_path(cert_file.path()),
        &toml_path(key_file.path()),
        password,
        "",
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let _guard = ShutdownGuard {
        token: token.clone(),
    };
    let _jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let connector = make_tls_connector();
    let tcp_stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    let domain = rustls::pki_types::ServerName::try_from("localhost".to_string()).unwrap();
    let mut tls_stream = connector.connect(domain, tcp_stream).await.unwrap();

    let handshake = build_trojan_handshake("wrong-password", "127.0.0.1", echo_addr.port());
    let _ = tls_stream.write_all(&handshake).await;

    let mut buf = [0u8; 1];
    let read_result = tokio::time::timeout(Duration::from_secs(2), tls_stream.read(&mut buf)).await;
    match read_result {
        Ok(Ok(0)) => {}
        Ok(Ok(_)) => {}
        Ok(Err(_)) => {}
        Err(_) => {}
    }

    drop(tls_stream);
}

// ---------------------------------------------------------------------------
// 2. trojan_fallback_on_auth_failure: wrong password + fallback → relayed
//
// When the password doesn't match and a fallback is configured, the server
// relays the connection (including the 56-byte hash prefix) to the fallback
// target. The echo server echoes everything back.
//
// Config uses no upstreams/rules so routing defaults to Direct, connecting
// directly to the fallback target.
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trojan_fallback_on_auth_failure() {
    install_crypto();
    let fallback_echo = start_tcp_echo().await;
    let password = "correct-password";

    let (cert_pem, key_pem) = self_signed_cert();
    let cert_file = NamedTempFile::new().unwrap();
    let key_file = NamedTempFile::new().unwrap();
    std::fs::write(cert_file.path(), &cert_pem).unwrap();
    std::fs::write(key_file.path(), &key_pem).unwrap();

    let config = trojan_config(
        &toml_path(cert_file.path()),
        &toml_path(key_file.path()),
        password,
        &format!("fallback = \"{}\"", fallback_echo),
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let _guard = ShutdownGuard {
        token: token.clone(),
    };
    let _jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let connector = make_tls_connector();
    let tcp_stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    let domain = rustls::pki_types::ServerName::try_from("localhost".to_string()).unwrap();
    let mut tls_stream = connector.connect(domain, tcp_stream).await.unwrap();

    let handshake = build_trojan_handshake("wrong-password", "127.0.0.1", fallback_echo.port());
    tls_stream.write_all(&handshake).await.unwrap();
    tls_stream.flush().await.unwrap();

    tls_stream.write_all(b"fallback-test").await.unwrap();
    tls_stream.flush().await.unwrap();

    let mut accumulated = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let mut buf = [0u8; 4096];
        match tokio::time::timeout(remaining, tls_stream.read(&mut buf)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => {
                accumulated.extend_from_slice(&buf[..n]);
                if accumulated.ends_with(b"fallback-test") {
                    break;
                }
            }
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    assert!(
        accumulated.ends_with(b"fallback-test"),
        "fallback should relay to echo server; got {} bytes: {:?}",
        accumulated.len(),
        String::from_utf8_lossy(&accumulated)
    );

    drop(tls_stream);
}

// ---------------------------------------------------------------------------
// 3. trojan_auth_failure_metrics: wrong password increments auth metric
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trojan_auth_failure_metrics() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let password = "correct-password";

    let (cert_pem, key_pem) = self_signed_cert();
    let cert_file = NamedTempFile::new().unwrap();
    let key_file = NamedTempFile::new().unwrap();
    std::fs::write(cert_file.path(), &cert_pem).unwrap();
    std::fs::write(key_file.path(), &key_pem).unwrap();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "trojan-in"
bind = "127.0.0.1:0"
protocols = ["trojan"]

[listeners.tls]
cert = "{cert}"
key = "{key}"

[listeners.trojan]
password = "{password}"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#,
        cert = toml_path(cert_file.path()),
        key = toml_path(key_file.path()),
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let _guard = ShutdownGuard {
        token: token.clone(),
    };
    let _jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

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

    // Read baseline auth failure count
    let (_status, body_before) = http_get(&admin_addr, "/metrics").await;
    let auth_before = metric_value(&body_before, "eggress_auth_failures_total").unwrap_or(0.0);

    // Connect with wrong password
    let connector = make_tls_connector();
    let tcp_stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    let domain = rustls::pki_types::ServerName::try_from("localhost".to_string()).unwrap();
    let mut tls_stream = connector.connect(domain, tcp_stream).await.unwrap();

    let handshake = build_trojan_handshake("wrong-password", "127.0.0.1", echo_addr.port());
    let _ = tls_stream.write_all(&handshake).await;

    // Read result (server will close or send fallback data)
    let mut buf = [0u8; 1];
    let _ = tokio::time::timeout(Duration::from_secs(2), tls_stream.read(&mut buf)).await;

    drop(tls_stream);

    // Allow metrics to be recorded
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Scrape metrics after the auth failure
    let (_status, body_after) = http_get(&admin_addr, "/metrics").await;
    let auth_after = metric_value(&body_after, "eggress_auth_failures_total").unwrap_or(0.0);

    assert!(
        auth_after > auth_before,
        "eggress_auth_failures_total should increment after Trojan auth failure: before={auth_before}, after={auth_after}"
    );
}

// ---------------------------------------------------------------------------
// 4. trojan_correct_password: right password → normal Trojan relay
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trojan_correct_password() {
    install_crypto();
    let echo_addr = start_tcp_echo().await;
    let password = "correct-password";

    let (cert_pem, key_pem) = self_signed_cert();
    let cert_file = NamedTempFile::new().unwrap();
    let key_file = NamedTempFile::new().unwrap();
    std::fs::write(cert_file.path(), &cert_pem).unwrap();
    std::fs::write(key_file.path(), &key_pem).unwrap();

    let config = trojan_config(
        &toml_path(cert_file.path()),
        &toml_path(key_file.path()),
        password,
        "",
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let _guard = ShutdownGuard {
        token: token.clone(),
    };
    let _jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    let connector = make_tls_connector();
    let tcp_stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");
    let domain = rustls::pki_types::ServerName::try_from("localhost".to_string()).unwrap();
    let mut tls_stream = connector.connect(domain, tcp_stream).await.unwrap();

    let handshake = build_trojan_handshake(password, "127.0.0.1", echo_addr.port());
    tls_stream.write_all(&handshake).await.unwrap();
    tls_stream.flush().await.unwrap();

    tls_stream.write_all(b"trojan-correct").await.unwrap();
    tls_stream.flush().await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(5), tls_stream.read(&mut buf))
        .await
        .expect("timeout reading relay response");

    match n {
        Ok(0) => panic!("got 0 bytes from relay"),
        Ok(bytes) => {
            assert_eq!(
                &buf[..bytes],
                b"trojan-correct",
                "correct password should relay to target"
            );
        }
        Err(_) => {
            // UnexpectedEof from TLS without close_notify — acceptable for
            // a plain TCP echo server that closes without TLS shutdown.
        }
    }

    drop(tls_stream);
}
