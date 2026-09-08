//! Supervisor startup preparation: validated config becomes running state.
//!
//! [`init_supervisor`] owns feature-gate validation, listener bind
//! pre-validation, metrics/UDP registry wiring, health manager construction,
//! and [`RuntimeState`] assembly. Listener socket binding stays in the `run()`
//! orchestration so bind failures surface as startup errors in one place.

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use eggress_routing::health::HealthManager;
use eggress_routing::SharedRoutingService;

use crate::error::RuntimeError;
use crate::snapshot::compile_runtime_snapshot;

#[cfg(feature = "operations")]
use super::operations::RuntimeAdminState;
use super::state::RuntimeState;
use super::{CompatibilityOptions, ServiceSupervisor};

/// Reject configurations the current feature set cannot run, and pre-validate
/// listener bind addresses so malformed binds fail before any state exists.
pub(crate) fn validate_startup_config(
    rt_config: &eggress_config::compile::RuntimeConfig,
) -> Result<(), RuntimeError> {
    #[cfg(not(feature = "reverse"))]
    if !rt_config.reverse_servers.is_empty() || !rt_config.reverse_clients.is_empty() {
        return Err(RuntimeError::Other(
            "reverse proxy support not included in this build".to_string(),
        ));
    }

    #[cfg(not(feature = "operations"))]
    if rt_config.admin.as_ref().is_some_and(|a| a.enabled) {
        return Err(RuntimeError::Other(
            "admin server support not included in this build; \
             enable the 'operations' feature or remove [admin] from config"
                .to_string(),
        ));
    }

    for lcfg in &rt_config.listeners {
        if lcfg.unix.is_none() {
            let _bind_addr: std::net::SocketAddr =
                lcfg.bind.parse().map_err(|e| RuntimeError::ListenerBind {
                    addr: lcfg.bind.clone(),
                    source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
                })?;
        }
    }

    Ok(())
}

/// Resolve the process-wide UDP association cap from listener configs.
///
/// Listeners are expected to agree; when they do not, the first value wins
/// and the disagreement is logged so the misconfiguration is visible.
pub(crate) fn resolve_udp_global_limit(
    rt_config: &eggress_config::compile::RuntimeConfig,
) -> usize {
    let mut udp_global_limit: Option<usize> = None;
    for listener in &rt_config.listeners {
        if let Some(udp) = &listener.udp {
            let value = udp.max_associations_global;
            match udp_global_limit {
                None => udp_global_limit = Some(value),
                Some(existing) if existing != value => {
                    tracing::warn!(
                        listener = %listener.name,
                        existing,
                        other = value,
                        "multiple listeners specify udp.max_associations_global with different values; using first"
                    );
                }
                Some(_) => {}
            }
        }
    }
    udp_global_limit.unwrap_or(1024)
}

/// Build the SSH session cache, honoring the compatibility-mode host-key
/// policy (explicit opt-in via `EGRESS_SSH_INSECURE_HOST_KEYS` only).
#[cfg(feature = "ssh")]
pub(crate) fn build_ssh_sessions(
    compatibility_mode: bool,
) -> Arc<eggress_transport_ssh::SshSessionCache> {
    Arc::new(if compatibility_mode {
        let insecure_acknowledged = std::env::var("EGRESS_SSH_INSECURE_HOST_KEYS")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        if insecure_acknowledged {
            eggress_transport_ssh::SshSessionCache::new_compatibility()
        } else {
            tracing::warn!(
                "compatibility mode would disable SSH host-key verification; \
                 keeping known_hosts verification enabled. To explicitly \
                 accept unverified SSH host keys (MITM risk), set \
                 EGRESS_SSH_INSECURE_HOST_KEYS=1"
            );
            eggress_transport_ssh::SshSessionCache::new()
        }
    } else {
        eggress_transport_ssh::SshSessionCache::new()
    })
}

