use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(feature = "operations")]
use eggress_admin::AdminSnapshotProvider;
use eggress_core::listener::{is_listener_cancelled, TcpListener, TcpListenerConfig};
use eggress_core::ProtocolId;
use eggress_routing::health::HealthManager;
use eggress_routing::upstream::UpstreamRuntime;
use eggress_routing::RouteService;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::Instrument;

use crate::error::RuntimeError;
use crate::platform::{check_capability, PlatformCapability};

pub(crate) mod accounting;
pub(crate) mod connection;
#[cfg(feature = "operations")]
pub(crate) mod operations;
pub(crate) mod reload;
pub(crate) mod shutdown;
pub(crate) mod startup;
pub(crate) mod state;
pub(crate) mod udp_runtime;

pub(crate) use accounting::{handle_accept_error, ActiveConnectionGuard, ListenerConnectionSlot};
pub(crate) use connection::PreparedListener;
#[cfg(feature = "quic")]
pub(crate) use connection::PreparedQuicListener;
pub(crate) use connection::{
    build_connection_config, wrap_tls_server, ConnectionBuildParams, InboundSecurity,
};
#[cfg(feature = "operations")]
pub(crate) use operations::RuntimeAdminListenerInfos;
pub use reload::{classify_reload_config, ReloadResult};
pub(crate) use shutdown::{shutdown_ordered, ShutdownPlan};
pub use state::RuntimeState;
#[allow(unused_imports)]
pub(crate) use udp_runtime::compute_advertise_ip;
pub(crate) use udp_runtime::make_udp_service;
#[cfg(feature = "extended")]
pub(crate) use udp_runtime::prepare_shadowsocks_udp_relay;

#[allow(dead_code)]
pub struct ServiceSupervisor {
    pub(crate) config_path: Option<String>,
    pub(crate) state: Arc<RuntimeState>,
    pub(crate) metrics_registry: Arc<eggress_metrics::MetricsRegistry>,
    pub(crate) cancel: CancellationToken,
    pub(crate) listener_cancel: CancellationToken,
    pub(crate) connection_cancel: CancellationToken,
    pub(crate) health_cancel: CancellationToken,
    pub(crate) admin_cancel: CancellationToken,
    pub(crate) health: Arc<Mutex<Option<HealthManager>>>,
    pub(crate) tasks: TaskTracker,
    pub(crate) connection_tasks: TaskTracker,
    pub(crate) admin_tasks: TaskTracker,
    pub(crate) shutdown_grace: Duration,
    pub(crate) rt_config: eggress_config::compile::RuntimeConfig,
    pub(crate) tls_client_config: Option<std::sync::Arc<rustls::ClientConfig>>,
    #[cfg(feature = "ssh")]
    pub(crate) ssh_sessions: Arc<eggress_transport_ssh::SshSessionCache>,
    pub(crate) compatibility_hooks: Option<CompatibilityRuntimeHooks>,
}

/// Explicit opt-in for `--sys`: apply the selected local HTTP/SOCKS5 listener
/// as the OS system proxy after bind, restoring on shutdown.
///
/// This is a narrow post-bind runtime hook, not ordinary listener
/// configuration: pproxy-style startup may bind an ephemeral port whose actual
/// address is required before OS state can be applied. Native startup passes
/// `None` (no OS mutation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SystemProxyRequest;

/// Minimal residual compatibility surface for runtime/post-bind behavior.
///
/// Only state that genuinely requires runtime participation lives here:
///
/// - `auth_reuse`: pre-built source-IP auth reuse cache for pproxy `--auth`.
///   The compatibility facade computes the timeout via
///   `PproxyArgs::effective_auth_timeout()` and constructs
///   `AuthReuseCache::new(timeout)` before startup; the generic supervisor
///   never interprets pproxy argument semantics, it only threads the typed
///   handle into inbound authentication.
/// - `system_proxy`: opt-in post-bind OS proxy hook (`--sys`). `None` means
///   no OS mutation.
/// - `allow_insecure_ssh_host_keys`: narrow SSH host-key policy resolved by
///   the compatibility facade from `EGRESS_SSH_INSECURE_HOST_KEYS`. Native
///   startup never sets it.
///
/// Pre-start adapter policy is intentionally absent: `-d`/`-v` log-level
/// selection is resolved by the CLI facade via
/// `PproxyArgs::default_log_level()` before supervisor construction with
/// explicit `RUST_LOG` precedence, and structured compatibility warnings stay
/// in `eggress-pproxy-compat`. The generic supervisor consumes ordinary
/// tracing and emits one generic `connection completed` log backed by normal
/// session reports.
///
/// Native startup passes `None`; compatibility startup passes
/// `Some(hooks)` via `start_from_config_with_compatibility`.
///
/// Legacy callers that still construct [`CompatibilityOptions`] should convert
/// via [`CompatibilityRuntimeHooks::from_legacy_options`]; the legacy DTO never
/// enters supervisor state.
#[derive(Clone, Default)]
pub struct CompatibilityRuntimeHooks {
    pub auth_reuse: Option<Arc<eggress_server::accept::AuthReuseCache>>,
    pub system_proxy: Option<SystemProxyRequest>,
    pub allow_insecure_ssh_host_keys: bool,
}

impl std::fmt::Debug for CompatibilityRuntimeHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompatibilityRuntimeHooks")
            .field("auth_reuse", &self.auth_reuse.is_some())
            .field("system_proxy", &self.system_proxy)
            .field(
                "allow_insecure_ssh_host_keys",
                &self.allow_insecure_ssh_host_keys,
            )
            .finish()
    }
}

impl CompatibilityRuntimeHooks {
    /// Build the canonical pproxy compatibility hooks from already-parsed
    /// facade decisions.
    ///
    /// `auth_timeout` is the fully-resolved reuse interval (typically
    /// `PproxyArgs::effective_auth_timeout()`); `system_proxy` mirrors `--sys`;
    /// `allow_insecure_ssh_host_keys` mirrors
    /// `ssh_insecure_acknowledged()` (explicit `EGRESS_SSH_INSECURE_HOST_KEYS`
    /// opt-in). This keeps pproxy argument interpretation in the facade while
    /// giving runtime a single typed construction point.
    pub fn from_facade(
        auth_timeout: Duration,
        system_proxy: bool,
        allow_insecure_ssh_host_keys: bool,
    ) -> Self {
        Self {
            auth_reuse: Some(Arc::new(eggress_server::accept::AuthReuseCache::new(
                auth_timeout,
            ))),
            system_proxy: system_proxy.then_some(SystemProxyRequest),
            allow_insecure_ssh_host_keys,
        }
    }

    /// Canonical conversion from the legacy public facade DTO.
    ///
    /// Ownership: `auth_timeout`/`system_proxy`/`compatibility_mode` are
    /// interpreted here once at the boundary; the supervisor never stores
    /// [`CompatibilityOptions`].
    ///
    /// - `auth_timeout: Some(d)` becomes a bounded process-local
    ///   `AuthReuseCache`; `None` means no reuse cache. The pproxy CLI's
    ///   30-day default lives in `PproxyArgs::effective_auth_timeout()` at the
    ///   facade, not here.
    /// - `system_proxy: true` becomes `Some(SystemProxyRequest)` (post-bind
    ///   opt-in); `false` becomes `None` (no OS mutation).
    /// - `compatibility_mode` is consumed into `allow_insecure_ssh_host_keys`
    ///   and never becomes a generic runtime mode flag: insecure SSH is
    ///   permitted only when `compatibility_mode` is true **and**
    ///   `ssh_insecure_acknowledged()` observes the explicit
    ///   `EGRESS_SSH_INSECURE_HOST_KEYS` opt-in. Non-compatibility startup
    ///   stays secure even if the variable is set.
    /// - `debug`/`verbose_level` are legacy presentation inputs and are
    ///   intentionally ignored here; logging policy is facade-owned (see
    ///   `PproxyArgs::default_log_level()`). The legacy startup shim emits a
    ///   single warning for non-default values.
    pub fn from_legacy_options(options: &CompatibilityOptions) -> Self {
        Self {
            auth_reuse: options
                .auth_timeout
                .map(eggress_server::accept::AuthReuseCache::new)
                .map(Arc::new),
            system_proxy: options.system_proxy.then_some(SystemProxyRequest),
            allow_insecure_ssh_host_keys: options.compatibility_mode && ssh_insecure_acknowledged(),
        }
    }

    /// Whether this hook set carries no compatibility state.
    ///
    /// Used by the legacy startup shim to preserve native-equivalent behavior
    /// for `CompatibilityOptions::default()` instead of fabricating hooks.
    pub fn is_empty(&self) -> bool {
        self.auth_reuse.is_none()
            && self.system_proxy.is_none()
            && !self.allow_insecure_ssh_host_keys
    }
}

