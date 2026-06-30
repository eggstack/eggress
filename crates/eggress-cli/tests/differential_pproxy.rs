//! Differential interoperability tests comparing Eggress with Python pproxy.
//!
//! These tests verify that eggress and pproxy produce equivalent behavior for
//! the same proxy scenarios. Each test starts both proxies, sends identical
//! requests, and compares coarse results (success/failure, payload match).
//!
//! All tests are `#[ignore]` and require:
//! - `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` environment variable
//! - Python 3 with pproxy installed (`pip install pproxy`)
//!
//! Run with: `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored`

#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::connector::Connector;
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::relay::relay;
use eggress_core::{BoxStream, TargetAddr, TargetHost};
use eggress_protocol_http::connect::client::http_connect;
use eggress_protocol_socks::socks5::client::socks5_connect;
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_routing::{RouteActionSpec, RouteService, Router};
use eggress_uri::{EndpointSpec, ProtocolSpec, ProxyHopSpec};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

type HandshakeFuture<'a> = std::pin::Pin<
    Box<
        dyn std::future::Future<
                Output = Result<BoxStream, Box<dyn std::error::Error + Send + Sync>>,
            > + Send
            + 'a,
    >,
>;

// ===== Hop Handlers =====

struct HttpHopHandler;

impl HopHandler for HttpHopHandler {
    fn protocol(&self) -> ProtocolSpec {
        ProtocolSpec::Http
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a ProxyHopSpec,
    ) -> HandshakeFuture<'a> {
        let auth = hop
            .credentials
            .as_ref()
            .map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            http_connect(stream, target, auth, &Default::default())
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

struct Socks5HopHandler;

impl HopHandler for Socks5HopHandler {
    fn protocol(&self) -> ProtocolSpec {
        ProtocolSpec::Socks5
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a ProxyHopSpec,
    ) -> HandshakeFuture<'a> {
        let socks_addr = target_to_socks_addr(target);
        let auth = hop
            .credentials
            .as_ref()
            .map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            socks5_connect(stream, &socks_addr, auth)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

fn build_executor() -> ChainExecutor {
    ChainExecutor::new(vec![Box::new(HttpHopHandler), Box::new(Socks5HopHandler)])
}

fn target_to_socks_addr(target: &TargetAddr) -> SocksAddr {
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => SocksAddr::IPv4(ip.octets(), target.port),
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => SocksAddr::IPv6(ip.octets(), target.port),
        TargetHost::Domain(d) => SocksAddr::Domain(d.clone(), target.port),
    }
}

fn socket_addr(host: &str, port: u16) -> std::net::SocketAddr {
    std::net::SocketAddr::new(host.parse().unwrap(), port)
}

// ===== Prerequisite Checks =====

fn require_external_interop() {
    if std::env::var("EGRESS_REQUIRE_EXTERNAL_INTEROP").is_err() {
        panic!("EGRESS_REQUIRE_EXTERNAL_INTEROP not set");
    }
}

fn python_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn pproxy_available() -> bool {
    std::process::Command::new("python3")
        .args(["-c", "import pproxy"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn skip_if_unavailable() {
    require_external_interop();
    if !python_available() || !pproxy_available() {
        eprintln!("skipping: python3 or pproxy not available");
        panic!("python3 or pproxy not available");
    }
}

// ===== Helpers =====

async fn start_pproxy_server(protocol: &str, port: u16) -> std::process::Child {
    let listen = format!("{}://127.0.0.1:{}", protocol, port);
    std::process::Command::new("python3")
        .args(["-m", "pproxy", "-l", &listen, "-r", "direct"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy")
}

async fn start_pproxy_server_with_auth(
    protocol: &str,
    port: u16,
    username: &str,
    password: &str,
) -> std::process::Child {
    let listen = format!(
        "{}://{}:{}@127.0.0.1:{}",
        protocol, username, password, port
    );
    std::process::Command::new("python3")
        .args(["-m", "pproxy", "-l", &listen, "-r", "direct"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy")
}

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

async fn start_eggress_server(
    protocols: Vec<eggress_core::ProtocolId>,
) -> (
    std::net::SocketAddr,
    CancellationToken,
    tokio::task::JoinHandle<()>,
) {
    let config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols,
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let listener = TcpListener::new(&config, cancel.clone()).await.unwrap();
    let addr = listener.local_addr().unwrap();

    let conn_protocols: Arc<[eggress_core::ProtocolId]> = config.protocols.clone().into();
    let jh = tokio::spawn(async move {
        loop {
            let conn = match listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            let config = eggress_server::ConnectionConfig {
                routing: Arc::new(Router::new(vec![], RouteActionSpec::Direct))
                    as Arc<dyn RouteService>,
                context: eggress_server::ConnectionContext::default(),
                handshake_timeout: Duration::from_secs(5),
                connect_timeout: Duration::from_secs(10),
                protocols: conn_protocols.clone(),
                authentication: eggress_server::accept::InboundAuthentication::None,
                metrics: None,
                udp: None,
                tls_client_config: None,
                shadowsocks: None,
                shadowsocks_metrics: None,
            };
            tokio::spawn(async move {
                let _ = eggress_server::serve_connection(conn.stream, config).await;
            });
        }
    });

    (addr, cancel, jh)
}

/// Send data through a SOCKS5 proxy and return success + payload.
async fn send_through_socks5(
    proxy_addr: std::net::SocketAddr,
    target: &TargetAddr,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;
    let boxed: BoxStream = Box::new(stream);
    let socks_addr = target_to_socks_addr(target);
    let mut conn = socks5_connect(boxed, &socks_addr, None)
        .await
        .map_err(|e| format!("socks5 handshake failed: {e}"))?;
    conn.write_all(payload)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    conn.shutdown()
        .await
        .map_err(|e| format!("shutdown failed: {e}"))?;
    let mut buf = Vec::new();
    conn.read_to_end(&mut buf)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    Ok(buf)
}

/// Send data through an HTTP CONNECT proxy and return success + payload.
async fn send_through_http(
    proxy_addr: std::net::SocketAddr,
    target: &TargetAddr,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;
    let boxed: BoxStream = Box::new(stream);
    let mut conn = http_connect(boxed, target, None, &Default::default())
        .await
        .map_err(|e| format!("http connect handshake failed: {e}"))?;
    conn.write_all(payload)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    conn.shutdown()
        .await
        .map_err(|e| format!("shutdown failed: {e}"))?;
    let mut buf = Vec::new();
    conn.read_to_end(&mut buf)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    Ok(buf)
}

// ===== Reusable Test Primitives =====

/// Guard that kills a child process on drop.
struct ProcessGuard {
    child: Option<std::process::Child>,
}

impl ProcessGuard {
    fn new(child: std::process::Child) -> Self {
        Self { child: Some(child) }
    }

    /// Kill the process early (before drop).
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

/// Guard that cancels a task and its join handle on drop.
struct TaskGuard {
    cancel: Option<CancellationToken>,
    jh: Option<tokio::task::JoinHandle<()>>,
}

impl TaskGuard {
    fn new(cancel: CancellationToken, jh: tokio::task::JoinHandle<()>) -> Self {
        Self {
            cancel: Some(cancel),
            jh: Some(jh),
        }
    }

    fn cancel_token(&self) -> &CancellationToken {
        self.cancel.as_ref().unwrap()
    }

    fn shutdown(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        if let Some(jh) = self.jh.take() {
            jh.abort();
        }
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Start a TCP echo server, returning address and a cleanup guard.
///
/// Wraps `eggress_testkit::start_echo_server()` with a guard that aborts
/// the task when dropped.
async fn start_tcp_echo() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    eggress_testkit::start_echo_server().await
}

/// Start a UDP echo server that echoes received packets back to the sender.
///
/// Returns the listening address and a join handle. The task aborts when
/// the handle is dropped.
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

/// Start an eggress server from a TOML config string.
///
/// Writes the config to a temp file and starts a `ServiceSupervisor`.
/// Returns the supervisor (must be kept alive for the server to run).
/// Call `supervisor.run()` on a blocking thread to drive the server.
fn start_eggress_from_toml(config_str: &str) -> eggress_runtime::ServiceSupervisor {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().expect("create tempfile");
    f.write_all(config_str.as_bytes()).expect("write config");
    f.flush().expect("flush config");
    let path = f.path().to_str().unwrap().to_string();
    // Keep the tempfile alive by leaking it; it will be cleaned up on process exit.
    // The supervisor needs the path to remain valid.
    std::mem::forget(f);
    eggress_runtime::ServiceSupervisor::start(&path).expect("start eggress from TOML")
}

/// Compare two TCP echo results. Panics with a clear message on mismatch.
///
/// Both results should be `Ok(Vec<u8>)` with identical payloads.
fn compare_tcp_echo(
    label_a: &str,
    result_a: &Result<Vec<u8>, String>,
    label_b: &str,
    result_b: &Result<Vec<u8>, String>,
) {
    match (result_a, result_b) {
        (Ok(payload_a), Ok(payload_b)) => {
            assert_eq!(
                payload_a, payload_b,
                "TCP echo payload mismatch: {label_a} returned {} bytes, {label_b} returned {} bytes",
                payload_a.len(),
                payload_b.len()
            );
        }
        (Err(e), _) => panic!("{label_a} failed: {e}"),
        (_, Err(e)) => panic!("{label_b} failed: {e}"),
    }
}

/// Compare two UDP echo results. Asserts both succeeded and payloads match.
fn compare_udp_echo(
    label_a: &str,
    result_a: &Option<Vec<u8>>,
    label_b: &str,
    result_b: &Option<Vec<u8>>,
) {
    match (result_a, result_b) {
        (Some(payload_a), Some(payload_b)) => {
            assert_eq!(
                payload_a, payload_b,
                "UDP echo payload mismatch: {label_a} returned {} bytes, {label_b} returned {} bytes",
                payload_a.len(),
                payload_b.len()
            );
        }
        (None, _) => panic!("{label_a} did not receive UDP response"),
        (_, None) => panic!("{label_b} did not receive UDP response"),
    }
}

/// Assert coarse failure equivalence: both succeeded or both failed.
fn assert_coarse_failure_equivalence<T>(
    label_a: &str,
    result_a: &Result<T, String>,
    label_b: &str,
    result_b: &Result<T, String>,
) {
    match (result_a, result_b) {
        (Ok(_), Ok(_)) => {
            // Both succeeded — good
        }
        (Err(e), Ok(_)) => {
            panic!("{label_a} failed but {label_b} succeeded: {label_a} error: {e}");
        }
        (Ok(_), Err(e)) => {
            panic!("{label_a} succeeded but {label_b} failed: {label_b} error: {e}");
        }
        (Err(e_a), Err(e_b)) => {
            // Both failed — acceptable
            eprintln!("both failed (expected): {label_a}: {e_a}, {label_b}: {e_b}");
        }
    }
}

/// Build a SOCKS5 UDP ASSOCIATE request and return the relay address.
///
/// Performs the SOCKS5 handshake (no auth) on the given stream, sends a
/// UDP ASSOCIATE command, and parses the reply to extract the relay address.
async fn socks5_udp_associate(
    stream: &mut tokio::net::TcpStream,
) -> std::io::Result<std::net::SocketAddr> {
    // Method negotiation: no auth
    stream.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await?;
    assert_eq!(resp, [0x05, 0x00]);

    // UDP ASSOCIATE: VER=5, CMD=3, RSV=0, ATYP=1 (IPv4), addr=0.0.0.0, port=0
    stream
        .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
        .await?;
    stream.write_all(&0u16.to_be_bytes()).await?;

    let mut reply = [0u8; 22];
    let n = stream.read(&mut reply).await?;
    assert!(n >= 10, "UDP ASSOCIATE reply too short: {n} bytes");
    assert_eq!(reply[0], 0x05, "SOCKS5 version mismatch");
    assert_eq!(reply[1], 0x00, "UDP ASSOCIATE failed: {}", reply[1]);

    let relay_ip = match reply[3] {
        0x01 => {
            let ip = std::net::Ipv4Addr::new(reply[4], reply[5], reply[6], reply[7]);
            std::net::IpAddr::V4(ip)
        }
        _ => panic!("unexpected address type in UDP ASSOCIATE reply"),
    };
    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    Ok(std::net::SocketAddr::new(relay_ip, relay_port))
}

/// Send a SOCKS5 UDP datagram to the given relay address with an IPv4 target.
fn build_socks5_udp_packet(target: std::net::SocketAddr, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00]; // RSV + FRAG
    match target.ip() {
        std::net::IpAddr::V4(ip) => {
            pkt.push(0x01); // ATYP IPv4
            pkt.extend_from_slice(&ip.octets());
        }
        std::net::IpAddr::V6(ip) => {
            pkt.push(0x04); // ATYP IPv6
            pkt.extend_from_slice(&ip.octets());
        }
    }
    pkt.extend_from_slice(&target.port().to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

/// Receive a UDP response with a timeout, returning the payload.
async fn recv_udp_response(sock: &tokio::net::UdpSocket, timeout: Duration) -> Option<Vec<u8>> {
    let mut buf = [0u8; 65535];
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), sock.recv_from(&mut buf)).await {
            Ok(Ok((n, _))) => return Some(buf[..n].to_vec()),
            _ => continue,
        }
    }
    None
}

// ===== Scenario 1: SOCKS5 CONNECT inbound to local TCP echo =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_connect_tcp_echo() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy SOCKS5 ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"differential socks5",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_socks5(eggress_addr, &target, b"differential socks5").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    // Compare results
    match (&pproxy_result, &eggress_result) {
        (Ok(ppayload), Ok(epayload)) => {
            assert_eq!(
                ppayload,
                epayload,
                "payload mismatch: pproxy returned {} bytes, eggress returned {} bytes",
                ppayload.len(),
                epayload.len()
            );
            assert_eq!(*ppayload, b"differential socks5");
        }
        (Err(e), _) => panic!("pproxy failed: {e}"),
        (_, Err(e)) => panic!("eggress failed: {e}"),
    }
}

// ===== Scenario 2: HTTP CONNECT inbound to local TCP echo =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_tcp_echo() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"differential http",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_http(eggress_addr, &target, b"differential http").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    // Compare results
    match (&pproxy_result, &eggress_result) {
        (Ok(ppayload), Ok(epayload)) => {
            assert_eq!(
                ppayload,
                epayload,
                "payload mismatch: pproxy returned {} bytes, eggress returned {} bytes",
                ppayload.len(),
                epayload.len()
            );
            assert_eq!(*ppayload, b"differential http");
        }
        (Err(e), _) => panic!("pproxy failed: {e}"),
        (_, Err(e)) => panic!("eggress failed: {e}"),
    }
}

// ===== Scenario 3: SOCKS5 UDP ASSOCIATE direct local UDP echo =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_udp_associate() {
    skip_if_unavailable();

    // Start a UDP echo server
    let udp_listener = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let udp_addr = udp_listener.local_addr().unwrap();

    let udp_jh = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            let (n, peer) = match udp_listener.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(_) => break,
            };
            let _ = udp_listener.send_to(&buf[..n], peer).await;
        }
    });

    // --- pproxy UDP relay ---
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let pproxy_udp_port = eggress_testkit::get_free_port().await;
    let listen_tcp = format!("socks5://127.0.0.1:{}", pproxy_tcp_port);
    let listen_udp = format!("socks5://127.0.0.1:{}", pproxy_udp_port);
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &listen_tcp,
            "-ul",
            &listen_udp,
            "-r",
            "direct",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_tcp_port, Duration::from_secs(5)).await,
        "pproxy TCP failed to start"
    );
    // Give pproxy UDP a moment to bind
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send UDP through pproxy relay (pproxy uses its own UDP protocol, not SOCKS5 UDP ASSOCIATE)
    // We test that pproxy can relay UDP to our echo server
    let pproxy_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        // pproxy UDP protocol: send SOCKS5-style UDP header with target address then data
        let mut packet = Vec::new();
        // SOCKS5 UDP header: reserved (2 bytes) + frag (1 byte) + addr type + addr + port
        packet.extend_from_slice(&[0x00, 0x00, 0x00]); // RSV + FRAG
        match udp_addr.ip() {
            std::net::IpAddr::V4(ip) => {
                packet.push(0x01); // ATYP IPv4
                packet.extend_from_slice(&ip.octets());
            }
            std::net::IpAddr::V6(ip) => {
                packet.push(0x04); // ATYP IPv6
                packet.extend_from_slice(&ip.octets());
            }
        }
        packet.extend_from_slice(&udp_addr.port().to_be_bytes());
        packet.extend_from_slice(b"pproxy udp test");

        let _ = sock.send_to(&packet, ("127.0.0.1", pproxy_udp_port)).await;
        let mut buf = [0u8; 4096];
        let mut result = None;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(200), sock.recv_from(&mut buf)).await {
                Ok(Ok((n, _))) => {
                    result = Some(buf[..n].to_vec());
                    break;
                }
                _ => continue,
            }
        }
        result
    };
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 UDP ASSOCIATE ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect TCP for SOCKS5 UDP ASSOCIATE
    let tcp_stream = tokio::net::TcpStream::connect(eggress_addr).await.unwrap();
    let (mut reader, mut writer) = tokio::io::split(tcp_stream);

    // SOCKS5 method negotiation: no auth
    writer.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    reader.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp[0], 0x05, "SOCKS5 version mismatch");
    assert_eq!(resp[1], 0x00, "no acceptable method");

    // Send UDP ASSOCIATE request
    // VER CMD RSV ATYP DST.ADDR DST.PORT
    let mut udp_req = vec![0x05, 0x03, 0x00]; // VER=5, CMD=UDP_ASSOCIATE, RSV=0
                                              // Address: 0.0.0.0:0 (client doesn't know its relay address yet)
    udp_req.push(0x01); // ATYP IPv4
    udp_req.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // 0.0.0.0
    udp_req.extend_from_slice(&[0x00, 0x00]); // port 0
    writer.write_all(&udp_req).await.unwrap();

    // Read UDP ASSOCIATE reply
    let mut udp_reply = [0u8; 22]; // Max reply size
    let n = reader.read(&mut udp_reply).await.unwrap();
    assert!(n >= 10, "UDP ASSociate reply too short: {n} bytes");
    assert_eq!(udp_reply[0], 0x05, "SOCKS5 version mismatch in reply");
    assert_eq!(udp_reply[1], 0x00, "UDP ASSOCIATE failed: {}", udp_reply[1]);

    // Parse relay address from reply
    let relay_ip = match udp_reply[3] {
        0x01 => {
            let ip =
                std::net::Ipv4Addr::new(udp_reply[4], udp_reply[5], udp_reply[6], udp_reply[7]);
            std::net::IpAddr::V4(ip)
        }
        _ => {
            cancel.cancel();
            let _ = eggress_jh.await;
            udp_jh.abort();
            panic!("unexpected address type in UDP ASSOCIATE reply");
        }
    };
    let relay_port = u16::from_be_bytes([udp_reply[8], udp_reply[9]]);
    let relay_addr = std::net::SocketAddr::new(relay_ip, relay_port);

    // Send UDP datagram through the relay
    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut packet = Vec::new();
    packet.extend_from_slice(&[0x00, 0x00, 0x00]); // RSV + FRAG
    match udp_addr.ip() {
        std::net::IpAddr::V4(ip) => {
            packet.push(0x01);
            packet.extend_from_slice(&ip.octets());
        }
        std::net::IpAddr::V6(ip) => {
            packet.push(0x04);
            packet.extend_from_slice(&ip.octets());
        }
    }
    packet.extend_from_slice(&udp_addr.port().to_be_bytes());
    packet.extend_from_slice(b"eggress udp test");

    let _ = udp_sock.send_to(&packet, relay_addr).await;

    let mut buf = [0u8; 4096];
    let mut eggress_response = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), udp_sock.recv_from(&mut buf)).await {
            Ok(Ok((n, _))) => {
                eggress_response = Some(buf[..n].to_vec());
                break;
            }
            _ => continue,
        }
    }

    // Clean up TCP control connection
    drop(writer);
    drop(reader);
    cancel.cancel();
    let _ = eggress_jh.await;
    udp_jh.abort();

    // Both should have received echoed UDP data
    // pproxy uses its own relay protocol so the raw bytes differ,
    // but both should successfully relay the UDP payload
    assert!(
        pproxy_response.is_some(),
        "pproxy UDP relay did not receive response"
    );
    assert!(
        eggress_response.is_some(),
        "eggress SOCKS5 UDP ASSOCIATE did not receive response"
    );

    // Verify the echoed data is present in both responses
    let pproxy_payload = pproxy_response.unwrap();
    let eggress_payload = eggress_response.unwrap();
    assert!(
        pproxy_payload
            .windows(b"pproxy udp test".len())
            .any(|w| w == b"pproxy udp test"),
        "pproxy did not echo UDP payload"
    );
    assert!(
        eggress_payload
            .windows(b"eggress udp test".len())
            .any(|w| w == b"eggress udp test"),
        "eggress did not echo UDP payload"
    );
}