/// Assemble a [`ServiceSupervisor`] (shared state, tokens, task trackers)
/// from an already-validated [`eggress_config::compile::RuntimeConfig`].
pub(crate) fn init_supervisor(
    rt_config: eggress_config::compile::RuntimeConfig,
    config_path: Option<String>,
    compatibility_options: CompatibilityOptions,
) -> Result<ServiceSupervisor, RuntimeError> {
    validate_startup_config(&rt_config)?;

    let udp_metrics = Arc::new(eggress_udp::metrics::UdpMetrics::new());
    #[cfg(feature = "extended")]
    let shadowsocks_metrics = Arc::new(eggress_protocol_shadowsocks::ShadowsocksMetrics::new());

    let metrics_registry = Arc::new(eggress_metrics::MetricsRegistry::new());
    metrics_registry.set_udp_metrics(udp_metrics.clone());
    #[cfg(feature = "extended")]
    {
        metrics_registry.set_shadowsocks_metrics(shadowsocks_metrics.clone());
    }
    let metrics: Arc<dyn eggress_server::SessionMetrics> = metrics_registry.clone();
    let runtime_metrics: Arc<dyn eggress_metrics::RuntimeMetrics> = metrics_registry.clone();
    let readiness = Arc::new(AtomicBool::new(false));

    let snapshot = compile_runtime_snapshot(&rt_config, None)
        .map_err(|e| RuntimeError::Config(e.to_string()))?;
    let snapshot = Arc::new(ArcSwap::from_pointee(snapshot));

    let routing = Arc::new(SharedRoutingService::new_arc(
        snapshot.load().router.clone(),
    ));

    let active_connections = Arc::new(AtomicU64::new(0));
    let connection_counter = Arc::new(AtomicU64::new(1));

    let udp_global_limit = resolve_udp_global_limit(&rt_config);

    let udp_registry = Arc::new(eggress_udp::registry::UdpAssociationRegistry::new(
        eggress_udp::limits::UdpLimits {
            max_associations_global: udp_global_limit,
            ..Default::default()
        },
    ));

    let cancel = CancellationToken::new();
    let listener_cancel = CancellationToken::new();
    let connection_cancel = CancellationToken::new();
    let health_cancel = CancellationToken::new();
    let admin_cancel = CancellationToken::new();
    let health = Arc::new(Mutex::new(if snapshot.load().upstreams.is_empty() {
        None
    } else {
        Some(HealthManager::new(health_cancel.clone()))
    }));

    #[cfg(feature = "reverse")]
    let reverse_metrics = Arc::new(eggress_protocol_reverse::metrics::ReverseMetrics::new());
    let udp_tasks = TaskTracker::new();

    let state = Arc::new(RuntimeState {
        snapshot: snapshot.clone(),
        routing: routing.clone(),
        metrics: metrics.clone(),
        runtime_metrics: runtime_metrics.clone(),
        readiness,
        start_time: Instant::now(),
        active_connections,
        connection_counter,
        admin_local_addr: Arc::new(Mutex::new(None)),
        listener_addrs: Arc::new(Mutex::new(Vec::new())),
        #[cfg(feature = "operations")]
        admin_snapshot: Arc::new(ArcSwap::from_pointee(RuntimeAdminState {
            snapshot: snapshot.load_full(),
            listener_addrs: Vec::new(),
        })),
        health: health.clone(),
        health_cancel: health_cancel.clone(),
        health_runtime: Mutex::new(None),
        udp_registry,
        udp_metrics,
        #[cfg(feature = "extended")]
        shadowsocks_metrics,
        udp_tasks: udp_tasks.clone(),
        transparent_accepted_total: Arc::new(AtomicU64::new(0)),
        transparent_original_dst_failed_total: Arc::new(AtomicU64::new(0)),
        #[cfg(feature = "reverse")]
        reverse_registry: Arc::new(eggress_admin::ReverseRegistry::new()),
        #[cfg(feature = "reverse")]
        reverse_metrics,
    });

    // Bridge transparent proxy atomics to MetricsRegistry for /metrics
    metrics_registry.set_transparent_counters(
        state.transparent_accepted_total.clone(),
        state.transparent_original_dst_failed_total.clone(),
    );

    #[cfg(feature = "ssh")]
    let ssh_sessions = build_ssh_sessions(compatibility_options.compatibility_mode);

    let tasks = TaskTracker::new();
    let connection_tasks = TaskTracker::new();

    let shutdown_grace = rt_config.process.shutdown_grace;

    Ok(ServiceSupervisor {
        config_path,
        state,
        metrics_registry,
        cancel,
        listener_cancel,
        connection_cancel,
        health_cancel,
        admin_cancel,
        health: health.clone(),
        tasks,
        connection_tasks,
        admin_tasks: TaskTracker::new(),
        shutdown_grace,
        rt_config,
        tls_client_config: None,
        #[cfg(feature = "ssh")]
        ssh_sessions,
        compatibility_options,
    })
}
