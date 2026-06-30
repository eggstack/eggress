//! Shadowsocks interoperability test scaffolding.
//!
//! These tests verify that eggress can interoperate with external Shadowsocks
//! implementations (`ssserver`/`sslocal` from shadowsocks-rust or shadowsocks-libev).
//!
//! Both TCP and UDP use standard SIP003 AEAD framing and are wire-compatible
//! with standard Shadowsocks implementations.
//!
//! # Running
//!
//! All tests are `#[ignore]` and gated behind `EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1`.
//!
//! ```bash
//! # Run all interop tests
//! EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored
//!
//! # Run only UDP tests
//! EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1 udp
//! ```
//!
//! # Environment Variables
//!
//! - `EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1` (required): Enable these tests
//! - `EGRESS_SSSERVER_BIN` (optional): Path to `ssserver` binary (default: `ssserver`)
//! - `EGRESS_SSLOCAL_BIN` (optional): Path to `sslocal` binary (default: `sslocal`)

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ===== Prerequisite Checks =====

fn require_shadowsocks_interop() {
    if std::env::var("EGRESS_REQUIRE_SHADOWSOCKS_INTEROP").is_err() {
        panic!("EGRESS_REQUIRE_SHADOWSOCKS_INTEROP not set");
    }
}

fn ssserver_bin() -> String {
    std::env::var("EGRESS_SSSERVER_BIN").unwrap_or_else(|_| "ssserver".to_string())
}

fn sslocal_bin() -> String {
    std::env::var("EGRESS_SSLOCAL_BIN").unwrap_or_else(|_| "sslocal".to_string())
}