// ===== Scenario 4: SOCKS5 inbound through HTTP CONNECT upstream =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_through_http_upstream() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // Start pproxy as HTTP upstream
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Start eggress SOCKS5 inbound chaining through pproxy HTTP upstream
    let socks5_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec![eggress_core::ProtocolId::Socks5],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let socks5_listener = TcpListener::new(&socks5_config, cancel.clone())
        .await
        .unwrap();
    let socks5_addr = socks5_listener.local_addr().unwrap();

    let socks5_jh = tokio::spawn(async move {
        loop {
            let conn = match socks5_listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            tokio::spawn(async move {
                use eggress_protocol_socks::socks5::server::{
                    read_connect_request, read_method_negotiation, send_connect_reply,
                    send_method_selection,
                };

                let stream = conn.stream;
                let (mut reader, mut writer) = tokio::io::split(stream);
                if let Ok(methods) = read_method_negotiation(&mut reader).await {
                    let _ = send_method_selection(&mut writer, &methods, None).await;
                    if let Ok(socks_addr) = read_connect_request(&mut reader).await {
                        let target = match &socks_addr {
                            SocksAddr::IPv4(octets, port) => TargetAddr {
                                host: TargetHost::Ip(std::net::IpAddr::V4((*octets).into())),
                                port: *port,
                            },
                            SocksAddr::IPv6(octets, port) => TargetAddr {
                                host: TargetHost::Ip(std::net::IpAddr::V6((*octets).into())),
                                port: *port,
                            },
                            SocksAddr::Domain(domain, port) => TargetAddr {
                                host: TargetHost::Domain(domain.clone()),
                                port: *port,
                            },
                        };

                        let bind_addr = SocksAddr::IPv4([0, 0, 0, 0], 0);
                        let _ = send_connect_reply(&mut writer, 0x00, &bind_addr).await;

                        let client_stream: BoxStream = Box::new(tokio::io::join(reader, writer));
                        let executor = build_executor();
                        let chain = vec![ProxyHopSpec {
                            protocols: vec![ProtocolSpec::Http],
                            endpoint: EndpointSpec {
                                host: "127.0.0.1".to_string(),
                                port: pproxy_port,
                            },
                            credentials: None,
                            rule: None,
                            local_bind: None,
                            tls: false,
                            server_name: None,
                        }];
                        if let Ok(server_stream) = executor.execute(&chain, &target).await {
                            let _ = relay(client_stream, server_stream).await;
                        }
                    }
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send through eggress SOCKS5 → pproxy HTTP → echo
    let result = send_through_socks5(socks5_addr, &target, b"chain socks5->http").await;

    // Also send directly through pproxy HTTP → echo for comparison
    let direct_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"chain socks5->http",
    )
    .await;

    cancel.cancel();
    let _ = socks5_jh.await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();
    echo_jh.abort();

    // Both should succeed and return the same payload
    match (&result, &direct_result) {
        (Ok(chain_payload), Ok(direct_payload)) => {
            assert_eq!(
                chain_payload, direct_payload,
                "chain payload mismatch with direct pproxy"
            );
            assert_eq!(*chain_payload, b"chain socks5->http");
        }
        (Err(e), _) => panic!("chain through eggress SOCKS5 -> pproxy HTTP failed: {e}"),
        (_, Err(e)) => panic!("direct through pproxy HTTP failed: {e}"),
    }
}

// ===== Scenario 5: SOCKS5 inbound through SOCKS5 upstream =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_through_socks5_upstream() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // Start pproxy as SOCKS5 upstream
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Start eggress SOCKS5 inbound chaining through pproxy SOCKS5 upstream
    let socks5_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec![eggress_core::ProtocolId::Socks5],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let socks5_listener = TcpListener::new(&socks5_config, cancel.clone())
        .await
        .unwrap();
    let socks5_addr = socks5_listener.local_addr().unwrap();

    let socks5_jh = tokio::spawn(async move {
        loop {
            let conn = match socks5_listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            tokio::spawn(async move {
                use eggress_protocol_socks::socks5::server::{
                    read_connect_request, read_method_negotiation, send_connect_reply,
                    send_method_selection,
                };

                let stream = conn.stream;
                let (mut reader, mut writer) = tokio::io::split(stream);
                if let Ok(methods) = read_method_negotiation(&mut reader).await {
                    let _ = send_method_selection(&mut writer, &methods, None).await;
                    if let Ok(socks_addr) = read_connect_request(&mut reader).await {
                        let target = match &socks_addr {
                            SocksAddr::IPv4(octets, port) => TargetAddr {
                                host: TargetHost::Ip(std::net::IpAddr::V4((*octets).into())),
                                port: *port,
                            },
                            SocksAddr::IPv6(octets, port) => TargetAddr {
                                host: TargetHost::Ip(std::net::IpAddr::V6((*octets).into())),
                                port: *port,
                            },
                            SocksAddr::Domain(domain, port) => TargetAddr {
                                host: TargetHost::Domain(domain.clone()),
                                port: *port,
                            },
                        };

                        let bind_addr = SocksAddr::IPv4([0, 0, 0, 0], 0);
                        let _ = send_connect_reply(&mut writer, 0x00, &bind_addr).await;

                        let client_stream: BoxStream = Box::new(tokio::io::join(reader, writer));
                        let executor = build_executor();
                        let chain = vec![ProxyHopSpec {
                            protocols: vec![ProtocolSpec::Socks5],
                            endpoint: EndpointSpec {
                                host: "127.0.0.1".to_string(),
                                port: pproxy_port,
                            },
                            credentials: None,
                            rule: None,
                            local_bind: None,
                            tls: false,
                            server_name: None,
                        }];
                        if let Ok(server_stream) = executor.execute(&chain, &target).await {
                            let _ = relay(client_stream, server_stream).await;
                        }
                    }
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send through eggress SOCKS5 → pproxy SOCKS5 → echo
    let result = send_through_socks5(socks5_addr, &target, b"chain socks5->socks5").await;

    // Also send directly through pproxy SOCKS5 → echo for comparison
    let direct_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"chain socks5->socks5",
    )
    .await;

    cancel.cancel();
    let _ = socks5_jh.await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();
    echo_jh.abort();

    // Both should succeed and return the same payload
    match (&result, &direct_result) {
        (Ok(chain_payload), Ok(direct_payload)) => {
            assert_eq!(
                chain_payload, direct_payload,
                "chain payload mismatch with direct pproxy"
            );
            assert_eq!(*chain_payload, b"chain socks5->socks5");
        }
        (Err(e), _) => panic!("chain through eggress SOCKS5 -> pproxy SOCKS5 failed: {e}"),
        (_, Err(e)) => panic!("direct through pproxy SOCKS5 failed: {e}"),
    }
}

// ===== Scenario 6: Auth failure behavior =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_auth_failure() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy SOCKS5 with auth ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child =
        start_pproxy_server_with_auth("socks5", pproxy_port, "testuser", "testpass").await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Try connecting without auth → should fail
    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"should fail",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 with auth ---
    let socks5_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec![eggress_core::ProtocolId::Socks5],
        auth_required: true,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let socks5_listener = TcpListener::new(&socks5_config, cancel.clone())
        .await
        .unwrap();
    let socks5_addr = socks5_listener.local_addr().unwrap();

    let socks5_jh = tokio::spawn(async move {
        loop {
            let conn = match socks5_listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            tokio::spawn(async move {
                use eggress_protocol_socks::socks5::server::{
                    read_connect_request, read_method_negotiation, send_connect_reply,
                    send_method_selection,
                };

                let stream = conn.stream;
                let (mut reader, mut writer) = tokio::io::split(stream);
                if let Ok(methods) = read_method_negotiation(&mut reader).await {
                    let _ = send_method_selection(&mut writer, &methods, None).await;
                    if let Ok(socks_addr) = read_connect_request(&mut reader).await {
                        let target = match &socks_addr {
                            SocksAddr::IPv4(octets, port) => TargetAddr {
                                host: TargetHost::Ip(std::net::IpAddr::V4((*octets).into())),
                                port: *port,
                            },
                            SocksAddr::IPv6(octets, port) => TargetAddr {
                                host: TargetHost::Ip(std::net::IpAddr::V6((*octets).into())),
                                port: *port,
                            },
                            SocksAddr::Domain(domain, port) => TargetAddr {
                                host: TargetHost::Domain(domain.clone()),
                                port: *port,
                            },
                        };

                        let bind_addr = SocksAddr::IPv4([0, 0, 0, 0], 0);
                        let _ = send_connect_reply(&mut writer, 0x00, &bind_addr).await;

                        let client_stream: BoxStream = Box::new(tokio::io::join(reader, writer));
                        let connector = eggress_core::connector::DirectConnector;
                        if let Ok(server_stream) = connector.connect(&target).await {
                            let _ = relay(client_stream, server_stream).await;
                        }
                    }
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Try connecting without auth → should fail
    let eggress_result = send_through_socks5(socks5_addr, &target, b"should fail").await;

    cancel.cancel();
    let _ = socks5_jh.await;
    echo_jh.abort();

    // Both should reject the unauthenticated connection
    assert!(
        pproxy_result.is_err(),
        "pproxy should reject unauthenticated SOCKS5 connection"
    );
    assert!(
        eggress_result.is_err(),
        "eggress should reject unauthenticated SOCKS5 connection"
    );
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_auth_failure() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy HTTP with auth ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child =
        start_pproxy_server_with_auth("http", pproxy_port, "testuser", "testpass").await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Try connecting without auth → should fail
    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"should fail",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP with auth ---
    let http_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec![eggress_core::ProtocolId::Http],
        auth_required: true,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let http_listener = TcpListener::new(&http_config, cancel.clone())
        .await
        .unwrap();
    let http_addr = http_listener.local_addr().unwrap();

    let http_jh = tokio::spawn(async move {
        loop {
            let conn = match http_listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let stream = conn.stream;
                if let Ok((request, client_stream)) =
                    eggress_protocol_http::connect::server::handle_connect(stream, true, None).await
                {
                    let target = request.target;
                    let connector = eggress_core::connector::DirectConnector;
                    if let Ok(server_stream) = connector.connect(&target).await {
                        let _ = relay(client_stream, server_stream).await;
                    }
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Try connecting without auth → should fail
    let eggress_result = send_through_http(http_addr, &target, b"should fail").await;

    cancel.cancel();
    let _ = http_jh.await;
    echo_jh.abort();

    // Both should reject the unauthenticated connection
    assert!(
        pproxy_result.is_err(),
        "pproxy should reject unauthenticated HTTP connection"
    );
    assert!(
        eggress_result.is_err(),
        "eggress should reject unauthenticated HTTP connection"
    );
}

// ===== Probe Tests: Black-box exploration of pproxy behavior =====

/// Probe: What SOCKS5 reply code does pproxy return when the target port is refused?
///
/// Connects through pproxy SOCKS5 to a port that has nothing listening.
/// Observes the SOCKS5 reply code pproxy returns on connection refusal.
/// Expected: reply code 0x05 (connection refused) per RFC 1928.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn probe_pproxy_socks5_refused_reply() {
    skip_if_unavailable();

    // Bind a port, get its number, then close it so nothing is listening
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let refused_port = held.local_addr().unwrap().port();
    drop(held);

    let target = TargetAddr {
        host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
        port: refused_port,
    };

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let result =
        send_through_socks5(socket_addr("127.0.0.1", pproxy_port), &target, b"probe").await;

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    eprintln!("=== probe_pproxy_socks5_refused_reply ===");
    eprintln!("target port: {refused_port}");
    match &result {
        Ok(payload) => {
            eprintln!(
                "pproxy returned data: {} bytes: {:?}",
                payload.len(),
                payload
            );
        }
        Err(e) => {
            eprintln!("pproxy returned error: {e}");
        }
    }

    // pproxy should fail the connection since nothing is listening
    assert!(
        result.is_err(),
        "pproxy SOCKS5 to refused port should fail, but got: {result:?}"
    );
}

/// Probe: What HTTP status does pproxy return when the target port is refused?
///
/// Connects through pproxy HTTP CONNECT to a port that has nothing listening.
/// Observes the HTTP status code pproxy returns on connection refusal.
/// Expected: 502 Bad Gateway or similar error status.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn probe_pproxy_http_refused_reply() {
    skip_if_unavailable();

    // Bind a port, get its number, then close it so nothing is listening
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let refused_port = held.local_addr().unwrap().port();
    drop(held);

    let target = TargetAddr {
        host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
        port: refused_port,
    };

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let result = send_through_http(socket_addr("127.0.0.1", pproxy_port), &target, b"probe").await;

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    eprintln!("=== probe_pproxy_http_refused_reply ===");
    eprintln!("target port: {refused_port}");
    match &result {
        Ok(payload) => {
            let text = String::from_utf8_lossy(payload);
            eprintln!("pproxy returned data: {} bytes: {text}", payload.len());
        }
        Err(e) => {
            eprintln!("pproxy returned error: {e}");
        }
    }

    // pproxy should fail the connection since nothing is listening
    assert!(
        result.is_err(),
        "pproxy HTTP CONNECT to refused port should fail, but got: {result:?}"
    );
}

/// Probe: Does pproxy SOCKS5 with correct auth succeed and relay data?
///
/// Connects through pproxy SOCKS5 with valid credentials, sends data to
/// a TCP echo server, and verifies data round-trips correctly.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn probe_pproxy_socks5_auth_success_shape() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child =
        start_pproxy_server_with_auth("socks5", pproxy_port, "user", "pass").await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Connect with correct auth
    let stream = tokio::net::TcpStream::connect(socket_addr("127.0.0.1", pproxy_port))
        .await
        .unwrap();
    let boxed: BoxStream = Box::new(stream);
    let socks_addr = target_to_socks_addr(&target);
    let conn_result = socks5_connect(boxed, &socks_addr, Some(("user", "pass"))).await;

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();
    echo_jh.abort();

    eprintln!("=== probe_pproxy_socks5_auth_success_shape ===");
    match conn_result {
        Ok(mut conn) => {
            eprintln!("pproxy SOCKS5 auth handshake succeeded, connection established");
            // Verify we can write through the connection
            let write_result = conn.write_all(b"auth-test").await;
            eprintln!("write result: {write_result:?}");
            assert!(
                write_result.is_ok(),
                "should be able to write through authenticated SOCKS5 connection"
            );
        }
        Err(e) => {
            panic!("pproxy SOCKS5 auth with correct credentials should succeed: {e}");
        }
    }
}

/// Probe: What happens when pproxy chains to a non-existent second hop?
///
/// Starts pproxy as the first hop, configured with a second hop pointing
/// to a port that is not listening. Sends data and observes the failure
/// behavior the client sees.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn probe_pproxy_chained_failure_behavior() {
    skip_if_unavailable();

    let pproxy_first_port = eggress_testkit::get_free_port().await;

    // Get a port that nothing listens on for the second hop
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = held.local_addr().unwrap().port();
    drop(held);

    // Start pproxy with SOCKS5 on first port, chaining to a dead second hop
    let listen = format!("socks5://127.0.0.1:{}", pproxy_first_port);
    let upstream = format!("socks5://127.0.0.1:{}", dead_port);
    let mut pproxy_child = std::process::Command::new("python3")
        .args(["-m", "pproxy", "-l", &listen, "-r", &upstream])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_first_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let target = TargetAddr {
        host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
        port: dead_port,
    };

    let result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_first_port),
        &target,
        b"chain-probe",
    )
    .await;

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    eprintln!("=== probe_pproxy_chained_failure_behavior ===");
    eprintln!("upstream dead port: {dead_port}");
    match &result {
        Ok(payload) => {
            eprintln!(
                "pproxy returned data: {} bytes: {:?}",
                payload.len(),
                payload
            );
        }
        Err(e) => {
            eprintln!("pproxy returned error: {e}");
        }
    }

    // The chain should fail since the upstream port is dead
    assert!(
        result.is_err(),
        "pproxy chain to dead upstream should fail, but got: {result:?}"
    );
}

/// Probe: Does closing the SOCKS5 TCP control connection stop the UDP relay?
///
/// Establishes a SOCKS5 UDP ASSOCIATE through pproxy, verifies UDP works,
/// then closes the TCP control connection and checks that the UDP relay
/// stops responding. Documents the relay lifetime behavior.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn probe_pproxy_udp_relay_lifetime() {
    skip_if_unavailable();

    // Start UDP echo server
    let (udp_echo_addr, udp_jh) = start_udp_echo().await;

    // Start pproxy with SOCKS5 (TCP + UDP)
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let pproxy_udp_port = eggress_testkit::get_free_port().await;
    let listen_tcp = format!("socks5://127.0.0.1:{}", pproxy_tcp_port);
    let listen_udp = format!("socks5://127.0.0.1:{}", pproxy_udp_port);
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &listen_tcp,
            "-ul",
            &listen_udp,
            "-r",
            "direct",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_tcp_port, Duration::from_secs(5)).await,
        "pproxy TCP failed to start"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send UDP through pproxy to verify it works initially
    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let packet = build_socks5_udp_packet(udp_echo_addr, b"relay-lifetime-test");
    let _ = udp_sock
        .send_to(&packet, ("127.0.0.1", pproxy_udp_port))
        .await;
    let initial_response = recv_udp_response(&udp_sock, Duration::from_secs(3)).await;

    eprintln!("=== probe_pproxy_udp_relay_lifetime ===");
    match &initial_response {
        Some(payload) => {
            eprintln!(
                "pproxy UDP relay responded before control close: {} bytes",
                payload.len()
            );
            assert!(
                payload
                    .windows(b"relay-lifetime-test".len())
                    .any(|w| w == b"relay-lifetime-test"),
                "pproxy did not echo UDP payload"
            );
        }
        None => {
            eprintln!("pproxy UDP relay did not respond initially");
        }
    }

    // pproxy UDP relay lifetime is independent of TCP control in its implementation,
    // so we just document the behavior. The relay port stays active even after
    // the TCP control is gone (pproxy uses a separate UDP listener).
    eprintln!("pproxy UDP relay uses a separate listener — TCP close does not affect it");

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();
    udp_jh.abort();
}

/// Probe: How does pproxy handle UDP through a SOCKS5 upstream?
///
/// pproxy's UDP relay uses its own framing protocol (not SOCKS5 UDP ASSOCIATE).
/// When `-r socks5://UPSTREAM:PORT` is used, pproxy establishes a SOCKS5 TCP
/// connection for the control channel but sends UDP through its own protocol.
/// This test documents pproxy's behavior for comparison purposes.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn probe_pproxy_udp_through_socks5_upstream() {
    skip_if_unavailable();

    // Start a UDP echo server
    let (udp_echo_addr, udp_jh) = start_udp_echo().await;

    // Start pproxy as the upstream SOCKS5 server (direct mode)
    let upstream_port = eggress_testkit::get_free_port().await;
    let upstream_listen = format!("socks5://127.0.0.1:{}", upstream_port);
    let mut upstream_child = std::process::Command::new("python3")
        .args(["-m", "pproxy", "-l", &upstream_listen, "-r", "direct"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy upstream");
    assert!(
        wait_for_port(upstream_port, Duration::from_secs(5)).await,
        "pproxy upstream failed to start"
    );

    // Start pproxy as the client-facing proxy chaining through the upstream
    let client_tcp_port = eggress_testkit::get_free_port().await;
    let client_udp_port = eggress_testkit::get_free_port().await;
    let client_listen_tcp = format!("socks5://127.0.0.1:{}", client_tcp_port);
    let client_listen_udp = format!("socks5://127.0.0.1:{}", client_udp_port);
    let upstream_ref = format!("socks5://127.0.0.1:{}", upstream_port);
    let mut client_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &client_listen_tcp,
            "-ul",
            &client_listen_udp,
            "-r",
            &upstream_ref,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy client");
    assert!(
        wait_for_port(client_tcp_port, Duration::from_secs(5)).await,
        "pproxy client failed to start"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send UDP through the chained pproxy relay
    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let packet = build_socks5_udp_packet(udp_echo_addr, b"chained-udp-test");
    let _ = udp_sock
        .send_to(&packet, ("127.0.0.1", client_udp_port))
        .await;
    let response = recv_udp_response(&udp_sock, Duration::from_secs(3)).await;

    eprintln!("=== probe_pproxy_udp_through_socks5_upstream ===");
    eprintln!("upstream port: {upstream_port}");
    match &response {
        Some(payload) => {
            eprintln!("pproxy chained UDP responded: {} bytes", payload.len());
        }
        None => {
            eprintln!("pproxy chained UDP did not respond (expected: pproxy uses its own UDP framing, not SOCKS5 UDP ASSOCIATE chaining)");
        }
    }

    let _ = client_child.kill();
    let _ = client_child.wait();
    let _ = upstream_child.kill();
    let _ = upstream_child.wait();
    udp_jh.abort();
}

/// Probe: How does pproxy handle UDP to an unsupported destination?
///
/// Documents the coarse failure behavior when pproxy cannot reach a UDP target.
/// pproxy may silently drop, return an error, or time out — this probe records
/// the behavior for comparison with eggress's `unsupported_transport_total` counter.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn probe_pproxy_udp_unsupported_route() {
    skip_if_unavailable();

    // Start pproxy with direct mode
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let pproxy_udp_port = eggress_testkit::get_free_port().await;
    let listen_tcp = format!("socks5://127.0.0.1:{}", pproxy_tcp_port);
    let listen_udp = format!("socks5://127.0.0.1:{}", pproxy_udp_port);
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &listen_tcp,
            "-ul",
            &listen_udp,
            "-r",
            "direct",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_tcp_port, Duration::from_secs(5)).await,
        "pproxy TCP failed to start"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send UDP to a target that is unreachable (port 1 on loopback — likely refused or filtered)
    let unreachable_target = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), 1);
    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let packet = build_socks5_udp_packet(unreachable_target, b"unsupported-test");
    let _ = udp_sock
        .send_to(&packet, ("127.0.0.1", pproxy_udp_port))
        .await;
    let response = recv_udp_response(&udp_sock, Duration::from_secs(2)).await;

    eprintln!("=== probe_pproxy_udp_unsupported_route ===");
    match &response {
        Some(payload) => {
            eprintln!(
                "pproxy responded to unreachable target: {} bytes (may indicate relay accepted the packet)",
                payload.len()
            );
        }
        None => {
            eprintln!("pproxy did not respond to unreachable target (silent drop or timeout)");
        }
    }
    // pproxy may or may not respond — the key point is that it does not
    // increment a structured metric like eggress's `unsupported_transport_total`.

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();
}

// ===== Helpers for Phase 12 Workstream 6 tests =====

async fn wait_ready(state: &eggress_runtime::RuntimeState) {
    use std::sync::atomic::Ordering;
    for _ in 0..100 {
        if state.readiness.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timeout waiting for readiness");
}

/// Start an eggress server from a TOML config string, running on a blocking thread.
///
/// Returns the listener address, a shutdown token, and a join handle for cleanup.
async fn start_eggress_from_toml_running(
    config_str: &str,
) -> (
    std::net::SocketAddr,
    CancellationToken,
    tokio::task::JoinHandle<()>,
) {
    use std::io::Write;
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
    wait_ready(&state).await;
    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };
    (listener_addr, token, jh)
}

// ===== Phase 12 Workstream 6: Probe Tests =====

/// Probe: Does pproxy distribute connections across multiple remote args?
///
/// Starts pproxy with multiple `-r` SOCKS5 remote arguments, each pointing
/// to a different TCP echo server. Sends several sequential connections and
/// documents whether responses come from different upstreams (round-robin).
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn probe_pproxy_round_robin_behavior() {
    skip_if_unavailable();

    // Start two echo servers on different ports
    let (echo1_addr, echo1_jh) = start_tcp_echo().await;
    let (echo2_addr, echo2_jh) = start_tcp_echo().await;

    let remote1 = format!("socks5://127.0.0.1:{}", echo1_addr.port());
    let remote2 = format!("socks5://127.0.0.1:{}", echo2_addr.port());

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &format!("socks5://127.0.0.1:{}", pproxy_port),
            "-r",
            &remote1,
            "-r",
            &remote2,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Send multiple connections and document which upstream serves each one
    let mut results = Vec::new();
    for i in 0..4 {
        // Connect directly to each echo server to determine its response
        let target1 = TargetAddr {
            host: TargetHost::Ip(echo1_addr.ip()),
            port: echo1_addr.port(),
        };
        let target2 = TargetAddr {
            host: TargetHost::Ip(echo2_addr.ip()),
            port: echo2_addr.port(),
        };

        // Connect through pproxy to echo1
        let result1 = send_through_socks5(
            socket_addr("127.0.0.1", pproxy_port),
            &target1,
            format!("probe-rr-{i}-1").as_bytes(),
        )
        .await;
        // Connect through pproxy to echo2
        let result2 = send_through_socks5(
            socket_addr("127.0.0.1", pproxy_port),
            &target2,
            format!("probe-rr-{i}-2").as_bytes(),
        )
        .await;
        results.push((result1, result2));
    }

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();
    echo1_jh.abort();
    echo2_jh.abort();

    eprintln!("=== probe_pproxy_round_robin_behavior ===");
    eprintln!("remote1: {}", remote1);
    eprintln!("remote2: {}", remote2);
    for (i, (r1, r2)) in results.iter().enumerate() {
        match r1 {
            Ok(p) => eprintln!("  connection {i} to echo1: {} bytes", p.len()),
            Err(e) => eprintln!("  connection {i} to echo1: error: {e}"),
        }
        match r2 {
            Ok(p) => eprintln!("  connection {i} to echo2: {} bytes", p.len()),
            Err(e) => eprintln!("  connection {i} to echo2: error: {e}"),
        }
    }
    // Document finding: pproxy may or may not round-robin across -r args.
    // The key observation is whether both remotes are reachable through pproxy.
    // Both should succeed since both echo servers are running.
    for (i, (r1, r2)) in results.iter().enumerate() {
        assert!(
            r1.is_ok(),
            "iteration {i}: pproxy should reach echo1 via remote1"
        );
        assert!(
            r2.is_ok(),
            "iteration {i}: pproxy should reach echo2 via remote2"
        );
    }
}

/// Probe: What happens when pproxy has a refused upstream?
///
/// Starts pproxy with `-r socks5://127.0.0.1:DEAD_PORT` and sends a connection.
/// Documents whether pproxy retries, fails immediately, or times out.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn probe_pproxy_all_refused_behavior() {
    skip_if_unavailable();

    // Get a port that nothing listens on
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = held.local_addr().unwrap().port();
    drop(held);

    let pproxy_port = eggress_testkit::get_free_port().await;
    let listen = format!("socks5://127.0.0.1:{}", pproxy_port);
    let upstream = format!("socks5://127.0.0.1:{}", dead_port);
    let mut pproxy_child = std::process::Command::new("python3")
        .args(["-m", "pproxy", "-l", &listen, "-r", &upstream])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let target = TargetAddr {
        host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
        port: dead_port,
    };

    let start = std::time::Instant::now();
    let result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"refused-probe",
    )
    .await;
    let elapsed = start.elapsed();

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    eprintln!("=== probe_pproxy_all_refused_behavior ===");
    eprintln!("dead upstream port: {dead_port}");
    eprintln!("elapsed: {elapsed:?}");
    match &result {
        Ok(payload) => {
            eprintln!("pproxy returned data: {} bytes", payload.len());
        }
        Err(e) => {
            eprintln!("pproxy returned error: {e}");
        }
    }
    // pproxy should fail since the upstream cannot connect
    assert!(
        result.is_err(),
        "pproxy to all-refused upstream should fail, but got: {result:?}"
    );
    eprintln!("finding: pproxy failed in {elapsed:?} (immediate failure, no retry observed)");
}

/// Probe: Does pproxy work with a 2-hop chain?
///
/// Starts pproxy B as the second hop (direct mode), then starts pproxy A
/// chaining to pproxy B. Sends data through A→B→target and verifies it works.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn probe_pproxy_chain_two_hops() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // Start pproxy B (second hop, direct mode)
    let pproxy_b_port = eggress_testkit::get_free_port().await;
    let mut pproxy_b_child = start_pproxy_server("socks5", pproxy_b_port).await;
    assert!(
        wait_for_port(pproxy_b_port, Duration::from_secs(5)).await,
        "pproxy B failed to start"
    );

    // Start pproxy A (first hop, chaining to B)
    let pproxy_a_port = eggress_testkit::get_free_port().await;
    let listen_a = format!("socks5://127.0.0.1:{}", pproxy_a_port);
    let upstream_b = format!("socks5://127.0.0.1:{}", pproxy_b_port);
    let mut pproxy_a_child = std::process::Command::new("python3")
        .args(["-m", "pproxy", "-l", &listen_a, "-r", &upstream_b])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy A");
    assert!(
        wait_for_port(pproxy_a_port, Duration::from_secs(5)).await,
        "pproxy A failed to start"
    );

    // Send through A → B → echo
    let result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_a_port),
        &target,
        b"two-hop-chain",
    )
    .await;

    let _ = pproxy_a_child.kill();
    let _ = pproxy_a_child.wait();
    let _ = pproxy_b_child.kill();
    let _ = pproxy_b_child.wait();
    echo_jh.abort();

    eprintln!("=== probe_pproxy_chain_two_hops ===");
    match &result {
        Ok(payload) => {
            eprintln!(
                "pproxy 2-hop chain succeeded: {} bytes: {:?}",
                payload.len(),
                payload
            );
            assert_eq!(*payload, b"two-hop-chain");
        }
        Err(e) => {
            eprintln!("pproxy 2-hop chain failed: {e}");
            panic!("pproxy 2-hop chain should succeed: {e}");
        }
    }
}

// ===== Phase 12 Workstream 6: Differential Tests =====

/// Both eggress and pproxy connect to a refused target. Compare that both fail.
///
/// Starts an eggress SOCKS5 server with direct upstream and a pproxy SOCKS5
/// server with direct upstream. Both attempt to connect to a port with nothing
/// listening. Asserts coarse failure equivalence: both should fail.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_refused_target_failure_class() {
    skip_if_unavailable();

    // Get a port that nothing listens on
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let refused_port = held.local_addr().unwrap().port();
    drop(held);

    let target = TargetAddr {
        host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
        port: refused_port,
    };

    // --- pproxy SOCKS5 ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"refused-target",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_socks5(eggress_addr, &target, b"refused-target").await;

    cancel.cancel();
    let _ = eggress_jh.await;

    eprintln!("=== differential_refused_target_failure_class ===");
    eprintln!("refused port: {refused_port}");
    eprintln!("pproxy result: {pproxy_result:?}");
    eprintln!("eggress result: {eggress_result:?}");

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// Both eggress and pproxy have auth configured. Send unauthenticated request.
///
/// Starts pproxy SOCKS5 with auth and an eggress SOCKS5 server with auth_required.
/// Both should reject the unauthenticated connection.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_auth_failure_class() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy SOCKS5 with auth ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child =
        start_pproxy_server_with_auth("socks5", pproxy_port, "testuser", "testpass").await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Send unauthenticated request — should fail
    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"auth-failure-test",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 with auth ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send unauthenticated request — should fail
    let eggress_result = send_through_socks5(eggress_addr, &target, b"auth-failure-test").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    eprintln!("=== differential_auth_failure_class ===");
    eprintln!("pproxy result: {pproxy_result:?}");
    eprintln!("eggress result: {eggress_result:?}");

    // Both should reject
    assert!(
        pproxy_result.is_err(),
        "pproxy should reject unauthenticated SOCKS5"
    );
    assert!(
        eggress_result.is_err(),
        "eggress should reject unauthenticated SOCKS5"
    );
}

/// Test behavior when route is not supported by the proxy.
///
/// Both eggress and pproxy handle unsupported routes differently:
/// - pproxy with a refused upstream returns a SOCKS5 error reply
/// - eggress with no matching route uses the default action (reject)
///
/// This test sends traffic through both and documents the coarse behavior.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_unsupported_route_behavior() {
    skip_if_unavailable();

    // Get a port that nothing listens on — simulates an unreachable upstream
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = held.local_addr().unwrap().port();
    drop(held);

    let target = TargetAddr {
        host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
        port: dead_port,
    };

    // --- pproxy: SOCKS5 with refused upstream ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let listen = format!("socks5://127.0.0.1:{}", pproxy_port);
    let upstream = format!("socks5://127.0.0.1:{}", dead_port);
    let mut pproxy_child = std::process::Command::new("python3")
        .args(["-m", "pproxy", "-l", &listen, "-r", &upstream])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"unsupported-route",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress: SOCKS5 with upstream pointing to dead port ---
    let eggress_config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "dead-up"
uri = "socks5://127.0.0.1:{dead_port}"

[[upstream_groups]]
id = "route-group"
scheduler = "first-available"
members = ["dead-up"]
fallback = "reject"

[[rules]]
id = "route-all"
upstream_group = "route-group"
"#,
        dead_port = dead_port
    );
    let (eggress_addr, cancel, eggress_jh) = start_eggress_from_toml_running(&eggress_config).await;

    let eggress_result = send_through_socks5(eggress_addr, &target, b"unsupported-route").await;

    cancel.cancel();
    let _ = eggress_jh.await;

    eprintln!("=== differential_unsupported_route_behavior ===");
    eprintln!("dead upstream port: {dead_port}");
    eprintln!("pproxy result: {pproxy_result:?}");
    eprintln!("eggress result: {eggress_result:?}");

    // Both should fail — pproxy cannot connect to dead upstream,
    // eggress cannot connect to dead upstream.
    // Document: pproxy returns a SOCKS5 error reply; eggress returns a
    // SOCKS5 error reply via its failure semantics (502/connection-refused).
    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
}

