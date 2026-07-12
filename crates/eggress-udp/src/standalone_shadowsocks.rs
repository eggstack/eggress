use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

use crate::direct::UdpTargetFlow;
use crate::error::UdpError;
use crate::flow::{
    can_use_flow, close_all_flows, local_udp_bind_addr, reap_idle_flows, resolve_endpoint,
    socks_addr_equivalent, socks_to_target_addr, target_to_socks_addr, total_target_flows,
    ClientFlowState, TargetFlowEntry, UdpFlowKey, UdpFlowKind,
};
use crate::limits::UdpLimits;
use crate::metrics::UdpMetrics;
use crate::security::validate_standalone_target;
use crate::udp_capability::{udp_capability, UdpRelayCapability};
use crate::upstream_socks5::{open_socks5_udp_upstream, Socks5UdpUpstreamConfig};
use eggress_core::{ClientIdentity, ProtocolId, TargetAddr};
use eggress_protocol_shadowsocks::udp::{decode_udp_packet, encode_udp_packet};
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_routing::{
    RouteError, RouteRequest, RouteService, SelectedRoute, SelectionReason, TransportKind,
};

pub struct ShadowsocksStandaloneUdpConfig {
    pub routing: Arc<dyn RouteService>,
    pub udp_metrics: Arc<UdpMetrics>,
    pub shadowsocks_metrics: Option<Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>>,
    pub limits: UdpLimits,
    pub listener: String,
    pub generation: u64,
    pub method: eggress_protocol_shadowsocks::CipherMethod,
    pub password: String,
    pub allow_private_egress: bool,
}

struct ResponseMsg {
    client: SocketAddr,
    target: SocksAddr,
    payload: Vec<u8>,
}

