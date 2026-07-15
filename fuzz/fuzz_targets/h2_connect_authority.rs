#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        // Fuzz the authority parser used by H2 CONNECT.
        // Invariant: must never panic; returns Err for malformed input.
        let _ = eggress_protocol_http::connect::server::parse_authority(text);

        // Fuzz the header line parser with each line in the input.
        for line in text.lines() {
            let _ = eggress_protocol_http::connect::server::parse_header_line(line);
        }

        // Fuzz the basic auth parser.
        let _ = eggress_protocol_http::connect::server::parse_basic_auth(text);

        // Fuzz the status code parser with configurable limits.
        let limits = eggress_protocol_http::connect::client::HttpConnectLimits::default();
        let _ = eggress_protocol_http::connect::client::parse_status_code(text, &limits);

        // Fuzz with restrictive limits to verify bounds enforcement.
        let restrictive = eggress_protocol_http::connect::client::HttpConnectLimits {
            max_status_line: 32,
            max_headers_bytes: 128,
            max_header_count: 4,
        };
        let _ = eggress_protocol_http::connect::client::parse_status_code(text, &restrictive);

        // Fuzz credential validation.
        let _ = eggress_protocol_http::connect::client::validate_credentials(text);
    }
});