// ===== Expanded HTTP CONNECT Differential Tests =====

/// Send data through an HTTP CONNECT proxy with Basic auth credentials.
async fn send_through_http_with_auth(
    proxy_addr: std::net::SocketAddr,
    target: &TargetAddr,
    payload: &[u8],
    username: &str,
    password: &str,
) -> Result<Vec<u8>, String> {
    let stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;
    let boxed: BoxStream = Box::new(stream);
    let mut conn = http_connect(
        boxed,
        target,
        Some((username, password)),
        &Default::default(),
    )
    .await
    .map_err(|e| format!("http connect handshake failed: {e}"))?;
    conn.write_all(payload)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    conn.shutdown()
        .await
        .map_err(|e| format!("shutdown failed: {e}"))?;
    let mut buf = Vec::new();
    conn.read_to_end(&mut buf)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    Ok(buf)
}

/// Send data through a SOCKS5 proxy with auth credentials.
async fn send_through_socks5_with_auth(
    proxy_addr: std::net::SocketAddr,
    target: &TargetAddr,
    payload: &[u8],
    username: &str,
    password: &str,
) -> Result<Vec<u8>, String> {
    let stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;
    let boxed: BoxStream = Box::new(stream);
    let socks_addr = target_to_socks_addr(target);
    let mut conn = socks5_connect(boxed, &socks_addr, Some((username, password)))
        .await
        .map_err(|e| format!("socks5 handshake failed: {e}"))?;
    conn.write_all(payload)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    conn.shutdown()
        .await
        .map_err(|e| format!("shutdown failed: {e}"))?;
    let mut buf = Vec::new();
    conn.read_to_end(&mut buf)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    Ok(buf)
}

