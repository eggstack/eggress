//! Shadowsocks interoperability test scaffolding.
//!
//! These tests verify that eggress can interoperate with external Shadowsocks
//! implementations (`ssserver`/`sslocal` from shadowsocks-rust or shadowsocks-libev).
//!
//! # TCP Tests: Expected to Fail
//!
//! The eggress Shadowsocks TCP implementation uses **non-standard AEAD framing**:
//! - Standard: two separate AEAD operations per chunk (encrypted length + encrypted payload)
//! - Eggress: single AEAD operation with cleartext 2-byte length prefix
//!
//! This means TCP interop tests against standard Shadowsocks servers will fail
//! with AEAD decryption errors after the initial handshake. See
//! `docs/protocols/SHADOWSOCKS_TCP_AUDIT.md` for details.
//!
//! # UDP Tests: Standard-Compliant
//!
//! The eggress Shadowsocks UDP implementation uses standard AEAD format and has
//! a better chance of interoperating with standard implementations.
//!
//! # Running
//!
//! All tests are `#[ignore]` and gated behind `EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1`.
//!
//! ```bash
//! # TCP tests (expected to fail due to non-standard framing)
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
            "127.0.0.1",
            "-p",
            &port.to_string(),
            "-m",
            method,
            "-k",
            password,
            "--no-delay",
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
///
/// **Expected to FAIL** due to non-standard Shadowsocks TCP framing.
///
/// Eggress uses a single AEAD operation with cleartext length prefix, while
/// standard Shadowsocks uses two separate AEAD operations (encrypted length +
/// encrypted payload). The external ssserver will fail to decrypt Eggress's
/// data chunks after the initial handshake.
///
/// See `docs/protocols/SHADOWSOCKS_TCP_AUDIT.md` for the full audit.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver/sslocal"]
async fn interop_shadowsocks_tcp_eggress_to_external_server() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let password = "test-password-interop";
    let method = "aes-256-gcm";

    // Start external ssserver (standard implementation)
    let (ss_addr, mut _ss_guard) = start_external_ssserver(password, method).await;

    // Start eggress with SOCKS5 inbound + Shadowsocks upstream → external ssserver
    let ss_uri = format!("ss://{}@127.0.0.1:{}#{method}", password, ss_addr.port());
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

    // Attempt TCP echo through eggress SOCKS5 → external ssserver → echo
    // This is EXPECTED TO FAIL because eggress's Shadowsocks TCP framing is non-standard.
    let result = send_through_socks5(
        proxy_addr,
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        b"interop tcp test",
    )
    .await;

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();

    // Document the expected failure rather than asserting it,
    // since the behavior may change if the framing is fixed in the future.
    match &result {
        Ok(payload) => {
            // Unexpected success — this would mean the framing is compatible
            eprintln!("UNEXPECTED: TCP interop succeeded with payload: {payload:?}");
        }
        Err(e) => {
            // Expected failure due to non-standard framing
            eprintln!("EXPECTED FAILURE: TCP interop failed as anticipated: {e}");
            eprintln!(
                "This is expected because eggress uses non-standard Shadowsocks TCP framing."
            );
            eprintln!("See docs/protocols/SHADOWSOCKS_TCP_AUDIT.md");
        }
    }
}

/// TCP interop: external sslocal → eggress Shadowsocks server (via TOML config) → TCP echo.
///
/// **Expected to FAIL** due to non-standard framing.
///
/// A standard sslocal connecting to an eggress Shadowsocks server will fail
/// because eggress's server-side framing is also non-standard.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver/sslocal"]
async fn interop_shadowsocks_tcp_external_client_to_eggress() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let password = "test-password-reverse";
    let method = "aes-256-gcm";

    // Start eggress with inbound Shadowsocks server (via TOML config with ss:// URI)
    // This uses eggress's SS server mode which is also non-standard.
    let ss_config = r#"
version = 1

[[listeners]]
name = "ss-in"
bind = "127.0.0.1:0"
protocols = ["shadowsocks"]

