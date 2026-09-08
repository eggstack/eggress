//! UDP relay metrics: bridge and direct recording for the UDP subsystem.
//!
//! [`eggress_udp::metrics::UdpMetrics`] atomics are canonical for relay
//! counters (hot-path, lock-free). [`MetricsRegistry::set_udp_metrics`]
//! installs the bridge; [`MetricsRegistry::sync_udp_bridges`] promotes
//! deltas into the exposition mirrors at render time. Gauges
//! (`*_active`) track current state and are set directly.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use eggress_udp::metrics::UdpMetrics;

use super::labels::DecodeErrorLabels;
use super::registry::MetricsRegistry;

#[derive(Default)]
pub(crate) struct BridgedUdpSnapshot {
    associations_total: u64,
    association_failures: u64,
    association_timeouts: u64,
    packets_up: u64,
    packets_down: u64,
    bytes_up: u64,
    bytes_down: u64,
    dropped_packets: u64,
    dropped_encode_errors: u64,
    dropped_send_errors: u64,
    dropped_response_channel_full: u64,
    target_flows_total: u64,
    decode_errors: u64,
    upstream_associations_total: u64,
    upstream_packets_up: u64,
    upstream_packets_down: u64,
    upstream_bytes_up: u64,
    upstream_bytes_down: u64,
    upstream_failures: u64,
    standalone_flows_total: u64,
    standalone_packets_in: u64,
    standalone_packets_out: u64,
    standalone_bytes_in: u64,
    standalone_bytes_out: u64,
    standalone_malformed_datagrams: u64,
    standalone_rejected_datagrams: u64,
    standalone_flow_reaps: u64,
}

impl MetricsRegistry {
    /// Bridge a shared `UdpMetrics` instance so that `render_prometheus()`
    /// exposes live relay counters (packets, bytes, drops, decode errors, etc.).
    pub fn set_udp_metrics(&self, metrics: Arc<UdpMetrics>) {
        let snapshot = BridgedUdpSnapshot {
            associations_total: metrics
                .associations_total
                .load(std::sync::atomic::Ordering::Relaxed),
            association_failures: metrics
                .association_failures
                .load(std::sync::atomic::Ordering::Relaxed),
            association_timeouts: metrics
                .association_timeouts
                .load(std::sync::atomic::Ordering::Relaxed),
            packets_up: metrics
                .packets_up
                .load(std::sync::atomic::Ordering::Relaxed),
            packets_down: metrics
                .packets_down
                .load(std::sync::atomic::Ordering::Relaxed),
            bytes_up: metrics.bytes_up.load(std::sync::atomic::Ordering::Relaxed),
            bytes_down: metrics
                .bytes_down
                .load(std::sync::atomic::Ordering::Relaxed),
            dropped_packets: metrics
                .dropped_packets
                .load(std::sync::atomic::Ordering::Relaxed),
            dropped_encode_errors: metrics
                .dropped_encode_errors
                .load(std::sync::atomic::Ordering::Relaxed),
            dropped_send_errors: metrics
                .dropped_send_errors
                .load(std::sync::atomic::Ordering::Relaxed),
            dropped_response_channel_full: metrics
                .dropped_response_channel_full
                .load(std::sync::atomic::Ordering::Relaxed),
            target_flows_total: metrics
                .target_flows_total
                .load(std::sync::atomic::Ordering::Relaxed),
            decode_errors: metrics
                .decode_errors
                .load(std::sync::atomic::Ordering::Relaxed),
            upstream_associations_total: metrics
                .upstream_associations_total
                .load(std::sync::atomic::Ordering::Relaxed),
            upstream_packets_up: metrics
                .upstream_packets_up
                .load(std::sync::atomic::Ordering::Relaxed),
            upstream_packets_down: metrics
                .upstream_packets_down
                .load(std::sync::atomic::Ordering::Relaxed),
            upstream_bytes_up: metrics
                .upstream_bytes_up
                .load(std::sync::atomic::Ordering::Relaxed),
            upstream_bytes_down: metrics
                .upstream_bytes_down
                .load(std::sync::atomic::Ordering::Relaxed),
            upstream_failures: metrics
                .upstream_failures
                .load(std::sync::atomic::Ordering::Relaxed),
            standalone_flows_total: metrics
                .standalone_flows_total
                .load(std::sync::atomic::Ordering::Relaxed),
            standalone_packets_in: metrics
                .standalone_packets_in
                .load(std::sync::atomic::Ordering::Relaxed),
            standalone_packets_out: metrics
                .standalone_packets_out
                .load(std::sync::atomic::Ordering::Relaxed),
            standalone_bytes_in: metrics
                .standalone_bytes_in
                .load(std::sync::atomic::Ordering::Relaxed),
            standalone_bytes_out: metrics
                .standalone_bytes_out
                .load(std::sync::atomic::Ordering::Relaxed),
            standalone_malformed_datagrams: metrics
                .standalone_malformed_datagrams
                .load(std::sync::atomic::Ordering::Relaxed),
            standalone_rejected_datagrams: metrics
                .standalone_rejected_datagrams
                .load(std::sync::atomic::Ordering::Relaxed),
            standalone_flow_reaps: metrics
                .standalone_flow_reaps
                .load(std::sync::atomic::Ordering::Relaxed),
        };
        *self
            .bridged_udp_metrics
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some((metrics, snapshot));
    }

