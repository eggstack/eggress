//! Trojan interoperability test scaffolding.
//!
//! These tests verify that eggress can interoperate with external Trojan
//! implementations (`trojan-go`, `trojan-rust`, or other servers implementing
//! the Trojan wire protocol over TLS).
//!
//! Trojan is TCP-only and requires TLS. Tests use self-signed certificates
//! for the external server and connect eggress as a Trojan upstream.
//!
//! # Running
//!
//! All tests are `#[ignore]` and gated behind `EGRESS_REQUIRE_TROJAN_INTEROP=1`.
//!
//! ```bash
//! EGRESS_REQUIRE_TROJAN_INTEROP=1 cargo test -p eggress-cli --test interoperability_trojan -- --ignored
//! ```
//!
//! # Environment Variables
//!
//! - `EGRESS_REQUIRE_TROJAN_INTEROP=1` (required): Enable these tests
//! - `EGRESS_TROJAN_BIN` (optional): Path to `trojan-go` binary (default: `trojan-go`)

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ===== Prerequisite Checks =====

fn require_trojan_interop() {
    if std::env::var("EGRESS_REQUIRE_TROJAN_INTEROP").is_err() {
        panic!("EGRESS_REQUIRE_TROJAN_INTEROP not set");
    }
}

fn trojan_bin() -> String {
    std::env::var("EGRESS_TROJAN_BIN").unwrap_or_else(|_| "trojan-go".to_string())
}

fn trojan_available() -> bool {
    std::process::Command::new(trojan_bin())
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn skip_if_unavailable() {
    require_trojan_interop();
    if !trojan_available() {
        eprintln!("skipping: trojan-go binary not available on PATH");
        panic!("trojan-go not available");
    }
}

// ===== Process Guards =====

struct ProcessGuard {
    child: Option<std::process::Child>,
}

impl ProcessGuard {
    fn new(child: std::process::Child) -> Self {
        Self { child: Some(child) }
    }

    #[allow(dead_code)]
    fn kill(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// ===== Helpers =====

/// Start a TCP echo server that echoes received data back to the sender.
async fn start_tcp_echo() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let jh = tokio::spawn(async move {
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
    (addr, jh)
}

/// Wait until a TCP port is accepting connections.
async fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Start an external trojan-go server process.
///
/// Returns the listening address and a process guard that kills the server on drop.
async fn start_external_trojan_server(password: &str) -> (std::net::SocketAddr, ProcessGuard) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let (cert_pem, key_pem) = self_signed_cert();

    // trojan-go reads cert/key from file paths, so write temp files
    let cert_file = tempfile::NamedTempFile::new().expect("create cert tempfile");
    let key_file = tempfile::NamedTempFile::new().expect("create key tempfile");
    std::fs::write(cert_file.path(), &cert_pem).expect("write cert");
    std::fs::write(key_file.path(), &key_pem).expect("write key");

    let config_value = serde_json::json!({
        "run_type": "server",
        "local_addr": "127.0.0.1",
        "local_port": port,
        "remote_addr": "127.0.0.1",
        "remote_port": 1,
        "password": [password],
        "log_level": "none",
        "ssl": {
            "cert": cert_file.path().to_str().unwrap(),
            "key": key_file.path().to_str().unwrap(),
        }
    });

    let mut config_file = tempfile::NamedTempFile::new().expect("create config tempfile");
    std::io::Write::write_all(&mut config_file, config_value.to_string().as_bytes())
        .expect("write config");
    std::io::Write::flush(&mut config_file).expect("flush config");
    let config_path = config_file.path().to_str().unwrap().to_string();

    // Prevent temp files from being deleted before trojan-go reads them
    let _cert_guard = cert_file;
    let _key_guard = key_file;
    std::mem::forget(config_file);

    let child = std::process::Command::new(trojan_bin())
        .arg("-config")
        .arg(&config_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start trojan-go");

    let addr = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), port);
    let guard = ProcessGuard::new(child);

    if !wait_for_port(port, Duration::from_secs(5)).await {
        panic!("trojan-go failed to start on port {port}");
    }

    (addr, guard)
}

/// Generate a self-signed certificate and key (PEM) for the external server.
fn self_signed_cert() -> (String, String) {
    let cert_params =
        rcgen::CertificateParams::new(vec!["localhost".to_string()]).expect("valid params");
    let key_pair = rcgen::KeyPair::generate().expect("key gen");
    let cert_der = cert_params
        .self_signed(&key_pair)
        .expect("self-signed cert");
    (cert_der.pem(), key_pair.serialize_pem())
}

/// Start an eggress server from a TOML config string, running on a blocking thread.
async fn start_eggress_from_toml_running(
    config_str: &str,
) -> (
    std::net::SocketAddr,
    tokio_util::sync::CancellationToken,
    tokio::task::JoinHandle<()>,
) {
    use std::io::Write;
    use std::sync::atomic::Ordering;

    let mut f = tempfile::NamedTempFile::new().expect("create tempfile");
    f.write_all(config_str.as_bytes()).expect("write config");
    f.flush().expect("flush config");
    let path = f.path().to_str().unwrap().to_string();
    std::mem::forget(f);
    let mut sup =
        eggress_runtime::ServiceSupervisor::start(&path).expect("start eggress from TOML");
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || {
        let _ = sup.run();
    });

    for _ in 0..100 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0].unwrap()
    };

    (listener_addr, token, jh)
}