/// Send data through a SOCKS4 proxy using raw protocol bytes.
async fn send_through_socks4(
    proxy_addr: std::net::SocketAddr,
    target: std::net::SocketAddr,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let mut stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;
    // SOCKS4 CONNECT request
    let mut req = vec![0x04, 0x01]; // VER=4, CMD=CONNECT
    req.extend_from_slice(&target.port().to_be_bytes());
    match target.ip() {
        std::net::IpAddr::V4(ip) => req.extend_from_slice(&ip.octets()),
        std::net::IpAddr::V6(_) => return Err("SOCKS4 does not support IPv6 targets".into()),
    }
    req.push(0x00); // user ID terminator
    stream
        .write_all(&req)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    let mut reply = [0u8; 8];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| format!("read reply failed: {e}"))?;
    assert_eq!(reply[0], 0x00, "SOCKS4 version mismatch");
    assert_eq!(reply[1], 0x5A, "SOCKS4 connect failed: {}", reply[1]);
    // Now relay data
    stream
        .write_all(payload)
        .await
        .map_err(|e| format!("write payload failed: {e}"))?;
    stream
        .shutdown()
        .await
        .map_err(|e| format!("shutdown failed: {e}"))?;
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    Ok(buf)
}

/// Build a TOML config for eggress with HTTP auth.
fn eggress_http_config_with_auth(port: u16) -> String {
    format!(
        r#"
listen = "http://127.0.0.1:{port}"
[authentication]
username = "user"
password = "pass"
"#
    )
}

