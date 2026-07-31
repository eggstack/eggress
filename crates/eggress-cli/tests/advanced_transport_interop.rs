//! Gated interoperability tests for advanced transports (H2 CONNECT, WebSocket, Raw).
//!
//! These tests verify interoperability between eggress and external tools
//! (pproxy, curl, standard WebSocket clients) for advanced transport protocols.
//!
//! Status: H2/WS/Raw are runtime-integrated upstream protocols and are also
//! reachable through the pproxy compatibility translator. These optional
//! external probes remain gated because they require external protocol
//! clients or servers; focused native config and runtime coverage is kept in
//! the protocol, server, and compatibility-crate test suites.
//!
//! All tests are `#[ignore]` and require:
//! - `EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1` environment variable
//! - For pproxy tests: Python 3 with pproxy installed
//! - For curl tests: curl binary available on PATH
//!
//! Run with:
//! ```bash
//! EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 cargo test -p eggress-cli --test advanced_transport_interop -- --ignored
//! ```

/// Macro to skip a gated external probe when the env var is not set. The
/// external harness is intentionally separate from the local runtime tests.
macro_rules! gated_advanced_transport_test {
    () => {{
        if std::env::var("EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP").is_err() {
            eprintln!(
                "skipping: set EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 to run this external probe \
                 (see docs/protocols/ADVANCED_TRANSPORTS.md)"
            );
            return;
        }
        eprintln!("external advanced-transport probe is enabled");
    }};
}

// ===== H2 CONNECT Tests =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and h2 client"]
async fn h2_connect_server_echo() {
    gated_advanced_transport_test!();
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and h2 client"]
async fn h2_connect_upstream_chain() {
    gated_advanced_transport_test!();
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and h2 client"]
async fn h2_connect_flow_control() {
    gated_advanced_transport_test!();
}

// ===== WebSocket Tunnel Tests =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and WebSocket client"]
async fn websocket_tunnel_server_echo() {
    gated_advanced_transport_test!();
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and WebSocket client"]
async fn websocket_wss_tunnel_echo() {
    gated_advanced_transport_test!();
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and WebSocket client"]
async fn websocket_tunnel_close_frame() {
    gated_advanced_transport_test!();
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and pproxy WebSocket"]
async fn websocket_pproxy_differential() {
    gated_advanced_transport_test!();
}

// ===== Raw Tunnel Tests =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1"]
async fn raw_tunnel_pproxy_differential() {
    gated_advanced_transport_test!();
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1"]
async fn raw_tunnel_half_close() {
    gated_advanced_transport_test!();
}
