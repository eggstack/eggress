use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct UdpMetrics {
    pub associations_active: AtomicU64,
    pub associations_total: AtomicU64,
    pub association_failures: AtomicU64,
    pub packets_up: AtomicU64,
    pub packets_down: AtomicU64,
    pub bytes_up: AtomicU64,
    pub bytes_down: AtomicU64,
    pub dropped_packets: AtomicU64,
    pub target_flows_active: AtomicU64,
    pub target_flows_total: AtomicU64,
    pub decode_errors: AtomicU64,
    pub upstream_associations_total: AtomicU64,
    pub upstream_associations_active: AtomicU64,
    pub upstream_packets_up: AtomicU64,
    pub upstream_packets_down: AtomicU64,
    pub upstream_bytes_up: AtomicU64,
    pub upstream_bytes_down: AtomicU64,
    pub upstream_failures: AtomicU64,
    pub unsupported_upstream_total: AtomicU64,
}

impl UdpMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_association_created(&self) {
        self.associations_active.fetch_add(1, Ordering::Relaxed);
        self.associations_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_association_closed(&self) {
        self.associations_active.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_association_failure(&self) {
        self.association_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_packet_up(&self, bytes: u64) {
        self.packets_up.fetch_add(1, Ordering::Relaxed);
        self.bytes_up.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_packet_down(&self, bytes: u64) {
        self.packets_down.fetch_add(1, Ordering::Relaxed);
        self.bytes_down.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_dropped(&self) {
        self.dropped_packets.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_target_flow_created(&self) {
        self.target_flows_active.fetch_add(1, Ordering::Relaxed);
        self.target_flows_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_target_flow_closed(&self) {
        self.target_flows_active.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_decode_error(&self) {
        self.decode_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_association_timeout(&self) {
        self.association_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_target_flow_timeout(&self) {
        self.target_flows_active.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_upstream_association_created(&self) {
        self.upstream_associations_active
            .fetch_add(1, Ordering::Relaxed);
        self.upstream_associations_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_upstream_association_closed(&self) {
        self.upstream_associations_active
            .fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_upstream_failure(&self) {
        self.upstream_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_upstream_packet_up(&self, bytes: u64) {
        self.upstream_packets_up.fetch_add(1, Ordering::Relaxed);
        self.upstream_bytes_up.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_upstream_packet_down(&self, bytes: u64) {
        self.upstream_packets_down.fetch_add(1, Ordering::Relaxed);
        self.upstream_bytes_down.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_unsupported_upstream(&self) {
        self.unsupported_upstream_total
            .fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_metrics_are_zero() {
        let metrics = UdpMetrics::new();
        assert_eq!(metrics.associations_active.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.associations_total.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.packets_up.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.packets_down.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.bytes_up.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.bytes_down.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.dropped_packets.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.target_flows_active.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.target_flows_total.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.decode_errors.load(Ordering::Relaxed), 0);
        assert_eq!(
            metrics.upstream_associations_total.load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            metrics.upstream_associations_active.load(Ordering::Relaxed),
            0
        );
        assert_eq!(metrics.upstream_packets_up.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.upstream_packets_down.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.upstream_bytes_up.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.upstream_bytes_down.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.upstream_failures.load(Ordering::Relaxed), 0);
        assert_eq!(
            metrics.unsupported_upstream_total.load(Ordering::Relaxed),
            0
        );
    }

    #[test]
    fn association_metrics() {
        let metrics = UdpMetrics::new();
        metrics.record_association_created();
        assert_eq!(metrics.associations_active.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.associations_total.load(Ordering::Relaxed), 1);

        metrics.record_association_created();
        assert_eq!(metrics.associations_active.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.associations_total.load(Ordering::Relaxed), 2);

        metrics.record_association_closed();
        assert_eq!(metrics.associations_active.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.associations_total.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn association_failure() {
        let metrics = UdpMetrics::new();
        metrics.record_association_failure();
        metrics.record_association_failure();
        assert_eq!(metrics.association_failures.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn packet_metrics() {
        let metrics = UdpMetrics::new();
        metrics.record_packet_up(100);
        metrics.record_packet_up(200);
        assert_eq!(metrics.packets_up.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.bytes_up.load(Ordering::Relaxed), 300);

        metrics.record_packet_down(50);
        assert_eq!(metrics.packets_down.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.bytes_down.load(Ordering::Relaxed), 50);
    }

    #[test]
    fn dropped_packets() {
        let metrics = UdpMetrics::new();
        metrics.record_dropped();
        metrics.record_dropped();
        metrics.record_dropped();
        assert_eq!(metrics.dropped_packets.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn target_flow_metrics() {
        let metrics = UdpMetrics::new();
        metrics.record_target_flow_created();
        metrics.record_target_flow_created();
        assert_eq!(metrics.target_flows_active.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.target_flows_total.load(Ordering::Relaxed), 2);

        metrics.record_target_flow_closed();
        assert_eq!(metrics.target_flows_active.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.target_flows_total.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn decode_errors() {
        let metrics = UdpMetrics::new();
        metrics.record_decode_error();
        assert_eq!(metrics.decode_errors.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn association_timeout_metric() {
        let metrics = UdpMetrics::new();
        metrics.record_association_created();
        assert_eq!(metrics.associations_active.load(Ordering::Relaxed), 1);
        metrics.record_association_timeout();
        assert_eq!(metrics.association_failures.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.associations_active.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn target_flow_timeout_metric() {
        let metrics = UdpMetrics::new();
        metrics.record_target_flow_created();
        assert_eq!(metrics.target_flows_active.load(Ordering::Relaxed), 1);
        metrics.record_target_flow_timeout();
        assert_eq!(metrics.target_flows_active.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn upstream_association_metrics() {
        let metrics = UdpMetrics::new();
        metrics.record_upstream_association_created();
        assert_eq!(
            metrics.upstream_associations_active.load(Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics.upstream_associations_total.load(Ordering::Relaxed),
            1
        );

        metrics.record_upstream_association_created();
        assert_eq!(
            metrics.upstream_associations_active.load(Ordering::Relaxed),
            2
        );
        assert_eq!(
            metrics.upstream_associations_total.load(Ordering::Relaxed),
            2
        );

        metrics.record_upstream_association_closed();
        assert_eq!(
            metrics.upstream_associations_active.load(Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics.upstream_associations_total.load(Ordering::Relaxed),
            2
        );
    }

    #[test]
    fn upstream_failure_metric() {
        let metrics = UdpMetrics::new();
        metrics.record_upstream_failure();
        metrics.record_upstream_failure();
        assert_eq!(metrics.upstream_failures.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn upstream_packet_metrics() {
        let metrics = UdpMetrics::new();
        metrics.record_upstream_packet_up(100);
        metrics.record_upstream_packet_up(200);
        assert_eq!(metrics.upstream_packets_up.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.upstream_bytes_up.load(Ordering::Relaxed), 300);

        metrics.record_upstream_packet_down(50);
        assert_eq!(metrics.upstream_packets_down.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.upstream_bytes_down.load(Ordering::Relaxed), 50);
    }

    #[test]
    fn unsupported_upstream_metric() {
        let metrics = UdpMetrics::new();
        metrics.record_unsupported_upstream();
        metrics.record_unsupported_upstream();
        assert_eq!(
            metrics.unsupported_upstream_total.load(Ordering::Relaxed),
            2
        );
    }
}
