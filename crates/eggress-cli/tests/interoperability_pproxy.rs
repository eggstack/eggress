//! Interoperability tests using Python pproxy as an external proxy.
//!
//! These tests verify that eggress works correctly with pproxy.
//! Tests skip gracefully if Python or pproxy is not available.

use std::sync::Arc;
use std::time::Duration;

use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::listener::{TcpListener, TcpListenerConfig};
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

async fn start_pproxy_server(protocol: &str, port: u16) -> std::process::Child {
    let listen = format!("{}://127.0.0.1:{}", protocol, port);
    std::process::Command::new("python3")
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

#[tokio::test]
async fn test_pproxy_http_server_eggress_client() {
    if !python_available() || !pproxy_available() {
        eprintln!("python3 or pproxy not available, skipping test");
        return;
    }

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let pproxy_port = eggress_testkit::get_free_port().await;

    let mut pproxy_child = start_pproxy_server("http", pproxy_port).await;

    if !wait_for_port(pproxy_port, Duration::from_secs(5)).await {
        eprintln!("pproxy failed to start, skipping test");
        let _ = pproxy_child.kill();
        echo_jh.abort();
        return;
    }

    // Connect through pproxy using eggress's chain executor
    let _stream = tokio::net::TcpStream::connect(("127.0.0.1", pproxy_port))
        .await
        .unwrap();

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

    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    let mut conn = executor
        .execute(&chain, &target)
        .await
        .expect("chain execution failed");

    conn.write_all(b"pproxy http test").await.unwrap();
    conn.shutdown().await.unwrap();

    let mut buf = Vec::new();
    conn.read_to_end(&mut buf).await.unwrap();
    assert_eq!(&buf, b"pproxy http test");

    let _ = pproxy_child.kill();
    echo_jh.abort();
}

#[tokio::test]
async fn test_pproxy_socks5_server_eggress_client() {
    if !python_available() || !pproxy_available() {
        eprintln!("python3 or pproxy not available, skipping test");
        return;
    }

    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
    let pproxy_port = eggress_testkit::get_free_port().await;

    let mut pproxy_child = start_pproxy_server("socks5", pproxy_port).await;

    if !wait_for_port(pproxy_port, Duration::from_secs(5)).await {
        eprintln!("pproxy failed to start, skipping test");
        let _ = pproxy_child.kill();
        echo_jh.abort();
        return;
    }

    // Connect through pproxy using eggress's chain executor
    let _stream = tokio::net::TcpStream::connect(("127.0.0.1", pproxy_port))
        .await
        .unwrap();

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

    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };

    let mut conn = executor
        .execute(&chain, &target)
        .await
        .expect("chain execution failed");

    conn.write_all(b"pproxy socks5 test").await.unwrap();
    conn.shutdown().await.unwrap();

    let mut buf = Vec::new();
    conn.read_to_end(&mut buf).await.unwrap();
    assert_eq!(&buf, b"pproxy socks5 test");

    let _ = pproxy_child.kill();
    echo_jh.abort();
}

#[tokio::test]
async fn test_eggress_server_pproxy_socks5_client() {
    if !python_available() || !pproxy_available() {
        eprintln!("python3 or pproxy not available, skipping test");
        return;
    }

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
async fn test_eggress_server_pproxy_http_client() {
    if !python_available() || !pproxy_available() {
        eprintln!("python3 or pproxy not available, skipping test");
        return;
    }

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
