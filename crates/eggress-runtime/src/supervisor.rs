use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eggress_config::compile::{GroupFallback, RuntimeConfig};
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::ProtocolId;
use eggress_routing::health::{HealthConfig, HealthManager};
use eggress_routing::upstream::{UpstreamGroup, UpstreamRuntime};
use eggress_routing::{
    RouteActionSpec, RouteService, Router, SharedRoutingService, UpstreamGroupId,
};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::Instrument;

use crate::error::RuntimeError;

pub struct RuntimeState {
    pub routing: Arc<SharedRoutingService>,
    pub metrics: Arc<eggress_metrics::MetricsRegistry>,
    pub readiness: Arc<AtomicBool>,
    pub start_time: Instant,
    pub generation: Arc<AtomicU64>,
    pub active_connections: Arc<AtomicU64>,
    pub connection_counter: Arc<AtomicU64>,
}

#[allow(dead_code)]
pub struct ServiceSupervisor {
    config_path: String,
    state: Arc<RuntimeState>,
    cancel: CancellationToken,
    health: Option<HealthManager>,
    admin_cancel: CancellationToken,
    tasks: TaskTracker,
    shutdown_grace: Duration,
}

impl ServiceSupervisor {
    pub fn start(config_path: &str) -> Result<Self, RuntimeError> {
        let rt_config = eggress_config::load_and_validate(config_path)
            .map_err(|e| RuntimeError::Config(e.to_string()))?;

        let metrics = Arc::new(eggress_metrics::MetricsRegistry::new());
        let generation = Arc::new(AtomicU64::new(0));
        let readiness = Arc::new(AtomicBool::new(false));

        let router = build_router_from_config(&rt_config)
            .map_err(|e| RuntimeError::Config(e.to_string()))?;
        let routing = Arc::new(SharedRoutingService::new(router));

        let active_connections = Arc::new(AtomicU64::new(0));
        let connection_counter = Arc::new(AtomicU64::new(1));

        let state = Arc::new(RuntimeState {
            routing: routing.clone(),
            metrics,
            readiness,
            start_time: Instant::now(),
            generation: generation.clone(),
            active_connections,
            connection_counter,
        });

        let cancel = CancellationToken::new();
        let admin_cancel = CancellationToken::new();

        let tasks = TaskTracker::new();

        let mut health: Option<HealthManager> = None;

        {
            let rt = &rt_config;
            let mut upstream_runtimes: Vec<Arc<UpstreamRuntime>> = Vec::new();

            for u in &rt.upstreams {
                let id = eggress_core::UpstreamId::new(u.id.clone());
                let mut runtime = UpstreamRuntime::new(id, u.chain.clone());

                if let Some(first_hop) = u.chain.hops.first() {
                    let addr: Result<std::net::SocketAddr, _> =
                        format!("{}:{}", first_hop.endpoint.host, first_hop.endpoint.port).parse();
                    if let Ok(addr) = addr {
                        runtime = runtime.with_health_probe(
                            eggress_routing::health::HealthProbe::TcpConnect {
                                target: addr,
                                timeout: std::time::Duration::from_secs(5),
                            },
                        );
                    }
                }

                upstream_runtimes.push(Arc::new(runtime));
            }

            if !upstream_runtimes.is_empty() {
                let mut hm = HealthManager::new(cancel.clone());
                hm.start_probes(&upstream_runtimes, &HealthConfig::default());
                health = Some(hm);
            }
        }

        let handshake_timeout = rt_config.timeouts.handshake;
        let connect_timeout = rt_config.timeouts.connect;
        let shutdown_grace = rt_config.process.shutdown_grace;

        for lcfg in &rt_config.listeners {
            let bind_addr: std::net::SocketAddr =
                lcfg.bind.parse().map_err(|e| RuntimeError::ListenerBind {
                    addr: lcfg.bind.clone(),
                    source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
                })?;

            let protocols: Vec<ProtocolId> = lcfg.protocols.to_vec();

            let auth = match &lcfg.auth {
                Some(auth_cfg) => {
                    if auth_cfg.auth_type == "password" {
                        let username = auth_cfg.username.clone().unwrap_or_default();
                        let password = auth_cfg.password.clone().unwrap_or_default();
                        eggress_server::accept::InboundAuthentication::UsernamePassword {
                            username,
                            password,
                        }
                    } else {
                        eggress_server::accept::InboundAuthentication::None
                    }
                }
                None => eggress_server::accept::InboundAuthentication::None,
            };

            let connection_limit = lcfg.connection_limit.unwrap_or(1024) as usize;

            let cancel = cancel.clone();
            let routing = routing.clone();
            let listener_name = lcfg.name.clone();
            let state = state.clone();

            tasks.spawn(async move {
                let config = TcpListenerConfig {
                    bind_addr,
                    protocols: protocols.clone(),
                    auth_required: false,
                    handshake_timeout,
                    connection_limit,
                };

                let listener = match TcpListener::new(&config, cancel.clone()).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(
                            "failed to bind listener '{listener_name}' on {bind_addr}: {e}"
                        );
                        return;
                    }
                };
                let local_addr = match listener.local_addr() {
                    Ok(a) => a,
                    Err(_) => return,
                };
                tracing::info!("listening on {local_addr} ({listener_name})");

                let proto_slice: Arc<[ProtocolId]> = config.protocols.clone().into();

                loop {
                    let conn = match listener.accept().await {
                        Ok(c) => c,
                        Err(e) => {
                            if e.to_string().contains("listener cancelled") {
                                break;
                            }
                            tracing::error!("accept error: {e}");
                            continue;
                        }
                    };

                    let routing = routing.clone();
                    let peer = conn.peer_addr;
                    let listener_str = local_addr.to_string();
                    let conn_id = state.connection_counter.fetch_add(1, Ordering::Relaxed);
                    let conn_protocols = proto_slice.clone();
                    let conn_auth = auth.clone();
                    let conn_metrics = state.metrics.clone();
                    let active = state.active_connections.clone();

                    active.fetch_add(1, Ordering::Relaxed);

                    let conn_cancel = cancel.child_token();

                    tokio::spawn(async move {
                        let started = std::time::Instant::now();
                        let config = eggress_server::ConnectionConfig {
                            routing: routing as Arc<dyn RouteService>,
                            context: eggress_server::ConnectionContext {
                                source: Some(peer),
                                listener: listener_str,
                            },
                            handshake_timeout,
                            connect_timeout,
                            protocols: conn_protocols,
                            authentication: conn_auth,
                            metrics: Some(conn_metrics),
                        };

                        let report = tokio::select! {
                            report = eggress_server::serve_connection(conn.stream, config)
                                .instrument(tracing::info_span!(
                                    "conn",
                                    id = conn_id,
                                    peer = %peer,
                                )) => {
                                report
                            }
                            _ = conn_cancel.cancelled() => {
                                eggress_server::SessionReport::cancelled(
                                    None,
                                    None,
                                    String::new(),
                                )
                            }
                        };

                        active.fetch_sub(1, Ordering::Relaxed);

                        tracing::info!(
                            protocol = ?report.protocol,
                            target = ?report.target,
                            route = %report.route,
                            outcome = ?report.outcome,
                            bytes_upstream = report.bytes_upstream,
                            bytes_downstream = report.bytes_downstream,
                            duration_ms = started.elapsed().as_millis() as u64,
                            "connection completed",
                        );
                    });
                }
            });
        }

        if let Some(ref admin_cfg) = rt_config.admin {
            if admin_cfg.enabled {
                let bind = admin_cfg.bind.clone();
                let admin_cancel = admin_cancel.clone();
                let state_ref = state.clone();
                tasks.spawn(async move {
                    let server = match eggress_admin::AdminServer::new(&bind, admin_cancel).await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("failed to bind admin on {bind}: {e}");
                            return;
                        }
                    };
                    let admin_state = eggress_admin::AdminState {
                        metrics: state_ref.metrics.clone(),
                        generation: state_ref.generation.clone(),
                        start_time: state_ref.start_time,
                        static_routes: Arc::new(vec![]),
                        pac_config: Arc::new(None),
                        router: Some(state_ref.routing.router()),
                        listeners: Arc::new(vec![]),
                        active_connections: Some(state_ref.active_connections.clone()),
                    };
                    if let Err(e) = server.run(admin_state).await {
                        tracing::error!("admin server error: {e}");
                    }
                });
            }
        }

        state.readiness.store(true, Ordering::Release);

        Ok(ServiceSupervisor {
            config_path: config_path.to_string(),
            state,
            cancel,
            health,
            admin_cancel,
            tasks,
            shutdown_grace,
        })
    }

    pub fn run(&mut self) {
        let config_path = self.config_path.clone();
        let routing = self.state.routing.clone();
        let admin_cancel = self.admin_cancel.clone();
        let cancel = self.cancel.clone();
        let metrics = self.state.metrics.clone();
        let readiness = self.state.readiness.clone();
        let active_connections = self.state.active_connections.clone();
        let shutdown_grace = self.shutdown_grace;
        let tasks = self.tasks.clone();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            #[cfg(unix)]
            {
                let mut sigterm =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("failed to register SIGTERM handler");
                let mut sighup =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
                        .expect("failed to register SIGHUP handler");

                loop {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            tracing::info!("shutdown signal received");
                            break;
                        }
                        _ = sigterm.recv() => {
                            tracing::info!("shutdown signal received");
                            break;
                        }
                        _ = sighup.recv() => {
                            tracing::info!("reload signal received, reloading config from {config_path}");
                            match eggress_config::compile::load_and_compile(&config_path) {
                                Ok(rt_config) => match build_router_from_config(&rt_config) {
                                    Ok(new_router) => {
                                        routing.swap(new_router);
                                        let gen = routing.generation();
                                        metrics.set_config_generation(gen);
                                        metrics.record_reload(true);
                                        tracing::info!(
                                            generation = gen,
                                            "config reloaded successfully"
                                        );
                                    }
                                    Err(e) => {
                                        metrics.record_reload(false);
                                        tracing::error!("reload failed (router build): {e}");
                                    }
                                },
                                Err(e) => {
                                    metrics.record_reload(false);
                                    tracing::error!("reload failed (config load): {e}");
                                }
                            }
                        }
                    }
                }
            }

            #[cfg(not(unix))]
            {
                tokio::signal::ctrl_c().await.ok();
                tracing::info!("shutdown signal received");
            }

            readiness.store(false, Ordering::Release);
            cancel.cancel();

            tracing::info!("draining active connections");

            let deadline = tokio::time::Instant::now() + shutdown_grace;
            loop {
                let active = active_connections.load(Ordering::Relaxed);
                if active == 0 {
                    tracing::info!("all connections drained");
                    break;
                }
                if tokio::time::Instant::now() >= deadline {
                    tracing::warn!(active, "drain timeout reached, forcing shutdown");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            admin_cancel.cancel();

            tasks.close();
            tasks.wait().await;
        });

        tracing::info!("eggress stopped");
    }
}

