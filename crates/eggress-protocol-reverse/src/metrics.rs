use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;

use crate::ControlState;

/// Reverse proxy protocol metrics.
///
/// Thread-safe counters for control connection lifecycle, stream events,
/// and error tracking. Pass `Arc<ReverseMetrics>` to `ReverseServer` or
/// `ReverseClient` via the `set_metrics` builder.
#[derive(Debug)]
pub struct ReverseMetrics {
    /// Current count of accepted control connections.
    pub control_connections_active: AtomicU64,
    /// Cumulative count of accepted control connections.
    pub control_connections_accepted_total: AtomicU64,
    /// Cumulative count of rejected control connections (auth failures).
    pub control_connections_rejected_total: AtomicU64,
    /// Cumulative client reconnect attempts.
    pub control_reconnects_total: AtomicU64,
    /// Cumulative auth failures (may overlap with rejected control connections
    /// when the auth phase is the rejection cause).
    pub auth_failures_total: AtomicU64,
    /// Cumulative heartbeat / keepalive failures detected.
    pub heartbeat_failures_total: AtomicU64,
    /// Cumulative drain operations completed.
    pub drain_total: AtomicU64,
    /// Cumulative drain duration in milliseconds.
    pub drain_duration_ms_total: AtomicU64,
    /// Control sessions initiated.
    pub streams_opened_total: AtomicU64,
    /// Control sessions completed cleanly.
    pub streams_closed_total: AtomicU64,
    /// Total bytes relayed through control channels.
    pub stream_bytes_total: AtomicU64,
    /// Per-state cumulative counts of time spent in each state (milliseconds).
    state_time_ms: Mutex<[u64; 6]>,
    /// Most recent error message (truncated to 256 chars).
    pub last_error: Mutex<Option<String>>,
}

const STATE_DISCONNECTED: usize = 0;
const STATE_CONNECTING: usize = 1;
const STATE_AUTHENTICATING: usize = 2;
const STATE_READY: usize = 3;
const STATE_DRAINING: usize = 4;
const STATE_CLOSED: usize = 5;

/// Serializable snapshot of reverse proxy metrics.
#[derive(Debug, Clone, Serialize)]
pub struct ReverseMetricsSnapshot {
    pub control_connections_active: u64,
    pub control_connections_accepted_total: u64,
    pub control_connections_rejected_total: u64,
    pub control_reconnects_total: u64,
    pub auth_failures_total: u64,
    pub heartbeat_failures_total: u64,
    pub drain_total: u64,
    pub drain_duration_ms_total: u64,
    pub streams_opened_total: u64,
    pub streams_closed_total: u64,
    pub stream_bytes_total: u64,
    /// Cumulative time spent in each ControlState, in milliseconds.
    /// Index order: [Disconnected, Connecting, Authenticating, Ready, Draining, Closed]
    pub state_time_ms: [u64; 6],
    pub last_error: Option<String>,
}

impl ReverseMetricsSnapshot {
    /// Format a human-readable summary for admin or log output.
    pub fn display_summary(&self) -> String {
        format!(
            "reverse: active={} accepted={} rejected={} reconnects={} \
             auth_failures={} heartbeat_failures={} drain_total={} drain_ms={} \
             streams_open={} streams_closed={} bytes={} last_error={}",
            self.control_connections_active,
            self.control_connections_accepted_total,
            self.control_connections_rejected_total,
            self.control_reconnects_total,
            self.auth_failures_total,
            self.heartbeat_failures_total,
            self.drain_total,
            self.drain_duration_ms_total,
            self.streams_opened_total,
            self.streams_closed_total,
            self.stream_bytes_total,
            self.last_error.as_deref().unwrap_or("(none)"),
        )
    }

    /// Look up cumulative milliseconds spent in the given state.
    pub fn state_ms(&self, state: ControlState) -> u64 {
        self.state_time_ms[state_index(state)]
    }
}

fn state_index(state: ControlState) -> usize {
    match state {
        ControlState::Disconnected => STATE_DISCONNECTED,
        ControlState::Connecting => STATE_CONNECTING,
        ControlState::Authenticating => STATE_AUTHENTICATING,
        ControlState::Ready => STATE_READY,
        ControlState::Draining => STATE_DRAINING,
        ControlState::Closed => STATE_CLOSED,
    }
}