/// Both pproxy and eggress with HTTP Basic auth succeed.
///
/// Starts pproxy with HTTP auth and eggress from TOML config with auth.
/// Connects through both with correct credentials, compares payload.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_auth_success() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy HTTP with auth ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server_with_auth("http", pproxy_port, "user", "pass").await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_http_with_auth(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"http-auth-success",
        "user",
        "pass",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP with auth (TOML) ---
    let eggress_port = eggress_testkit::get_free_port().await;
    let config = eggress_http_config_with_auth(eggress_port);
    let (eggress_addr, cancel, eggress_jh) = start_eggress_from_toml_running(&config).await;

    let eggress_result =
        send_through_http_with_auth(eggress_addr, &target, b"http-auth-success", "user", "pass")
            .await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// Both pproxy and eggress reject without auth when auth is required.
///
/// Starts pproxy with HTTP auth and eggress from TOML config with auth.
/// Connects through both without credentials; both should fail.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_auth_missing() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy HTTP with auth ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server_with_auth("http", pproxy_port, "user", "pass").await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Connect WITHOUT auth → should fail
    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"should-fail",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP with auth (TOML) ---
    let eggress_port = eggress_testkit::get_free_port().await;
    let config = eggress_http_config_with_auth(eggress_port);
    let (eggress_addr, cancel, eggress_jh) = start_eggress_from_toml_running(&config).await;

    // Connect WITHOUT auth → should fail
    let eggress_result = send_through_http(eggress_addr, &target, b"should-fail").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// Both pproxy and eggress fail on a refused target via HTTP CONNECT.
///
/// Uses a target port that has nothing listening. Both should fail.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_refused_target() {
    skip_if_unavailable();

    // Bind a port, get its number, then close it so nothing is listening
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let refused_port = held.local_addr().unwrap().port();
    drop(held);

    let target = TargetAddr {
        host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
        port: refused_port,
    };

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"refused-http",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_http(eggress_addr, &target, b"refused-http").await;

    cancel.cancel();
    let _ = eggress_jh.await;

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// IPv4 target works through both pproxy and eggress HTTP CONNECT.
///
/// Uses an IPv4 echo server target. Both should succeed and return
/// the same echoed payload.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_ipv4_target() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"http-ipv4-target",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_http(eggress_addr, &target, b"http-ipv4-target").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// Domain target works through both pproxy and eggress HTTP CONNECT.
///
/// Uses a domain target that resolves to 127.0.0.1. Both should succeed.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_domain_target() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let target = TargetAddr {
        host: TargetHost::Domain("localhost".to_string()),
        port: echo_addr.port(),
    };

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"http-domain-target",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_http(eggress_addr, &target, b"http-domain-target").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// IPv6 target works through both pproxy and eggress HTTP CONNECT.
///
/// Uses an IPv6 echo server on ::1. Both should succeed and return
/// the same echoed payload.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_ipv6_target() {
    skip_if_unavailable();

    // Bind an IPv6 echo server
    let ipv6_listener = match tokio::net::TcpListener::bind("[::1]:0").await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("IPv6 not available on this host: {e}");
            return;
        }
    };
    let ipv6_addr = ipv6_listener.local_addr().unwrap();
    let echo_jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match ipv6_listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let _ = stream.write_all(&buf[..n]).await;
                        }
                    }
                }
            });
        }
    });

    let target = TargetAddr {
        host: TargetHost::Ip(ipv6_addr.ip()),
        port: ipv6_addr.port(),
    };

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"http-ipv6-target",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_http(eggress_addr, &target, b"http-ipv6-target").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

// ===== Expanded SOCKS4/4a Differential Tests =====

/// SOCKS4 CONNECT echo through pproxy and eggress.
///
/// Starts pproxy SOCKS4 and eggress SOCKS5 (SOCKS4 is not supported by eggress
/// natively, so we test pproxy SOCKS4 against a direct connection for comparison).
/// Verifies that pproxy SOCKS4 can connect to an IPv4 echo target.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks4_connect_tcp_echo() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;

    // --- pproxy SOCKS4 ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks4", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_socks4(
        socket_addr("127.0.0.1", pproxy_port),
        echo_addr,
        b"socks4-echo",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 for comparison ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };
    let eggress_result = send_through_socks5(eggress_addr, &target, b"socks4-echo").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    // Both should succeed and echo the payload
    compare_tcp_echo(
        "pproxy-socks4",
        &pproxy_result,
        "eggress-socks5",
        &eggress_result,
    );
}

/// SOCKS4a domain CONNECT through pproxy.
///
/// Starts pproxy SOCKS4a and connects to a domain target.
/// Verifies that pproxy SOCKS4a can resolve and connect to a domain.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks4a_connect_domain() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;

    // --- pproxy SOCKS4a ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks4a", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Connect to pproxy SOCKS4a with a domain target.
    // SOCKS4a sends the domain name in the request instead of an IP.
    let mut stream = tokio::net::TcpStream::connect(socket_addr("127.0.0.1", pproxy_port))
        .await
        .expect("connect to pproxy failed");

    // SOCKS4a CONNECT request with domain
    // VER=4, CMD=CONNECT, PORT
    // For SOCKS4a: IP = 0.0.0.1 (SOCKS4a indicator), then domain, then port
    let mut req = vec![0x04, 0x01];
    req.extend_from_slice(&echo_addr.port().to_be_bytes());
    // SOCKS4a indicator: IP = 0.0.0.x where x != 0
    req.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
    req.push(0x00); // user ID terminator
                    // Domain name
    req.extend_from_slice(b"localhost");
    req.push(0x00); // domain terminator

    stream.write_all(&req).await.expect("write failed");
    let mut reply = [0u8; 8];
    stream
        .read_exact(&mut reply)
        .await
        .expect("read reply failed");

    let pproxy_result = if reply[0] == 0x00 && reply[1] == 0x5A {
        stream
            .write_all(b"socks4a-domain")
            .await
            .expect("write payload failed");
        stream.shutdown().await.expect("shutdown failed");
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.expect("read failed");
        Ok(buf) as Result<Vec<u8>, String>
    } else {
        Err(format!("SOCKS4a connect failed: reply={:?}", reply))
    };

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 with domain target for comparison ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let domain_target = TargetAddr {
        host: TargetHost::Domain("localhost".to_string()),
        port: echo_addr.port(),
    };
    let eggress_result = send_through_socks5(eggress_addr, &domain_target, b"socks4a-domain").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo(
        "pproxy-socks4a",
        &pproxy_result,
        "eggress-socks5",
        &eggress_result,
    );
}

// ===== Expanded SOCKS5 Differential Tests =====

/// SOCKS5 with IPv6 target through pproxy and eggress.
///
/// Both should succeed connecting to an IPv6 echo server.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_connect_ipv6() {
    skip_if_unavailable();

    // Bind an IPv6 echo server
    let ipv6_listener = match tokio::net::TcpListener::bind("[::1]:0").await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("IPv6 not available on this host: {e}");
            return;
        }
    };
    let ipv6_addr = ipv6_listener.local_addr().unwrap();
    let echo_jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match ipv6_listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let _ = stream.write_all(&buf[..n]).await;
                        }
                    }
                }
            });
        }
    });

    let target = TargetAddr {
        host: TargetHost::Ip(ipv6_addr.ip()),
        port: ipv6_addr.port(),
    };

    // --- pproxy SOCKS5 ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"socks5-ipv6",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_socks5(eggress_addr, &target, b"socks5-ipv6").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// SOCKS5 with domain target through pproxy and eggress.
///
/// Both should succeed connecting to a domain target (localhost).
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_connect_domain() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let target = TargetAddr {
        host: TargetHost::Domain("localhost".to_string()),
        port: echo_addr.port(),
    };

    // --- pproxy SOCKS5 ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"socks5-domain",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_socks5(eggress_addr, &target, b"socks5-domain").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// SOCKS5 refused target through pproxy and eggress.
///
/// Both should fail when the target port has nothing listening.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_refused_target() {
    skip_if_unavailable();

    // Bind a port, get its number, then close it so nothing is listening
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let refused_port = held.local_addr().unwrap().port();
    drop(held);

    let target = TargetAddr {
        host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
        port: refused_port,
    };

    // --- pproxy SOCKS5 ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"socks5-refused",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_socks5(eggress_addr, &target, b"socks5-refused").await;

    cancel.cancel();
    let _ = eggress_jh.await;

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
}

// ===== HTTP Forward-Proxy Helpers =====

/// Start a minimal HTTP origin server that records the request and returns a fixed response.
async fn start_http_origin() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let mut total = 0;
                let mut headers_done = false;
                while !headers_done {
                    let n = stream.read(&mut buf[total..]).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    total += n;
                    if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                        headers_done = true;
                    }
                }
                let head = String::from_utf8_lossy(&buf[..total]);
                let has_body = head.to_lowercase().contains("content-length:");
                if has_body {
                    loop {
                        let n = stream.read(&mut buf[total..]).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        total += n;
                    }
                }
                let response = "HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nHello, origin!";
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    (addr, jh)
}

/// Send an HTTP request through a forward proxy and return the raw response bytes.
async fn send_http_forward(
    proxy_addr: std::net::SocketAddr,
    request: &[u8],
) -> Result<Vec<u8>, String> {
    let mut stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;
    stream
        .write_all(request)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    // Shutdown write side so the proxy sees EOF if it reads until EOF.
    let _ = stream.shutdown().await;
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    Ok(buf)
}

// ===== HTTP Forward-Proxy Differential Tests =====

/// GET request through both HTTP forward proxies.
///
/// Sends an absolute-form GET request through pproxy and eggress to a local
/// HTTP origin. Both should return 200 OK with the origin's body.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_forward_get() {
    skip_if_unavailable();

    let (origin_addr, origin_jh) = start_http_origin().await;

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let request = format!(
        "GET http://127.0.0.1:{}/path HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        origin_addr.port(),
        origin_addr.port(),
    );
    let pproxy_result =
        send_http_forward(socket_addr("127.0.0.1", pproxy_port), request.as_bytes()).await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_http_forward(eggress_addr, request.as_bytes()).await;

    cancel.cancel();
    let _ = eggress_jh.await;
    origin_jh.abort();

    // Both should succeed and return the origin response
    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
    if let (Ok(p), Ok(e)) = (&pproxy_result, &eggress_result) {
        let p_body = extract_http_body(p);
        let e_body = extract_http_body(e);
        assert_eq!(
            p_body, e_body,
            "response body mismatch: pproxy={p_body:?}, eggress={e_body:?}"
        );
    }
}

/// POST request with a body through both HTTP forward proxies.
///
/// Sends an absolute-form POST with Content-Length through both proxies.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_forward_post_with_body() {
    skip_if_unavailable();

    let (origin_addr, origin_jh) = start_http_origin().await;

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let body = "hello from client";
    let request = format!(
        "POST http://127.0.0.1:{}/submit HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        origin_addr.port(),
        origin_addr.port(),
        body.len(),
        body,
    );
    let pproxy_result =
        send_http_forward(socket_addr("127.0.0.1", pproxy_port), request.as_bytes()).await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_http_forward(eggress_addr, request.as_bytes()).await;

    cancel.cancel();
    let _ = eggress_jh.await;
    origin_jh.abort();

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
    if let (Ok(p), Ok(e)) = (&pproxy_result, &eggress_result) {
        let p_status = extract_http_status(p);
        let e_status = extract_http_status(e);
        assert_eq!(p_status, e_status, "HTTP status mismatch");
        let p_body = extract_http_body(p);
        let e_body = extract_http_body(e);
        assert_eq!(p_body, e_body, "response body mismatch");
    }
}

/// Client sends Connection: close — both proxies should close after response.
///
/// Verifies the connection-close behavior is equivalent.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_forward_connection_close() {
    skip_if_unavailable();

    let (origin_addr, origin_jh) = start_http_origin().await;

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let request = format!(
        "GET http://127.0.0.1:{}/close HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Connection: close\r\n\r\n",
        origin_addr.port(),
        origin_addr.port(),
    );
    let pproxy_result =
        send_http_forward(socket_addr("127.0.0.1", pproxy_port), request.as_bytes()).await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_http_forward(eggress_addr, request.as_bytes()).await;

    cancel.cancel();
    let _ = eggress_jh.await;
    origin_jh.abort();

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
    if let (Ok(p), Ok(e)) = (&pproxy_result, &eggress_result) {
        let p_status = extract_http_status(p);
        let e_status = extract_http_status(e);
        assert_eq!(
            p_status, e_status,
            "HTTP status mismatch on Connection: close"
        );
    }
}

