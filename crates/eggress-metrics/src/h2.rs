//! HTTP/2 metrics: global protocol atomics, direct recording, snapshots.
//!
//! `eggress_protocol_http::H2_PROTOCOL_METRICS` atomics are canonical for H2
//! connection/stream/goaway/handshake/auth/flow-control/pool/byte counters.
//! [`MetricsRegistry::sync_h2`] promotes deltas into the exposition mirrors
//! (including the labeled `h2_streams_total` family, which stays canonical
//! here because the protocol layer owns no per-label state). Active gauges
//! are derived live as `opened - closed`.

use std::sync::atomic::Ordering;

use super::labels::H2StreamLabels;
use super::registry::MetricsRegistry;

#[derive(Debug, Clone)]
pub struct H2MetricsSnapshot {
    pub connections_active: u64,
    pub connections_total: u64,
    pub streams_active: u64,
    pub goaway_total: u64,
    pub handshake_failures_total: u64,
    pub auth_failures_total: u64,
    pub flow_control_stalls_total: u64,
    pub pool_exhausted_total: u64,
    pub bytes_relayed_total: u64,
}

impl MetricsRegistry {
    pub fn record_h2_connection_opened(&self) {
        self.h2_connections_active.inc();
        self.h2_connections_total.inc();
    }

    pub fn record_h2_connection_closed(&self) {
        self.h2_connections_active.dec();
    }

    pub fn record_h2_stream_opened(&self, upstream_id: &str, outcome: &str) {
        self.h2_streams_active.inc();
        self.h2_streams_total
            .get_or_create(&H2StreamLabels {
                upstream_id: upstream_id.to_string(),
                outcome: outcome.to_string(),
            })
            .inc();
    }

    pub fn record_h2_stream_closed(&self) {
        self.h2_streams_active.dec();
    }

    pub fn record_h2_goaway(&self) {
        self.h2_goaway_total.inc();
    }

    pub fn record_h2_handshake_failure(&self) {
        self.h2_handshake_failures_total.inc();
    }

    pub fn record_h2_auth_failure(&self) {
        self.h2_auth_failures_total.inc();
    }

    pub fn record_h2_flow_control_stall(&self) {
        self.h2_flow_control_stalls_total.inc();
    }

    pub fn record_h2_pool_exhausted(&self) {
        self.h2_pool_exhausted_total.inc();
    }

    pub fn record_h2_bytes_relayed(&self, bytes: u64) {
        self.h2_bytes_relayed_total.inc_by(bytes);
    }

    pub fn h2_snapshot(&self) -> H2MetricsSnapshot {
        H2MetricsSnapshot {
            // Clamp active-resource gauges so an incidental inc/dec bug
            // cannot surface as a huge unsigned value.
            connections_active: self.h2_connections_active.get().max(0) as u64,
            connections_total: self.h2_connections_total.get(),
            streams_active: self.h2_streams_active.get().max(0) as u64,
            goaway_total: self.h2_goaway_total.get(),
            handshake_failures_total: self.h2_handshake_failures_total.get(),
            auth_failures_total: self.h2_auth_failures_total.get(),
            flow_control_stalls_total: self.h2_flow_control_stalls_total.get(),
            pool_exhausted_total: self.h2_pool_exhausted_total.get(),
            bytes_relayed_total: self.h2_bytes_relayed_total.get(),
        }
    }

    /// Promote global H2 atomics into exposition mirrors (delta rules as
    /// elsewhere; active gauges derived live as opened-minus-closed).
    pub(crate) fn sync_h2(&self) {
        {
            {
                use eggress_protocol_http::H2_PROTOCOL_METRICS;

                let cur_opened = H2_PROTOCOL_METRICS
                    .connections_opened
                    .load(Ordering::Relaxed);
                let cur_closed = H2_PROTOCOL_METRICS
                    .connections_closed
                    .load(Ordering::Relaxed);
                let mut prev_opened = self
                    .h2_prev_connections_opened
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let delta_opened = cur_opened.saturating_sub(*prev_opened);
                if delta_opened > 0 {
                    self.h2_connections_total.inc_by(delta_opened);
                }
                *prev_opened = cur_opened;
                // active = total opened - total closed (capped at 0)
                let active = cur_opened.saturating_sub(cur_closed);
                self.h2_connections_active
                    .set(active.min(i64::MAX as u64) as i64);

                let cur_streams_opened = H2_PROTOCOL_METRICS.streams_opened.load(Ordering::Relaxed);
                let mut prev = self
                    .h2_prev_streams_opened
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let delta = cur_streams_opened.saturating_sub(*prev);
                if delta > 0 {
                    self.h2_streams_total
                        .get_or_create(&H2StreamLabels {
                            upstream_id: "h2".to_string(),
                            outcome: "opened".to_string(),
                        })
                        .inc_by(delta);
                }
                *prev = cur_streams_opened;

                let cur_streams_closed = H2_PROTOCOL_METRICS.streams_closed.load(Ordering::Relaxed);
                let mut prev = self
                    .h2_prev_streams_closed
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let delta = cur_streams_closed.saturating_sub(*prev);
                if delta > 0 {
                    self.h2_streams_total
                        .get_or_create(&H2StreamLabels {
                            upstream_id: "h2".to_string(),
                            outcome: "closed".to_string(),
                        })
                        .inc_by(delta);
                }
                *prev = cur_streams_closed;

                let active_streams = cur_streams_opened.saturating_sub(cur_streams_closed);
                self.h2_streams_active
                    .set(active_streams.min(i64::MAX as u64) as i64);

                let cur = H2_PROTOCOL_METRICS.goaway_received.load(Ordering::Relaxed);
                let mut prev = self
                    .h2_prev_goaway
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let delta = cur.saturating_sub(*prev);
                if delta > 0 {
                    self.h2_goaway_total.inc_by(delta);
                }
                *prev = cur;

                let cur = H2_PROTOCOL_METRICS
                    .handshake_failures
                    .load(Ordering::Relaxed);
                let mut prev = self
                    .h2_prev_handshake_failures
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let delta = cur.saturating_sub(*prev);
                if delta > 0 {
                    self.h2_handshake_failures_total.inc_by(delta);
                }
                *prev = cur;

                let cur = H2_PROTOCOL_METRICS.auth_failures.load(Ordering::Relaxed);
                let mut prev = self
                    .h2_prev_auth_failures
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let delta = cur.saturating_sub(*prev);
                if delta > 0 {
                    self.h2_auth_failures_total.inc_by(delta);
                }
                *prev = cur;

                let cur = H2_PROTOCOL_METRICS
                    .flow_control_stalls
                    .load(Ordering::Relaxed);
                let mut prev = self
                    .h2_prev_flow_control_stalls
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let delta = cur.saturating_sub(*prev);
                if delta > 0 {
                    self.h2_flow_control_stalls_total.inc_by(delta);
                }
                *prev = cur;

                let cur = H2_PROTOCOL_METRICS.pool_exhausted.load(Ordering::Relaxed);
                let mut prev = self
                    .h2_prev_pool_exhausted
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let delta = cur.saturating_sub(*prev);
                if delta > 0 {
                    self.h2_pool_exhausted_total.inc_by(delta);
                }
                *prev = cur;

                let cur = H2_PROTOCOL_METRICS.bytes_relayed.load(Ordering::Relaxed);
                let mut prev = self
                    .h2_prev_bytes_relayed
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let delta = cur.saturating_sub(*prev);
                if delta > 0 {
                    self.h2_bytes_relayed_total.inc_by(delta);
                }
                *prev = cur;
            }
        }
    }
}