fn ssserver_available() -> bool {
    std::process::Command::new(ssserver_bin())
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn sslocal_available() -> bool {
    std::process::Command::new(sslocal_bin())
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn skip_if_unavailable() {
    require_shadowsocks_interop();
    if !ssserver_available() || !sslocal_available() {
        eprintln!("skipping: ssserver or sslocal not available");
        panic!("ssserver or sslocal not available");
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

/// Start a UDP echo server that echoes received packets back to the sender.
async fn start_udp_echo() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    let jh = tokio::spawn(async move {
        let mut buf = [0u8; 65535];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let _ = socket.send_to(&buf[..n], peer).await;
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

/// Start an external ssserver process with the given password and method.
///
/// Returns the listening address and a process guard that kills the server on drop.
async fn start_external_ssserver(
    password: &str,
    method: &str,
) -> (std::net::SocketAddr, ProcessGuard) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let child = std::process::Command::new(ssserver_bin())
        .args([
            "-s",
            &format!("127.0.0.1:{port}"),
            "-m",
            method,
            "-k",
            password,
            "-U",
            "--tcp-no-delay",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start ssserver");

    let addr = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), port);
    let guard = ProcessGuard::new(child);

    if !wait_for_port(port, Duration::from_secs(5)).await {
        panic!("ssserver failed to start on port {port}");
    }

    (addr, guard)
}

/// Start an external sslocal process using a config file.
///
/// Returns the local SOCKS5 address and a process guard that kills the client on drop.
fn start_sslocal(
    server_addr: std::net::SocketAddr,
    password: &str,
    method: &str,
) -> (std::net::SocketAddr, ProcessGuard) {
    // Bind a TCP port for the SOCKS5 local listener
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let socks_port = listener.local_addr().unwrap().port();
    drop(listener);

    // Write a temp config file for sslocal
    let config = serde_json::json!({
        "server": "127.0.0.1",
        "server_port": server_addr.port(),
        "password": password,
        "method": method,
        "local_address": "127.0.0.1",
        "local_port": socks_port,
    });
    let mut config_file = tempfile::NamedTempFile::new().expect("create sslocal config tempfile");
    std::io::Write::write_all(&mut config_file, config.to_string().as_bytes())
        .expect("write sslocal config");
    std::io::Write::flush(&mut config_file).expect("flush sslocal config");
    let config_path = config_file.path().to_str().unwrap().to_string();
    std::mem::forget(config_file);

    let child = std::process::Command::new(sslocal_bin())
        .args(["-c", &config_path])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start sslocal");

    let addr = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), socks_port);
    let guard = ProcessGuard::new(child);

    (addr, guard)
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
        addrs[0]
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

    // Read response
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("read response: {e}"))?;
    Ok(buf)
}

/// Send a UDP packet through a SOCKS5 UDP ASSOCIATE relay.
async fn send_udp_through_socks5(
    proxy_addr: std::net::SocketAddr,
    target_addr: std::net::SocketAddr,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

    // UDP ASSOCIATE request (0.0.0.0:0 — client doesn't know relay addr yet)
    let mut udp_req = vec![0x05, 0x03, 0x00]; // VER, CMD=UDP_ASSOCIATE, RSV
    udp_req.push(0x01); // ATYP IPv4
    udp_req.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // 0.0.0.0
    udp_req.extend_from_slice(&[0x00, 0x00]); // port 0
    writer
        .write_all(&udp_req)
        .await
        .map_err(|e| format!("write UDP ASSOCIATE: {e}"))?;

    let mut udp_reply = [0u8; 22];
    let n = reader
        .read(&mut udp_reply)
        .await
        .map_err(|e| format!("read UDP ASSOCIATE reply: {e}"))?;
    if n < 10 || udp_reply[1] != 0x00 {
        return Err(format!("UDP ASSOCIATE failed: reply={:?}", &udp_reply[..n]));
    }

    // Parse relay address
    let relay_ip = match udp_reply[3] {
        0x01 => {
            let ip =
                std::net::Ipv4Addr::new(udp_reply[4], udp_reply[5], udp_reply[6], udp_reply[7]);
            std::net::IpAddr::V4(ip)
        }
        _ => return Err("unexpected address type in UDP ASSOCIATE reply".to_string()),
    };
    let relay_port = u16::from_be_bytes([udp_reply[8], udp_reply[9]]);
    let relay_addr = std::net::SocketAddr::new(relay_ip, relay_port);

    // Build SOCKS5 UDP packet: RSV(2) + FRAG(1) + ATYP(1) + ADDR(4) + PORT(2) + PAYLOAD
    let mut packet = vec![0x00, 0x00, 0x00]; // RSV + FRAG
    match target_addr.ip() {
        std::net::IpAddr::V4(ip) => {
            packet.push(0x01);
            packet.extend_from_slice(&ip.octets());
        }
        std::net::IpAddr::V6(ip) => {
            packet.push(0x04);
            packet.extend_from_slice(&ip.octets());
        }
    }
    packet.extend_from_slice(&target_addr.port().to_be_bytes());
    packet.extend_from_slice(payload);

    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind UDP: {e}"))?;
    udp_sock
        .send_to(&packet, relay_addr)
        .await
        .map_err(|e| format!("send UDP: {e}"))?;

    // Read response with timeout
    let mut buf = [0u8; 65535];
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), udp_sock.recv_from(&mut buf)).await {
            Ok(Ok((n, _))) => {
                // Parse SOCKS5 UDP response: skip RSV(2) + FRAG(1) + ATYP(1) + ADDR(4) + PORT(2)
                if n >= 10 {
                    return Ok(buf[10..n].to_vec());
                }
                return Ok(buf[..n].to_vec());
            }
            _ => continue,
        }
    }
    Err("UDP response timeout".to_string())
}

// ===== TCP Interop Tests =====

/// TCP interop: eggress Shadowsocks upstream → external ssserver → TCP echo.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver/sslocal"]
async fn interop_shadowsocks_tcp_eggress_to_external_server() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let password = "test-password-interop";
    let method = "aes-256-gcm";

    let (ss_addr, mut _ss_guard) = start_external_ssserver(password, method).await;

    let ss_uri = format!("ss://{method}:{password}@127.0.0.1:{}", ss_addr.port());
    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "{ss_uri}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["ss-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let payload = send_through_socks5(
        proxy_addr,
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        b"interop tcp test",
    )
    .await
    .expect("TCP interop failed");

    assert_eq!(payload, b"interop tcp test");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