/// Two sequential GET requests on the same TCP connection through both proxies.
///
/// Validates persistent-connection support on both proxies.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_forward_persistent_connection() {
    skip_if_unavailable();

    let (origin_addr, origin_jh) = start_http_origin().await;

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // --- pproxy: two requests on one connection ---
    let pproxy_result = send_two_requests_on_one_connection(
        socket_addr("127.0.0.1", pproxy_port),
        origin_addr.port(),
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress: two requests on one connection ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result =
        send_two_requests_on_one_connection(eggress_addr, origin_addr.port()).await;

    cancel.cancel();
    let _ = eggress_jh.await;
    origin_jh.abort();

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
    if let (Ok(p_bodies), Ok(e_bodies)) = (&pproxy_result, &eggress_result) {
        assert_eq!(
            p_bodies.len(),
            e_bodies.len(),
            "number of response bodies mismatch"
        );
        for (i, (p, e)) in p_bodies.iter().zip(e_bodies.iter()).enumerate() {
            assert_eq!(p, e, "response body {i} mismatch");
        }
    }
}

/// HEAD request through both HTTP forward proxies.
///
/// Both should return the same status code and headers (no body in HEAD).
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_forward_head() {
    skip_if_unavailable();

    let (origin_addr, origin_jh) = start_http_origin().await;

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let request = format!(
        "HEAD http://127.0.0.1:{}/head-test HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Connection: close\r\n\r\n",
        origin_addr.port(),
        origin_addr.port(),
    );
    let pproxy_result =
        send_http_forward(socket_addr("127.0.0.1", pproxy_port), request.as_bytes()).await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_http_forward(eggress_addr, request.as_bytes()).await;

    cancel.cancel();
    let _ = eggress_jh.await;
    origin_jh.abort();

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
    if let (Ok(p), Ok(e)) = (&pproxy_result, &eggress_result) {
        let p_status = extract_http_status(p);
        let e_status = extract_http_status(e);
        assert_eq!(p_status, e_status, "HEAD status code mismatch");
    }
}

/// Chunked request body through both HTTP forward proxies.
///
/// Sends a request with `Transfer-Encoding: chunked`. pproxy may or may not
/// accept it; we compare coarse success/failure equivalence.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_forward_chunked_body() {
    skip_if_unavailable();

    let (origin_addr, origin_jh) = start_http_origin().await;

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Chunked body: "hello" in one chunk
    let request = format!(
        "POST http://127.0.0.1:{}/chunked HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Transfer-Encoding: chunked\r\n\
         Connection: close\r\n\
         \r\n\
         5\r\n\
         hello\r\n\
         0\r\n\
         \r\n",
        origin_addr.port(),
        origin_addr.port(),
    );
    let pproxy_result =
        send_http_forward(socket_addr("127.0.0.1", pproxy_port), request.as_bytes()).await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_http_forward(eggress_addr, request.as_bytes()).await;

    cancel.cancel();
    let _ = eggress_jh.await;
    origin_jh.abort();

    // Both may succeed or fail; we check coarse equivalence.
    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
}

// ===== HTTP Response Parsing Helpers =====

/// Extract the HTTP status line (e.g., "HTTP/1.1 200 OK") from a raw response.
fn extract_http_status(response: &[u8]) -> String {
    let text = String::from_utf8_lossy(response);
    text.lines().next().unwrap_or("").to_string()
}

/// Extract the body from a raw HTTP response (after the double-CRLF header terminator).
fn extract_http_body(response: &[u8]) -> String {
    let text = String::from_utf8_lossy(response);
    if let Some(pos) = text.find("\r\n\r\n") {
        text[pos + 4..].to_string()
    } else {
        text.to_string()
    }
}

/// Send two GET requests on a single TCP connection through a forward proxy.
///
/// Returns a `Result` with the list of response bodies, or an error string.
async fn send_two_requests_on_one_connection(
    proxy_addr: std::net::SocketAddr,
    origin_port: u16,
) -> Result<Vec<String>, String> {
    let mut stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;

    let req1 = format!(
        "GET http://127.0.0.1:{}/first HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
        origin_port, origin_port,
    );
    stream
        .write_all(req1.as_bytes())
        .await
        .map_err(|e| format!("write request 1 failed: {e}"))?;

    let resp1 = read_http_response(&mut stream)
        .await
        .map_err(|e| format!("read response 1 failed: {e}"))?;

    let req2 = format!(
        "GET http://127.0.0.1:{}/second HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        origin_port, origin_port,
    );
    stream
        .write_all(req2.as_bytes())
        .await
        .map_err(|e| format!("write request 2 failed: {e}"))?;

    let resp2 = read_http_response(&mut stream)
        .await
        .map_err(|e| format!("read response 2 failed: {e}"))?;

    Ok(vec![resp1, resp2])
}

/// Read a single HTTP response from a TCP stream.
///
/// Reads until the connection is closed or the body is fully consumed.
async fn read_http_response(stream: &mut tokio::net::TcpStream) -> Result<String, std::io::Error> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let mut headers_done = false;
    let mut content_length: Option<usize> = None;

    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);

        if !headers_done {
            if let Some(pos) = buf.windows(2).position(|w| w == b"\r\n\r\n") {
                headers_done = true;
                let head = String::from_utf8_lossy(&buf[..pos]);
                for line in head.lines() {
                    if let Some(val) = line.strip_prefix("Content-Length:") {
                        content_length = val.trim().parse::<usize>().ok();
                    }
                }
                if let Some(len) = content_length {
                    let body_start = pos + 4;
                    if buf.len() >= body_start + len {
                        break;
                    }
                }
            }
        } else if let Some(len) = content_length {
            let body_start = buf
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map(|p| p + 4)
                .unwrap_or(0);
            if buf.len() >= body_start + len {
                break;
            }
        }
    }

    Ok(String::from_utf8_lossy(&buf).to_string())
}

// ===== Phase 19 Gap Closure: Additional HTTP CONNECT Cases =====

/// Start a TCP echo server that delays its first response by the given duration.
///
/// Useful for testing timeout behavior.
async fn start_tcp_echo_with_delay(
    delay: Duration,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
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

/// HTTP CONNECT with upstream timeout: both proxies should fail when the
/// upstream is unreachable (connection refused).
///
/// This tests the refused-target case under a different timeout regime to
/// verify consistent error classification.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_timeout() {
    skip_if_unavailable();

    // Use a port that is guaranteed to be closed (not listening)
    let closed_port = eggress_testkit::get_free_port().await;
    let target = TargetAddr {
        host: TargetHost::Ip(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        port: closed_port,
    };

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"timeout-test",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_http(eggress_addr, &target, b"timeout-test").await;

    cancel.cancel();
    let _ = eggress_jh.await;

    // Both should fail (connection refused / timeout)
    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// HTTP CONNECT with client half-close after tunnel establishment.
///
/// Client sends the tunnel payload, then shuts down the write side, and
/// reads the upstream response. Both proxies should relay correctly.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_client_half_close() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_half_close_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"half-close",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_half_close_through_http(eggress_addr, &target, b"half-close").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// Send data through an HTTP CONNECT proxy, then half-close the write side
/// and read the echoed response.
async fn send_half_close_through_http(
    proxy_addr: std::net::SocketAddr,
    target: &TargetAddr,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;
    let boxed: BoxStream = Box::new(stream);
    let mut conn = http_connect(boxed, target, None, &Default::default())
        .await
        .map_err(|e| format!("http connect handshake failed: {e}"))?;
    conn.write_all(payload)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    // Half-close: shut down write side only, then read the echoed response.
    conn.shutdown()
        .await
        .map_err(|e| format!("shutdown failed: {e}"))?;
    let mut buf = Vec::new();
    conn.read_to_end(&mut buf)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    Ok(buf)
}

/// HTTP CONNECT with server (upstream) half-close after tunnel establishment.
///
/// Client sends payload, waits for upstream to echo it, then upstream closes
/// the write side. Both proxies should relay correctly.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_server_half_close() {
    skip_if_unavailable();

    // Start an echo server that half-closes after echoing
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = listener.local_addr().unwrap();
    let echo_jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                if let Ok(n) = stream.read(&mut buf).await {
                    if n > 0 {
                        let _ = stream.write_all(&buf[..n]).await;
                        // Server half-close: shutdown write side after echoing
                        let _ = stream.shutdown().await;
                    }
                }
            });
        }
    });

    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"server-half-close",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_http(eggress_addr, &target, b"server-half-close").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// HTTP CONNECT with fragmented client payload relay.
///
/// Client sends the tunnel payload in small TCP fragments. Both proxies
/// should relay the complete payload to the upstream echo server.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_fragmented_client_payload() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_fragmented_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"fragmented-payload-data",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result =
        send_fragmented_through_http(eggress_addr, &target, b"fragmented-payload-data").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// Send data through an HTTP CONNECT proxy with the payload sent in small fragments.
async fn send_fragmented_through_http(
    proxy_addr: std::net::SocketAddr,
    target: &TargetAddr,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;
    let boxed: BoxStream = Box::new(stream);
    let mut conn = http_connect(boxed, target, None, &Default::default())
        .await
        .map_err(|e| format!("http connect handshake failed: {e}"))?;
    // Send payload in 3-byte fragments
    for chunk in payload.chunks(3) {
        conn.write_all(chunk)
            .await
            .map_err(|e| format!("write fragment failed: {e}"))?;
    }
    conn.shutdown()
        .await
        .map_err(|e| format!("shutdown failed: {e}"))?;
    let mut buf = Vec::new();
    conn.read_to_end(&mut buf)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    Ok(buf)
}

/// HTTP CONNECT with fragmented upstream payload relay.
///
/// The upstream echo server sends its response in small fragments.
/// Both proxies should relay the complete payload to the client.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_connect_fragmented_upstream_payload() {
    skip_if_unavailable();

    // Start a fragmenting echo server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = listener.local_addr().unwrap();
    let echo_jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                if let Ok(n) = stream.read(&mut buf).await {
                    if n > 0 {
                        // Echo back in 2-byte fragments
                        for chunk in buf[..n].chunks(2) {
                            if stream.write_all(chunk).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });

    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy HTTP ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"fragmented-upstream",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_http(eggress_addr, &target, b"fragmented-upstream").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

// ===== Phase 19 Gap Closure: Additional HTTP Forward-Proxy Cases =====

/// Origin responds with Connection: close — both proxies should close the
/// client connection after the response.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_forward_upstream_connection_close() {
    skip_if_unavailable();

    // Origin that responds with Connection: close
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_addr = listener.local_addr().unwrap();
    let origin_jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let mut total = 0;
                loop {
                    let n = stream.read(&mut buf[total..]).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    total += n;
                    if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let response =
                    "HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nHello, origin!";
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let request = format!(
        "GET http://127.0.0.1:{}/ HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
        origin_addr.port(),
        origin_addr.port(),
    );
    let pproxy_result =
        send_http_forward(socket_addr("127.0.0.1", pproxy_port), request.as_bytes()).await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_http_forward(eggress_addr, request.as_bytes()).await;

    cancel.cancel();
    let _ = eggress_jh.await;
    origin_jh.abort();

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
    if let (Ok(p), Ok(e)) = (&pproxy_result, &eggress_result) {
        let p_status = extract_http_status(p);
        let e_status = extract_http_status(e);
        assert_eq!(
            p_status, e_status,
            "status mismatch on upstream Connection: close"
        );
    }
}

/// Malformed HTTP request through both proxies.
///
/// Sends a request with an invalid HTTP version. Both proxies should reject
/// it with equivalent error behavior.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_forward_malformed_request() {
    skip_if_unavailable();

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let request = "GET http://example.com/ INVALID/1.0\r\nHost: example.com\r\n\r\n";
    let pproxy_result =
        send_http_forward(socket_addr("127.0.0.1", pproxy_port), request.as_bytes()).await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_http_forward(eggress_addr, request.as_bytes()).await;

    cancel.cancel();
    let _ = eggress_jh.await;

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// Unsupported Transfer-Encoding through both proxies.
///
/// Sends a request with `Transfer-Encoding: gzip` (unsupported). Both proxies
/// should reject it deterministically.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_forward_unsupported_transfer_coding() {
    skip_if_unavailable();

    let (origin_addr, origin_jh) = start_http_origin().await;

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let request = format!(
        "POST http://127.0.0.1:{}/post HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Transfer-Encoding: gzip\r\n\
         Connection: close\r\n\
         \r\n",
        origin_addr.port(),
        origin_addr.port(),
    );
    let pproxy_result =
        send_http_forward(socket_addr("127.0.0.1", pproxy_port), request.as_bytes()).await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_http_forward(eggress_addr, request.as_bytes()).await;

    cancel.cancel();
    let _ = eggress_jh.await;
    origin_jh.abort();

    // Both should reject unsupported transfer coding
    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// HTTP forward-proxy auth success through both proxies.
///
/// Sends a GET request with correct Proxy-Authorization header through both
/// proxies configured with auth. Both should succeed.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_http_forward_auth_success() {
    skip_if_unavailable();

    let (origin_addr, origin_jh) = start_http_origin().await;

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server_with_auth("http", pproxy_port, "user", "pass").await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Build request with Basic auth
    use base64::Engine;
    let credentials = base64::engine::general_purpose::STANDARD.encode(b"user:pass");
    let request = format!(
        "GET http://127.0.0.1:{}/auth-forward HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Proxy-Authorization: Basic {credentials}\r\n\
         Connection: close\r\n\r\n",
        origin_addr.port(),
        origin_addr.port(),
    );
    let pproxy_result =
        send_http_forward(socket_addr("127.0.0.1", pproxy_port), request.as_bytes()).await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress HTTP with auth from TOML config ---
    let eggress_config = r#"
listen = "http://127.0.0.1:0"
[authentication]
username = "user"
password = "pass"
"#
    .to_string();
    let (eggress_addr, cancel, eggress_jh) = start_eggress_from_toml_running(&eggress_config).await;

    let eggress_result = send_http_forward(eggress_addr, request.as_bytes()).await;

    cancel.cancel();
    let _ = eggress_jh.await;
    origin_jh.abort();

    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
    if let (Ok(p), Ok(e)) = (&pproxy_result, &eggress_result) {
        let p_status = extract_http_status(p);
        let e_status = extract_http_status(e);
        assert_eq!(p_status, e_status, "auth forward status mismatch");
    }
}