fn build_router_from_config(
    rt: &RuntimeConfig,
) -> Result<Router, Box<dyn std::error::Error + Send + Sync>> {
    let mut upstreams = Vec::new();

    for u in &rt.upstreams {
        let id = UpstreamGroupId(Arc::from(u.id.as_str()));
        let runtime =
            UpstreamRuntime::new(eggress_core::UpstreamId::new(u.id.clone()), u.chain.clone());
        upstreams.push((id, runtime));
    }

    let upstream_map: std::collections::HashMap<String, Arc<UpstreamRuntime>> = upstreams
        .into_iter()
        .map(|(id, runtime)| (id.0.to_string(), Arc::new(runtime)))
        .collect();

    let mut groups = Vec::new();

    for g in &rt.groups {
        let mut members = Vec::new();
        for m in &g.members {
            let member = upstream_map
                .get(m)
                .ok_or_else(|| format!("group '{}' references unknown upstream '{}'", g.id, m))?;
            members.push(member.clone());
        }
        if members.is_empty() {
            return Err(format!("group '{}' has no valid members", g.id).into());
        }

        let fallback = match g.fallback {
            GroupFallback::Reject => eggress_routing::upstream::GroupFallback::Reject,
            GroupFallback::Direct => eggress_routing::upstream::GroupFallback::Direct,
            GroupFallback::UseUnhealthy => eggress_routing::upstream::GroupFallback::UseUnhealthy,
        };

        groups.push((
            g.id.clone(),
            UpstreamGroup::new(g.id.clone(), g.scheduler, Arc::from(members), fallback),
        ));
    }

    let group_ids: std::collections::HashSet<_> = groups.iter().map(|(id, _)| id.clone()).collect();

    let mut rules = Vec::new();
    for r in &rt.rules {
        let action = match &r.action {
            RouteActionSpec::Direct => RouteActionSpec::Direct,
            RouteActionSpec::UpstreamGroup(gid) => {
                if !group_ids.contains(gid) {
                    return Err(
                        format!("rule '{}' references unknown group '{}'", r.id, gid).into(),
                    );
                }
                RouteActionSpec::UpstreamGroup(gid.clone())
            }
            RouteActionSpec::Reject(reason) => RouteActionSpec::Reject(reason.clone()),
        };
        rules.push(eggress_routing::CompiledRule {
            id: r.id.clone(),
            matcher: r.matcher.clone(),
            action,
        });
    }

    Ok(Router::with_groups(
        rules,
        rt.default_action.clone(),
        groups,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    use eggress_routing::scheduler::SchedulerKind;

    fn write_config(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn build_router_direct_only() {
        let rt_config = RuntimeConfig {
            process: eggress_config::compile::ProcessConfig::default(),
            timeouts: eggress_config::compile::TimeoutConfig::default(),
            listeners: vec![],
            upstreams: vec![],
            groups: vec![],
            rules: vec![],
            default_action: RouteActionSpec::Direct,
            admin: None,
        };
        let router = build_router_from_config(&rt_config).unwrap();
        assert!(router.rules().is_empty());
    }

    #[test]
    fn build_router_with_group_references_unknown_upstream() {
        let rt_config = RuntimeConfig {
            process: eggress_config::compile::ProcessConfig::default(),
            timeouts: eggress_config::compile::TimeoutConfig::default(),
            listeners: vec![],
            upstreams: vec![],
            groups: vec![eggress_config::compile::UpstreamGroupConfig {
                id: UpstreamGroupId(Arc::from("main")),
                scheduler: SchedulerKind::RoundRobin,
                members: vec!["nonexistent".to_string()],
                fallback: GroupFallback::Reject,
            }],
            rules: vec![],
            default_action: RouteActionSpec::Direct,
            admin: None,
        };
        let result = build_router_from_config(&rt_config);
        assert!(result.is_err(), "expected error, got Ok");
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("nonexistent"));
    }

    #[test]
    fn build_router_with_valid_group() {
        let rt_config = RuntimeConfig {
            process: eggress_config::compile::ProcessConfig::default(),
            timeouts: eggress_config::compile::TimeoutConfig::default(),
            listeners: vec![],
            upstreams: vec![eggress_config::compile::UpstreamConfig {
                id: "proxy1".to_string(),
                chain: eggress_uri::ProxyChainSpec { hops: vec![] },
            }],
            groups: vec![eggress_config::compile::UpstreamGroupConfig {
                id: UpstreamGroupId(Arc::from("main")),
                scheduler: SchedulerKind::RoundRobin,
                members: vec!["proxy1".to_string()],
                fallback: GroupFallback::Reject,
            }],
            rules: vec![],
            default_action: RouteActionSpec::Direct,
            admin: None,
        };
        let router = build_router_from_config(&rt_config).unwrap();
        assert!(router.rules().is_empty());
    }

    #[test]
    fn build_router_rule_references_unknown_group() {
        let rt_config = RuntimeConfig {
            process: eggress_config::compile::ProcessConfig::default(),
            timeouts: eggress_config::compile::TimeoutConfig::default(),
            listeners: vec![],
            upstreams: vec![],
            groups: vec![],
            rules: vec![eggress_routing::CompiledRule {
                id: eggress_routing::RuleId(Arc::from("r1")),
                matcher: eggress_routing::MatchExpr::Any,
                action: RouteActionSpec::UpstreamGroup(UpstreamGroupId(Arc::from("missing"))),
            }],
            default_action: RouteActionSpec::Direct,
            admin: None,
        };
        let result = build_router_from_config(&rt_config);
        assert!(result.is_err(), "expected error, got Ok");
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("missing"));
    }

    #[tokio::test]
    async fn load_config_start_supervisor() {
        let config = r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = ServiceSupervisor::start(path);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }
}