/// TCP interop: external sslocal → eggress Shadowsocks server → TCP echo.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver/sslocal"]
async fn interop_shadowsocks_tcp_external_client_to_eggress() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let password = "test-password-reverse";
    let method = "aes-256-gcm";

    let ss_config = r#"
version = 1

[[listeners]]
name = "ss-in"
bind = "127.0.0.1:0"
protocols = ["shadowsocks"]

[listeners.shadowsocks]
method = "aes-256-gcm"
password = "test-password-reverse"

[[rules]]
id = "route-all"
direct = true
"#
    .to_string();

    let (ss_addr, cancel, ss_jh) = start_eggress_from_toml_running(&ss_config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (socks_addr, mut _sslocal_guard) = start_sslocal(ss_addr, password, method);

    if !wait_for_port(socks_addr.port(), Duration::from_secs(5)).await {
        cancel.cancel();
        let _ = ss_jh.await;
        echo_jh.abort();
        panic!("sslocal failed to start");
    }

    let payload = send_through_socks5(
        socks_addr,
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        b"interop tcp test reverse",
    )
    .await
    .expect("TCP interop (reverse) failed");

    assert_eq!(payload, b"interop tcp test reverse");

    cancel.cancel();
    let _ = ss_jh.await;
    echo_jh.abort();
}

// ===== UDP Interop Tests =====

/// UDP interop: eggress Shadowsocks UDP → external ssserver → UDP echo.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver"]
async fn interop_shadowsocks_udp_eggress_to_external_server() {
    skip_if_unavailable();

    let (udp_echo_addr, udp_jh) = start_udp_echo().await;
    let password = "test-password-udp";
    let method = "aes-256-gcm";
    let cipher: eggress_protocol_shadowsocks::CipherMethod =
        eggress_protocol_shadowsocks::CipherMethod::parse_method(method).unwrap();

    let (ss_addr, mut _ss_guard) = start_external_ssserver(password, method).await;

    let target = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), udp_echo_addr.port());
    let salt = vec![0x42u8; cipher.salt_size()];

    let encoded = eggress_protocol_shadowsocks::udp::encode_udp_packet(
        cipher,
        password.as_bytes(),
        &eggress_core::TargetAddr {
            host: eggress_core::TargetHost::Ip(target.ip()),
            port: target.port(),
        },
        b"interop udp test",
        &salt,
    )
    .expect("encode_udp_packet failed");

    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    udp_sock
        .send_to(&encoded, ss_addr)
        .await
        .expect("send failed");

    let mut buf = [0u8; 65535];
    let mut response = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), udp_sock.recv_from(&mut buf)).await {
            Ok(Ok((n, _))) => {
                response = Some(buf[..n].to_vec());
                break;
            }
            _ => continue,
        }
    }

    let data = response.expect("No UDP response received from external ssserver");
    let (_target, payload) =
        eggress_protocol_shadowsocks::udp::decode_udp_packet(cipher, password.as_bytes(), &data)
            .expect("UDP decode failed");

    assert_eq!(payload, b"interop udp test");

    _ss_guard.kill();
    udp_jh.abort();
}

/// UDP interop: verify UDP via full TOML-configured eggress stack.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver"]
async fn interop_shadowsocks_udp_via_toml_config() {
    skip_if_unavailable();

    let (udp_echo_addr, udp_jh) = start_udp_echo().await;
    let password = "test-password-toml";
    let method = "aes-256-gcm";

    let (ss_addr, mut _ss_guard) = start_external_ssserver(password, method).await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "ss://{method}:{password}@127.0.0.1:{}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["ss-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        ss_addr.port()
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let payload = send_udp_through_socks5(proxy_addr, udp_echo_addr, b"toml config udp test")
        .await
        .expect("UDP interop via TOML config failed");

    assert_eq!(payload, b"toml config udp test");

    cancel.cancel();
    let _ = proxy_jh.await;
    udp_jh.abort();
}

