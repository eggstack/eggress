use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::assoc::UdpAssociation;
use crate::codec::{decode_packet, encode_socks5_udp_datagram};
use crate::direct::UdpTargetFlow;
use crate::error::UdpError;
use crate::flow::{TargetFlowEntry, UdpFlowKey, UdpFlowKind};
use crate::limits::UdpLimits;
use crate::metrics::UdpMetrics;
use crate::registry::UdpAssociationRegistry;
use crate::security::validate_target;
use crate::udp_capability::{udp_capability, UdpRelayCapability};
use crate::upstream_socks5::{open_socks5_udp_upstream, Socks5UdpUpstreamConfig};
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

fn reap_idle_flows(
    flows: &mut HashMap<UdpFlowKey, TargetFlowEntry>,
    now: Instant,
    timeout: std::time::Duration,
    metrics: &UdpMetrics,
) {
    flows.retain(|_, entry| {
        let keep = now.duration_since(entry.last_activity()) < timeout;
        if !keep {
            entry.recv_task.abort();
            match &entry.flow {
                UdpFlowKind::Socks5Upstream(ref u) => {
                    u.control_cancel.cancel();
                }
                UdpFlowKind::ShadowsocksUpstream(_) => {}
                UdpFlowKind::Direct(_) => {}
            }
            metrics.record_target_flow_timeout();
        }
        keep
    });
}

