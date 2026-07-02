use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use eggress_admin::{AdminSnapshot, AdminSnapshotProvider, ListenerInfo};
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::ProtocolId;
use eggress_routing::health::HealthManager;
use eggress_routing::upstream::UpstreamRuntime;
use eggress_routing::{RouteService, SharedRoutingService};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::Instrument;

use crate::error::RuntimeError;
use crate::platform::{check_capability, PlatformCapability};
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

/// Adapter that exposes the runtime's compiled snapshot to the admin server.
///
/// Implements `AdminSnapshotProvider` so that admin handlers see live data:
/// each request reads the current `ArcSwap<CompiledRuntimeSnapshot>` rather
/// than a startup-captured copy. Reloads take effect on the next request.
pub struct RuntimeAdminListenerInfos {
    state: Arc<RuntimeState>,
}

impl AdminSnapshotProvider for RuntimeAdminListenerInfos {
    fn snapshot(&self) -> AdminSnapshot {
        let snap = self.state.snapshot.load();
        let addrs = self
            .state
            .listener_addrs
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let listeners: Vec<ListenerInfo> = snap
            .listeners
            .iter()
            .enumerate()
            .map(|(idx, lcfg)| {
                let mode = if lcfg.transparent.as_ref().is_some_and(|t| t.enabled) {
                    Some("transparent".to_string())
                } else if lcfg.unix.is_some() {
                    Some("unix".to_string())
                } else {
                    Some("standard".to_string())
                };

                let capability_status = if lcfg.transparent.as_ref().is_some_and(|t| t.enabled) {
                    let cap = crate::platform::check_capability(
                        crate::platform::PlatformCapability::LinuxOriginalDstIpv4,
                    );
                    Some(cap.to_string())
                } else {
                    None
                };

                let original_dst_support = if lcfg.transparent.as_ref().is_some_and(|t| t.enabled) {
                    let cap = crate::platform::check_capability(
                        crate::platform::PlatformCapability::LinuxOriginalDstIpv4,
                    );
                    Some(cap == crate::platform::CapabilityStatus::Available)
                } else {
                    None
                };

                let (unix_socket_path, unix_socket_unlink_existing) =
                    if let Some(ref unix_cfg) = lcfg.unix {
                        (
                            Some(unix_cfg.path.display().to_string()),
                            Some(unix_cfg.unlink_existing),
                        )
                    } else {
                        (None, None)
                    };

                ListenerInfo {
                    name: lcfg.name.clone(),
                    bind: lcfg.bind.clone(),
                    local_addr: addrs.get(idx).map(|a| a.to_string()).unwrap_or_default(),
                    protocols: lcfg.protocols.iter().map(|p| p.to_string()).collect(),
                    udp_enabled: lcfg.udp.as_ref().is_some_and(|u| u.enabled),
                    mode,
                    capability_status,
                    original_dst_support,
                    unix_socket_path,
                    unix_socket_unlink_existing,
                }
            })
            .collect();
        AdminSnapshot {
            generation: snap.generation,
            router: snap.router.clone(),
            pac: snap.admin.as_ref().and_then(|a| a.pac.clone()),
            static_routes: snap
                .admin
                .as_ref()
                .map(|a| a.static_content.clone())
                .unwrap_or_default(),
            listeners,
        }
    }
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
///
/// **UDP-specific reload semantics:**
/// - UDP limits apply to new associations only; existing associations keep their limits.
/// - UDP bind changes are restart-required.
/// - UDP advertise address changes are restart-required if socket bind changes.
/// - Route changes apply immediately to future UDP packets.
///
/// Classify whether a reload is supported given old and new listener
/// configs. Returns `Ok(())` if the reload is safe, or `Err(reason)`
/// if it should be rejected.
fn classify_listeners(
    old_listeners: &[eggress_config::compile::ListenerConfig],
    new_listeners: &[eggress_config::compile::ListenerConfig],
) -> Result<(), String> {
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
        match (&old.udp, &new.udp) {
            (Some(old_udp), Some(new_udp)) => {
                if old_udp.bind != new_udp.bind {
                    return Err(format!(
                        "UDP bind address changed for '{}': '{}' -> '{}'; restart required",
                        old.name, old_udp.bind, new_udp.bind
                    ));
                }
            }
            (None, Some(new_udp)) => {
                if !new_udp.bind.ip().is_unspecified() {
                    return Err(format!(
                        "UDP bind address added for '{}': '{}'; restart required",
                        new.name, new_udp.bind
                    ));
                }
            }
            (Some(_old_udp), None) => {}
            (None, None) => {}
        }

        match (&old.transparent, &new.transparent) {
            (Some(old_t), Some(new_t)) => {
                if old_t.enabled != new_t.enabled {
                    return Err(format!(
                        "transparent config changed for '{}': enabled {} -> {}; restart required",
                        old.name, old_t.enabled, new_t.enabled
                    ));
                }
            }
            (None, Some(new_t)) => {
                if new_t.enabled {
                    return Err(format!(
                        "transparent proxy enabled for '{}'; restart required",
                        new.name
                    ));
                }
            }
            (Some(_old_t), None) => {}
            (None, None) => {}
        }

        match (&old.unix, &new.unix) {
            (Some(old_u), Some(new_u)) => {
                if old_u.path != new_u.path {
                    return Err(format!(
                        "unix socket path changed for '{}': '{}' -> '{}'; restart required",
                        old.name,
                        old_u.path.display(),
                        new_u.path.display()
                    ));
                }
            }
            (None, Some(_new_u)) => {
                return Err(format!(
                    "unix socket added for '{}'; restart required",
                    new.name
                ));
            }
            (Some(_old_u), None) => {
                return Err(format!(
                    "unix socket removed for '{}'; restart required",
                    old.name
                ));
            }
            (None, None) => {}
        }
    }

    Ok(())
}

struct PreparedListener {
    name: String,
    bind: String,
    protocols: Vec<ProtocolId>,
    listener: TcpListener,
    local_addr: std::net::SocketAddr,
    auth: eggress_server::accept::InboundAuthentication,
    handshake_timeout: Duration,
    udp: Option<eggress_config::compile::CompiledListenerUdpConfig>,
    tls: Option<eggress_config::compile::CompiledListenerTlsConfig>,
    shadowsocks: Option<eggress_config::model::ShadowsocksListenerConfig>,
}

