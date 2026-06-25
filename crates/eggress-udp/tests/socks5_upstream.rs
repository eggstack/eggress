use std::sync::atomic::Ordering;
use std::sync::Arc;

use eggress_core::{ClientIdentity, UpstreamId};
use eggress_routing::upstream::{GroupFallback, UpstreamGroup, UpstreamRuntime};
use eggress_routing::{CompiledRule, MatchExpr, RouteActionSpec, Router, RuleId, UpstreamGroupId};
use eggress_udp::assoc::UdpAssociation;
use eggress_udp::assoc::UdpAssociationId;
use eggress_udp::codec::decode_packet;
use eggress_udp::limits::UdpLimits;
use eggress_udp::metrics::UdpMetrics;
use eggress_udp::registry::UdpAssociationRegistry;
use eggress_udp::relay::{udp_relay_loop, RelayConfig};
use eggress_udp::testkit::{Socks5TestMode, Socks5TestServerConfig, Socks5UdpTestServer};
use eggress_udp::upstream_socks5::{
    open_socks5_udp_upstream, Socks5UdpUpstreamConfig, UdpUpstreamError,
};
use eggress_uri::{EndpointSpec, ProtocolSpec, ProxyChainSpec, ProxyHopSpec};
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

fn test_addr() -> std::net::SocketAddr {
    "127.0.0.1:1080".parse().unwrap()
}

fn test_assoc() -> Arc<UdpAssociation> {
    Arc::new(UdpAssociation::new(
        UdpAssociationId(1),
        "test".to_string(),
        test_addr(),
        ClientIdentity::Anonymous,
        1,
    ))
}

fn test_registry() -> Arc<UdpAssociationRegistry> {
    Arc::new(UdpAssociationRegistry::new(UdpLimits::default()))
}

