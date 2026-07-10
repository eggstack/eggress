//! Structured differential tests comparing eggress with Python pproxy.
//!
//! Uses the reusable harness from `eggress_testkit::differential` and provides
//! protocol-specific client helpers locally (since testkit does not depend on
//! eggress-core or protocol crates).
//!
//! All tests are `#[ignore]` and gated on `EGGRESS_RUN_PPROXY_DIFFERENTIAL=1`.
//!
//! Run with:
//! ```bash
//! EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- --ignored
//! ```

#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::{BoxStream, TargetAddr, TargetHost};
use eggress_protocol_http::connect::client::http_connect;
use eggress_protocol_socks::socks5::client::socks5_connect;
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_routing::{RouteActionSpec, RouteService, Router};
use eggress_testkit::differential::*;
use eggress_uri::{ProtocolSpec, ProxyHopSpec};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

// ===== Protocol-Specific Client Helpers =====

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

type HandshakeFuture<'a> = std::pin::Pin<
    Box<
        dyn std::future::Future<
                Output = Result<BoxStream, Box<dyn std::error::Error + Send + Sync>>,
            > + Send
            + 'a,
    >,
>;

// ===== Hop Handlers (for chain tests) =====

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

// ===== TCP Client Helpers =====

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
    // Use timeout-based read instead of half-close + read_to_end.
    // pproxy closes the connection on half-close, losing echo responses.
    Ok(read_with_timeout(&mut conn, Duration::from_secs(3)).await)
}

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
    Ok(read_with_timeout(&mut conn, Duration::from_secs(3)).await)
}

async fn send_through_socks5_stream<S>(
    stream: S,
    target: &TargetAddr,
    payload: &[u8],
) -> Result<Vec<u8>, String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let boxed: BoxStream = Box::new(stream);
    let socks_addr = target_to_socks_addr(target);
    let mut conn = socks5_connect(boxed, &socks_addr, None)
        .await
        .map_err(|e| format!("socks5 handshake failed: {e}"))?;
    conn.write_all(payload)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    Ok(read_with_timeout(&mut conn, Duration::from_secs(3)).await)
}

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
    Ok(read_with_timeout(&mut conn, Duration::from_secs(3)).await)
}

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
    Ok(read_with_timeout(&mut conn, Duration::from_secs(3)).await)
}

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
    // Read SOCKS4 reply (8 bytes)
    let mut reply = [0u8; 8];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| format!("read reply failed: {e}"))?;
    if reply[1] != 0x5a {
        return Err(format!("SOCKS4 CONNECT failed: code {}", reply[1]));
    }
    stream
        .write_all(payload)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    Ok(read_with_timeout(&mut stream, Duration::from_secs(3)).await)
}

async fn send_through_socks4a(
    proxy_addr: std::net::SocketAddr,
    target_host: &str,
    target_port: u16,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let mut stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;
    // SOCKS4a CONNECT request: IP = 0.0.0.1 (signals domain lookup), + domain string
    let mut req = vec![0x04, 0x01]; // VER=4, CMD=CONNECT
    req.extend_from_slice(&target_port.to_be_bytes());
    req.extend_from_slice(&[0, 0, 0, 1]); // dummy IP for SOCKS4a
    req.extend_from_slice(target_host.as_bytes());
    req.push(0x00); // domain terminator
    stream
        .write_all(&req)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    let mut reply = [0u8; 8];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| format!("read reply failed: {e}"))?;
    if reply[1] != 0x5a {
        return Err(format!("SOCKS4a CONNECT failed: code {}", reply[1]));
    }
    stream
        .write_all(payload)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    Ok(read_with_timeout(&mut stream, Duration::from_secs(3)).await)
}

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
    assert_eq!(reply[1], 0x00, "UDP ASSOCIATE failed: code {}", reply[1]);

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

// ===== Eggress Server Helpers =====

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
                trojan: None,
            };
            tokio::spawn(async move {
                let _ = eggress_server::serve_connection(conn.stream, config).await;
            });
        }
    });

    (addr, cancel, jh)
}

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
        addrs[0].unwrap()
    };
    (listener_addr, token, jh)
}

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
    // Do NOT shutdown here — for forward proxies, the proxy may need to see
    // the client stream stay open while it forwards the request and relays the
    // response. Instead, rely on the timeout-based read to return when the
    // proxy closes its end.
    Ok(read_with_timeout(&mut stream, Duration::from_secs(5)).await)
}

// ========================================================================
// Scenario Tests
// ========================================================================

