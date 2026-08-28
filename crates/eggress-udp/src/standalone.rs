use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

use crate::codec::{decode_packet, encode_socks5_udp_datagram};
use crate::composed::open_composed_udp_upstream;
use crate::direct::UdpTargetFlow;
use crate::error::UdpError;
#[cfg(feature = "shadowsocks")]
use crate::flow::resolve_endpoint;
use crate::flow::target_to_socks_addr;
use crate::flow::{
    can_use_flow, close_all_flows, local_udp_bind_addr, reap_idle_flows, socks_addr_equivalent,
    socks_to_target_addr, total_target_flows, ClientFlowState, TargetFlowEntry, UdpFlowKey,
    UdpFlowKind,
};
use crate::limits::UdpLimits;
use crate::metrics::UdpMetrics;
use crate::security::validate_standalone_target;
use crate::udp_capability::{udp_capability, UdpRelayCapability};
use crate::upstream_socks5::{open_socks5_udp_upstream, Socks5UdpUpstreamConfig};
use eggress_core::{ClientIdentity, ProtocolId};
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_routing::{
    RouteError, RouteRequest, RouteService, SelectedRoute, SelectionReason, TransportKind,
};

pub struct StandaloneUdpConfig {
    pub routing: Arc<dyn RouteService>,
    pub udp_metrics: Arc<UdpMetrics>,
    pub limits: UdpLimits,
    pub listener: String,
    pub generation: u64,
    pub allow_private_egress: bool,
}

struct ResponseMsg {
    client: SocketAddr,
    target: SocksAddr,
    payload: Vec<u8>,
}

const RESPONSE_CHANNEL_CAPACITY: usize = 256;