/// Legacy public source-compatible facade DTO.
///
/// This type exists only so pre-Phase-3 Rust callers keep compiling. It must
/// not become a field on `ServiceSupervisor`, `RuntimeState`, connection
/// configuration, or other generic runtime state. New code should use
/// [`CompatibilityRuntimeHooks`] plus facade-level logging initialization.
///
/// - `auth_timeout`/`system_proxy` translate into narrow runtime hooks via
///   [`CompatibilityRuntimeHooks::from_legacy_options`];
/// - `compatibility_mode` exists for source compatibility and controls only
///   the legacy SSH host-key disposition (insecure only with explicit
///   `EGRESS_SSH_INSECURE_HOST_KEYS` opt-in);
/// - `debug`/`verbose_level` are legacy presentation inputs that cannot safely
///   configure an already-initialized process-global tracing subscriber from
///   deep runtime code and are therefore ignored apart from a single facade
///   warning in the legacy startup shim.
#[derive(Debug, Clone, Default)]
pub struct CompatibilityOptions {
    /// Legacy compatibility-mode selector. Consumed during conversion into
    /// the narrow `allow_insecure_ssh_host_keys` hook boolean; never stored
    /// as a generic runtime mode flag.
    pub compatibility_mode: bool,
    /// Optional per-client source-IP auth reuse interval. `None` means no
    /// reuse cache for low-level legacy callers (the CLI facade applies its
    /// own `effective_auth_timeout()` default).
    pub auth_timeout: Option<Duration>,
    /// Legacy `--sys` selector. Maps only to the narrow post-bind
    /// `SystemProxyRequest` hook.
    pub system_proxy: bool,
    /// Legacy presentation input. Accepted for source compatibility; does not
    /// configure runtime tracing.
    pub debug: bool,
    /// Legacy presentation input. Accepted for source compatibility; does not
    /// configure runtime tracing.
    pub verbose_level: u8,
}

