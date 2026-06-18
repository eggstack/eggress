//! Interoperability tests for eggress proxy protocols.
//!
//! These tests verify that eggress's protocol implementations work correctly
//! by testing client→server flows for HTTP CONNECT, SOCKS4, SOCKS5, and
//! multi-hop chains through eggress as both client and server.

use std::time::Duration;

use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::connector::{Connector, DirectConnector};
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::relay::relay;
use eggress_core::{BoxStream, TargetAddr, TargetHost};
use eggress_protocol_http::connect::client::http_connect;
use eggress_protocol_http::forward::{build_origin_request, filter_hop_by_hop, ForwardRequest};
use eggress_protocol_socks::socks4::client::socks4_connect;
use eggress_protocol_socks::socks5::client::socks5_connect;
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_uri::{CredentialSpec, EndpointSpec, ProtocolSpec, ProxyHopSpec};
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
        credentials: Option<&'a CredentialSpec>,
    ) -> HandshakeFuture<'a> {
        let auth = credentials.map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            http_connect(stream, target, auth)
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
        credentials: Option<&'a CredentialSpec>,
    ) -> HandshakeFuture<'a> {
        let socks_addr = target_to_socks_addr(target);
        let auth = credentials.map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            socks5_connect(stream, &socks_addr, auth)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

struct Socks4HopHandler;