type PreparedShadowsocksUdpRelay = (
    Arc<tokio::net::UdpSocket>,
    eggress_udp::standalone_shadowsocks::ShadowsocksStandaloneUdpConfig,
);

async fn prepare_shadowsocks_udp_relay(
    prepared_listener: &PreparedListener,
    udp_cfg: &eggress_config::compile::CompiledListenerUdpConfig,
    routing: Arc<dyn RouteService>,
    state: &RuntimeState,
) -> Result<PreparedShadowsocksUdpRelay, RuntimeError> {
    let ss = prepared_listener.shadowsocks.as_ref().ok_or_else(|| {
        RuntimeError::Other(format!(
            "listener '{}' shadowsocks_udp mode requires shadowsocks config",
            prepared_listener.name
        ))
    })?;
    let method =
        eggress_protocol_shadowsocks::CipherMethod::parse_method(&ss.method).map_err(|e| {
            RuntimeError::Other(format!(
                "listener '{}' has invalid shadowsocks method '{}': {}",
                prepared_listener.name, ss.method, e
            ))
        })?;
    let socket = Arc::new(
        tokio::net::UdpSocket::bind(udp_cfg.bind)
            .await
            .map_err(|e| RuntimeError::ListenerBind {
                addr: udp_cfg.bind.to_string(),
                source: e,
            })?,
    );
    let local_addr = socket
        .local_addr()
        .map_err(|e| RuntimeError::ListenerBind {
            addr: udp_cfg.bind.to_string(),
            source: e,
        })?;
    tracing::info!(
        "shadowsocks UDP relay listening on {local_addr} ({})",
        prepared_listener.name
    );

    let relay_config = eggress_udp::standalone_shadowsocks::ShadowsocksStandaloneUdpConfig {
        routing,
        udp_metrics: state.udp_metrics.clone(),
        shadowsocks_metrics: Some(state.shadowsocks_metrics.clone()),
        limits: eggress_udp::limits::UdpLimits::from_listener_config(
            udp_cfg.max_associations,
            udp_cfg.max_targets_per_association,
            udp_cfg.max_datagram_size,
            udp_cfg.idle_timeout,
            udp_cfg.client_pin,
            udp_cfg.target_idle_timeout,
        ),
        listener: prepared_listener.name.clone(),
        generation: state.snapshot.load().generation,
        method,
        password: ss.password.clone(),
    };

    Ok((socket, relay_config))
}

/// Compute the advertised IP for the SOCKS5 UDP ASSOCIATE reply.
///
/// Derivation rules:
/// 1. If `advertise` is configured, use it.
/// 2. Else if UDP bind IP is not unspecified, use UDP bind IP.
/// 3. Else if TCP peer is loopback, use loopback matching address family.
/// 4. Else return a config error requiring explicit `advertise`.
fn compute_advertise_ip(
    configured_advertise: Option<std::net::IpAddr>,
    udp_bind_ip: std::net::IpAddr,
    tcp_peer: std::net::SocketAddr,
) -> Result<std::net::IpAddr, eggress_udp::error::UdpError> {
    if let Some(ip) = configured_advertise {
        return Ok(ip);
    }

    if !udp_bind_ip.is_unspecified() {
        return Ok(udp_bind_ip);
    }

    if tcp_peer.ip().is_loopback() {
        match tcp_peer {
            std::net::SocketAddr::V4(_) => {
                return Ok(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
            }
            std::net::SocketAddr::V6(_) => {
                return Ok(std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST));
            }
        }
    }

    Err(eggress_udp::error::UdpError::Other(
        "UDP relay requires explicit advertise address when bind is unspecified and client is not loopback".to_string()
    ))
}

struct RuntimeUdpService {
    _listener_name: String,
    udp_config: eggress_config::compile::CompiledListenerUdpConfig,
    registry: Arc<eggress_udp::registry::UdpAssociationRegistry>,
    metrics: Arc<eggress_metrics::MetricsRegistry>,
    udp_metrics: Arc<eggress_udp::metrics::UdpMetrics>,
    routing: Arc<SharedRoutingService>,
    udp_tasks: TaskTracker,
}