    pub fn record_udp_association_created(&self) {
        self.udp_associations_active.inc();
        self.udp_associations_total.inc();
    }

    pub fn record_udp_association_closed(&self) {
        self.udp_associations_active.dec();
    }

    pub fn record_udp_association_failure(&self) {
        self.udp_association_failures.inc();
    }

    pub fn record_udp_packet_up(&self, bytes: u64) {
        self.udp_packets_up_total.inc();
        self.udp_bytes_up_total.inc_by(bytes);
    }

    pub fn record_udp_packet_down(&self, bytes: u64) {
        self.udp_packets_down_total.inc();
        self.udp_bytes_down_total.inc_by(bytes);
    }

    pub fn record_udp_dropped(&self) {
        self.udp_dropped_packets_total.inc();
    }

    pub fn record_udp_target_flow_created(&self) {
        self.udp_target_flows_active.inc();
        self.udp_target_flows_total.inc();
    }

    pub fn record_udp_target_flow_closed(&self) {
        self.udp_target_flows_active.dec();
    }

    pub fn record_udp_decode_error(&self, kind: &str) {
        self.udp_decode_errors_total
            .get_or_create(&DecodeErrorLabels {
                kind: kind.to_string(),
            })
            .inc();
    }

    pub fn record_udp_unsupported_upstream(&self) {
        self.udp_unsupported_upstream_total.inc();
    }

    pub fn record_udp_upstream_association_created(&self) {
        self.udp_upstream_associations_active.inc();
        self.udp_upstream_associations_total.inc();
    }

    pub fn record_udp_upstream_association_closed(&self) {
        self.udp_upstream_associations_active.dec();
    }

    pub fn record_udp_upstream_failure(&self) {
        self.udp_upstream_failures_total.inc();
    }

    pub fn record_udp_upstream_packet_up(&self, bytes: u64) {
        self.udp_upstream_packets_up_total.inc();
        self.udp_upstream_bytes_up_total.inc_by(bytes);
    }

    pub fn record_udp_upstream_packet_down(&self, bytes: u64) {
        self.udp_upstream_packets_down_total.inc();
        self.udp_upstream_bytes_down_total.inc_by(bytes);
    }

    pub fn udp_associations_active_gauge(&self) -> i64 {
        self.udp_associations_active.get()
    }

    pub fn udp_associations_total_count(&self) -> u64 {
        self.udp_associations_total.get()
    }

    pub fn udp_target_flows_active_gauge(&self) -> i64 {
        self.udp_target_flows_active.get()
    }

    pub fn udp_upstream_associations_active_gauge(&self) -> i64 {
        self.udp_upstream_associations_active.get()
    }

    /// Promote bridged [`UdpMetrics`] counters into the exposition mirrors.
    /// Each counter advances by saturating delta since the previous render;
    /// gauges are set from current values. Locking is poison-tolerant.
    pub(crate) fn sync_udp_bridges(&self) {
        // Sync live UDP relay counters from the bridged UdpMetrics into
        // Prometheus gauges/counters before encoding.
        if let Some((metrics, prev)) = self
            .bridged_udp_metrics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            // Gauges: set directly (active counts are current-state, not cumulative)
            self.udp_associations_active.set(
                metrics
                    .associations_active
                    .load(Ordering::Relaxed)
                    .min(i64::MAX as u64) as i64,
            );
            self.udp_target_flows_active.set(
                metrics
                    .target_flows_active
                    .load(Ordering::Relaxed)
                    .min(i64::MAX as u64) as i64,
            );
            self.udp_upstream_associations_active.set(
                metrics
                    .upstream_associations_active
                    .load(Ordering::Relaxed)
                    .min(i64::MAX as u64) as i64,
            );

            // Counters: increment by delta since last render
            let cur_total = metrics.associations_total.load(Ordering::Relaxed);
            let delta = cur_total.saturating_sub(prev.associations_total);
            if delta > 0 {
                self.udp_associations_total.inc_by(delta);
            }
            prev.associations_total = cur_total;

            let cur = metrics.association_failures.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.association_failures);
            if delta > 0 {
                self.udp_association_failures.inc_by(delta);
            }
            prev.association_failures = cur;

