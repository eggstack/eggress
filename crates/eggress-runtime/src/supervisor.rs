use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::ProtocolId;
use eggress_routing::health::HealthManager;
use eggress_routing::upstream::UpstreamRuntime;
use eggress_routing::{RouteService, SharedRoutingService};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::Instrument;

use crate::error::RuntimeError;
use crate::snapshot::{compile_runtime_snapshot, CompiledRuntimeSnapshot};

/// Result of a reload attempt.
#[derive(Debug)]
pub enum ReloadResult {
    /// Reload was applied successfully.
    Applied { generation: u64, upstreams: usize },
    /// Reload was rejected due to unsupported changes.
    Rejected { reason: String },
    /// Reload failed due to a compile or build error.
    Failed { error: String },
}

/// What is and isn't reloaded on SIGHUP:
///
/// **Reloaded (hot-swap, no downtime):**
/// - Upstream chains and health config (with Arc reuse for unchanged upstreams)
/// - Upstream groups, schedulers, and fallback policies
/// - Routing rules and default action
/// - Listener configuration metadata (name, bind, protocols, auth)
/// - Admin PAC and static content configuration
///
/// **NOT reloaded (requires full restart):**
/// - Listener socket bindings (bound before readiness)
/// - Process-level settings (log format, log level, shutdown grace)
/// - Timeout configuration
/// - Admin bind address
struct PreparedListener {
    name: String,
    bind: String,
    protocols: Vec<ProtocolId>,
    listener: TcpListener,
    local_addr: std::net::SocketAddr,
    auth: eggress_server::accept::InboundAuthentication,
    handshake_timeout: Duration,
}

pub struct RuntimeState {
    pub snapshot: Arc<ArcSwap<CompiledRuntimeSnapshot>>,
    pub routing: Arc<SharedRoutingService>,
    pub metrics: Arc<eggress_metrics::MetricsRegistry>,
    pub readiness: Arc<AtomicBool>,
    pub start_time: Instant,
    pub active_connections: Arc<AtomicU64>,
    pub connection_counter: Arc<AtomicU64>,
    pub admin_local_addr: Arc<Mutex<Option<std::net::SocketAddr>>>,
    pub listener_addrs: Arc<Mutex<Vec<std::net::SocketAddr>>>,
}

impl RuntimeState {
    pub fn generation(&self) -> u64 {
        self.snapshot.load().generation
    }
}

#[allow(dead_code)]
pub struct ServiceSupervisor {
    config_path: String,
    state: Arc<RuntimeState>,
    cancel: CancellationToken,
    listener_cancel: CancellationToken,
    connection_cancel: CancellationToken,
    health_cancel: CancellationToken,
    admin_cancel: CancellationToken,
    health: Option<HealthManager>,
    tasks: TaskTracker,
    connection_tasks: TaskTracker,
    admin_tasks: TaskTracker,
    shutdown_grace: Duration,
    rt_config: eggress_config::compile::RuntimeConfig,
}

