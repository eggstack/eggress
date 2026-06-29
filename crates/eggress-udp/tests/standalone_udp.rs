use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

use eggress_core::RejectReason;
use eggress_routing::{CompiledRule, MatchExpr, RouteActionSpec, Router};
use eggress_udp::codec::decode_packet;
use eggress_udp::limits::UdpLimits;
use eggress_udp::metrics::UdpMetrics;
use eggress_udp::standalone::{standalone_udp_relay, StandaloneUdpConfig};

fn ipv4_socks5_packet(target: [u8; 4], port: u16, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00, 0x01];
    pkt.extend_from_slice(&target);
    pkt.extend_from_slice(&port.to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

fn domain_socks5_packet(domain: &str, port: u16, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00, 0x03];
    pkt.push(domain.len() as u8);
    pkt.extend_from_slice(domain.as_bytes());
    pkt.extend_from_slice(&port.to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

async fn start_udp_echo() -> std::net::SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 65535];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let _ = socket.send_to(&buf[..n], peer).await;
        }
    });
    addr
}

fn direct_router() -> Arc<dyn eggress_routing::RouteService> {
    Arc::new(Router::new(vec![], RouteActionSpec::Direct))
}

fn reject_router() -> Arc<dyn eggress_routing::RouteService> {
    let rules = vec![CompiledRule {
        id: eggress_routing::RuleId(std::sync::Arc::from("reject-all")),
        matcher: MatchExpr::Any,
        action: RouteActionSpec::Reject(RejectReason::AccessDenied),
    }];
    Arc::new(Router::new(rules, RouteActionSpec::Direct))
}

fn standalone_config(routing: Arc<dyn eggress_routing::RouteService>) -> StandaloneUdpConfig {
    StandaloneUdpConfig {
        routing,
        udp_metrics: Arc::new(UdpMetrics::new()),
        limits: UdpLimits::default(),
        listener: "test-standalone".to_string(),
        generation: 1,
    }
}

fn standalone_config_with_limits(
    routing: Arc<dyn eggress_routing::RouteService>,
    limits: UdpLimits,
) -> StandaloneUdpConfig {
    StandaloneUdpConfig {
        routing,
        udp_metrics: Arc::new(UdpMetrics::new()),
        limits,
        listener: "test-standalone".to_string(),
        generation: 1,
    }
}

fn standalone_config_with_metrics(
    routing: Arc<dyn eggress_routing::RouteService>,
    udp_metrics: Arc<UdpMetrics>,
) -> StandaloneUdpConfig {
    StandaloneUdpConfig {
        routing,
        udp_metrics,
        limits: UdpLimits::default(),
        listener: "test-standalone".to_string(),
        generation: 1,
    }
}

#[tokio::test]
async fn standalone_udp_direct_echo() {
    let echo_addr = start_udp_echo().await;
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let config = standalone_config(direct_router());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"hello direct echo");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("recv should not timeout")
    .expect("recv should succeed");

    let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
    assert_eq!(resp.payload, b"hello direct echo");

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_domain_target_echo() {
    let echo_addr = start_udp_echo().await;
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let config = standalone_config(direct_router());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = domain_socks5_packet("127.0.0.1", echo_addr.port(), b"domain target");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("recv should not timeout")
    .expect("recv should succeed");

    let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
    assert_eq!(resp.payload, b"domain target");

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_malformed_short_datagram() {
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let udp_metrics = Arc::new(UdpMetrics::new());
    let mut config = standalone_config(direct_router());
    config.udp_metrics = udp_metrics.clone();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    client_socket.send(&[0x00, 0x00]).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        udp_metrics
            .standalone_malformed_datagrams
            .load(Ordering::Relaxed),
        1,
        "should record decode error for short datagram"
    );

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_two_clients() {
    let echo_addr = start_udp_echo().await;
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let config = standalone_config(direct_router());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client1 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client1.connect(relay_addr).await.unwrap();

    let client2 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client2.connect(relay_addr).await.unwrap();

    let pkt1 = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"client1 msg");
    client1.send(&pkt1).await.unwrap();

    let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"client2 msg");
    client2.send(&pkt2).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n1 = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client1.recv(&mut recv_buf).await
    })
    .await
    .expect("client1 recv timeout")
    .expect("client1 recv failed");
    let resp1 = decode_packet(&recv_buf[..n1], &UdpLimits::default()).unwrap();
    assert_eq!(resp1.payload, b"client1 msg");

    let n2 = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client2.recv(&mut recv_buf).await
    })
    .await
    .expect("client2 recv timeout")
    .expect("client2 recv failed");
    let resp2 = decode_packet(&recv_buf[..n2], &UdpLimits::default()).unwrap();
    assert_eq!(resp2.payload, b"client2 msg");

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_two_targets_from_same_client() {
    let echo_addr1 = start_udp_echo().await;
    let echo_addr2 = start_udp_echo().await;
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let config = standalone_config(direct_router());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt1 = ipv4_socks5_packet([127, 0, 0, 1], echo_addr1.port(), b"to target1");
    client_socket.send(&pkt1).await.unwrap();

    let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], echo_addr2.port(), b"to target2");
    client_socket.send(&pkt2).await.unwrap();

    let mut recv_buf = [0u8; 65535];

    let mut received = vec![];
    for _ in 0..2 {
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .expect("recv timeout")
        .expect("recv failed");
        let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
        received.push(resp.payload.to_vec());
    }

    assert!(received.contains(&b"to target1".to_vec()));
    assert!(received.contains(&b"to target2".to_vec()));

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_idle_timeout_cleanup() {
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let limits = UdpLimits {
        idle_timeout: std::time::Duration::from_millis(100),
        target_idle_timeout: std::time::Duration::from_millis(50),
        ..UdpLimits::default()
    };
    let udp_metrics = Arc::new(UdpMetrics::new());
    let mut config = standalone_config_with_limits(direct_router(), limits);
    config.udp_metrics = udp_metrics.clone();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"idle test");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert_eq!(
        udp_metrics.standalone_flows_active.load(Ordering::Relaxed),
        1,
        "should have one active flow after initial packet"
    );

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    assert_eq!(
        udp_metrics.standalone_flows_active.load(Ordering::Relaxed),
        0,
        "flow should be cleaned up after idle timeout"
    );

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_flow_cap_enforcement() {
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let limits = UdpLimits {
        max_targets_per_association: 1,
        ..UdpLimits::default()
    };
    let udp_metrics = Arc::new(UdpMetrics::new());
    let mut config = standalone_config_with_limits(direct_router(), limits);
    config.udp_metrics = udp_metrics.clone();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt1 = ipv4_socks5_packet([127, 0, 0, 1], 8081, b"first");
    client_socket.send(&pkt1).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], 8082, b"second");
    client_socket.send(&pkt2).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        udp_metrics
            .standalone_rejected_datagrams
            .load(Ordering::Relaxed),
        1,
        "second target should be dropped due to per-client limit"
    );

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_route_reject_drops() {
    let echo_addr = start_udp_echo().await;
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let udp_metrics = Arc::new(UdpMetrics::new());
    let config = standalone_config_with_metrics(reject_router(), udp_metrics.clone());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"should be rejected");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        udp_metrics
            .standalone_rejected_datagrams
            .load(Ordering::Relaxed),
        1,
        "rejected route should increment rejected counter"
    );

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_closes_on_cancel() {
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let config = standalone_config(direct_router());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();

    let result = relay_handle.await.unwrap();
    assert!(result.is_ok());
}

