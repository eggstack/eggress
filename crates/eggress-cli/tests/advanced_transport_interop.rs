//! Gated interoperability tests for advanced transports (H2 CONNECT, WebSocket, Raw).
//!
//! These tests verify interoperability between eggress and external tools
//! (pproxy, curl, standard WebSocket clients) for advanced transport protocols.
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

fn require_advanced_transport_interop() {
    if std::env::var("EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP").is_err() {
        panic!("EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP not set");
    }
}

// ===== H2 CONNECT Tests =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and h2 client"]
async fn h2_connect_server_echo() {
    require_advanced_transport_interop();
    // TODO: Start eggress with H2 CONNECT listener (TLS + ALPN h2)
    // Connect with h2 client, issue CONNECT, verify bidirectional relay
    todo!("H2 CONNECT server echo test");
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and h2 client"]
async fn h2_connect_upstream_chain() {
    require_advanced_transport_interop();
    // TODO: Start SOCKS5 inbound -> H2 CONNECT upstream chain
    // Verify payload reaches echo server through H2 CONNECT tunnel
    todo!("H2 CONNECT upstream chain test");
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and h2 client"]
async fn h2_connect_flow_control() {
    require_advanced_transport_interop();
    // TODO: Send large payload (>64KB) through H2 CONNECT
    // Verify H2 flow control window updates work correctly
    todo!("H2 CONNECT flow control test");
}

// ===== WebSocket Tunnel Tests =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and WebSocket client"]
async fn websocket_tunnel_server_echo() {
    require_advanced_transport_interop();
    // TODO: Start eggress with WebSocket tunnel listener
    // Connect with standard WebSocket client, verify binary frame relay
    todo!("WebSocket tunnel server echo test");
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and WebSocket client"]
async fn websocket_wss_tunnel_echo() {
    require_advanced_transport_interop();
    // TODO: Start eggress with WSS (WebSocket over TLS) listener
    // Connect with TLS WebSocket client, verify secure tunnel
    todo!("WebSocket WSS tunnel echo test");
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and WebSocket client"]
async fn websocket_tunnel_close_frame() {
    require_advanced_transport_interop();
    // TODO: Establish WebSocket tunnel, send close frame
    // Verify clean shutdown propagation
    todo!("WebSocket tunnel close frame test");
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 and pproxy WebSocket"]
async fn websocket_pproxy_differential() {
    require_advanced_transport_interop();
    // TODO: Start pproxy with ws:// listener, start eggress with ws:// listener
    // Send identical payloads, compare relay behavior
    todo!("WebSocket pproxy differential test");
}

// ===== Raw Tunnel Tests =====

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1"]
async fn raw_tunnel_pproxy_differential() {
    require_advanced_transport_interop();
    // TODO: Start pproxy with raw:// listener, start eggress with raw:// listener
    // Connect TCP clients, verify bidirectional relay
    todo!("Raw tunnel pproxy differential test");
}

#[tokio::test]
#[ignore = "requires EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1"]
async fn raw_tunnel_half_close() {
    require_advanced_transport_interop();
    // TODO: Establish raw tunnel, shutdown write side of client
    // Verify server-side read returns EOF, server can still write
    todo!("Raw tunnel half-close test");
}
