use eggress_core::ClientIdentity;
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_udp::assoc::UdpAssociationId;
use eggress_udp::codec::decode_packet;
use eggress_udp::direct::{encode_response, UdpTargetFlow};
use eggress_udp::error::UdpError;
use eggress_udp::limits::UdpLimits;
use eggress_udp::metrics::UdpMetrics;
use eggress_udp::registry::UdpAssociationRegistry;
use eggress_udp::security::validate_target;
use eggress_udp::testkit::start_udp_echo_server;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn direct_flow_ipv4_echo() {
    let echo_addr = start_udp_echo_server().await;
    let flow = UdpTargetFlow::new(
        SocksAddr::IPv4([127, 0, 0, 1], echo_addr.port()),
        "127.0.0.1:0".parse().unwrap(),
    )
    .await
    .unwrap();

    flow.send(b"hello udp").await.unwrap();

    let mut buf = [0u8; 65535];
    let n = flow.recv(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"hello udp");
}

#[tokio::test]
async fn direct_flow_multiple_packets() {
    let echo_addr = start_udp_echo_server().await;
    let flow = UdpTargetFlow::new(
        SocksAddr::IPv4([127, 0, 0, 1], echo_addr.port()),
        "127.0.0.1:0".parse().unwrap(),
    )
    .await
    .unwrap();

    for i in 0..5 {
        let msg = format!("packet {i}");
        flow.send(msg.as_bytes()).await.unwrap();
        let mut buf = [0u8; 65535];
        let n = flow.recv(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], msg.as_bytes());
    }

    assert_eq!(flow.packets_up.load(Ordering::Relaxed), 5);
    assert_eq!(flow.packets_down.load(Ordering::Relaxed), 5);
}