impl ServiceSupervisor {
    pub fn start(config_path: &str) -> Result<Self, RuntimeError> {
        let rt_config = eggress_config::load_and_validate(config_path)
            .map_err(|e| RuntimeError::Config(e.to_string()))?;

        for lcfg in &rt_config.listeners {
            let _bind_addr: std::net::SocketAddr =
                lcfg.bind.parse().map_err(|e| RuntimeError::ListenerBind {
                    addr: lcfg.bind.clone(),
                    source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
                })?;
        }

        let metrics = Arc::new(eggress_metrics::MetricsRegistry::new());
        let readiness = Arc::new(AtomicBool::new(false));

        let snapshot = compile_runtime_snapshot(&rt_config, None)
            .map_err(|e| RuntimeError::Config(e.to_string()))?;
        let snapshot = Arc::new(ArcSwap::from_pointee(snapshot));

        let routing = Arc::new(SharedRoutingService::new_arc(
            snapshot.load().router.clone(),
        ));

        let active_connections = Arc::new(AtomicU64::new(0));
        let connection_counter = Arc::new(AtomicU64::new(1));

        let state = Arc::new(RuntimeState {
            snapshot: snapshot.clone(),
            routing: routing.clone(),
            metrics,
            readiness,
            start_time: Instant::now(),
            active_connections,
            connection_counter,
            admin_local_addr: Arc::new(Mutex::new(None)),
            listener_addrs: Arc::new(Mutex::new(Vec::new())),
        });

        let cancel = CancellationToken::new();
        let listener_cancel = CancellationToken::new();
        let connection_cancel = CancellationToken::new();
        let health_cancel = CancellationToken::new();
        let admin_cancel = CancellationToken::new();

        let tasks = TaskTracker::new();
        let connection_tasks = TaskTracker::new();

        let mut health: Option<HealthManager> = None;

        {
            let upstream_runtimes: Vec<Arc<UpstreamRuntime>> =
                snapshot.load().upstreams.values().cloned().collect();

            if !upstream_runtimes.is_empty() {
                let hm = HealthManager::new(health_cancel.clone());
                health = Some(hm);
            }
        }

        let shutdown_grace = rt_config.process.shutdown_grace;

        Ok(ServiceSupervisor {
            config_path: config_path.to_string(),
            state,
            cancel,
            listener_cancel,
            connection_cancel,
            health_cancel,
            admin_cancel,
            health,
            tasks,
            connection_tasks,
            admin_tasks: TaskTracker::new(),
            shutdown_grace,
            rt_config,
        })
    }