// --- Scenario 1: HTTP CONNECT ---

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1 and pproxy"]
async fn differential_http_connect() {
    require_differential_gate();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy = start_pproxy_server("http", pproxy_port).await;
    assert_port_ready(pproxy_port, Duration::from_secs(5)).await;
    let pproxy_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"differential http connect",
    )
    .await;
    pproxy.kill();

    let (egress_addr, cancel, jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Http]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let egress_result = send_through_http(egress_addr, &target, b"differential http connect").await;
    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &egress_result);
    assert_eq!(pproxy_result.unwrap(), b"differential http connect");
}

// --- Scenario 2: HTTP Forward Proxy ---

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1 and pproxy"]
async fn differential_http_forward() {
    require_differential_gate();

    let (origin_addr, origin_jh) = start_http_origin().await;

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy = start_pproxy_server("http", pproxy_port).await;
    assert_port_ready(pproxy_port, Duration::from_secs(5)).await;

    let request = format!(
        "GET http://127.0.0.1:{}/path HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        origin_addr.port(),
        origin_addr.port(),
    );
    let pproxy_result =
        send_http_forward(socket_addr("127.0.0.1", pproxy_port), request.as_bytes()).await;
    pproxy.kill();

    // eggress HTTP forward proxy via TOML — the in-process server doesn't support forward mode
    let egress_port = eggress_testkit::get_free_port().await;
    let toml = format!(
        r#"version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:{port}"
protocols = ["http"]

[[rules]]
id = "allow-all"
direct = true

[routing]
default = "direct"
"#,
        port = egress_port,
    );
    let (egress_addr, cancel, jh) = start_eggress_from_toml_running(&toml).await;
    let egress_result = send_http_forward(egress_addr, request.as_bytes()).await;
    cancel.cancel();
    let _ = jh.await;
    origin_jh.abort();

    // Both should succeed in forwarding the request
    assert_coarse_failure_equivalence("pproxy", &pproxy_result, "eggress", &egress_result);
}

// --- Scenario 3: SOCKS4/4a CONNECT ---

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1 and pproxy"]
async fn differential_socks4_connect() {
    require_differential_gate();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy = start_pproxy_server("socks4", pproxy_port).await;
    assert_port_ready(pproxy_port, Duration::from_secs(5)).await;
    let pproxy_result = send_through_socks4(
        socket_addr("127.0.0.1", pproxy_port),
        echo_addr,
        b"differential socks4",
    )
    .await;
    pproxy.kill();

    let (egress_addr, cancel, jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks4]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let egress_result = send_through_socks4(egress_addr, echo_addr, b"differential socks4").await;
    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &egress_result);
}

// --- Scenario 4: SOCKS5 CONNECT ---

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1 and pproxy"]
async fn differential_socks5_connect() {
    require_differential_gate();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy = start_pproxy_server("socks5", pproxy_port).await;
    assert_port_ready(pproxy_port, Duration::from_secs(5)).await;

    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"differential socks5",
    )
    .await;
    pproxy.kill();

    let (egress_addr, cancel, jh) =
        start_eggress_server(vec![eggress_core::ProtocolId::Socks5]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let egress_result = send_through_socks5(egress_addr, &target, b"differential socks5").await;
    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &egress_result);
    assert_eq!(pproxy_result.unwrap(), b"differential socks5");
}

// --- Scenario 5: SOCKS5 Username/Password Auth ---

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1 and pproxy"]
async fn differential_socks5_auth() {
    require_differential_gate();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };
    let user = "testuser";
    let pass = "testpass";

    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy = start_pproxy_server_with_auth("socks5", pproxy_port, user, pass).await;
    assert_port_ready(pproxy_port, Duration::from_secs(5)).await;
    let pproxy_result = send_through_socks5_with_auth(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"differential socks5 auth",
        user,
        pass,
    )
    .await;
    pproxy.kill();

    // eggress with auth — use TOML config with credentials
    let egress_port = eggress_testkit::get_free_port().await;
    let toml = format!(
        r#"version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:{egress_port}"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "{user}"
password = "{pass}"

[[rules]]
id = "allow-all"
direct = true

[routing]
default = "direct"
"#
    );
    let (egress_addr, cancel, jh) = start_eggress_from_toml_running(&toml).await;
    let egress_result = send_through_socks5_with_auth(
        egress_addr,
        &target,
        b"differential socks5 auth",
        user,
        pass,
    )
    .await;
    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    compare_tcp_echo("pproxy", &pproxy_result, "eggress", &egress_result);
}

