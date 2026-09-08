//! Session/route/upstream recording: the server-facing metrics domain.
//!
//! Implements `eggress_server::SessionMetrics` (narrowed to session, route,
//! upstream, and auth events) so the server records sessions without knowing
//! about Prometheus. Labeled families (`route_decisions`, `upstream_open_*`)
//! stay canonical in this registry: subsystems emit per-event calls and own
//! no parallel labeled totals.

use super::labels::{
    bounded_route_label, RouteLabels, UnsupportedTransportLabels, UpstreamFailureLabels,
    UpstreamLabels, UpstreamOpenLabels,
};
use super::registry::MetricsRegistry;
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

    fn record_upstream_open(&self, protocol: &str, outcome: &str) {
        MetricsRegistry::record_upstream_open(self, protocol, outcome);
    }

    fn record_upstream_failure(&self, protocol: &str, reason: &str) {
        MetricsRegistry::record_upstream_failure(self, protocol, reason);
    }

    fn record_auth_failure(&self) {
        MetricsRegistry::record_auth_failure(self);
    }
}

impl MetricsRegistry {
    pub fn record_session_start(&self) {
        self.connections_active.inc();
    }

    pub fn record_auth_failure(&self) {
        self.auth_failures.inc();
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
                rule: bounded_route_label(rule),
                action: bounded_route_label(action),
                outcome: bounded_route_label(outcome),
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

    pub fn record_upstream_open(&self, protocol: &str, outcome: &str) {
        self.upstream_open_total
            .get_or_create(&UpstreamOpenLabels {
                protocol: protocol.to_string(),
                outcome: outcome.to_string(),
            })
            .inc();
    }

    pub fn record_upstream_failure(&self, protocol: &str, reason: &str) {
        self.upstream_open_failures_total
            .get_or_create(&UpstreamFailureLabels {
                protocol: protocol.to_string(),
                reason: reason.to_string(),
            })
            .inc();
    }

    pub fn record_unsupported_transport(&self, protocol: &str, transport: &str, reason: &str) {
        self.unsupported_transport_total
            .get_or_create(&UnsupportedTransportLabels {
                protocol: protocol.to_string(),
                transport: transport.to_string(),
                reason: reason.to_string(),
            })
            .inc();
    }
}