const MAX_ERROR_LEN: usize = 256;

impl Default for ReverseMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl ReverseMetrics {
    pub fn new() -> Self {
        Self {
            control_connections_active: AtomicU64::new(0),
            control_connections_accepted_total: AtomicU64::new(0),
            control_connections_rejected_total: AtomicU64::new(0),
            control_reconnects_total: AtomicU64::new(0),
            auth_failures_total: AtomicU64::new(0),
            heartbeat_failures_total: AtomicU64::new(0),
            drain_total: AtomicU64::new(0),
            drain_duration_ms_total: AtomicU64::new(0),
            streams_opened_total: AtomicU64::new(0),
            streams_closed_total: AtomicU64::new(0),
            stream_bytes_total: AtomicU64::new(0),
            state_time_ms: Mutex::new([0u64; 6]),
            last_error: Mutex::new(None),
        }
    }

    /// Record a control connection that passed authentication.
    pub fn record_control_accepted(&self, _peer: SocketAddr) {
        self.control_connections_active
            .fetch_add(1, Ordering::Relaxed);
        self.control_connections_accepted_total
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record a control connection that was rejected (auth failure).
    pub fn record_control_rejected(&self, _peer: SocketAddr, reason: &str) {
        self.control_connections_rejected_total
            .fetch_add(1, Ordering::Relaxed);
        self.auth_failures_total.fetch_add(1, Ordering::Relaxed);
        self.record_error(reason);
    }

    /// Record a heartbeat / keepalive failure.
    pub fn record_heartbeat_failure(&self) {
        self.heartbeat_failures_total
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record a completed drain operation.
    pub fn record_drain(&self, duration_ms: u64) {
        self.drain_total.fetch_add(1, Ordering::Relaxed);
        self.drain_duration_ms_total
            .fetch_add(duration_ms, Ordering::Relaxed);
    }

    /// Record a client reconnect attempt.
    pub fn record_reconnect(&self) {
        self.control_reconnects_total
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record that a control session was opened (data flowing).
    pub fn record_stream_opened(&self) {
        self.streams_opened_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record that a control session completed cleanly.
    pub fn record_stream_closed(&self, bytes: u64) {
        self.streams_closed_total.fetch_add(1, Ordering::Relaxed);
        self.stream_bytes_total.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record that the connection was in the given state for `duration_ms` ms.
    pub fn record_state_duration(&self, state: ControlState, duration_ms: u64) {
        if let Ok(mut guard) = self.state_time_ms.lock() {
            guard[state_index(state)] = guard[state_index(state)].saturating_add(duration_ms);
        }
    }

    /// Record an error message (truncated to 256 chars).
    pub fn record_error(&self, msg: &str) {
        let truncated = if msg.len() > MAX_ERROR_LEN {
            format!("{}…", &msg[..MAX_ERROR_LEN])
        } else {
            msg.to_string()
        };
        if let Ok(mut guard) = self.last_error.lock() {
            *guard = Some(truncated);
        }
    }

    /// Take a point-in-time snapshot of all metric values.
    pub fn snapshot(&self) -> ReverseMetricsSnapshot {
        let last_error = self.last_error.lock().ok().and_then(|guard| guard.clone());
        let state_time_ms = self
            .state_time_ms
            .lock()
            .map(|guard| *guard)
            .unwrap_or([0u64; 6]);
        ReverseMetricsSnapshot {
            control_connections_active: self.control_connections_active.load(Ordering::Relaxed),
            control_connections_accepted_total: self
                .control_connections_accepted_total
                .load(Ordering::Relaxed),
            control_connections_rejected_total: self
                .control_connections_rejected_total
                .load(Ordering::Relaxed),
            control_reconnects_total: self.control_reconnects_total.load(Ordering::Relaxed),
            auth_failures_total: self.auth_failures_total.load(Ordering::Relaxed),
            heartbeat_failures_total: self.heartbeat_failures_total.load(Ordering::Relaxed),
            drain_total: self.drain_total.load(Ordering::Relaxed),
            drain_duration_ms_total: self.drain_duration_ms_total.load(Ordering::Relaxed),
            streams_opened_total: self.streams_opened_total.load(Ordering::Relaxed),
            streams_closed_total: self.streams_closed_total.load(Ordering::Relaxed),
            stream_bytes_total: self.stream_bytes_total.load(Ordering::Relaxed),
            state_time_ms,
            last_error,
        }
    }

    /// Render counters in Prometheus text exposition format.
    pub fn render_prometheus(&self) -> String {
        let s = self.snapshot();
        let mut out = String::with_capacity(1024);

        out.push_str("# HELP eggress_reverse_control_connections_active Currently active control connections.\n");
        out.push_str("# TYPE eggress_reverse_control_connections_active gauge\n");
        out.push_str(&format!(
            "eggress_reverse_control_connections_active {}\n",
            s.control_connections_active
        ));

        out.push_str("# HELP eggress_reverse_control_connections_accepted_total Total accepted control connections.\n");
        out.push_str("# TYPE eggress_reverse_control_connections_accepted_total counter\n");
        out.push_str(&format!(
            "eggress_reverse_control_connections_accepted_total {}\n",
            s.control_connections_accepted_total
        ));

        out.push_str("# HELP eggress_reverse_control_connections_rejected_total Total rejected control connections.\n");
        out.push_str("# TYPE eggress_reverse_control_connections_rejected_total counter\n");
        out.push_str(&format!(
            "eggress_reverse_control_connections_rejected_total {}\n",
            s.control_connections_rejected_total
        ));

        out.push_str(
            "# HELP eggress_reverse_control_reconnects_total Total client reconnect attempts.\n",
        );
        out.push_str("# TYPE eggress_reverse_control_reconnects_total counter\n");
        out.push_str(&format!(
            "eggress_reverse_control_reconnects_total {}\n",
            s.control_reconnects_total
        ));

        out.push_str(
            "# HELP eggress_reverse_auth_failures_total Total control channel auth failures.\n",
        );
        out.push_str("# TYPE eggress_reverse_auth_failures_total counter\n");
        out.push_str(&format!(
            "eggress_reverse_auth_failures_total {}\n",
            s.auth_failures_total
        ));

        out.push_str(
            "# HELP eggress_reverse_heartbeat_failures_total Total heartbeat / keepalive failures.\n",
        );
        out.push_str("# TYPE eggress_reverse_heartbeat_failures_total counter\n");
        out.push_str(&format!(
            "eggress_reverse_heartbeat_failures_total {}\n",
            s.heartbeat_failures_total
        ));

        out.push_str("# HELP eggress_reverse_drain_total Total drain operations completed.\n");
        out.push_str("# TYPE eggress_reverse_drain_total counter\n");
        out.push_str(&format!("eggress_reverse_drain_total {}\n", s.drain_total));

        out.push_str(
            "# HELP eggress_reverse_drain_duration_ms_total Cumulative drain duration in milliseconds.\n",
        );
        out.push_str("# TYPE eggress_reverse_drain_duration_ms_total counter\n");
        out.push_str(&format!(
            "eggress_reverse_drain_duration_ms_total {}\n",
            s.drain_duration_ms_total
        ));

        out.push_str(
            "# HELP eggress_reverse_streams_opened_total Total control sessions initiated.\n",
        );
        out.push_str("# TYPE eggress_reverse_streams_opened_total counter\n");
        out.push_str(&format!(
            "eggress_reverse_streams_opened_total {}\n",
            s.streams_opened_total
        ));

        out.push_str("# HELP eggress_reverse_streams_closed_total Total control sessions completed cleanly.\n");
        out.push_str("# TYPE eggress_reverse_streams_closed_total counter\n");
        out.push_str(&format!(
            "eggress_reverse_streams_closed_total {}\n",
            s.streams_closed_total
        ));

        out.push_str("# HELP eggress_reverse_stream_bytes_total Total bytes relayed through control channels.\n");
        out.push_str("# TYPE eggress_reverse_stream_bytes_total counter\n");
        out.push_str(&format!(
            "eggress_reverse_stream_bytes_total {}\n",
            s.stream_bytes_total
        ));

        out.push_str(
            "# HELP eggress_reverse_state_time_ms Cumulative time spent in each control state.\n",
        );
        out.push_str("# TYPE eggress_reverse_state_time_ms counter\n");
        let state_labels = [
            "disconnected",
            "connecting",
            "authenticating",
            "ready",
            "draining",
            "closed",
        ];
        for (idx, label) in state_labels.iter().enumerate() {
            out.push_str(&format!(
                "eggress_reverse_state_time_ms{{state=\"{}\"}} {}\n",
                label, s.state_time_ms[idx]
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_counters_are_zero() {
        let m = ReverseMetrics::new();
        assert_eq!(m.control_connections_active.load(Ordering::Relaxed), 0);
        assert_eq!(
            m.control_connections_accepted_total.load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            m.control_connections_rejected_total.load(Ordering::Relaxed),
            0
        );
        assert_eq!(m.control_reconnects_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.auth_failures_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.heartbeat_failures_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.drain_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.drain_duration_ms_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.streams_opened_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.streams_closed_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.stream_bytes_total.load(Ordering::Relaxed), 0);
        assert_eq!(*m.state_time_ms.lock().unwrap(), [0u64; 6]);
        assert!(m.last_error.lock().unwrap().is_none());
    }

    fn peer() -> SocketAddr {
        "127.0.0.1:9999".parse().unwrap()
    }

    #[test]
    fn record_control_accepted() {
        let m = ReverseMetrics::new();
        m.record_control_accepted(peer());
        m.record_control_accepted(peer());
        assert_eq!(m.control_connections_active.load(Ordering::Relaxed), 2);
        assert_eq!(
            m.control_connections_accepted_total.load(Ordering::Relaxed),
            2
        );
    }

    #[test]
    fn record_control_rejected() {
        let m = ReverseMetrics::new();
        m.record_control_rejected(peer(), "bad credentials");
        assert_eq!(
            m.control_connections_rejected_total.load(Ordering::Relaxed),
            1
        );
        assert_eq!(m.auth_failures_total.load(Ordering::Relaxed), 1);
        assert_eq!(
            m.last_error.lock().unwrap().as_deref(),
            Some("bad credentials")
        );
    }

    #[test]
    fn record_heartbeat_failure() {
        let m = ReverseMetrics::new();
        m.record_heartbeat_failure();
        m.record_heartbeat_failure();
        assert_eq!(m.heartbeat_failures_total.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn record_drain() {
        let m = ReverseMetrics::new();
        m.record_drain(150);
        m.record_drain(50);
        assert_eq!(m.drain_total.load(Ordering::Relaxed), 2);
        assert_eq!(m.drain_duration_ms_total.load(Ordering::Relaxed), 200);
    }

    #[test]
    fn record_reconnect() {
        let m = ReverseMetrics::new();
        m.record_reconnect();
        m.record_reconnect();
        m.record_reconnect();
        assert_eq!(m.control_reconnects_total.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn record_stream_opened_and_closed() {
        let m = ReverseMetrics::new();
        m.record_stream_opened();
        m.record_stream_opened();
        assert_eq!(m.streams_opened_total.load(Ordering::Relaxed), 2);

        m.record_stream_closed(1024);
        assert_eq!(m.streams_closed_total.load(Ordering::Relaxed), 1);
        assert_eq!(m.stream_bytes_total.load(Ordering::Relaxed), 1024);

        m.record_stream_closed(512);
        assert_eq!(m.streams_closed_total.load(Ordering::Relaxed), 2);
        assert_eq!(m.stream_bytes_total.load(Ordering::Relaxed), 1536);
    }

    #[test]
    fn record_state_duration() {
        let m = ReverseMetrics::new();
        m.record_state_duration(ControlState::Connecting, 100);
        m.record_state_duration(ControlState::Ready, 500);
        m.record_state_duration(ControlState::Ready, 250);
        let snap = m.snapshot();
        assert_eq!(snap.state_ms(ControlState::Connecting), 100);
        assert_eq!(snap.state_ms(ControlState::Ready), 750);
        assert_eq!(snap.state_ms(ControlState::Draining), 0);
    }

    #[test]
    fn record_error_truncates_long_messages() {
        let m = ReverseMetrics::new();
        let long_msg = "x".repeat(500);
        m.record_error(&long_msg);
        let err = m.last_error.lock().unwrap().clone().unwrap();
        // Truncated to 256 chars + '…' (3 bytes in UTF-8) = 259 bytes
        assert_eq!(err.chars().count(), MAX_ERROR_LEN + 1);
        assert!(err.ends_with('…'));
    }

    #[test]
    fn snapshot_returns_expected_values() {
        let m = ReverseMetrics::new();
        m.record_control_accepted(peer());
        m.record_control_accepted(peer());
        m.record_stream_opened();
        m.record_stream_closed(2048);
        m.record_reconnect();
        m.record_control_rejected(peer(), "timeout");
        m.record_heartbeat_failure();
        m.record_drain(75);
        m.record_state_duration(ControlState::Ready, 1000);

        let snap = m.snapshot();
        assert_eq!(snap.control_connections_active, 2);
        assert_eq!(snap.control_connections_accepted_total, 2);
        assert_eq!(snap.control_connections_rejected_total, 1);
        assert_eq!(snap.auth_failures_total, 1);
        assert_eq!(snap.heartbeat_failures_total, 1);
        assert_eq!(snap.drain_total, 1);
        assert_eq!(snap.drain_duration_ms_total, 75);
        assert_eq!(snap.streams_opened_total, 1);
        assert_eq!(snap.streams_closed_total, 1);
        assert_eq!(snap.stream_bytes_total, 2048);
        assert_eq!(snap.state_ms(ControlState::Ready), 1000);
        assert_eq!(snap.last_error.as_deref(), Some("timeout"));
    }

    #[test]
    fn snapshot_display_summary() {
        let m = ReverseMetrics::new();
        m.record_control_accepted(peer());
        m.record_stream_opened();
        m.record_stream_closed(100);
        m.record_drain(20);
        let snap = m.snapshot();
        let display = snap.display_summary();
        assert!(display.contains("active=1"));
        assert!(display.contains("accepted=1"));
        assert!(display.contains("bytes=100"));
        assert!(display.contains("drain_total=1"));
        assert!(display.contains("drain_ms=20"));
        assert!(display.contains("last_error=(none)"));
    }

    #[test]
    fn prometheus_output_contains_expected_names() {
        let m = ReverseMetrics::new();
        m.record_control_accepted(peer());
        m.record_stream_opened();
        m.record_stream_closed(42);
        m.record_heartbeat_failure();
        m.record_drain(5);
        m.record_state_duration(ControlState::Ready, 100);

        let prom = m.render_prometheus();
        assert!(prom.contains("eggress_reverse_control_connections_active"));
        assert!(prom.contains("eggress_reverse_control_connections_accepted_total"));
        assert!(prom.contains("eggress_reverse_control_connections_rejected_total"));
        assert!(prom.contains("eggress_reverse_control_reconnects_total"));
        assert!(prom.contains("eggress_reverse_auth_failures_total"));
        assert!(prom.contains("eggress_reverse_heartbeat_failures_total"));
        assert!(prom.contains("eggress_reverse_drain_total"));
        assert!(prom.contains("eggress_reverse_drain_duration_ms_total"));
        assert!(prom.contains("eggress_reverse_streams_opened_total"));
        assert!(prom.contains("eggress_reverse_streams_closed_total"));
        assert!(prom.contains("eggress_reverse_stream_bytes_total"));
        assert!(prom.contains("eggress_reverse_state_time_ms"));
        assert!(prom.contains("TYPE eggress_reverse_control_connections_active gauge"));
        assert!(prom.contains("TYPE eggress_reverse_streams_opened_total counter"));
        assert!(prom.contains("state=\"ready\""));
    }

    #[test]
    fn snapshot_is_clone() {
        let m = ReverseMetrics::new();
        m.record_control_accepted(peer());
        let s1 = m.snapshot();
        let s2 = s1.clone();
        assert_eq!(s1.control_connections_active, s2.control_connections_active);
    }

    #[test]
    fn snapshot_is_debug() {
        let m = ReverseMetrics::new();
        let snap = m.snapshot();
        let debug_str = format!("{:?}", snap);
        assert!(debug_str.contains("ReverseMetricsSnapshot"));
    }

    #[test]
    fn snapshot_is_serialize() {
        let m = ReverseMetrics::new();
        let snap = m.snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("control_connections_active"));
        assert!(json.contains("stream_bytes_total"));
        assert!(json.contains("auth_failures_total"));
        assert!(json.contains("state_time_ms"));
    }
}