// ===== Wrong Password Tests =====

/// Wrong password should cause decryption failure in the external ssserver (TCP).
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver"]
async fn interop_shadowsocks_tcp_wrong_password_fails() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let correct_password = "correct-password";
    let wrong_password = "wrong-password";
    let method = "aes-256-gcm";

    // Start external ssserver with correct password
    let (ss_addr, mut _ss_guard) = start_external_ssserver(correct_password, method).await;

    // Start eggress with wrong password
    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "ss://{method}:{}@127.0.0.1:{}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["ss-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        wrong_password,
        ss_addr.port()
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Attempt TCP echo — should fail due to wrong password
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

/// Wrong password should cause decryption failure in the UDP path.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver"]
async fn interop_shadowsocks_udp_wrong_password_fails() {
    skip_if_unavailable();

    let (udp_echo_addr, udp_jh) = start_udp_echo().await;
    let correct_password = "correct-password-udp";
    let wrong_password = "wrong-password-udp";
    let method = "aes-256-gcm";
    let cipher: eggress_protocol_shadowsocks::CipherMethod =
        eggress_protocol_shadowsocks::CipherMethod::parse_method(method).unwrap();

    // Start external ssserver
    let (ss_addr, mut _ss_guard) = start_external_ssserver(correct_password, method).await;

    // Send a packet encrypted with the WRONG password
    let target = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), udp_echo_addr.port());
    let salt = vec![0x43u8; cipher.salt_size()];

    let encoded = eggress_protocol_shadowsocks::udp::encode_udp_packet(
        cipher,
        wrong_password.as_bytes(),
        &eggress_core::TargetAddr {
            host: eggress_core::TargetHost::Ip(target.ip()),
            port: target.port(),
        },
        b"should fail",
        &salt,
    )
    .expect("encode failed");

    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    udp_sock
        .send_to(&encoded, ss_addr)
        .await
        .expect("send failed");

    // Should not receive a response (server can't decrypt)
    let mut buf = [0u8; 65535];
    let result = tokio::time::timeout(Duration::from_secs(2), udp_sock.recv_from(&mut buf)).await;

    assert!(
        result.is_err(),
        "wrong password should not produce a response, but got data"
    );

    _ss_guard.kill();
    udp_jh.abort();
}

// ===== Method Coverage Tests =====

/// Test TCP interop with aes-128-gcm method.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver"]
async fn interop_shadowsocks_tcp_aes_128_gcm() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let password = "test-password-128";
    let method = "aes-128-gcm";

    let (ss_addr, mut _ss_guard) = start_external_ssserver(password, method).await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "ss://{method}:{password}@127.0.0.1:{}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["ss-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        ss_addr.port()
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let payload = send_through_socks5(
        proxy_addr,
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        b"aes-128-gcm test",
    )
    .await
    .expect("TCP interop failed with aes-128-gcm");

    assert_eq!(payload, b"aes-128-gcm test");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

/// Test TCP interop with chacha20-ietf-poly1305 method.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver"]
async fn interop_shadowsocks_tcp_chacha20_ietf_poly1305() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let password = "test-password-chacha";
    let method = "chacha20-ietf-poly1305";

    let (ss_addr, mut _ss_guard) = start_external_ssserver(password, method).await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "ss://{method}:{password}@127.0.0.1:{}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["ss-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        ss_addr.port()
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let payload = send_through_socks5(
        proxy_addr,
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        b"chacha20 test",
    )
    .await
    .expect("TCP interop failed with chacha20-ietf-poly1305");

    assert_eq!(payload, b"chacha20 test");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

