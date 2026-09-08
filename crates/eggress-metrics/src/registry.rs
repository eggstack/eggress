//! Registry setup: the [`MetricsRegistry`] struct owns every Prometheus
//! family plus the bridge slots that mirror subsystem-native counters.
//!
//! Subsystem atomics (UDP relay, Shadowsocks, H2, transparent proxy) are the
//! canonical stored values for their counters; the Prometheus counters here
//! are exposition mirrors fed by delta promotion at render time because
//! `prometheus_client::Counter` is increment-only. See `render.rs` and the
//! crate docs for the ownership rules.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

#[cfg(feature = "extended")]
use eggress_protocol_shadowsocks::ShadowsocksMetrics;
use eggress_udp::metrics::UdpMetrics;

use super::labels::{
    DecodeErrorLabels, H2StreamLabels, RouteLabels, UnsupportedTransportLabels,
    UpstreamFailureLabels, UpstreamLabels, UpstreamOpenLabels,
};
#[cfg(feature = "extended")]
use super::shadowsocks::BridgedShadowsocksSnapshot;
use super::udp::BridgedUdpSnapshot;

#[allow(dead_code)]
pub struct MetricsRegistry {
    pub(crate) registry: Registry,
    pub(crate) connections_active: Gauge,
    pub(crate) connections_total: Counter,
    pub(crate) connection_failures: Counter,
    pub(crate) auth_failures: Counter,
    pub(crate) bytes_upstream_total: Counter,
    pub(crate) bytes_downstream_total: Counter,
    pub(crate) route_decisions: Family<RouteLabels, Counter>,
    pub(crate) upstream_health: Family<UpstreamLabels, Gauge>,
    pub(crate) reload_total: Counter,
    pub(crate) reload_failures: Counter,
    pub(crate) config_generation: Gauge,
    pub(crate) udp_associations_active: Gauge,
    pub(crate) udp_associations_total: Counter,
    pub(crate) udp_association_failures: Counter,
    pub(crate) udp_association_timeouts: Counter,
    pub(crate) udp_packets_up_total: Counter,
    pub(crate) udp_packets_down_total: Counter,
    pub(crate) udp_bytes_up_total: Counter,
    pub(crate) udp_bytes_down_total: Counter,
    pub(crate) udp_dropped_packets_total: Counter,
    pub(crate) udp_dropped_encode_errors_total: Counter,
    pub(crate) udp_dropped_send_errors_total: Counter,
    pub(crate) udp_dropped_response_channel_full_total: Counter,
    pub(crate) udp_target_flows_active: Gauge,
    pub(crate) udp_target_flows_total: Counter,
    pub(crate) udp_decode_errors_total: Family<DecodeErrorLabels, Counter>,
    pub(crate) udp_unsupported_upstream_total: Counter,
    pub(crate) udp_upstream_associations_active: Gauge,
    pub(crate) udp_upstream_associations_total: Counter,
    pub(crate) udp_upstream_packets_up_total: Counter,
    pub(crate) udp_upstream_packets_down_total: Counter,
    pub(crate) udp_upstream_bytes_up_total: Counter,
    pub(crate) udp_upstream_bytes_down_total: Counter,
    pub(crate) udp_upstream_failures_total: Counter,
    pub(crate) standalone_udp_flows_active: Gauge,
    pub(crate) standalone_udp_flows_total: Counter,
    pub(crate) standalone_udp_packets_in_total: Counter,
    pub(crate) standalone_udp_packets_out_total: Counter,
    pub(crate) standalone_udp_bytes_in_total: Counter,
    pub(crate) standalone_udp_bytes_out_total: Counter,
    pub(crate) standalone_udp_malformed_total: Counter,
    pub(crate) standalone_udp_rejected_total: Counter,
    pub(crate) standalone_udp_flow_reaps_total: Counter,
    pub(crate) upstream_open_total: Family<UpstreamOpenLabels, Counter>,
    pub(crate) upstream_open_failures_total: Family<UpstreamFailureLabels, Counter>,
    pub(crate) unsupported_transport_total: Family<UnsupportedTransportLabels, Counter>,
    pub(crate) transparent_connections_accepted: Counter,
    pub(crate) transparent_original_dst_failed: Counter,
    pub(crate) transparent_route_rejects: Counter,
    pub(crate) unix_listener_connections_accepted: Counter,
    pub(crate) unix_listener_bind_failures: Counter,
    pub(crate) platform_capability_check_failures: Counter,
    pub(crate) ss_tcp_sessions_active: Gauge,
    pub(crate) ss_tcp_sessions_total: Counter,
    pub(crate) ss_tcp_upstream_sessions_total: Counter,
    pub(crate) ss_tcp_decrypt_failures_total: Counter,
    pub(crate) ss_tcp_frame_parse_failures_total: Counter,
    pub(crate) ss_tcp_unsupported_method_rejects_total: Counter,
    pub(crate) ss_tcp_active_flows: Gauge,
    pub(crate) ss_udp_packets_in_total: Counter,
    pub(crate) ss_udp_packets_out_total: Counter,
    pub(crate) ss_udp_bytes_in_total: Counter,
    pub(crate) ss_udp_bytes_out_total: Counter,
    pub(crate) ss_udp_decrypt_failures_total: Counter,
    pub(crate) ss_udp_unsupported_method_rejects_total: Counter,
    pub(crate) ss_udp_active_flows: Gauge,
    pub(crate) h2_connections_active: Gauge,
    pub(crate) h2_connections_total: Counter,
    pub(crate) h2_streams_active: Gauge,
    pub(crate) h2_streams_total: Family<H2StreamLabels, Counter>,
    pub(crate) h2_goaway_total: Counter,
    pub(crate) h2_handshake_failures_total: Counter,
    pub(crate) h2_auth_failures_total: Counter,
    pub(crate) h2_flow_control_stalls_total: Counter,
    pub(crate) h2_pool_exhausted_total: Counter,
    pub(crate) h2_bytes_relayed_total: Counter,
    pub(crate) transparent_accepted_bridged: Mutex<Option<Arc<AtomicU64>>>,
    pub(crate) transparent_dst_failed_bridged: Mutex<Option<Arc<AtomicU64>>>,
    pub(crate) transparent_prev_accepted: Mutex<u64>,
    pub(crate) transparent_prev_dst_failed: Mutex<u64>,
    pub(crate) bridged_udp_metrics: Mutex<Option<(Arc<UdpMetrics>, BridgedUdpSnapshot)>>,
    #[cfg(feature = "extended")]
    pub(crate) bridged_shadowsocks_metrics:
        Mutex<Option<(Arc<ShadowsocksMetrics>, BridgedShadowsocksSnapshot)>>,
    pub(crate) h2_prev_connections_opened: Mutex<u64>,
    pub(crate) h2_prev_streams_opened: Mutex<u64>,
    pub(crate) h2_prev_streams_closed: Mutex<u64>,
    pub(crate) h2_prev_goaway: Mutex<u64>,
    pub(crate) h2_prev_handshake_failures: Mutex<u64>,
    pub(crate) h2_prev_auth_failures: Mutex<u64>,
    pub(crate) h2_prev_flow_control_stalls: Mutex<u64>,
    pub(crate) h2_prev_pool_exhausted: Mutex<u64>,
    pub(crate) h2_prev_bytes_relayed: Mutex<u64>,
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

        let udp_dropped_encode_errors_total = Counter::default();
        registry.register(
            "eggress_udp_dropped_encode_errors_total",
            "Total UDP datagrams dropped because response encoding failed",
            udp_dropped_encode_errors_total.clone(),
        );

        let udp_dropped_send_errors_total = Counter::default();
        registry.register(
            "eggress_udp_dropped_send_errors_total",
            "Total UDP datagrams dropped because response sending failed",
            udp_dropped_send_errors_total.clone(),
        );

        let udp_dropped_response_channel_full_total = Counter::default();
        registry.register(
            "eggress_udp_dropped_response_channel_full_total",
            "Total UDP response datagrams dropped because the relay response channel was full",
            udp_dropped_response_channel_full_total.clone(),
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
            udp_dropped_encode_errors_total,
            udp_dropped_send_errors_total,
            udp_dropped_response_channel_full_total,
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
            #[cfg(feature = "extended")]
            bridged_shadowsocks_metrics: Mutex::new(None),
            h2_prev_connections_opened: Mutex::new(0),
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
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}