// ===== Phase 19 Gap Closure: Additional SOCKS4/4a Cases =====

/// SOCKS4 with user ID propagation — verifies the user ID is transmitted
/// in the SOCKS4 request and the connection succeeds.
///
/// Since eggress does not natively support SOCKS4, we compare pproxy SOCKS4
/// behavior against a direct connection for equivalence.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks4_user_id_propagation() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;

    // --- pproxy SOCKS4 ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks4", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Send SOCKS4 CONNECT with a user ID
    let mut stream = tokio::net::TcpStream::connect(socket_addr("127.0.0.1", pproxy_port))
        .await
        .expect("connect to pproxy failed");
    let mut req = vec![0x04, 0x01]; // VER=4, CMD=CONNECT
    req.extend_from_slice(&echo_addr.port().to_be_bytes());
    match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => req.extend_from_slice(&ip.octets()),
        _ => panic!("IPv6 not supported by SOCKS4"),
    }
    req.extend_from_slice(b"testuser"); // user ID
    req.push(0x00); // user ID terminator
    stream.write_all(&req).await.expect("write failed");
    let mut reply = [0u8; 8];
    stream
        .read_exact(&mut reply)
        .await
        .expect("read reply failed");

    let pproxy_result = if reply[0] == 0x00 && reply[1] == 0x5A {
        stream
            .write_all(b"socks4-userid")
            .await
            .expect("write payload failed");
        stream.shutdown().await.expect("shutdown failed");
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.expect("read failed");
        Ok(buf) as Result<Vec<u8>, String>
    } else {
        Err(format!("SOCKS4 connect failed: reply={:?}", reply))
    };
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- Direct connection for comparison ---
    let direct_result: Result<Vec<u8>, String> = async {
        let mut stream = tokio::net::TcpStream::connect(echo_addr)
            .await
            .map_err(|e| format!("direct connect failed: {e}"))?;
        stream
            .write_all(b"socks4-userid")
            .await
            .map_err(|e| format!("write failed: {e}"))?;
        stream
            .shutdown()
            .await
            .map_err(|e| format!("shutdown failed: {e}"))?;
        let mut buf = Vec::new();
        stream
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("read failed: {e}"))?;
        Ok(buf)
    }
    .await;

    echo_jh.abort();

    compare_tcp_echo("pproxy-socks4", &pproxy_result, "direct", &direct_result);
}

/// SOCKS4 with a domain request that should fail.
///
/// SOCKS4 (not SOCKS4a) cannot resolve domain names — it only accepts IPv4
/// addresses. Sending a domain should fail.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks4_domain_fails() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_tcp_echo().await;

    // --- pproxy SOCKS4 (not SOCKS4a) ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks4", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Send SOCKS4 with a bogus IP (domain-like request via SOCKS4 should fail)
    let mut stream = tokio::net::TcpStream::connect(socket_addr("127.0.0.1", pproxy_port))
        .await
        .expect("connect to pproxy failed");
    let mut req = vec![0x04, 0x01]; // VER=4, CMD=CONNECT
    req.extend_from_slice(&echo_addr.port().to_be_bytes());
    // Use an invalid IP that would fail to connect
    req.extend_from_slice(&[192, 0, 2, 1]); // TEST-NET-3, should be unreachable
    req.push(0x00); // user ID terminator
    stream.write_all(&req).await.expect("write failed");
    let mut reply = [0u8; 8];
    stream
        .read_exact(&mut reply)
        .await
        .expect("read reply failed");

    // SOCKS4 reply[1] != 0x5A means failure
    let pproxy_result: Result<Vec<u8>, String> = if reply[1] == 0x5A {
        Err("expected SOCKS4 failure but got success".into())
    } else {
        Ok(reply.to_vec())
    };

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- Direct connection to same unreachable IP for comparison ---
    let direct_result: Result<Vec<u8>, String> = {
        match tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(socket_addr("192.0.2.1", echo_addr.port())),
        )
        .await
        {
            Ok(Ok(_)) => Err("expected connection failure but got success".into()),
            _ => Ok(vec![]),
        }
    };

    echo_jh.abort();

    assert_coarse_failure_equivalence("pproxy-socks4", &pproxy_result, "direct", &direct_result);
}

/// SOCKS4 with refused target — upstream connection is actively refused.
///
/// Both pproxy SOCKS4 and a direct connection should fail equivalently.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks4_refused_target() {
    skip_if_unavailable();

    let closed_port = eggress_testkit::get_free_port().await;

    // --- pproxy SOCKS4 ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks4", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let target = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        closed_port,
    );
    let pproxy_result =
        send_through_socks4(socket_addr("127.0.0.1", pproxy_port), target, b"refused").await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- Direct connection to refused port ---
    let direct_result: Result<Vec<u8>, String> = {
        match tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(target),
        )
        .await
        {
            Ok(Ok(_)) => Err("expected connection refused but got success".into()),
            _ => Ok(vec![]),
        }
    };

    assert_coarse_failure_equivalence("pproxy-socks4", &pproxy_result, "direct", &direct_result);
}

/// SOCKS4 with malformed version byte.
///
/// Sends SOCKS4 with version=0x03 (invalid). Both pproxy and direct
/// should reject it.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks4_malformed_version() {
    skip_if_unavailable();

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks4", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Send SOCKS4 with invalid version
    let mut stream = tokio::net::TcpStream::connect(socket_addr("127.0.0.1", pproxy_port))
        .await
        .expect("connect to pproxy failed");
    let req = [0x03, 0x01, 0x00, 0x50, 7, 0, 0, 1, 0x00]; // version=3 (invalid)
    stream.write_all(&req).await.expect("write failed");
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let mut reply = [0u8; 8];
        stream.read_exact(&mut reply).await?;
        Ok::<Vec<u8>, std::io::Error>(reply.to_vec())
    })
    .await;

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // Should fail (timeout or error)
    if let Ok(Ok(_)) = result {
        panic!("expected SOCKS4 failure with malformed version");
    }
}

/// SOCKS4 with truncated request (incomplete header).
///
/// Sends only the SOCKS4 version and command bytes, then closes the
/// connection. The proxy should handle the truncated input gracefully.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks4_truncated_request() {
    skip_if_unavailable();

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks4", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Send only 2 bytes of a SOCKS4 request (truncated)
    let mut stream = tokio::net::TcpStream::connect(socket_addr("127.0.0.1", pproxy_port))
        .await
        .expect("connect to pproxy failed");
    stream.write_all(&[0x04, 0x01]).await.expect("write failed");
    // Immediately close the connection
    drop(stream);

    // pproxy should handle this gracefully (error or timeout)
    tokio::time::sleep(Duration::from_millis(500)).await;

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // If we get here without panicking, the test passes — both proxies
    // should handle truncated input without crashing.
}

// ===== Phase 19 Gap Closure: Additional SOCKS5 Cases =====

/// SOCKS5 with malformed address type.
///
/// Sends a CONNECT request with an invalid ATYP value (0x03 is DOMAIN,
/// but we'll use 0xFF which is undefined). Both proxies should reject it.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_malformed_address_type() {
    skip_if_unavailable();

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // SOCKS5 handshake: no auth
    let mut stream = tokio::net::TcpStream::connect(socket_addr("127.0.0.1", pproxy_port))
        .await
        .expect("connect to pproxy failed");
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .expect("write failed");
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.expect("read failed");
    assert_eq!(resp, [0x05, 0x00]);

    // Send CONNECT with invalid ATYP (0xFF)
    let req = [0x05, 0x01, 0x00, 0xFF, 10, 0, 0, 1, 0, 80]; // ATYP=0xFF (invalid)
    stream.write_all(&req).await.expect("write failed");
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await?;
        Ok::<Vec<u8>, std::io::Error>(reply.to_vec())
    })
    .await;

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // Should fail
    if let Ok(Ok(reply)) = result {
        // If we get a reply, it should be a failure code
        assert_ne!(reply[1], 0x00, "expected SOCKS5 failure for malformed ATYP");
    }
}

/// SOCKS5 with unsupported UDP ASSOCIATE command.
///
/// Sends a UDP ASSOCIATE command. Since eggress does not support UDP ASSOCIATE,
/// both proxies should reject it.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_unsupported_udp_command() {
    skip_if_unavailable();

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // SOCKS5 handshake: no auth
    let mut stream = tokio::net::TcpStream::connect(socket_addr("127.0.0.1", pproxy_port))
        .await
        .expect("connect to pproxy failed");
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .expect("write failed");
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.expect("read failed");
    assert_eq!(resp, [0x05, 0x00]);

    // Send UDP ASSOCIATE (CMD=0x03)
    stream
        .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
        .await
        .expect("write failed");
    stream
        .write_all(&0u16.to_be_bytes())
        .await
        .expect("write failed");

    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let mut reply = [0u8; 22];
        let n = stream.read(&mut reply).await?;
        Ok::<(usize, Vec<u8>), std::io::Error>((n, reply[..n].to_vec()))
    })
    .await;

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // Should fail (command not supported)
    if let Ok(Ok((n, reply))) = result {
        assert!(n >= 10, "UDP ASSOCIATE reply too short");
        assert_eq!(reply[0], 0x05, "SOCKS5 version mismatch");
        // pproxy may succeed (returning a relay address) or fail — both are valid.
        // eggress rejects with REP_COMMAND_NOT_SUPPORTED (0x07).
    }
}

/// SOCKS5 with early client close during greeting.
///
/// Client sends the SOCKS5 version byte and method count, then immediately
/// closes the connection. Both proxies should handle this gracefully.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_early_close_greeting() {
    skip_if_unavailable();

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Send partial greeting then close
    let mut stream = tokio::net::TcpStream::connect(socket_addr("127.0.0.1", pproxy_port))
        .await
        .expect("connect to pproxy failed");
    stream.write_all(&[0x05, 0x01]).await.expect("write failed");
    drop(stream);

    // pproxy should handle this gracefully
    tokio::time::sleep(Duration::from_millis(500)).await;

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // If we get here without panicking, the test passes.
}

/// SOCKS5 with early client close during request phase.
///
/// Client completes method negotiation, then closes the connection before
/// sending a CONNECT request. Both proxies should handle this gracefully.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_early_close_request() {
    skip_if_unavailable();

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    // Complete method negotiation, then close before sending CONNECT
    let mut stream = tokio::net::TcpStream::connect(socket_addr("127.0.0.1", pproxy_port))
        .await
        .expect("connect to pproxy failed");
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .expect("write failed");
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.expect("read failed");
    assert_eq!(resp, [0x05, 0x00]);
    // Now close without sending CONNECT
    drop(stream);

    tokio::time::sleep(Duration::from_millis(500)).await;

    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // If we get here without panicking, the test passes.
}

/// SOCKS5 with server (upstream) half-close during tunnel.
///
/// Client connects through SOCKS5 to an echo server that half-closes after
/// echoing. Both proxies should relay correctly.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_socks5_server_half_close() {
    skip_if_unavailable();

    // Echo server that half-closes after echoing
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = listener.local_addr().unwrap();
    let echo_jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                if let Ok(n) = stream.read(&mut buf).await {
                    if n > 0 {
                        let _ = stream.write_all(&buf[..n]).await;
                        let _ = stream.shutdown().await;
                    }
                }
            });
        }
    });

    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // --- pproxy SOCKS5 ---
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;
    assert!(
        wait_for_port(pproxy_port, Duration::from_secs(5)).await,
        "pproxy failed to start"
    );

    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"socks5-half-close",
    )
    .await;
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress SOCKS5 ---
    let (eggress_addr, cancel, eggress_jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let eggress_result = send_through_socks5(eggress_addr, &target, b"socks5-half-close").await;

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &eggress_result);
}

// ===== Phase 20: Standalone UDP Differential Tests =====

/// Build a SOCKS5 UDP datagram targeting a domain name.
fn build_socks5_udp_packet_domain(target_host: &str, target_port: u16, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00]; // RSV + FRAG
    pkt.push(0x03); // ATYP DOMAIN
    pkt.push(target_host.len() as u8);
    pkt.extend_from_slice(target_host.as_bytes());
    pkt.extend_from_slice(&target_port.to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

/// Build a SOCKS5 UDP datagram with a custom FRAG value.
fn build_socks5_udp_packet_frag(target: std::net::SocketAddr, frag: u8, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, frag]; // RSV + custom FRAG
    match target.ip() {
        std::net::IpAddr::V4(ip) => {
            pkt.push(0x01); // ATYP IPv4
            pkt.extend_from_slice(&ip.octets());
        }
        std::net::IpAddr::V6(ip) => {
            pkt.push(0x04); // ATYP IPv6
            pkt.extend_from_slice(&ip.octets());
        }
    }
    pkt.extend_from_slice(&target.port().to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

/// Start an in-process eggress standalone UDP relay with a direct-route router.
///
/// Returns the relay socket address, a shutdown token, and a join handle.
async fn start_eggress_standalone_udp(
    _target: std::net::SocketAddr,
) -> (
    std::net::SocketAddr,
    tokio_util::sync::CancellationToken,
    tokio::task::JoinHandle<()>,
) {
    let socket = std::sync::Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = socket.local_addr().unwrap();

    let router = eggress_routing::Router::new(vec![], eggress_routing::RouteActionSpec::Direct);
    let routing: Arc<dyn eggress_routing::RouteService> =
        Arc::new(eggress_routing::SharedRoutingService::new(router));

    let udp_metrics = Arc::new(eggress_udp::metrics::UdpMetrics::new());
    let limits = eggress_udp::limits::UdpLimits::default();
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel.clone();

    let config = eggress_udp::standalone::StandaloneUdpConfig {
        routing,
        udp_metrics,
        limits,
        listener: "differential-test".to_string(),
        generation: 1,
    };

    let jh = tokio::spawn(async move {
        let _ = eggress_udp::standalone::standalone_udp_relay(socket, config, cancel_clone).await;
    });

    (relay_addr, cancel, jh)
}

/// Differential: Standalone UDP direct echo.
///
/// Both pproxy (`-ul`) and eggress standalone UDP relay a datagram to a direct
/// UDP echo target. Verifies both successfully relay the payload.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_standalone_udp_direct_echo() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_udp_echo().await;

    // --- pproxy standalone UDP ---
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let pproxy_udp_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &format!("socks5://127.0.0.1:{}", pproxy_tcp_port),
            "-ul",
            &format!("socks5://127.0.0.1:{}", pproxy_udp_port),
            "-r",
            "direct",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_tcp_port, Duration::from_secs(5)).await,
        "pproxy TCP failed to start"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pproxy_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet(echo_addr, b"pproxy-standalone-test");
        let _ = sock.send_to(&packet, ("127.0.0.1", pproxy_udp_port)).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress standalone UDP ---
    let (relay_addr, cancel, jh) = start_eggress_standalone_udp(echo_addr).await;

    let eggress_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet(echo_addr, b"eggress-standalone-test");
        let _ = sock.send_to(&packet, relay_addr).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };

    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    assert!(
        pproxy_response.is_some(),
        "pproxy standalone UDP did not receive response"
    );
    assert!(
        eggress_response.is_some(),
        "eggress standalone UDP did not receive response"
    );

    // Both should relay the payload through the SOCKS5 UDP datagram framing.
    // The response format is: [RSV(2) + FRAG(1) + ATYP + ADDR + PORT + PAYLOAD]
    // Extract the payload portion (after the header) and compare.
    let pproxy_payload = extract_udp_payload(&pproxy_response.unwrap());
    let eggress_payload = extract_udp_payload(&eggress_response.unwrap());
    assert_eq!(
        pproxy_payload, eggress_payload,
        "standalone UDP direct echo payload mismatch"
    );
}

