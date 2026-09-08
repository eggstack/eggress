//! Shadowsocks metrics bridge (feature `extended`).
//!
//! [`eggress_protocol_shadowsocks::ShadowsocksMetrics`] atomics are canonical
//! for Shadowsocks TCP/UDP counters; promotion into the exposition mirrors
//! follows the same delta rules as the UDP bridge.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use eggress_protocol_shadowsocks::ShadowsocksMetrics;

use super::registry::MetricsRegistry;

#[cfg(feature = "extended")]
#[derive(Default)]
pub(crate) struct BridgedShadowsocksSnapshot {
    tcp_sessions_total: u64,
    tcp_upstream_sessions_total: u64,
    tcp_decrypt_failures_total: u64,
    tcp_frame_parse_failures_total: u64,
    tcp_unsupported_method_rejects_total: u64,
    udp_packets_in_total: u64,
    udp_packets_out_total: u64,
    udp_bytes_in_total: u64,
    udp_bytes_out_total: u64,
    udp_decrypt_failures_total: u64,
    udp_unsupported_method_rejects_total: u64,
}

impl MetricsRegistry {
    /// Bridge a shared `ShadowsocksMetrics` instance so that `render_prometheus()`
    /// exposes live Shadowsocks protocol-specific counters and gauges.
    pub fn set_shadowsocks_metrics(&self, metrics: Arc<ShadowsocksMetrics>) {
        let snapshot = BridgedShadowsocksSnapshot {
            tcp_sessions_total: metrics
                .tcp_sessions_total
                .load(std::sync::atomic::Ordering::Relaxed),
            tcp_upstream_sessions_total: metrics
                .tcp_upstream_sessions_total
                .load(std::sync::atomic::Ordering::Relaxed),
            tcp_decrypt_failures_total: metrics
                .tcp_decrypt_failures_total
                .load(std::sync::atomic::Ordering::Relaxed),
            tcp_frame_parse_failures_total: metrics
                .tcp_frame_parse_failures_total
                .load(std::sync::atomic::Ordering::Relaxed),
            tcp_unsupported_method_rejects_total: metrics
                .tcp_unsupported_method_rejects_total
                .load(std::sync::atomic::Ordering::Relaxed),
            udp_packets_in_total: metrics
                .udp_packets_in_total
                .load(std::sync::atomic::Ordering::Relaxed),
            udp_packets_out_total: metrics
                .udp_packets_out_total
                .load(std::sync::atomic::Ordering::Relaxed),
            udp_bytes_in_total: metrics
                .udp_bytes_in_total
                .load(std::sync::atomic::Ordering::Relaxed),
            udp_bytes_out_total: metrics
                .udp_bytes_out_total
                .load(std::sync::atomic::Ordering::Relaxed),
            udp_decrypt_failures_total: metrics
                .udp_decrypt_failures_total
                .load(std::sync::atomic::Ordering::Relaxed),
            udp_unsupported_method_rejects_total: metrics
                .udp_unsupported_method_rejects_total
                .load(std::sync::atomic::Ordering::Relaxed),
        };
        *self
            .bridged_shadowsocks_metrics
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some((metrics, snapshot));
    }

    /// Promote bridged [`ShadowsocksMetrics`] counters into exposition mirrors.
    pub(crate) fn sync_shadowsocks_bridges(&self) {
        if let Some((metrics, prev)) = self
            .bridged_shadowsocks_metrics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            self.ss_tcp_sessions_active.set(
                metrics
                    .tcp_sessions_active
                    .load(Ordering::Relaxed)
                    .min(i64::MAX as u64) as i64,
            );
            self.ss_tcp_active_flows.set(
                metrics
                    .tcp_active_flows
                    .load(Ordering::Relaxed)
                    .min(i64::MAX as u64) as i64,
            );
            self.ss_udp_active_flows.set(
                metrics
                    .udp_active_flows
                    .load(Ordering::Relaxed)
                    .min(i64::MAX as u64) as i64,
            );

            let cur = metrics.tcp_sessions_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.tcp_sessions_total);
            if delta > 0 {
                self.ss_tcp_sessions_total.inc_by(delta);
            }
            prev.tcp_sessions_total = cur;

            let cur = metrics.tcp_upstream_sessions_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.tcp_upstream_sessions_total);
            if delta > 0 {
                self.ss_tcp_upstream_sessions_total.inc_by(delta);
            }
            prev.tcp_upstream_sessions_total = cur;

            let cur = metrics.tcp_decrypt_failures_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.tcp_decrypt_failures_total);
            if delta > 0 {
                self.ss_tcp_decrypt_failures_total.inc_by(delta);
            }
            prev.tcp_decrypt_failures_total = cur;

            let cur = metrics
                .tcp_frame_parse_failures_total
                .load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.tcp_frame_parse_failures_total);
            if delta > 0 {
                self.ss_tcp_frame_parse_failures_total.inc_by(delta);
            }
            prev.tcp_frame_parse_failures_total = cur;

            let cur = metrics
                .tcp_unsupported_method_rejects_total
                .load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.tcp_unsupported_method_rejects_total);
            if delta > 0 {
                self.ss_tcp_unsupported_method_rejects_total.inc_by(delta);
            }
            prev.tcp_unsupported_method_rejects_total = cur;

            let cur = metrics.udp_packets_in_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.udp_packets_in_total);
            if delta > 0 {
                self.ss_udp_packets_in_total.inc_by(delta);
            }
            prev.udp_packets_in_total = cur;

            let cur = metrics.udp_packets_out_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.udp_packets_out_total);
            if delta > 0 {
                self.ss_udp_packets_out_total.inc_by(delta);
            }
            prev.udp_packets_out_total = cur;

            let cur = metrics.udp_bytes_in_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.udp_bytes_in_total);
            if delta > 0 {
                self.ss_udp_bytes_in_total.inc_by(delta);
            }
            prev.udp_bytes_in_total = cur;

            let cur = metrics.udp_bytes_out_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.udp_bytes_out_total);
            if delta > 0 {
                self.ss_udp_bytes_out_total.inc_by(delta);
            }
            prev.udp_bytes_out_total = cur;

            let cur = metrics.udp_decrypt_failures_total.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.udp_decrypt_failures_total);
            if delta > 0 {
                self.ss_udp_decrypt_failures_total.inc_by(delta);
            }
            prev.udp_decrypt_failures_total = cur;

            let cur = metrics
                .udp_unsupported_method_rejects_total
                .load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev.udp_unsupported_method_rejects_total);
            if delta > 0 {
                self.ss_udp_unsupported_method_rejects_total.inc_by(delta);
            }
            prev.udp_unsupported_method_rejects_total = cur;
        }
    }
}