fn ipv4_socks5_packet(target: [u8; 4], port: u16, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00, 0x01];
    pkt.extend_from_slice(&target);
    pkt.extend_from_slice(&port.to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

fn socks5_upstream_chain(tcp_addr: std::net::SocketAddr) -> ProxyChainSpec {
    ProxyChainSpec {
        hops: vec![ProxyHopSpec {
            protocols: vec![ProtocolSpec::Socks5],
            endpoint: EndpointSpec {
                host: tcp_addr.ip().to_string(),
                port: tcp_addr.port(),
            },
            credentials: None,
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        }],
    }
}

fn socks5_upstream_chain_with_auth(
    tcp_addr: std::net::SocketAddr,
    username: &str,
    password: &str,
) -> ProxyChainSpec {
    ProxyChainSpec {
        hops: vec![ProxyHopSpec {
            protocols: vec![ProtocolSpec::Socks5],
            endpoint: EndpointSpec {
                host: tcp_addr.ip().to_string(),
                port: tcp_addr.port(),
            },
            credentials: Some(eggress_uri::CredentialSpec {
                username: username.to_string(),
                password: password.to_string(),
            }),
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        }],
    }
}

fn http_upstream_chain(tcp_addr: std::net::SocketAddr) -> ProxyChainSpec {
    ProxyChainSpec {
        hops: vec![ProxyHopSpec {
            protocols: vec![ProtocolSpec::Http],
            endpoint: EndpointSpec {
                host: tcp_addr.ip().to_string(),
                port: tcp_addr.port(),
            },
            credentials: None,
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        }],
    }
}

fn multi_hop_chain(addr1: std::net::SocketAddr, addr2: std::net::SocketAddr) -> ProxyChainSpec {
    ProxyChainSpec {
        hops: vec![
            ProxyHopSpec {
                protocols: vec![ProtocolSpec::Socks5],
                endpoint: EndpointSpec {
                    host: addr1.ip().to_string(),
                    port: addr1.port(),
                },
                credentials: None,
                rule: None,
                local_bind: None,
                tls: false,
                server_name: None,
            },
            ProxyHopSpec {
                protocols: vec![ProtocolSpec::Socks5],
                endpoint: EndpointSpec {
                    host: addr2.ip().to_string(),
                    port: addr2.port(),
                },
                credentials: None,
                rule: None,
                local_bind: None,
                tls: false,
                server_name: None,
            },
        ],
    }
}

fn upstream_group_router(
    chain: ProxyChainSpec,
    fallback: GroupFallback,
) -> Arc<dyn eggress_routing::RouteService> {
    let upstream = Arc::new(UpstreamRuntime::new(
        UpstreamId::new("socks-upstream"),
        chain,
    ));
    let group_id = UpstreamGroupId(std::sync::Arc::from("udp-proxy"));
    let group = UpstreamGroup::new(
        group_id.clone(),
        eggress_routing::scheduler::SchedulerKind::FirstAvailable,
        std::sync::Arc::from(vec![upstream]),
        fallback,
    );
    let rules = vec![CompiledRule {
        id: RuleId(std::sync::Arc::from("to-proxy")),
        matcher: MatchExpr::Any,
        action: RouteActionSpec::UpstreamGroup(group_id.clone()),
    }];
    Arc::new(Router::with_groups(
        rules,
        RouteActionSpec::Direct,
        vec![(group_id, group)],
    ))
}

fn relay_config(routing: Arc<dyn eggress_routing::RouteService>) -> (RelayConfig, Arc<UdpMetrics>) {
    let udp_metrics = Arc::new(UdpMetrics::new());
    let config = RelayConfig {
        routing,
        udp_metrics: udp_metrics.clone(),
        limits: UdpLimits::default(),
        listener: "test".to_string(),
        generation: 1,
        identity: ClientIdentity::Anonymous,
        client_tcp_peer: test_addr(),
        registry: test_registry(),
    };
    (config, udp_metrics)
}

#[tokio::test]
async fn socks5_upstream_echo_no_auth() {
    let upstream = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::Echo,
        relay_addr: None,
    })
    .await
    .unwrap();

    let chain = socks5_upstream_chain(upstream.tcp_addr);
    let (config, _) = relay_config(upstream_group_router(chain, GroupFallback::Reject));

    let assoc = test_assoc();
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_assoc = assoc.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(
            async move { udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await },
        );

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"hello upstream");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout waiting for response")
    .unwrap();

    let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
    assert_eq!(resp.payload, b"hello upstream");

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn socks5_upstream_auth_success() {
    let upstream = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::EchoWithCredentials {
            username: "user".to_string(),
            password: "pass".to_string(),
        },
        relay_addr: None,
    })
    .await
    .unwrap();

    let chain = socks5_upstream_chain_with_auth(upstream.tcp_addr, "user", "pass");
    let (config, _) = relay_config(upstream_group_router(chain, GroupFallback::Reject));

    let assoc = test_assoc();
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_assoc = assoc.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(
            async move { udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await },
        );

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"auth test");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout waiting for response")
    .unwrap();

    let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
    assert_eq!(resp.payload, b"auth test");

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn socks5_upstream_auth_failure_drops() {
    let upstream = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::AuthFailure,
        relay_addr: None,
    })
    .await
    .unwrap();

    let chain = socks5_upstream_chain(upstream.tcp_addr);
    let (config, udp_metrics) = relay_config(upstream_group_router(chain, GroupFallback::Reject));

    let assoc = test_assoc();
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_assoc = assoc.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(
            async move { udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await },
        );

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"should drop");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert_eq!(udp_metrics.dropped_packets.load(Ordering::Relaxed), 1);

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn socks5_upstream_associate_failure_drops() {
    let upstream = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::AssociateFailure { reply_code: 0x01 },
        relay_addr: None,
    })
    .await
    .unwrap();

    let chain = socks5_upstream_chain(upstream.tcp_addr);
    let (config, udp_metrics) = relay_config(upstream_group_router(chain, GroupFallback::Reject));

    let assoc = test_assoc();
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_assoc = assoc.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(
            async move { udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await },
        );

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"assoc fail");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert_eq!(udp_metrics.dropped_packets.load(Ordering::Relaxed), 1);

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn http_upstream_drops_unsupported() {
    let upstream = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::Echo,
        relay_addr: None,
    })
    .await
    .unwrap();

    let chain = http_upstream_chain(upstream.tcp_addr);
    let (config, udp_metrics) = relay_config(upstream_group_router(chain, GroupFallback::Reject));

    let assoc = test_assoc();
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_assoc = assoc.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(
            async move { udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await },
        );

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"http drop");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert_eq!(udp_metrics.dropped_packets.load(Ordering::Relaxed), 1);

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn multi_hop_chain_drops_unsupported() {
    let upstream1 = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::NoAuth,
        relay_addr: None,
    })
    .await
    .unwrap();

    let upstream2 = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::NoAuth,
        relay_addr: None,
    })
    .await
    .unwrap();

    let chain = multi_hop_chain(upstream1.tcp_addr, upstream2.tcp_addr);
    let (config, udp_metrics) = relay_config(upstream_group_router(chain, GroupFallback::Reject));

    let assoc = test_assoc();
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_assoc = assoc.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(
            async move { udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await },
        );

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"multi-hop drop");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert_eq!(udp_metrics.dropped_packets.load(Ordering::Relaxed), 1);

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn upstream_target_flow_idle_cleanup() {
    let upstream = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::Echo,
        relay_addr: None,
    })
    .await
    .unwrap();

    let chain = socks5_upstream_chain(upstream.tcp_addr);
    let udp_metrics = Arc::new(UdpMetrics::new());
    let limits = UdpLimits {
        target_idle_timeout: std::time::Duration::from_millis(150),
        ..UdpLimits::default()
    };
    let config = RelayConfig {
        routing: upstream_group_router(chain, GroupFallback::Reject),
        udp_metrics: udp_metrics.clone(),
        limits,
        listener: "test".to_string(),
        generation: 1,
        identity: ClientIdentity::Anonymous,
        client_tcp_peer: test_addr(),
        registry: test_registry(),
    };

    let assoc = test_assoc();
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_assoc = assoc.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(
            async move { udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await },
        );

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"idle-test");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(udp_metrics.target_flows_active.load(Ordering::Relaxed), 1);

    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    assert_eq!(
        udp_metrics.target_flows_active.load(Ordering::Relaxed),
        0,
        "upstream flow should be evicted after idle timeout"
    );

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn upstream_metrics_tracking() {
    let upstream = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::Echo,
        relay_addr: None,
    })
    .await
    .unwrap();

    let chain = socks5_upstream_chain(upstream.tcp_addr);
    let udp_metrics = Arc::new(UdpMetrics::new());
    let config = RelayConfig {
        routing: upstream_group_router(chain, GroupFallback::Reject),
        udp_metrics: udp_metrics.clone(),
        limits: UdpLimits::default(),
        listener: "test".to_string(),
        generation: 1,
        identity: ClientIdentity::Anonymous,
        client_tcp_peer: test_addr(),
        registry: test_registry(),
    };

    let assoc = test_assoc();
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_assoc = assoc.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(
            async move { udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await },
        );

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"metrics-check");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .unwrap()
    .unwrap();

    assert!(udp_metrics.target_flows_active.load(Ordering::Relaxed) >= 1);

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