/// Send data through a SOCKS5 proxy and return the echoed payload.
async fn send_through_socks5(
    proxy_addr: std::net::SocketAddr,
    target_host: &str,
    target_port: u16,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;
    let (mut reader, mut writer) = tokio::io::split(stream);

    // SOCKS5 method negotiation: no auth
    writer
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| format!("write method negotiation: {e}"))?;
    let mut resp = [0u8; 2];
    reader
        .read_exact(&mut resp)
        .await
        .map_err(|e| format!("read method selection: {e}"))?;
    if resp != [0x05, 0x00] {
        return Err(format!("unexpected method selection: {resp:?}"));
    }

    // SOCKS5 CONNECT request
    let mut req = vec![0x05, 0x01, 0x00]; // VER, CMD=CONNECT, RSV
    req.push(0x01); // ATYP IPv4
    let ip: std::net::Ipv4Addr = target_host
        .parse()
        .map_err(|_| format!("invalid IPv4 target: {target_host}"))?;
    req.extend_from_slice(&ip.octets());
    req.extend_from_slice(&target_port.to_be_bytes());
    writer
        .write_all(&req)
        .await
        .map_err(|e| format!("write CONNECT request: {e}"))?;

    let mut reply = [0u8; 10];
    reader
        .read_exact(&mut reply)
        .await
        .map_err(|e| format!("read CONNECT reply: {e}"))?;
    if reply[1] != 0x00 {
        return Err(format!(
            "SOCKS5 CONNECT failed: reply code 0x{:02x}",
            reply[1]
        ));
    }

    // Send payload
    writer
        .write_all(payload)
        .await
        .map_err(|e| format!("write payload: {e}"))?;
    writer
        .shutdown()
        .await
        .map_err(|e| format!("shutdown write: {e}"))?;

    // Read response with timeout
    let mut buf = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let mut chunk = [0u8; 4096];
        match tokio::time::timeout(Duration::from_millis(500), reader.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => buf.extend_from_slice(&chunk[..n]),
            Ok(Err(e)) => return Err(format!("read response: {e}")),
            Err(_) => continue,
        }
    }
    Ok(buf)
}

// ===== TCP Interop Tests =====

/// TCP interop: eggress Trojan upstream -> external trojan-go -> TCP echo.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_TROJAN_INTEROP=1 and trojan-go"]
async fn interop_trojan_tcp_eggress_to_external_server() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let password = "interop-test-password";

    let (trojan_addr, mut _trojan_guard) = start_external_trojan_server(password).await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "trojan-up"
uri = "trojan://x:{password}@127.0.0.1:{port}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["trojan-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        password = password,
        port = trojan_addr.port()
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let payload = send_through_socks5(
        proxy_addr,
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        b"trojan interop tcp test",
    )
    .await
    .expect("TCP interop failed");

    assert_eq!(payload, b"trojan interop tcp test");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

/// TCP interop: large payload (multiple TLS records) through Trojan upstream.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_TROJAN_INTEROP=1 and trojan-go"]
async fn interop_trojan_tcp_large_payload() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let password = "interop-test-large";

    let (trojan_addr, mut _trojan_guard) = start_external_trojan_server(password).await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "trojan-up"