    pub fn state(&self) -> &Arc<RuntimeState> {
        &self.state
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Classify whether a reload is supported given old and new listener configs.
    /// Returns `Ok(())` if the reload is safe, or `Err(reason)` if it should be rejected.
    fn classify_reload(
        &self,
        new_config: &eggress_config::compile::RuntimeConfig,
    ) -> Result<(), String> {
        let old_listeners = &self.rt_config.listeners;
        let new_listeners = &new_config.listeners;

        if old_listeners.len() != new_listeners.len() {
            return Err(format!(
                "listener count changed ({} -> {}); restart required",
                old_listeners.len(),
                new_listeners.len()
            ));
        }

        for (old, new) in old_listeners.iter().zip(new_listeners.iter()) {
            if old.name != new.name {
                return Err(format!(
                    "listener name changed ('{}' -> '{}'); restart required",
                    old.name, new.name
                ));
            }
            if old.bind != new.bind {
                return Err(format!(
                    "listener bind address changed for '{}': '{}' -> '{}'; restart required",
                    old.name, old.bind, new.bind
                ));
            }
        }

        Ok(())
    }

    /// Attempt to reload configuration. Encapsulates the full reload transaction:
    /// 1. Load and compile new config
    /// 2. Classify unsupported changes (reject if listener topology changed)
    /// 3. Build new snapshot with previous snapshot for Arc reuse
    /// 4. Atomically swap routing and snapshot
    /// 5. Update stored rt_config for subsequent reloads
    pub fn reload_config(&mut self) -> ReloadResult {
        let config_path = self.config_path.clone();
        let new_rt_config = match eggress_config::compile::load_and_compile(&config_path) {
            Ok(c) => c,
            Err(e) => {
                return ReloadResult::Failed {
                    error: format!("config load: {e}"),
                };
            }
        };

        if let Err(reason) = self.classify_reload(&new_rt_config) {
            return ReloadResult::Rejected { reason };
        }

        let prev_snapshot = self.state.snapshot.load();
        let prev_ref: Option<&CompiledRuntimeSnapshot> = Some(&prev_snapshot);
        let new_snapshot = match compile_runtime_snapshot(&new_rt_config, prev_ref) {
            Ok(s) => s,
            Err(e) => {
                return ReloadResult::Failed {
                    error: format!("snapshot build: {e}"),
                };
            }
        };

        let upstream_count = new_snapshot.upstreams.len();
        let gen = new_snapshot.generation;

        self.state.routing.swap_arc(new_snapshot.router.clone());
        self.state.snapshot.store(Arc::new(new_snapshot));

        self.rt_config = new_rt_config;

        ReloadResult::Applied {
            generation: gen,
            upstreams: upstream_count,
        }
    }

    pub fn run(&mut self) -> Result<(), RuntimeError> {
        let config_path = self.config_path.clone();
        let routing = self.state.routing.clone();
        let listener_cancel = self.listener_cancel.clone();
        let connection_cancel = self.connection_cancel.clone();
        let health_cancel = self.health_cancel.clone();
        let admin_cancel = self.admin_cancel.clone();
        let cancel = self.cancel.clone();
        let metrics = self.state.metrics.clone();
        let readiness = self.state.readiness.clone();
        let active_connections = self.state.active_connections.clone();
        let shutdown_grace = self.shutdown_grace;
        let tasks = self.tasks.clone();
        let connection_tasks = self.connection_tasks.clone();
        let admin_tasks = self.admin_tasks.clone();
        let health = std::sync::Arc::new(std::sync::Mutex::new(self.health.take()));
        let health_clone = health.clone();
        let health_for_run = health.clone();
        let snapshot = self.state.snapshot.clone();
        let state_ref = self.state.clone();
        let rt_config = self.rt_config.clone();

        let handshake_timeout = rt_config.timeouts.handshake;
        let connect_timeout = rt_config.timeouts.connect;

        let rt = tokio::runtime::Runtime::new()?;
        let result = rt.block_on(async move {
            // Start health probes inside the runtime context
            {
                let mut guard = health_for_run.lock().unwrap();
                if let Some(ref mut hm) = *guard {
                    let upstream_runtimes: Vec<Arc<UpstreamRuntime>> =
                        snapshot.load().upstreams.values().cloned().collect();
                    if !upstream_runtimes.is_empty() {
                        hm.start_probes(&upstream_runtimes);
                    }
                }
            }

            let current_snapshot = snapshot.load();
            let listener_configs = current_snapshot.listeners.clone();
            let admin_config = current_snapshot.admin.clone();
            drop(current_snapshot);

            if listener_configs.is_empty() {
                tracing::warn!("no listeners configured; the proxy will not accept connections");
            }

            let mut prepared = Vec::new();
            for lcfg in &listener_configs {
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

                let config = TcpListenerConfig {
                    bind_addr,
                    protocols: protocols.clone(),
                    auth_required: false,
                    handshake_timeout,
                    connection_limit,
                };

                let listener = TcpListener::new(&config, listener_cancel.clone())
                    .await
                    .map_err(|e| RuntimeError::ListenerBind {
                        addr: lcfg.bind.clone(),
                        source: e,
                    })?;
                let local_addr = listener.local_addr().map_err(|e| {
                    RuntimeError::ListenerBind {
                        addr: lcfg.bind.clone(),
                        source: e,
                    }
                })?;
                tracing::info!("listening on {local_addr} ({})", lcfg.name);

                prepared.push(PreparedListener {
                    name: lcfg.name.clone(),
                    bind: lcfg.bind.clone(),
                    protocols,
                    listener,
                    local_addr,
                    auth,
                    handshake_timeout,
                });
            }

            let listener_infos: Vec<eggress_admin::ListenerInfo> = prepared
                .iter()
                .map(|p| eggress_admin::ListenerInfo {
                    name: p.name.clone(),
                    bind: p.bind.clone(),
                    local_addr: p.local_addr.to_string(),
                    protocols: p.protocols.iter().map(|p| p.to_string()).collect(),
                })
                .collect();

            // Store listener addresses for test discovery
            {
                let addrs: Vec<std::net::SocketAddr> =
                    prepared.iter().map(|p| p.local_addr).collect();
                *state_ref.listener_addrs.lock().unwrap() = addrs;
            }

            for prepared_listener in prepared {
                let routing = routing.clone();
                let state = state_ref.clone();
                let conn_tasks = connection_tasks.clone();
                let conn_cancel = connection_cancel.clone();

                tasks.spawn(async move {
                    let proto_slice: Arc<[ProtocolId]> =
                        prepared_listener.protocols.clone().into();

                    loop {
                        let conn = match prepared_listener.listener.accept().await {
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
                        let listener_str = prepared_listener.name.clone();
                        let conn_id =
                            state.connection_counter.fetch_add(1, Ordering::Relaxed);
                        let conn_protocols = proto_slice.clone();
                        let conn_auth = prepared_listener.auth.clone();
                        let conn_metrics = state.metrics.clone();
                        let active = state.active_connections.clone();
                        let conn_cancel = conn_cancel.child_token();

                        active.fetch_add(1, Ordering::Relaxed);

                        conn_tasks.spawn(async move {
                            let started = std::time::Instant::now();
                            let config = eggress_server::ConnectionConfig {
                                routing: routing as Arc<dyn RouteService>,
                                context: eggress_server::ConnectionContext {
                                    source: Some(peer),
                                    listener: listener_str,
                                },
                                handshake_timeout: prepared_listener.handshake_timeout,
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

            if let Some(ref admin_cfg) = admin_config {
                if admin_cfg.enabled {
                    let bind = admin_cfg.bind.clone();
                    let admin_cancel = admin_cancel.clone();
                    let state_ref = state_ref.clone();
                    let listener_infos = listener_infos.clone();
                    let pac_config = Arc::new(admin_cfg.pac.clone());
                    let static_routes = Arc::new(admin_cfg.static_content.clone());
                    admin_tasks.spawn(async move {
                        let server =
                            match eggress_admin::AdminServer::new(&bind, admin_cancel).await {
                                Ok(s) => s,
                                Err(e) => {
                                    tracing::error!("failed to bind admin on {bind}: {e}");
                                    return;
                                }
                            };
                        if let Ok(addr) = server.local_addr() {
                            *state_ref.admin_local_addr.lock().unwrap() = Some(addr);
                        }
                        let admin_state = eggress_admin::AdminState {
                            metrics: state_ref.metrics.clone(),
                            generation: Arc::new(AtomicU64::new(state_ref.generation())),
                            start_time: state_ref.start_time,
                            static_routes,
                            pac_config,
                            router: Some(state_ref.routing.router()),
                            routing: Some(state_ref.routing.clone()),
                            listeners: Arc::new(listener_infos),
                            active_connections: Some(state_ref.active_connections.clone()),
                            readiness: state_ref.readiness.clone(),
                        };
                        if let Err(e) = server.run(admin_state).await {
                            tracing::error!("admin server error: {e}");
                        }
                    });
                }
            }

            readiness.store(true, Ordering::Release);

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
                        _ = cancel.cancelled() => {
                            tracing::info!("shutdown requested via cancel token");
                            break;
                        }
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
                            // Reload is performed inside the async block via a mutable reference
                            // captured from the outer scope. We use a helper that mirrors
                            // reload_config but operates on the async-captured state.
                            let prev_snapshot = snapshot.load();
                            let prev_ref: Option<&CompiledRuntimeSnapshot> = Some(&prev_snapshot);
                            match eggress_config::compile::load_and_compile(&config_path) {
                                Ok(new_rt_config) => {
                                    // Classify unsupported changes: reject if listener topology changed
                                    let old_listeners = &snapshot.load().listeners;
                                    let new_listeners = &new_rt_config.listeners;
                                    let mut rejected = false;
                                    if old_listeners.len() != new_listeners.len() {
                                        tracing::error!(
                                            "reload rejected: listener count changed ({} -> {}); restart required",
                                            old_listeners.len(),
                                            new_listeners.len()
                                        );
                                        metrics.record_reload(false);
                                        rejected = true;
                                    } else {
                                        for (old, new) in old_listeners.iter().zip(new_listeners.iter()) {
                                            if old.name != new.name {
                                                tracing::error!(
                                                    "reload rejected: listener name changed ('{}' -> '{}'); restart required",
                                                    old.name, new.name
                                                );
                                                metrics.record_reload(false);
                                                rejected = true;
                                                break;
                                            }
                                            if old.bind != new.bind {
                                                tracing::error!(
                                                    "reload rejected: listener bind changed for '{}': '{}' -> '{}'; restart required",
                                                    old.name, old.bind, new.bind
                                                );
                                                metrics.record_reload(false);
                                                rejected = true;
                                                break;
                                            }
                                        }
                                    }
                                    if rejected {
                                        continue;
                                    }
                                    match compile_runtime_snapshot(&new_rt_config, prev_ref) {
                                        Ok(new_snapshot) => {
                                            let upstream_count = new_snapshot.upstreams.len();
                                            let gen = new_snapshot.generation;

                                            routing.swap_arc(new_snapshot.router.clone());
                                            snapshot.store(Arc::new(new_snapshot));

                                            metrics.set_config_generation(gen);
                                            metrics.record_reload(true);

                                            if let Ok(mut guard) = health_clone.lock() {
                                                if let Some(ref mut hm) = *guard {
                                                    hm.stop_all();
                                                }
                                                let upstream_runtimes: Vec<Arc<UpstreamRuntime>> = snapshot
                                                    .load()
                                                    .upstreams
                                                    .values()
                                                    .cloned()
                                                    .collect();
                                                if !upstream_runtimes.is_empty() {
                                                    let mut hm = HealthManager::new(health_cancel.clone());
                                                    hm.start_probes(&upstream_runtimes);
                                                    *guard = Some(hm);
                                                } else {
                                                    *guard = None;
                                                }
                                            }

                                            tracing::info!(
                                                generation = gen,
                                                upstreams = upstream_count,
                                                "config reloaded successfully"
                                            );
                                        }
                                        Err(e) => {
                                            metrics.record_reload(false);
                                            tracing::error!("reload failed (snapshot build): {e}");
                                        }
                                    }
                                }
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

            // 1. Set readiness false
            readiness.store(false, Ordering::Release);

            // 2. Stop listeners
            listener_cancel.cancel();

            // 3. Stop health
            health_cancel.cancel();

            // 4. Stop admin
            admin_cancel.cancel();

            // 5. Wait for listener and admin tasks to finish
            tasks.close();
            admin_tasks.close();
            tasks.wait().await;
            admin_tasks.wait().await;

            // 6. Wait for connections to drain
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
                    connection_cancel.cancel();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            // 7. Wait for connection tasks
            connection_tasks.close();
            connection_tasks.wait().await;

            Ok::<_, RuntimeError>(())
        });

        self.health = std::sync::Arc::try_unwrap(health)
            .ok()
            .and_then(|m| m.into_inner().ok())
            .flatten();

        match &result {
            Ok(()) => tracing::info!("eggress stopped"),
            Err(e) => tracing::error!(error = %e, "eggress stopped with error"),
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    use crate::snapshot::compile_runtime_snapshot;
    use eggress_config::compile::{GroupFallback, ProcessConfig, RuntimeConfig, TimeoutConfig};
    use eggress_routing::scheduler::SchedulerKind;
    use eggress_routing::{MatchExpr, RouteActionSpec, RuleId, UpstreamGroupId};

    fn write_config(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn build_router_direct_only() {
        let rt_config = RuntimeConfig {
            process: ProcessConfig::default(),
            timeouts: TimeoutConfig::default(),
            listeners: vec![],
            upstreams: vec![],
            groups: vec![],
            rules: vec![],
            default_action: RouteActionSpec::Direct,
            admin: None,
        };
        let snap = compile_runtime_snapshot(&rt_config, None).unwrap();
        assert!(snap.router.rules().is_empty());
    }

    #[test]
    fn build_router_with_group_references_unknown_upstream() {
        let rt_config = RuntimeConfig {
            process: ProcessConfig::default(),
            timeouts: TimeoutConfig::default(),
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
        let result = compile_runtime_snapshot(&rt_config, None);
        assert!(result.is_err(), "expected error, got Ok");
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("nonexistent"));
    }

    #[test]
    fn build_router_with_valid_group() {
        let rt_config = RuntimeConfig {
            process: ProcessConfig::default(),
            timeouts: TimeoutConfig::default(),
            listeners: vec![],
            upstreams: vec![eggress_config::compile::UpstreamConfig {
                id: "proxy1".to_string(),
                chain: eggress_uri::ProxyChainSpec { hops: vec![] },
                health: eggress_routing::health::HealthConfig::default(),
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
        let snap = compile_runtime_snapshot(&rt_config, None).unwrap();
        assert!(snap.router.rules().is_empty());
    }

    #[test]
    fn build_router_rule_references_unknown_group() {
        let rt_config = RuntimeConfig {
            process: ProcessConfig::default(),
            timeouts: TimeoutConfig::default(),
            listeners: vec![],
            upstreams: vec![],
            groups: vec![],
            rules: vec![eggress_routing::CompiledRule {
                id: RuleId(Arc::from("r1")),
                matcher: MatchExpr::Any,
                action: RouteActionSpec::UpstreamGroup(UpstreamGroupId(Arc::from("missing"))),
            }],
            default_action: RouteActionSpec::Direct,
            admin: None,
        };
        let result = compile_runtime_snapshot(&rt_config, None);
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

    #[test]
    fn active_connections_counter_increments_and_decrements() {
        let active = Arc::new(AtomicU64::new(0));
        assert_eq!(active.load(Ordering::Relaxed), 0);
        active.fetch_add(1, Ordering::Relaxed);
        assert_eq!(active.load(Ordering::Relaxed), 1);
        active.fetch_add(1, Ordering::Relaxed);
        assert_eq!(active.load(Ordering::Relaxed), 2);
        active.fetch_sub(1, Ordering::Relaxed);
        assert_eq!(active.load(Ordering::Relaxed), 1);
        active.fetch_sub(1, Ordering::Relaxed);
        assert_eq!(active.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn readiness_flag_controls_ready_endpoint() {
        let readiness = Arc::new(AtomicBool::new(true));
        assert!(readiness.load(Ordering::Relaxed));
        readiness.store(false, Ordering::Relaxed);
        assert!(!readiness.load(Ordering::Relaxed));
        readiness.store(true, Ordering::Relaxed);
        assert!(readiness.load(Ordering::Relaxed));
    }

    #[test]
    fn reload_rejects_listener_name_change() {
        let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]
"#;
        let config2 = r#"
version = 1

[[listeners]]
name = "http-changed"
bind = "127.0.0.1:8080"
protocols = ["http"]
"#;
        let f1 = write_config(config1);
        let f2 = write_config(config2);
        let path1 = f1.path().to_str().unwrap();
        let path2 = f2.path().to_str().unwrap();

        let sup = ServiceSupervisor::start(path1).unwrap();
        let new_config = eggress_config::compile::load_and_compile(path2).unwrap();
        let result = sup.classify_reload(&new_config);
        assert!(result.is_err(), "listener name change should be rejected");
        assert!(result.unwrap_err().contains("name changed"));
    }

    #[test]
    fn reload_rejects_listener_bind_change() {
        let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]
"#;
        let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:9090"
protocols = ["http"]
"#;
        let f1 = write_config(config1);
        let f2 = write_config(config2);
        let path1 = f1.path().to_str().unwrap();
        let path2 = f2.path().to_str().unwrap();

        let sup = ServiceSupervisor::start(path1).unwrap();
        let new_config = eggress_config::compile::load_and_compile(path2).unwrap();
        let result = sup.classify_reload(&new_config);
        assert!(result.is_err(), "listener bind change should be rejected");
        assert!(result.unwrap_err().contains("bind"));
    }

    #[test]
    fn reload_accepts_unchanged_listeners() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();

        let sup = ServiceSupervisor::start(path).unwrap();
        let new_config = eggress_config::compile::load_and_compile(path).unwrap();
        let result = sup.classify_reload(&new_config);
        assert!(result.is_ok(), "unchanged listeners should be accepted");
    }

    #[test]
    fn reload_rejects_listener_count_change() {
        let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]
"#;
        let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:1080"
protocols = ["socks5"]
"#;
        let f1 = write_config(config1);
        let f2 = write_config(config2);
        let path1 = f1.path().to_str().unwrap();
        let path2 = f2.path().to_str().unwrap();

        let sup = ServiceSupervisor::start(path1).unwrap();
        let new_config = eggress_config::compile::load_and_compile(path2).unwrap();
        let result = sup.classify_reload(&new_config);
        assert!(result.is_err(), "listener count change should be rejected");
        assert!(result.unwrap_err().contains("listener count"));
    }
}