fn upstream_config_for_timeout_test(
    tcp_addr: std::net::SocketAddr,
    credentials: Option<(String, String)>,
) -> Socks5UdpUpstreamConfig {
    let hop = ProxyHopSpec {
        protocols: vec![ProtocolSpec::Socks5],
        endpoint: EndpointSpec {
            host: tcp_addr.ip().to_string(),
            port: tcp_addr.port(),
        },
        credentials: credentials.map(|(u, p)| eggress_uri::CredentialSpec {
            username: u,
            password: p,
        }),
        rule: None,
        local_bind: None,
        tls: false,
        server_name: None,
    };
    Socks5UdpUpstreamConfig {
        upstream_id: UpstreamId::new("timeout-test"),
        hop,
        connect_timeout: std::time::Duration::from_millis(50),
        udp_bind: "127.0.0.1:0".parse().unwrap(),
    }
}

#[tokio::test]
async fn upstream_method_negotiation_timeout_is_bounded() {
    let upstream = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::MethodStall,
        relay_addr: None,
    })
    .await
    .unwrap();

    let config = upstream_config_for_timeout_test(upstream.tcp_addr, None);
    let result = open_socks5_udp_upstream(config, None).await;
    assert!(
        matches!(result, Err(UdpUpstreamError::Timeout)),
        "method stall should return Timeout"
    );
}

#[tokio::test]
async fn upstream_auth_timeout_is_bounded() {
    let upstream = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::AuthStall,
        relay_addr: None,
    })
    .await
    .unwrap();

    let config = upstream_config_for_timeout_test(
        upstream.tcp_addr,
        Some(("user".to_string(), "pass".to_string())),
    );
    let result = open_socks5_udp_upstream(config, None).await;
    assert!(
        matches!(result, Err(UdpUpstreamError::Timeout)),
        "auth stall should return Timeout"
    );
}

#[tokio::test]
async fn upstream_associate_timeout_is_bounded() {
    let upstream = Socks5UdpTestServer::start(Socks5TestServerConfig {
        mode: Socks5TestMode::AssociateStall,
        relay_addr: None,
    })
    .await
    .unwrap();

    let config = upstream_config_for_timeout_test(upstream.tcp_addr, None);
    let result = open_socks5_udp_upstream(config, None).await;
    assert!(
        matches!(result, Err(UdpUpstreamError::Timeout)),
        "associate stall should return Timeout"
    );
}