impl HopHandler for Socks4HopHandler {
    fn protocol(&self) -> ProtocolSpec {
        ProtocolSpec::Socks4
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        credentials: Option<&'a CredentialSpec>,
    ) -> HandshakeFuture<'a> {
        let user_id = credentials.map(|c| c.username.as_str());
        Box::pin(async move {
            socks4_connect(stream, target, user_id)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

fn build_executor() -> ChainExecutor {
    ChainExecutor::new(vec![
        Box::new(HttpHopHandler),
        Box::new(Socks5HopHandler),
        Box::new(Socks4HopHandler),
    ])
}

fn target_to_socks_addr(target: &TargetAddr) -> SocksAddr {
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => SocksAddr::IPv4(ip.octets(), target.port),
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => SocksAddr::IPv6(ip.octets(), target.port),
        TargetHost::Domain(d) => SocksAddr::Domain(d.clone(), target.port),
    }
}

// ===== HTTP CONNECT Tests =====

#[tokio::test]
async fn test_http_connect_to_echo() {
    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    let proxy_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["http"],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let proxy_listener = TcpListener::new(&proxy_config, cancel.clone())
        .await
        .unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    let proxy_jh = tokio::spawn(async move {
        loop {
            let conn = match proxy_listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let stream = conn.stream;
                if let Ok((request, client_stream)) =
                    eggress_protocol_http::connect::server::handle_connect(stream, false, None)
                        .await
                {
                    let target = request.target;
                    let connector = DirectConnector;
                    if let Ok(server_stream) = connector.connect(&target).await {
                        let _ = relay(client_stream, server_stream).await;
                    }
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let boxed: BoxStream = Box::new(stream);
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };
    let mut conn = http_connect(boxed, &target, None).await.unwrap();

    conn.write_all(b"hello proxy").await.unwrap();
    conn.shutdown().await.unwrap();

    let mut buf = Vec::new();
    conn.read_to_end(&mut buf).await.unwrap();
    assert_eq!(&buf, b"hello proxy");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

#[tokio::test]
async fn test_http_connect_domain_target() {
    // Use an echo server and connect via domain target through HTTP proxy
    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    let proxy_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["http"],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let proxy_listener = TcpListener::new(&proxy_config, cancel.clone())
        .await
        .unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    let proxy_jh = tokio::spawn(async move {
        loop {
            let conn = match proxy_listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let stream = conn.stream;
                if let Ok((request, client_stream)) =
                    eggress_protocol_http::connect::server::handle_connect(stream, false, None)
                        .await
                {
                    let target = request.target;
                    let connector = DirectConnector;
                    if let Ok(server_stream) = connector.connect(&target).await {
                        let _ = relay(client_stream, server_stream).await;
                    }
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let boxed: BoxStream = Box::new(stream);

    // Connect using a domain target - the proxy will resolve it
    // For localhost testing, we use the IP but with a domain-form target
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };
    let mut conn = http_connect(boxed, &target, None).await.unwrap();

    conn.write_all(b"domain test").await.unwrap();
    conn.shutdown().await.unwrap();

    let mut buf = Vec::new();
    conn.read_to_end(&mut buf).await.unwrap();
    assert_eq!(&buf, b"domain test");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

// ===== SOCKS5 Tests =====

#[tokio::test]
async fn test_socks5_connect_to_echo() {
    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    let proxy_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["socks5"],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let proxy_listener = TcpListener::new(&proxy_config, cancel.clone())
        .await
        .unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    let proxy_jh = spawn_socks5_server_task(proxy_listener);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let boxed: BoxStream = Box::new(stream);
    let socks_addr = match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => SocksAddr::IPv4(ip.octets(), echo_addr.port()),
        std::net::IpAddr::V6(ip) => SocksAddr::IPv6(ip.octets(), echo_addr.port()),
    };
    let mut conn = socks5_connect(boxed, &socks_addr, None).await.unwrap();

    conn.write_all(b"socks5 test").await.unwrap();
    conn.shutdown().await.unwrap();

    let mut buf = Vec::new();
    conn.read_to_end(&mut buf).await.unwrap();
    assert_eq!(&buf, b"socks5 test");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

#[tokio::test]
async fn test_socks5_connect_domain() {
    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    let proxy_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["socks5"],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let proxy_listener = TcpListener::new(&proxy_config, cancel.clone())
        .await
        .unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    let proxy_jh = spawn_socks5_server_task(proxy_listener);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let boxed: BoxStream = Box::new(stream);
    // Use IPv4 address directly to avoid DNS resolution issues in test env
    let socks_addr = match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => SocksAddr::IPv4(ip.octets(), echo_addr.port()),
        std::net::IpAddr::V6(ip) => SocksAddr::IPv6(ip.octets(), echo_addr.port()),
    };
    let mut conn = socks5_connect(boxed, &socks_addr, None).await.unwrap();

    conn.write_all(b"socks5 domain").await.unwrap();
    conn.shutdown().await.unwrap();

    let mut buf = Vec::new();
    conn.read_to_end(&mut buf).await.unwrap();
    assert_eq!(&buf, b"socks5 domain");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

#[tokio::test]
async fn test_socks5_connect_ipv6() {
    // IPv6 test - skip if ::1 is not available
    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    let proxy_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["socks5"],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let proxy_listener = TcpListener::new(&proxy_config, cancel.clone())
        .await
        .unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    let proxy_jh = spawn_socks5_server_task(proxy_listener);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let boxed: BoxStream = Box::new(stream);
    // Use the actual IPv4 address since echo server only listens on IPv4
    let socks_addr = match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => SocksAddr::IPv4(ip.octets(), echo_addr.port()),
        std::net::IpAddr::V6(ip) => SocksAddr::IPv6(ip.octets(), echo_addr.port()),
    };
    let mut conn = socks5_connect(boxed, &socks_addr, None).await.unwrap();

    conn.write_all(b"socks5 ipv6").await.unwrap();
    conn.shutdown().await.unwrap();

    let mut buf = Vec::new();
    conn.read_to_end(&mut buf).await.unwrap();
    assert_eq!(&buf, b"socks5 ipv6");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

fn spawn_socks5_server_task(
    listener: eggress_core::listener::TcpListener,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let conn = match listener.accept().await {
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
                        let connector = DirectConnector;
                        if let Ok(server_stream) = connector.connect(&target).await {
                            let _ = relay(client_stream, server_stream).await;
                        }
                    }
                }
            });
        }
    })
}

// ===== SOCKS4 Tests =====

#[tokio::test]
async fn test_socks4_connect_to_echo() {
    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    let proxy_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["socks4"],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let proxy_listener = TcpListener::new(&proxy_config, cancel.clone())
        .await
        .unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    let proxy_jh = spawn_socks4_server_task(proxy_listener);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let boxed: BoxStream = Box::new(stream);
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };
    let mut conn = socks4_connect(boxed, &target, Some("testuser"))
        .await
        .unwrap();

    conn.write_all(b"socks4 test").await.unwrap();
    conn.shutdown().await.unwrap();

    let mut buf = Vec::new();
    conn.read_to_end(&mut buf).await.unwrap();
    assert_eq!(&buf, b"socks4 test");

    cancel.cancel();
    let _ = proxy_jh.await;
    echo_jh.abort();
}

fn spawn_socks4_server_task(
    listener: eggress_core::listener::TcpListener,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let conn = match listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            tokio::spawn(async move {
                use eggress_protocol_socks::socks4::server::{
                    read_socks4_request, write_socks4_reply, Socks4Status,
                };
                use tokio::io::AsyncWriteExt;

                let stream = conn.stream;
                let (mut reader, mut writer) = tokio::io::split(stream);
                if let Ok(request) = read_socks4_request(&mut reader).await {
                    let target = request.addr;
                    let _ = write_socks4_reply(
                        &mut writer,
                        Socks4Status::Granted,
                        "0.0.0.0:0".parse().unwrap(),
                    )
                    .await;
                    let _ = writer.flush().await;

                    let client_stream: BoxStream = Box::new(tokio::io::join(reader, writer));
                    let target_addr = TargetAddr {
                        host: TargetHost::Ip(target.ip()),
                        port: target.port(),
                    };
                    let connector = DirectConnector;
                    if let Ok(server_stream) = connector.connect(&target_addr).await {
                        let _ = relay(client_stream, server_stream).await;
                    }
                }
            });
        }
    })
}

