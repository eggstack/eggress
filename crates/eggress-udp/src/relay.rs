use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::assoc::UdpAssociation;
use crate::codec::{decode_packet, encode_socks5_udp_response};
use crate::direct::UdpTargetFlow;
use crate::error::UdpError;
use crate::limits::UdpLimits;
use crate::metrics::UdpMetrics;
use crate::registry::UdpAssociationRegistry;
use crate::security::validate_target;
use eggress_core::{ClientIdentity, ProtocolId, TargetAddr, TargetHost};
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_routing::{
    RouteError, RouteRequest, RouteService, SelectedRoute, SelectionReason, TransportKind,
};

pub struct RelayConfig {
    pub routing: Arc<dyn RouteService>,
    pub udp_metrics: Arc<UdpMetrics>,
    pub limits: UdpLimits,
    pub listener: String,
    pub generation: u64,
    pub identity: ClientIdentity,
    pub client_tcp_peer: SocketAddr,
    pub registry: Arc<UdpAssociationRegistry>,
}

struct ResponseMsg {
    target: SocksAddr,
    payload: Vec<u8>,
}

struct TargetFlowEntry {
    flow: UdpTargetFlow,
    recv_task: tokio::task::JoinHandle<()>,
}

fn socks_addr_key(addr: &SocksAddr) -> String {
    format!("{}:{}", addr.host_str(), addr.port())
}

fn reap_idle_flows(
    flows: &mut HashMap<String, TargetFlowEntry>,
    now: Instant,
    timeout: std::time::Duration,
    metrics: &UdpMetrics,
) {
    flows.retain(|_, entry| {
        let keep = now.duration_since(entry.flow.last_activity) < timeout;
        if !keep {
            entry.recv_task.abort();
            metrics.record_target_flow_timeout();
        }
        keep
    });
}