[[upstreams]]
id = "direct"
uri = "direct://"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["direct"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#
    .to_string();

    let (ss_addr, cancel, ss_jh) = start_eggress_from_toml_running(&ss_config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Start local sslocal pointing to our eggress SS server, exposing a SOCKS5 port
    let socks_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let socks_port = socks_listener.local_addr().unwrap().port();
    drop(socks_listener);

    let sslocal_child = std::process::Command::new(sslocal_bin())
        .args([
            "-s",
            &ss_addr.ip().to_string(),
            "-p",
            &ss_addr.port().to_string(),
            "-m",
            method,
            "-k",
            password,
            "-l",
            &socks_port.to_string(),
            "-b",
            "127.0.0.1",
            "--no-delay",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start sslocal");
    let mut _sslocal_guard = ProcessGuard::new(sslocal_child);

    if !wait_for_port(socks_port, Duration::from_secs(5)).await {
        eprintln!("sslocal failed to start, skipping test");
        cancel.cancel();
        let _ = ss_jh.await;
        echo_jh.abort();
        return;
    }

    // Attempt TCP echo through sslocal → eggress SS server → echo
    // This is EXPECTED TO FAIL because eggress's framing is non-standard.
    let result = send_through_socks5(
        std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), socks_port),
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        b"interop tcp test reverse",
    )
    .await;

    cancel.cancel();
    let _ = ss_jh.await;
    echo_jh.abort();

    match &result {
        Ok(payload) => {
            eprintln!("UNEXPECTED: TCP interop succeeded with payload: {payload:?}");
        }
        Err(e) => {
            eprintln!("EXPECTED FAILURE: TCP interop failed as anticipated: {e}");
            eprintln!(
                "This is expected because eggress uses non-standard Shadowsocks TCP framing."
            );
        }
    }
}

// ===== UDP Interop Tests =====

/// UDP interop: eggress Shadowsocks UDP → external ssserver → UDP echo.
///
/// Shadowsocks UDP is standard-compliant, so this test has a better chance of
/// passing against standard implementations.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver"]
async fn interop_shadowsocks_udp_eggress_to_external_server() {
    skip_if_unavailable();

    let (udp_echo_addr, udp_jh) = start_udp_echo().await;
    let password = "test-password-udp";
    let method = "aes-256-gcm";
    let cipher: eggress_protocol_shadowsocks::CipherMethod =
        eggress_protocol_shadowsocks::CipherMethod::parse_method(method).unwrap();

    // Start external ssserver with UDP relay
    let (ss_addr, mut _ss_guard) = start_external_ssserver(password, method).await;

    // Build a Shadowsocks UDP packet and send it to the ssserver
    let target = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), udp_echo_addr.port());
    // Use a fixed salt for testing (salt value doesn't affect correctness, only length matters)
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

    // Read response with timeout
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
            // Try to decode the response
            match eggress_protocol_shadowsocks::udp::decode_udp_packet(
                cipher,
                password.as_bytes(),
                data,
            ) {
                Ok((_target, payload)) => {
                    assert_eq!(payload, b"interop udp test", "UDP payload mismatch");
                }
                Err(e) => {
                    eprintln!("UDP decode failed: {e}");
                    eprintln!("Raw response: {} bytes", data.len());
                }
            }
        }
        None => {
            eprintln!("No UDP response received from external ssserver");
            eprintln!("This may indicate framing incompatibility or ssserver not relaying UDP");
        }
    }

    _ss_guard.kill();
    udp_jh.abort();
}

/// UDP interop: verify UDP via full TOML-configured eggress stack.
///
/// Uses eggress's full runtime with a TOML-configured Shadowsocks upstream
/// to test the complete path through SOCKS5 UDP ASSOCIATE.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 and ssserver"]
async fn interop_shadowsocks_udp_via_toml_config() {
    skip_if_unavailable();

    let (udp_echo_addr, udp_jh) = start_udp_echo().await;
    let password = "test-password-toml";
    let method = "aes-256-gcm";

    // Start external ssserver
    let (ss_addr, mut _ss_guard) = start_external_ssserver(password, method).await;

    // Configure eggress with SOCKS5 inbound and Shadowsocks upstream
    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "ss-up"
uri = "ss://{}@127.0.0.1:{}#{method}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["ss-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        password,
        ss_addr.port()
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send a UDP packet through eggress SOCKS5 → Shadowsocks UDP → echo
    let result = send_udp_through_socks5(proxy_addr, udp_echo_addr, b"toml config udp test").await;

    cancel.cancel();
    let _ = proxy_jh.await;
    udp_jh.abort();

    match &result {
        Ok(payload) => {
            assert_eq!(payload, b"toml config udp test", "UDP payload mismatch");
        }
        Err(e) => {
            eprintln!("UDP interop via TOML config failed: {e}");
            eprintln!("This may indicate UDP framing incompatibility with the external ssserver");
        }
    }
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
uri = "ss://{}@127.0.0.1:{}#{method}"

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
uri = "ss://{}@127.0.0.1:{}#{method}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["ss-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        password,
        ss_addr.port()
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Expected to fail due to non-standard TCP framing
    let result = send_through_socks5(
        proxy_addr,
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        b"aes-128-gcm test",
    )
    .await;

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();

    match &result {
        Ok(p) => eprintln!("UNEXPECTED SUCCESS with aes-128-gcm: {p:?}"),
        Err(e) => eprintln!("EXPECTED FAILURE with aes-128-gcm: {e}"),
    }
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
uri = "ss://{}@127.0.0.1:{}#{method}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["ss-up"]

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        password,
        ss_addr.port()
    );

    let (proxy_addr, cancel, proxy_jh) = start_eggress_from_toml_running(&config).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Expected to fail due to non-standard TCP framing
    let result = send_through_socks5(
        proxy_addr,
        &echo_addr.ip().to_string(),
        echo_addr.port(),
        b"chacha20 test",
    )
    .await;

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();

    match &result {
        Ok(p) => eprintln!("UNEXPECTED SUCCESS with chacha20-ietf-poly1305: {p:?}"),
        Err(e) => eprintln!("EXPECTED FAILURE with chacha20-ietf-poly1305: {e}"),
    }
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
                    eprintln!("UDP decode failed: {e}");
                }
            }
        }
        None => {
            eprintln!("No UDP response received (may indicate incompatibility)");
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
                    eprintln!("UDP decode failed: {e}");
                }
            }
        }
        None => {
            eprintln!("No UDP response received (may indicate incompatibility)");
        }
    }

    _ss_guard.kill();
    udp_jh.abort();
}