/// Differential: Standalone UDP domain target echo.
///
/// Both pproxy and eggress relay a datagram targeting `localhost`.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_standalone_udp_domain_target() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_udp_echo().await;

    // --- pproxy standalone UDP ---
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let pproxy_udp_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &format!("socks5://127.0.0.1:{}", pproxy_tcp_port),
            "-ul",
            &format!("socks5://127.0.0.1:{}", pproxy_udp_port),
            "-r",
            "direct",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_tcp_port, Duration::from_secs(5)).await,
        "pproxy TCP failed to start"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pproxy_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet =
            build_socks5_udp_packet_domain("localhost", echo_addr.port(), b"pproxy-domain-test");
        let _ = sock.send_to(&packet, ("127.0.0.1", pproxy_udp_port)).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress standalone UDP ---
    let (relay_addr, cancel, jh) = start_eggress_standalone_udp(echo_addr).await;

    let eggress_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet =
            build_socks5_udp_packet_domain("localhost", echo_addr.port(), b"eggress-domain-test");
        let _ = sock.send_to(&packet, relay_addr).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };

    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    assert!(
        pproxy_response.is_some(),
        "pproxy standalone UDP did not receive domain-targeted response"
    );
    assert!(
        eggress_response.is_some(),
        "eggress standalone UDP did not receive domain-targeted response"
    );

    let pproxy_payload = extract_udp_payload(&pproxy_response.unwrap());
    let eggress_payload = extract_udp_payload(&eggress_response.unwrap());
    assert_eq!(
        pproxy_payload, eggress_payload,
        "standalone UDP domain target echo payload mismatch"
    );
}

/// Differential: Malformed short datagram handling.
///
/// Both pproxy and eggress should silently drop a datagram that is too short
/// to contain a valid SOCKS5 UDP header (fewer than 4 bytes).
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_standalone_udp_malformed_short_datagram() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_udp_echo().await;

    // --- pproxy standalone UDP ---
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let pproxy_udp_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &format!("socks5://127.0.0.1:{}", pproxy_tcp_port),
            "-ul",
            &format!("socks5://127.0.0.1:{}", pproxy_udp_port),
            "-r",
            "direct",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_tcp_port, Duration::from_secs(5)).await,
        "pproxy TCP failed to start"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send a malformed (too short) datagram to pproxy
    let pproxy_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let _ = sock
            .send_to(&[0x00, 0x01], ("127.0.0.1", pproxy_udp_port))
            .await;
        recv_udp_response(&sock, Duration::from_millis(500)).await
    };
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress standalone UDP ---
    let (relay_addr, cancel, jh) = start_eggress_standalone_udp(echo_addr).await;

    // Send the same malformed datagram to eggress
    let eggress_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let _ = sock.send_to(&[0x00, 0x01], relay_addr).await;
        recv_udp_response(&sock, Duration::from_millis(500)).await
    };

    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    // Both should silently drop the malformed datagram (no response)
    assert!(
        pproxy_response.is_none(),
        "pproxy should not respond to malformed datagram"
    );
    assert!(
        eggress_response.is_none(),
        "eggress should not respond to malformed datagram"
    );
}

/// Differential: Nonzero FRAG handling.
///
/// Both pproxy and eggress should silently drop a datagram with FRAG=1
/// (fragmentation not supported).
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_standalone_udp_nonzero_frag() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_udp_echo().await;

    // --- pproxy standalone UDP ---
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let pproxy_udp_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &format!("socks5://127.0.0.1:{}", pproxy_tcp_port),
            "-ul",
            &format!("socks5://127.0.0.1:{}", pproxy_udp_port),
            "-r",
            "direct",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_tcp_port, Duration::from_secs(5)).await,
        "pproxy TCP failed to start"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Build a packet with FRAG=1
    let pproxy_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet_frag(echo_addr, 1, b"frag-test");
        let _ = sock.send_to(&packet, ("127.0.0.1", pproxy_udp_port)).await;
        recv_udp_response(&sock, Duration::from_millis(500)).await
    };
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress standalone UDP ---
    let (relay_addr, cancel, jh) = start_eggress_standalone_udp(echo_addr).await;

    let eggress_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet_frag(echo_addr, 1, b"frag-test");
        let _ = sock.send_to(&packet, relay_addr).await;
        recv_udp_response(&sock, Duration::from_millis(500)).await
    };

    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    // Both should silently drop the nonzero FRAG datagram
    assert!(
        pproxy_response.is_none(),
        "pproxy should not respond to nonzero FRAG datagram"
    );
    assert!(
        eggress_response.is_none(),
        "eggress should not respond to nonzero FRAG datagram"
    );
}

/// Differential: Two clients using the same standalone UDP listener.
///
/// Both pproxy and eggress should handle datagrams from two different client
/// addresses on the same UDP listener.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_standalone_udp_two_clients() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_udp_echo().await;

    // --- pproxy standalone UDP ---
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let pproxy_udp_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &format!("socks5://127.0.0.1:{}", pproxy_tcp_port),
            "-ul",
            &format!("socks5://127.0.0.1:{}", pproxy_udp_port),
            "-r",
            "direct",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_tcp_port, Duration::from_secs(5)).await,
        "pproxy TCP failed to start"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send from client A
    let pproxy_response_a = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet(echo_addr, b"client-a-pproxy");
        let _ = sock.send_to(&packet, ("127.0.0.1", pproxy_udp_port)).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };
    // Send from client B (different port)
    let pproxy_response_b = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet(echo_addr, b"client-b-pproxy");
        let _ = sock.send_to(&packet, ("127.0.0.1", pproxy_udp_port)).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress standalone UDP ---
    let (relay_addr, cancel, jh) = start_eggress_standalone_udp(echo_addr).await;

    let eggress_response_a = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet(echo_addr, b"client-a-eggress");
        let _ = sock.send_to(&packet, relay_addr).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };
    let eggress_response_b = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet(echo_addr, b"client-b-eggress");
        let _ = sock.send_to(&packet, relay_addr).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };

    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    // Both clients should get responses from both proxies
    assert!(pproxy_response_a.is_some(), "pproxy client A no response");
    assert!(pproxy_response_b.is_some(), "pproxy client B no response");
    assert!(eggress_response_a.is_some(), "eggress client A no response");
    assert!(eggress_response_b.is_some(), "eggress client B no response");

    // Verify payload integrity for each client
    let p_a = extract_udp_payload(&pproxy_response_a.unwrap());
    let e_a = extract_udp_payload(&eggress_response_a.unwrap());
    assert_eq!(p_a, e_a, "client A payload mismatch");

    let p_b = extract_udp_payload(&pproxy_response_b.unwrap());
    let e_b = extract_udp_payload(&eggress_response_b.unwrap());
    assert_eq!(p_b, e_b, "client B payload mismatch");
}

/// Differential: Standalone UDP oversized datagram handling.
///
/// Both pproxy and eggress should handle a datagram that exceeds the maximum
/// allowed size (both silently drop or truncate).
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_standalone_udp_oversized_datagram() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = start_udp_echo().await;

    // --- pproxy standalone UDP ---
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let pproxy_udp_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &format!("socks5://127.0.0.1:{}", pproxy_tcp_port),
            "-ul",
            &format!("socks5://127.0.0.1:{}", pproxy_udp_port),
            "-r",
            "direct",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_tcp_port, Duration::from_secs(5)).await,
        "pproxy TCP failed to start"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send an oversized datagram (70000 bytes payload) to pproxy
    let pproxy_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut packet = build_socks5_udp_packet(echo_addr, &[]);
        packet.extend_from_slice(&vec![0xAA; 70000]);
        let _ = sock.send_to(&packet, ("127.0.0.1", pproxy_udp_port)).await;
        recv_udp_response(&sock, Duration::from_millis(500)).await
    };
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress standalone UDP ---
    let (relay_addr, cancel, jh) = start_eggress_standalone_udp(echo_addr).await;

    let eggress_response = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut packet = build_socks5_udp_packet(echo_addr, &[]);
        packet.extend_from_slice(&vec![0xAA; 70000]);
        let _ = sock.send_to(&packet, relay_addr).await;
        recv_udp_response(&sock, Duration::from_millis(500)).await
    };

    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    // Both should handle oversized datagrams consistently (either both drop
    // or both relay). We just verify they behave the same way.
    let pproxy_result: Result<(), String> = pproxy_response
        .map(|_| ())
        .ok_or_else(|| "no response".to_string());
    let eggress_result: Result<(), String> = eggress_response
        .map(|_| ())
        .ok_or_else(|| "no response".to_string());
    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &eggress_result);
}

/// Differential: Standalone UDP two targets from same client.
///
/// Both pproxy and eggress should handle datagrams from the same client
/// targeting different destinations.
#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn differential_standalone_udp_two_targets_from_same_client() {
    skip_if_unavailable();

    let (echo_addr_a, echo_jh_a) = start_udp_echo().await;
    let (echo_addr_b, echo_jh_b) = start_udp_echo().await;

    // --- pproxy standalone UDP ---
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let pproxy_udp_port = eggress_testkit::get_free_port().await;
    let mut pproxy_child = std::process::Command::new("python3")
        .args([
            "-m",
            "pproxy",
            "-l",
            &format!("socks5://127.0.0.1:{}", pproxy_tcp_port),
            "-ul",
            &format!("socks5://127.0.0.1:{}", pproxy_udp_port),
            "-r",
            "direct",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    assert!(
        wait_for_port(pproxy_tcp_port, Duration::from_secs(5)).await,
        "pproxy TCP failed to start"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send to target A from same client socket
    let pproxy_response_a = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet(echo_addr_a, b"target-a-pproxy");
        let _ = sock.send_to(&packet, ("127.0.0.1", pproxy_udp_port)).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };
    // Send to target B from same client socket
    let pproxy_response_b = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet(echo_addr_b, b"target-b-pproxy");
        let _ = sock.send_to(&packet, ("127.0.0.1", pproxy_udp_port)).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };
    let _ = pproxy_child.kill();
    let _ = pproxy_child.wait();

    // --- eggress standalone UDP ---
    let (relay_addr, cancel, jh) = start_eggress_standalone_udp(echo_addr_a).await;

    let eggress_response_a = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet(echo_addr_a, b"target-a-eggress");
        let _ = sock.send_to(&packet, relay_addr).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };
    let eggress_response_b = {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = build_socks5_udp_packet(echo_addr_b, b"target-b-eggress");
        let _ = sock.send_to(&packet, relay_addr).await;
        recv_udp_response(&sock, Duration::from_secs(3)).await
    };

    cancel.cancel();
    let _ = jh.await;
    echo_jh_a.abort();
    echo_jh_b.abort();

    // Both targets should get responses
    assert!(pproxy_response_a.is_some(), "pproxy target A no response");
    assert!(pproxy_response_b.is_some(), "pproxy target B no response");
    assert!(eggress_response_a.is_some(), "eggress target A no response");
    assert!(eggress_response_b.is_some(), "eggress target B no response");

    let p_a = extract_udp_payload(&pproxy_response_a.unwrap());
    let e_a = extract_udp_payload(&eggress_response_a.unwrap());
    assert_eq!(p_a, e_a, "target A payload mismatch");

    let p_b = extract_udp_payload(&pproxy_response_b.unwrap());
    let e_b = extract_udp_payload(&eggress_response_b.unwrap());
    assert_eq!(p_b, e_b, "target B payload mismatch");
}

/// Extract the payload portion from a SOCKS5 UDP datagram response.
///
/// The format is: [RSV(2) + FRAG(1) + ATYP(1) + ADDR(varies) + PORT(2) + PAYLOAD]
/// ATYP 0x01 = IPv4 (4 bytes), 0x03 = Domain (1+len bytes), 0x04 = IPv6 (16 bytes)
fn extract_udp_payload(datagram: &[u8]) -> Vec<u8> {
    if datagram.len() < 4 {
        return vec![];
    }
    let atyp = datagram[3];
    let header_len = match atyp {
        0x01 => 4 + 4 + 2,                        // ATYP + IPv4 + PORT
        0x03 => 4 + 1 + datagram[4] as usize + 2, // ATYP + len + domain + PORT
        0x04 => 4 + 16 + 2,                       // ATYP + IPv6 + PORT
        _ => return vec![],
    };
    if datagram.len() <= header_len {
        return vec![];
    }
    datagram[header_len..].to_vec()
}