async fn handle_client_datagram(
    packet: &[u8],
    client_addr: SocketAddr,
    flows: &mut HashMap<UdpFlowKey, TargetFlowEntry>,
    config: &RelayConfig,
    response_tx: &mpsc::UnboundedSender<ResponseMsg>,
    _association: &UdpAssociation,
) -> Result<(), UdpError> {
    let request = match decode_packet(packet, &config.limits) {
        Ok(r) => r,
        Err(_e) => {
            config.udp_metrics.record_decode_error();
            return Ok(());
        }
    };

    if let Err(_e) = validate_target(&request.target) {
        config.udp_metrics.record_dropped();
        return Ok(());
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
        Err(RouteError::Rejected { rule: _, .. }) => {
            config.udp_metrics.record_dropped();
            return Ok(());
        }
        Err(_) => {
            config.udp_metrics.record_dropped();
            return Ok(());
        }
    };

    match selected {
        SelectedRoute::Direct {
            selection_reason, ..
        } => {
            if selection_reason == SelectionReason::DirectFallback {
                tracing::debug!(
                    target = %request.target.host_str(),
                    "UDP using direct fallback"
                );
            }
        }
        SelectedRoute::Upstream {
            upstream,
            chain,
            pending_lease,
            ..
        } => match udp_capability(&chain) {
            UdpRelayCapability::SupportedSocks5 => {
                let key = UdpFlowKey::Socks5Upstream {
                    target: request.target.clone(),
                    upstream_id: upstream.clone(),
                };

                if flows.len() >= config.limits.max_targets_per_association
                    && !flows.contains_key(&key)
                {
                    config.udp_metrics.record_dropped();
                    drop(pending_lease);
                    return Ok(());
                }

                let entry = match flows.entry(key) {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        e.get_mut().touch();
                        e.into_mut()
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        let hop = &chain.hops[0];
                        let upstream_config = Socks5UdpUpstreamConfig {
                            upstream_id: upstream.clone(),
                            hop: hop.clone(),
                            connect_timeout: std::time::Duration::from_secs(10),
                            udp_bind: "127.0.0.1:0".parse().unwrap(),
                        };

                        match open_socks5_udp_upstream(upstream_config, None).await {
                            Ok(upstream_assoc) => {
                                let active_lease = pending_lease.established();
                                let target = request.target.clone();
                                let udp_socket = upstream_assoc.udp_socket.clone();
                                let relay_addr = upstream_assoc.relay_addr;
                                let upstream_id = upstream_assoc.upstream_id.clone();
                                let control_cancel = upstream_assoc.control_cancel.clone();
                                let control_task = upstream_assoc.control_task;

                                let flow_response_tx = response_tx.clone();
                                let flow_target = target.clone();
                                let flow_socket = udp_socket.clone();

                                let recv_task = tokio::spawn(async move {
                                    let mut recv_buf = [0u8; 65535];
                                    while let Ok(Ok((n, _peer))) = tokio::time::timeout(
                                        std::time::Duration::from_secs(30),
                                        flow_socket.recv_from(&mut recv_buf),
                                    )
                                    .await
                                    {
                                        if let Ok(upstream_resp) = eggress_protocol_socks::socks5::udp_codec::decode_socks5_udp_datagram(&recv_buf[..n]) {
                                            if socks_addr_equivalent(&upstream_resp.target, &flow_target) {
                                                let _ = flow_response_tx.send(ResponseMsg {
                                                    target: upstream_resp.target.clone(),
                                                    payload: upstream_resp.payload.to_vec(),
                                                });
                                            } else {
                                                tracing::trace!(
                                                    "upstream response target mismatch: expected {:?}, got {:?}",
                                                    flow_target,
                                                    upstream_resp.target
                                                );
                                            }
                                        }
                                    }
                                });

                                config.udp_metrics.record_target_flow_created();

                                let flow = crate::flow::Socks5UdpTargetFlow {
                                    target: request.target.clone(),
                                    upstream_id,
                                    upstream_relay_addr: relay_addr,
                                    udp_socket,
                                    control_cancel,
                                    control_task,
                                    lease: active_lease,
                                    last_activity: Instant::now(),
                                };

                                e.insert(TargetFlowEntry {
                                    flow: UdpFlowKind::Socks5Upstream(flow),
                                    recv_task,
                                })
                            }
                            Err(_e) => {
                                config.udp_metrics.record_dropped();
                                drop(pending_lease);
                                return Ok(());
                            }
                        }
                    }
                };

                match &entry.flow {
                    UdpFlowKind::Socks5Upstream(f) => {
                        if let Err(_e) = f.send(&request.target, request.payload).await {
                            config.udp_metrics.record_dropped();
                            return Ok(());
                        }
                    }
                    other => {
                        tracing::error!(
                            "unexpected flow kind for Socks5Upstream key: {:?}",
                            std::mem::discriminant(other)
                        );
                        config.udp_metrics.record_dropped();
                        return Ok(());
                    }
                }

                config
                    .udp_metrics
                    .record_packet_up(request.payload.len() as u64);
                return Ok(());
            }
            UdpRelayCapability::SupportedShadowsocks { method, password } => {
                let key = UdpFlowKey::ShadowsocksUpstream {
                    target: request.target.clone(),
                    upstream_id: upstream.clone(),
                };

                if flows.len() >= config.limits.max_targets_per_association
                    && !flows.contains_key(&key)
                {
                    config.udp_metrics.record_dropped();
                    drop(pending_lease);
                    return Ok(());
                }

                let entry = match flows.entry(key) {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        e.get_mut().touch();
                        e.into_mut()
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        let hop = &chain.hops[0];
                        let upstream_addr: SocketAddr =
                            format!("{}:{}", hop.endpoint.host, hop.endpoint.port)
                                .parse()
                                .map_err(|_| {
                                    UdpError::Other("invalid shadowsocks upstream address".into())
                                })?;

                        let udp_socket = Arc::new(
                            UdpSocket::bind("127.0.0.1:0")
                                .await
                                .map_err(|e| UdpError::Other(e.to_string()))?,
                        );

                        let flow_response_tx = response_tx.clone();
                        let flow_target = request.target.clone();
                        let flow_socket = udp_socket.clone();
                        let flow_method = method;
                        let flow_password = password.as_bytes().to_vec();

                        let recv_task = tokio::spawn(async move {
                            let mut recv_buf = [0u8; 65535];
                            while let Ok(Ok((n, _peer))) = tokio::time::timeout(
                                std::time::Duration::from_secs(30),
                                flow_socket.recv_from(&mut recv_buf),
                            )
                            .await
                            {
                                if let Ok((resp_target, resp_payload)) =
                                    eggress_protocol_shadowsocks::udp::decode_udp_packet(
                                        flow_method,
                                        &flow_password,
                                        &recv_buf[..n],
                                    )
                                {
                                    let resp_socks_addr = target_to_socks_addr(&resp_target);
                                    if socks_addr_equivalent(&resp_socks_addr, &flow_target) {
                                        let _ = flow_response_tx.send(ResponseMsg {
                                            target: resp_socks_addr,
                                            payload: resp_payload,
                                        });
                                    } else {
                                        tracing::trace!(
                                            "shadowsocks upstream response target mismatch: expected {:?}, got {:?}",
                                            flow_target,
                                            resp_target
                                        );
                                    }
                                }
                            }
                        });

                        let active_lease = pending_lease.established();

                        config.udp_metrics.record_target_flow_created();

                        let flow = crate::flow::ShadowsocksUdpTargetFlow {
                            target: request.target.clone(),
                            upstream_id: upstream.clone(),
                            upstream_addr,
                            udp_socket,
                            method,
                            password: password.into_bytes(),
                            lease: active_lease,
                            last_activity: Instant::now(),
                        };

                        e.insert(TargetFlowEntry {
                            flow: UdpFlowKind::ShadowsocksUpstream(flow),
                            recv_task,
                        })
                    }
                };

                match &entry.flow {
                    UdpFlowKind::ShadowsocksUpstream(f) => {
                        if let Err(_e) = f.send(&request.target, request.payload).await {
                            config.udp_metrics.record_dropped();
                            return Ok(());
                        }
                    }
                    other => {
                        tracing::error!(
                            "unexpected flow kind for ShadowsocksUpstream key: {:?}",
                            std::mem::discriminant(other)
                        );
                        config.udp_metrics.record_dropped();
                        return Ok(());
                    }
                }

                config
                    .udp_metrics
                    .record_packet_up(request.payload.len() as u64);
                return Ok(());
            }
            UdpRelayCapability::UnsupportedProtocol { .. } => {
                config.udp_metrics.record_dropped();
                drop(pending_lease);
                return Ok(());
            }
            UdpRelayCapability::UnsupportedMultiHop => {
                config.udp_metrics.record_dropped();
                drop(pending_lease);
                return Ok(());
            }
        },
    }

    let key = UdpFlowKey::Direct {
        target: request.target.clone(),
    };

    if flows.len() >= config.limits.max_targets_per_association && !flows.contains_key(&key) {
        config.udp_metrics.record_dropped();
        return Ok(());
    }

    let entry = match flows.entry(key) {
        std::collections::hash_map::Entry::Occupied(mut e) => {
            e.get_mut().touch();
            e.into_mut()
        }
        std::collections::hash_map::Entry::Vacant(e) => {
            let flow =
                UdpTargetFlow::new(request.target.clone(), "127.0.0.1:0".parse().unwrap()).await?;

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
                flow: UdpFlowKind::Direct(flow),
                recv_task,
            })
        }
    };

    if let UdpFlowKind::Direct(ref f) = entry.flow {
        if let Err(_e) = f.send(request.payload).await {
            config.udp_metrics.record_dropped();
            return Ok(());
        }
    }

    config
        .udp_metrics
        .record_packet_up(request.payload.len() as u64);
    Ok(())
}