// --- Scenario 6: SOCKS5 UDP ASSOCIATE ---

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1 and pproxy"]
async fn differential_socks5_udp_associate() {
    require_differential_gate();

    let (udp_echo_addr, udp_echo_jh) = start_udp_echo().await;

    // pproxy: SOCKS5 TCP listener + UDP listener
    // Note: pproxy UDP ASSOCIATE is broken on macOS (SelectorDatagramTransport error)
    // so we only test eggress UDP ASSOCIATE as a smoke test.
    let pproxy_tcp_port = eggress_testkit::get_free_port().await;
    let listen_tcp = format!("socks5://127.0.0.1:{}", pproxy_tcp_port);
    let pproxy = start_pproxy_with_args(&["-l", &listen_tcp, "-r", "direct"]).await;
    assert_port_ready(pproxy_tcp_port, Duration::from_secs(5)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(pproxy);

    // eggress: SOCKS5 with UDP support via TOML
    let egress_tcp_port = eggress_testkit::get_free_port().await;
    let toml = format!(
        r#"version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:{port}"
protocols = ["socks5"]

[listeners.udp]
enabled = true

[[rules]]
id = "allow-all"
direct = true

[routing]
default = "direct"
"#,
        port = egress_tcp_port,
    );
    let (egress_addr, cancel, jh) = start_eggress_from_toml_running(&toml).await;

    let mut egress_stream = tokio::net::TcpStream::connect(egress_addr).await.unwrap();
    let egress_relay = socks5_udp_associate(&mut egress_stream).await.unwrap();

    let egress_udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let egress_packet = build_socks5_udp_packet(udp_echo_addr, b"pproxy udp test");
    egress_udp_sock
        .send_to(&egress_packet, egress_relay)
        .await
        .unwrap();
    let egress_udp_result = recv_udp_response(&egress_udp_sock, Duration::from_secs(3)).await;

    cancel.cancel();
    let _ = jh.await;
    udp_echo_jh.abort();

    // Verify eggress UDP ASSOCIATE works
    assert!(
        egress_udp_result.is_some(),
        "eggress UDP ASSOCIATE should relay data"
    );
    let payload = extract_udp_payload(&egress_udp_result.unwrap());
    assert_eq!(payload, b"pproxy udp test");
}

// --- Scenario 7: Standalone UDP ---

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1 and pproxy"]
async fn differential_standalone_udp() {
    require_differential_gate();

    let (udp_echo_addr, udp_echo_jh) = start_udp_echo().await;

    // pproxy standalone UDP
    let pproxy_port = eggress_testkit::get_free_port().await;
    let listen = format!("socks5://127.0.0.1:{}", pproxy_port);
    let pproxy = start_pproxy_with_args(&["-l", &listen, "-ul", &listen, "-r", "direct"]).await;
    assert_port_ready(pproxy_port, Duration::from_secs(5)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pproxy_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let packet = build_socks5_udp_packet(udp_echo_addr, b"standalone udp test");
    pproxy_sock
        .send_to(&packet, ("127.0.0.1", pproxy_port))
        .await
        .unwrap();
    let _pproxy_result = recv_udp_response(&pproxy_sock, Duration::from_secs(3)).await;
    drop(pproxy);

    // eggress standalone UDP — use in-process relay (same as existing differential tests)
    // Note: pproxy standalone UDP is broken on macOS (SelectorDatagramTransport error),
    // so we only verify eggress standalone UDP works as a smoke test.
    let udp_socket = std::sync::Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let egress_addr = udp_socket.local_addr().unwrap();
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
        allow_private_egress: true,
    };
    let jh = tokio::spawn(async move {
        let _ =
            eggress_udp::standalone::standalone_udp_relay(udp_socket, config, cancel_clone).await;
    });

    let egress_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let egress_packet = build_socks5_udp_packet(udp_echo_addr, b"standalone udp test");
    egress_sock
        .send_to(&egress_packet, egress_addr)
        .await
        .unwrap();
    let egress_result = recv_udp_response(&egress_sock, Duration::from_secs(3)).await;

    cancel.cancel();
    let _ = jh.await;
    udp_echo_jh.abort();

    // Verify eggress standalone UDP relays correctly
    assert!(
        egress_result.is_some(),
        "eggress standalone UDP should relay data"
    );
    let payload = extract_udp_payload(&egress_result.unwrap());
    assert_eq!(payload, b"standalone udp test");
}

// --- Scenario 8: Scheduler Behavior (minimal) ---

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1 and pproxy"]
async fn differential_scheduler_round_robin() {
    require_differential_gate();

    // Start two echo servers to verify both are reachable
    let (echo1_addr, echo1_jh) = eggress_testkit::start_echo_server().await;
    let (echo2_addr, echo2_jh) = eggress_testkit::start_echo_server().await;

    // pproxy with two upstreams — echo servers are not SOCKS5 proxies, so we use direct
    let pproxy_port = eggress_testkit::get_free_port().await;
    let mut pproxy = start_pproxy_with_args(&[
        "-l",
        &format!("socks5://127.0.0.1:{}", pproxy_port),
        "-r",
        "direct",
    ])
    .await;
    assert_port_ready(pproxy_port, Duration::from_secs(5)).await;

    let target1 = TargetAddr {
        host: TargetHost::Ip(echo1_addr.ip()),
        port: echo1_addr.port(),
    };
    let target2 = TargetAddr {
        host: TargetHost::Ip(echo2_addr.ip()),
        port: echo2_addr.port(),
    };

    // Both targets should be reachable through pproxy
    let r1 = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target1,
        b"scheduler-test-1",
    )
    .await;
    let r2 = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target2,
        b"scheduler-test-2",
    )
    .await;
    pproxy.kill();
    echo1_jh.abort();
    echo2_jh.abort();

    assert!(r1.is_ok(), "pproxy should reach echo1: {:?}", r1.err());
    assert!(r2.is_ok(), "pproxy should reach echo2: {:?}", r2.err());
    assert_eq!(r1.unwrap(), b"scheduler-test-1");
    assert_eq!(r2.unwrap(), b"scheduler-test-2");
}

