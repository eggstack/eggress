#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Exercise the WebSocket error type construction with fuzz data.
    // This verifies that error formatting never panics regardless of input.
    if let Ok(text) = std::str::from_utf8(data) {
        let err = eggress_protocol_websocket::error::WebSocketError::Handshake(text.to_string());
        let _ = err.to_string();

        let err = eggress_protocol_websocket::error::WebSocketError::Connect(text.to_string());
        let _ = err.to_string();

        let err = eggress_protocol_websocket::error::WebSocketError::Protocol(text.to_string());
        let _ = err.to_string();
    }

    // Exercise the message-too-large error with fuzz sizes.
    if data.len() >= 8 {
        let size = u64::from_be_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]) as usize;
        let max = size / 2;
        let err = eggress_protocol_websocket::error::WebSocketError::MessageTooLarge { size, max };
        let _ = err.to_string();
    }

    // Exercise WebSocketTunnelServer construction with fuzz config values.
    if data.len() >= 8 {
        let max_msg = usize::from_be_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);
        let _server = eggress_protocol_websocket::WebSocketTunnelServer::new(max_msg);
        let _client = eggress_protocol_websocket::WebSocketTunnelClient::new(max_msg);
    }
});