#[tokio::test]
async fn direct_flow_metrics_tracking() {
    let echo_addr = start_udp_echo_server().await;
    let flow = UdpTargetFlow::new(
        SocksAddr::IPv4([127, 0, 0, 1], echo_addr.port()),
        "127.0.0.1:0".parse().unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(flow.packets_up.load(Ordering::Relaxed), 0);
    assert_eq!(flow.bytes_up.load(Ordering::Relaxed), 0);

    flow.send(b"test").await.unwrap();
    assert_eq!(flow.packets_up.load(Ordering::Relaxed), 1);
    assert_eq!(flow.bytes_up.load(Ordering::Relaxed), 4);

    let mut buf = [0u8; 65535];
    let n = flow.recv(&mut buf).await.unwrap();
    assert_eq!(flow.packets_down.load(Ordering::Relaxed), 1);
    assert_eq!(flow.bytes_down.load(Ordering::Relaxed), n as u64);
}

#[tokio::test]
async fn association_registry_global_limit() {
    let limits = UdpLimits {
        max_associations_global: 2,
        max_associations_per_listener: 10,
        ..Default::default()
    };
    let registry = UdpAssociationRegistry::new(limits);

    let _a1 = registry
        .create_association("l1", addr(1000), ClientIdentity::Anonymous, 1)
        .await
        .unwrap();
    let _a2 = registry
        .create_association("l1", addr(1001), ClientIdentity::Anonymous, 1)
        .await
        .unwrap();

    let result = registry
        .create_association("l1", addr(1002), ClientIdentity::Anonymous, 1)
        .await;
    assert!(matches!(result, Err(UdpError::AssociationLimitExceeded)));
}

#[tokio::test]
async fn association_registry_per_listener_limit() {
    let limits = UdpLimits {
        max_associations_global: 100,
        max_associations_per_listener: 1,
        ..Default::default()
    };
    let registry = UdpAssociationRegistry::new(limits);

    let _a1 = registry
        .create_association("l1", addr(1000), ClientIdentity::Anonymous, 1)
        .await
        .unwrap();

    let result = registry
        .create_association("l1", addr(1001), ClientIdentity::Anonymous, 1)
        .await;
    assert!(matches!(
        result,
        Err(UdpError::ListenerAssociationLimitExceeded)
    ));

    let _a2 = registry
        .create_association("l2", addr(1002), ClientIdentity::Anonymous, 1)
        .await
        .unwrap();
}

#[tokio::test]
async fn association_registry_slot_released_after_remove() {
    let limits = UdpLimits {
        max_associations_global: 1,
        ..Default::default()
    };
    let registry = UdpAssociationRegistry::new(limits);

    let a1 = registry
        .create_association("l1", addr(1000), ClientIdentity::Anonymous, 1)
        .await
        .unwrap();
    assert!(registry
        .create_association("l1", addr(1001), ClientIdentity::Anonymous, 1)
        .await
        .is_err());

    registry.remove(a1.id).await;
    assert!(registry
        .create_association("l1", addr(1002), ClientIdentity::Anonymous, 1)
        .await
        .is_ok());
}

#[tokio::test]
async fn association_close_all() {
    let registry = UdpAssociationRegistry::new(UdpLimits::default());
    let a1 = registry
        .create_association("l1", addr(1000), ClientIdentity::Anonymous, 1)
        .await
        .unwrap();
    let a2 = registry
        .create_association("l1", addr(1001), ClientIdentity::Anonymous, 1)
        .await
        .unwrap();

    registry.close_all().await;
    assert!(!a1.is_open());
    assert!(!a2.is_open());
}

#[test]
fn security_valid_target_passes() {
    let target = SocksAddr::IPv4([127, 0, 0, 1], 80);
    assert!(validate_target(&target).is_ok());
}

#[test]
fn security_rejects_multicast() {
    let target = SocksAddr::IPv4([224, 0, 0, 1], 80);
    assert!(matches!(
        validate_target(&target),
        Err(UdpError::MulticastTarget)
    ));
}

#[test]
fn security_rejects_broadcast() {
    let target = SocksAddr::IPv4([255, 255, 255, 255], 80);
    assert!(matches!(
        validate_target(&target),
        Err(UdpError::BroadcastTarget)
    ));
}

#[test]
fn security_rejects_port_zero() {
    let target = SocksAddr::IPv4([127, 0, 0, 1], 0);
    assert!(matches!(validate_target(&target), Err(UdpError::PortZero)));
}

#[test]
fn security_rejects_unspecified() {
    let target = SocksAddr::IPv4([0, 0, 0, 0], 80);
    assert!(matches!(
        validate_target(&target),
        Err(UdpError::UnspecifiedTarget)
    ));
}

#[test]
fn security_rejects_ipv6_multicast() {
    let target = SocksAddr::IPv6([0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], 80);
    assert!(matches!(
        validate_target(&target),
        Err(UdpError::MulticastTarget)
    ));
}

#[test]
fn security_rejects_ipv6_unspecified() {
    let target = SocksAddr::IPv6([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 80);
    assert!(matches!(
        validate_target(&target),
        Err(UdpError::UnspecifiedTarget)
    ));
}

#[test]
fn security_domain_valid() {
    let target = SocksAddr::Domain("example.com".to_string(), 443);
    assert!(validate_target(&target).is_ok());
}

#[test]
fn security_domain_port_zero_rejected() {
    let target = SocksAddr::Domain("example.com".to_string(), 0);
    assert!(matches!(validate_target(&target), Err(UdpError::PortZero)));
}

#[test]
fn codec_decode_encode_roundtrip_ipv4() {
    let limits = UdpLimits::default();
    let target = SocksAddr::IPv4([10, 0, 0, 1], 9999);
    let payload = b"roundtrip data";

    let encoded = encode_response(&target, payload);
    let req = decode_packet(&encoded, &limits).unwrap();
    assert_eq!(req.target, target);
    assert_eq!(req.payload, payload);
}

#[test]
fn codec_decode_encode_roundtrip_ipv6() {
    let limits = UdpLimits::default();
    let target = SocksAddr::IPv6(
        [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
        80,
    );
    let payload = b"ipv6 roundtrip";

    let encoded = encode_response(&target, payload);
    let req = decode_packet(&encoded, &limits).unwrap();
    assert_eq!(req.target, target);
    assert_eq!(req.payload, payload);
}

#[test]
fn codec_decode_encode_roundtrip_domain() {
    let limits = UdpLimits::default();
    let target = SocksAddr::Domain("localhost".to_string(), 5353);
    let payload = b"dns query";

    let encoded = encode_response(&target, payload);
    let req = decode_packet(&encoded, &limits).unwrap();
    assert_eq!(req.target, target);
    assert_eq!(req.payload, payload);
}

#[test]
fn codec_rejects_oversized_packet() {
    let limits = UdpLimits {
        max_datagram_size: 10,
        ..Default::default()
    };
    let target = SocksAddr::IPv4([192, 168, 1, 1], 8080);
    let encoded = encode_response(&target, b"hello world this is long");
    assert!(matches!(
        decode_packet(&encoded, &limits),
        Err(UdpError::DatagramTooLarge(_, 10))
    ));
}

#[test]
fn metrics_association_tracking() {
    let m = UdpMetrics::new();
    m.record_association_created();
    m.record_association_created();
    assert_eq!(m.associations_active.load(Ordering::Relaxed), 2);
    assert_eq!(m.associations_total.load(Ordering::Relaxed), 2);

    m.record_association_closed();
    assert_eq!(m.associations_active.load(Ordering::Relaxed), 1);
    assert_eq!(m.associations_total.load(Ordering::Relaxed), 2);
}

#[test]
fn metrics_packet_tracking() {
    let m = UdpMetrics::new();
    m.record_packet_up(100);
    m.record_packet_up(200);
    assert_eq!(m.packets_up.load(Ordering::Relaxed), 2);
    assert_eq!(m.bytes_up.load(Ordering::Relaxed), 300);

    m.record_packet_down(50);
    assert_eq!(m.packets_down.load(Ordering::Relaxed), 1);
    assert_eq!(m.bytes_down.load(Ordering::Relaxed), 50);
}

#[test]
fn metrics_dropped_packets() {
    let m = UdpMetrics::new();
    m.record_dropped();
    m.record_dropped();
    assert_eq!(m.dropped_packets.load(Ordering::Relaxed), 2);
}

#[test]
fn metrics_target_flow_tracking() {
    let m = UdpMetrics::new();
    m.record_target_flow_created();
    m.record_target_flow_created();
    assert_eq!(m.target_flows_active.load(Ordering::Relaxed), 2);
    assert_eq!(m.target_flows_total.load(Ordering::Relaxed), 2);

    m.record_target_flow_closed();
    assert_eq!(m.target_flows_active.load(Ordering::Relaxed), 1);
}

#[test]
fn metrics_decode_errors() {
    let m = UdpMetrics::new();
    m.record_decode_error();
    m.record_decode_error();
    assert_eq!(m.decode_errors.load(Ordering::Relaxed), 2);
}

#[test]
fn association_client_pinning() {
    let assoc = eggress_udp::assoc::UdpAssociation::new(
        UdpAssociationId(1),
        "test".to_string(),
        addr(1000),
        ClientIdentity::Anonymous,
        1,
    );

    let addr1: SocketAddr = "127.0.0.1:5000".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:5001".parse().unwrap();

    assert!(assoc.pin_client_addr(addr1).is_ok());
    assert_eq!(assoc.client_udp_addr(), Some(addr1));

    assert!(assoc.pin_client_addr(addr1).is_ok());
    assert!(matches!(
        assoc.pin_client_addr(addr2),
        Err(UdpError::ClientAddressMismatch)
    ));
}

#[test]
fn association_close_cancel_and_notify() {
    let assoc = eggress_udp::assoc::UdpAssociation::new(
        UdpAssociationId(1),
        "test".to_string(),
        addr(1000),
        ClientIdentity::Anonymous,
        1,
    );

    assert!(!assoc.cancel.is_cancelled());
    assert!(assoc.is_open());

    assoc.close();
    assert!(!assoc.is_open());
    assert!(assoc.cancel.is_cancelled());
}

#[test]
fn limits_default_values() {
    let limits = UdpLimits::default();
    assert_eq!(limits.max_associations_global, 1024);
    assert_eq!(limits.max_associations_per_listener, 256);
    assert_eq!(limits.max_targets_per_association, 64);
    assert_eq!(limits.max_datagram_size, 65535);
    assert_eq!(limits.idle_timeout, std::time::Duration::from_secs(60));
    assert!(limits.client_pin);
    assert_eq!(
        limits.target_idle_timeout,
        std::time::Duration::from_secs(30)
    );
}

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
        port,
    )
}
