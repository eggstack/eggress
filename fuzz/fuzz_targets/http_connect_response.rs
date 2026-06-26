#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Decode arbitrary bytes as UTF-8 (lossy) so the parser sees a string
    // view of the response. We split on \r\n\r\n to simulate a header
    // terminator, then feed the head to the status-code parser.
    let head = match data.iter().position(|&b| b == b'\n') {
        Some(_) => data,
        None => data,
    };

    // Try interpreting the head as a UTF-8 string and parse the status code.
    if let Ok(text) = std::str::from_utf8(head) {
        let limits = eggress_protocol_http::connect::client::HttpConnectLimits::default();
        let _ = eggress_protocol_http::connect::client::parse_status_code(text, &limits);
    }

    // Also exercise the server-side authority / header / basic-auth parsers
    // by treating the fuzz input as authority strings and header lines.
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = eggress_protocol_http::connect::server::parse_authority(text);

        for line in text.split(['\n', '\r']) {
            let _ = eggress_protocol_http::connect::server::parse_header_line(line);
            let _ = eggress_protocol_http::connect::server::parse_basic_auth(line);
        }
    }
});