impl eggress_server::UdpService for RuntimeUdpService {
    fn create_association(
        &self,
        listener: &str,
        client_tcp_peer: std::net::SocketAddr,
        identity: eggress_core::ClientIdentity,
        generation: u64,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        eggress_server::UdpAssociationHandle,
                        eggress_udp::error::UdpError,
                    >,
                > + Send
                + 'static,
        >,
    > {
        let registry = self.registry.clone();
        let metrics = self.metrics.clone();
        let udp_metrics = self.udp_metrics.clone();
        let routing = self.routing.clone();
        let udp_tasks = self.udp_tasks.clone();
        let udp_config = self.udp_config.clone();
        let listener = listener.to_string();
        Box::pin(async move {
            let assoc = registry
                .create_association(&listener, client_tcp_peer, identity, generation)
                .await?;
            metrics.record_udp_association_created();

            let relay_socket =
                std::sync::Arc::new(tokio::net::UdpSocket::bind(udp_config.bind).await?);
            let local_addr = relay_socket.local_addr()?;

            let advertised_ip =
                compute_advertise_ip(udp_config.advertise, local_addr.ip(), client_tcp_peer)?;
            let relay_addr = std::net::SocketAddr::new(advertised_ip, local_addr.port());

            let relay_config = eggress_udp::relay::RelayConfig {
                routing: routing as Arc<dyn RouteService>,
                udp_metrics: udp_metrics.clone(),
                limits: eggress_udp::limits::UdpLimits::from_listener_config(
                    udp_config.max_associations,
                    udp_config.max_targets_per_association,
                    udp_config.max_datagram_size,
                    udp_config.idle_timeout,
                    udp_config.client_pin,
                    udp_config.target_idle_timeout,
                ),
                listener: listener.clone(),
                generation,
                identity: assoc.meta.identity.clone(),
                client_tcp_peer,
                registry: registry.clone(),
            };

            let relay_assoc = assoc.clone();
            let relay_cancel = assoc.cancel.clone();
            let assoc_id = assoc.id;
            udp_tasks.spawn(async move {
                let result = eggress_udp::relay::udp_relay_loop(
                    relay_socket,
                    relay_assoc,
                    relay_config,
                    relay_cancel,
                )
                .await;
                if let Err(error) = result {
                    tracing::debug!(
                        %error,
                        association_id = ?assoc_id,
                        "UDP relay ended with error"
                    );
                }
            });

            Ok(eggress_server::UdpAssociationHandle {
                id: assoc.id,
                relay_addr,
                cancel: assoc.cancel.clone(),
            })
        })
    }

    fn is_enabled(&self) -> bool {
        self.udp_config.enabled
    }

    fn active_count(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = usize> + Send + 'static>> {
        let registry = self.registry.clone();
        Box::pin(async move { registry.active_count().await })
    }
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
    pub udp_registry: Arc<eggress_udp::registry::UdpAssociationRegistry>,
    pub udp_metrics: Arc<eggress_udp::metrics::UdpMetrics>,
    pub shadowsocks_metrics: Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>,
    pub udp_tasks: TaskTracker,
    pub transparent_accepted_total: Arc<AtomicU64>,
    pub transparent_original_dst_failed_total: Arc<AtomicU64>,
    pub reverse_registry: Arc<eggress_admin::ReverseRegistry>,
    pub reverse_metrics: Arc<eggress_protocol_reverse::metrics::ReverseMetrics>,
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
    tls_client_config: Option<std::sync::Arc<rustls::ClientConfig>>,
}

impl ServiceSupervisor {
    pub fn start(config_path: &str) -> Result<Self, RuntimeError> {
        let rt_config = eggress_config::load_and_validate(config_path)
            .map_err(|e| RuntimeError::Config(e.to_string()))?;

        for lcfg in &rt_config.listeners {
            if lcfg.unix.is_none() {
                let _bind_addr: std::net::SocketAddr =
                    lcfg.bind.parse().map_err(|e| RuntimeError::ListenerBind {
                        addr: lcfg.bind.clone(),
                        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
                    })?;
            }
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

        let udp_registry = Arc::new(eggress_udp::registry::UdpAssociationRegistry::new(
            eggress_udp::limits::UdpLimits::default(),
        ));

        let udp_metrics = Arc::new(eggress_udp::metrics::UdpMetrics::new());
        let shadowsocks_metrics = Arc::new(eggress_protocol_shadowsocks::ShadowsocksMetrics::new());
        let reverse_metrics = Arc::new(eggress_protocol_reverse::metrics::ReverseMetrics::new());
        let udp_tasks = TaskTracker::new();

        metrics.set_udp_metrics(udp_metrics.clone());
        metrics.set_shadowsocks_metrics(shadowsocks_metrics.clone());

        let state = Arc::new(RuntimeState {
            snapshot: snapshot.clone(),
            routing: routing.clone(),
            metrics: metrics.clone(),
            readiness,
            start_time: Instant::now(),
            active_connections,
            connection_counter,
            admin_local_addr: Arc::new(Mutex::new(None)),
            listener_addrs: Arc::new(Mutex::new(Vec::new())),
            udp_registry,
            udp_metrics,
            shadowsocks_metrics,
            udp_tasks: udp_tasks.clone(),
            transparent_accepted_total: Arc::new(AtomicU64::new(0)),
            transparent_original_dst_failed_total: Arc::new(AtomicU64::new(0)),
            reverse_registry: Arc::new(eggress_admin::ReverseRegistry::new()),
            reverse_metrics,
        });

        // Bridge transparent proxy atomics to MetricsRegistry for /metrics
        metrics.set_transparent_counters(
            state.transparent_accepted_total.clone(),
            state.transparent_original_dst_failed_total.clone(),
        );

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
            tls_client_config: None,
        })
    }