/// Test UDP interop with aes-128-gcm method.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver"]
async fn interop_shadowsocks_udp_aes_128_gcm() {
    skip_if_unavailable();

    let (udp_echo_addr, udp_jh) = start_udp_echo().await;
    let password = "test-password-udp-128";
    let method = "aes-128-gcm";
    let cipher: eggress_protocol_shadowsocks::CipherMethod =
        eggress_protocol_shadowsocks::CipherMethod::parse_method(method).unwrap();

    let (ss_addr, mut _ss_guard) = start_external_ssserver(password, method).await;

    let target = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), udp_echo_addr.port());
    let salt = vec![0x44u8; cipher.salt_size()];

    let encoded = eggress_protocol_shadowsocks::udp::encode_udp_packet(
        cipher,
        password.as_bytes(),
        &eggress_core::TargetAddr {
            host: eggress_core::TargetHost::Ip(target.ip()),
            port: target.port(),
        },
        b"aes-128-gcm udp test",
        &salt,
    )
    .expect("encode failed");

    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    udp_sock
        .send_to(&encoded, ss_addr)
        .await
        .expect("send failed");

    let mut buf = [0u8; 65535];
    let mut response = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), udp_sock.recv_from(&mut buf)).await {
            Ok(Ok((n, _))) => {
                response = Some(buf[..n].to_vec());
                break;
            }
            _ => continue,
        }
    }

    match &response {
        Some(data) => {
            match eggress_protocol_shadowsocks::udp::decode_udp_packet(
                cipher,
                password.as_bytes(),
                data,
            ) {
                Ok((_target, payload)) => {
                    assert_eq!(payload, b"aes-128-gcm udp test", "UDP payload mismatch");
                }
                Err(e) => {
                    panic!("UDP decode failed: {e}");
                }
            }
        }
        None => {
            panic!("No UDP response received from external ssserver");
        }
    }

    _ss_guard.kill();
    udp_jh.abort();
}

/// Test UDP interop with chacha20-ietf-poly1305 method.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver"]
async fn interop_shadowsocks_udp_chacha20_ietf_poly1305() {
    skip_if_unavailable();

    let (udp_echo_addr, udp_jh) = start_udp_echo().await;
    let password = "test-password-udp-chacha";
    let method = "chacha20-ietf-poly1305";
    let cipher: eggress_protocol_shadowsocks::CipherMethod =
        eggress_protocol_shadowsocks::CipherMethod::parse_method(method).unwrap();

    let (ss_addr, mut _ss_guard) = start_external_ssserver(password, method).await;

    let target = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), udp_echo_addr.port());
    let salt = vec![0x44u8; cipher.salt_size()];

    let encoded = eggress_protocol_shadowsocks::udp::encode_udp_packet(
        cipher,
        password.as_bytes(),
        &eggress_core::TargetAddr {
            host: eggress_core::TargetHost::Ip(target.ip()),
            port: target.port(),
        },
        b"chacha20 udp test",
        &salt,
    )
    .expect("encode failed");

    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    udp_sock
        .send_to(&encoded, ss_addr)
        .await
        .expect("send failed");

    let mut buf = [0u8; 65535];
    let mut response = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), udp_sock.recv_from(&mut buf)).await {
            Ok(Ok((n, _))) => {
                response = Some(buf[..n].to_vec());
                break;
            }
            _ => continue,
        }
    }

    match &response {
        Some(data) => {
            match eggress_protocol_shadowsocks::udp::decode_udp_packet(
                cipher,
                password.as_bytes(),
                data,
            ) {
                Ok((_target, payload)) => {
                    assert_eq!(payload, b"chacha20 udp test", "UDP payload mismatch");
                }
                Err(e) => {
                    panic!("UDP decode failed: {e}");
                }
            }
        }
        None => {
            panic!("No UDP response received from external ssserver");
        }
    }

    _ss_guard.kill();
    udp_jh.abort();
}

// ===== Inbound Shadowsocks UDP Tests =====

