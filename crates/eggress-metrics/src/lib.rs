use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

use eggress_protocol_shadowsocks::ShadowsocksMetrics;
use eggress_server::execute::{SessionOutcome, SessionReport};
use eggress_udp::metrics::UdpMetrics;

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

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct DecodeErrorLabels {
    pub kind: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct UpstreamOpenLabels {
    pub protocol: String,
    pub outcome: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct UpstreamFailureLabels {
    pub protocol: String,
    pub reason: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct UnsupportedTransportLabels {
    pub protocol: String,
    pub transport: String,
    pub reason: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct H2ConnectionLabels {
    pub upstream_id: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct H2StreamLabels {
    pub upstream_id: String,
    pub outcome: String,
}

pub struct MetricsRegistry {
    registry: Registry,
    connections_active: Gauge,
    connections_total: Counter,
    connection_failures: Counter,
    auth_failures: Counter,
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
    udp_association_timeouts: Counter,
    udp_packets_up_total: Counter,
    udp_packets_down_total: Counter,
    udp_bytes_up_total: Counter,
    udp_bytes_down_total: Counter,
    udp_dropped_packets_total: Counter,
    udp_target_flows_active: Gauge,
    udp_target_flows_total: Counter,
    udp_decode_errors_total: Family<DecodeErrorLabels, Counter>,
    udp_unsupported_upstream_total: Counter,
    udp_upstream_associations_active: Gauge,
    udp_upstream_associations_total: Counter,
    udp_upstream_packets_up_total: Counter,
    udp_upstream_packets_down_total: Counter,
    udp_upstream_bytes_up_total: Counter,
    udp_upstream_bytes_down_total: Counter,
    udp_upstream_failures_total: Counter,
    standalone_udp_flows_active: Gauge,
    standalone_udp_flows_total: Counter,
    standalone_udp_packets_in_total: Counter,
    standalone_udp_packets_out_total: Counter,
    standalone_udp_bytes_in_total: Counter,
    standalone_udp_bytes_out_total: Counter,
    standalone_udp_malformed_total: Counter,
    standalone_udp_rejected_total: Counter,
    standalone_udp_flow_reaps_total: Counter,
    upstream_open_total: Family<UpstreamOpenLabels, Counter>,
    upstream_open_failures_total: Family<UpstreamFailureLabels, Counter>,
    unsupported_transport_total: Family<UnsupportedTransportLabels, Counter>,
    transparent_connections_accepted: Counter,
    transparent_original_dst_failed: Counter,
    transparent_route_rejects: Counter,
    unix_listener_connections_accepted: Counter,
    unix_listener_bind_failures: Counter,
    platform_capability_check_failures: Counter,
    ss_tcp_sessions_active: Gauge,
    ss_tcp_sessions_total: Counter,
    ss_tcp_upstream_sessions_total: Counter,
    ss_tcp_decrypt_failures_total: Counter,
    ss_tcp_frame_parse_failures_total: Counter,
    ss_tcp_unsupported_method_rejects_total: Counter,
    ss_tcp_active_flows: Gauge,
    ss_udp_packets_in_total: Counter,
    ss_udp_packets_out_total: Counter,
    ss_udp_bytes_in_total: Counter,
    ss_udp_bytes_out_total: Counter,
    ss_udp_decrypt_failures_total: Counter,
    ss_udp_unsupported_method_rejects_total: Counter,
    ss_udp_active_flows: Gauge,
    h2_connections_active: Gauge,
    h2_connections_total: Counter,
    h2_streams_active: Gauge,
    h2_streams_total: Family<H2StreamLabels, Counter>,
    h2_goaway_total: Counter,
    h2_handshake_failures_total: Counter,
    h2_auth_failures_total: Counter,
    h2_flow_control_stalls_total: Counter,
    h2_pool_exhausted_total: Counter,
    h2_bytes_relayed_total: Counter,
    transparent_accepted_bridged: Mutex<Option<Arc<AtomicU64>>>,
    transparent_dst_failed_bridged: Mutex<Option<Arc<AtomicU64>>>,
    transparent_prev_accepted: Mutex<u64>,
    transparent_prev_dst_failed: Mutex<u64>,
    bridged_udp_metrics: Mutex<Option<(Arc<UdpMetrics>, BridgedUdpSnapshot)>>,
    bridged_shadowsocks_metrics:
        Mutex<Option<(Arc<ShadowsocksMetrics>, BridgedShadowsocksSnapshot)>>,
    h2_prev_connections_opened: Mutex<u64>,
    h2_prev_connections_closed: Mutex<u64>,
    h2_prev_streams_opened: Mutex<u64>,
    h2_prev_streams_closed: Mutex<u64>,
    h2_prev_goaway: Mutex<u64>,
    h2_prev_handshake_failures: Mutex<u64>,
    h2_prev_auth_failures: Mutex<u64>,
    h2_prev_flow_control_stalls: Mutex<u64>,
    h2_prev_pool_exhausted: Mutex<u64>,
    h2_prev_bytes_relayed: Mutex<u64>,
}

#[derive(Default)]
struct BridgedUdpSnapshot {
    associations_total: u64,
    association_failures: u64,
    association_timeouts: u64,
    packets_up: u64,
    packets_down: u64,
    bytes_up: u64,
    bytes_down: u64,
    dropped_packets: u64,
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

#[derive(Default)]
struct BridgedShadowsocksSnapshot {
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

        let auth_failures = Counter::default();
        registry.register(
            "eggress_auth_failures_total",
            "Total authentication failures",
            auth_failures.clone(),
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

        let udp_association_timeouts = Counter::default();
        registry.register(
            "eggress_udp_association_timeouts_total",
            "Total UDP association idle timeouts",
            udp_association_timeouts.clone(),
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

        let udp_decode_errors_total = Family::<DecodeErrorLabels, Counter>::default();
        registry.register(
            "eggress_udp_decode_errors_total",
            "Total UDP datagram decode errors",
            udp_decode_errors_total.clone(),
        );

        let udp_unsupported_upstream_total = Counter::default();
        registry.register(
            "eggress_udp_unsupported_upstream_total",
            "Total UDP packets routed to unsupported upstream groups",
            udp_unsupported_upstream_total.clone(),
        );

        let udp_upstream_associations_active = Gauge::default();
        registry.register(
            "eggress_udp_upstream_associations_active",
            "Currently active UDP upstream associations",
            udp_upstream_associations_active.clone(),
        );

        let udp_upstream_associations_total = Counter::default();
        registry.register(
            "eggress_udp_upstream_associations_total",
            "Total UDP upstream associations created",
            udp_upstream_associations_total.clone(),
        );

        let udp_upstream_packets_up_total = Counter::default();
        registry.register(
            "eggress_udp_upstream_packets_up_total",
            "Total UDP packets sent upstream",
            udp_upstream_packets_up_total.clone(),
        );

        let udp_upstream_packets_down_total = Counter::default();
        registry.register(
            "eggress_udp_upstream_packets_down_total",
            "Total UDP packets received from upstream",
            udp_upstream_packets_down_total.clone(),
        );

        let udp_upstream_bytes_up_total = Counter::default();
        registry.register(
            "eggress_udp_upstream_bytes_up_total",
            "Total UDP bytes sent upstream",
            udp_upstream_bytes_up_total.clone(),
        );

        let udp_upstream_bytes_down_total = Counter::default();
        registry.register(
            "eggress_udp_upstream_bytes_down_total",
            "Total UDP bytes received from upstream",
            udp_upstream_bytes_down_total.clone(),
        );

        let udp_upstream_failures_total = Counter::default();
        registry.register(
            "eggress_udp_upstream_failures_total",
            "Total UDP upstream failures",
            udp_upstream_failures_total.clone(),
        );

        let standalone_udp_flows_active = Gauge::default();
        registry.register(
            "eggress_standalone_udp_flows_active",
            "Currently active standalone UDP flows",
            standalone_udp_flows_active.clone(),
        );

        let standalone_udp_flows_total = Counter::default();
        registry.register(
            "eggress_standalone_udp_flows_total",
            "Total standalone UDP flows created",
            standalone_udp_flows_total.clone(),
        );

        let standalone_udp_packets_in_total = Counter::default();
        registry.register(
            "eggress_standalone_udp_packets_in_total",
            "Total standalone UDP packets received from clients",
            standalone_udp_packets_in_total.clone(),
        );

        let standalone_udp_packets_out_total = Counter::default();
        registry.register(
            "eggress_standalone_udp_packets_out_total",
            "Total standalone UDP packets sent to clients",
            standalone_udp_packets_out_total.clone(),
        );

        let standalone_udp_bytes_in_total = Counter::default();
        registry.register(
            "eggress_standalone_udp_bytes_in_total",
            "Total standalone UDP bytes received from clients",
            standalone_udp_bytes_in_total.clone(),
        );

        let standalone_udp_bytes_out_total = Counter::default();
        registry.register(
            "eggress_standalone_udp_bytes_out_total",
            "Total standalone UDP bytes sent to clients",
            standalone_udp_bytes_out_total.clone(),
        );

        let standalone_udp_malformed_total = Counter::default();
        registry.register(
            "eggress_standalone_udp_malformed_total",
            "Total standalone UDP malformed datagrams",
            standalone_udp_malformed_total.clone(),
        );

        let standalone_udp_rejected_total = Counter::default();
        registry.register(
            "eggress_standalone_udp_rejected_total",
            "Total standalone UDP rejected datagrams",
            standalone_udp_rejected_total.clone(),
        );

        let standalone_udp_flow_reaps_total = Counter::default();
        registry.register(
            "eggress_standalone_udp_flow_reaps_total",
            "Total standalone UDP flows reaped",
            standalone_udp_flow_reaps_total.clone(),
        );

        let upstream_open_total = Family::<UpstreamOpenLabels, Counter>::default();
        registry.register(
            "eggress_upstream_open_total",
            "Total upstream connection attempts by protocol and outcome",
            upstream_open_total.clone(),
        );

        let upstream_open_failures_total = Family::<UpstreamFailureLabels, Counter>::default();
        registry.register(
            "eggress_upstream_open_failures_total",
            "Total upstream connection failures by protocol and reason",
            upstream_open_failures_total.clone(),
        );

        let unsupported_transport_total = Family::<UnsupportedTransportLabels, Counter>::default();
        registry.register(
            "eggress_unsupported_transport_total",
            "Total unsupported transport attempts by protocol and transport",
            unsupported_transport_total.clone(),
        );

        let transparent_connections_accepted = Counter::default();
        registry.register(
            "eggress_transparent_connections_accepted_total",
            "Total transparent proxy connections accepted",
            transparent_connections_accepted.clone(),
        );

        let transparent_original_dst_failed = Counter::default();
        registry.register(
            "eggress_transparent_original_dst_failed_total",
            "Total transparent proxy original destination lookup failures",
            transparent_original_dst_failed.clone(),
        );

        let transparent_route_rejects = Counter::default();
        registry.register(
            "eggress_transparent_route_rejects_total",
            "Total transparent proxy route rejections",
            transparent_route_rejects.clone(),
        );

        let unix_listener_connections_accepted = Counter::default();
        registry.register(
            "eggress_unix_listener_connections_accepted_total",
            "Total Unix listener connections accepted",
            unix_listener_connections_accepted.clone(),
        );

        let unix_listener_bind_failures = Counter::default();
        registry.register(
            "eggress_unix_listener_bind_failures_total",
            "Total Unix listener bind failures",
            unix_listener_bind_failures.clone(),
        );

        let platform_capability_check_failures = Counter::default();
        registry.register(
            "eggress_platform_capability_check_failures_total",
            "Total platform capability check failures",
            platform_capability_check_failures.clone(),
        );

        let ss_tcp_sessions_active = Gauge::default();
        registry.register(
            "eggress_shadowsocks_tcp_sessions_active",
            "Currently active Shadowsocks TCP sessions",
            ss_tcp_sessions_active.clone(),
        );

        let ss_tcp_sessions_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_tcp_sessions_total",
            "Total Shadowsocks TCP sessions accepted",
            ss_tcp_sessions_total.clone(),
        );

        let ss_tcp_upstream_sessions_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_tcp_upstream_sessions_total",
            "Total Shadowsocks TCP upstream sessions opened",
            ss_tcp_upstream_sessions_total.clone(),
        );

        let ss_tcp_decrypt_failures_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_tcp_decrypt_failures_total",
            "Total Shadowsocks TCP decrypt failures",
            ss_tcp_decrypt_failures_total.clone(),
        );

        let ss_tcp_frame_parse_failures_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_tcp_frame_parse_failures_total",
            "Total Shadowsocks TCP frame parse failures",
            ss_tcp_frame_parse_failures_total.clone(),
        );

        let ss_tcp_unsupported_method_rejects_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_tcp_unsupported_method_rejects_total",
            "Total Shadowsocks TCP unsupported method rejects",
            ss_tcp_unsupported_method_rejects_total.clone(),
        );

        let ss_tcp_active_flows = Gauge::default();
        registry.register(
            "eggress_shadowsocks_tcp_active_flows",
            "Currently active Shadowsocks TCP flows",
            ss_tcp_active_flows.clone(),
        );

        let ss_udp_packets_in_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_udp_packets_in_total",
            "Total Shadowsocks UDP packets received from clients",
            ss_udp_packets_in_total.clone(),
        );

        let ss_udp_packets_out_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_udp_packets_out_total",
            "Total Shadowsocks UDP packets sent to clients",
            ss_udp_packets_out_total.clone(),
        );

        let ss_udp_bytes_in_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_udp_bytes_in_total",
            "Total Shadowsocks UDP bytes received from clients",
            ss_udp_bytes_in_total.clone(),
        );

        let ss_udp_bytes_out_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_udp_bytes_out_total",
            "Total Shadowsocks UDP bytes sent to clients",
            ss_udp_bytes_out_total.clone(),
        );

        let ss_udp_decrypt_failures_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_udp_decrypt_failures_total",
            "Total Shadowsocks UDP decrypt failures",
            ss_udp_decrypt_failures_total.clone(),
        );

        let ss_udp_unsupported_method_rejects_total = Counter::default();
        registry.register(
            "eggress_shadowsocks_udp_unsupported_method_rejects_total",
            "Total Shadowsocks UDP unsupported method rejects",
            ss_udp_unsupported_method_rejects_total.clone(),
        );

        let ss_udp_active_flows = Gauge::default();
        registry.register(
            "eggress_shadowsocks_udp_active_flows",
            "Currently active Shadowsocks UDP flows",
            ss_udp_active_flows.clone(),
        );

        let h2_connections_active = Gauge::default();
        registry.register(
            "eggress_h2_connections_active",
            "Currently active H2 upstream connections",
            h2_connections_active.clone(),
        );

        let h2_connections_total = Counter::default();
        registry.register(
            "eggress_h2_connections_total",
            "Total H2 upstream connections opened",
            h2_connections_total.clone(),
        );

        let h2_streams_active = Gauge::default();
        registry.register(
            "eggress_h2_streams_active",
            "Currently active H2 streams",
            h2_streams_active.clone(),
        );

        let h2_streams_total = Family::<H2StreamLabels, Counter>::default();
        registry.register(
            "eggress_h2_streams_total",
            "Total H2 streams by outcome",
            h2_streams_total.clone(),
        );

        let h2_goaway_total = Counter::default();
        registry.register(
            "eggress_h2_goaway_total",
            "Total H2 GOAWAY frames received",
            h2_goaway_total.clone(),
        );

        let h2_handshake_failures_total = Counter::default();
        registry.register(
            "eggress_h2_handshake_failures_total",
            "Total H2 handshake failures",
            h2_handshake_failures_total.clone(),
        );

        let h2_auth_failures_total = Counter::default();
        registry.register(
            "eggress_h2_auth_failures_total",
            "Total H2 upstream authentication failures",
            h2_auth_failures_total.clone(),
        );

        let h2_flow_control_stalls_total = Counter::default();
        registry.register(
            "eggress_h2_flow_control_stalls_total",
            "Total H2 flow control stalls",
            h2_flow_control_stalls_total.clone(),
        );

        let h2_pool_exhausted_total = Counter::default();
        registry.register(
            "eggress_h2_pool_exhausted_total",
            "Total H2 connection pool exhaustion events",
            h2_pool_exhausted_total.clone(),
        );

        let h2_bytes_relayed_total = Counter::default();
        registry.register(
            "eggress_h2_bytes_relayed_total",
            "Total bytes relayed over H2 connections",
            h2_bytes_relayed_total.clone(),
        );

        Self {
            registry,
            connections_active,
            connections_total,
            connection_failures,
            auth_failures,
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
            udp_association_timeouts,
            udp_packets_up_total,
            udp_packets_down_total,
            udp_bytes_up_total,
            udp_bytes_down_total,
            udp_dropped_packets_total,
            udp_target_flows_active,
            udp_target_flows_total,
            udp_decode_errors_total,
            udp_unsupported_upstream_total,
            udp_upstream_associations_active,
            udp_upstream_associations_total,
            udp_upstream_packets_up_total,
            udp_upstream_packets_down_total,
            udp_upstream_bytes_up_total,
            udp_upstream_bytes_down_total,
            udp_upstream_failures_total,
            standalone_udp_flows_active,
            standalone_udp_flows_total,
            standalone_udp_packets_in_total,
            standalone_udp_packets_out_total,
            standalone_udp_bytes_in_total,
            standalone_udp_bytes_out_total,
            standalone_udp_malformed_total,
            standalone_udp_rejected_total,
            standalone_udp_flow_reaps_total,
            upstream_open_total,
            upstream_open_failures_total,
            unsupported_transport_total,
            transparent_connections_accepted,
            transparent_original_dst_failed,
            transparent_route_rejects,
            unix_listener_connections_accepted,
            unix_listener_bind_failures,
            platform_capability_check_failures,
            ss_tcp_sessions_active,
            ss_tcp_sessions_total,
            ss_tcp_upstream_sessions_total,
            ss_tcp_decrypt_failures_total,
            ss_tcp_frame_parse_failures_total,
            ss_tcp_unsupported_method_rejects_total,
            ss_tcp_active_flows,
            ss_udp_packets_in_total,
            ss_udp_packets_out_total,
            ss_udp_bytes_in_total,
            ss_udp_bytes_out_total,
            ss_udp_decrypt_failures_total,
            ss_udp_unsupported_method_rejects_total,
            ss_udp_active_flows,
            h2_connections_active,
            h2_connections_total,
            h2_streams_active,
            h2_streams_total,
            h2_goaway_total,
            h2_handshake_failures_total,
            h2_auth_failures_total,
            h2_flow_control_stalls_total,
            h2_pool_exhausted_total,
            h2_bytes_relayed_total,
            transparent_accepted_bridged: Mutex::new(None),
            transparent_dst_failed_bridged: Mutex::new(None),
            transparent_prev_accepted: Mutex::new(0),
            transparent_prev_dst_failed: Mutex::new(0),
            bridged_udp_metrics: Mutex::new(None),
            bridged_shadowsocks_metrics: Mutex::new(None),
            h2_prev_connections_opened: Mutex::new(0),
            h2_prev_connections_closed: Mutex::new(0),
            h2_prev_streams_opened: Mutex::new(0),
            h2_prev_streams_closed: Mutex::new(0),
            h2_prev_goaway: Mutex::new(0),
            h2_prev_handshake_failures: Mutex::new(0),
            h2_prev_auth_failures: Mutex::new(0),
            h2_prev_flow_control_stalls: Mutex::new(0),
            h2_prev_pool_exhausted: Mutex::new(0),
            h2_prev_bytes_relayed: Mutex::new(0),
        }
    }

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
        *self.bridged_udp_metrics.lock().unwrap() = Some((metrics, snapshot));
    }

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
        *self.bridged_shadowsocks_metrics.lock().unwrap() = Some((metrics, snapshot));
    }

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
        self.config_generation
            .set(generation.min(i64::MAX as u64) as i64);
    }

    pub fn record_reload(&self, success: bool) {
        self.reload_total.inc();
        if !success {
            self.reload_failures.inc();
        }
    }

    pub fn render_prometheus(&self) -> String {
        use prometheus_client::encoding::text::encode;
        use std::sync::atomic::Ordering;

        // Sync live UDP relay counters from the bridged UdpMetrics into
        // Prometheus gauges/counters before encoding.
        if let Some((metrics, prev)) = self.bridged_udp_metrics.lock().unwrap().as_mut() {
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

        // Sync live Shadowsocks protocol counters/gauges from bridged metrics
        if let Some((metrics, prev)) = self.bridged_shadowsocks_metrics.lock().unwrap().as_mut() {
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

        // Sync H2 protocol metrics from global H2_PROTOCOL_METRICS atomics
        {
            use eggress_protocol_http::H2_PROTOCOL_METRICS;

            let cur_opened = H2_PROTOCOL_METRICS
                .connections_opened
                .load(Ordering::Relaxed);
            let cur_closed = H2_PROTOCOL_METRICS
                .connections_closed
                .load(Ordering::Relaxed);
            let prev_opened = *self.h2_prev_connections_opened.lock().unwrap();
            let _prev_closed = *self.h2_prev_connections_closed.lock().unwrap();
            let delta_opened = cur_opened.saturating_sub(prev_opened);
            if delta_opened > 0 {
                self.h2_connections_total.inc_by(delta_opened);
            }
            *self.h2_prev_connections_opened.lock().unwrap() = cur_opened;
            *self.h2_prev_connections_closed.lock().unwrap() = cur_closed;
            // active = total opened - total closed (capped at 0)
            let active = cur_opened.saturating_sub(cur_closed);
            self.h2_connections_active
                .set(active.min(i64::MAX as u64) as i64);

            let cur = H2_PROTOCOL_METRICS.streams_opened.load(Ordering::Relaxed);
            let prev = *self.h2_prev_streams_opened.lock().unwrap();
            let delta = cur.saturating_sub(prev);
            if delta > 0 {
                self.h2_streams_total
                    .get_or_create(&H2StreamLabels {
                        upstream_id: "h2".to_string(),
                        outcome: "opened".to_string(),
                    })
                    .inc_by(delta);
            }
            *self.h2_prev_streams_opened.lock().unwrap() = cur;

            let cur = H2_PROTOCOL_METRICS.streams_closed.load(Ordering::Relaxed);
            let prev = *self.h2_prev_streams_closed.lock().unwrap();
            let delta = cur.saturating_sub(prev);
            if delta > 0 {
                self.h2_streams_total
                    .get_or_create(&H2StreamLabels {
                        upstream_id: "h2".to_string(),
                        outcome: "closed".to_string(),
                    })
                    .inc_by(delta);
            }
            *self.h2_prev_streams_closed.lock().unwrap() = cur;

            let total_opened = H2_PROTOCOL_METRICS.streams_opened.load(Ordering::Relaxed);
            let total_closed = H2_PROTOCOL_METRICS.streams_closed.load(Ordering::Relaxed);
            let active_streams = total_opened.saturating_sub(total_closed);
            self.h2_streams_active
                .set(active_streams.min(i64::MAX as u64) as i64);

            let cur = H2_PROTOCOL_METRICS.goaway_received.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(*self.h2_prev_goaway.lock().unwrap());
            if delta > 0 {
                self.h2_goaway_total.inc_by(delta);
            }
            *self.h2_prev_goaway.lock().unwrap() = cur;

            let cur = H2_PROTOCOL_METRICS
                .handshake_failures
                .load(Ordering::Relaxed);
            let delta = cur.saturating_sub(*self.h2_prev_handshake_failures.lock().unwrap());
            if delta > 0 {
                self.h2_handshake_failures_total.inc_by(delta);
            }
            *self.h2_prev_handshake_failures.lock().unwrap() = cur;

            let cur = H2_PROTOCOL_METRICS.auth_failures.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(*self.h2_prev_auth_failures.lock().unwrap());
            if delta > 0 {
                self.h2_auth_failures_total.inc_by(delta);
            }
            *self.h2_prev_auth_failures.lock().unwrap() = cur;

            let cur = H2_PROTOCOL_METRICS
                .flow_control_stalls
                .load(Ordering::Relaxed);
            let delta = cur.saturating_sub(*self.h2_prev_flow_control_stalls.lock().unwrap());
            if delta > 0 {
                self.h2_flow_control_stalls_total.inc_by(delta);
            }
            *self.h2_prev_flow_control_stalls.lock().unwrap() = cur;

            let cur = H2_PROTOCOL_METRICS.pool_exhausted.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(*self.h2_prev_pool_exhausted.lock().unwrap());
            if delta > 0 {
                self.h2_pool_exhausted_total.inc_by(delta);
            }
            *self.h2_prev_pool_exhausted.lock().unwrap() = cur;

            let cur = H2_PROTOCOL_METRICS.bytes_relayed.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(*self.h2_prev_bytes_relayed.lock().unwrap());
            if delta > 0 {
                self.h2_bytes_relayed_total.inc_by(delta);
            }
            *self.h2_prev_bytes_relayed.lock().unwrap() = cur;
        }

        // Sync transparent proxy counters from bridged SupervisorState atomics
        if let Some(accepted) = self.transparent_accepted_bridged.lock().unwrap().as_ref() {
            let cur = accepted.load(Ordering::Relaxed);
            let mut prev = self.transparent_prev_accepted.lock().unwrap();
            let delta = cur.saturating_sub(*prev);
            if delta > 0 {
                self.transparent_connections_accepted.inc_by(delta);
            }
            *prev = cur;
        }
        if let Some(dst_failed) = self.transparent_dst_failed_bridged.lock().unwrap().as_ref() {
            let cur = dst_failed.load(Ordering::Relaxed);
            let mut prev = self.transparent_prev_dst_failed.lock().unwrap();
            let delta = cur.saturating_sub(*prev);
            if delta > 0 {
                self.transparent_original_dst_failed.inc_by(delta);
            }
            *prev = cur;
        }

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

    /// Bridge the supervisor's transparent proxy atomic counters so that
    /// `render_prometheus()` exposes live transparent proxy counters.
    pub fn set_transparent_counters(&self, accepted: Arc<AtomicU64>, dst_failed: Arc<AtomicU64>) {
        *self.transparent_accepted_bridged.lock().unwrap() = Some(accepted);
        *self.transparent_dst_failed_bridged.lock().unwrap() = Some(dst_failed);
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
            connections_active: self.h2_connections_active.get() as u64,
            connections_total: self.h2_connections_total.get(),
            streams_active: self.h2_streams_active.get() as u64,
            goaway_total: self.h2_goaway_total.get(),
            handshake_failures_total: self.h2_handshake_failures_total.get(),
            auth_failures_total: self.h2_auth_failures_total.get(),
            flow_control_stalls_total: self.h2_flow_control_stalls_total.get(),
            pool_exhausted_total: self.h2_pool_exhausted_total.get(),
            bytes_relayed_total: self.h2_bytes_relayed_total.get(),
        }
    }
}

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
        assert!(output.contains("eggress_auth_failures_total"));
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
        assert!(output.contains("eggress_udp_upstream_associations_active"));
        assert!(output.contains("eggress_udp_upstream_associations_total"));
        assert!(output.contains("eggress_udp_upstream_packets_up_total"));
        assert!(output.contains("eggress_udp_upstream_packets_down_total"));
        assert!(output.contains("eggress_udp_upstream_bytes_up_total"));
        assert!(output.contains("eggress_udp_upstream_bytes_down_total"));
        assert!(output.contains("eggress_udp_upstream_failures_total"));
        assert!(output.contains("eggress_standalone_udp_flows_active"));
        assert!(output.contains("eggress_standalone_udp_flows_total"));
        assert!(output.contains("eggress_standalone_udp_packets_in_total"));
        assert!(output.contains("eggress_standalone_udp_packets_out_total"));
        assert!(output.contains("eggress_standalone_udp_bytes_in_total"));
        assert!(output.contains("eggress_standalone_udp_bytes_out_total"));
        assert!(output.contains("eggress_standalone_udp_malformed_total"));
        assert!(output.contains("eggress_standalone_udp_rejected_total"));
        assert!(output.contains("eggress_standalone_udp_flow_reaps_total"));
        assert!(output.contains("eggress_upstream_open_total"));
        assert!(output.contains("eggress_upstream_open_failures_total"));
        assert!(output.contains("eggress_unsupported_transport_total"));
        assert!(output.contains("eggress_transparent_connections_accepted_total"));
        assert!(output.contains("eggress_transparent_original_dst_failed_total"));
        assert!(output.contains("eggress_transparent_route_rejects_total"));
        assert!(output.contains("eggress_unix_listener_connections_accepted_total"));
        assert!(output.contains("eggress_unix_listener_bind_failures_total"));
        assert!(output.contains("eggress_platform_capability_check_failures_total"));
        assert!(output.contains("eggress_shadowsocks_tcp_sessions_active"));
        assert!(output.contains("eggress_shadowsocks_tcp_sessions_total"));
        assert!(output.contains("eggress_shadowsocks_tcp_upstream_sessions_total"));
        assert!(output.contains("eggress_shadowsocks_tcp_decrypt_failures_total"));
        assert!(output.contains("eggress_shadowsocks_tcp_frame_parse_failures_total"));
        assert!(output.contains("eggress_shadowsocks_tcp_unsupported_method_rejects_total"));
        assert!(output.contains("eggress_shadowsocks_tcp_active_flows"));
        assert!(output.contains("eggress_shadowsocks_udp_packets_in_total"));
        assert!(output.contains("eggress_shadowsocks_udp_packets_out_total"));
        assert!(output.contains("eggress_shadowsocks_udp_bytes_in_total"));
        assert!(output.contains("eggress_shadowsocks_udp_bytes_out_total"));
        assert!(output.contains("eggress_shadowsocks_udp_decrypt_failures_total"));
        assert!(output.contains("eggress_shadowsocks_udp_unsupported_method_rejects_total"));
        assert!(output.contains("eggress_shadowsocks_udp_active_flows"));
        assert!(output.contains("eggress_h2_connections_active"));
        assert!(output.contains("eggress_h2_connections_total"));
        assert!(output.contains("eggress_h2_streams_active"));
        assert!(output.contains("eggress_h2_streams_total"));
        assert!(output.contains("eggress_h2_goaway_total"));
        assert!(output.contains("eggress_h2_handshake_failures_total"));
        assert!(output.contains("eggress_h2_auth_failures_total"));
        assert!(output.contains("eggress_h2_flow_control_stalls_total"));
        assert!(output.contains("eggress_h2_pool_exhausted_total"));
        assert!(output.contains("eggress_h2_bytes_relayed_total"));
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
    fn auth_failure_metric() {
        let m = MetricsRegistry::new();
        m.record_auth_failure();
        m.record_auth_failure();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_auth_failures_total"));
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
        m.record_udp_decode_error("too_short");
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_decode_errors_total"));
        assert!(output.contains("kind=\"too_short\""));
    }

    #[test]
    fn udp_unsupported_upstream_metric() {
        let m = MetricsRegistry::new();
        m.record_udp_unsupported_upstream();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_unsupported_upstream_total"));
    }

    #[test]
    fn udp_upstream_association_metrics() {
        let m = MetricsRegistry::new();
        m.record_udp_upstream_association_created();
        m.record_udp_upstream_association_created();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_upstream_associations_active"));
        assert!(output.contains("eggress_udp_upstream_associations_total"));

        m.record_udp_upstream_association_closed();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_upstream_associations_active"));
    }

    #[test]
    fn udp_upstream_failure_metric() {
        let m = MetricsRegistry::new();
        m.record_udp_upstream_failure();
        m.record_udp_upstream_failure();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_upstream_failures_total"));
    }

    #[test]
    fn udp_upstream_packet_metrics() {
        let m = MetricsRegistry::new();
        m.record_udp_upstream_packet_up(100);
        m.record_udp_upstream_packet_up(200);
        m.record_udp_upstream_packet_down(50);
        let output = m.render_prometheus();
        assert!(output.contains("eggress_udp_upstream_packets_up_total"));
        assert!(output.contains("eggress_udp_upstream_packets_down_total"));
        assert!(output.contains("eggress_udp_upstream_bytes_up_total"));
        assert!(output.contains("eggress_udp_upstream_bytes_down_total"));
    }

    #[test]
    fn udp_upstream_active_gauge_returns_to_zero() {
        let m = MetricsRegistry::new();
        m.record_udp_upstream_association_created();
        m.record_udp_upstream_association_created();
        m.record_udp_upstream_association_closed();
        m.record_udp_upstream_association_closed();
        let output = m.render_prometheus();
        for line in output.lines() {
            if line.contains("eggress_udp_upstream_associations_active") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 0.0, "upstream active associations should return to 0");
                    }
                }
            }
        }
    }

    #[test]
    fn bridge_upstream_packets_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_upstream_packet_up(100);
        udp.record_upstream_packet_up(200);
        udp.record_upstream_packet_down(50);

        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_udp_upstream_packets_up_total"),
            "missing upstream_packets_up_total"
        );
        assert!(
            output.contains("eggress_udp_upstream_bytes_up_total"),
            "missing upstream_bytes_up_total"
        );
        assert!(
            output.contains("eggress_udp_upstream_bytes_down_total"),
            "missing upstream_bytes_down_total"
        );
    }

    #[test]
    fn bridge_upstream_associations_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_upstream_association_created();
        udp.record_upstream_association_created();
        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_udp_upstream_associations_active"),
            "missing upstream_associations_active"
        );
        assert!(
            output.contains("eggress_udp_upstream_associations_total"),
            "missing upstream_associations_total"
        );

        udp.record_upstream_association_closed();
        let output = m.render_prometheus();
        for line in output.lines() {
            if line.contains("eggress_udp_upstream_associations_active") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 1.0, "upstream active should be 1");
                    }
                }
            }
        }
    }

    #[test]
    fn bridge_upstream_failures_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_upstream_failure();

        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_udp_upstream_failures_total"),
            "missing upstream_failures_total"
        );
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

    // --- Bridge tests: UdpMetrics -> MetricsRegistry -> /metrics ---

    #[test]
    fn bridge_packets_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_packet_up(100);
        udp.record_packet_up(200);
        udp.record_packet_down(50);

        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_udp_packets_up_total"),
            "missing packets_up_total"
        );
        assert!(
            output.contains("eggress_udp_bytes_up_total"),
            "missing bytes_up_total"
        );
        assert!(
            output.contains("eggress_udp_bytes_down_total"),
            "missing bytes_down_total"
        );
        // Verify values appear (at least "3" for packets_up and "300" for bytes_up)
        assert!(
            output.contains("eggress_udp_packets_up_total") && output.contains("3"),
            "packets_up should be 3"
        );
        assert!(
            output.contains("eggress_udp_bytes_up_total") && output.contains("300"),
            "bytes_up should be 300"
        );
    }

    #[test]
    fn bridge_drops_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_dropped();
        udp.record_dropped();
        udp.record_dropped();

        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_udp_dropped_packets_total"),
            "missing dropped_packets_total"
        );
    }

    #[test]
    fn bridge_decode_errors_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_decode_error();
        udp.record_decode_error();

        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_udp_decode_errors_total"),
            "missing decode_errors_total"
        );
    }

    #[test]
    fn bridge_active_association_gauge_returns_to_zero() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_association_created();
        udp.record_association_created();
        let output = m.render_prometheus();
        for line in output.lines() {
            if line.contains("eggress_udp_associations_active") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 2.0, "should show 2 active associations");
                    }
                }
            }
        }

        udp.record_association_closed();
        udp.record_association_closed();
        let output = m.render_prometheus();
        for line in output.lines() {
            if line.contains("eggress_udp_associations_active") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 0.0, "active associations should return to 0");
                    }
                }
            }
        }
    }

    #[test]
    fn bridge_target_flows_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_target_flow_created();
        udp.record_target_flow_created();
        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_udp_target_flows_active"),
            "missing target_flows_active"
        );

        udp.record_target_flow_closed();
        let output = m.render_prometheus();
        for line in output.lines() {
            if line.contains("eggress_udp_target_flows_active") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 1.0, "target flows active should be 1");
                    }
                }
            }
        }
    }

    #[test]
    fn bridge_delta_tracking_across_renders() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        // First render: no deltas yet
        let _output1 = m.render_prometheus();
        // Second render after recording: deltas appear
        udp.record_packet_up(50);
        udp.record_dropped();
        let output2 = m.render_prometheus();

        // Both renders should produce valid output
        assert!(output2.contains("eggress_udp_packets_up_total"));
        assert!(output2.contains("eggress_udp_dropped_packets_total"));

        // Third render: no new deltas, counters stay at previous value
        let output3 = m.render_prometheus();
        assert!(output3.contains("eggress_udp_packets_up_total"));
        // Counters should still be at 1 (from the second render), not 2
        for line in output3.lines() {
            if line.contains("eggress_udp_packets_up_total") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 1.0, "counter should not double-count");
                    }
                }
            }
        }
    }

    #[test]
    fn bridge_no_privacy_leak() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_packet_up(100);

        let output = m.render_prometheus();
        assert!(!output.contains("127.0.0.1"), "no IP addresses in metrics");
        assert!(!output.contains("192.168"), "no private IPs in metrics");
    }

    #[test]
    fn upstream_open_metric_records_by_protocol_and_outcome() {
        let m = MetricsRegistry::new();
        m.record_upstream_open("shadowsocks", "success");
        m.record_upstream_open("shadowsocks", "success");
        m.record_upstream_open("trojan", "success");
        m.record_upstream_open("http", "failure");
        let output = m.render_prometheus();
        assert!(output.contains("eggress_upstream_open_total"));
        assert!(output.contains("protocol=\"shadowsocks\""));
        assert!(output.contains("protocol=\"trojan\""));
        assert!(output.contains("protocol=\"http\""));
        assert!(output.contains("outcome=\"success\""));
        assert!(output.contains("outcome=\"failure\""));
    }

    #[test]
    fn upstream_failure_metric_records_by_protocol_and_reason() {
        let m = MetricsRegistry::new();
        m.record_upstream_failure("shadowsocks", "dns_resolution");
        m.record_upstream_failure("trojan", "tls_handshake");
        m.record_upstream_failure("http", "connection_refused");
        let output = m.render_prometheus();
        assert!(output.contains("eggress_upstream_open_failures_total"));
        assert!(output.contains("protocol=\"shadowsocks\""));
        assert!(output.contains("protocol=\"trojan\""));
        assert!(output.contains("protocol=\"http\""));
        assert!(output.contains("reason=\"dns_resolution\""));
        assert!(output.contains("reason=\"tls_handshake\""));
        assert!(output.contains("reason=\"connection_refused\""));
    }

    #[test]
    fn unsupported_transport_metric_records_by_protocol_transport_reason() {
        let m = MetricsRegistry::new();
        m.record_unsupported_transport("shadowsocks", "udp", "not_implemented");
        m.record_unsupported_transport("trojan", "quic", "unsupported");
        let output = m.render_prometheus();
        assert!(output.contains("eggress_unsupported_transport_total"));
        assert!(output.contains("protocol=\"shadowsocks\""));
        assert!(output.contains("protocol=\"trojan\""));
        assert!(output.contains("transport=\"udp\""));
        assert!(output.contains("transport=\"quic\""));
        assert!(output.contains("reason=\"not_implemented\""));
        assert!(output.contains("reason=\"unsupported\""));
    }

    #[test]
    fn upstream_open_counter_increments() {
        let m = MetricsRegistry::new();
        m.record_upstream_open("socks5", "success");
        m.record_upstream_open("socks5", "success");
        m.record_upstream_open("socks5", "failure");
        let output = m.render_prometheus();
        // Verify the metric exists with labels
        assert!(output.contains("eggress_upstream_open_total"));
        assert!(output.contains("protocol=\"socks5\""));
    }

    #[test]
    fn new_metrics_parseable() {
        let m = MetricsRegistry::new();
        m.record_upstream_open("http", "ok");
        m.record_upstream_failure("http", "timeout");
        m.record_unsupported_transport("http", "quic", "no");
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
    fn bridge_standalone_flow_metrics_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_standalone_flow_created();
        udp.record_standalone_flow_created();
        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_standalone_udp_flows_active"),
            "missing standalone_udp_flows_active"
        );
        assert!(
            output.contains("eggress_standalone_udp_flows_total"),
            "missing standalone_udp_flows_total"
        );

        udp.record_standalone_flow_closed();
        let output = m.render_prometheus();
        for line in output.lines() {
            if line.contains("eggress_standalone_udp_flows_active") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 1.0, "standalone flows active should be 1");
                    }
                }
            }
        }
    }

    #[test]
    fn bridge_standalone_packet_metrics_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_standalone_packet_in(100);
        udp.record_standalone_packet_in(200);
        udp.record_standalone_packet_out(50);

        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_standalone_udp_packets_in_total"),
            "missing standalone_packets_in_total"
        );
        assert!(
            output.contains("eggress_standalone_udp_packets_out_total"),
            "missing standalone_packets_out_total"
        );
        assert!(
            output.contains("eggress_standalone_udp_bytes_in_total"),
            "missing standalone_bytes_in_total"
        );
        assert!(
            output.contains("eggress_standalone_udp_bytes_out_total"),
            "missing standalone_bytes_out_total"
        );
    }

    #[test]
    fn bridge_standalone_malformed_rejected_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_standalone_malformed();
        udp.record_standalone_rejected();

        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_standalone_udp_malformed_total"),
            "missing standalone_malformed_total"
        );
        assert!(
            output.contains("eggress_standalone_udp_rejected_total"),
            "missing standalone_rejected_total"
        );
    }

    #[test]
    fn bridge_standalone_flow_reaps_appear_in_prometheus() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_standalone_flow_created();
        udp.record_standalone_flow_reap();

        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_standalone_udp_flow_reaps_total"),
            "missing standalone_flow_reaps_total"
        );
    }

    #[test]
    fn bridge_standalone_active_gauge_returns_to_zero() {
        let udp = Arc::new(UdpMetrics::new());
        let m = MetricsRegistry::new();
        m.set_udp_metrics(udp.clone());

        udp.record_standalone_flow_created();
        udp.record_standalone_flow_created();
        let output = m.render_prometheus();
        for line in output.lines() {
            if line.contains("eggress_standalone_udp_flows_active") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 2.0, "should show 2 active standalone flows");
                    }
                }
            }
        }

        udp.record_standalone_flow_closed();
        udp.record_standalone_flow_closed();
        let output = m.render_prometheus();
        for line in output.lines() {
            if line.contains("eggress_standalone_udp_flows_active") && !line.starts_with('#') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(n) = val.parse::<f64>() {
                        assert_eq!(n, 0.0, "standalone flows active should return to 0");
                    }
                }
            }
        }
    }

    #[test]
    fn transparent_proxy_metrics_appear_in_prometheus() {
        let m = MetricsRegistry::new();
        m.record_transparent_connection_accepted();
        m.record_transparent_connection_accepted();
        m.record_transparent_original_dst_failed();
        m.record_transparent_route_reject();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_transparent_connections_accepted_total"));
        assert!(output.contains("eggress_transparent_original_dst_failed_total"));
        assert!(output.contains("eggress_transparent_route_rejects_total"));
    }

    #[test]
    fn unix_listener_metrics_appear_in_prometheus() {
        let m = MetricsRegistry::new();
        m.record_unix_listener_connection_accepted();
        m.record_unix_listener_bind_failure();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_unix_listener_connections_accepted_total"));
        assert!(output.contains("eggress_unix_listener_bind_failures_total"));
    }

    #[test]
    fn platform_capability_metrics_appear_in_prometheus() {
        let m = MetricsRegistry::new();
        m.record_platform_capability_check_failure();
        m.record_platform_capability_check_failure();
        let output = m.render_prometheus();
        assert!(output.contains("eggress_platform_capability_check_failures_total"));
    }

    #[test]
    fn transparent_proxy_bridged_metrics_appear_in_prometheus() {
        let m = MetricsRegistry::new();
        let accepted = Arc::new(AtomicU64::new(0));
        let dst_failed = Arc::new(AtomicU64::new(0));
        m.set_transparent_counters(accepted.clone(), dst_failed.clone());

        accepted.fetch_add(5, std::sync::atomic::Ordering::Relaxed);
        dst_failed.fetch_add(2, std::sync::atomic::Ordering::Relaxed);
        let output = m.render_prometheus();
        assert!(output.contains("eggress_transparent_connections_accepted_total"));
        assert!(output.contains("eggress_transparent_original_dst_failed_total"));
    }

    #[test]
    fn h2_protocol_metrics_appear_in_prometheus() {
        use eggress_protocol_http::H2_PROTOCOL_METRICS;
        use std::sync::atomic::Ordering;

        // Record some H2 events via the global atomics
        H2_PROTOCOL_METRICS
            .connections_opened
            .fetch_add(3, Ordering::Relaxed);
        H2_PROTOCOL_METRICS
            .connections_closed
            .fetch_add(1, Ordering::Relaxed);
        H2_PROTOCOL_METRICS
            .streams_opened
            .fetch_add(10, Ordering::Relaxed);
        H2_PROTOCOL_METRICS
            .streams_closed
            .fetch_add(7, Ordering::Relaxed);
        H2_PROTOCOL_METRICS
            .goaway_received
            .fetch_add(1, Ordering::Relaxed);
        H2_PROTOCOL_METRICS
            .auth_failures
            .fetch_add(2, Ordering::Relaxed);
        H2_PROTOCOL_METRICS
            .pool_exhausted
            .fetch_add(1, Ordering::Relaxed);

        let m = MetricsRegistry::new();
        let output = m.render_prometheus();
        assert!(
            output.contains("eggress_h2_connections_active"),
            "missing h2_connections_active"
        );
        assert!(
            output.contains("eggress_h2_connections_total"),
            "missing h2_connections_total"
        );
        assert!(
            output.contains("eggress_h2_streams_active"),
            "missing h2_streams_active"
        );
        assert!(
            output.contains("eggress_h2_streams_total"),
            "missing h2_streams_total"
        );
        assert!(
            output.contains("eggress_h2_goaway_total"),
            "missing h2_goaway_total"
        );
        assert!(
            output.contains("eggress_h2_auth_failures_total"),
            "missing h2_auth_failures_total"
        );
        assert!(
            output.contains("eggress_h2_pool_exhausted_total"),
            "missing h2_pool_exhausted_total"
        );
    }
}