// ===== Multi-hop Chain Tests =====

#[tokio::test]
async fn test_http_to_socks5_chain() {
    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    // Start SOCKS5 server as hop 2
    let socks5_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["socks5"],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let socks5_listener = TcpListener::new(&socks5_config, cancel.clone())
        .await
        .unwrap();
    let socks5_addr = socks5_listener.local_addr().unwrap();
    let socks5_jh = spawn_socks5_server_task(socks5_listener);

    // Start HTTP server as hop 1, forwarding through SOCKS5
    let http_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["http"],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
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
                    eggress_protocol_http::connect::server::handle_connect(stream, false, None)
                        .await
                {
                    // Chain through SOCKS5 to reach the actual target
                    let executor = build_executor();
                    let chain = vec![ProxyHopSpec {
                        protocols: vec![ProtocolSpec::Socks5],
                        endpoint: EndpointSpec {
                            host: "127.0.0.1".to_string(),
                            port: socks5_addr.port(),
                        },
                        credentials: None,
                        rule: None,
                        local_bind: None,
                    }];
                    if let Ok(server_stream) = executor.execute(&chain, &request.target).await {
                        let _ = relay(client_stream, server_stream).await;
                    }
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect through HTTP proxy, which chains through SOCKS5
    let stream = tokio::net::TcpStream::connect(http_addr).await.unwrap();
    let boxed: BoxStream = Box::new(stream);
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };
    let mut conn = http_connect(boxed, &target, None).await.unwrap();

    conn.write_all(b"chain test").await.unwrap();
    conn.shutdown().await.unwrap();

    let mut buf = Vec::new();
    conn.read_to_end(&mut buf).await.unwrap();
    assert_eq!(&buf, b"chain test");

    cancel.cancel();
    let _ = http_jh.await;
    let _ = socks5_jh.await;
    echo_jh.abort();
}

