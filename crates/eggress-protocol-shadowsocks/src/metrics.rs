use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct ShadowsocksMetrics {
    pub tcp_sessions_active: AtomicU64,
    pub tcp_sessions_total: AtomicU64,
    pub tcp_upstream_sessions_total: AtomicU64,
    pub tcp_decrypt_failures_total: AtomicU64,
    pub tcp_frame_parse_failures_total: AtomicU64,
    pub tcp_unsupported_method_rejects_total: AtomicU64,
    pub tcp_active_flows: AtomicU64,
    pub udp_packets_in_total: AtomicU64,
    pub udp_packets_out_total: AtomicU64,
    pub udp_bytes_in_total: AtomicU64,
    pub udp_bytes_out_total: AtomicU64,
    pub udp_decrypt_failures_total: AtomicU64,
    pub udp_unsupported_method_rejects_total: AtomicU64,
    pub udp_active_flows: AtomicU64,
}

impl ShadowsocksMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_tcp_session_accepted(&self) {
        self.tcp_sessions_active.fetch_add(1, Ordering::Relaxed);
        self.tcp_sessions_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_tcp_session_closed(&self) {
        self.tcp_sessions_active.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_tcp_upstream_session(&self) {
        self.tcp_upstream_sessions_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_tcp_decrypt_failure(&self) {
        self.tcp_decrypt_failures_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_tcp_frame_parse_failure(&self) {
        self.tcp_frame_parse_failures_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_tcp_unsupported_method_reject(&self) {
        self.tcp_unsupported_method_rejects_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_tcp_flow_open(&self) {
        self.tcp_active_flows.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_tcp_flow_close(&self) {
        self.tcp_active_flows.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_udp_packet_in(&self, bytes: u64) {
        self.udp_packets_in_total.fetch_add(1, Ordering::Relaxed);
        self.udp_bytes_in_total.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_udp_packet_out(&self, bytes: u64) {
        self.udp_packets_out_total.fetch_add(1, Ordering::Relaxed);
        self.udp_bytes_out_total.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_udp_decrypt_failure(&self) {
        self.udp_decrypt_failures_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_udp_unsupported_method_reject(&self) {
        self.udp_unsupported_method_rejects_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_udp_flow_open(&self) {
        self.udp_active_flows.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_udp_flow_close(&self) {
        self.udp_active_flows.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_metrics_are_zero() {
        let m = ShadowsocksMetrics::new();
        assert_eq!(m.tcp_sessions_active.load(Ordering::Relaxed), 0);
        assert_eq!(m.tcp_sessions_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.tcp_upstream_sessions_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.tcp_decrypt_failures_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.tcp_frame_parse_failures_total.load(Ordering::Relaxed), 0);
        assert_eq!(
            m.tcp_unsupported_method_rejects_total
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(m.tcp_active_flows.load(Ordering::Relaxed), 0);
        assert_eq!(m.udp_packets_in_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.udp_packets_out_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.udp_bytes_in_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.udp_bytes_out_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.udp_decrypt_failures_total.load(Ordering::Relaxed), 0);
        assert_eq!(
            m.udp_unsupported_method_rejects_total
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(m.udp_active_flows.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn tcp_session_metrics() {
        let m = ShadowsocksMetrics::new();
        m.record_tcp_session_accepted();
        m.record_tcp_session_accepted();
        assert_eq!(m.tcp_sessions_total.load(Ordering::Relaxed), 2);
        assert_eq!(m.tcp_sessions_active.load(Ordering::Relaxed), 2);

        m.record_tcp_session_closed();
        assert_eq!(m.tcp_sessions_active.load(Ordering::Relaxed), 1);
        assert_eq!(m.tcp_sessions_total.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn tcp_upstream_metrics() {
        let m = ShadowsocksMetrics::new();
        m.record_tcp_upstream_session();
        m.record_tcp_upstream_session();
        m.record_tcp_upstream_session();
        assert_eq!(m.tcp_upstream_sessions_total.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn tcp_failure_metrics() {
        let m = ShadowsocksMetrics::new();
        m.record_tcp_decrypt_failure();
        m.record_tcp_frame_parse_failure();
        m.record_tcp_unsupported_method_reject();
        assert_eq!(m.tcp_decrypt_failures_total.load(Ordering::Relaxed), 1);
        assert_eq!(m.tcp_frame_parse_failures_total.load(Ordering::Relaxed), 1);
        assert_eq!(
            m.tcp_unsupported_method_rejects_total
                .load(Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn tcp_flow_metrics() {
        let m = ShadowsocksMetrics::new();
        m.record_tcp_flow_open();
        m.record_tcp_flow_open();
        assert_eq!(m.tcp_active_flows.load(Ordering::Relaxed), 2);
        m.record_tcp_flow_close();
        assert_eq!(m.tcp_active_flows.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn udp_packet_metrics() {
        let m = ShadowsocksMetrics::new();
        m.record_udp_packet_in(100);
        m.record_udp_packet_in(200);
        m.record_udp_packet_out(150);
        assert_eq!(m.udp_packets_in_total.load(Ordering::Relaxed), 2);
        assert_eq!(m.udp_bytes_in_total.load(Ordering::Relaxed), 300);
        assert_eq!(m.udp_packets_out_total.load(Ordering::Relaxed), 1);
        assert_eq!(m.udp_bytes_out_total.load(Ordering::Relaxed), 150);
    }

    #[test]
    fn udp_failure_metrics() {
        let m = ShadowsocksMetrics::new();
        m.record_udp_decrypt_failure();
        m.record_udp_unsupported_method_reject();
        m.record_udp_flow_open();
        assert_eq!(m.udp_decrypt_failures_total.load(Ordering::Relaxed), 1);
        assert_eq!(
            m.udp_unsupported_method_rejects_total
                .load(Ordering::Relaxed),
            1
        );
        assert_eq!(m.udp_active_flows.load(Ordering::Relaxed), 1);
        m.record_udp_flow_close();
        assert_eq!(m.udp_active_flows.load(Ordering::Relaxed), 0);
    }
}