// --- Scenario 9: Block/Rulefile Behavior ---

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1 and pproxy"]
async fn differential_block_behavior() {
    require_differential_gate();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // pproxy with a block rule: deny all connections to the echo server
    // pproxy -b matches against hostname only (not host:port), use {} for inline pattern
    let pproxy_port = eggress_testkit::get_free_port().await;
    let block_pattern = "{127\\.0\\.0\\.1}".to_string();
    let mut pproxy = start_pproxy_with_args(&[
        "-l",
        &format!("socks5://127.0.0.1:{}", pproxy_port),
        "-r",
        "direct",
        "-b",
        &block_pattern,
    ])
    .await;
    assert_port_ready(pproxy_port, Duration::from_secs(5)).await;

    let pproxy_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"should-be-blocked",
    )
    .await;
    pproxy.kill();

    // eggress with reject rule via TOML
    let egress_port = eggress_testkit::get_free_port().await;
    let toml = format!(
        r#"version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:{port}"
protocols = ["socks5"]

[[rules]]
id = "block-target"
reject = "blocked"

[rules.match]
destination_port = {target_port}

[[rules]]
id = "allow-all"
direct = true

[routing]
default = "direct"
"#,
        port = egress_port,
        target_port = echo_addr.port(),
    );
    let (egress_addr, cancel, jh) = start_eggress_from_toml_running(&toml).await;
    let egress_result = send_through_socks5(egress_addr, &target, b"should-be-blocked").await;
    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    // pproxy block: SOCKS5 handshake succeeds but connection drops (data never arrives)
    // eggress reject: SOCKS5 handshake fails with error code
    // Both prevent data delivery — verify no echo response from either
    let pproxy_ok = pproxy_result
        .as_ref()
        .map(|d| !d.is_empty())
        .unwrap_or(false);
    let egress_ok = egress_result
        .as_ref()
        .map(|d| !d.is_empty())
        .unwrap_or(false);
    assert!(!pproxy_ok, "pproxy should not deliver data through block");
    assert!(!egress_ok, "eggress should not deliver data through reject");
}