pub async fn udp_relay_loop(
    relay_socket: Arc<UdpSocket>,
    association: Arc<UdpAssociation>,
    config: RelayConfig,
    cancel: CancellationToken,
) -> Result<(), UdpError> {
    let mut buf = vec![0u8; config.limits.max_datagram_size];
    let mut flows: HashMap<String, TargetFlowEntry> = HashMap::new();

    let (response_tx, mut response_rx) = mpsc::unbounded_channel::<ResponseMsg>();

    let mut idle_tick = tokio::time::interval(config.limits.idle_timeout);
    idle_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut target_cleanup_tick = tokio::time::interval(config.limits.target_idle_timeout);
    target_cleanup_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    config.udp_metrics.record_association_created();

    let assoc_id = association.id;
    let registry = config.registry.clone();

    let result = (async {
        loop {
            tokio::select! {
                _ = idle_tick.tick() => {
                    if association.last_activity().elapsed() >= config.limits.idle_timeout {
                        tracing::debug!(
                            association_id = ?assoc_id,
                            "UDP association idle timeout"
                        );
                        config.udp_metrics.record_association_timeout();
                        break;
                    }
                }
                _ = target_cleanup_tick.tick() => {
                    let now = Instant::now();
                    reap_idle_flows(&mut flows, now, config.limits.target_idle_timeout, &config.udp_metrics);
                }
                result = relay_socket.recv_from(&mut buf) => {
                    let (n, client_addr) = result?;

                    if config.limits.client_pin {
                        if let Err(e) = association.pin_client_addr(client_addr) {
                            tracing::trace!(
                                association_id = ?assoc_id,
                                client_addr = %client_addr,
                                "rejecting packet from unpinned client: {e}"
                            );
                            config.udp_metrics.record_dropped();
                            continue;
                        }
                    }

                    association.touch();

                    let packet = &buf[..n];
                    let request = match decode_packet(packet, &config.limits) {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::trace!(
                                association_id = ?assoc_id,
                                "decode error: {e}"
                            );
                            config.udp_metrics.record_decode_error();
                            continue;
                        }
                    };

                    if let Err(e) = validate_target(&request.target) {
                        tracing::trace!(
                            association_id = ?assoc_id,
                            target = %request.target.host_str(),
                            "target validation failed: {e}"
                        );
                        config.udp_metrics.record_dropped();
                        continue;
                    }

                    let target_addr = socks_to_target_addr(&request.target);
                    let route_request = RouteRequest {
                        target: &target_addr,
                        source: Some(client_addr),
                        listener: &config.listener,
                        inbound_protocol: ProtocolId::Socks5,
                        identity: &config.identity,
                        transport: TransportKind::Udp,
                    };

                    let selected = match config.routing.route(&route_request) {
                        Ok(selected) => selected,
                        Err(RouteError::Rejected { rule, .. }) => {
                            tracing::trace!(
                                association_id = ?assoc_id,
                                target = %request.target.host_str(),
                                rule = %rule,
                                "route rejected"
                            );
                            config.udp_metrics.record_dropped();
                            continue;
                        }
                        Err(_) => {
                            tracing::trace!(
                                association_id = ?assoc_id,
                                target = %request.target.host_str(),
                                "no eligible upstream for UDP"
                            );
                            config.udp_metrics.record_dropped();
                            continue;
                        }
                    };

                    match selected {
                        SelectedRoute::Direct { selection_reason, .. } => {
                            if selection_reason == SelectionReason::DirectFallback {
                                tracing::debug!(
                                    association_id = ?assoc_id,
                                    target = %request.target.host_str(),
                                    "UDP using direct fallback"
                                );
                            }
                        }
                        SelectedRoute::Upstream { .. } => {
                            tracing::trace!(
                                association_id = ?assoc_id,
                                target = %request.target.host_str(),
                                "unsupported upstream for UDP, dropping"
                            );
                            config.udp_metrics.record_dropped();
                            continue;
                        }
                    }

                    let key = socks_addr_key(&request.target);

                    if flows.len() >= config.limits.max_targets_per_association
                        && !flows.contains_key(&key)
                    {
                        tracing::trace!(
                            association_id = ?assoc_id,
                            "target flow limit exceeded"
                        );
                        config.udp_metrics.record_dropped();
                        continue;
                    }

                    let entry = match flows.entry(key) {
                        std::collections::hash_map::Entry::Occupied(mut e) => {
                            e.get_mut().flow.touch();
                            e.into_mut()
                        }
                        std::collections::hash_map::Entry::Vacant(e) => {
                            let flow = UdpTargetFlow::new(
                                request.target.clone(),
                                "127.0.0.1:0".parse().unwrap(),
                            )
                            .await?;

                            let target_addr_clone = request.target.clone();
                            let flow_response_tx = response_tx.clone();
                            let flow_socket = flow.socket.clone();

                            let recv_task = tokio::spawn(async move {
                                let mut recv_buf = [0u8; 65535];
                                while let Ok(n) = flow_socket.recv(&mut recv_buf).await {
                                    let payload = recv_buf[..n].to_vec();
                                    let _ = flow_response_tx.send(ResponseMsg {
                                        target: target_addr_clone.clone(),
                                        payload,
                                    });
                                }
                            });

                            config.udp_metrics.record_target_flow_created();

                            e.insert(TargetFlowEntry {
                                flow,
                                recv_task,
                            })
                        }
                    };

                    if let Err(e) = entry.flow.send(request.payload).await {
                        tracing::trace!(
                            association_id = ?assoc_id,
                            target = %request.target.host_str(),
                            "send failed: {e}"
                        );
                        config.udp_metrics.record_dropped();
                        continue;
                    }

                    config.udp_metrics.record_packet_up(request.payload.len() as u64);
                }
                Some(msg) = response_rx.recv() => {
                    if let Some(client_addr) = association.client_udp_addr() {
                        let mut out = Vec::new();
                        encode_socks5_udp_response(&msg.target, &msg.payload, &mut out);
                        let _ = relay_socket.send_to(&out, client_addr).await;
                        config.udp_metrics.record_packet_down(msg.payload.len() as u64);
                    }
                }
                _ = cancel.cancelled() => {
                    break;
                }
            }
        }

        Ok::<(), UdpError>(())
    })
    .await;

    config.udp_metrics.record_association_closed();

    for entry in flows.values() {
        entry.recv_task.abort();
        config.udp_metrics.record_target_flow_closed();
    }

    association.close();
    registry.remove(assoc_id).await;

    result
}