#[tokio::test]
async fn standalone_udp_records_metrics() {
    let echo_addr = start_udp_echo().await;
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let udp_metrics = Arc::new(UdpMetrics::new());
    let config = standalone_config_with_metrics(direct_router(), udp_metrics.clone());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"metrics test");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .unwrap()
    .unwrap();
    let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
    assert_eq!(resp.payload, b"metrics test");

    assert!(udp_metrics.standalone_packets_in.load(Ordering::Relaxed) >= 1);
    assert!(udp_metrics.standalone_packets_out.load(Ordering::Relaxed) >= 1);
    assert!(udp_metrics.standalone_flows_active.load(Ordering::Relaxed) >= 1);
    assert!(udp_metrics.standalone_flows_total.load(Ordering::Relaxed) >= 1);

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_flow_reused_for_same_target() {
    let echo_addr = start_udp_echo().await;
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let udp_metrics = Arc::new(UdpMetrics::new());
    let config = standalone_config_with_metrics(direct_router(), udp_metrics.clone());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt1 = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"reuse1");
    client_socket.send(&pkt1).await.unwrap();
    let mut recv_buf = [0u8; 65535];
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .unwrap()
    .unwrap();

    let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"reuse2");
    client_socket.send(&pkt2).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(
        udp_metrics.standalone_flows_active.load(Ordering::Relaxed),
        1,
        "should reuse flow for same target"
    );
    assert_eq!(
        udp_metrics.standalone_flows_total.load(Ordering::Relaxed),
        1,
        "only one flow total should be created"
    );

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_target_flow_timeout() {
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let limits = UdpLimits {
        target_idle_timeout: std::time::Duration::from_millis(100),
        idle_timeout: std::time::Duration::from_secs(10),
        ..UdpLimits::default()
    };
    let udp_metrics = Arc::new(UdpMetrics::new());
    let mut config = standalone_config_with_limits(direct_router(), limits);
    config.udp_metrics = udp_metrics.clone();
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"timeout test");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        udp_metrics.standalone_flows_active.load(Ordering::Relaxed),
        1
    );

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    assert_eq!(
        udp_metrics.standalone_flows_active.load(Ordering::Relaxed),
        0,
        "target flow should be cleaned up after idle timeout"
    );

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_multicast_target_dropped() {
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let udp_metrics = Arc::new(UdpMetrics::new());
    let config = standalone_config_with_metrics(direct_router(), udp_metrics.clone());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([224, 0, 0, 1], 80, b"multicast");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        udp_metrics
            .standalone_rejected_datagrams
            .load(Ordering::Relaxed),
        1,
        "multicast target should be dropped"
    );

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_broadcast_target_dropped() {
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let udp_metrics = Arc::new(UdpMetrics::new());
    let config = standalone_config_with_metrics(direct_router(), udp_metrics.clone());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([255, 255, 255, 255], 80, b"broadcast");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        udp_metrics
            .standalone_rejected_datagrams
            .load(Ordering::Relaxed),
        1,
        "broadcast target should be dropped"
    );

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn standalone_udp_port_zero_dropped() {
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let udp_metrics = Arc::new(UdpMetrics::new());
    let config = standalone_config_with_metrics(direct_router(), udp_metrics.clone());
    let cancel = CancellationToken::new();

    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let relay_handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([192, 168, 1, 1], 0, b"port zero");
    client_socket.send(&pkt).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        udp_metrics
            .standalone_rejected_datagrams
            .load(Ordering::Relaxed),
        1,
        "port zero target should be dropped"
    );

    cancel.cancel();
    relay_handle.await.unwrap().unwrap();
}
