//! Shared runtime state and the canonical reload transaction.
//!
//! [`RuntimeState`] is the single owner of the compiled snapshot, routing
//! service, metrics handle, readiness flag, connection accounting, UDP
//! registry, health manager, and (feature-gated) reverse/admin state.
//! [`RuntimeState::apply_compiled_config`] is the one canonical reload
//! transaction used by file-backed reload, SIGHUP handling, and embed
//! string/file/compiled reload entry points.

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use eggress_routing::health::HealthManager;
use eggress_routing::upstream::UpstreamRuntime;
use eggress_routing::SharedRoutingService;

use crate::snapshot::{compile_runtime_snapshot, CompiledRuntimeSnapshot};

#[cfg(feature = "operations")]
use super::operations::RuntimeAdminState;
use super::reload::{classify_reload_config, ReloadResult};

pub struct RuntimeState {
    pub snapshot: Arc<ArcSwap<CompiledRuntimeSnapshot>>,
    pub routing: Arc<SharedRoutingService>,
    pub metrics: Arc<dyn eggress_server::SessionMetrics>,
    pub runtime_metrics: Arc<dyn eggress_metrics::RuntimeMetrics>,
    pub readiness: Arc<AtomicBool>,
    pub start_time: Instant,
    pub active_connections: Arc<AtomicU64>,
    pub connection_counter: Arc<AtomicU64>,
    pub admin_local_addr: Arc<Mutex<Option<std::net::SocketAddr>>>,
    pub listener_addrs: Arc<Mutex<Vec<Option<std::net::SocketAddr>>>>,
    #[cfg(feature = "operations")]
    pub(crate) admin_snapshot: Arc<ArcSwap<RuntimeAdminState>>,
    pub health: Arc<Mutex<Option<HealthManager>>>,
    pub health_cancel: CancellationToken,
    pub health_runtime: Mutex<Option<tokio::runtime::Handle>>,
    pub udp_registry: Arc<eggress_udp::registry::UdpAssociationRegistry>,
    pub udp_metrics: Arc<eggress_udp::metrics::UdpMetrics>,
    #[cfg(feature = "extended")]
    pub shadowsocks_metrics: Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>,
    pub udp_tasks: TaskTracker,
    pub transparent_accepted_total: Arc<AtomicU64>,
    pub transparent_original_dst_failed_total: Arc<AtomicU64>,
    #[cfg(feature = "reverse")]
    pub reverse_registry: Arc<eggress_admin::ReverseRegistry>,
    #[cfg(feature = "reverse")]
    pub reverse_metrics: Arc<eggress_protocol_reverse::metrics::ReverseMetrics>,
}

impl RuntimeState {
    pub fn generation(&self) -> u64 {
        self.snapshot.load().generation
    }

    /// Canonical reload transaction shared by file-backed supervisor reload,
    /// SIGHUP handling, and embed string/file reload entry points.
    ///
    /// Applies a newly compiled [`eggress_config::compile::RuntimeConfig`] to
    /// the running state with all side effects centralized:
    /// classification, snapshot compilation, snapshot publication, routing
    /// swap, admin publication, health restart, H2 pool invalidation, and
    /// metrics recording. Failure preserves the prior generation.
    ///
    /// Callers differ only in how `new_config` is obtained (file load vs.
    /// string parse). Supervisors with stored `rt_config` must update that
    /// bookkeeping on `Applied`; the snapshot itself remains authoritative
    /// for the next classification.
    pub fn apply_compiled_config(
        &self,
        new_config: &eggress_config::compile::RuntimeConfig,
    ) -> ReloadResult {
        let prev_snapshot = self.snapshot.load();
        if let Err(reason) = classify_reload_config(
            &prev_snapshot.listeners,
            &prev_snapshot.timeouts,
            prev_snapshot.admin.as_ref(),
            new_config,
        ) {
            self.runtime_metrics.record_reload(false);
            return ReloadResult::Rejected { reason };
        }

        let prev_ref: Option<&CompiledRuntimeSnapshot> = Some(&prev_snapshot);
        let new_snapshot = match compile_runtime_snapshot(new_config, prev_ref) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.runtime_metrics.record_reload(false);
                return ReloadResult::Failed {
                    error: format!("snapshot build: {error}"),
                };
            }
        };

        let upstream_count = new_snapshot.upstreams.len();
        let generation = new_snapshot.generation;

        // Snapshot must be published before the router swap. Readers that
        // observe the new generation via `snapshot.load()` pull the router
        // from that same snapshot Arc, so any reader seeing the new
        // generation also sees the router that belongs to it.
        let new_snapshot = Arc::new(new_snapshot);
        self.snapshot.store(new_snapshot.clone());
        self.routing.swap_arc(new_snapshot.router.clone());
        #[cfg(feature = "operations")]
        self.publish_admin_snapshot(new_snapshot.clone());

        self.restart_health_probes();
        eggress_protocol_http::H2_POOL_REGISTRY.clear();

        self.runtime_metrics.set_config_generation(generation);
        self.runtime_metrics.record_reload(true);

        ReloadResult::Applied {
            generation,
            upstreams: upstream_count,
        }
    }

    #[cfg(feature = "operations")]
    pub(crate) fn publish_admin_snapshot(&self, snapshot: Arc<CompiledRuntimeSnapshot>) {
        let listener_addrs = self.admin_snapshot.load().listener_addrs.clone();
        self.admin_snapshot.store(Arc::new(RuntimeAdminState {
            snapshot,
            listener_addrs,
        }));
    }

    #[cfg(feature = "operations")]
    pub(crate) fn publish_admin_listener_addrs(
        &self,
        snapshot: Arc<CompiledRuntimeSnapshot>,
        listener_addrs: Vec<Option<std::net::SocketAddr>>,
    ) {
        self.admin_snapshot.store(Arc::new(RuntimeAdminState {
            snapshot,
            listener_addrs,
        }));
    }

    /// Restart health probes for the upstreams in the current snapshot.
    pub fn restart_health_probes(&self) {
        let mut guard = self.health.lock().unwrap_or_else(|error| {
            tracing::warn!("health manager state was poisoned; resetting it: {error}");
            let mut guard = error.into_inner();
            *guard = None;
            self.health.clear_poison();
            guard
        });
        if let Some(ref mut health) = *guard {
            health.stop_all();
        }
        let upstreams: Vec<Arc<UpstreamRuntime>> =
            self.snapshot.load().upstreams.values().cloned().collect();
        if !upstreams.is_empty() {
            let mut health = HealthManager::new(self.health_cancel.clone());
            if let Some(handle) = self
                .health_runtime
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone()
            {
                health.start_probes_on(&handle, &upstreams);
            }
            *guard = Some(health);
        } else {
            *guard = None;
        }
    }
}
