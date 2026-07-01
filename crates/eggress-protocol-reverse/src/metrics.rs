use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;

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
    /// Control sessions initiated.
    pub streams_opened_total: AtomicU64,
    /// Control sessions completed cleanly.
    pub streams_closed_total: AtomicU64,
    /// Total bytes relayed through control channels.
    pub stream_bytes_total: AtomicU64,
    /// Most recent error message (truncated to 256 chars).
    pub last_error: Mutex<Option<String>>,
}

/// Serializable snapshot of reverse proxy metrics.
#[derive(Debug, Clone, Serialize)]
pub struct ReverseMetricsSnapshot {
    pub control_connections_active: u64,
    pub control_connections_accepted_total: u64,
    pub control_connections_rejected_total: u64,
    pub control_reconnects_total: u64,
    pub streams_opened_total: u64,
    pub streams_closed_total: u64,
    pub stream_bytes_total: u64,
    pub last_error: Option<String>,
}

impl ReverseMetricsSnapshot {
    /// Format a human-readable summary for admin or log output.
    pub fn display_summary(&self) -> String {
        format!(
            "reverse: active={} accepted={} rejected={} reconnects={} \
             streams_open={} streams_closed={} bytes={} last_error={}",
            self.control_connections_active,
            self.control_connections_accepted_total,
            self.control_connections_rejected_total,
            self.control_reconnects_total,
            self.streams_opened_total,
            self.streams_closed_total,
            self.stream_bytes_total,
            self.last_error.as_deref().unwrap_or("(none)"),
        )
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
            streams_opened_total: AtomicU64::new(0),
            streams_closed_total: AtomicU64::new(0),
            stream_bytes_total: AtomicU64::new(0),
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
        self.record_error(reason);
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
        ReverseMetricsSnapshot {
            control_connections_active: self.control_connections_active.load(Ordering::Relaxed),
            control_connections_accepted_total: self
                .control_connections_accepted_total
                .load(Ordering::Relaxed),
            control_connections_rejected_total: self
                .control_connections_rejected_total
                .load(Ordering::Relaxed),
            control_reconnects_total: self.control_reconnects_total.load(Ordering::Relaxed),
            streams_opened_total: self.streams_opened_total.load(Ordering::Relaxed),
            streams_closed_total: self.streams_closed_total.load(Ordering::Relaxed),
            stream_bytes_total: self.stream_bytes_total.load(Ordering::Relaxed),
            last_error,
        }
    }

    /// Render counters in Prometheus text exposition format.
    pub fn render_prometheus(&self) -> String {
        let s = self.snapshot();
        let mut out = String::with_capacity(512);

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
        assert_eq!(m.streams_opened_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.streams_closed_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.stream_bytes_total.load(Ordering::Relaxed), 0);
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
        assert_eq!(
            m.last_error.lock().unwrap().as_deref(),
            Some("bad credentials")
        );
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

        let snap = m.snapshot();
        assert_eq!(snap.control_connections_active, 2);
        assert_eq!(snap.control_connections_accepted_total, 2);
        assert_eq!(snap.control_connections_rejected_total, 1);
        assert_eq!(snap.control_reconnects_total, 1);
        assert_eq!(snap.streams_opened_total, 1);
        assert_eq!(snap.streams_closed_total, 1);
        assert_eq!(snap.stream_bytes_total, 2048);
        assert_eq!(snap.last_error.as_deref(), Some("timeout"));
    }

    #[test]
    fn snapshot_display_summary() {
        let m = ReverseMetrics::new();
        m.record_control_accepted(peer());
        m.record_stream_opened();
        m.record_stream_closed(100);
        let snap = m.snapshot();
        let display = snap.display_summary();
        assert!(display.contains("active=1"));
        assert!(display.contains("accepted=1"));
        assert!(display.contains("bytes=100"));
        assert!(display.contains("last_error=(none)"));
    }

    #[test]
    fn prometheus_output_contains_expected_names() {
        let m = ReverseMetrics::new();
        m.record_control_accepted(peer());
        m.record_stream_opened();
        m.record_stream_closed(42);

        let prom = m.render_prometheus();
        assert!(prom.contains("eggress_reverse_control_connections_active"));
        assert!(prom.contains("eggress_reverse_control_connections_accepted_total"));
        assert!(prom.contains("eggress_reverse_control_connections_rejected_total"));
        assert!(prom.contains("eggress_reverse_control_reconnects_total"));
        assert!(prom.contains("eggress_reverse_streams_opened_total"));
        assert!(prom.contains("eggress_reverse_streams_closed_total"));
        assert!(prom.contains("eggress_reverse_stream_bytes_total"));
        assert!(prom.contains("TYPE eggress_reverse_control_connections_active gauge"));
        assert!(prom.contains("TYPE eggress_reverse_streams_opened_total counter"));
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
    }
}
