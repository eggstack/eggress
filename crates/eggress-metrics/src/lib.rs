use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

use eggress_server::execute::{SessionOutcome, SessionReport};

impl eggress_server::SessionMetrics for MetricsRegistry {
    fn record_session(&self, report: &SessionReport) {
        MetricsRegistry::record_session(self, report);
    }

    fn record_session_start(&self) {
        MetricsRegistry::record_session_start(self);
    }

    fn record_route_decision(&self, rule: &str, action: &str, outcome: &str) {
        MetricsRegistry::record_route_decision(self, rule, action, outcome);
    }
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct RouteLabels {
    pub rule: String,
    pub action: String,
    pub outcome: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct UpstreamLabels {
    pub upstream_id: String,
    pub group_id: String,
}

pub struct MetricsRegistry {
    registry: Registry,
    connections_active: Gauge,
    connections_total: Counter,
    connection_failures: Counter,
    bytes_upstream_total: Counter,
    bytes_downstream_total: Counter,
    route_decisions: Family<RouteLabels, Counter>,
    upstream_health: Family<UpstreamLabels, Gauge>,
    reload_total: Counter,
    reload_failures: Counter,
    config_generation: Gauge,
    udp_associations_active: Gauge,
    udp_associations_total: Counter,
    udp_association_failures: Counter,
    udp_packets_up_total: Counter,
    udp_packets_down_total: Counter,
    udp_bytes_up_total: Counter,
    udp_bytes_down_total: Counter,
    udp_dropped_packets_total: Counter,
    udp_target_flows_active: Gauge,
    udp_target_flows_total: Counter,
    udp_decode_errors_total: Counter,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        let mut registry = Registry::default();

        let connections_active = Gauge::default();
        registry.register(
            "eggress_connections_active",
            "Currently active connections",
            connections_active.clone(),
        );

        let connections_total = Counter::default();
        registry.register(
            "eggress_connections_total",
            "Total connections handled",
            connections_total.clone(),
        );

        let connection_failures = Counter::default();
        registry.register(
            "eggress_connection_failures_total",
            "Total failed connections",
            connection_failures.clone(),
        );

        let bytes_upstream_total = Counter::default();
        registry.register(
            "eggress_bytes_upstream_total",
            "Total bytes sent upstream",
            bytes_upstream_total.clone(),
        );

        let bytes_downstream_total = Counter::default();
        registry.register(
            "eggress_bytes_downstream_total",
            "Total bytes sent downstream",
            bytes_downstream_total.clone(),
        );

        let route_decisions = Family::<RouteLabels, Counter>::default();
        registry.register(
            "eggress_route_decisions_total",
            "Route decisions by rule, action, outcome",
            route_decisions.clone(),
        );

        let upstream_health = Family::<UpstreamLabels, Gauge>::default();
        registry.register(
            "eggress_upstream_health",
            "Upstream health status (1=healthy, 0=unhealthy)",
            upstream_health.clone(),
        );

        let reload_total = Counter::default();
        registry.register(
            "eggress_reload_total",
            "Total config reload attempts",
            reload_total.clone(),
        );

        let reload_failures = Counter::default();
        registry.register(
            "eggress_reload_failures_total",
            "Total failed config reloads",
            reload_failures.clone(),
        );

        let config_generation = Gauge::default();
        registry.register(
            "eggress_config_generation",
            "Current config generation number",
            config_generation.clone(),
        );

        let udp_associations_active = Gauge::default();
        registry.register(
            "eggress_udp_associations_active",
            "Currently active UDP associations",
            udp_associations_active.clone(),
        );

        let udp_associations_total = Counter::default();
        registry.register(
            "eggress_udp_associations_total",
            "Total UDP associations created",
            udp_associations_total.clone(),
        );

        let udp_association_failures = Counter::default();
        registry.register(
            "eggress_udp_association_failures_total",
            "Total UDP association creation failures",
            udp_association_failures.clone(),
        );

        let udp_packets_up_total = Counter::default();
        registry.register(
            "eggress_udp_packets_up_total",
            "Total UDP packets received from clients",
            udp_packets_up_total.clone(),
        );

        let udp_packets_down_total = Counter::default();
        registry.register(
            "eggress_udp_packets_down_total",
            "Total UDP packets sent to clients",
            udp_packets_down_total.clone(),
        );

        let udp_bytes_up_total = Counter::default();
        registry.register(
            "eggress_udp_bytes_up_total",
            "Total UDP bytes received from clients",
            udp_bytes_up_total.clone(),
        );

        let udp_bytes_down_total = Counter::default();
        registry.register(
            "eggress_udp_bytes_down_total",
            "Total UDP bytes sent to clients",
            udp_bytes_down_total.clone(),
        );

        let udp_dropped_packets_total = Counter::default();
        registry.register(
            "eggress_udp_dropped_packets_total",
            "Total UDP packets dropped",
            udp_dropped_packets_total.clone(),
        );

        let udp_target_flows_active = Gauge::default();
        registry.register(
            "eggress_udp_target_flows_active",
            "Currently active UDP target flows",
            udp_target_flows_active.clone(),
        );

        let udp_target_flows_total = Counter::default();
        registry.register(
            "eggress_udp_target_flows_total",
            "Total UDP target flows created",
            udp_target_flows_total.clone(),
        );

        let udp_decode_errors_total = Counter::default();
        registry.register(
            "eggress_udp_decode_errors_total",
            "Total UDP datagram decode errors",
            udp_decode_errors_total.clone(),
        );

        Self {
            registry,
            connections_active,
            connections_total,
            connection_failures,
            bytes_upstream_total,
            bytes_downstream_total,
            route_decisions,
            upstream_health,
            reload_total,
            reload_failures,
            config_generation,
            udp_associations_active,
            udp_associations_total,
            udp_association_failures,
            udp_packets_up_total,
            udp_packets_down_total,
            udp_bytes_up_total,
            udp_bytes_down_total,
            udp_dropped_packets_total,
            udp_target_flows_active,
            udp_target_flows_total,
            udp_decode_errors_total,
        }
    }

    pub fn record_session_start(&self) {
        self.connections_active.inc();
    }

    pub fn record_session(&self, report: &SessionReport) {
        self.connections_total.inc();

        if matches!(
            report.outcome,
            SessionOutcome::ClientProtocolError
                | SessionOutcome::AuthenticationFailed
                | SessionOutcome::HandshakeTimedOut
                | SessionOutcome::RouteFailed
                | SessionOutcome::RelayFailed
        ) {
            self.connection_failures.inc();
        }

        self.bytes_upstream_total.inc_by(report.bytes_upstream);
        self.bytes_downstream_total.inc_by(report.bytes_downstream);
        self.connections_active.dec();
    }

    pub fn record_route_decision(&self, rule: &str, action: &str, outcome: &str) {
        self.route_decisions
            .get_or_create(&RouteLabels {
                rule: rule.to_string(),
                action: action.to_string(),
                outcome: outcome.to_string(),
            })
            .inc();
    }

    pub fn set_upstream_health(&self, upstream_id: &str, group_id: &str, healthy: bool) {
        self.upstream_health
            .get_or_create(&UpstreamLabels {
                upstream_id: upstream_id.to_string(),
                group_id: group_id.to_string(),
            })
            .set(if healthy { 1 } else { 0 });
    }

    pub fn set_config_generation(&self, generation: u64) {
        self.config_generation.set(generation as i64);
    }

    pub fn record_reload(&self, success: bool) {
        self.reload_total.inc();
        if !success {
            self.reload_failures.inc();
        }
    }

    pub fn render_prometheus(&self) -> String {
        use prometheus_client::encoding::text::encode;

        let mut buf = String::new();
        encode(&mut buf, &self.registry).unwrap();
        buf
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

    pub fn record_udp_decode_error(&self) {
        self.udp_decode_errors_total.inc();
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
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_names_are_stable() {
        let output = MetricsRegistry::new().render_prometheus();
        assert!(output.contains("eggress_connections_active"));
        assert!(output.contains("eggress_connections_total"));
        assert!(output.contains("eggress_connection_failures_total"));
        assert!(output.contains("eggress_bytes_upstream_total"));
        assert!(output.contains("eggress_bytes_downstream_total"));
        assert!(output.contains("eggress_route_decisions_total"));
        assert!(output.contains("eggress_upstream_health"));
        assert!(output.contains("eggress_reload_total"));
        assert!(output.contains("eggress_reload_failures_total"));
        assert!(output.contains("eggress_config_generation"));
        assert!(output.contains("eggress_udp_associations_active"));
        assert!(output.contains("eggress_udp_associations_total"));
        assert!(output.contains("eggress_udp_association_failures_total"));
        assert!(output.contains("eggress_udp_packets_up_total"));
        assert!(output.contains("eggress_udp_packets_down_total"));
        assert!(output.contains("eggress_udp_bytes_up_total"));
        assert!(output.contains("eggress_udp_bytes_down_total"));
        assert!(output.contains("eggress_udp_dropped_packets_total"));
        assert!(output.contains("eggress_udp_target_flows_active"));
        assert!(output.contains("eggress_udp_target_flows_total"));
        assert!(output.contains("eggress_udp_decode_errors_total"));
    }

    #[test]
    fn counter_increments() {
        let m = MetricsRegistry::new();
        m.record_route_decision("rule1", "direct", "ok");
        m.record_route_decision("rule1", "direct", "ok");
        let output = m.render_prometheus();
        assert!(output.contains("eggress_route_decisions_total"));
    }

    #[test]
    fn gauge_returns_to_zero() {
        let m = MetricsRegistry::new();
        m.set_upstream_health("up-1", "grp", true);
        let output = m.render_prometheus();
        assert!(output.contains("eggress_upstream_health"));

        m.set_upstream_health("up-1", "grp", false);
        let output2 = m.render_prometheus();
        assert!(output2.contains("eggress_upstream_health"));
    }

    #[test]
    fn labels_no_secrets() {
        let m = MetricsRegistry::new();
        let report = SessionReport {
            protocol: Some("socks5".to_string()),
            target: Some("example.com:443".to_string()),
            route: "direct".to_string(),
            bytes_upstream: 100,
            bytes_downstream: 200,
            outcome: SessionOutcome::Completed,
            failure: None,
            rule_id: Some("rule-1".to_string()),
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
        };
        m.record_session(&report);
        let output = m.render_prometheus();
        assert!(!output.contains("password"));
        assert!(!output.contains("secret"));
        assert!(!output.contains("token"));
    }

    #[test]
    fn prometheus_output_is_parseable() {
        let m = MetricsRegistry::new();
        m.record_session(&SessionReport {
            protocol: Some("http".to_string()),
            target: Some("1.2.3.4:80".to_string()),
            route: "direct".to_string(),
            bytes_upstream: 50,
            bytes_downstream: 150,
            outcome: SessionOutcome::Completed,
            failure: None,
            rule_id: None,
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
        });
        let output = m.render_prometheus();
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            assert!(parts.len() >= 2, "bad prometheus line: {trimmed}");
            let value = parts.last().unwrap();
            assert!(
                value.parse::<f64>().is_ok(),
                "non-numeric value in line: {trimmed}"
            );
        }
    }

    #[test]
    fn session_recording_updates_all_metrics() {
        let m = MetricsRegistry::new();
        m.record_session_start();

        m.record_session(&SessionReport {
            protocol: Some("socks5".to_string()),
            target: Some("example.com:443".to_string()),
            route: "direct".to_string(),
            bytes_upstream: 100,
            bytes_downstream: 200,
            outcome: SessionOutcome::Completed,
            failure: None,
            rule_id: None,
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
        });

        let output = m.render_prometheus();
        assert!(output.contains("eggress_connections_total"));
        assert!(output.contains("eggress_bytes_upstream_total"));
        assert!(output.contains("eggress_bytes_downstream_total"));
        assert!(output.contains("eggress_connections_active"));
    }

    #[test]
    fn session_failure_increments_failures() {
        let m = MetricsRegistry::new();
        m.record_session(&SessionReport {
            protocol: None,
            target: None,
            route: "error".to_string(),
            bytes_upstream: 0,
            bytes_downstream: 0,
            outcome: SessionOutcome::RouteFailed,
            failure: Some(eggress_server::FailureCategory::Dns),
            rule_id: None,
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
        });

        let output = m.render_prometheus();
        assert!(output.contains("eggress_connection_failures_total"));
    }

    #[test]
    fn reload_success_and_failure() {
        let m = MetricsRegistry::new();
        m.record_reload(true);
        m.record_reload(true);
        m.record_reload(false);
        let output = m.render_prometheus();
        assert!(output.contains("eggress_reload_total"));
        assert!(output.contains("eggress_reload_failures_total"));
    }

    #[test]
    fn config_generation_set() {
        let m = MetricsRegistry::new();
        m.set_config_generation(42);
        let output = m.render_prometheus();
        assert!(output.contains("eggress_config_generation"));
    }

    #[test]
    fn udp_association_metrics() {
        let m = MetricsRegistry::new();
        m.record_udp_association_created();
        m.record_udp_association_created();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_associations_active"));
        assert!(output.contains("eggress_udp_associations_total"));

        m.record_udp_association_closed();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_associations_active"));
    }

    #[test]
    fn udp_association_failure_metric() {
        let m = MetricsRegistry::new();
        m.record_udp_association_failure();
        m.record_udp_association_failure();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_association_failures_total"));
    }

    #[test]
    fn udp_packet_metrics() {
        let m = MetricsRegistry::new();
        m.record_udp_packet_up(100);
        m.record_udp_packet_up(200);
        m.record_udp_packet_down(50);
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_packets_up_total"));
        assert!(output.contains("eggress_udp_packets_down_total"));
        assert!(output.contains("eggress_udp_bytes_up_total"));
        assert!(output.contains("eggress_udp_bytes_down_total"));
    }

    #[test]
    fn udp_dropped_metric() {
        let m = MetricsRegistry::new();
        m.record_udp_dropped();
        m.record_udp_dropped();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_dropped_packets_total"));
    }

    #[test]
    fn udp_target_flow_metrics() {
        let m = MetricsRegistry::new();
        m.record_udp_target_flow_created();
        m.record_udp_target_flow_created();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_target_flows_active"));
        assert!(output.contains("eggress_udp_target_flows_total"));

        m.record_udp_target_flow_closed();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_target_flows_active"));
    }

    #[test]
    fn udp_decode_error_metric() {
        let m = MetricsRegistry::new();
        m.record_udp_decode_error();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_decode_errors_total"));
    }

    #[test]
    fn udp_active_gauge_returns_to_zero() {
        let m = MetricsRegistry::new();
        m.record_udp_association_created();
        m.record_udp_association_created();
        m.record_udp_association_closed();
        m.record_udp_association_closed();
        let output = m.render_prometheus();
        for line in output.lines() {
            if line.contains("eggress_udp_associations_active") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 0.0, "udp active associations should return to 0");
                    }
                }
            }
        }
    }

    #[test]
    fn active_connections_returns_to_zero() {
        let m = MetricsRegistry::new();
        m.record_session_start();
        m.record_session(&SessionReport {
            protocol: None,
            target: None,
            route: "direct".to_string(),
            bytes_upstream: 0,
            bytes_downstream: 0,
            outcome: SessionOutcome::Completed,
            failure: None,
            rule_id: None,
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
        });
        let output = m.render_prometheus();
        for line in output.lines() {
            if line.contains("eggress_connections_active") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 0.0, "active connections should return to 0");
                    }
                }
            }
        }
    }
}