    pub fn state(&self) -> &Arc<RuntimeState> {
        &self.state
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Override the TLS client config used for upstream connections (e.g., Trojan).
    /// Intended for test-only use (e.g., insecure TLS for self-signed certs).
    #[allow(dead_code)]
    pub fn with_tls_client_config(mut self, config: std::sync::Arc<rustls::ClientConfig>) -> Self {
        self.tls_client_config = Some(config);
        self
    }

    /// Classify whether a reload is supported given old and new listener configs.
    /// Returns `Ok(())` if the reload is safe, or `Err(reason)` if it should be rejected.
    ///
    /// UDP-specific reload semantics:
    /// - UDP bind changes are restart-required.
    /// - UDP advertise address changes are restart-required if socket bind changes.
    /// - UDP limits apply to new associations only; existing ones keep their limits.
    /// - Route changes apply immediately to future UDP packets.
    fn classify_reload(
        &self,
        new_config: &eggress_config::compile::RuntimeConfig,
    ) -> Result<(), String> {
        classify_listeners(&self.rt_config.listeners, &new_config.listeners)
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
        let admin_state_ref = self.state.clone();
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
        let tls_client_config = self.tls_client_config.clone();

        let handshake_timeout = rt_config.timeouts.handshake;
        let connect_timeout = rt_config.timeouts.connect;

        let listener_infos_provider: Arc<RuntimeAdminListenerInfos> =
            Arc::new(RuntimeAdminListenerInfos {
                state: admin_state_ref.clone(),
            });

        let rt = tokio::runtime::Runtime::new()?;
        let result = rt.block_on(async move {
            // Start health probes inside the runtime context
            {
                let mut guard = health_for_run.lock().unwrap_or_else(|e| e.into_inner());
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
            let mut unix_listener_args = Vec::new();
            let mut transparent_listener_args = Vec::new();

            for lcfg in &listener_configs {
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

                // Handle Unix domain socket listeners separately
                if let Some(ref unix_cfg) = lcfg.unix {
                    #[cfg(unix)]
                    {
                        match eggress_server::listener::unix::create_unix_listener(
                            &eggress_server::listener::unix::UnixListenerConfig::from_compiled(
                                &unix_cfg.path,
                                unix_cfg.unlink_existing,
                                Some(unix_cfg.mode),
                            ),
                        ) {
                            Ok(unix_listener) => {
                                tracing::info!(
                                    "unix socket listener created at {} ({})",
                                    unix_cfg.path.display(),
                                    lcfg.name
                                );
                                unix_listener_args.push((
                                    lcfg.name.clone(),
                                    unix_listener,
                                    protocols,
                                    auth,
                                    handshake_timeout,
                                    lcfg.tls.clone(),
                                    lcfg.shadowsocks.clone(),
                                    lcfg.udp.clone(),
                                ));
                                continue;
                            }
                            Err(e) => {
                                tracing::error!(
                                    "failed to bind unix socket at {} for listener '{}': {e}",
                                    unix_cfg.path.display(),
                                    lcfg.name
                                );
                                continue;
                            }
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        tracing::error!(
                            "unix socket listener '{}' skipped: not supported on this platform",
                            lcfg.name
                        );
                        continue;
                    }
                }

                // Handle transparent TCP listeners
                if let Some(ref transparent_cfg) = lcfg.transparent {
                    if transparent_cfg.enabled {
                        let capability =
                            check_capability(PlatformCapability::LinuxOriginalDstIpv4);
                        if capability != crate::platform::CapabilityStatus::Available {
                            state_ref.metrics.record_platform_capability_check_failure();
                            let _cap_span = tracing::info_span!(
                                "capability_check_failed",
                                capability = %PlatformCapability::LinuxOriginalDstIpv4,
                                status = %capability,
                                listener = %lcfg.name,
                            );
                            tracing::warn!(
                                "transparent proxy not available for listener '{}' ({}); \
                                 falling back to normal TCP listener",
                                lcfg.name,
                                capability
                            );
                        } else {
                            let bind_addr: std::net::SocketAddr =
                                lcfg.bind.parse().map_err(|e| RuntimeError::ListenerBind {
                                    addr: lcfg.bind.clone(),
                                    source: std::io::Error::new(
                                        std::io::ErrorKind::InvalidInput,
                                        e,
                                    ),
                                })?;

                            let transparent_listener =
                                eggress_server::listener::transparent::TransparentListener::bind(
                                    &bind_addr.to_string(),
                                )
                                .await
                                .map_err(|e| RuntimeError::ListenerBind {
                                    addr: lcfg.bind.clone(),
                                    source: e,
                                })?;

                            let local_addr = transparent_listener.local_addr().map_err(|e| {
                                RuntimeError::ListenerBind {
                                    addr: lcfg.bind.clone(),
                                    source: e,
                                }
                            })?;

                            tracing::info!(
                                "transparent TCP listener listening on {local_addr} ({})",
                                lcfg.name
                            );

                            transparent_listener_args.push((
                                lcfg.name.clone(),
                                transparent_listener,
                                protocols,
                                auth,
                                handshake_timeout,
                                lcfg.tls.clone(),
                                lcfg.shadowsocks.clone(),
                                lcfg.udp.clone(),
                            ));
                            continue;
                        }
                    }
                }

                // Standard TCP listener path
                let bind_addr: std::net::SocketAddr =
                    lcfg.bind.parse().map_err(|e| RuntimeError::ListenerBind {
                        addr: lcfg.bind.clone(),
                        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
                    })?;

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
                    udp: lcfg.udp.clone(),
                    tls: lcfg.tls.clone(),
                    shadowsocks: lcfg.shadowsocks.clone(),
                });
            }

            let listener_infos: Vec<eggress_admin::ListenerInfo> = prepared
                .iter()
                .map(|p| eggress_admin::ListenerInfo {
                    name: p.name.clone(),
                    bind: p.bind.clone(),
                    local_addr: p.local_addr.to_string(),
                    protocols: p.protocols.iter().map(|p| p.to_string()).collect(),
                    udp_enabled: p.udp.as_ref().is_some_and(|u| u.enabled),
                    mode: Some("standard".to_string()),
                    capability_status: None,
                    original_dst_support: None,
                    unix_socket_path: None,
                    unix_socket_unlink_existing: None,
                })
                .collect();
            drop(listener_infos);

            // Store listener addresses for admin snapshot
            {
                let addrs: Vec<std::net::SocketAddr> =
                    prepared.iter().map(|p| p.local_addr).collect();
                *state_ref.listener_addrs.lock().unwrap_or_else(|e| e.into_inner()) = addrs;
            }

            let mut shadowsocks_udp_relays = Vec::new();

            for prepared_listener in &prepared {
                if let Some(ref udp_cfg) = prepared_listener.udp {
                    if udp_cfg.mode == eggress_udp::UdpMode::ShadowsocksUdp {
                        shadowsocks_udp_relays.push(
                            prepare_shadowsocks_udp_relay(
                                prepared_listener,
                                udp_cfg,
                                routing.clone(),
                                &state_ref,
                            )
                            .await?,
                        );
                    }
                }
            }

            for (socket, relay_config) in shadowsocks_udp_relays {
                let relay_cancel = cancel.clone();
                tasks.spawn(async move {
                    let result = eggress_udp::standalone_shadowsocks::shadowsocks_standalone_udp_relay(
                        socket,
                        relay_config,
                        relay_cancel,
                    )
                    .await;
                    if let Err(error) = result {
                        tracing::debug!(
                            %error,
                            "Shadowsocks UDP relay ended with error"
                        );
                    }
                });
            }

            // Spawn transparent listener accept loops
            for (
                listener_name,
                transparent_listener,
                protocols,
                auth,
                hs_timeout,
                tls_cfg,
                ss_cfg,
                udp_cfg,
            ) in transparent_listener_args
            {
                let routing = routing.clone();
                let state = state_ref.clone();
                let conn_tasks = connection_tasks.clone();
                let conn_cancel = connection_cancel.clone();
                let tls_client_config = tls_client_config.clone();
                let listener_cancel = listener_cancel.clone();

                tasks.spawn(async move {
                    let proto_slice: Arc<[ProtocolId]> = protocols.clone().into();
                    let transparent_listener_inner = transparent_listener.inner();

                    let transparent_accepted = state.transparent_accepted_total.clone();
                    let transparent_dst_failed =
                        state.transparent_original_dst_failed_total.clone();

                    loop {
                        let accept_result = tokio::select! {
                            result = transparent_listener_inner.accept() => result,
                            _ = listener_cancel.cancelled() => {
                                break;
                            }
                        };

                        let (stream, _peer) = match accept_result {
                            Ok(s) => s,
                            Err(e) => {
                                if e.to_string().contains("listener cancelled") {
                                    break;
                                }
                                tracing::error!(
                                    "transparent accept error on '{}': {e}",
                                    listener_name
                                );
                                continue;
                            }
                        };

                        transparent_accepted.fetch_add(1, Ordering::Relaxed);

                        let original_dst =
                            match eggress_server::listener::transparent::get_original_destination(&stream) {
                                Ok(addr) => addr,
                                Err(e) => {
                                    transparent_dst_failed.fetch_add(1, Ordering::Relaxed);
                                    let _span = tracing::info_span!(
                                        "transparent_original_dst_failed",
                                        listener = %listener_name,
                                        error = %e,
                                    );
                                    tracing::warn!(
                                        "failed to get original destination for transparent connection on '{}': {e}",
                                        listener_name
                                    );
                                    continue;
                                }
                            };

                        let peer = stream
                            .peer_addr()
                            .unwrap_or_else(|_| {
                                std::net::SocketAddr::new(
                                    std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                                    0,
                                )
                            });

                        let routing = routing.clone();
                        let tls_client_config = tls_client_config.clone();
                        let listener_str = listener_name.clone();
                        let conn_id = state
                            .connection_counter
                            .fetch_add(1, Ordering::Relaxed);
                        let conn_protocols = proto_slice.clone();
                        let conn_auth = auth.clone();
                        let conn_metrics = state.metrics.clone();
                        let conn_ss_metrics = state.shadowsocks_metrics.clone();
                        let active = state.active_connections.clone();
                        let conn_cancel = conn_cancel.child_token();
                        let generation = state.snapshot.load().generation;
                        let tls_config = tls_cfg.clone();
                        let ss_config = ss_cfg.clone();

                        let udp_svc = udp_cfg.as_ref().map(|udp_config| {
                            Arc::new(RuntimeUdpService {
                                _listener_name: listener_name.clone(),
                                udp_config: udp_config.clone(),
                                registry: state.udp_registry.clone(),
                                metrics: state.metrics.clone(),
                                udp_metrics: state.udp_metrics.clone(),
                                routing: routing.clone(),
                                udp_tasks: state.udp_tasks.clone(),
                            }) as Arc<dyn eggress_server::UdpService>
                        });

                        active.fetch_add(1, Ordering::Relaxed);

                        conn_tasks.spawn(async move {
                            let started = std::time::Instant::now();

                            let stream: eggress_core::BoxStream =
                                if let Some(ref tls_cfg) = tls_config {
                                    let server_config = match eggress_transport_tls::TlsServerConfigBuilder::new()
                                        .with_certificate_pem(&tls_cfg.cert_pem)
                                        .and_then(|b| b.with_key_pem(&tls_cfg.key_pem))
                                        .and_then(|b| {
                                            let b = if tls_cfg.alpn.is_empty() { b } else { b.with_alpn(tls_cfg.alpn.clone()) };
                                            b.build()
                                        }) {
                                            Ok(c) => c,
                                            Err(e) => {
                                                tracing::error!(%peer, "TLS config error: {e}");
                                                active.fetch_sub(1, Ordering::Relaxed);
                                                return;
                                            }
                                        };
                                    match eggress_transport_tls::tls_accept(Box::new(stream), server_config).await {
                                        Ok(s) => s,
                                        Err(e) => {
                                            tracing::debug!(%peer, "TLS accept failed: {e}");
                                            active.fetch_sub(1, Ordering::Relaxed);
                                            return;
                                        }
                                    }
                                } else {
                                    Box::new(stream)
                                };

                            let config = eggress_server::ConnectionConfig {
                                routing: routing as Arc<dyn RouteService>,
                                context: eggress_server::ConnectionContext {
                                    source: Some(peer),
                                    listener: listener_str.clone(),
                                    generation,
                                },
                                handshake_timeout: hs_timeout,
                                connect_timeout,
                                protocols: conn_protocols,
                                authentication: conn_auth,
                                metrics: Some(conn_metrics),
                                udp: udp_svc,
                                tls_client_config,
                                shadowsocks: ss_config.map(
                                    |ss| eggress_server::accept::InboundShadowsocksConfig {
                                        method: ss.method,
                                        password: ss.password,
                                    },
                                ),
                                shadowsocks_metrics: Some(conn_ss_metrics),
                            };

                        let report = tokio::select! {
                            report = eggress_server::serve_connection(stream, config)
                                .instrument(tracing::info_span!(
                                    "conn",
                                    id = conn_id,
                                    peer = %peer,
                                    original_dst = %original_dst,
                                    listener_type = "transparent",
                                    listener = %listener_str,
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
                                original_dst = %original_dst,
                                route = %report.route,
                                outcome = ?report.outcome,
                                bytes_upstream = report.bytes_upstream,
                                bytes_downstream = report.bytes_downstream,
                                duration_ms = started.elapsed().as_millis() as u64,
                                "transparent connection completed",
                            );
                        });
                    }
                });
            }

            // Spawn Unix domain socket accept loops
            for (
                listener_name,
                unix_listener,
                protocols,
                auth,
                hs_timeout,
                tls_cfg,
                ss_cfg,
                udp_cfg,
            ) in unix_listener_args
            {
                let routing = routing.clone();
                let state = state_ref.clone();
                let conn_tasks = connection_tasks.clone();
                let conn_cancel = connection_cancel.clone();
                let tls_client_config = tls_client_config.clone();
                let listener_cancel = listener_cancel.clone();

                let socket_path = unix_listener.path().display().to_string();
                let unix_metrics = state.metrics.clone();

                tasks.spawn(async move {
                    let proto_slice: Arc<[ProtocolId]> = protocols.clone().into();

                    let _accept_loop_span = tracing::info_span!(
                        "unix_accept_loop",
                        listener = %listener_name,
                        socket_path = %socket_path,
                    );

                    loop {
                        let (stream, _peer_addr) = tokio::select! {
                            result = unix_listener.accept() => match result {
                                Ok(r) => r,
                                Err(e) => {
                                    tracing::error!("unix accept error on '{}': {e}", listener_name);
                                    continue;
                                }
                            },
                            _ = listener_cancel.cancelled() => {
                                break;
                            }
                        };

                        unix_metrics.record_unix_listener_connection_accepted();

                        let routing = routing.clone();
                        let tls_client_config = tls_client_config.clone();
                        let listener_str = listener_name.clone();
                        let conn_id =
                            state.connection_counter.fetch_add(1, Ordering::Relaxed);
                        let conn_protocols = proto_slice.clone();
                        let conn_auth = auth.clone();
                        let conn_metrics = state.metrics.clone();
                        let conn_ss_metrics = state.shadowsocks_metrics.clone();
                        let active = state.active_connections.clone();
                        let conn_cancel = conn_cancel.child_token();
                        let generation = state.snapshot.load().generation;

                        let tls_config = tls_cfg.clone();
                        let ss_config = ss_cfg.clone();
                        let socket_path_clone = socket_path.clone();
                        let listener_str_for_span = listener_str.clone();

                        let udp_svc = udp_cfg.as_ref().map(|udp_config| {
                            Arc::new(RuntimeUdpService {
                                _listener_name: listener_name.clone(),
                                udp_config: udp_config.clone(),
                                registry: state.udp_registry.clone(),
                                metrics: state.metrics.clone(),
                                udp_metrics: state.udp_metrics.clone(),
                                routing: routing.clone(),
                                udp_tasks: state.udp_tasks.clone(),
                            }) as Arc<dyn eggress_server::UdpService>
                        });

                        active.fetch_add(1, Ordering::Relaxed);

                        conn_tasks.spawn(async move {
                            let started = std::time::Instant::now();

                            let stream: eggress_core::BoxStream =
                                if let Some(ref tls_cfg) = tls_config {
                                    let server_config = match eggress_transport_tls::TlsServerConfigBuilder::new()
                                        .with_certificate_pem(&tls_cfg.cert_pem)
                                        .and_then(|b| b.with_key_pem(&tls_cfg.key_pem))
                                        .and_then(|b| {
                                            let b = if tls_cfg.alpn.is_empty() { b } else { b.with_alpn(tls_cfg.alpn.clone()) };
                                            b.build()
                                        }) {
                                            Ok(c) => c,
                                            Err(e) => {
                                                tracing::error!("TLS config error for unix connection: {e}");
                                                active.fetch_sub(1, Ordering::Relaxed);
                                                return;
                                            }
                                        };
                                    match eggress_transport_tls::tls_accept(Box::new(stream), server_config).await {
                                        Ok(s) => s,
                                        Err(e) => {
                                            tracing::debug!("TLS accept failed for unix connection: {e}");
                                            active.fetch_sub(1, Ordering::Relaxed);
                                            return;
                                        }
                                    }
                                } else {
                                    Box::new(stream)
                                };

                            let peer = std::net::SocketAddr::new(
                                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                                0,
                            );

                            let config = eggress_server::ConnectionConfig {
                                routing: routing as Arc<dyn RouteService>,
                                context: eggress_server::ConnectionContext {
                                    source: Some(peer),
                                    listener: listener_str,
                                    generation,
                                },
                                handshake_timeout: hs_timeout,
                                connect_timeout,
                                protocols: conn_protocols,
                                authentication: conn_auth,
                                metrics: Some(conn_metrics),
                                udp: udp_svc,
                                tls_client_config,
                                shadowsocks: ss_config.map(
                                    |ss| eggress_server::accept::InboundShadowsocksConfig {
                                        method: ss.method,
                                        password: ss.password,
                                    },
                                ),
                                shadowsocks_metrics: Some(conn_ss_metrics),
                            };

                            let report = tokio::select! {
                                report = eggress_server::serve_connection(stream, config)
                                    .instrument(tracing::info_span!(
                                        "conn",
                                        id = conn_id,
                                        peer = %peer,
                                        listener_type = "unix",
                                        listener = %listener_str_for_span,
                                        socket_path = %socket_path_clone,
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
                                "unix connection completed",
                            );
                        });
                    }

                    // Cleanup socket file on shutdown
                    unix_listener.cleanup().unwrap_or_else(|e| {
                        tracing::warn!("failed to cleanup unix socket: {e}");
                    });
                });
            }

            // Spawn standard TCP accept loops
            for prepared_listener in prepared {
                let routing = routing.clone();
                let state = state_ref.clone();
                let conn_tasks = connection_tasks.clone();
                let conn_cancel = connection_cancel.clone();
                let tls_client_config = tls_client_config.clone();

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
                        let tls_client_config = tls_client_config.clone();
                        let peer = conn.peer_addr;
                        let listener_str = prepared_listener.name.clone();
                        let conn_id =
                            state.connection_counter.fetch_add(1, Ordering::Relaxed);
                        let conn_protocols = proto_slice.clone();
                        let conn_auth = prepared_listener.auth.clone();
                        let conn_metrics = state.metrics.clone();
                        let conn_ss_metrics = state.shadowsocks_metrics.clone();
                        let active = state.active_connections.clone();
                        let conn_cancel = conn_cancel.child_token();
                        let generation = state.snapshot.load().generation;

                        active.fetch_add(1, Ordering::Relaxed);

                        let tls_config = prepared_listener.tls.clone();
                        let ss_config = prepared_listener.shadowsocks.clone();

                        let udp_svc = if let Some(ref udp_config) = prepared_listener.udp {
                            Some(Arc::new(RuntimeUdpService {
                                _listener_name: prepared_listener.name.clone(),
                                udp_config: udp_config.clone(),
                                registry: state.udp_registry.clone(),
                                metrics: state.metrics.clone(),
                                udp_metrics: state.udp_metrics.clone(),
                                routing: routing.clone(),
                                udp_tasks: state.udp_tasks.clone(),
                            }) as Arc<dyn eggress_server::UdpService>)
                        } else {
                            None
                        };
                        conn_tasks.spawn(async move {
                            let started = std::time::Instant::now();

                            // Apply TLS if configured for this listener
                            let stream: eggress_core::BoxStream = if let Some(ref tls_cfg) = tls_config {
                                let server_config = match eggress_transport_tls::TlsServerConfigBuilder::new()
                                    .with_certificate_pem(&tls_cfg.cert_pem)
                                    .and_then(|b| b.with_key_pem(&tls_cfg.key_pem))
                                    .and_then(|b| {
                                        let b = if tls_cfg.alpn.is_empty() { b } else { b.with_alpn(tls_cfg.alpn.clone()) };
                                        b.build()
                                    }) {
                                        Ok(c) => c,
                                        Err(e) => {
                                            tracing::error!(%peer, "TLS config error: {e}");
                                            active.fetch_sub(1, Ordering::Relaxed);
                                            return;
                                        }
                                    };
                                match eggress_transport_tls::tls_accept(Box::new(conn.stream), server_config).await {
                                    Ok(s) => s,
                                    Err(e) => {
                                        tracing::debug!(%peer, "TLS accept failed: {e}");
                                        active.fetch_sub(1, Ordering::Relaxed);
                                        return;
                                    }
                                }
                            } else {
                                Box::new(conn.stream)
                            };

                            let config = eggress_server::ConnectionConfig {
                                routing: routing as Arc<dyn RouteService>,
                                context: eggress_server::ConnectionContext {
                                    source: Some(peer),
                                    listener: listener_str,
                                    generation,
                                },
                                handshake_timeout: prepared_listener.handshake_timeout,
                                connect_timeout,
                                protocols: conn_protocols,
                                authentication: conn_auth,
                                metrics: Some(conn_metrics),
                                udp: udp_svc,
                                tls_client_config: tls_client_config.clone(),
                                shadowsocks: ss_config.map(
                                    |ss| eggress_server::accept::InboundShadowsocksConfig {
                                        method: ss.method,
                                        password: ss.password,
                                    },
                                ),
                                shadowsocks_metrics: Some(conn_ss_metrics),
                            };

                            let report = tokio::select! {
                                report = eggress_server::serve_connection(stream, config)
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

            // Spawn reverse servers and clients
            {
                let current_snapshot = snapshot.load();
                let reverse_servers = current_snapshot.reverse_servers.clone();
                let reverse_clients = current_snapshot.reverse_clients.clone();
                drop(current_snapshot);

                for rs_cfg in reverse_servers {
                    let server_config = eggress_protocol_reverse::server::ReverseServerConfig {
                        control_bind: rs_cfg.control_bind,
                        external_bind: rs_cfg.external_bind,
                        auth_username: rs_cfg.auth_username,
                        auth_password: rs_cfg.auth_password,
                        max_control_connections: rs_cfg.max_control_connections,
                        read_timeout_ms: rs_cfg.read_timeout_ms,
                        allow_bind: rs_cfg.allow_bind,
                        max_listeners_per_client: rs_cfg.max_listeners_per_client,
                        max_streams_per_listener: rs_cfg.max_streams_per_listener,
                        max_pending_external: rs_cfg.max_pending_external,
                    };
                    // Defense-in-depth: validate the configuration before
                    // spawning the task so unsafe configurations fail at
                    // startup rather than silently at bind time.
                    if let Err(e) = server_config.validate() {
                        tracing::error!(
                            server_id = %rs_cfg.id,
                            error = %e,
                            "reverse server configuration validation failed; skipping",
                        );
                        continue;
                    }
                    let mut server = eggress_protocol_reverse::server::ReverseServer::new(server_config);
                    server.set_metrics(state_ref.reverse_metrics.clone());
                    let server_state = server.state_handle();
                    let server_cancel = server.cancel_token();

                    state_ref.reverse_registry.register(
                        eggress_admin::ReverseServerEntry {
                            id: eggress_admin::ReverseServerId::from(rs_cfg.id.as_str()),
                            control_bind: rs_cfg.control_bind.to_string(),
                            state: server_state,
                        },
                    );

                    let cancel_clone = cancel.clone();
                    tasks.spawn(async move {
                        let result = tokio::select! {
                            r = server.run() => r,
                            _ = cancel_clone.cancelled() => {
                                server_cancel.cancel();
                                Ok(())
                            }
                        };
                        if let Err(e) = result {
                            tracing::error!(error = %e, "reverse server error");
                        }
                    });
                }

                for rc_cfg in reverse_clients {
                    let host = rc_cfg
                        .default_target_host
                        .clone()
                        .unwrap_or_else(|| "127.0.0.1".to_string());
                    let port = rc_cfg.default_target_port.unwrap_or(0);

                    let parallel = rc_cfg.parallel_connections.max(1);
                    for conn_idx in 0..parallel {
                        let client_config = eggress_protocol_reverse::client::ReverseClientConfig {
                            server_addr: rc_cfg.server_addr,
                            auth_username: rc_cfg.auth_username.clone(),
                            auth_password: rc_cfg.auth_password.clone(),
                            reconnect_initial_ms: rc_cfg.reconnect_initial_ms,
                            reconnect_max_ms: rc_cfg.reconnect_max_ms,
                            default_target_host: rc_cfg.default_target_host.clone(),
                            default_target_port: rc_cfg.default_target_port,
                            read_timeout_ms: rc_cfg.read_timeout_ms,
                            drain_grace_ms: rc_cfg.drain_grace_ms,
                        };
                        let mut client = eggress_protocol_reverse::client::ReverseClient::new(client_config);
                        client.set_metrics(state_ref.reverse_metrics.clone());

                        let resolver = crate::reverse::RouteEngineTargetResolver::new(
                            routing.clone(),
                            host.clone(),
                            port,
                            std::sync::Arc::from(rc_cfg.id.as_str()),
                            Some(rc_cfg.server_addr),
                        );
                        client.set_resolver(std::sync::Arc::new(resolver));

                        let cancel_clone = cancel.clone();
                        let client_cancel = client.cancel_token();
                        let client_id = rc_cfg.id.clone();
                        let server_addr = rc_cfg.server_addr;

                        tasks.spawn(async move {
                            let result = tokio::select! {
                                r = client.run() => r,
                                _ = cancel_clone.cancelled() => {
                                    client_cancel.cancel();
                                    Ok(())
                                }
                            };
                            if let Err(e) = result {
                                tracing::error!(error = %e, client_id = %client_id, server = %server_addr, conn = conn_idx, "reverse client error");
                            }
                        });
                    }
                }
            }

            if let Some(ref admin_cfg) = admin_config {
                if admin_cfg.enabled {
                    let bind = admin_cfg.bind.clone();
                    let admin_cancel = admin_cancel.clone();
                    let state_ref = state_ref.clone();
                    let provider: Arc<dyn AdminSnapshotProvider> = listener_infos_provider.clone();
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
                            *state_ref.admin_local_addr.lock().unwrap_or_else(|e| e.into_inner()) = Some(addr);
                        }
                        let admin_state = eggress_admin::AdminState {
                            metrics: state_ref.metrics.clone(),
                            start_time: state_ref.start_time,
                            readiness: state_ref.readiness.clone(),
                            active_connections: Some(state_ref.active_connections.clone()),
                            provider,
                            udp_registry: state_ref.udp_registry.clone(),
                            reverse_registry: state_ref.reverse_registry.clone(),
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
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
                let mut sighup =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup());

                if let Err(ref e) = sigterm {
                    tracing::warn!("failed to register SIGTERM handler: {e}");
                }
                if let Err(ref e) = sighup {
                    tracing::warn!("failed to register SIGHUP handler: {e}");
                }

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
                        _ = async { sigterm.as_mut().ok()?.recv().await }, if sigterm.is_ok() => {
                            tracing::info!("shutdown signal received");
                            break;
                        }
                        _ = async { sighup.as_mut().ok()?.recv().await }, if sighup.is_ok() => {
                            tracing::info!("reload signal received, reloading config from {config_path}");
                            let prev_snapshot = snapshot.load();
                            let prev_ref: Option<&CompiledRuntimeSnapshot> = Some(&prev_snapshot);
                            let config_path_clone = config_path.clone();
                            let load_result = tokio::task::spawn_blocking(move || {
                                eggress_config::compile::load_and_compile(&config_path_clone)
                            }).await;
                            match load_result {
                                Ok(Ok(new_rt_config)) => {
                                    // Classify unsupported changes: reject if listener topology changed
                                    let old_listeners = &snapshot.load().listeners;
                                    if let Err(reason) = classify_listeners(old_listeners, &new_rt_config.listeners) {
                                        tracing::error!("reload rejected: {reason}");
                                        metrics.record_reload(false);
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
                                Ok(Err(e)) => {
                                    metrics.record_reload(false);
                                    tracing::error!("reload failed (config load): {e}");
                                }
                                Err(join_err) => {
                                    metrics.record_reload(false);
                                    tracing::error!("reload task panicked: {join_err}");
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

            // 1. Set readiness false (admin /-/ready will report 503 during drain)
            readiness.store(false, Ordering::Release);

            // 2. Stop listeners (no new connections accepted)
            listener_cancel.cancel();

            // 3. Stop health probes
            health_cancel.cancel();

            // 4. Close all UDP associations
            state_ref.udp_registry.close_all().await;

            // 5. Wait for UDP relay tasks to complete
            state_ref.udp_tasks.close();
            let _ = tokio::time::timeout(
                shutdown_grace,
                state_ref.udp_tasks.wait(),
            )
            .await;

            // 6. Wait for listener accept loops to exit so they cannot hand
            //    new connections to the connection tracker.
            tasks.close();
            tasks.wait().await;

            // 7. Drain active connections within the grace period; force-cancel
            //    afterwards. Admin stays up through this window so operators
            //    can observe drain progress via /-/ready, /-/status, /metrics.
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

            // 8. Wait for connection tasks (either drained naturally or force-cancelled)
            connection_tasks.close();
            connection_tasks.wait().await;

            // 9. Now that the proxy has fully stopped accepting and serving
            //    traffic, stop the admin server. /-/ready has been reporting
            //    503 since step 1.
            admin_cancel.cancel();
            admin_tasks.close();
            admin_tasks.wait().await;

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
            reverse_servers: vec![],
            reverse_clients: vec![],
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
            reverse_servers: vec![],
            reverse_clients: vec![],
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
            reverse_servers: vec![],
            reverse_clients: vec![],
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
            reverse_servers: vec![],
            reverse_clients: vec![],
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

    #[test]
    fn reload_rejects_transparent_enabled_change() {
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

[listeners.transparent]
enabled = true
"#;
        let f1 = write_config(config1);
        let f2 = write_config(config2);
        let path1 = f1.path().to_str().unwrap();
        let path2 = f2.path().to_str().unwrap();

        let sup = ServiceSupervisor::start(path1).unwrap();
        let new_config = eggress_config::compile::load_and_compile(path2).unwrap();
        let result = sup.classify_reload(&new_config);
        assert!(
            result.is_err(),
            "transparent enabled change should be rejected"
        );
        assert!(result.unwrap_err().contains("transparent"));
    }

    #[test]
    fn reload_rejects_unix_path_change() {
        let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[listeners.unix]
path = "/tmp/eggress.sock"
"#;
        let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[listeners.unix]
path = "/tmp/eggress-new.sock"
"#;
        let f1 = write_config(config1);
        let f2 = write_config(config2);
        let path1 = f1.path().to_str().unwrap();
        let path2 = f2.path().to_str().unwrap();

        let sup = ServiceSupervisor::start(path1).unwrap();
        let new_config = eggress_config::compile::load_and_compile(path2).unwrap();
        let result = sup.classify_reload(&new_config);
        assert!(result.is_err(), "unix path change should be rejected");
        assert!(result.unwrap_err().contains("unix socket path"));
    }

    #[test]
    fn compute_advertise_explicit() {
        let result = compute_advertise_ip(
            Some("10.0.0.1".parse().unwrap()),
            "0.0.0.0".parse().unwrap(),
            "127.0.0.1:5000".parse().unwrap(),
        );
        assert_eq!(
            result.unwrap(),
            std::net::IpAddr::V4("10.0.0.1".parse().unwrap())
        );
    }

    #[test]
    fn compute_advertise_bind_ip() {
        let result = compute_advertise_ip(
            None,
            "192.168.1.1".parse().unwrap(),
            "127.0.0.1:5000".parse().unwrap(),
        );
        assert_eq!(
            result.unwrap(),
            std::net::IpAddr::V4("192.168.1.1".parse().unwrap())
        );
    }

    #[test]
    fn compute_advertise_loopback_fallback() {
        let result = compute_advertise_ip(
            None,
            "0.0.0.0".parse().unwrap(),
            "127.0.0.1:5000".parse().unwrap(),
        );
        assert_eq!(
            result.unwrap(),
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        );
    }

    #[test]
    fn compute_advertise_unspecified_non_loopback_rejected() {
        let result = compute_advertise_ip(
            None,
            "0.0.0.0".parse().unwrap(),
            "192.168.1.10:5000".parse().unwrap(),
        );
        assert!(
            result.is_err(),
            "non-loopback with unspecified bind should fail"
        );
    }

    #[test]
    fn compute_advertise_ipv6_loopback() {
        let result =
            compute_advertise_ip(None, "::".parse().unwrap(), "[::1]:5000".parse().unwrap());
        assert_eq!(
            result.unwrap(),
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        );
    }

    #[test]
    fn compute_advertise_explicit_overrides_bind() {
        let result = compute_advertise_ip(
            Some("10.0.0.1".parse().unwrap()),
            "192.168.1.1".parse().unwrap(),
            "127.0.0.1:5000".parse().unwrap(),
        );
        assert_eq!(
            result.unwrap(),
            std::net::IpAddr::V4("10.0.0.1".parse().unwrap())
        );
    }
}
