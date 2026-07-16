use eggress_protocol_http::connect::client::{
    parse_status_code, validate_credentials, HttpConnectLimits,
};
use eggress_protocol_http::connect::server::{
    parse_authority, parse_basic_auth, parse_header_line,
};

#[test]
fn fuzz_smoke_http_connect_response() {
    let text_inputs: &[&str] = &[
        "",
        "HTTP/1.1 200 OK\r\n",
        "HTTP/1.1 407 Proxy Authentication Required\r\n",
        "HTTP/1.1 504 Gateway Timeout\r\n",
        "\u{0000}\u{0001}\u{ffff}",
        "HTTP/1.1 999 Unknown\r\n",
        "not-http-at-all",
        "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
    ];
    let limits = HttpConnectLimits::default();
    for input in text_inputs {
        let _ = parse_status_code(input, &limits);
    }
}

#[test]
fn fuzz_smoke_http_connect_authority() {
    let inputs: &[&str] = &[
        "",
        "example.com:443",
        "[::1]:8080",
        "127.0.0.1:80",
        "host-without-port",
        ":99999",
        "\u{0000}\u{0001}",
        "example.com:abc",
    ];
    for input in inputs {
        let _ = parse_authority(input);
    }
}

#[test]
fn fuzz_smoke_http_header_parsers() {
    let lines: &[&str] = &[
        "",
        "Host: example.com",
        "Content-Length: 0",
        "Proxy-Authorization: Basic dXNlcjpwYXNz",
        "\u{0000}bad-header",
        "X-Long: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "Authorization: Bearer token123",
    ];
    for line in lines {
        let _ = parse_header_line(line);
        let _ = parse_basic_auth(line);
    }
}

#[test]
fn fuzz_smoke_http_validate_credentials() {
    let inputs: &[&str] = &[
        "",
        "Basic dXNlcjpwYXNz",
        "Bearer token",
        "dXNlcjpwYXNz",
        "\u{0000}",
    ];
    for input in inputs {
        let _ = validate_credentials(input);
    }
}
