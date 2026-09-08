//! Runtime/reload/platform recording: the runtime-facing metrics domain.
//!
//! [`RuntimeMetrics`] is the narrow interface the supervisor, reload paths,
//! and embed API use for non-session events. `eggress-server` never sees it:
//! connection tasks receive only `SessionMetrics`, so runtime/admin concerns
//! cannot leak into the data-plane session path.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::registry::MetricsRegistry;

/// Runtime-only metrics events, implemented by [`MetricsRegistry`].
///
/// Kept separate from `eggress_server::SessionMetrics` (session, route,
/// upstream, auth) so the server crate does not own reload, platform, or
/// exposition concerns.
pub trait RuntimeMetrics: Send + Sync {
    /// Record a reload attempt (`success == false` also bumps reload_failures).
    fn record_reload(&self, success: bool);
    /// Publish the active config generation.
    fn set_config_generation(&self, generation: u64);
    /// A platform capability probe (e.g. transparent proxy support) failed.
    fn record_platform_capability_check_failure(&self);
    /// A Unix listener connection was accepted.
    fn record_unix_listener_connection_accepted(&self);
    /// A Unix listener bind failed.
    fn record_unix_listener_bind_failure(&self);
    /// A transparent connection was accepted (direct-registry fallback; prefer
    /// bridged supervisor atomics when a bridge is installed).
    fn record_transparent_connection_accepted(&self);
    /// A transparent original-destination lookup failed (direct fallback).
    fn record_transparent_original_dst_failed(&self);
    /// A transparent connection was rejected by routing (direct fallback).
    fn record_transparent_route_reject(&self);
    /// A UDP association was created (direct-registry fallback).
    ///
    /// Production runtime associations are recorded once via the canonical
    /// subsystem counter by the relay loop and promoted by the bridge; that
    /// path must not also call this method for the same association or the
    /// total double-counts. Use this only where no subsystem bridge owns the
    /// family (e.g. unit tests without a bridge installed).
    fn record_udp_association_created(&self);
    /// Render the full Prometheus exposition (idempotent w.r.t. stored totals:
    /// repeated renders with no new activity change nothing).
    fn render_prometheus(&self) -> String;
}

impl RuntimeMetrics for MetricsRegistry {
    fn record_reload(&self, success: bool) {
        MetricsRegistry::record_reload(self, success);
    }

    fn set_config_generation(&self, generation: u64) {
        MetricsRegistry::set_config_generation(self, generation);
    }

    fn record_platform_capability_check_failure(&self) {
        self.platform_capability_check_failures.inc();
    }

    fn record_unix_listener_connection_accepted(&self) {
        self.unix_listener_connections_accepted.inc();
    }

    fn record_unix_listener_bind_failure(&self) {
        self.unix_listener_bind_failures.inc();
    }

    fn record_transparent_connection_accepted(&self) {
        MetricsRegistry::record_transparent_connection_accepted(self);
    }

    fn record_transparent_original_dst_failed(&self) {
        MetricsRegistry::record_transparent_original_dst_failed(self);
    }

    fn record_transparent_route_reject(&self) {
        MetricsRegistry::record_transparent_route_reject(self);
    }

    fn record_udp_association_created(&self) {
        MetricsRegistry::record_udp_association_created(self);
    }

    fn render_prometheus(&self) -> String {
        MetricsRegistry::render_prometheus(self)
    }
}

impl MetricsRegistry {
    pub fn set_config_generation(&self, generation: u64) {
        self.config_generation
            .set(generation.min(i64::MAX as u64) as i64);
    }

    pub fn record_reload(&self, success: bool) {
        self.reload_total.inc();
        if !success {
            self.reload_failures.inc();
        }
    }

    /// Bridge the supervisor's transparent proxy atomic counters so that
    /// `render_prometheus()` exposes live transparent proxy counters.
    pub fn set_transparent_counters(&self, accepted: Arc<AtomicU64>, dst_failed: Arc<AtomicU64>) {
        *self
            .transparent_accepted_bridged
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(accepted);
        *self
            .transparent_dst_failed_bridged
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(dst_failed);
    }

    pub fn record_transparent_connection_accepted(&self) {
        self.transparent_connections_accepted.inc();
    }

    pub fn record_transparent_original_dst_failed(&self) {
        self.transparent_original_dst_failed.inc();
    }

    pub fn record_transparent_route_reject(&self) {
        self.transparent_route_rejects.inc();
    }

    pub fn record_unix_listener_connection_accepted(&self) {
        self.unix_listener_connections_accepted.inc();
    }

    pub fn record_unix_listener_bind_failure(&self) {
        self.unix_listener_bind_failures.inc();
    }

    pub fn record_platform_capability_check_failure(&self) {
        self.platform_capability_check_failures.inc();
    }

    /// Promote bridged supervisor transparent-proxy atomics into the
    /// `transparent_*` counters. Gauges are current-state and set directly;
    /// cumulative counters advance by saturating delta so repeated renders
    /// never double-count.
    pub(crate) fn sync_transparent(&self) {
        if let Some(accepted) = self
            .transparent_accepted_bridged
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let cur = accepted.load(Ordering::Relaxed);
            let mut prev = self
                .transparent_prev_accepted
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let delta = cur.saturating_sub(*prev);
            if delta > 0 {
                self.transparent_connections_accepted.inc_by(delta);
            }
            *prev = cur;
        }
        if let Some(dst_failed) = self
            .transparent_dst_failed_bridged
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let cur = dst_failed.load(Ordering::Relaxed);
            let mut prev = self
                .transparent_prev_dst_failed
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let delta = cur.saturating_sub(*prev);
            if delta > 0 {
                self.transparent_original_dst_failed.inc_by(delta);
            }
            *prev = cur;
        }
    }
}