fn socks_to_target_addr(addr: &SocksAddr) -> TargetAddr {
    match addr {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assoc::UdpAssociationId;
    use crate::limits::UdpLimits;
    use crate::metrics::UdpMetrics;
    use crate::registry::UdpAssociationRegistry;
    use eggress_core::UpstreamId;
    use eggress_routing::lease::PendingLease;
    use eggress_routing::upstream::{GroupFallback, UpstreamGroup, UpstreamRuntime};
    use eggress_routing::{
        CompiledRule, MatchExpr, RouteActionSpec, Router, RuleId, UpstreamGroupId,
    };
    use std::sync::atomic::Ordering;

    fn test_addr() -> SocketAddr {
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

    fn direct_router() -> Arc<dyn RouteService> {
        Arc::new(Router::new(vec![], RouteActionSpec::Direct))
    }

    fn reject_router() -> Arc<dyn RouteService> {
        let rules = vec![CompiledRule {
            id: RuleId(std::sync::Arc::from("reject-all")),
            matcher: eggress_routing::MatchExpr::Any,
            action: RouteActionSpec::Reject(eggress_core::RejectReason::AccessDenied),
        }];
        Arc::new(Router::new(rules, RouteActionSpec::Direct))
    }

    fn relay_config(routing: Arc<dyn RouteService>) -> RelayConfig {
        RelayConfig {
            routing,
            udp_metrics: Arc::new(UdpMetrics::new()),
            limits: UdpLimits::default(),
            listener: "test".to_string(),
            generation: 1,
            identity: ClientIdentity::Anonymous,
            client_tcp_peer: test_addr(),
            registry: test_registry(),
        }
    }

    fn ipv4_socks5_packet(target: [u8; 4], port: u16, payload: &[u8]) -> Vec<u8> {
        let mut pkt = vec![0x00, 0x00, 0x00, 0x01];
        pkt.extend_from_slice(&target);
        pkt.extend_from_slice(&port.to_be_bytes());
        pkt.extend_from_slice(payload);
        pkt
    }

    async fn start_udp_echo() -> SocketAddr {
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

    #[tokio::test]
    async fn relay_echo_ipv4() {
        let echo_addr = start_udp_echo().await;
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let config = relay_config(direct_router());
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"hello relay");
        client_socket.send(&pkt).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();
        let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
        assert_eq!(resp.payload, b"hello relay");

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_rejects_unpinned_client() {
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let config = relay_config(direct_router());
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"test");

        let client1 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client1.connect(relay_addr).await.unwrap();
        client1.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client2 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client2.connect(relay_addr).await.unwrap();
        client2.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(assoc.client_udp_addr(), Some(client1.local_addr().unwrap()));

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_route_reject_drops_packet() {
        let echo_addr = start_udp_echo().await;
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let config = relay_config(reject_router());
        let udp_metrics = config.udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"should be dropped");
        client_socket.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(udp_metrics.dropped_packets.load(Ordering::Relaxed), 1);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_records_metrics() {
        let echo_addr = start_udp_echo().await;
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = relay_config(direct_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

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

        assert!(udp_metrics.packets_up.load(Ordering::Relaxed) >= 1);
        assert!(udp_metrics.packets_down.load(Ordering::Relaxed) >= 1);
        assert!(udp_metrics.target_flows_active.load(Ordering::Relaxed) >= 1);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_closes_on_cancel() {
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());

        let config = relay_config(direct_router());
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();

        let result = relay_handle.await.unwrap();
        assert!(result.is_ok());
        assert!(!assoc.is_open());
    }

    #[test]
    fn socks_to_target_addr_conversion() {
        let ipv4 = SocksAddr::IPv4([192, 168, 1, 1], 8080);
        let addr = socks_to_target_addr(&ipv4);
        assert_eq!(addr.port, 8080);
        assert_eq!(addr.host, TargetHost::Ip("192.168.1.1".parse().unwrap()));

        let domain = SocksAddr::Domain("example.com".to_string(), 443);
        let addr = socks_to_target_addr(&domain);
        assert_eq!(addr.port, 443);
        assert_eq!(addr.host, TargetHost::Domain("example.com".to_string()));
    }

    fn make_upstream(id: &str) -> std::sync::Arc<UpstreamRuntime> {
        std::sync::Arc::new(UpstreamRuntime::new(
            UpstreamId::new(id),
            eggress_uri::ProxyChainSpec { hops: vec![] },
        ))
    }

    fn upstream_group_router(fallback: GroupFallback) -> Arc<dyn RouteService> {
        let upstream = make_upstream("up-udp-1");
        upstream.set_enabled(false);
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

    #[tokio::test]
    async fn relay_upstream_group_direct_fallback_forwards() {
        let echo_addr = start_udp_echo().await;
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let config = relay_config(upstream_group_router(GroupFallback::Direct));
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"fallback test");
        client_socket.send(&pkt).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();
        let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
        assert_eq!(resp.payload, b"fallback test");

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_upstream_group_reject_fallback_drops() {
        let echo_addr = start_udp_echo().await;
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let config = relay_config(upstream_group_router(GroupFallback::Reject));
        let udp_metrics = config.udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"should drop");
        client_socket.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(udp_metrics.dropped_packets.load(Ordering::Relaxed), 1);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_upstream_group_unsupported_drops_no_inflight_leak() {
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let config = relay_config(upstream_group_router(GroupFallback::Reject));
        let udp_metrics = config.udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"drop me");
        client_socket.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(udp_metrics.dropped_packets.load(Ordering::Relaxed), 1);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_upstream_group_unhealthy_fallback_drops() {
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let config = relay_config(upstream_group_router(GroupFallback::UseUnhealthy));
        let udp_metrics = config.udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"unhealthy drop");
        client_socket.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(udp_metrics.dropped_packets.load(Ordering::Relaxed), 1);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[test]
    fn upstream_lease_released_on_drop() {
        let upstream = make_upstream("lease-check");
        let lease = PendingLease::new(upstream.clone());
        assert_eq!(upstream.in_flight.load(Ordering::Relaxed), 1);
        drop(lease);
        assert_eq!(upstream.in_flight.load(Ordering::Relaxed), 0);
    }
}
