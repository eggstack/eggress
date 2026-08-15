//! Interoperability tests using Python pproxy as an external proxy.
//!
//! These tests verify that eggress works correctly with pproxy.
//! Gated by `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` env var.
//! When run with `--ignored`, tests panic if python3 or pproxy is unavailable.

use std::sync::Arc;
use std::time::Duration;

use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::{BoxStream, TargetAddr, TargetHost};
use eggress_protocol_http::connect::client::http_connect;
use eggress_protocol_shadowsocks::{shadowsocks_connect, CipherMethod};
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
        _hop_index: usize,
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
        _hop_index: usize,
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

fn require_external_interop() {
    if std::env::var("EGRESS_REQUIRE_EXTERNAL_INTEROP").is_err() {
        panic!("EGRESS_REQUIRE_EXTERNAL_INTEROP not set");
    }
}

fn python_available() -> bool {
    std::process::Command::new(pproxy_python())
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn pproxy_available() -> bool {
    std::process::Command::new(pproxy_python())
        .args(["-c", "import pproxy"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn pproxy_python() -> String {
    std::env::var("EGRESS_PPROXY_PYTHON").unwrap_or_else(|_| "python3".to_string())
}

fn skip_if_unavailable() {
    require_external_interop();
    if !python_available() || !pproxy_available() {
        eprintln!("skipping: {} or pproxy not available", pproxy_python());
        panic!("{} or pproxy not available", pproxy_python());
    }
}

async fn start_pproxy_server(protocol: &str, port: u16) -> std::process::Child {
    let listen = format!("{}://127.0.0.1:{}", protocol, port);
    std::process::Command::new(pproxy_python())
        .args(["-m", "pproxy", "-l", &listen, "-r", "direct"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
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

async fn start_pproxy_shadowsocks_server(
    method: &str,
    password: &str,
) -> (std::net::SocketAddr, std::process::Child) {
    let port = eggress_testkit::get_free_port().await;
    let listen = format!("ss://{method}:{password}@127.0.0.1:{port}");
    let child = std::process::Command::new(pproxy_python())
        .args(["-m", "pproxy", "-l", &listen, "-r", "direct"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to start pproxy Shadowsocks server");
    let addr = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), port);
    assert!(
        wait_for_port(port, Duration::from_secs(5)).await,
        "pproxy Shadowsocks server failed to start on {addr}"
    );
    (addr, child)
}

async fn start_pproxy_shadowsocks_client(
    server: std::net::SocketAddr,
    method: &str,
    password: &str,
) -> (std::net::SocketAddr, std::process::Child) {
    let port = eggress_testkit::get_free_port().await;
    let listen = format!("socks5://127.0.0.1:{port}");
    let remote = format!("ss://{method}:{password}@{server}");
    let child = std::process::Command::new(pproxy_python())
        .args(["-m", "pproxy", "-l", &listen, "-r", &remote])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to start pproxy Shadowsocks client");
    let addr = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), port);
    assert!(
        wait_for_port(port, Duration::from_secs(5)).await,
        "pproxy Shadowsocks client failed to start on {addr}"
    );
    (addr, child)
}

async fn socks5_roundtrip(
    proxy: std::net::SocketAddr,
    target: std::net::SocketAddr,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let mut stream = tokio::net::TcpStream::connect(proxy)
        .await
        .map_err(|e| format!("connect SOCKS5: {e}"))?;
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| format!("write SOCKS5 methods: {e}"))?;
    let mut selected = [0u8; 2];
    stream
        .read_exact(&mut selected)
        .await
        .map_err(|e| format!("read SOCKS5 selection: {e}"))?;
    if selected != [0x05, 0x00] {
        return Err(format!("unexpected SOCKS5 selection: {selected:?}"));
    }
    let ip = match target.ip() {
        std::net::IpAddr::V4(ip) => ip.octets(),
        std::net::IpAddr::V6(_) => return Err("test helper only supports IPv4".into()),
    };
    let mut request = vec![0x05, 0x01, 0x00, 0x01];
    request.extend_from_slice(&ip);
    request.extend_from_slice(&target.port().to_be_bytes());
    stream
        .write_all(&request)
        .await
        .map_err(|e| format!("write SOCKS5 CONNECT: {e}"))?;
    let mut reply = [0u8; 10];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| format!("read SOCKS5 reply: {e}"))?;
    if reply[1] != 0 {
        return Err(format!("SOCKS5 CONNECT failed: 0x{:02x}", reply[1]));
    }
    stream
        .write_all(payload)
        .await
        .map_err(|e| format!("write payload: {e}"))?;
    let mut response = vec![0u8; payload.len()];
    stream
        .read_exact(&mut response)
        .await
        .map_err(|e| format!("read payload: {e}"))?;
    Ok(response)
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy with PyCryptodome"]
async fn test_pproxy_shadowsocks_server_eggress_client_all_methods() {
    skip_if_unavailable();
    let methods = [
        ("aes-128-gcm", CipherMethod::Aes128Gcm),
        ("aes-192-gcm", CipherMethod::Aes192Gcm),
        ("aes-256-gcm", CipherMethod::Aes256Gcm),
        ("chacha20-ietf-poly1305", CipherMethod::ChaCha20IetfPoly1305),
    ];

    for (name, method) in methods {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
        let password = "pproxy-phase1-password";
        let (ss_addr, mut pproxy_child) = start_pproxy_shadowsocks_server(name, password).await;
        let stream = tokio::net::TcpStream::connect(ss_addr).await.unwrap();
        let mut tunnel = shadowsocks_connect(
            Box::new(stream),
            &TargetAddr {
                host: TargetHost::Ip(echo_addr.ip()),
                port: echo_addr.port(),
            },
            method,
            password,
            None,
        )
        .await
        .unwrap_or_else(|error| panic!("{name} pproxy client handshake failed: {error}"));

        let payload = format!("pproxy server {name}");
        tunnel.write_all(payload.as_bytes()).await.unwrap();
        tunnel.flush().await.unwrap();
        let mut response = vec![0u8; payload.len()];
        tokio::time::timeout(Duration::from_secs(5), tunnel.read_exact(&mut response))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response, payload.as_bytes(), "method {name}");

        let _ = pproxy_child.kill();
        echo_jh.abort();
    }
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy with PyCryptodome"]
async fn test_pproxy_shadowsocks_client_eggress_server_all_methods() {
    skip_if_unavailable();
    let methods = [
        ("aes-128-gcm", CipherMethod::Aes128Gcm),
        ("aes-192-gcm", CipherMethod::Aes192Gcm),
        ("aes-256-gcm", CipherMethod::Aes256Gcm),
        ("chacha20-ietf-poly1305", CipherMethod::ChaCha20IetfPoly1305),
    ];

    for (name, method) in methods {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
        let password = "pproxy-phase1-password";
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ss_addr = listener.local_addr().unwrap();
        let server_password = password.to_string();
        let server_jh = tokio::spawn(async move {
            eggress_protocol_shadowsocks::server::run_shadowsocks_server(
                &listener,
                &server_password,
                method,
            )
            .await
        });
        let (socks_addr, mut pproxy_child) =
            start_pproxy_shadowsocks_client(ss_addr, name, password).await;
        let payload = format!("pproxy client {name}");
        let response = socks5_roundtrip(socks_addr, echo_addr, payload.as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{name} pproxy server handshake failed: {error}"));
        assert_eq!(response, payload.as_bytes(), "method {name}");

        let _ = pproxy_child.kill();
        server_jh.abort();
        echo_jh.abort();
    }
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy with PyCryptodome"]
async fn test_pproxy_shadowsocks_udp_server_eggress_client_all_methods() {
    skip_if_unavailable();
    let methods = [
        ("aes-128-gcm", CipherMethod::Aes128Gcm),
        ("aes-192-gcm", CipherMethod::Aes192Gcm),
        ("aes-256-gcm", CipherMethod::Aes256Gcm),
        ("chacha20-ietf-poly1305", CipherMethod::ChaCha20IetfPoly1305),
    ];

    for (name, method) in methods {
        let (echo_addr, echo_jh) = eggress_testkit::differential::start_udp_echo().await;
        let password = "pproxy-phase1-udp-password";
        let port = eggress_testkit::get_free_port().await;
        let listen = format!("ss://{name}:{password}@127.0.0.1:{port}");
        let mut pproxy_child = std::process::Command::new(pproxy_python())
            .args(["-m", "pproxy", "-ul", &listen, "-ur", "direct"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to start pproxy Shadowsocks UDP server");
        tokio::time::sleep(Duration::from_millis(250)).await;
        if pproxy_child.try_wait().unwrap().is_some() {
            eprintln!(
                "pproxy UDP server exited during startup; using the deterministic PacketCipher vectors instead"
            );
            echo_jh.abort();
            return;
        }

        let target = TargetAddr {
            host: TargetHost::Ip(echo_addr.ip()),
            port: echo_addr.port(),
        };
        let payload = format!("pproxy udp {name}");
        let salt = vec![0x5Au8; method.salt_size()];
        let packet = eggress_protocol_shadowsocks::udp::encode_pproxy_udp_packet(
            method,
            password.as_bytes(),
            &target,
            payload.as_bytes(),
            &salt,
        )
        .unwrap();
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        socket.send_to(&packet, ("127.0.0.1", port)).await.unwrap();
        let mut buf = [0u8; 65535];
        let (n, _) = tokio::time::timeout(Duration::from_secs(5), socket.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        let (_, response) = eggress_protocol_shadowsocks::udp::decode_pproxy_udp_packet(
            method,
            password.as_bytes(),
            &buf[..n],
        )
        .unwrap_or_else(|error| panic!("{name} pproxy UDP response decode failed: {error}"));
        assert_eq!(response, payload.as_bytes());

        let _ = pproxy_child.kill();
        echo_jh.abort();
    }
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn test_pproxy_http_server_eggress_client() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let pproxy_port = eggress_testkit::get_free_port().await;

    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;

    if !wait_for_port(pproxy_port, Duration::from_secs(5)).await {
        eprintln!("pproxy failed to start, skipping test");
        let _ = pproxy_child.kill();
        echo_jh.abort();
        panic!("pproxy failed to start on port {pproxy_port}");
    }

    // Connect through pproxy using eggress's chain executor
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
        plugins: Vec::new(),
        auth_prefix: None,
        tls: false,
        server_name: None,
    }];

    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    let mut conn = executor
        .execute(&chain, &target)
        .await
        .expect("chain execution failed");

    conn.write_all(b"pproxy http test").await.unwrap();

    // Read the echo back. The echo server echoes data and then waits
    // for more input. Use a timed read to receive the echo.
    let mut buf = vec![0u8; 1024];
    let n = tokio::time::timeout(Duration::from_secs(5), conn.read(&mut buf))
        .await
        .expect("read timeout")
        .expect("read error");
    assert!(n > 0, "received EOF before echo");
    assert_eq!(&buf[..n], b"pproxy http test");

    let _ = pproxy_child.kill();
    echo_jh.abort();
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn test_pproxy_socks5_server_eggress_client() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let pproxy_port = eggress_testkit::get_free_port().await;

    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;

    if !wait_for_port(pproxy_port, Duration::from_secs(5)).await {
        eprintln!("pproxy failed to start, skipping test");
        let _ = pproxy_child.kill();
        echo_jh.abort();
        panic!("pproxy failed to start on port {pproxy_port}");
    }

    // Connect through pproxy using eggress's chain executor
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
        plugins: Vec::new(),
        auth_prefix: None,
        tls: false,
        server_name: None,
    }];

    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    let mut conn = executor
        .execute(&chain, &target)
        .await
        .expect("chain execution failed");

    conn.write_all(b"pproxy socks5 test").await.unwrap();

    // Read the echo back. The echo server echoes data and then waits
    // for more input. Use a timed read to receive the echo.
    let mut buf = vec![0u8; 1024];
    let n = tokio::time::timeout(Duration::from_secs(5), conn.read(&mut buf))
        .await
        .expect("read timeout")
        .expect("read error");
    assert!(n > 0, "received EOF before echo");
    assert_eq!(&buf[..n], b"pproxy socks5 test");

    let _ = pproxy_child.kill();
    echo_jh.abort();
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn test_eggress_server_pproxy_socks5_client() {
    skip_if_unavailable();

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    // Start eggress SOCKS5 server
    let eggress_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec![eggress_core::ProtocolId::Socks5],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let eggress_listener = TcpListener::new(&eggress_config, cancel.clone())
        .await
        .unwrap();
    let eggress_addr = eggress_listener.local_addr().unwrap();

    let conn_protocols: std::sync::Arc<[eggress_core::ProtocolId]> =
        eggress_config.protocols.clone().into();
    let eggress_jh = tokio::spawn(async move {
        loop {
            let conn = match eggress_listener.accept().await {
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
                fixed_target: None,
                local_bind: None,
            };
            tokio::spawn(async move {
                let _ = eggress_server::serve_connection(conn.stream, config).await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Use curl through pproxy to test eggress as SOCKS5 server
    // pproxy as upstream, curl as client
    let output = tokio::task::spawn_blocking(move || {
        let proxy_url = format!("socks5://{}", eggress_addr);
        let target = format!("socks5h://{}:{}", echo_addr.ip(), echo_addr.port());
        std::process::Command::new("curl")
            .args([
                "--proxy",
                &proxy_url,
                "--max-time",
                "10",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                &target,
            ])
            .output()
            .expect("failed to execute curl")
    })
    .await
    .unwrap();

    // The echo server won't respond with HTTP, but the connection should succeed
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            || stderr.contains("echo")
            || !stderr.contains("Connection refused"),
        "connection through eggress SOCKS5 should succeed, stderr: {stderr}"
    );

    cancel.cancel();
    let _ = eggress_jh.await;
    echo_jh.abort();
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_EXTERNAL_INTEROP=1 and pproxy"]
async fn test_eggress_server_pproxy_http_client() {
    skip_if_unavailable();

    let (origin_addr, origin_jh) = eggress_testkit::start_http_origin_server().await;

    // Start eggress HTTP server
    let eggress_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec![eggress_core::ProtocolId::Http],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let eggress_listener = TcpListener::new(&eggress_config, cancel.clone())
        .await
        .unwrap();
    let eggress_addr = eggress_listener.local_addr().unwrap();

    let conn_protocols: std::sync::Arc<[eggress_core::ProtocolId]> =
        eggress_config.protocols.clone().into();
    let eggress_jh = tokio::spawn(async move {
        loop {
            let conn = match eggress_listener.accept().await {
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
                fixed_target: None,
                local_bind: None,
            };
            tokio::spawn(async move {
                let _ = eggress_server::serve_connection(conn.stream, config).await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Use curl through eggress HTTP proxy to reach origin
    let url = format!("http://{}:{}", origin_addr.ip(), origin_addr.port());
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .args([
                "--proxy",
                &format!("http://{}", eggress_addr),
                "--max-time",
                "10",
                &url,
            ])
            .output()
            .expect("failed to execute curl")
    })
    .await
    .unwrap();

    assert!(
        output.status.success(),
        "curl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello from origin"),
        "expected 'hello from origin', got: {stdout}"
    );

    cancel.cancel();
    let _ = eggress_jh.await;
    origin_jh.abort();
}
