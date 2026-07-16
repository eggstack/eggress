use eggress_protocol_websocket::error::WebSocketError;
use eggress_protocol_websocket::{WebSocketTunnelClient, WebSocketTunnelServer};

#[test]
fn fuzz_smoke_websocket_error_display() {
    let strings: &[&str] = &["", "test", "\u{0000}\u{0001}\u{ffff}", &"x".repeat(10000)];
    for s in strings {
        let err = WebSocketError::Handshake(s.to_string());
        let _ = err.to_string();
        let err = WebSocketError::Connect(s.to_string());
        let _ = err.to_string();
        let err = WebSocketError::Protocol(s.to_string());
        let _ = err.to_string();
    }
}

#[test]
fn fuzz_smoke_websocket_message_too_large() {
    let sizes: &[usize] = &[0, 1, 1024, usize::MAX / 2, usize::MAX];
    for &size in sizes {
        let max = size / 2;
        let err = WebSocketError::MessageTooLarge { size, max };
        let _ = err.to_string();
    }
}

#[test]
fn fuzz_smoke_websocket_tunnel_construction() {
    let max_sizes: &[usize] = &[0, 1, 1024, 16 * 1024 * 1024, usize::MAX];
    for &max in max_sizes {
        let _server = WebSocketTunnelServer::new(max);
        let _client = WebSocketTunnelClient::new(max);
    }
}