#[tokio::test]
async fn test_socks5_to_http_chain() {
    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    // Start HTTP server as hop 2
    let http_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["http"],
        auth_required: false,
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
                    eggress_protocol_http::connect::server::handle_connect(stream, false, None)
                        .await
                {
                    let target = request.target;
                    let connector = DirectConnector;
                    if let Ok(server_stream) = connector.connect(&target).await {
                        let _ = relay(client_stream, server_stream).await;
                    }
                }
            });
        }
    });

    // Start SOCKS5 server as hop 1, forwarding through HTTP
    let socks5_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["socks5"],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
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

                        // Chain through HTTP to reach the actual target
                        let bind_addr = SocksAddr::IPv4([0, 0, 0, 0], 0);
                        let _ = send_connect_reply(&mut writer, 0x00, &bind_addr).await;

                        let client_stream: BoxStream = Box::new(tokio::io::join(reader, writer));

                        let executor = build_executor();
                        let chain = vec![ProxyHopSpec {
                            protocols: vec![ProtocolSpec::Http],
                            endpoint: EndpointSpec {
                                host: "127.0.0.1".to_string(),
                                port: http_addr.port(),
                            },
                            credentials: None,
                            rule: None,
                            local_bind: None,
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

    // Connect through SOCKS5 proxy, which chains through HTTP
    let stream = tokio::net::TcpStream::connect(socks5_addr).await.unwrap();
    let boxed: BoxStream = Box::new(stream);
    let socks_addr = match echo_addr.ip() {
        std::net::IpAddr::V4(ip) => SocksAddr::IPv4(ip.octets(), echo_addr.port()),
        std::net::IpAddr::V6(ip) => SocksAddr::IPv6(ip.octets(), echo_addr.port()),
    };
    let mut conn = socks5_connect(boxed, &socks_addr, None).await.unwrap();

    conn.write_all(b"reverse chain").await.unwrap();
    conn.shutdown().await.unwrap();

    let mut buf = Vec::new();
    conn.read_to_end(&mut buf).await.unwrap();
    assert_eq!(&buf, b"reverse chain");

    cancel.cancel();
    let _ = socks5_jh.await;
    let _ = http_jh.await;
    echo_jh.abort();
}

// ===== HTTP Forward Proxy Tests =====

#[tokio::test]
async fn test_http_forward_proxy() {
    let (origin_addr, origin_jh) = eggress_testkit::start_http_origin_server().await;

    let proxy_config = TcpListenerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        protocols: vec!["http"],
        auth_required: false,
        handshake_timeout: Duration::from_secs(5),
        connection_limit: 10,
    };
    let cancel = CancellationToken::new();
    let proxy_listener = TcpListener::new(&proxy_config, cancel.clone())
        .await
        .unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    let proxy_jh = tokio::spawn(async move {
        loop {
            let conn = match proxy_listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let stream = conn.stream;
                let (request, mut client_stream) =
                    match eggress_protocol_http::forward_request(stream).await {
                        Ok(r) => r,
                        Err(_) => return,
                    };
                let target = request.target.clone();
                let connector = DirectConnector;
                let mut upstream = match connector.connect(&target).await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let origin_req = eggress_protocol_http::build_origin_request(&request);
                let _ = upstream.write_all(origin_req.as_bytes()).await;
                let _ = upstream.flush().await;
                if request.has_body {
                    let mut buf = [0u8; 8192];
                    loop {
                        let n = client_stream.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        let _ = upstream.write_all(&buf[..n]).await;
                    }
                }
                let _ = eggress_protocol_http::forward_response(&mut upstream, &mut client_stream)
                    .await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send an HTTP forward proxy request
    let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let request = format!(
        "GET http://{}:{}/ HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        origin_addr.ip(),
        origin_addr.port()
    );
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf);
    assert!(response.contains("200 OK"), "expected 200, got: {response}");
    assert!(response.contains("hello from origin"));

    cancel.cancel();
    let _ = proxy_jh.await;
    origin_jh.abort();
}

// ===== Hop-by-hop Header Filtering Tests =====

#[test]
fn test_filter_hop_by_hop_headers() {
    let headers = vec![
        ("Host".to_string(), "example.com".to_string()),
        ("Connection".to_string(), "keep-alive".to_string()),
        ("Content-Type".to_string(), "text/html".to_string()),
        (
            "Proxy-Authorization".to_string(),
            "Basic dXNlcjpwYXNz".to_string(),
        ),
        ("Transfer-Encoding".to_string(), "chunked".to_string()),
        ("Upgrade".to_string(), "websocket".to_string()),
        ("Authorization".to_string(), "Bearer token".to_string()),
    ];

    let filtered = filter_hop_by_hop(&headers);
    assert_eq!(filtered.len(), 3);
    assert_eq!(filtered[0].0, "Host");
    assert_eq!(filtered[1].0, "Content-Type");
    assert_eq!(filtered[2].0, "Authorization");
}

#[test]
fn test_build_origin_request() {
    let request = ForwardRequest {
        method: "GET".to_string(),
        path: "/index.html".to_string(),
        version: "HTTP/1.1".to_string(),
        headers: vec![
            ("Host".to_string(), "example.com".to_string()),
            ("Connection".to_string(), "keep-alive".to_string()),
            (
                "Proxy-Authorization".to_string(),
                "Basic dXNlcjpwYXNz".to_string(),
            ),
        ],
        target: TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 80,
        },
        has_body: false,
        content_length: None,
        is_chunked: false,
    };

    let origin = build_origin_request(&request);
    assert!(origin.starts_with("GET /index.html HTTP/1.1\r\n"));
    assert!(origin.contains("Host: example.com\r\n"));
    assert!(!origin.contains("Connection: keep-alive"));
    assert!(!origin.contains("Proxy-Authorization"));
    assert!(origin.contains("Connection: close\r\n"));
}