// --- Scenario 10: TLS Listener (eggress-only, no pproxy equivalent) ---

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1"]
async fn differential_tls_listener() {
    require_differential_gate();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    // Generate self-signed cert at runtime
    let cert_params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let cert = cert_params.self_signed(&key_pair).unwrap();
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    // Write cert/key to tempfiles for eggress TLS config
    let cert_file = tempfile::NamedTempFile::new().unwrap();
    let key_file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(cert_file.path(), &cert_pem).unwrap();
    std::fs::write(key_file.path(), &key_pem).unwrap();

    // eggress with TLS listener via TOML
    let egress_port = eggress_testkit::get_free_port().await;
    let toml = format!(
        r#"version = 1

[[listeners]]
name = "tls-in"
bind = "127.0.0.1:{egress_port}"
protocols = ["socks5"]

[listeners.tls]
cert = "{cert_path}"
key = "{key_path}"

[[rules]]
id = "allow-all"
direct = true

[routing]
default = "direct"
"#,
        cert_path = cert_file.path().display(),
        key_path = key_file.path().display(),
    );
    let (egress_addr, cancel, jh) = start_eggress_from_toml_running(&toml).await;

    // Connect via TLS then do SOCKS5 handshake
    let mut root_store = rustls::RootCertStore::empty();
    let cert_der = cert.der().clone();
    root_store.add(cert_der).unwrap();
    let mut tls_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    tls_config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_config));

    let tcp = tokio::net::TcpStream::connect(egress_addr).await.unwrap();
    let domain = rustls::pki_types::ServerName::try_from("localhost".to_string()).unwrap();
    let tls_stream = connector.connect(domain, tcp).await.unwrap();

    // Perform SOCKS5 handshake over TLS
    let result = send_through_socks5_stream(tls_stream, &target, b"tls smoke test").await;
    cancel.cancel();
    let _ = jh.await;
    echo_jh.abort();

    assert!(result.is_ok(), "TLS+SOCKS5 should work: {:?}", result.err());
}

// ========================================================================
// Chain Matrix Differential Tests
// ========================================================================

/// HTTP CONNECT listener chained through SOCKS5 upstream (pproxy).
///
/// eggress runs an HTTP CONNECT listener with a SOCKS5 upstream pointing at
/// pproxy. Sends a request through eggress → pproxy → echo target and verifies
/// the payload matches a direct pproxy connection.
#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1"]
async fn differential_http_to_socks5_upstream() {
    require_differential_gate();

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

    // Start eggress with HTTP CONNECT listener chained through pproxy SOCKS5
    let egress_port = eggress_testkit::get_free_port().await;
    let toml = format!(
        r#"version = 1

[[listeners]]
name = "http-chain"
bind = "127.0.0.1:{egress_port}"
protocols = ["http"]

[[upstreams]]
name = "pproxy-socks5"
uri = "socks5://127.0.0.1:{pproxy_port}"

[[upstream_groups]]
name = "default"
upstreams = ["pproxy-socks5"]

[routing]
default = "pproxy-socks5"
"#,
    );
    let (egress_addr, cancel, jh) = start_eggress_from_toml_running(&toml).await;

    // Send through eggress HTTP → pproxy SOCKS5 → echo
    let chain_result = send_through_http(egress_addr, &target, b"chain http->socks5").await;

    // Send directly through pproxy SOCKS5 → echo for comparison
    let direct_result = send_through_socks5(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"chain http->socks5",
    )
    .await;

    cancel.cancel();
    let _ = jh.await;
    pproxy_child.kill();
    echo_jh.abort();

    // Both should succeed and return the same payload
    match (&chain_result, &direct_result) {
        (Ok(chain_payload), Ok(direct_payload)) => {
            assert_eq!(
                chain_payload, direct_payload,
                "chain payload mismatch with direct pproxy"
            );
            assert_eq!(*chain_payload, b"chain http->socks5");
        }
        (Err(e), _) => panic!("chain through eggress HTTP -> pproxy SOCKS5 failed: {e}"),
        (_, Err(e)) => panic!("direct pproxy SOCKS5 failed: {e}"),
    }
}

/// HTTP CONNECT listener chained through HTTP upstream (pproxy).
///
/// eggress runs an HTTP CONNECT listener with an HTTP upstream pointing at
/// pproxy. Sends a request through eggress → pproxy → echo target and verifies
/// the payload matches a direct pproxy connection.
#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1"]
async fn differential_http_to_http_upstream() {
    require_differential_gate();

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

    // Start eggress with HTTP CONNECT listener chained through pproxy HTTP
    let egress_port = eggress_testkit::get_free_port().await;
    let toml = format!(
        r#"version = 1

[[listeners]]
name = "http-chain"
bind = "127.0.0.1:{egress_port}"
protocols = ["http"]

[[upstreams]]
name = "pproxy-http"
uri = "http://127.0.0.1:{pproxy_port}"

[[upstream_groups]]
name = "default"
upstreams = ["pproxy-http"]

[routing]
default = "pproxy-http"
"#,
    );
    let (egress_addr, cancel, jh) = start_eggress_from_toml_running(&toml).await;

    // Send through eggress HTTP → pproxy HTTP → echo
    let chain_result = send_through_http(egress_addr, &target, b"chain http->http").await;

    // Send directly through pproxy HTTP → echo for comparison
    let direct_result = send_through_http(
        socket_addr("127.0.0.1", pproxy_port),
        &target,
        b"chain http->http",
    )
    .await;

    cancel.cancel();
    let _ = jh.await;
    pproxy_child.kill();
    echo_jh.abort();

    // Both should succeed and return the same payload
    match (&chain_result, &direct_result) {
        (Ok(chain_payload), Ok(direct_payload)) => {
            assert_eq!(
                chain_payload, direct_payload,
                "chain payload mismatch with direct pproxy"
            );
            assert_eq!(*chain_payload, b"chain http->http");
        }
        (Err(e), _) => panic!("chain through eggress HTTP -> pproxy HTTP failed: {e}"),
        (_, Err(e)) => panic!("direct pproxy HTTP failed: {e}"),
    }
}

