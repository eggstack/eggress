use std::sync::atomic::Ordering;

use crate::pac::generate_pac;
use crate::server::{
    build_json_response, build_not_found, build_response, build_text_response, AdminResponse,
    AdminState,
};
use crate::static_content::serve_static;

pub fn handle_request(
    req: http::Request<hyper::body::Incoming>,
    state: &AdminState,
) -> AdminResponse {
    let path = req.uri().path();

    match path {
        "/-/health" => build_text_response(200, "ok"),
        "/-/ready" => build_text_response(200, "ready"),
        "/-/status" => {
            let uptime = state.start_time.elapsed().as_secs();
            let generation = state.generation.load(Ordering::Relaxed);
            let active = state
                .active_connections
                .as_ref()
                .map(|c| c.load(Ordering::Relaxed))
                .unwrap_or(0);
            let listeners: Vec<serde_json::Value> = state
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
                "generation": generation,
                "uptime_seconds": uptime,
                "active_connections": active,
                "listeners": listeners,
            });
            build_json_response(200, status.to_string())
        }
        "/-/routes" => {
            if let Some(router) = &state.router {
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
                build_json_response(200, serde_json::to_string(&rules).unwrap())
            } else {
                let rules: Vec<serde_json::Value> = state
                    .static_routes
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "path": r.path,
                            "content_type": r.content_type,
                            "body_len": r.body.len(),
                        })
                    })
                    .collect();
                build_json_response(200, serde_json::to_string(&rules).unwrap())
            }
        }
        "/-/upstreams" => {
            if let Some(router) = &state.router {
                let groups: Vec<serde_json::Value> = router
                    .rules()
                    .iter()
                    .filter_map(|r| {
                        if let eggress_routing::RouteActionSpec::UpstreamGroup(gid) = &r.action {
                            Some(serde_json::json!({
                                "group_id": gid.0.to_string(),
                                "rule_id": r.id.0.to_string(),
                            }))
                        } else {
                            None
                        }
                    })
                    .collect();
                build_json_response(200, serde_json::to_string(&groups).unwrap())
            } else {
                build_json_response(200, "[]")
            }
        }
        "/-/config" => {
            let generation = state.generation.load(Ordering::Relaxed);
            let uptime = state.start_time.elapsed().as_secs();
            let config_summary = serde_json::json!({
                "generation": generation,
                "uptime_seconds": uptime,
                "has_router": state.router.is_some(),
                "static_routes_count": state.static_routes.len(),
                "has_pac": state.pac_config.is_some(),
            });
            build_json_response(200, config_summary.to_string())
        }
        "/metrics" => build_response(
            200,
            state.metrics.render_prometheus(),
            "text/plain; version=0.0.4",
        ),
        "/pac" => {
            if let Some(pac_config) = state.pac_config.as_ref() {
                let pac = generate_pac(pac_config);
                build_response(200, pac, "application/x-ns-proxy-autoconfig")
            } else {
                build_text_response(404, "pac not configured")
            }
        }
        _ => {
            for route in state.static_routes.iter() {
                if route.path == path {
                    return serve_static(route);
                }
            }
            build_not_found()
        }
    }
}
