#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Run the full SOCKS5 server-side handshake parser over arbitrary bytes.
    // Invariants:
    //   - no panic;
    //   - no unbounded allocation;
    //   - parser returns structured error or valid decoded value;
    //   - no infinite loop (bounded by input length).
    //
    // We split the input into two halves (or as close as we can get) so that
    // we exercise method negotiation parsing followed by request parsing,
    // matching the on-wire order: client sends method negotiation, server
    // replies, then client sends the request.
    if data.len() < 2 {
        // Truncated: method negotiation alone requires at least 2 bytes.
        let _ = eggress_protocol_socks::socks5::server::parse_method_negotiation(data);
        return;
    }

    let split = 2 + data[1] as usize;
    let split = split.min(data.len());
    let (method_bytes, rest) = data.split_at(split);

    let _ = eggress_protocol_socks::socks5::server::parse_method_negotiation(method_bytes);

    if rest.is_empty() {
        return;
    }

    let _ = eggress_protocol_socks::socks5::server::parse_connect_request(rest);
    let _ = eggress_protocol_socks::socks5::server::parse_socks5_request(rest);
});