uri = "trojan://x:{password}@127.0.0.1:{port}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["trojan-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        password = password,
        port = trojan_addr.port()
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let payload = vec![0x5Bu8; 256 * 1024];
    let sent = payload.clone();
    let received = send_through_socks5(
        proxy_addr,
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        &payload,
    )
    .await
    .expect("TCP interop failed with large payload");

    assert_eq!(received.len(), sent.len(), "byte count mismatch");
    assert_eq!(received, sent, "large payload bytes mismatch");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

/// TCP interop: wrong password should cause connection failure.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_TROJAN_INTEROP=1 and trojan-go"]
async fn interop_trojan_tcp_wrong_password_fails() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let correct_password = "correct-password";
    let wrong_password = "wrong-password";

    let (trojan_addr, mut _trojan_guard) = start_external_trojan_server(correct_password).await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "trojan-up"
uri = "trojan://x:{wrong_password}@127.0.0.1:{port}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["trojan-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        wrong_password = wrong_password,
        port = trojan_addr.port()
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Attempt TCP echo - should fail due to wrong password
    let result = send_through_socks5(
        proxy_addr,
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        b"should fail",
    )
    .await;

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();

    assert!(
        result.is_err(),
        "wrong password should cause connection failure, but got: {result:?}"
    );
}

/// TCP interop: eggress Trojan server + external client sending Trojan handshake.
///
/// This verifies the inbound listener side of the Trojan protocol by having
/// a raw TCP client send a properly formed Trojan handshake through TLS.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_TROJAN_INTEROP=1"]
async fn interop_trojan_tcp_external_client_to_eggress() {
    require_trojan_interop();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let password = "inbound-test-password";

    let cert_params =
        rcgen::CertificateParams::new(vec!["localhost".to_string()]).expect("valid params");
    let key_pair = rcgen::KeyPair::generate().expect("key gen");
    let cert_der = cert_params
        .self_signed(&key_pair)
        .expect("self-signed cert");
    let cert_pem = cert_der.pem();
    let key_pem = key_pair.serialize_pem();

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

[[rules]]
id = "route-all"
direct = true
"#,
        cert = cert_pem.replace('\n', "\\n"),
        key = key_pem.replace('\n', "\\n"),
        password = password
    );

    let (trojan_addr, cancel, trojan_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Build a raw Trojan handshake and send it through TLS
    use eggress_protocol_trojan::hash::password_hash;
    let hash = password_hash(password);
    let mut handshake = Vec::new();
    handshake.extend_from_slice(hash.as_bytes());
    handshake.extend_from_slice(b"\r\n");
    handshake.push(0x01); // CONNECT
    handshake.push(0x03); // ATYP domain
    let target = echo_addr.ip().to_string();
    handshake.push(target.len() as u8);
    handshake.extend_from_slice(target.as_bytes());
    handshake.extend_from_slice(&echo_addr.port().to_be_bytes());
    handshake.extend_from_slice(b"\r\n");

    // Connect via TLS with insecure (self-signed cert)
    let client_config = eggress_transport_tls::TlsClientConfigBuilder::new()
        .with_insecure()
        .build()
        .unwrap();
    let connector = tokio_rustls::TlsConnector::from(client_config);

    let tcp_stream = tokio::net::TcpStream::connect(trojan_addr)
        .await
        .expect("connect to eggress trojan listener");
    let domain = rustls::pki_types::ServerName::try_from("localhost".to_string()).unwrap();
    let mut tls_stream = connector.connect(domain, tcp_stream).await.unwrap();

    tls_stream.write_all(&handshake).await.unwrap();
    tls_stream.flush().await.unwrap();

    // Send data and read echo
    tls_stream.write_all(b"external-client-test").await.unwrap();
    tls_stream.flush().await.unwrap();

    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(5), tls_stream.read(&mut buf))
        .await
        .expect("timeout reading echo response");

    match n {
        Ok(0) => panic!("got 0 bytes from echo"),
        Ok(bytes) => {
            assert_eq!(
                &buf[..bytes],
                b"external-client-test",
                "echo mismatch from eggress Trojan server"
            );
        }
        Err(_) => {}
    }

    drop(tls_stream);
    cancel.cancel();
    let _ = trojan_jh.await;
    echo_jh.abort();
}
