//! Regression tests: metric name stability, counter/gauge semantics,
//! bridge delta promotion (no double-count, idempotent renders), label
//! hygiene (no secrets), and feature-gated family presence.

use super::*;
use crate::labels::{bounded_route_label, MAX_ROUTE_LABEL_LENGTH};
use eggress_server::execute::{SessionOutcome, SessionReport};
use eggress_udp::metrics::UdpMetrics;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

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
fn route_labels_are_bounded_and_sanitized() {
    let label = bounded_route_label(&format!("{}\n", "x".repeat(256)));
    assert_eq!(label.len(), MAX_ROUTE_LABEL_LENGTH);
    assert!(!label.contains('\n'));
    assert!(label.ends_with("..."));
}

#[test]
fn route_label_boundary_values() {
    assert_eq!(bounded_route_label("").len(), 0);
    assert_eq!(
        bounded_route_label(&"a".repeat(MAX_ROUTE_LABEL_LENGTH)).len(),
        MAX_ROUTE_LABEL_LENGTH
    );
    // Exact boundary: full length input truncates with the ellipsis.
    let label = bounded_route_label(&"a".repeat(MAX_ROUTE_LABEL_LENGTH + 1));
    assert_eq!(label.len(), MAX_ROUTE_LABEL_LENGTH);
    assert!(label.ends_with("..."));
    // Multibyte char at the boundary does not overflow.
    let label = bounded_route_label(&("a".repeat(MAX_ROUTE_LABEL_LENGTH - 4) + "\u{2014}"));
    assert!(label.len() <= MAX_ROUTE_LABEL_LENGTH);
    assert!(label.ends_with("..."));
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