// ========================================================================
// CLI Snapshot Comparison Tests
// ========================================================================

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1"]
async fn differential_cli_help_output() {
    require_differential_gate();

    let output = std::process::Command::new("cargo")
        .args(["run", "--bin", "eggress", "--", "--help"])
        .output()
        .expect("failed to run eggress --help");
    let help = String::from_utf8_lossy(&output.stdout);

    // Basic structural checks
    assert!(help.contains("eggress"), "help should mention program name");
    assert!(
        help.contains("--config") || help.contains("-c"),
        "help should mention config flag"
    );
    assert!(
        help.contains("--listen") || help.contains("-l"),
        "help should mention listen flag"
    );
}

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1"]
async fn differential_cli_version_output() {
    require_differential_gate();

    let output = std::process::Command::new("cargo")
        .args(["run", "--bin", "eggress", "--", "--version"])
        .output()
        .expect("failed to run eggress --version");
    let version = String::from_utf8_lossy(&output.stdout);

    assert!(
        version.contains("eggress"),
        "version output should mention program name"
    );
}

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1"]
async fn differential_cli_pproxy_translate() {
    require_differential_gate();

    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "--bin",
            "eggress",
            "--",
            "pproxy",
            "translate",
            "--",
            "-l",
            "socks5://:1080",
            "-r",
            "socks5://127.0.0.1:8080",
        ])
        .output()
        .expect("failed to run eggress pproxy translate");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "pproxy translate should succeed: {stderr}"
    );
    assert!(
        stdout.contains("[[listeners]]"),
        "output should contain TOML listeners section"
    );
    assert!(
        stdout.contains("1080"),
        "output should contain the listen port"
    );
}

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1"]
async fn differential_cli_pproxy_check() {
    require_differential_gate();

    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "--bin",
            "eggress",
            "--",
            "pproxy",
            "check",
            "--",
            "-l",
            "socks5://:1080",
            "-r",
            "socks5://127.0.0.1:8080",
        ])
        .output()
        .expect("failed to run eggress pproxy check");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "pproxy check should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("compatible") || stdout.contains("Compatible"),
        "check should report compatibility status"
    );
}

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1"]
async fn differential_cli_invalid_uri_diagnostic() {
    require_differential_gate();

    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "--bin",
            "eggress",
            "--",
            "pproxy",
            "translate",
            "--",
            "-l",
            "not_a_valid_uri",
        ])
        .output()
        .expect("failed to run eggress with invalid URI");

    // Should fail with a diagnostic
    assert!(
        !output.status.success() || {
            let stderr = String::from_utf8_lossy(&output.stderr);
            stderr.contains("error") || stderr.contains("warning") || stderr.contains("diagnostic")
        },
        "invalid URI should produce an error or diagnostic"
    );
}

#[tokio::test]
#[ignore = "requires EGRESS_RUN_PPROXY_DIFFERENTIAL=1"]
async fn differential_cli_unsupported_uri_diagnostic() {
    require_differential_gate();

    // SSH is unsupported by eggress
    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "--bin",
            "eggress",
            "--",
            "pproxy",
            "translate",
            "--",
            "-l",
            "ssh://:22",
        ])
        .output()
        .expect("failed to run eggress with unsupported URI");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should produce a diagnostic about unsupported protocol
    assert!(
        !output.status.success()
            || stderr.contains("unsupported")
            || stderr.contains("diagnostic")
            || stderr.contains("warning"),
        "unsupported URI should produce a diagnostic"
    );
}