            let cur = metrics.association_timeouts.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.association_timeouts);
            if delta > 0 {
                self.udp_association_timeouts.inc_by(delta);
            }
            prev.association_timeouts = cur;

            let cur = metrics.packets_up.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.packets_up);
            if delta > 0 {
                self.udp_packets_up_total.inc_by(delta);
            }
            prev.packets_up = cur;

            let cur = metrics.packets_down.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.packets_down);
            if delta > 0 {
                self.udp_packets_down_total.inc_by(delta);
            }
            prev.packets_down = cur;

            let cur = metrics.bytes_up.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.bytes_up);
            if delta > 0 {
                self.udp_bytes_up_total.inc_by(delta);
            }
            prev.bytes_up = cur;

            let cur = metrics.bytes_down.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.bytes_down);
            if delta > 0 {
                self.udp_bytes_down_total.inc_by(delta);
            }
            prev.bytes_down = cur;

            let cur = metrics.dropped_packets.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.dropped_packets);
            if delta > 0 {
                self.udp_dropped_packets_total.inc_by(delta);
            }
            prev.dropped_packets = cur;

            let cur = metrics.dropped_encode_errors.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.dropped_encode_errors);
            if delta > 0 {
                self.udp_dropped_encode_errors_total.inc_by(delta);
            }
            prev.dropped_encode_errors = cur;

            let cur = metrics.dropped_send_errors.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.dropped_send_errors);
            if delta > 0 {
                self.udp_dropped_send_errors_total.inc_by(delta);
            }
            prev.dropped_send_errors = cur;

            let cur = metrics
                .dropped_response_channel_full
                .load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.dropped_response_channel_full);
            if delta > 0 {
                self.udp_dropped_response_channel_full_total.inc_by(delta);
            }
            prev.dropped_response_channel_full = cur;

            let cur = metrics.target_flows_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.target_flows_total);
            if delta > 0 {
                self.udp_target_flows_total.inc_by(delta);
            }
            prev.target_flows_total = cur;

            let cur = metrics.decode_errors.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.decode_errors);
            if delta > 0 {
                // Total decode errors across all kinds
                self.udp_decode_errors_total
                    .get_or_create(&DecodeErrorLabels {
                        kind: "total".to_string(),
                    })
                    .inc_by(delta);
            }
            prev.decode_errors = cur;

            let cur = metrics.upstream_associations_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.upstream_associations_total);
            if delta > 0 {
                self.udp_upstream_associations_total.inc_by(delta);
            }
            prev.upstream_associations_total = cur;

            let cur = metrics.upstream_packets_up.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.upstream_packets_up);
            if delta > 0 {
                self.udp_upstream_packets_up_total.inc_by(delta);
            }
            prev.upstream_packets_up = cur;

            let cur = metrics.upstream_packets_down.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.upstream_packets_down);
            if delta > 0 {
                self.udp_upstream_packets_down_total.inc_by(delta);
            }
            prev.upstream_packets_down = cur;

            let cur = metrics.upstream_bytes_up.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.upstream_bytes_up);
            if delta > 0 {
                self.udp_upstream_bytes_up_total.inc_by(delta);
            }
            prev.upstream_bytes_up = cur;

            let cur = metrics.upstream_bytes_down.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.upstream_bytes_down);
            if delta > 0 {
                self.udp_upstream_bytes_down_total.inc_by(delta);
            }
            prev.upstream_bytes_down = cur;

            let cur = metrics.upstream_failures.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.upstream_failures);
            if delta > 0 {
                self.udp_upstream_failures_total.inc_by(delta);
            }
            prev.upstream_failures = cur;

            // Standalone UDP metrics
            self.standalone_udp_flows_active.set(
                metrics
                    .standalone_flows_active
                    .load(Ordering::Relaxed)
                    .min(i64::MAX as u64) as i64,
            );

            let cur = metrics.standalone_flows_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.standalone_flows_total);
            if delta > 0 {
                self.standalone_udp_flows_total.inc_by(delta);
            }
            prev.standalone_flows_total = cur;

            let cur = metrics.standalone_packets_in.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.standalone_packets_in);
            if delta > 0 {
                self.standalone_udp_packets_in_total.inc_by(delta);
            }
            prev.standalone_packets_in = cur;

            let cur = metrics.standalone_packets_out.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.standalone_packets_out);
            if delta > 0 {
                self.standalone_udp_packets_out_total.inc_by(delta);
            }
            prev.standalone_packets_out = cur;

            let cur = metrics.standalone_bytes_in.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.standalone_bytes_in);
            if delta > 0 {
                self.standalone_udp_bytes_in_total.inc_by(delta);
            }
            prev.standalone_bytes_in = cur;

            let cur = metrics.standalone_bytes_out.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.standalone_bytes_out);
            if delta > 0 {
                self.standalone_udp_bytes_out_total.inc_by(delta);
            }
            prev.standalone_bytes_out = cur;

            let cur = metrics
                .standalone_malformed_datagrams
                .load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.standalone_malformed_datagrams);
            if delta > 0 {
                self.standalone_udp_malformed_total.inc_by(delta);
            }
            prev.standalone_malformed_datagrams = cur;

            let cur = metrics
                .standalone_rejected_datagrams
                .load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.standalone_rejected_datagrams);
            if delta > 0 {
                self.standalone_udp_rejected_total.inc_by(delta);
            }
            prev.standalone_rejected_datagrams = cur;

            let cur = metrics.standalone_flow_reaps.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.standalone_flow_reaps);
            if delta > 0 {
                self.standalone_udp_flow_reaps_total.inc_by(delta);
            }
            prev.standalone_flow_reaps = cur;
        }
    }
}
