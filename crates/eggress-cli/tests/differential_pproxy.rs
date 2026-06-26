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
