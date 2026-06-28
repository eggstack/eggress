use std::sync::atomic::Ordering;

use eggress_core::capability::classify_upstream_chain;

use crate::pac::generate_pac;
use crate::server::{
    build_json_response, build_not_found, build_response, build_text_response, AdminResponse,
    AdminState,
};
use crate::static_content::serve_static;

const MAX_ADMIN_BODY: usize = 16 * 1024;
const MAX_IDENTITY_LEN: usize = 256;

pub async fn handle_request(
    req: http::Request<hyper::body::Incoming>,
    state: &AdminState,
) -> AdminResponse {
    let path = req.uri().path();
    let method = req.method();

    match path {
        "/-/health" => build_text_response(200, "ok"),
        "/-/ready" => {
            if state.readiness.load(Ordering::Relaxed) {
                build_text_response(200, "ready")
            } else {
                build_text_response(503, "not ready")
            }
        }
        "/-/status" => {
            let snap = state.snapshot();
            let uptime = state.start_time.elapsed().as_secs();
            let active = state
                .active_connections
                .as_ref()
                .map(|c| c.load(Ordering::Relaxed))
                .unwrap_or(0);
            let listeners: Vec<serde_json::Value> = snap
                .listeners
                .iter()
                .map(|l| {
                    serde_json::json!({
                        "name": l.name,
                        "bind": l.bind,
                        "protocols": l.protocols,
                    })
                })
                .collect();
            let status = serde_json::json!({
                "version": "0.1.0",
                "generation": snap.generation,
                "uptime_seconds": uptime,
                "active_connections": active,
                "listeners": listeners,
            });
            build_json_response(200, status.to_string())
        }
        "/-/routes" => {
            let router = &state.snapshot().router;
            let default_action = format!("{:?}", router.default_action());
            let rules: Vec<serde_json::Value> = router
                .rules()
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.id.0.to_string(),
                        "action": format!("{:?}", r.action),
                    })
                })
                .collect();
            let body = serde_json::json!({
                "rules": rules,
                "default_action": default_action,
                "rule_count": rules.len(),
            });
            build_json_response(200, body.to_string())
        }
        "/-/upstreams" => {
            let router = &state.snapshot().router;
            let groups: Vec<serde_json::Value> = router
                .groups()
                .iter()
                .map(|(gid, group)| {
                    let members: Vec<serde_json::Value> = group
                        .members
                        .iter()
                        .map(|m| {
                            let health_state = m.health.state();
                            let eligible = eggress_routing::health::is_eligible(m);
                            let caps = classify_upstream_chain(&m.chain);
                            let protocols: Vec<String> = m
                                .chain
                                .hops
                                .iter()
                                .flat_map(|h| h.protocols.iter())
                                .map(|p| format!("{:?}", p).to_lowercase())
                                .collect();
                            let tcp_connect = if caps.is_tcp_supported() {
                                "supported"
                            } else {
                                "unsupported"
                            };
                            let udp_associate = if caps.is_udp_supported() {
                                "supported"
                            } else {
                                "unsupported"
                            };
                            serde_json::json!({
                                "id": m.id.to_string(),
                                "protocols": protocols,
                                "tcp_connect": tcp_connect,
                                "udp_associate": udp_associate,
                                "health": format!("{:?}", health_state),
                                "eligible": eligible,
                                "enabled": m.is_enabled(),
                                "active": m.active.load(Ordering::Relaxed),
                                "in_flight": m.in_flight.load(Ordering::Relaxed),
                            })
                        })
                        .collect();
                    let sched_name = match group.scheduler_kind {
                        eggress_routing::scheduler::SchedulerKind::FirstAvailable => {
                            "first-available"
                        }
                        eggress_routing::scheduler::SchedulerKind::RoundRobin => "round-robin",
                        eggress_routing::scheduler::SchedulerKind::Random => "random",
                        eggress_routing::scheduler::SchedulerKind::LeastConnections => {
                            "least-connections"
                        }
                    };
                    serde_json::json!({
                        "group_id": gid.0.to_string(),
                        "scheduler": sched_name,
                        "member_count": group.members.len(),
                        "members": members,
                    })
                })
                .collect();
            build_json_response(
                200,
                match serde_json::to_string(&groups) {
                    Ok(s) => s,
                    Err(e) => return build_json_response(500, format!("serialization error: {e}")),
                },
            )
        }
        "/-/config" => {
            let snap = state.snapshot();
            let router = &snap.router;
            let uptime = state.start_time.elapsed().as_secs();
            let rule_count = router.rules().len();
            let upstream_group_count = router.groups().len();
            let default_action = format!("{:?}", router.default_action());
            let listener_names: Vec<&str> =
                snap.listeners.iter().map(|l| l.name.as_str()).collect();
            let config_summary = serde_json::json!({
                "generation": snap.generation,
                "uptime_seconds": uptime,
                "has_router": true,
                "rule_count": rule_count,
                "upstream_group_count": upstream_group_count,
                "default_action": default_action,
                "static_routes_count": snap.static_routes.len(),
                "has_pac": snap.pac.is_some(),
                "listeners": listener_names,
                "active_connections": state.active_connections.as_ref().map(|c| c.load(Ordering::Relaxed)).unwrap_or(0),
            });
            build_json_response(200, config_summary.to_string())
        }
        "/-/udp" => {
            let snap = state.snapshot();
            let mut listeners = Vec::with_capacity(snap.listeners.len());
            for l in &snap.listeners {
                let active = state.udp_registry.active_count_for_listener(&l.name).await;
                listeners.push(serde_json::json!({
                    "name": l.name,
                    "udp_enabled": l.udp_enabled,
                    "active_associations": active,
                }));
            }
            let udp = serde_json::json!({
                "associations_active": state.metrics.udp_associations_active_gauge(),
                "target_flows_active": state.metrics.udp_target_flows_active_gauge(),
                "upstream_flows_active": state.metrics.udp_upstream_associations_active_gauge(),
                "listeners": listeners,
            });
            build_json_response(200, udp.to_string())
        }
        "/-/route-explain" => {
            if method != http::Method::POST {
                return build_text_response(405, "method not allowed");
            }
            let snap = state.snapshot();
            let router = &snap.router;
            let body_bytes = match http_body_util::BodyExt::collect(req.into_body()).await {
                Ok(b) => b.to_bytes(),
                Err(_) => {
                    return build_json_response(
                        400,
                        serde_json::json!({"error": "failed to read request body"}).to_string(),
                    );
                }
            };
            if body_bytes.len() > MAX_ADMIN_BODY {
                return build_json_response(
                    413,
                    serde_json::json!({"error": "request body too large"}).to_string(),
                );
            }
            let body: serde_json::Value = match serde_json::from_slice(&body_bytes) {
                Ok(v) => v,
                Err(_) => {
                    return build_json_response(
                        400,
                        serde_json::json!({"error": "invalid JSON body"}).to_string(),
                    );
                }
            };
            let target_str = match body.get("target").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => {
                    return build_json_response(
                        400,
                        serde_json::json!({"error": "missing 'target' field"}).to_string(),
                    );
                }
            };
            let listener_str = match body.get("listener").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => {
                    return build_json_response(
                        400,
                        serde_json::json!({"error": "missing 'listener' field"}).to_string(),
                    );
                }
            };
            let protocol_str = match body.get("protocol").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => {
                    return build_json_response(
                        400,
                        serde_json::json!({"error": "missing 'protocol' field"}).to_string(),
                    );
                }
            };
            let target: eggress_core::TargetAddr = match target_str.parse() {
                Ok(t) => t,
                Err(e) => {
                    return build_json_response(
                        400,
                        serde_json::json!({"error": format!("invalid target: {e}")}).to_string(),
                    );
                }
            };
            let protocol = match protocol_str {
                "http" => eggress_core::ProtocolId::Http,
                "socks4" => eggress_core::ProtocolId::Socks4,
                "socks5" => eggress_core::ProtocolId::Socks5,
                _ => {
                    return build_json_response(
                        400,
                        serde_json::json!({"error": format!("unknown protocol: {protocol_str}")})
                            .to_string(),
                    );
                }
            };
            let source = match body.get("source").and_then(|v| v.as_str()) {
                Some(raw) => match raw.parse::<std::net::SocketAddr>() {
                    Ok(addr) => Some(addr),
                    Err(e) => {
                        return build_json_response(
                            400,
                            serde_json::json!({"error": format!("invalid source: {e}")})
                                .to_string(),
                        );
                    }
                },
                None => None,
            };
            let identity_owned;
            let identity: &eggress_core::ClientIdentity =
                match body.get("identity").and_then(|v| v.as_str()) {
                    Some("") => {
                        return build_json_response(
                            400,
                            serde_json::json!({"error": "identity must be non-empty"}).to_string(),
                        );
                    }
                    Some(raw) if raw.len() > MAX_IDENTITY_LEN => {
                        return build_json_response(
                            400,
                            serde_json::json!({"error": "identity too long"}).to_string(),
                        );
                    }
                    Some(raw) => {
                        identity_owned = eggress_core::ClientIdentity::Username(raw.to_string());
                        &identity_owned
                    }
                    None => &eggress_core::ClientIdentity::Anonymous,
                };
            let request = eggress_routing::RouteRequest {
                target: &target,
                source,
                listener: listener_str,
                inbound_protocol: protocol,
                identity,
                transport: eggress_routing::TransportKind::Tcp,
            };
            let explanation = router.explain(&request, snap.generation);
            build_json_response(
                200,
                match serde_json::to_string(&explanation) {
                    Ok(s) => s,
                    Err(e) => return build_json_response(500, format!("serialization error: {e}")),
                },
            )
        }
        "/metrics" => build_response(
            200,
            state.metrics.render_prometheus(),
            "text/plain; version=0.0.4",
        ),
        "/pac" => {
            if let Some(pac_config) = state.snapshot().pac.as_ref() {
                let pac = generate_pac(pac_config);
                build_response(200, pac, "application/x-ns-proxy-autoconfig")
            } else {
                build_text_response(404, "pac not configured")
            }
        }
        _ => {
            for route in state.snapshot().static_routes.iter() {
                if route.path == path {
                    return serve_static(route);
                }
            }
            build_not_found()
        }
    }
}