pub async fn standalone_udp_relay(
    socket: Arc<UdpSocket>,
    config: StandaloneUdpConfig,
    cancel: CancellationToken,
) -> Result<(), UdpError> {
    let mut buf = vec![0u8; config.limits.max_datagram_size];
    let target_idle_timeout = config.limits.target_idle_timeout;
    let mut clients: HashMap<SocketAddr, ClientFlowState> = HashMap::new();
    let (response_tx, mut response_rx) =
        tokio::sync::mpsc::channel::<ResponseMsg>(RESPONSE_CHANNEL_CAPACITY);

    let socket_clone = socket.clone();
    let metrics_clone = config.udp_metrics.clone();
    let response_cancel = cancel.clone();
    tokio::spawn(async move {
        let mut out = Vec::new();
        loop {
            let msg = tokio::select! {
                _ = response_cancel.cancelled() => break,
                msg = response_rx.recv() => match msg {
                    Some(msg) => msg,
                    None => break,
                },
            };
            out.clear();
            match encode_socks5_udp_datagram(&msg.target, &msg.payload, &mut out) {
                Ok(()) => match socket_clone.send_to(&out, msg.client).await {
                    Ok(_) => metrics_clone.record_standalone_packet_out(out.len() as u64),
                    Err(error) => {
                        tracing::debug!(%error, client = %msg.client, "failed to send standalone UDP response");
                        metrics_clone.record_dropped_send_error();
                    }
                },
                Err(error) => {
                    tracing::debug!(%error, "failed to encode standalone UDP response");
                    metrics_clone.record_dropped_encode_error();
                }
            }
        }
    });

    // One reaper tick suffices: reap_idle_flows enforces both the per-target
    // and per-client idle timeouts, so it must run at the shorter period.
    let reap_period = config
        .limits
        .idle_timeout
        .min(config.limits.target_idle_timeout);
    let mut idle_tick = tokio::time::interval(reap_period);
    idle_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            result = socket.recv_from(&mut buf) => {
                let (n, client_addr) = result?;

                let packet = &buf[..n];

                let request = match decode_packet(packet, &config.limits) {
                    Ok(r) => r,
                    Err(_) => {
                        config.udp_metrics.record_standalone_malformed();
                        continue;
                    }
                };

                if validate_standalone_target(&request.target, config.allow_private_egress).is_err() {
                    config.udp_metrics.record_standalone_rejected();
                    continue;
                }

                config.udp_metrics.record_standalone_packet_in(n as u64);

                let target_addr = socks_to_target_addr(&request.target);
                let route_request = RouteRequest {
                    target: &target_addr,
                    source: Some(client_addr),
                    listener: &config.listener,
                    inbound_protocol: ProtocolId::Socks5,
                    identity: &ClientIdentity::Anonymous,
                    transport: TransportKind::Udp,
                };

                let selected = match config.routing.route(&route_request) {
                    Ok(s) => s,
                    Err(RouteError::Rejected { .. }) | Err(_) => {
                        config.udp_metrics.record_standalone_rejected();
                        continue;
                    }
                };

                // Bound client bookkeeping as well as target flows. A client
                // that sends only rejected/unsupported packets must not be
                // able to grow this map without limit.
                if !clients.contains_key(&client_addr)
                    && clients.len() >= crate::flow::max_standalone_flows(&config.limits)
                {
                    config.udp_metrics.record_standalone_rejected();
                    continue;
                }

                let total_flows = total_target_flows(&clients);
                let state = clients.entry(client_addr).or_default();
                state.touch();

                match selected {
                    SelectedRoute::Direct {
                        selection_reason, ..
                    } => {
                        if selection_reason == SelectionReason::DirectFallback {
                            tracing::debug!(
                                target = %request.target.host_str(),
                                "UDP standalone using direct fallback"
                            );
                        }

                        let key = UdpFlowKey::Direct {
                            target: request.target.clone(),
                        };
                        if !can_use_flow(state, &key, total_flows, &config.limits) {
                            config.udp_metrics.record_standalone_rejected();
                            continue;
                        }

                        let entry = match state.target_flows.entry(key) {
                            std::collections::hash_map::Entry::Occupied(mut e) => {
                                e.get_mut().touch();
                                e.into_mut()
                            }
                            std::collections::hash_map::Entry::Vacant(e) => {
                                match build_direct_flow(
                                    &config,
                                    client_addr,
                                    &response_tx,
                                    e.key().target().clone(),
                                    &cancel,
                                ).await {
                                    Ok(entry) => e.insert(entry),
                                    Err(e) => {
                                        tracing::trace!(
                                            client = %client_addr,
                                            "failed to create direct target flow: {e}"
                                        );
                                        config.udp_metrics.record_standalone_rejected();
                                        continue;
                                    }
                                }
                            }
                        };

                        if let UdpFlowKind::Direct(ref f) = entry.flow {
                            if f.send(request.payload).await.is_err() {
                                config.udp_metrics.record_standalone_rejected();
                            }
                        } else {
                            tracing::error!(
                                "unexpected flow kind for Direct key"
                            );
                            config.udp_metrics.record_standalone_rejected();
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
                            if !can_use_flow(state, &key, total_flows, &config.limits) {
                                config.udp_metrics.record_standalone_rejected();
                                drop(pending_lease);
                                continue;
                            }

                            let entry = match state.target_flows.entry(key) {
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
                                        udp_bind: local_udp_bind_addr(),
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
                                            let flow_metrics = config.udp_metrics.clone();
                                            let flow_target = target.clone();
                                            let flow_socket = udp_socket.clone();
                                            let flow_client = client_addr;
                                            let flow_cancel = cancel.clone();

                                            let recv_task = tokio::spawn(async move {
                                                let mut recv_buf = [0u8; 65535];
                                                loop {
                                                    let result = tokio::select! {
                                                        _ = flow_cancel.cancelled() => break,
                                                        result = tokio::time::timeout(
                                                            target_idle_timeout,
                                                            flow_socket.recv_from(&mut recv_buf),
                                                        ) => result,
                                                    };
                                                    let Ok(Ok((n, peer))) = result else { break };
                                                    if peer != relay_addr {
                                                        tracing::trace!(%peer, %relay_addr, "discarding UDP response from unexpected peer");
                                                        continue;
                                                    }
                                                    if let Ok(upstream_resp) = eggress_protocol_socks::socks5::udp_codec::decode_socks5_udp_datagram(&recv_buf[..n]) {
                                                        if socks_addr_equivalent(&upstream_resp.target, &flow_target) {
                                                            if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) = flow_response_tx.try_send(ResponseMsg {
                                                                client: flow_client,
                                                                target: upstream_resp.target.clone(),
                                                                payload: upstream_resp.payload.to_vec(),
                                                            }) {
                                                                flow_metrics.record_dropped_response_channel_full();
                                                                tracing::debug!("UDP response channel full; response datagram dropped");
                                                            }
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

                                            config.udp_metrics.record_standalone_flow_created();

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
                                            config.udp_metrics.record_standalone_rejected();
                                            drop(pending_lease);
                                            continue;
                                        }
                                    }
                                }
                            };

                            match &entry.flow {
                                UdpFlowKind::Socks5Upstream(f) => {
                                    if let Err(_e) = f.send(&request.target, request.payload).await {
                                        config.udp_metrics.record_standalone_rejected();
                                        continue;
                                    }
                                }
                                other => {
                                    tracing::error!(
                                        "unexpected flow kind for Socks5Upstream key: {:?}",
                                        std::mem::discriminant(other)
                                    );
                                    config.udp_metrics.record_standalone_rejected();
                                    continue;
                                }
                            }
                        }
                        #[cfg(feature = "shadowsocks")]
                        UdpRelayCapability::SupportedShadowsocks { method, password } => {
                            let key = UdpFlowKey::ShadowsocksUpstream {
                                target: request.target.clone(),
                                upstream_id: upstream.clone(),
                            };
                            if !can_use_flow(state, &key, total_flows, &config.limits) {
                                config.udp_metrics.record_standalone_rejected();
                                drop(pending_lease);
                                continue;
                            }

                            let entry = match state.target_flows.entry(key) {
                                std::collections::hash_map::Entry::Occupied(mut e) => {
                                    e.get_mut().touch();
                                    e.into_mut()
                                }
                                std::collections::hash_map::Entry::Vacant(e) => {
                                    let hop = &chain.hops[0];
                                    let upstream_addr = match resolve_endpoint(&hop.endpoint).await
                                    {
                                        Ok(addr) => addr,
                                        Err(_) => {
                                            config.udp_metrics.record_standalone_rejected();
                                            drop(pending_lease);
                                            continue;
                                        }
                                    };

                                    let udp_socket = Arc::new(
                                        match UdpSocket::bind(local_udp_bind_addr()).await {
                                            Ok(socket) => socket,
                                            Err(_) => {
                                                config.udp_metrics.record_standalone_rejected();
                                                drop(pending_lease);
                                                continue;
                                            }
                                        },
                                    );

                                    let flow_response_tx = response_tx.clone();
                                    let flow_metrics = config.udp_metrics.clone();
                                    let flow_target = request.target.clone();
                                    let flow_socket = udp_socket.clone();
                                        let flow_method = method;
                                        let flow_password: Arc<[u8]> = Arc::from(password.as_bytes());
                                        let flow_password_ikm: Arc<[u8]> = Arc::from(
                                            eggress_protocol_shadowsocks::CipherMethod::password_key_material(
                                                &flow_password,
                                            ),
                                        );
                                        let flow_password_ikm_for_recv = flow_password_ikm.clone();
                                    let flow_client = client_addr;
                                    let flow_cancel = cancel.clone();

                                    let recv_task = tokio::spawn(async move {
                                        let mut recv_buf = [0u8; 65535];
                                        loop {
                                            let result = tokio::select! {
                                                _ = flow_cancel.cancelled() => break,
                                                result = tokio::time::timeout(
                                                    target_idle_timeout,
                                                    flow_socket.recv_from(&mut recv_buf),
                                                ) => result,
                                            };
                                            let Ok(Ok((n, _peer))) = result else { break };
                                            if let Ok((resp_target, resp_payload)) =
                                                eggress_protocol_shadowsocks::udp::decode_udp_packet_with_ikm(
                                                    flow_method,
                                                    &flow_password_ikm_for_recv,
                                                    &recv_buf[..n],
                                                )
                                            {
                                                let resp_socks_addr = target_to_socks_addr(&resp_target);
                                                if socks_addr_equivalent(&resp_socks_addr, &flow_target) {
                                                    if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) = flow_response_tx.try_send(ResponseMsg {
                                                        client: flow_client,
                                                        target: resp_socks_addr,
                                                        payload: resp_payload,
                                                    }) {
                                                        flow_metrics.record_dropped_response_channel_full();
                                                        tracing::debug!("UDP response channel full; response datagram dropped");
                                                    }
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

                                    config.udp_metrics.record_standalone_flow_created();

                                    let flow = crate::flow::ShadowsocksUdpTargetFlow {
                                        target: request.target.clone(),
                                        upstream_id: upstream.clone(),
                                        upstream_addr,
                                        udp_socket,
                                        method,
                                        password: flow_password,
                                        password_ikm: flow_password_ikm,
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
                                        config.udp_metrics.record_standalone_rejected();
                                        continue;
                                    }
                                }
                                other => {
                                    tracing::error!(
                                        "unexpected flow kind for ShadowsocksUpstream key: {:?}",
                                        std::mem::discriminant(other)
                                    );
                                    config.udp_metrics.record_standalone_rejected();
                                    continue;
                                }
                            }
                        }
                        UdpRelayCapability::SupportedComposed => {
                            let key = UdpFlowKey::Composed {
                                target: request.target.clone(),
                                upstream_id: upstream.clone(),
                            };
                            if !can_use_flow(state, &key, total_flows, &config.limits) {
                                config.udp_metrics.record_standalone_rejected();
                                drop(pending_lease);
                                continue;
                            }

                            let active_lease = pending_lease.established();
                            let entry = match state.target_flows.entry(key) {
                                std::collections::hash_map::Entry::Occupied(mut e) => {
                                    e.get_mut().touch();
                                    e.into_mut()
                                }
                                std::collections::hash_map::Entry::Vacant(e) => {
                                    let flow = match open_composed_udp_upstream(
                                        upstream.clone(),
                                        (*chain).clone(),
                                        local_udp_bind_addr(),
                                        active_lease,
                                    )
                                    .await
                                    {
                                        Ok(flow) => flow,
                                        Err(_) => {
                                            config.udp_metrics.record_standalone_rejected();
                                            continue;
                                        }
                                    };
                                    let flow_socket = flow.socket.clone();
                                    let flow_stack = flow.stack.clone();
                                    let flow_response_tx = response_tx.clone();
                                    let flow_metrics = config.udp_metrics.clone();
                                    let flow_client = client_addr;
                                    let flow_cancel = cancel.clone();
                                    let recv_task = tokio::spawn(async move {
                                        let mut recv_buf = [0u8; 65535];
                                        loop {
                                            let result = tokio::select! {
                                                _ = flow_cancel.cancelled() => break,
                                                result = tokio::time::timeout(
                                                    target_idle_timeout,
                                                    flow_socket.recv_from(&mut recv_buf),
                                                ) => result,
                                            };
                                            let Ok(Ok((n, _peer))) = result else { break };
                                            if let Ok((target, payload)) =
                                                flow_stack.decode_response(&recv_buf[..n])
                                            {
                                                if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) = flow_response_tx.try_send(ResponseMsg {
                                                    client: flow_client,
                                                    target: target_to_socks_addr(&target),
                                                    payload,
                                                }) {
                                                    flow_metrics.record_dropped_response_channel_full();
                                                    tracing::debug!("UDP response channel full; response datagram dropped");
                                                }
                                            }
                                        }
                                    });
                                    config.udp_metrics.record_standalone_flow_created();
                                    e.insert(TargetFlowEntry {
                                        flow: UdpFlowKind::Composed(flow),
                                        recv_task,
                                    })
                                }
                            };

                            if let UdpFlowKind::Composed(flow) = &mut entry.flow {
                                if flow.send(&request.target, request.payload).await.is_err() {
                                    config.udp_metrics.record_standalone_rejected();
                                }
                            } else {
                                config.udp_metrics.record_standalone_rejected();
                            }
                        }
                        UdpRelayCapability::UnsupportedProtocol { .. } => {
                            config.udp_metrics.record_standalone_rejected();
                            drop(pending_lease);
                        }
                        UdpRelayCapability::UnsupportedMultiHop => {
                            config.udp_metrics.record_standalone_rejected();
                            drop(pending_lease);
                        }
                    },
                }
            }
            _ = idle_tick.tick() => {
                reap_idle_flows(&mut clients, &config.limits, &config.udp_metrics);
            }
            _ = cancel.cancelled() => {
                break;
            }
        }
    }

    close_all_flows(&mut clients, &config.udp_metrics);

    Ok(())
}

async fn build_direct_flow(
    config: &StandaloneUdpConfig,
    client_addr: SocketAddr,
    response_tx: &tokio::sync::mpsc::Sender<ResponseMsg>,
    target: SocksAddr,
    cancel: &CancellationToken,
) -> Result<TargetFlowEntry, UdpError> {
    let flow = UdpTargetFlow::new(target.clone(), local_udp_bind_addr()).await?;
    let target_idle_timeout = config.limits.target_idle_timeout;

    let flow_response_tx = response_tx.clone();
    let flow_metrics = config.udp_metrics.clone();
    let flow_target = target.clone();
    let flow_socket = flow.socket.clone();
    let flow_cancel = cancel.clone();

    let recv_task = tokio::spawn(async move {
        let mut recv_buf = [0u8; 65535];
        loop {
            let result = tokio::select! {
                _ = flow_cancel.cancelled() => break,
                result = tokio::time::timeout(
                    target_idle_timeout,
                    flow_socket.recv(&mut recv_buf),
                ) => result,
            };
            let Ok(Ok(n)) = result else { break };
            let payload = recv_buf[..n].to_vec();
            if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) =
                flow_response_tx.try_send(ResponseMsg {
                    client: client_addr,
                    target: flow_target.clone(),
                    payload,
                })
            {
                flow_metrics.record_dropped_response_channel_full();
                tracing::debug!("UDP response channel full; response datagram dropped");
            }
        }
    });

    config.udp_metrics.record_standalone_flow_created();

    Ok(TargetFlowEntry {
        flow: UdpFlowKind::Direct(flow),
        recv_task,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::UdpLimits;
    use crate::max_standalone_flows;
    use crate::metrics::UdpMetrics;
    use eggress_core::{TargetAddr, TargetHost};
    use std::sync::atomic::Ordering;

    fn direct_router() -> Arc<dyn RouteService> {
        Arc::new(eggress_routing::Router::new(
            vec![],
            eggress_routing::RouteActionSpec::Direct,
        ))
    }

    fn reject_router() -> Arc<dyn RouteService> {
        let rules = vec![eggress_routing::CompiledRule {
            id: eggress_routing::RuleId(std::sync::Arc::from("reject-all")),
            matcher: eggress_routing::MatchExpr::Any,
            action: eggress_routing::RouteActionSpec::Reject(
                eggress_core::RejectReason::AccessDenied,
            ),
        }];
        Arc::new(eggress_routing::Router::new(
            rules,
            eggress_routing::RouteActionSpec::Direct,
        ))
    }

    fn standalone_config(routing: Arc<dyn RouteService>) -> StandaloneUdpConfig {
        StandaloneUdpConfig {
            routing,
            udp_metrics: Arc::new(UdpMetrics::new()),
            limits: UdpLimits::default(),
            listener: "test-standalone".to_string(),
            generation: 1,
            allow_private_egress: true,
        }
    }

    fn standalone_config_with_limits(
        routing: Arc<dyn RouteService>,
        limits: UdpLimits,
    ) -> StandaloneUdpConfig {
        StandaloneUdpConfig {
            routing,
            udp_metrics: Arc::new(UdpMetrics::new()),
            limits,
            listener: "test-standalone".to_string(),
            generation: 1,
            allow_private_egress: true,
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
    async fn standalone_echo_ipv4() {
        let echo_addr = start_udp_echo().await;
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let config = standalone_config(direct_router());
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle =
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"hello standalone");
        client_socket.send(&pkt).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();
        let resp = decode_packet(&recv_buf[..n], &UdpLimits::default()).unwrap();
        assert_eq!(resp.payload, b"hello standalone");

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_route_reject_drops_packet() {
        let echo_addr = start_udp_echo().await;
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = standalone_config(reject_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle =
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"should be dropped");
        client_socket.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(
            udp_metrics
                .standalone_rejected_datagrams
                .load(Ordering::Relaxed),
            1
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_records_metrics() {
        let echo_addr = start_udp_echo().await;
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = standalone_config(direct_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle =
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

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

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_closes_on_cancel() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());

        let config = standalone_config(direct_router());
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle =
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();

        let result = relay_handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn standalone_flow_reused_for_same_target() {
        let echo_addr = start_udp_echo().await;
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = standalone_config(direct_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle =
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

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

        // Only one flow created despite two packets to the same target
        assert_eq!(
            udp_metrics.standalone_flows_active.load(Ordering::Relaxed),
            1
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_per_client_limit_enforced() {
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
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        // First packet to target 8081 should create a flow
        let pkt1 = ipv4_socks5_packet([127, 0, 0, 1], 8081, b"first");
        client_socket.send(&pkt1).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Second packet to different target should be dropped (limit = 1)
        let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], 8082, b"second");
        client_socket.send(&pkt2).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(
            udp_metrics
                .standalone_rejected_datagrams
                .load(Ordering::Relaxed),
            1
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_per_client_limit_allows_reuse() {
        let echo_addr = start_udp_echo().await;
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
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

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

        let reuse = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"reuse");
        client_socket.send(&reuse).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();

        assert_eq!(
            udp_metrics
                .standalone_rejected_datagrams
                .load(Ordering::Relaxed),
            0
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_decode_error_recorded() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = standalone_config(direct_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle =
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        // Send a malformed packet (too short for SOCKS5 UDP header)
        client_socket.send(&[0x00, 0x00]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(
            udp_metrics
                .standalone_malformed_datagrams
                .load(Ordering::Relaxed),
            1
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_target_flow_timeout() {
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
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        // Send a packet to create a flow
        let pkt = ipv4_socks5_packet([127, 0, 0, 1], 8080, b"timeout test");
        client_socket.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(
            udp_metrics.standalone_flows_active.load(Ordering::Relaxed),
            1
        );

        // Wait for target idle timeout
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        assert_eq!(
            udp_metrics.standalone_flows_active.load(Ordering::Relaxed),
            0
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_ipv6_target() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = standalone_config(direct_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle =
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        // Send to a likely-unreachable IPv6 target - should not panic, just drop
        let pkt = ipv4_socks5_packet([127, 0, 0, 1], 9999, b"v6 test");
        client_socket.send(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_global_flow_cap_enforced() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let limits = UdpLimits {
            max_standalone_flows: 2,
            max_targets_per_association: 10,
            ..UdpLimits::default()
        };
        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = standalone_config_with_limits(direct_router(), limits);
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle =
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        // Create two flows (to different targets) from client1
        let pkt1 = ipv4_socks5_packet([127, 0, 0, 1], 8081, b"flow1");
        client_socket.send(&pkt1).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], 8082, b"flow2");
        client_socket.send(&pkt2).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Third flow from a different client should be dropped (global cap = 2)
        let client2 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client2.connect(relay_addr).await.unwrap();

        let pkt3 = ipv4_socks5_packet([127, 0, 0, 1], 8083, b"flow3");
        client2.send(&pkt3).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(
            udp_metrics
                .standalone_rejected_datagrams
                .load(Ordering::Relaxed),
            1
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_global_flow_cap_rejects_new_target_from_existing_client() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let limits = UdpLimits {
            max_standalone_flows: 1,
            max_targets_per_association: 10,
            ..UdpLimits::default()
        };
        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = standalone_config_with_limits(direct_router(), limits);
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle =
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let pkt1 = ipv4_socks5_packet([127, 0, 0, 1], 8081, b"flow1");
        client_socket.send(&pkt1).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], 8082, b"flow2");
        client_socket.send(&pkt2).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(
            udp_metrics
                .standalone_rejected_datagrams
                .load(Ordering::Relaxed),
            1
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn standalone_flow_cap_allows_reuse() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let limits = UdpLimits {
            max_standalone_flows: 2,
            max_targets_per_association: 10,
            ..UdpLimits::default()
        };
        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = standalone_config_with_limits(direct_router(), limits);
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle =
            tokio::spawn(
                async move { standalone_udp_relay(relay_sock, config, relay_cancel).await },
            );

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        // Create two flows
        let pkt1 = ipv4_socks5_packet([127, 0, 0, 1], 8081, b"flow1");
        client_socket.send(&pkt1).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let pkt2 = ipv4_socks5_packet([127, 0, 0, 1], 8082, b"flow2");
        client_socket.send(&pkt2).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Reuse of existing flow should succeed even at cap
        let pkt1_reuse = ipv4_socks5_packet([127, 0, 0, 1], 8081, b"reuse");
        client_socket.send(&pkt1_reuse).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // No dropped packets - reuse is allowed
        assert_eq!(udp_metrics.dropped_packets.load(Ordering::Relaxed), 0);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
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

    #[test]
    fn socks_addr_equivalent_ipv4() {
        let a = SocksAddr::IPv4([127, 0, 0, 1], 80);
        let b = SocksAddr::IPv4([127, 0, 0, 1], 80);
        assert!(socks_addr_equivalent(&a, &b));

        let c = SocksAddr::IPv4([127, 0, 0, 1], 443);
        assert!(!socks_addr_equivalent(&a, &c));
    }

    #[test]
    fn socks_addr_equivalent_domain() {
        let a = SocksAddr::Domain("example.com".to_string(), 443);
        let b = SocksAddr::Domain("example.com".to_string(), 443);
        assert!(socks_addr_equivalent(&a, &b));

        let c = SocksAddr::Domain("other.com".to_string(), 443);
        assert!(!socks_addr_equivalent(&a, &c));
    }

    #[test]
    fn socks_addr_equivalent_mixed() {
        let ipv4 = SocksAddr::IPv4([127, 0, 0, 1], 80);
        let domain = SocksAddr::Domain("example.com".to_string(), 80);
        assert!(!socks_addr_equivalent(&ipv4, &domain));
    }

    #[test]
    fn target_to_socks_addr_roundtrip() {
        let addr = TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V4("10.0.0.1".parse().unwrap())),
            port: 9090,
        };
        let socks = target_to_socks_addr(&addr);
        assert_eq!(socks, SocksAddr::IPv4([10, 0, 0, 1], 9090));

        let addr = TargetAddr {
            host: TargetHost::Domain("test.example".to_string()),
            port: 443,
        };
        let socks = target_to_socks_addr(&addr);
        assert_eq!(socks, SocksAddr::Domain("test.example".to_string(), 443));
    }

    #[test]
    fn max_standalone_flows_default_uses_global() {
        let limits = UdpLimits::default();
        assert_eq!(
            max_standalone_flows(&limits),
            limits.max_associations_global
        );
    }

    #[test]
    fn max_standalone_flows_explicit() {
        let limits = UdpLimits {
            max_standalone_flows: 42,
            ..UdpLimits::default()
        };
        assert_eq!(max_standalone_flows(&limits), 42);
    }
}