/// Whether the operator explicitly acknowledged unverified SSH host keys.
///
/// The compatibility facade calls this before startup; the generic supervisor
/// consumes only the resulting bool. `1`/`true`/`yes` opts into
/// `SshSessionCache::new_compatibility()` (MITM risk); anything else keeps
/// `known_hosts` verification.
pub fn ssh_insecure_acknowledged() -> bool {
    std::env::var("EGRESS_SSH_INSECURE_HOST_KEYS")
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

impl ServiceSupervisor {
    pub fn start(config_path: &str) -> Result<Self, RuntimeError> {
        let (rt_config, warnings) = eggress_config::load_and_validate_with_warnings(config_path)
            .map_err(|e| RuntimeError::Config(e.to_string()))?;

        for warning in &warnings {
            tracing::warn!("config security warning: {warning}");
        }

        startup::init_supervisor(rt_config, Some(config_path.to_string()), None)
    }

    /// Start from an already-validated [`RuntimeConfig`] without reading a file.
    ///
    /// When `config_path` is provided (file-backed startup), SIGHUP reload
    /// is enabled. When `None` (in-memory/compatibility startup), SIGHUP
    /// reload is disabled because there is no stable user-authored config
    /// file to reload from.
    ///
    /// Native startup passes no compatibility state (`None`).
    pub fn start_from_config(
        rt_config: eggress_config::compile::RuntimeConfig,
        config_path: Option<String>,
    ) -> Result<Self, RuntimeError> {
        startup::init_supervisor(rt_config, config_path, None)
    }

    /// Start from an already-validated [`RuntimeConfig`] with explicit
    /// compatibility runtime hooks.
    ///
    /// Only the pproxy compatibility facades use this path, passing hooks
    /// built via `CompatibilityRuntimeHooks::from_facade`. Native startup
    /// uses `start`/`start_from_config` (no compatibility state).
    pub fn start_from_config_with_compatibility(
        rt_config: eggress_config::compile::RuntimeConfig,
        config_path: Option<String>,
        hooks: CompatibilityRuntimeHooks,
    ) -> Result<Self, RuntimeError> {
        startup::init_supervisor(rt_config, config_path, Some(hooks))
    }

    /// Legacy source-compatible startup shim.
    ///
    /// Restored so pre-Phase-3 callers of
    /// `start_from_config_with_options(RuntimeConfig, Option<String>,
    /// CompatibilityOptions)` keep compiling. Converts the legacy DTO via
    /// [`CompatibilityRuntimeHooks::from_legacy_options`] and delegates to the
    /// canonical Phase 3 startup path without duplicating supervisor
    /// initialization:
    ///
    /// ```text
    /// start_from_config_with_options(... CompatibilityOptions)
    ///     -> legacy conversion
    ///     -> start_from_config_with_compatibility(... hooks)
    ///     -> startup::init_supervisor(... Some(hooks))
    /// ```
    ///
    /// A default legacy value preserves native-equivalent behavior by
    /// delegating to `start_from_config()` (no fabricated hooks).
    /// `debug`/`verbose_level` are accepted for source compatibility but do
    /// not configure tracing; a single warning explains that logging policy
    /// is facade-owned. New code should use
    /// `start_from_config_with_compatibility` with
    /// [`CompatibilityRuntimeHooks`].
    #[deprecated(note = "use start_from_config_with_compatibility with CompatibilityRuntimeHooks")]
    pub fn start_from_config_with_options(
        rt_config: eggress_config::compile::RuntimeConfig,
        config_path: Option<String>,
        compatibility_options: CompatibilityOptions,
    ) -> Result<Self, RuntimeError> {
        if compatibility_options.debug || compatibility_options.verbose_level != 0 {
            tracing::warn!(
                "legacy CompatibilityOptions debug/verbose_level are presentation-only \
                 and do not configure runtime tracing; resolve logging at the facade \
                 via PproxyArgs::default_log_level() before startup"
            );
        }
        #[cfg(feature = "ssh")]
        if compatibility_options.compatibility_mode && !ssh_insecure_acknowledged() {
            tracing::warn!(
                "compatibility mode would disable SSH host-key verification; \
                 keeping known_hosts verification enabled. To explicitly \
                 accept unverified SSH host keys (MITM risk), set \
                 EGRESS_SSH_INSECURE_HOST_KEYS=1"
            );
        }
        let hooks = CompatibilityRuntimeHooks::from_legacy_options(&compatibility_options);
        if hooks.is_empty() {
            Self::start_from_config(rt_config, config_path)
        } else {
            Self::start_from_config_with_compatibility(rt_config, config_path, hooks)
        }
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

    /// Attempt to reload configuration. Encapsulates the full reload transaction:
    /// 1. Load and compile new config
    /// 2. Delegate to the canonical [`RuntimeState::apply_compiled_config`]
    ///    transaction (classification, snapshot build, publish, routing swap,
    ///    admin publish, health restart, pool invalidation, metrics).
    /// 3. Update stored rt_config for subsequent reloads on success.
    pub fn reload_config(&mut self) -> ReloadResult {
        let config_path = match self.config_path {
            Some(ref p) => p.clone(),
            None => {
                return ReloadResult::Rejected {
                    reason: "no config file path available for reload".to_string(),
                };
            }
        };
        let new_rt_config = match eggress_config::compile::load_and_compile(&config_path) {
            Ok(c) => c,
            Err(e) => {
                self.state.runtime_metrics.record_reload(false);
                return ReloadResult::Failed {
                    error: format!("config load: {e}"),
                };
            }
        };

        let result = self.state.apply_compiled_config(&new_rt_config);
        if matches!(result, ReloadResult::Applied { .. }) {
            // Keep the configuration used by the next reload in sync. The
            // snapshot remains authoritative for classification; this stored
            // copy only supports file-backed bookkeeping.
            self.rt_config = new_rt_config;
        }
        result
    }

    pub fn run(&mut self) -> Result<(), RuntimeError> {
        #[allow(unused_variables)]
        let config_path = self.config_path.clone().unwrap_or_default();
        let routing = self.state.routing.clone();
        let listener_cancel = self.listener_cancel.clone();
        let connection_cancel = self.connection_cancel.clone();
        let health_cancel = self.health_cancel.clone();
        let admin_cancel = self.admin_cancel.clone();
        let cancel = self.cancel.clone();
        #[allow(unused_variables)]
        let metrics = self.state.metrics.clone();
        let runtime_metrics = self.state.runtime_metrics.clone();
        let readiness = self.state.readiness.clone();
        #[cfg(feature = "operations")]
        let admin_state_ref = self.state.clone();
        let active_connections = self.state.active_connections.clone();
        let shutdown_grace = self.shutdown_grace;
        let tasks = self.tasks.clone();
        let connection_tasks = self.connection_tasks.clone();
        let admin_tasks = self.admin_tasks.clone();
        let health_for_run = self.health.clone();
        let snapshot = self.state.snapshot.clone();
        let state_ref = self.state.clone();
        let rt_config = self.rt_config.clone();
        let tls_client_config = self.tls_client_config.clone();
        let compatibility_hooks = self.compatibility_hooks.clone();
        #[cfg(feature = "ssh")]
        let ssh_sessions = self.ssh_sessions.clone();

        let handshake_timeout = rt_config.timeouts.handshake;
        let connect_timeout = rt_config.timeouts.connect;

        #[cfg(feature = "operations")]
        let listener_infos_provider: Arc<RuntimeAdminListenerInfos> =
            Arc::new(RuntimeAdminListenerInfos {
                state: admin_state_ref.clone(),
            });

        #[cfg(feature = "operations")]
        let metrics_registry_for_admin = self.metrics_registry.clone();

        let run_async = async move {
            match state_ref.health_runtime.lock() {
                Ok(mut runtime) => {
                    *runtime = Some(tokio::runtime::Handle::current());
                }
                Err(error) => {
                    tracing::warn!("health runtime state was poisoned; resetting it: {error}");
                    let mut runtime = error.into_inner();
                    *runtime = Some(tokio::runtime::Handle::current());
                    state_ref.health_runtime.clear_poison();
                }
            }
            #[cfg(feature = "operations")]
            let mut compatibility_system_proxy: Option<
                eggress_system_proxy::AppliedProxy,
            > = None;

            #[cfg(feature = "operations")]
            let metrics_registry = metrics_registry_for_admin;
            // Start health probes inside the runtime context
            {
                let mut guard = match health_for_run.lock() {
                    Ok(guard) => guard,
                    Err(error) => {
                        tracing::warn!("health manager state was poisoned; resetting it: {error}");
                        let mut guard = error.into_inner();
                        *guard = None;
                        health_for_run.clear_poison();
                        guard
                    }
                };
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
            #[cfg(feature = "operations")]
            let admin_config = current_snapshot.admin.clone();
            drop(current_snapshot);

            if listener_configs.is_empty() {
                tracing::warn!("no listeners configured; the proxy will not accept connections");
            }

            let mut prepared = Vec::new();
            #[cfg(feature = "quic")]
            let mut prepared_quic = Vec::<PreparedQuicListener>::new();
            // The facade pre-builds the typed reuse handle; runtime only
            // threads it into inbound authentication (no pproxy arg parsing).
            let compatibility_auth_reuse = compatibility_hooks
                .as_ref()
                .and_then(|hooks| hooks.auth_reuse.clone());
            #[cfg(unix)]
            let mut unix_listener_args = Vec::new();
            let mut transparent_listener_args = Vec::new();

            for lcfg in &listener_configs {
                let protocols: Vec<ProtocolId> = lcfg.protocols.to_vec();

                let auth = match &lcfg.auth {
                    Some(auth_cfg) => {
                        if auth_cfg.auth_type == "password" {
                            let username = auth_cfg.username.clone().unwrap_or_default();
                            let password = auth_cfg.password.clone().unwrap_or_default();
                            if let Some(reuse) = compatibility_auth_reuse.clone() {
                                eggress_server::accept::InboundAuthentication::UsernamePasswordWithReuse {
                                    username,
                                    password,
                                    reuse,
                                }
                            } else {
                                eggress_server::accept::InboundAuthentication::UsernamePassword {
                                    username,
                                    password,
                                }
                            }
                        } else {
                            eggress_server::accept::InboundAuthentication::None
                        }
                    }
                    None => eggress_server::accept::InboundAuthentication::None,
                };

                let connection_limit = lcfg.connection_limit.unwrap_or(1024) as usize;

                // Handle Unix domain socket listeners separately
                #[allow(unused_variables)]
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
                                    connection_limit as u64,
                                    lcfg.tls.clone(),
                                    lcfg.shadowsocks.clone(),
                                    lcfg.trojan.clone(),
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
                        let capability = check_capability(PlatformCapability::LinuxOriginalDstIpv4);
                        if capability != crate::platform::CapabilityStatus::Available {
                            #[cfg(feature = "operations")]
                            state_ref
                                .runtime_metrics
                                .record_platform_capability_check_failure();
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
                                .map_err(|e| {
                                    RuntimeError::ListenerBind {
                                        addr: lcfg.bind.clone(),
                                        source: e,
                                    }
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
                                connection_limit as u64,
                                lcfg.tls.clone(),
                                lcfg.shadowsocks.clone(),
                                lcfg.trojan.clone(),
                                lcfg.udp.clone(),
                            ));
                            continue;
                        }
                    }
                }

                #[cfg(feature = "quic")]
                if protocols.contains(&ProtocolId::Quic) || protocols.contains(&ProtocolId::Http3) {
                    let tls = lcfg.tls.clone().ok_or_else(|| {
                        RuntimeError::Other(format!(
                            "QUIC/HTTP3 listener '{}' requires certificate and key material",
                            lcfg.name
                        ))
                    })?;
                    let bind_addr: std::net::SocketAddr =
                        lcfg.bind.parse().map_err(|e| RuntimeError::ListenerBind {
                            addr: lcfg.bind.clone(),
                            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
                        })?;
                    let listener = eggress_transport_quic::QuicListener::bind(
                        bind_addr,
                        eggress_transport_quic::QuicServerConfig {
                            certificate_pem: tls.cert_pem.clone(),
                            private_key_pem: tls.key_pem.clone(),
                            idle_timeout: Duration::from_secs(60),
                            max_concurrent_streams: lcfg.connection_limit.unwrap_or(1024).max(1),
                            alpn_protocols: if protocols.contains(&ProtocolId::Http3) {
                                vec![b"h3".to_vec()]
                            } else {
                                Vec::new()
                            },
                        },
                    )
                    .await
                    .map_err(|e| RuntimeError::ListenerBind {
                        addr: lcfg.bind.clone(),
                        source: std::io::Error::other(e.to_string()),
                    })?;
                    let local_addr =
                        listener
                            .local_addr()
                            .map_err(|e| RuntimeError::ListenerBind {
                                addr: lcfg.bind.clone(),
                                source: std::io::Error::other(e.to_string()),
                            })?;
                    tracing::info!("QUIC listening on {local_addr} ({})", lcfg.name);
                    prepared_quic.push(PreparedQuicListener {
                        name: lcfg.name.clone(),
                        protocols,
                        listener,
                        local_addr,
                        auth,
                        handshake_timeout,
                        connection_limit: lcfg.connection_limit.unwrap_or(1024) as u64,
                    });
                    continue;
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

                let listener = TcpListener::new_with_reuse_port(
                    &config,
                    listener_cancel.clone(),
                    lcfg.reuse_port.unwrap_or(false),
                )
                .await
                .map_err(|e| RuntimeError::ListenerBind {
                    addr: lcfg.bind.clone(),
                    source: e,
                })?;
                let local_addr = listener
                    .local_addr()
                    .map_err(|e| RuntimeError::ListenerBind {
                        addr: lcfg.bind.clone(),
                        source: e,
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
                    trojan: lcfg.trojan.clone(),
                    fixed_target: lcfg.fixed_target.clone(),
                    local_bind: lcfg.local_bind.clone(),
                });
            }

            #[cfg(feature = "operations")]
            {
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
            }

            // Store listener addresses for admin snapshot, indexed by config order.
            // Build a lookup map from all listener types (standard, transparent, unix).
            {
                let mut addr_map: std::collections::HashMap<String, Option<std::net::SocketAddr>> =
                    std::collections::HashMap::new();
                for p in &prepared {
                    addr_map.insert(p.name.clone(), Some(p.local_addr));
                }
                #[cfg(feature = "quic")]
                for p in &prepared_quic {
                    addr_map.insert(p.name.clone(), Some(p.local_addr));
                }
                for (name, transparent_listener, _, _, _, _, _, _, _, _) in
                    &transparent_listener_args
                {
                    let addr = transparent_listener.local_addr().ok();
                    addr_map.insert(name.clone(), addr);
                }
                #[cfg(unix)]
                for (name, _, _, _, _, _, _, _, _, _) in &unix_listener_args {
                    // Unix domain sockets don't have a meaningful TCP socket address
                    addr_map.insert(name.clone(), None);
                }
                let addrs: Vec<Option<std::net::SocketAddr>> = listener_configs
                    .iter()
                    .map(|lcfg| addr_map.get(&lcfg.name).copied().flatten())
                    .collect();
                let admin_addrs = addrs.clone();
                match state_ref.listener_addrs.lock() {
                    Ok(mut guard) => *guard = addrs,
                    Err(error) => {
                        tracing::warn!(
                            "listener address state was poisoned; resetting it: {error}"
                        );
                        let mut guard = error.into_inner();
                        *guard = addrs;
                        state_ref.listener_addrs.clear_poison();
                    }
                }
                #[cfg(feature = "operations")]
                state_ref.publish_admin_listener_addrs(state_ref.snapshot.load_full(), admin_addrs);
            }

            #[cfg(feature = "extended")]
            let mut shadowsocks_udp_relays = Vec::new();
            let mut echo_udp_relays = Vec::new();

            for prepared_listener in &prepared {
                if let Some(ref udp_cfg) = prepared_listener.udp {
                    #[cfg(feature = "extended")]
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
                    if udp_cfg.mode == eggress_udp::UdpMode::Echo {
                        let socket =
                            Arc::new(tokio::net::UdpSocket::bind(udp_cfg.bind).await.map_err(
                                |e| RuntimeError::ListenerBind {
                                    addr: udp_cfg.bind.to_string(),
                                    source: e,
                                },
                            )?);
                        echo_udp_relays.push((prepared_listener.name.clone(), socket, None));
                    } else if udp_cfg.mode == eggress_udp::UdpMode::FixedTarget {
                        let socket =
                            Arc::new(tokio::net::UdpSocket::bind(udp_cfg.bind).await.map_err(
                                |e| RuntimeError::ListenerBind {
                                    addr: udp_cfg.bind.to_string(),
                                    source: e,
                                },
                            )?);
                        echo_udp_relays.push((
                            prepared_listener.name.clone(),
                            socket,
                            udp_cfg.fixed_target.clone(),
                        ));
                    }
                }
            }

            #[cfg(feature = "extended")]
            for (socket, relay_config) in shadowsocks_udp_relays {
                let relay_cancel = cancel.clone();
                tasks.spawn(async move {
                    let result =
                        eggress_udp::standalone_shadowsocks::shadowsocks_standalone_udp_relay(
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

            for (listener_name, socket, fixed_target) in echo_udp_relays {
                let relay_cancel = cancel.clone();
                let max_size = listener_configs
                    .iter()
                    .find(|l| l.name == listener_name)
                    .and_then(|l| l.udp.as_ref())
                    .map(|u| u.max_datagram_size)
                    .unwrap_or(65535);
                tasks.spawn(async move {
                    let mut buf = vec![0u8; max_size];
                    const ECHO_DNS_TIMEOUT: Duration = Duration::from_secs(2);
                    const ECHO_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
                    const ECHO_RECV_TIMEOUT: Duration = Duration::from_secs(5);
                    let target_socket = if let Some(target) = fixed_target {
                        let lookup = async {
                            match target.host {
                                eggress_core::TargetHost::Ip(ip) => {
                                    Ok(std::net::SocketAddr::new(ip, target.port))
                                }
                                eggress_core::TargetHost::Domain(domain) => match tokio::net::lookup_host((domain.as_str(), target.port)).await {
                                    Ok(mut addrs) => match addrs.next() {
                                        Some(addr) => Ok(addr),
                                        None => Err(()),
                                    },
                                    Err(_) => Err(()),
                                },
                            }
                        };
                        let addr = match tokio::time::timeout(ECHO_DNS_TIMEOUT, lookup).await {
                            Ok(Ok(a)) => a,
                            Ok(Err(())) | Err(_) => return,
                        };
                        let target_socket = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
                            Ok(s) => s,
                            Err(_) => return,
                        };
                        if tokio::time::timeout(ECHO_CONNECT_TIMEOUT, target_socket.connect(addr))
                            .await
                            .map(|r| r.is_ok())
                            .unwrap_or(false)
                        {
                            Some(target_socket)
                        } else {
                            return;
                        }
                    } else { None };
                    let mut response = vec![0u8; max_size];
                    loop {
                        tokio::select! {
                            result = socket.recv_from(&mut buf) => match result {
                                Ok((n, peer)) => {
                                    if let Some(ref target_socket) = target_socket {
                                        if target_socket.send(&buf[..n]).await.is_ok() {
                                            if let Ok(Ok(m)) = tokio::time::timeout(ECHO_RECV_TIMEOUT, target_socket.recv(&mut response)).await {
                                                let _ = socket.send_to(&response[..m], peer).await;
                                            }
                                        }
                                    } else { let _ = socket.send_to(&buf[..n], peer).await; }
                                }
                                Err(_) => break,
                            },
                            _ = relay_cancel.cancelled() => break,
                        }
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
                connection_limit,
                tls_cfg,
                ss_cfg,
                trojan_cfg,
                udp_cfg,
            ) in transparent_listener_args
            {
                let routing = routing.clone();
                let state = state_ref.clone();
                let conn_tasks = connection_tasks.clone();
                let conn_cancel = connection_cancel.clone();
                let tls_client_config = tls_client_config.clone();
                let listener_cancel = listener_cancel.clone();

                #[cfg(feature = "ssh")]
                let listener_ssh_sessions = ssh_sessions.clone();

                tasks.spawn(async move {
                    let proto_slice: Arc<[ProtocolId]> = protocols.clone().into();
                    let transparent_listener_inner = transparent_listener.inner();
                    let listener_active = Arc::new(AtomicU64::new(0));

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
                                // Cancellation is handled by the token select
                                // above; the inner accept cannot report it.
                                handle_accept_error(
                                    &format!("transparent on '{listener_name}'"),
                                    &e,
                                )
                                .await;
                                continue;
                            }
                        };

                        let connection_slot = match ListenerConnectionSlot::try_acquire(
                            &listener_active,
                            connection_limit,
                        ) {
                            Some(slot) => slot,
                            None => {
                                tracing::debug!(
                                    listener = %listener_name,
                                    limit = connection_limit,
                                    "dropping transparent connection: connection limit reached"
                                );
                                drop(stream);
                                continue;
                            }
                        };

                        transparent_accepted.fetch_add(1, Ordering::Relaxed);

                        let original_dst =
                            match eggress_server::listener::transparent::get_original_destination(&stream) {
                                Ok(addr) => addr,
                                Err(e) => {
                                    drop(connection_slot);
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
                        #[cfg(feature = "extended")]
                        let conn_ss_metrics = state.shadowsocks_metrics.clone();
                        let active = state.active_connections.clone();
                        let conn_cancel = conn_cancel.child_token();
                        let generation = state.snapshot.load().generation;
                        let tls_config = tls_cfg.clone();
                        let ss_config = ss_cfg.clone();
                        let trojan_config = trojan_cfg.clone();

                        let udp_svc =
                            make_udp_service(&state, &routing, &listener_name, udp_cfg.as_ref());

                        #[cfg(feature = "ssh")]
                        let conn_ssh_sessions = listener_ssh_sessions.clone();
                        conn_tasks.spawn(async move {
                            let _active_guard = ActiveConnectionGuard::new(active);
                            let _connection_slot = connection_slot;
                            let started = std::time::Instant::now();

                            let Some(stream) = wrap_tls_server(
                                Box::new(stream),
                                tls_config.as_ref(),
                                peer,
                            )
                            .await
                            else {
                                return;
                            };

                            let config = build_connection_config(ConnectionBuildParams {
                                routing: routing as Arc<dyn RouteService>,
                                listener: listener_str.clone(),
                                peer: Some(peer),
                                generation,
                                handshake_timeout: hs_timeout,
                                connect_timeout,
                                protocols: conn_protocols,
                                authentication: conn_auth,
                                metrics: conn_metrics,
                                udp: udp_svc,
                                tls_client_config,
                                security: InboundSecurity {
                                    shadowsocks: ss_config,
                                    trojan: trojan_config,
                                },
                                fixed_target: None,
                                local_bind: None,
                                #[cfg(feature = "extended")]
                                shadowsocks_metrics: conn_ss_metrics,
                                #[cfg(feature = "ssh")]
                                ssh_sessions: Some(conn_ssh_sessions),
                            });

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
            #[cfg(unix)]
            for (
                listener_name,
                unix_listener,
                protocols,
                auth,
                hs_timeout,
                connection_limit,
                tls_cfg,
                ss_cfg,
                trojan_cfg,
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

                #[cfg(feature = "ssh")]
                let listener_ssh_sessions = ssh_sessions.clone();
                tasks.spawn(async move {
                    let proto_slice: Arc<[ProtocolId]> = protocols.clone().into();
                    let listener_active = Arc::new(AtomicU64::new(0));

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
                                    handle_accept_error("unix", &e).await;
                                    continue;
                                }
                            },
                            _ = listener_cancel.cancelled() => {
                                break;
                            }
                        };

                        let connection_slot = match ListenerConnectionSlot::try_acquire(
                            &listener_active,
                            connection_limit,
                        ) {
                            Some(slot) => slot,
                            None => {
                                tracing::debug!(
                                    listener = %listener_name,
                                    limit = connection_limit,
                                    "dropping Unix connection: connection limit reached"
                                );
                                drop(stream);
                                continue;
                            }
                        };

                        state
                            .runtime_metrics
                            .record_unix_listener_connection_accepted();

                        let routing = routing.clone();
                        let tls_client_config = tls_client_config.clone();
                        let listener_str = listener_name.clone();
                        let conn_id = state.connection_counter.fetch_add(1, Ordering::Relaxed);
                        let conn_protocols = proto_slice.clone();
                        let conn_auth = auth.clone();
                        let conn_metrics = state.metrics.clone();
                        #[cfg(feature = "extended")]
                        let conn_ss_metrics = state.shadowsocks_metrics.clone();
                        let active = state.active_connections.clone();
                        let conn_cancel = conn_cancel.child_token();
                        let generation = state.snapshot.load().generation;

                        let tls_config = tls_cfg.clone();
                        let ss_config = ss_cfg.clone();
                        let trojan_config = trojan_cfg.clone();
                        let socket_path_clone = socket_path.clone();
                        let listener_str_for_span = listener_str.clone();

                        let udp_svc =
                            make_udp_service(&state, &routing, &listener_name, udp_cfg.as_ref());

                        #[cfg(feature = "ssh")]
                        let conn_ssh_sessions = listener_ssh_sessions.clone();
                        conn_tasks.spawn(async move {
                            let _active_guard = ActiveConnectionGuard::new(active);
                            let _connection_slot = connection_slot;
                            let started = std::time::Instant::now();

                            let peer = std::net::SocketAddr::new(
                                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                                0,
                            );

                            let Some(stream) =
                                wrap_tls_server(Box::new(stream), tls_config.as_ref(), peer).await
                            else {
                                return;
                            };

                            let config = build_connection_config(ConnectionBuildParams {
                                routing: routing as Arc<dyn RouteService>,
                                listener: listener_str,
                                peer: Some(peer),
                                generation,
                                handshake_timeout: hs_timeout,
                                connect_timeout,
                                protocols: conn_protocols,
                                authentication: conn_auth,
                                metrics: conn_metrics,
                                udp: udp_svc,
                                tls_client_config,
                                security: InboundSecurity {
                                    shadowsocks: ss_config,
                                    trojan: trojan_config,
                                },
                                fixed_target: None,
                                local_bind: None,
                                #[cfg(feature = "extended")]
                                shadowsocks_metrics: conn_ss_metrics,
                                #[cfg(feature = "ssh")]
                                ssh_sessions: Some(conn_ssh_sessions),
                            });

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
            #[cfg(feature = "operations")]
            let compatibility_proxy_selection = prepared
                .iter()
                .find(|listener| {
                    listener.protocols.contains(&ProtocolId::Socks5)
                        && listener.local_addr.port() != 0
                })
                .map(|listener| {
                    (
                        eggress_system_proxy::CompatibilityProxyKind::Socks5,
                        listener.local_addr.port(),
                    )
                })
                .or_else(|| {
                    prepared
                        .iter()
                        .find(|listener| {
                            listener.protocols.contains(&ProtocolId::Http)
                                && listener.local_addr.port() != 0
                        })
                        .map(|listener| {
                            (
                                eggress_system_proxy::CompatibilityProxyKind::Http,
                                listener.local_addr.port(),
                            )
                        })
                });

            // All compatibility TCP listeners are bound before this point,
            // so --sys can use the actual selected port. Apply before any
            // accept loop or admin task is started; an apply failure is
            // therefore still a pre-run startup error. Only present when the
            // compatibility facade explicitly opted in via
            // `CompatibilityRuntimeHooks { system_proxy: Some(..) }`; native
            // startup (`None`) never mutates OS proxy state.
            if compatibility_hooks
                .as_ref()
                .and_then(|hooks| hooks.system_proxy)
                .is_some()
            {
                #[cfg(feature = "operations")]
                {
                    let selected = compatibility_proxy_selection.ok_or_else(|| {
                        RuntimeError::Other(
                            "--sys requires a usable local HTTP or SOCKS5 listener".to_string(),
                        )
                    })?;
                    let address = std::net::SocketAddr::new(
                        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                        selected.1,
                    );
                    compatibility_system_proxy = Some(
                        eggress_system_proxy::apply_compatibility_proxy(selected.0, address)
                            .map_err(RuntimeError::Other)?,
                    );
                }
                #[cfg(not(feature = "operations"))]
                {
                    return Err(RuntimeError::Other(
                        "--sys requires the operations feature".to_string(),
                    ));
                }
            }

            #[cfg(feature = "quic")]
            for prepared_listener in prepared_quic {
                let listener_name = prepared_listener.name.clone();
                let listener_protocols: Arc<[ProtocolId]> = prepared_listener
                    .protocols
                    .iter()
                    .copied()
                    .filter(|protocol| !matches!(protocol, ProtocolId::Quic | ProtocolId::Http3))
                    .collect::<Vec<_>>()
                    .into();
                let routing = routing.clone();
                let state = state_ref.clone();
                let conn_tasks = connection_tasks.clone();
                let conn_cancel = connection_cancel.clone();
                let listener_cancel = listener_cancel.clone();
                let is_h3 = prepared_listener.protocols.contains(&ProtocolId::Http3);
                let auth = prepared_listener.auth.clone();
                let handshake_timeout_for_listener = prepared_listener.handshake_timeout;
                let connection_limit = prepared_listener.connection_limit;

                #[cfg(feature = "ssh")]
                let listener_ssh_sessions = ssh_sessions.clone();

                if is_h3 {
                    let listener = prepared_listener.listener.clone();
                    let tls_client_config_for_listener = tls_client_config.clone();
                    tasks.spawn(async move {
                        let active_streams = Arc::new(AtomicU64::new(0));
                        loop {
                            let connection = match listener.accept_connection(&listener_cancel).await {
                                Ok(Some(connection)) => connection,
                                Ok(None) => break,
                                Err(error) => {
                                    tracing::debug!(%error, listener = %listener_name, "H3 connection failed");
                                    continue;
                                }
                            };
                            let routing = routing.clone();
                            let state = state.clone();
                            let conn_tasks = conn_tasks.clone();
                            let conn_cancel = conn_cancel.child_token();
                            let listener_name = listener_name.clone();
                            let auth = auth.clone();
                            let authorization = match &auth {
                                eggress_server::accept::InboundAuthentication::None => None,
                                eggress_server::accept::InboundAuthentication::UsernamePassword { username, password }
                                | eggress_server::accept::InboundAuthentication::UsernamePasswordWithReuse { username, password, .. } => {
                                    Some((username.clone(), password.clone()))
                                }
                            };
                            let active_streams = active_streams.clone();
                            let protocols = listener_protocols.clone();
                            let connection_limit = connection_limit;
                            let tls_client_config_for_connection = tls_client_config_for_listener.clone();
                            #[cfg(feature = "ssh")]
                            let listener_ssh_sessions_for_connection = listener_ssh_sessions.clone();
                            conn_tasks.spawn(async move {
                                let result = eggress_protocol_h3::serve_connection(
                                    connection,
                                    conn_cancel.clone(),
                                    authorization,
                                    move |request, stream, peer| {
                                        let routing = routing.clone();
                                        let state = state.clone();
                                        let listener_name = listener_name.clone();
                                        let auth = auth.clone();
                                        let protocols = protocols.clone();
                                        let active_streams = active_streams.clone();
                                        let tls_client_config = tls_client_config_for_connection.clone();
                                        #[cfg(feature = "ssh")]
                                        let ssh_sessions = listener_ssh_sessions_for_connection.clone();
                                        async move {
                                            let slot = match ListenerConnectionSlot::try_acquire(&active_streams, connection_limit) {
                                                Some(slot) => slot,
                                                None => return,
                                            };
                                            let target = match request.target() {
                                                Ok(target) => target,
                                                Err(error) => {
                                                    tracing::debug!(%error, "invalid H3 CONNECT authority");
                                                    drop(slot);
                                                    return;
                                                }
                                            };
                                            let generation = state.snapshot.load().generation;
                                            let config = eggress_server::ConnectionConfig {
                                                routing: routing as Arc<dyn RouteService>,
                                                context: eggress_server::ConnectionContext {
                                                    source: Some(peer),
                                                    listener: listener_name,
                                                    generation,
                                                },
                                                handshake_timeout: handshake_timeout_for_listener,
                                                connect_timeout,
                                                protocols,
                                                authentication: auth,
                                                metrics: Some(state.metrics.clone()),
                                                udp: None,
                                                tls_client_config,
                                                shadowsocks: None,
                                                #[cfg(feature = "extended")]
                                                shadowsocks_metrics: Some(state.shadowsocks_metrics.clone()),
                                                #[cfg(not(feature = "extended"))]
                                                shadowsocks_metrics: None,
                                                trojan: None,
                                                fixed_target: None,
                                                local_bind: None,
                                                #[cfg(feature = "ssh")]
                                                ssh_sessions: Some(ssh_sessions),
                                            };
                                            let pending = eggress_server::accept::PendingTunnel {
                                                target: target.clone(),
                                                client: stream,
                                                protocol: eggress_server::accept::TunnelProtocol::Http3,
                                                reply_context: eggress_server::accept::ReplyContext::Http3,
                                                identity: eggress_core::ClientIdentity::Anonymous,
                                            };
                                            state.metrics.record_session_start();
                                            let report = eggress_server::execute::execute(
                                                eggress_server::accept::AcceptedSession::Tunnel(pending),
                                                &config,
                                            ).await;
                                            state.metrics.record_session(&report);
                                            drop(slot);
                                        }
                                    },
                                ).await;
                                if let Err(error) = result {
                                    tracing::debug!(%error, "H3 connection ended");
                                }
                            });
                        }
                    });
                } else {
                    let listener = prepared_listener.listener.clone();
                    let auth = prepared_listener.auth.clone();
                    let tls_client_config_for_listener = tls_client_config.clone();
                    tasks.spawn(async move {
                        let active_streams = Arc::new(AtomicU64::new(0));
                        let listener_name_for_handler = listener_name.clone();
                        let protocols_for_handler = listener_protocols.clone();
                        let routing_for_handler = routing.clone();
                        let state_for_handler = state.clone();
                        let auth_for_handler = auth.clone();
                        #[cfg(feature = "ssh")]
                        let listener_ssh_sessions_for_handler = Some(listener_ssh_sessions.clone());
                        let result = listener
                            .run(listener_cancel, move |stream, peer| {
                                let routing = routing_for_handler.clone();
                                let state = state_for_handler.clone();
                                let listener_name = listener_name_for_handler.clone();
                                let protocols = protocols_for_handler.clone();
                                let auth = auth_for_handler.clone();
                                let active_streams = active_streams.clone();
                                #[cfg(feature = "ssh")]
                                let ssh_sessions = listener_ssh_sessions_for_handler.clone();
                                let tls_client_config = tls_client_config_for_listener.clone();
                                async move {
                                    let Some(slot) = ListenerConnectionSlot::try_acquire(
                                        &active_streams,
                                        connection_limit,
                                    ) else {
                                        return;
                                    };
                                    let generation = state.snapshot.load().generation;
                                    let config = build_connection_config(ConnectionBuildParams {
                                        routing: routing as Arc<dyn RouteService>,
                                        listener: listener_name,
                                        peer: Some(peer),
                                        generation,
                                        handshake_timeout: handshake_timeout_for_listener,
                                        connect_timeout,
                                        protocols,
                                        authentication: auth,
                                        metrics: state.metrics.clone(),
                                        udp: None,
                                        tls_client_config,
                                        security: InboundSecurity {
                                            shadowsocks: None,
                                            trojan: None,
                                        },
                                        fixed_target: None,
                                        local_bind: None,
                                        #[cfg(feature = "extended")]
                                        shadowsocks_metrics: state.shadowsocks_metrics.clone(),
                                        #[cfg(feature = "ssh")]
                                        ssh_sessions,
                                    });
                                    let _ = eggress_server::serve_connection(stream, config).await;
                                    drop(slot);
                                }
                            })
                            .await;
                        if let Err(error) = result {
                            tracing::debug!(%error, "QUIC listener ended");
                        }
                    });
                }
            }

            for prepared_listener in prepared {
                let routing = routing.clone();
                let state = state_ref.clone();
                let conn_tasks = connection_tasks.clone();
                let conn_cancel = connection_cancel.clone();
                let tls_client_config = tls_client_config.clone();

                #[cfg(feature = "ssh")]
                let listener_ssh_sessions = ssh_sessions.clone();
                tasks.spawn(async move {
                    let proto_slice: Arc<[ProtocolId]> = prepared_listener.protocols.clone().into();

                    loop {
                        let conn = match prepared_listener.listener.accept().await {
                            Ok(c) => c,
                            Err(e) => {
                                if is_listener_cancelled(&e) {
                                    break;
                                }
                                handle_accept_error("tcp", &e).await;
                                continue;
                            }
                        };

                        let routing = routing.clone();
                        let tls_client_config = tls_client_config.clone();
                        let peer = conn.peer_addr;
                        let listener_str = prepared_listener.name.clone();
                        let conn_id = state.connection_counter.fetch_add(1, Ordering::Relaxed);
                        let conn_protocols = proto_slice.clone();
                        let conn_auth = prepared_listener.auth.clone();
                        let conn_metrics = state.metrics.clone();
                        #[cfg(feature = "extended")]
                        let conn_ss_metrics = state.shadowsocks_metrics.clone();
                        let active = state.active_connections.clone();
                        let conn_cancel = conn_cancel.child_token();
                        let generation = state.snapshot.load().generation;

                        let tls_config = prepared_listener.tls.clone();
                        let ss_config = prepared_listener.shadowsocks.clone();
                        let trojan_config = prepared_listener.trojan.clone();
                        let fixed_target = prepared_listener.fixed_target.clone();
                        let local_bind = prepared_listener.local_bind.clone();

                        let udp_svc = make_udp_service(
                            &state,
                            &routing,
                            &prepared_listener.name,
                            prepared_listener.udp.as_ref(),
                        );
                        #[cfg(feature = "ssh")]
                        let conn_ssh_sessions = listener_ssh_sessions.clone();
                        let stream_tasks = conn_tasks.clone();
                        conn_tasks.spawn(async move {
                            let _active_guard = ActiveConnectionGuard::new(active);
                            let started = std::time::Instant::now();

                            // Apply TLS if configured for this listener
                            let Some(stream) =
                                wrap_tls_server(Box::new(conn.stream), tls_config.as_ref(), peer)
                                    .await
                            else {
                                return;
                            };

                            #[cfg(feature = "extended")]
                            let advanced_protocol = conn_protocols.first().copied();
                            #[cfg(feature = "extended")]
                            let advanced_is_single = conn_protocols.len() == 1;
                            #[cfg(feature = "extended")]
                            let advanced_fixed_target = fixed_target.clone();
                            let config = build_connection_config(ConnectionBuildParams {
                                routing: routing as Arc<dyn RouteService>,
                                listener: listener_str,
                                peer: Some(peer),
                                generation,
                                handshake_timeout: prepared_listener.handshake_timeout,
                                connect_timeout,
                                protocols: conn_protocols,
                                authentication: conn_auth,
                                metrics: conn_metrics,
                                udp: udp_svc,
                                tls_client_config: tls_client_config.clone(),
                                security: InboundSecurity {
                                    shadowsocks: ss_config,
                                    trojan: trojan_config,
                                },
                                fixed_target,
                                local_bind,
                                #[cfg(feature = "extended")]
                                shadowsocks_metrics: conn_ss_metrics,
                                #[cfg(feature = "ssh")]
                                ssh_sessions: Some(conn_ssh_sessions),
                            });

                            #[cfg(feature = "extended")]
                            if matches!(
                                advanced_protocol,
                                Some(ProtocolId::Http2 | ProtocolId::WebSocket)
                            ) && advanced_is_single
                            {
                                let advanced_result = match advanced_protocol {
                                    Some(ProtocolId::Http2) => {
                                        eggress_server::advanced::serve_h2_connection(
                                            stream,
                                            config,
                                            &stream_tasks,
                                            conn_cancel.clone(),
                                        )
                                        .await
                                    }
                                    Some(ProtocolId::WebSocket) => match advanced_fixed_target {
                                        Some(target) => {
                                            eggress_server::advanced::serve_websocket_connection(
                                                stream, config, target,
                                            )
                                            .await
                                        }
                                        None => Err("WebSocket listener requires a fixed target"
                                            .to_string()),
                                    },
                                    None => {
                                        Err("advanced listener has no configured protocol"
                                            .to_string())
                                    }
                                    Some(_) => {
                                        Err("advanced listener protocol is not supported here"
                                            .to_string())
                                    }
                                };
                                if let Err(error) = advanced_result {
                                    tracing::debug!(%peer, %error, "advanced listener ended");
                                }
                                return;
                            }

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

                            // Generic session reporting: one `connection completed`
                            // line backed by the normal session report. pproxy
                            // `-d`/`-v` presentation policy is resolved by the
                            // CLI facade via `default_log_level()` before
                            // startup; no compatibility verbosity state lives
                            // in the supervisor.
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
            #[cfg(feature = "reverse")]
            {
                let current_snapshot = snapshot.load();
                let reverse_servers = current_snapshot.reverse_servers.clone();
                let reverse_clients = current_snapshot.reverse_clients.clone();
                drop(current_snapshot);

                for rs_cfg in reverse_servers {
                    if rs_cfg.pproxy_compat {
                        let server_config =
                            eggress_protocol_reverse::compat_pproxy::PproxyBackwardServerConfig {
                                control_bind: rs_cfg.control_bind,
                                external_bind: rs_cfg.external_bind,
                                auth: eggress_protocol_reverse::compat_pproxy::raw_auth(
                                    rs_cfg.auth_username.as_deref(),
                                    rs_cfg.auth_password.as_deref(),
                                ),
                                max_control_connections: rs_cfg.max_control_connections as usize,
                                max_pending_external: rs_cfg.max_pending_external as usize,
                                read_timeout_ms: rs_cfg.read_timeout_ms,
                                socks5_target: None,
                                client_framing:
                                    eggress_protocol_reverse::compat_pproxy::PproxyBackwardFraming::Raw,
                            };
                        let server =
                            eggress_protocol_reverse::compat_pproxy::PproxyBackwardServer::new(
                                server_config,
                            );
                        let server_cancel = server.cancel_token();
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
                                tracing::error!(error = %e, "pproxy backward server error");
                            }
                        });
                        continue;
                    }
                    let server_tls = rs_cfg.tls.as_ref().map(|t| {
                        eggress_protocol_reverse::tls::ReverseServerTlsConfig {
                            cert_pem: t.cert_pem.clone(),
                            key_pem: t.key_pem.clone(),
                            client_ca_pem: t.client_ca_pem.clone(),
                            require_client_cert: t.require_client_cert,
                        }
                    });
                    let server_config = eggress_protocol_reverse::server::ReverseServerConfig {
                        control_bind: rs_cfg.control_bind,
                        external_bind: Some(rs_cfg.external_bind),
                        auth_username: rs_cfg.auth_username.clone(),
                        auth_password: rs_cfg.auth_password.clone(),
                        max_control_connections: rs_cfg.max_control_connections,
                        read_timeout_ms: rs_cfg.read_timeout_ms,
                        allow_bind: rs_cfg.allow_bind.clone(),
                        max_listeners_per_client: rs_cfg.max_listeners_per_client,
                        max_streams_per_listener: rs_cfg.max_streams_per_listener,
                        max_pending_external: rs_cfg.max_pending_external,
                        tls: server_tls,
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
                    let mut server =
                        eggress_protocol_reverse::server::ReverseServer::new(server_config);
                    server.set_metrics(state_ref.reverse_metrics.clone());
                    let server_state = server.state_handle();
                    let server_cancel = server.cancel_token();

                    state_ref
                        .reverse_registry
                        .register(eggress_admin::ReverseServerEntry {
                            id: eggress_admin::ReverseServerId::from(rs_cfg.id.as_str()),
                            control_bind: rs_cfg.control_bind.to_string(),
                            state: server_state,
                        });

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
                        if rc_cfg.pproxy_compat {
                            let client_config = eggress_protocol_reverse::compat_pproxy::PproxyBackwardClientConfig {
                                server_addr: rc_cfg.server_addr,
                                server_chain: rc_cfg.server_chain.clone(),
                                auth: eggress_protocol_reverse::compat_pproxy::raw_auth(
                                    rc_cfg.auth_username.as_deref(),
                                    rc_cfg.auth_password.as_deref(),
                                ),
                                reconnect_initial_ms: rc_cfg.reconnect_initial_ms,
                                reconnect_max_ms: rc_cfg.reconnect_max_ms,
                                read_timeout_ms: rc_cfg.read_timeout_ms,
                                target_connect_timeout_ms: 10_000,
                                server_framing:
                                    eggress_protocol_reverse::compat_pproxy::PproxyBackwardFraming::Raw,
                            };
                            let client =
                                eggress_protocol_reverse::compat_pproxy::PproxyBackwardClient::new(
                                    client_config,
                                    std::sync::Arc::new(
                                        crate::reverse::RouteEngineTargetResolver::new(
                                            routing.clone(),
                                            host.clone(),
                                            port,
                                            std::sync::Arc::from(rc_cfg.id.as_str()),
                                            Some(rc_cfg.server_addr),
                                        ),
                                    ),
                                );
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
                                    tracing::error!(error = %e, client_id = %client_id, server = %server_addr, conn = conn_idx, "pproxy backward client error");
                                }
                            });
                            continue;
                        }
                        let client_tls = rc_cfg.tls.as_ref().map(|t| {
                            eggress_protocol_reverse::tls::ReverseClientTlsConfig {
                                ca_pem: t.ca_pem.clone(),
                                server_name: t.server_name.clone(),
                                client_cert_pem: t.client_cert_pem.clone(),
                                client_key_pem: t.client_key_pem.clone(),
                            }
                        });
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
                            target_connect_timeout_ms: 10_000,
                            tls: client_tls,
                        };
                        let mut client =
                            eggress_protocol_reverse::client::ReverseClient::new(client_config);
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

            // Pre-bind admin listener before marking readiness so bind failures
            // are surfaced as startup errors rather than silent background failures.
            #[cfg(feature = "operations")]
            let pre_bound_admin = if let Some(ref admin_cfg) = admin_config {
                if admin_cfg.enabled {
                    let bind = admin_cfg.bind.clone();
                    let admin_cancel_token = admin_cancel.clone();
                    match eggress_admin::AdminServer::new(&bind, admin_cancel_token).await {
                        Ok(s) => Some(s),
                        Err(e) => {
                            return Err(RuntimeError::ListenerBind {
                                addr: bind,
                                source: std::io::Error::new(
                                    std::io::ErrorKind::AddrInUse,
                                    e.to_string(),
                                ),
                            });
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            #[cfg(feature = "operations")]
            if let (Some(server), Some(admin_cfg)) = (pre_bound_admin, admin_config.as_ref()) {
                let metrics_enabled = admin_cfg.metrics;
                let state_ref = state_ref.clone();
                let provider: Arc<dyn AdminSnapshotProvider> = listener_infos_provider.clone();
                if let Ok(addr) = server.local_addr() {
                    match state_ref.admin_local_addr.lock() {
                        Ok(mut guard) => *guard = Some(addr),
                        Err(error) => {
                            tracing::warn!(
                                "admin listener address state was poisoned; resetting it: {error}"
                            );
                            let mut guard = error.into_inner();
                            *guard = Some(addr);
                            state_ref.admin_local_addr.clear_poison();
                        }
                    }
                }
                let admin_auth = admin_cfg.auth.clone();
                admin_tasks.spawn(async move {
                    let admin_state = eggress_admin::AdminState {
                        metrics: metrics_registry.clone(),
                        start_time: state_ref.start_time,
                        readiness: state_ref.readiness.clone(),
                        active_connections: Some(state_ref.active_connections.clone()),
                        provider,
                        udp_registry: state_ref.udp_registry.clone(),
                        #[cfg(feature = "reverse")]
                        reverse_registry: state_ref.reverse_registry.clone(),
                        #[cfg(not(feature = "reverse"))]
                        reverse_registry: std::sync::Arc::new(eggress_admin::ReverseRegistry::new()),
                        metrics_enabled,
                        auth: admin_auth,
                    };
                    if let Err(e) = server.run(admin_state).await {
                        tracing::error!("admin server error: {e}");
                    }
                });
            }

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

                // Readiness also means signal handling is installed. This
                // prevents a reload signal from racing startup.
                readiness.store(true, Ordering::Release);

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
                        _ = async { sighup.as_mut().ok()?.recv().await }, if sighup.is_ok() && !config_path.is_empty() => {
                            tracing::info!("reload signal received, reloading config from {config_path}");
                            let config_path_clone = config_path.clone();
                            let load_result = tokio::task::spawn_blocking(move || {
                                eggress_config::compile::load_and_compile(&config_path_clone)
                            }).await;
                            match load_result {
                                Ok(Ok(new_rt_config)) => {
                                    // Single canonical transaction owns
                                    // classification, snapshot build, publish,
                                    // routing/admin/health/pool/metrics side
                                    // effects. Metrics are recorded inside.
                                    match state_ref.apply_compiled_config(&new_rt_config) {
                                        ReloadResult::Applied { generation: gen, upstreams: upstream_count } => {
                                            tracing::info!(
                                                generation = gen,
                                                upstreams = upstream_count,
                                                "config reloaded successfully"
                                            );
                                        }
                                        ReloadResult::Rejected { reason } => {
                                            tracing::error!("reload rejected: {reason}");
                                        }
                                        ReloadResult::Failed { error } => {
                                            tracing::error!("reload failed (snapshot build): {error}");
                                        }
                                    }
                                }
                                Ok(Err(e)) => {
                                    runtime_metrics.record_reload(false);
                                    tracing::error!("reload failed (config load): {e}");
                                }
                                Err(join_err) => {
                                    runtime_metrics.record_reload(false);
                                    tracing::error!("reload task panicked: {join_err}");
                                }
                            }
                        }
                    }
                }
            }

            #[cfg(not(unix))]
            {
                readiness.store(true, Ordering::Release);
                tokio::select! {
                    _ = cancel.cancelled() => {
                        tracing::info!("shutdown requested via cancel token");
                    }
                    _ = tokio::signal::ctrl_c() => {
                        tracing::info!("shutdown signal received");
                    }
                }
            }

            shutdown_ordered(ShutdownPlan {
                readiness: readiness.clone(),
                listener_cancel: listener_cancel.clone(),
                health_cancel: health_cancel.clone(),
                connection_cancel: connection_cancel.clone(),
                admin_cancel: admin_cancel.clone(),
                state: state_ref.clone(),
                tasks: tasks.clone(),
                connection_tasks: connection_tasks.clone(),
                admin_tasks: admin_tasks.clone(),
                active_connections: active_connections.clone(),
                shutdown_grace,
                #[cfg(feature = "ssh")]
                ssh_sessions: ssh_sessions.clone(),
                #[cfg(feature = "operations")]
                compatibility_system_proxy,
            })
            .await?;
            Ok::<_, RuntimeError>(())
        };

        let result = if tokio::runtime::Handle::try_current().is_err() {
            // Caller is not inside a tokio runtime; create one and block on it.
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_async)
        } else {
            // Caller is already inside a tokio runtime. Driving a long-lived
            // supervisor body via Handle::current().block_on from a worker
            // thread would panic or deadlock. Run on a dedicated OS thread
            // with its own runtime.
            std::thread::Builder::new()
                .name("eggress-supervisor".to_string())
                .spawn(move || -> Result<(), RuntimeError> {
                    let rt = tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()
                        .map_err(RuntimeError::RuntimeInit)?;
                    rt.block_on(run_async)
                })
                .map_err(RuntimeError::RuntimeInit)?
                .join()
                .map_err(|payload| {
                    let message = payload
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| payload.downcast_ref::<&'static str>().copied())
                        .unwrap_or("unknown panic payload");
                    RuntimeError::Other(format!("supervisor thread panicked: {message}"))
                })?
        };

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
    use std::sync::atomic::AtomicBool;
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
                h2: None,
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
        assert_eq!(active.load(Ordering::Acquire), 0);
        active.fetch_add(1, Ordering::AcqRel);
        assert_eq!(active.load(Ordering::Acquire), 1);
        active.fetch_add(1, Ordering::AcqRel);
        assert_eq!(active.load(Ordering::Acquire), 2);
        active.fetch_sub(1, Ordering::Release);
        assert_eq!(active.load(Ordering::Acquire), 1);
        active.fetch_sub(1, Ordering::Release);
        assert_eq!(active.load(Ordering::Acquire), 0);
    }

    #[test]
    fn active_connections_guard_releases_on_panic() {
        let active = Arc::new(AtomicU64::new(0));
        let result = std::panic::catch_unwind({
            let active = active.clone();
            move || {
                let _guard = ActiveConnectionGuard::new(active);
                panic!("connection task panic");
            }
        });
        assert!(result.is_err());
        assert_eq!(active.load(Ordering::Acquire), 0);
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
        let snap = sup.state.snapshot.load();
        let result = classify_reload_config(
            &snap.listeners,
            &snap.timeouts,
            snap.admin.as_ref(),
            &new_config,
        );
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
        let snap = sup.state.snapshot.load();
        let result = classify_reload_config(
            &snap.listeners,
            &snap.timeouts,
            snap.admin.as_ref(),
            &new_config,
        );
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
        let snap = sup.state.snapshot.load();
        let result = classify_reload_config(
            &snap.listeners,
            &snap.timeouts,
            snap.admin.as_ref(),
            &new_config,
        );
        assert!(result.is_ok(), "unchanged listeners should be accepted");
    }

    #[test]
    fn reload_rejects_udp_topology_changes() {
        let config_without_udp = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:8080"
protocols = ["socks5"]
"#;
        let config_with_udp = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:8080"
protocols = ["socks5"]

[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
"#;
        let f1 = write_config(config_without_udp);
        let f2 = write_config(config_with_udp);
        let sup = ServiceSupervisor::start(f1.path().to_str().unwrap()).unwrap();
        let new_config =
            eggress_config::compile::load_and_compile(f2.path().to_str().unwrap()).unwrap();

        let snap = sup.state.snapshot.load();
        let result = classify_reload_config(
            &snap.listeners,
            &snap.timeouts,
            snap.admin.as_ref(),
            &new_config,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("UDP"));
    }

    #[test]
    fn reload_rejects_timeout_change() {
        let config1 = r#"
version = 1

[timeouts]
handshake = "10s"

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]
"#;
        let config2 = r#"
version = 1

[timeouts]
handshake = "5s"

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]
"#;
        let f1 = write_config(config1);
        let f2 = write_config(config2);
        let sup = ServiceSupervisor::start(f1.path().to_str().unwrap()).unwrap();
        let new_config =
            eggress_config::compile::load_and_compile(f2.path().to_str().unwrap()).unwrap();

        let snap = sup.state.snapshot.load();
        let result = classify_reload_config(
            &snap.listeners,
            &snap.timeouts,
            snap.admin.as_ref(),
            &new_config,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timeout"));
    }

    #[test]
    fn reload_rejects_admin_bind_change() {
        let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[admin]
bind = "127.0.0.1:9090"
enabled = false
"#;
        let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[admin]
bind = "127.0.0.1:9091"
enabled = false
"#;
        let f1 = write_config(config1);
        let f2 = write_config(config2);
        let sup = ServiceSupervisor::start(f1.path().to_str().unwrap()).unwrap();
        let new_config =
            eggress_config::compile::load_and_compile(f2.path().to_str().unwrap()).unwrap();

        let snap = sup.state.snapshot.load();
        let result = classify_reload_config(
            &snap.listeners,
            &snap.timeouts,
            snap.admin.as_ref(),
            &new_config,
        );
        assert!(result.is_err(), "admin bind change should be rejected");
        assert!(result.unwrap_err().contains("admin"));
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
        let snap = sup.state.snapshot.load();
        let result = classify_reload_config(
            &snap.listeners,
            &snap.timeouts,
            snap.admin.as_ref(),
            &new_config,
        );
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
        let snap = sup.state.snapshot.load();
        let result = classify_reload_config(
            &snap.listeners,
            &snap.timeouts,
            snap.admin.as_ref(),
            &new_config,
        );
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
        let snap = sup.state.snapshot.load();
        let result = classify_reload_config(
            &snap.listeners,
            &snap.timeouts,
            snap.admin.as_ref(),
            &new_config,
        );
        assert!(result.is_err(), "unix path change should be rejected");
        assert!(result.unwrap_err().contains("unix socket path"));
    }

    #[test]
    fn compute_advertise_explicit() {
        let result = compute_advertise_ip(
            Some("10.0.0.1".parse().unwrap()),
            "0.0.0.0".parse().unwrap(),
            Some("127.0.0.1:5000".parse().unwrap()),
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
            Some("127.0.0.1:5000".parse().unwrap()),
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
            Some("127.0.0.1:5000".parse().unwrap()),
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
            Some("192.168.1.10:5000".parse().unwrap()),
        );
        assert!(
            result.is_err(),
            "non-loopback with unspecified bind should fail"
        );
    }

    #[test]
    fn compute_advertise_without_tcp_peer_is_rejected() {
        let result = compute_advertise_ip(None, "0.0.0.0".parse().unwrap(), None);
        assert!(result.is_err());
    }

    #[test]
    fn compute_advertise_ipv6_loopback() {
        let result = compute_advertise_ip(
            None,
            "::".parse().unwrap(),
            Some("[::1]:5000".parse().unwrap()),
        );
        assert_eq!(
            result.unwrap(),
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        );
    }

    #[test]
    fn compute_advertise_preserves_unspecified_bind_family() {
        let result = compute_advertise_ip(
            None,
            "0.0.0.0".parse().unwrap(),
            Some("[::ffff:127.0.0.1]:5000".parse().unwrap()),
        );
        assert_eq!(
            result.unwrap(),
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        );
    }

    #[test]
    fn compute_advertise_explicit_overrides_bind() {
        let result = compute_advertise_ip(
            Some("10.0.0.1".parse().unwrap()),
            "192.168.1.1".parse().unwrap(),
            Some("127.0.0.1:5000".parse().unwrap()),
        );
        assert_eq!(
            result.unwrap(),
            std::net::IpAddr::V4("10.0.0.1".parse().unwrap())
        );
    }

    // --- Legacy CompatibilityOptions source-compat + conversion (closure) ---

    static LEGACY_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_legacy_env_reset(f: impl FnOnce()) {
        let _guard = LEGACY_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("EGRESS_SSH_INSECURE_HOST_KEYS").ok();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        match saved {
            Some(v) => std::env::set_var("EGRESS_SSH_INSECURE_HOST_KEYS", v),
            None => std::env::remove_var("EGRESS_SSH_INSECURE_HOST_KEYS"),
        }
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    fn minimal_runtime_config() -> eggress_config::compile::RuntimeConfig {
        eggress_config::compile::RuntimeConfig {
            process: eggress_config::compile::ProcessConfig::default(),
            timeouts: eggress_config::compile::TimeoutConfig::default(),
            listeners: vec![],
            upstreams: vec![],
            groups: vec![],
            rules: vec![],
            default_action: RouteActionSpec::Direct,
            admin: None,
            reverse_servers: vec![],
            reverse_clients: vec![],
        }
    }

    #[test]
    #[allow(deprecated)]
    fn legacy_compatibility_options_source_surface_type_checks() {
        // Fails to compile if the pre-Phase-3 public field names/types disappear.
        let options = crate::CompatibilityOptions {
            compatibility_mode: true,
            auth_timeout: Some(Duration::from_secs(60)),
            system_proxy: false,
            debug: true,
            verbose_level: 2,
        };
        assert!(options.compatibility_mode);
        assert_eq!(options.auth_timeout, Some(Duration::from_secs(60)));
        assert!(!options.system_proxy);
        assert!(options.debug);
        assert_eq!(options.verbose_level, 2);
        let cloned = options.clone();
        let _debug = format!("{cloned:?}");
        let _default = crate::CompatibilityOptions::default();

        // Old runtime entry-point shape must keep type-checking.
        let _method: fn(
            eggress_config::compile::RuntimeConfig,
            Option<String>,
            crate::CompatibilityOptions,
        ) -> Result<ServiceSupervisor, crate::RuntimeError> =
            ServiceSupervisor::start_from_config_with_options;

        // Conversion entry point must keep its canonical shape.
        let hooks = CompatibilityRuntimeHooks::from_legacy_options(&options);
        assert!(hooks.auth_reuse.is_some());
    }

    #[test]
    fn legacy_default_options_convert_to_empty_hooks() {
        with_legacy_env_reset(|| {
            std::env::remove_var("EGRESS_SSH_INSECURE_HOST_KEYS");
            let hooks = CompatibilityRuntimeHooks::from_legacy_options(
                &crate::CompatibilityOptions::default(),
            );
            assert!(hooks.auth_reuse.is_none());
            assert!(hooks.system_proxy.is_none());
            assert!(!hooks.allow_insecure_ssh_host_keys);
            assert!(hooks.is_empty());
        });
    }

    #[test]
    fn legacy_auth_timeout_maps_to_cache() {
        with_legacy_env_reset(|| {
            std::env::remove_var("EGRESS_SSH_INSECURE_HOST_KEYS");
            let options = crate::CompatibilityOptions {
                auth_timeout: Some(Duration::from_secs(30)),
                ..Default::default()
            };
            let hooks = CompatibilityRuntimeHooks::from_legacy_options(&options);
            assert!(hooks.auth_reuse.is_some());
            assert!(hooks.system_proxy.is_none());
        });
    }

    #[test]
    fn legacy_system_proxy_maps_to_narrow_hook() {
        with_legacy_env_reset(|| {
            std::env::remove_var("EGRESS_SSH_INSECURE_HOST_KEYS");
            let options = crate::CompatibilityOptions {
                system_proxy: true,
                ..Default::default()
            };
            let hooks = CompatibilityRuntimeHooks::from_legacy_options(&options);
            assert!(hooks.system_proxy.is_some());
            assert!(hooks.auth_reuse.is_none());
        });
    }

    #[test]
    fn legacy_logging_fields_do_not_enter_hooks() {
        with_legacy_env_reset(|| {
            std::env::remove_var("EGRESS_SSH_INSECURE_HOST_KEYS");
            let options = crate::CompatibilityOptions {
                debug: true,
                verbose_level: 3,
                ..Default::default()
            };
            let hooks = CompatibilityRuntimeHooks::from_legacy_options(&options);
            assert!(hooks.is_empty());
        });
    }

    #[test]
    fn legacy_compat_mode_without_env_stays_secure() {
        with_legacy_env_reset(|| {
            std::env::remove_var("EGRESS_SSH_INSECURE_HOST_KEYS");
            let options = crate::CompatibilityOptions {
                compatibility_mode: true,
                ..Default::default()
            };
            let hooks = CompatibilityRuntimeHooks::from_legacy_options(&options);
            assert!(!hooks.allow_insecure_ssh_host_keys);
        });
    }

    #[test]
    fn legacy_compat_mode_with_false_env_stays_secure() {
        with_legacy_env_reset(|| {
            std::env::set_var("EGRESS_SSH_INSECURE_HOST_KEYS", "0");
            let options = crate::CompatibilityOptions {
                compatibility_mode: true,
                ..Default::default()
            };
            let hooks = CompatibilityRuntimeHooks::from_legacy_options(&options);
            assert!(!hooks.allow_insecure_ssh_host_keys);
        });
    }

    #[test]
    fn legacy_compat_mode_with_accepted_env_allows_insecure() {
        for accepted in ["1", "true", "yes"] {
            with_legacy_env_reset(|| {
                std::env::set_var("EGRESS_SSH_INSECURE_HOST_KEYS", accepted);
                let options = crate::CompatibilityOptions {
                    compatibility_mode: true,
                    ..Default::default()
                };
                let hooks = CompatibilityRuntimeHooks::from_legacy_options(&options);
                assert!(
                    hooks.allow_insecure_ssh_host_keys,
                    "accepted value {accepted:?} should allow insecure SSH"
                );
            });
        }
    }

    #[test]
    fn legacy_non_compat_mode_with_accepted_env_stays_secure() {
        with_legacy_env_reset(|| {
            std::env::set_var("EGRESS_SSH_INSECURE_HOST_KEYS", "1");
            let options = crate::CompatibilityOptions {
                compatibility_mode: false,
                ..Default::default()
            };
            let hooks = CompatibilityRuntimeHooks::from_legacy_options(&options);
            assert!(!hooks.allow_insecure_ssh_host_keys);
        });
    }

    #[test]
    #[allow(deprecated)]
    fn legacy_start_shim_default_preserves_native_hooks() {
        with_legacy_env_reset(|| {
            std::env::remove_var("EGRESS_SSH_INSECURE_HOST_KEYS");
            let sup = ServiceSupervisor::start_from_config_with_options(
                minimal_runtime_config(),
                None,
                crate::CompatibilityOptions::default(),
            )
            .unwrap();
            assert!(sup.compatibility_hooks.is_none());
        });
    }

    #[test]
    #[allow(deprecated)]
    fn legacy_start_shim_nonempty_produces_hooks() {
        with_legacy_env_reset(|| {
            std::env::remove_var("EGRESS_SSH_INSECURE_HOST_KEYS");
            let sup = ServiceSupervisor::start_from_config_with_options(
                minimal_runtime_config(),
                None,
                crate::CompatibilityOptions {
                    auth_timeout: Some(Duration::from_secs(5)),
                    ..Default::default()
                },
            )
            .unwrap();
            let hooks = sup.compatibility_hooks.as_ref().unwrap();
            assert!(hooks.auth_reuse.is_some());
        });
    }
}
