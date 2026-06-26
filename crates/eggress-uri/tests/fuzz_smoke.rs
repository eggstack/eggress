use eggress_uri::parse_proxy_chain;

#[test]
fn fuzz_smoke_uri_parse() {
    let inputs: &[&str] = &[
        "",
        "http",
        "://",
        "http://user:pass@host:8080",
        "socks5://host:1080//socks5://host2:1080",
        "\u{0000}\u{0001}\u{ffff}",
        "http://",
        "socks5://[::1]:1080",
        "http://host:99999",
        "http://very-long-hostname-that-exceeds-normal-limits.example.com:80/path?query=value&another=param#fragment",
    ];
    for input in inputs {
        let _ = parse_proxy_chain(input);
    }
}