pub async fn udp_relay_loop(
    relay_socket: Arc<UdpSocket>,
    association: Arc<UdpAssociation>,
    config: RelayConfig,
    cancel: CancellationToken,
) -> Result<(), UdpError> {
    let mut buf = vec![0u8; config.limits.max_datagram_size];
    let mut flows: HashMap<UdpFlowKey, TargetFlowEntry> = HashMap::new();

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

                    let packet = buf[..n].to_vec();
                    if let Err(e) = handle_client_datagram(
                        &packet,
                        client_addr,
                        &mut flows,
                        &config,
                        &response_tx,
                        &association,
                    ).await {
                        tracing::trace!(
                            association_id = ?assoc_id,
                            "datagram handling error: {e}"
                        );
                    }
                }
                Some(msg) = response_rx.recv() => {
                    if let Some(client_addr) = association.client_udp_addr() {
                        let mut out = Vec::new();
                        encode_socks5_udp_datagram(&msg.target, &msg.payload, &mut out);
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
        match &entry.flow {
            UdpFlowKind::Socks5Upstream(ref u) => {
                u.control_cancel.cancel();
            }
            UdpFlowKind::ShadowsocksUpstream(_) => {}
            UdpFlowKind::Direct(_) => {}
        }
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

fn socks_addr_equivalent(a: &SocksAddr, b: &SocksAddr) -> bool {
    match (a, b) {
        (SocksAddr::IPv4(a_addr, a_port), SocksAddr::IPv4(b_addr, b_port)) => {
            a_addr == b_addr && a_port == b_port
        }
        (SocksAddr::IPv6(a_addr, a_port), SocksAddr::IPv6(b_addr, b_port)) => {
            a_addr == b_addr && a_port == b_port
        }
        (SocksAddr::Domain(a_dom, a_port), SocksAddr::Domain(b_dom, b_port)) => {
            a_dom == b_dom && a_port == b_port
        }
        _ => false,
    }
}

fn target_to_socks_addr(target: &TargetAddr) -> SocksAddr {
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => SocksAddr::IPv4(ip.octets(), target.port),
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => SocksAddr::IPv6(ip.octets(), target.port),
        TargetHost::Domain(domain) => SocksAddr::Domain(domain.clone(), target.port),
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

    fn relay_config_with_limits(routing: Arc<dyn RouteService>, limits: UdpLimits) -> RelayConfig {
        RelayConfig {
            routing,
            udp_metrics: Arc::new(UdpMetrics::new()),
            limits,
            listener: "test".to_string(),
            generation: 1,
            identity: ClientIdentity::Anonymous,
            client_tcp_peer: test_addr(),
            registry: test_registry(),
        }
    }

    #[tokio::test]
    async fn relay_double_close_does_not_panic() {
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
        relay_handle.await.unwrap().unwrap();

        assoc.close();
        assoc.close();

        let registry = test_registry();
        registry.remove(assoc.id).await;
        registry.remove(assoc.id).await;
    }

    #[tokio::test]
    async fn relay_idle_timeout_closes_association() {
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());

        let limits = UdpLimits {
            idle_timeout: std::time::Duration::from_millis(150),
            ..UdpLimits::default()
        };
        let config = relay_config_with_limits(direct_router(), limits);
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), relay_handle).await;
        assert!(result.is_ok(), "relay should exit within timeout");
        assert!(
            !assoc.is_open(),
            "association should be closed after idle timeout"
        );
    }

    #[tokio::test]
    async fn relay_valid_packet_extends_lifetime() {
        let echo_addr = start_udp_echo().await;
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let limits = UdpLimits {
            idle_timeout: std::time::Duration::from_millis(300),
            ..UdpLimits::default()
        };
        let config = relay_config_with_limits(direct_router(), limits);
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"extend");

        client_socket.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        client_socket.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let mut recv_buf = [0u8; 65535];
        let n = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .expect("relay should still be alive");
        assert!(n.unwrap() > 0);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_wrong_client_packet_does_not_extend() {
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let limits = UdpLimits {
            idle_timeout: std::time::Duration::from_millis(200),
            client_pin: true,
            ..UdpLimits::default()
        };
        let config = relay_config_with_limits(direct_router(), limits);
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let client1 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client1.connect(relay_addr).await.unwrap();
        let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"pin");
        client1.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client2 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client2.connect(relay_addr).await.unwrap();
        let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"wrong");
        client2.send(&pkt2).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(assoc.client_udp_addr(), Some(client1.local_addr().unwrap()));

        let result = tokio::time::timeout(std::time::Duration::from_secs(1), relay_handle).await;
        assert!(result.is_ok(), "relay should exit after idle timeout");
        assert!(!assoc.is_open());
    }

    #[tokio::test]
    async fn relay_policy_rejected_packet_extends_lifetime() {
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let limits = UdpLimits {
            idle_timeout: std::time::Duration::from_millis(300),
            ..UdpLimits::default()
        };
        let config = relay_config_with_limits(reject_router(), limits);
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

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"rejected");
        client_socket.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(udp_metrics.dropped_packets.load(Ordering::Relaxed), 1);
        assert!(
            assoc.is_open(),
            "association should still be open after policy reject"
        );

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(
            assoc.is_open(),
            "association should still be open - rejected packet extended lifetime"
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_idle_timeout_records_metric() {
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());

        let limits = UdpLimits {
            idle_timeout: std::time::Duration::from_millis(150),
            ..UdpLimits::default()
        };
        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = relay_config_with_limits(direct_router(), limits);
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        relay_handle.await.unwrap().unwrap();

        assert_eq!(udp_metrics.associations_total.load(Ordering::Relaxed), 1);
        assert!(!assoc.is_open());
    }

    #[tokio::test]
    async fn relay_flow_created_on_first_packet() {
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

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"flow1");
        client_socket.send(&pkt).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();
        assert!(n > 0);

        assert_eq!(udp_metrics.target_flows_active.load(Ordering::Relaxed), 1);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_flow_reused_for_same_target() {
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

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"reuse1");
        client_socket.send(&pkt).await.unwrap();

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

        assert_eq!(udp_metrics.target_flows_active.load(Ordering::Relaxed), 1);
        assert_eq!(udp_metrics.target_flows_total.load(Ordering::Relaxed), 1);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_flow_evicted_after_target_idle_timeout() {
        let echo_addr = start_udp_echo().await;
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let limits = UdpLimits {
            target_idle_timeout: std::time::Duration::from_millis(150),
            ..UdpLimits::default()
        };
        let mut config = relay_config_with_limits(direct_router(), limits);
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

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"evict-me");
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

        let flows_active = udp_metrics.target_flows_active.load(Ordering::Relaxed);
        assert_eq!(
            flows_active, 0,
            "flow should be evicted after target idle timeout"
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_new_flow_after_eviction() {
        let echo_addr = start_udp_echo().await;
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let limits = UdpLimits {
            target_idle_timeout: std::time::Duration::from_millis(150),
            ..UdpLimits::default()
        };
        let mut config = relay_config_with_limits(direct_router(), limits);
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

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"first");
        client_socket.send(&pkt).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        assert_eq!(udp_metrics.target_flows_active.load(Ordering::Relaxed), 0);

        let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"after-evict");
        client_socket.send(&pkt2).await.unwrap();

        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();
        let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
        assert_eq!(resp.payload, b"after-evict");

        assert_eq!(udp_metrics.target_flows_active.load(Ordering::Relaxed), 1);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_target_slot_reusable_after_eviction() {
        let echo_addr = start_udp_echo().await;
        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let limits = UdpLimits {
            max_targets_per_association: 2,
            target_idle_timeout: std::time::Duration::from_millis(150),
            ..UdpLimits::default()
        };
        let mut config = relay_config_with_limits(direct_router(), limits);
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

        let echo_addr2 = start_udp_echo().await;

        let pkt1 = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"target1");
        client_socket.send(&pkt1).await.unwrap();
        let mut recv_buf = [0u8; 65535];
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();

        let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], echo_addr2.port(), b"target2");
        client_socket.send(&pkt2).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();

        assert_eq!(udp_metrics.target_flows_active.load(Ordering::Relaxed), 2);

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        assert_eq!(udp_metrics.target_flows_active.load(Ordering::Relaxed), 0);

        let pkt3 = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"target3");
        client_socket.send(&pkt3).await.unwrap();
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();
        assert!(n > 0);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[test]
    fn socks_addr_equivalent_works() {
        assert!(socks_addr_equivalent(
            &SocksAddr::IPv4([127, 0, 0, 1], 80),
            &SocksAddr::IPv4([127, 0, 0, 1], 80)
        ));
        assert!(!socks_addr_equivalent(
            &SocksAddr::IPv4([127, 0, 0, 1], 80),
            &SocksAddr::IPv4([127, 0, 0, 2], 80)
        ));
        assert!(socks_addr_equivalent(
            &SocksAddr::Domain("example.com".to_string(), 443),
            &SocksAddr::Domain("example.com".to_string(), 443)
        ));
        assert!(!socks_addr_equivalent(
            &SocksAddr::IPv4([127, 0, 0, 1], 80),
            &SocksAddr::Domain("example.com".to_string(), 80)
        ));
    }

    #[tokio::test]
    async fn relay_shadowsocks_upstream_encrypts_and_relay() {
        use crate::udp_capability::udp_capability;
        use eggress_protocol_shadowsocks::udp::{decode_udp_packet, encode_udp_packet};
        use eggress_protocol_shadowsocks::CipherMethod;
        use eggress_uri::{CredentialSpec, EndpointSpec, ProtocolSpec, ProxyHopSpec};
        use rand::RngCore;

        // Start a synthetic Shadowsocks UDP echo server
        let ss_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let ss_addr = ss_socket.local_addr().unwrap();
        let ss_method = CipherMethod::Aes256Gcm;
        let ss_password = "test-ss-password";

        let ss_password_owned = ss_password.as_bytes().to_vec();
        let ss_method_clone = ss_method;

        tokio::spawn(async move {
            let mut buf = [0u8; 65535];
            while let Ok((n, peer)) = ss_socket.recv_from(&mut buf).await {
                if let Ok((target, payload)) =
                    decode_udp_packet(ss_method_clone, &ss_password_owned, &buf[..n])
                {
                    // Echo back the payload re-encrypted with a random salt
                    let mut resp_salt = vec![0u8; ss_method_clone.salt_size()];
                    rand::thread_rng().fill_bytes(&mut resp_salt);
                    if let Ok(response) = encode_udp_packet(
                        ss_method_clone,
                        &ss_password_owned,
                        &target,
                        &payload,
                        &resp_salt,
                    ) {
                        let _ = ss_socket.send_to(&response, peer).await;
                    }
                }
            }
        });

        // Create upstream chain with Shadowsocks credentials
        let hop = ProxyHopSpec {
            protocols: vec![ProtocolSpec::Shadowsocks],
            endpoint: EndpointSpec {
                host: ss_addr.ip().to_string(),
                port: ss_addr.port(),
            },
            credentials: Some(CredentialSpec {
                username: "aes-256-gcm".to_string(),
                password: ss_password.to_string(),
            }),
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        };

        let chain = eggress_uri::ProxyChainSpec {
            hops: vec![hop.clone()],
        };

        // Verify capability detection works for Shadowsocks
        let cap = udp_capability(&chain);
        assert!(
            matches!(
                cap,
                crate::udp_capability::UdpRelayCapability::SupportedShadowsocks { .. }
            ),
            "expected SupportedShadowsocks, got {:?}",
            cap
        );

        // Create upstream with the proper chain
        let upstream = std::sync::Arc::new(UpstreamRuntime::new(UpstreamId::new("ss-up-1"), chain));
        let group_id = UpstreamGroupId(std::sync::Arc::from("ss-proxy"));
        let group = UpstreamGroup::new(
            group_id.clone(),
            eggress_routing::scheduler::SchedulerKind::FirstAvailable,
            std::sync::Arc::from(vec![upstream]),
            GroupFallback::Reject,
        );

        let rules = vec![CompiledRule {
            id: RuleId(std::sync::Arc::from("to-ss")),
            matcher: MatchExpr::Any,
            action: RouteActionSpec::UpstreamGroup(group_id.clone()),
        }];

        let router = Arc::new(Router::with_groups(
            rules,
            RouteActionSpec::Direct,
            vec![(group_id, group)],
        ));

        let assoc = test_assoc();
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let config = relay_config(router);
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_assoc = assoc.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            udp_relay_loop(relay_sock, relay_assoc, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], 9090, b"ss relay test");
        client_socket.send(&pkt).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();
        let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
        assert_eq!(resp.payload, b"ss relay test");

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }
}