pub async fn shadowsocks_standalone_udp_relay(
    socket: Arc<UdpSocket>,
    config: ShadowsocksStandaloneUdpConfig,
    cancel: CancellationToken,
) -> Result<(), UdpError> {
    let mut buf = vec![0u8; config.limits.max_datagram_size];
    let mut clients: HashMap<SocketAddr, ClientFlowState> = HashMap::new();
    let (response_tx, mut response_rx) = tokio::sync::mpsc::unbounded_channel::<ResponseMsg>();

    let socket_clone = socket.clone();
    let metrics_clone = config.udp_metrics.clone();
    let ss_metrics_clone = config.shadowsocks_metrics.clone();
    let resp_method = config.method;
    let resp_password = config.password.as_bytes().to_vec();
    tokio::spawn(async move {
        while let Some(msg) = response_rx.recv().await {
            let target_addr = socks_to_target_addr(&msg.target);
            let mut salt = vec![0u8; resp_method.salt_size()];
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
            match encode_udp_packet(
                resp_method,
                &resp_password,
                &target_addr,
                &msg.payload,
                &salt,
            ) {
                Ok(encoded) => {
                    if socket_clone.send_to(&encoded, msg.client).await.is_ok() {
                        metrics_clone.record_standalone_packet_out(encoded.len() as u64);
                        if let Some(ss) = ss_metrics_clone.as_ref() {
                            ss.record_udp_packet_out(encoded.len() as u64);
                        }
                    } else {
                        metrics_clone.record_dropped();
                    }
                }
                Err(_) => {
                    metrics_clone.record_dropped();
                }
            }
        }
    });

    let mut idle_tick = tokio::time::interval(config.limits.idle_timeout);
    idle_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut target_cleanup_tick = tokio::time::interval(config.limits.target_idle_timeout);
    target_cleanup_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            result = socket.recv_from(&mut buf) => {
                let (n, client_addr) = result?;

                let packet = &buf[..n];

                let (target_addr, payload) = match decode_udp_packet(
                    config.method,
                    config.password.as_bytes(),
                    packet,
                ) {
                    Ok(r) => r,
                    Err(_) => {
                        config.udp_metrics.record_standalone_malformed();
                        if let Some(ss) = config.shadowsocks_metrics.as_ref() {
                            ss.record_udp_decrypt_failure();
                        }
                        continue;
                    }
                };

                let target_socks = target_to_socks_addr(&target_addr);

                if validate_standalone_target(&target_socks, config.allow_private_egress).is_err() {
                    config.udp_metrics.record_standalone_rejected();
                    continue;
                }

                config.udp_metrics.record_standalone_packet_in(n as u64);
                if let Some(ss) = config.shadowsocks_metrics.as_ref() {
                    ss.record_udp_packet_in(n as u64);
                }

                let route_target = TargetAddr {
                    host: target_addr.host.clone(),
                    port: target_addr.port,
                };
                let route_request = RouteRequest {
                    target: &route_target,
                    source: Some(client_addr),
                    listener: &config.listener,
                    inbound_protocol: ProtocolId::Shadowsocks,
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
                                target = %target_socks.host_str(),
                                "Shadowsocks UDP standalone using direct fallback"
                            );
                        }

                        let key = UdpFlowKey::Direct {
                            target: target_socks.clone(),
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
                            if f.send(&payload).await.is_err() {
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
                                target: target_socks.clone(),
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
                                            let target = target_socks.clone();
                                            let udp_socket = upstream_assoc.udp_socket.clone();
                                            let relay_addr = upstream_assoc.relay_addr;
                                            let upstream_id = upstream_assoc.upstream_id.clone();
                                            let control_cancel = upstream_assoc.control_cancel.clone();
                                            let control_task = upstream_assoc.control_task;

                                            let flow_response_tx = response_tx.clone();
                                            let flow_target = target.clone();
                                            let flow_socket = udp_socket.clone();
                                            let flow_client = client_addr;

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
                                                                client: flow_client,
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

                                            config.udp_metrics.record_standalone_flow_created();

                                            let flow = crate::flow::Socks5UdpTargetFlow {
                                                target: target_socks.clone(),
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
                                    if let Err(_e) = f.send(&target_socks, &payload).await {
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
                        UdpRelayCapability::SupportedShadowsocks { method, password } => {
                            let key = UdpFlowKey::ShadowsocksUpstream {
                                target: target_socks.clone(),
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
                                    let upstream_addr = resolve_endpoint(&hop.endpoint).await?;

                                    let udp_socket = Arc::new(
                                        UdpSocket::bind(local_udp_bind_addr())
                                            .await
                                            .map_err(|e| UdpError::Other(e.to_string()))?,
                                    );

                                    let flow_response_tx = response_tx.clone();
                                    let flow_target = target_socks.clone();
                                    let flow_socket = udp_socket.clone();
                                    let flow_method = method;
                                    let flow_password = password.as_bytes().to_vec();
                                    let flow_client = client_addr;

                                    let recv_task = tokio::spawn(async move {
                                        let mut recv_buf = [0u8; 65535];
                                        while let Ok(Ok((n, _peer))) = tokio::time::timeout(
                                            std::time::Duration::from_secs(30),
                                            flow_socket.recv_from(&mut recv_buf),
                                        )
                                        .await
                                        {
                                            if let Ok((resp_target, resp_payload)) =
                                                decode_udp_packet(
                                                    flow_method,
                                                    &flow_password,
                                                    &recv_buf[..n],
                                                )
                                            {
                                                let resp_socks_addr = target_to_socks_addr(&resp_target);
                                                if socks_addr_equivalent(&resp_socks_addr, &flow_target) {
                                                    let _ = flow_response_tx.send(ResponseMsg {
                                                        client: flow_client,
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

                                    config.udp_metrics.record_standalone_flow_created();

                                    let flow = crate::flow::ShadowsocksUdpTargetFlow {
                                        target: target_socks.clone(),
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
                                    if let Err(_e) = f.send(&target_socks, &payload).await {
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
            _ = target_cleanup_tick.tick() => {
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
    config: &ShadowsocksStandaloneUdpConfig,
    client_addr: SocketAddr,
    response_tx: &tokio::sync::mpsc::UnboundedSender<ResponseMsg>,
    target: SocksAddr,
) -> Result<TargetFlowEntry, UdpError> {
    let flow = UdpTargetFlow::new(target.clone(), local_udp_bind_addr()).await?;

    let flow_response_tx = response_tx.clone();
    let flow_target = target.clone();
    let flow_socket = flow.socket.clone();

    let recv_task = tokio::spawn(async move {
        let mut recv_buf = [0u8; 65535];
        while let Ok(Ok(n)) = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            flow_socket.recv(&mut recv_buf),
        )
        .await
        {
            let payload = recv_buf[..n].to_vec();
            let _ = flow_response_tx.send(ResponseMsg {
                client: client_addr,
                target: flow_target.clone(),
                payload,
            });
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
    use crate::metrics::UdpMetrics;
    use eggress_core::TargetHost;
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

    fn shadowsocks_config(routing: Arc<dyn RouteService>) -> ShadowsocksStandaloneUdpConfig {
        ShadowsocksStandaloneUdpConfig {
            routing,
            udp_metrics: Arc::new(UdpMetrics::new()),
            shadowsocks_metrics: None,
            limits: UdpLimits::default(),
            listener: "test-shadowsocks-standalone".to_string(),
            generation: 1,
            method: eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            password: "test-password-123456".to_string(),
            allow_private_egress: true,
        }
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

    fn encode_ss_packet(
        method: eggress_protocol_shadowsocks::CipherMethod,
        password: &[u8],
        target: &TargetAddr,
        payload: &[u8],
    ) -> Vec<u8> {
        let salt = [0x42u8; 16];
        encode_udp_packet(method, password, target, payload, &salt).unwrap()
    }

    #[tokio::test]
    async fn shadowsocks_standalone_echo_ipv4() {
        let echo_addr = start_udp_echo().await;
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let config = shadowsocks_config(direct_router());
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: echo_addr.port(),
        };
        let pkt = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target,
            b"hello shadowsocks",
        );
        client_socket.send(&pkt).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();
        let (resp_target, resp_payload) = decode_udp_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &recv_buf[..n],
        )
        .unwrap();
        assert_eq!(resp_payload, b"hello shadowsocks");
        assert_eq!(resp_target.port, echo_addr.port());

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn shadowsocks_standalone_route_reject_drops_packet() {
        let echo_addr = start_udp_echo().await;
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = shadowsocks_config(reject_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: echo_addr.port(),
        };
        let pkt = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target,
            b"should be dropped",
        );
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
    async fn shadowsocks_standalone_records_metrics() {
        let echo_addr = start_udp_echo().await;
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = shadowsocks_config(direct_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: echo_addr.port(),
        };
        let pkt = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target,
            b"metrics test",
        );
        client_socket.send(&pkt).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();
        let (_, resp_payload) = decode_udp_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &recv_buf[..n],
        )
        .unwrap();
        assert_eq!(resp_payload, b"metrics test");

        assert!(udp_metrics.standalone_packets_in.load(Ordering::Relaxed) >= 1);
        assert!(udp_metrics.standalone_packets_out.load(Ordering::Relaxed) >= 1);
        assert!(udp_metrics.standalone_flows_active.load(Ordering::Relaxed) >= 1);

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn shadowsocks_standalone_closes_on_cancel() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());

        let config = shadowsocks_config(direct_router());
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();

        let result = relay_handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn shadowsocks_standalone_malformed_packet_recorded() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = shadowsocks_config(direct_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

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
    async fn shadowsocks_standalone_flow_reused_for_same_target() {
        let echo_addr = start_udp_echo().await;
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = shadowsocks_config(direct_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: echo_addr.port(),
        };
        let pkt1 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target,
            b"reuse1",
        );
        client_socket.send(&pkt1).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();

        let pkt2 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target,
            b"reuse2",
        );
        client_socket.send(&pkt2).await.unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();

        assert_eq!(
            udp_metrics.standalone_flows_active.load(Ordering::Relaxed),
            1
        );

        cancel.cancel();
        relay_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn shadowsocks_standalone_per_client_limit_enforced() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let limits = UdpLimits {
            max_targets_per_association: 1,
            ..UdpLimits::default()
        };
        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = shadowsocks_config(direct_router());
        config.limits = limits;
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let target1 = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 8081,
        };
        let pkt1 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target1,
            b"first",
        );
        client_socket.send(&pkt1).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let target2 = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 8082,
        };
        let pkt2 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target2,
            b"second",
        );
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
    async fn shadowsocks_standalone_per_client_limit_allows_reuse() {
        let echo_addr = start_udp_echo().await;
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let limits = UdpLimits {
            max_targets_per_association: 1,
            ..UdpLimits::default()
        };
        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = shadowsocks_config(direct_router());
        config.limits = limits;
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: echo_addr.port(),
        };
        let pkt1 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target,
            b"first",
        );
        client_socket.send(&pkt1).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap()
        .unwrap();

        let pkt2 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target,
            b"reuse",
        );
        client_socket.send(&pkt2).await.unwrap();
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
    async fn shadowsocks_standalone_wrong_password_fails() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = shadowsocks_config(direct_router());
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 8080,
        };
        let pkt = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"wrong-password-xxxxx",
            &target,
            b"wrong password",
        );
        client_socket.send(&pkt).await.unwrap();
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
    async fn shadowsocks_standalone_global_flow_cap_enforced() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let limits = UdpLimits {
            max_standalone_flows: 2,
            max_targets_per_association: 10,
            ..UdpLimits::default()
        };
        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = shadowsocks_config(direct_router());
        config.limits = limits;
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let target1 = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 8081,
        };
        let pkt1 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target1,
            b"flow1",
        );
        client_socket.send(&pkt1).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let target2 = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 8082,
        };
        let pkt2 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target2,
            b"flow2",
        );
        client_socket.send(&pkt2).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client2 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client2.connect(relay_addr).await.unwrap();

        let target3 = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 8083,
        };
        let pkt3 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target3,
            b"flow3",
        );
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
    async fn shadowsocks_standalone_global_flow_cap_rejects_new_target_from_existing_client() {
        let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();

        let limits = UdpLimits {
            max_standalone_flows: 1,
            max_targets_per_association: 10,
            ..UdpLimits::default()
        };
        let udp_metrics = Arc::new(UdpMetrics::new());
        let mut config = shadowsocks_config(direct_router());
        config.limits = limits;
        config.udp_metrics = udp_metrics.clone();
        let cancel = CancellationToken::new();

        let relay_cancel = cancel.clone();
        let relay_sock = relay_socket.clone();
        let relay_handle = tokio::spawn(async move {
            shadowsocks_standalone_udp_relay(relay_sock, config, relay_cancel).await
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_socket.connect(relay_addr).await.unwrap();

        let target1 = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 8081,
        };
        let pkt1 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target1,
            b"flow1",
        );
        client_socket.send(&pkt1).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let target2 = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 8082,
        };
        let pkt2 = encode_ss_packet(
            eggress_protocol_shadowsocks::CipherMethod::Aes256Gcm,
            b"test-password-123456",
            &target2,
            b"flow2",
        );
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
}