/// Inbound Shadowsocks UDP: eggress receives encrypted packet, decrypts,
/// forwards to UDP echo, re-encrypts response.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1"]
async fn interop_shadowsocks_udp_inbound_echo() {
    require_shadowsocks_interop();

    let (udp_echo_addr, udp_jh) = start_udp_echo().await;
    let password = "test-password-inbound-udp";
    let method = "aes-256-gcm";
    let cipher: eggress_protocol_shadowsocks::CipherMethod =
        eggress_protocol_shadowsocks::CipherMethod::parse_method(method).unwrap();

    let ss_config = format!(
        r#"
version = 1

[[listeners]]
name = "ss-in"
bind = "127.0.0.1:0"
protocols = ["shadowsocks"]

[listeners.shadowsocks]
method = "{method}"
password = "{password}"

[listeners.udp]
mode = "shadowsocks_udp"
client_pin = true

[[rules]]
id = "route-all"
direct = true
"#
    );

    let (ss_addr, cancel, ss_jh) = start_eggress_from_toml_running(&ss_config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let target = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), udp_echo_addr.port());
    let salt = vec![0x55u8; cipher.salt_size()];

    let encoded = eggress_protocol_shadowsocks::udp::encode_udp_packet(
        cipher,
        password.as_bytes(),
        &eggress_core::TargetAddr {
            host: eggress_core::TargetHost::Ip(target.ip()),
            port: target.port(),
        },
        b"inbound udp echo",
        &salt,
    )
    .expect("encode failed");

    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    udp_sock
        .send_to(&encoded, ss_addr)
        .await
        .expect("send failed");

    let mut buf = [0u8; 65535];
    let mut response = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), udp_sock.recv_from(&mut buf)).await {
            Ok(Ok((n, _))) => {
                response = Some(buf[..n].to_vec());
                break;
            }
            _ => continue,
        }
    }

    let data = response.expect("No UDP response received from eggress inbound SS server");
    let (_target, payload) =
        eggress_protocol_shadowsocks::udp::decode_udp_packet(cipher, password.as_bytes(), &data)
            .expect("UDP decode failed");

    assert_eq!(payload, b"inbound udp echo");

    cancel.cancel();
    let _ = ss_jh.await;
    udp_jh.abort();
}

/// Inbound Shadowsocks UDP: wrong password should produce no response.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1"]
async fn interop_shadowsocks_udp_inbound_wrong_password() {
    require_shadowsocks_interop();

    let (udp_echo_addr, udp_jh) = start_udp_echo().await;
    let correct_password = "correct-password-inbound";
    let wrong_password = "wrong-password-inbound";
    let method = "aes-256-gcm";
    let cipher: eggress_protocol_shadowsocks::CipherMethod =
        eggress_protocol_shadowsocks::CipherMethod::parse_method(method).unwrap();

    let ss_config = format!(
        r#"
version = 1

[[listeners]]
name = "ss-in"
bind = "127.0.0.1:0"
protocols = ["shadowsocks"]

[listeners.shadowsocks]
method = "{method}"
password = "{correct_password}"

[listeners.udp]
mode = "shadowsocks_udp"
client_pin = true

[[rules]]
id = "route-all"
direct = true
"#
    );

    let (ss_addr, cancel, ss_jh) = start_eggress_from_toml_running(&ss_config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let target = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), udp_echo_addr.port());
    let salt = vec![0x56u8; cipher.salt_size()];

    let encoded = eggress_protocol_shadowsocks::udp::encode_udp_packet(
        cipher,
        wrong_password.as_bytes(),
        &eggress_core::TargetAddr {
            host: eggress_core::TargetHost::Ip(target.ip()),
            port: target.port(),
        },
        b"should fail",
        &salt,
    )
    .expect("encode failed");

    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    udp_sock
        .send_to(&encoded, ss_addr)
        .await
        .expect("send failed");

    let mut buf = [0u8; 65535];
    let result = tokio::time::timeout(Duration::from_secs(2), udp_sock.recv_from(&mut buf)).await;

    assert!(
        result.is_err(),
        "wrong password should not produce a response, but got data"
    );

    cancel.cancel();
    let _ = ss_jh.await;
    udp_jh.abort();
}
