//! Phase 2 cross-parser equivalence: shared syntax, separate grammars.
//!
//! ```text
//! shared lexical primitives != shared grammar
//! ```
//!
//! Both parsers build on `eggress_uri::syntax` for endpoint/userinfo/
//! chain-splitting/redaction where semantics are identical, then apply their
//! own grammar policy. These tests pin the shared corpus plus the intentional
//! differences (empty hosts, default ports, percent-decoding, compat-only
//! pseudo-protocols) so future cleanups cannot silently merge the grammars.

use eggress_pproxy_compat::uri::{parse_pproxy_chain, parse_pproxy_uri};
use eggress_uri::syntax;

// Shared lexical primitives agree on userinfo separation.
#[test]
fn userinfo_separator_is_last_at_outside_brackets() {
    assert_eq!(
        syntax::find_userinfo_separator("user:p@ss@proxy:8080"),
        Some("user:p@ss".len())
    );
    assert_eq!(syntax::find_userinfo_separator("proxy:8080"), None);
    assert_eq!(syntax::find_userinfo_separator("[::1]:8080"), None);
}

// Shared host/port split covers the common endpoint corpus.
#[test]
fn shared_host_port_corpus() {
    for (input, host, port) in [
        ("192.0.2.1:1080", "192.0.2.1", 1080),
        ("proxy.example:8080", "proxy.example", 8080),
        ("[::1]:8080", "::1", 8080),
        ("[2001:db8::1]:1080", "2001:db8::1", 1080),
    ] {
        let hp = syntax::parse_host_port(input).expect("shared parse");
        assert_eq!(hp.host, host, "host for {input}");
        assert_eq!(hp.port, Some(port), "port for {input}");
        assert!(hp.port_specified);

        // Both grammars accept these endpoints with explicit ports.
        let native = eggress_uri::parse_proxy_chain(&format!("http://{input}")).unwrap();
        assert_eq!(native.hops[0].endpoint.host, host);
        assert_eq!(native.hops[0].endpoint.port, port);

        let compat = parse_pproxy_uri(&format!("http://{input}")).unwrap();
        assert_eq!(compat.host, host);
        assert_eq!(compat.port, port);
    }
}

// Userinfo with password containing `@` uses the last separator in both.
#[test]
fn at_in_password_preserved_by_both() {
    let native = eggress_uri::parse_proxy_chain("http://admin:s3cret_p@ssw0rd@proxy:8080").unwrap();
    let creds = native.hops[0].credentials.as_ref().unwrap();
    assert_eq!(creds.username, "admin");
    assert_eq!(creds.password, "s3cret_p@ssw0rd");

    let compat = parse_pproxy_uri("http://admin:s3cret_p@ssw0rd@proxy:8080").unwrap();
    assert_eq!(compat.username.as_deref(), Some("admin"));
    assert_eq!(compat.password.as_deref(), Some("s3cret_p@ssw0rd"));
}

// Chain splitting agrees on two- and three-hop forms.
#[test]
fn chain_splitting_two_and_three_hops() {
    let native = eggress_uri::parse_proxy_chain("socks5://hop1:1080__http://hop2:8080").unwrap();
    assert_eq!(native.hops.len(), 2);

    let compat = parse_pproxy_chain("socks5://hop1:1080__http://hop2:8080").unwrap();
    assert_eq!(compat.hops.len(), 2);
    assert_eq!(compat.hops[0].host, "hop1");
    assert_eq!(compat.hops[1].host, "hop2");

    let native =
        eggress_uri::parse_proxy_chain("http://h1:80__socks5://h2:1080__socks4://h3:1080").unwrap();
    assert_eq!(native.hops.len(), 3);
    let compat = parse_pproxy_chain("http://h1:80__socks5://h2:1080__socks4://h3:1080").unwrap();
    assert_eq!(compat.hops.len(), 3);
}

// TLS modifier normalizes the same way in both grammars.
#[test]
fn tls_modifier_agrees() {
    let native = eggress_uri::parse_proxy_chain("socks5+tls://proxy:1080").unwrap();
    assert!(native.hops[0].tls);

    let compat = parse_pproxy_uri("socks5+tls://proxy:1080").unwrap();
    assert!(compat.tls);
    assert_eq!(compat.scheme, "socks5");
}

// Native aliases lower identically through the compat path.
#[test]
fn aliases_lower_to_same_native_chain() {
    for (compat_uri, native_uri) in [
        (
            "ss://aes-256-gcm:secret@127.0.0.1:8388",
            "ss://aes-256-gcm:secret@127.0.0.1:8388",
        ),
        (
            "shadowsocks://aes-256-gcm:secret@127.0.0.1:8388",
            "shadowsocks://aes-256-gcm:secret@127.0.0.1:8388",
        ),
    ] {
        let chain = parse_pproxy_chain(compat_uri).unwrap();
        let lowered = eggress_pproxy_compat::translate::compile_chain_to_native(&chain).unwrap();
        let direct = eggress_uri::parse_proxy_chain(native_uri).unwrap();
        assert_eq!(lowered.hops[0].protocols, direct.hops[0].protocols);
        assert_eq!(lowered.hops[0].endpoint, direct.hops[0].endpoint);
    }

    // `raw`/`tunnel` alias to the same native syntax type.
    let raw = eggress_uri::parse_proxy_chain("raw://proxy:8080").unwrap();
    let tunnel = eggress_uri::parse_proxy_chain("tunnel://proxy:8080").unwrap();
    assert_eq!(raw.hops[0].protocols, tunnel.hops[0].protocols);

    // `ws`/`wss` alias to WebSocket natively.
    let ws = eggress_uri::parse_proxy_chain("ws://proxy:8080").unwrap();
    let wss = eggress_uri::parse_proxy_chain("wss://proxy:8080").unwrap();
    assert_eq!(ws.hops[0].protocols, wss.hops[0].protocols);
}

// Redaction never leaks credentials in either display path.
#[test]
fn redaction_hides_credentials_both_grammars() {
    let native = eggress_uri::parse_proxy_chain("http://user:s3cret@proxy:8080").unwrap();
    let redacted = eggress_uri::RedactedUri::new(&native).to_string();
    assert!(redacted.contains("****"));
    assert!(!redacted.contains("s3cret"));

    let compat = parse_pproxy_uri("http://user:s3cret@proxy:8080").unwrap();
    let display = compat.redacted_display();
    assert!(display.contains("****"));
    assert!(!display.contains("s3cret"));

    // Tolerant string redactor is scheme-agnostic and IPv6-safe.
    assert_eq!(
        eggress_uri::redact_proxy_uri("http://user:p@ss@[::1]:8080"),
        "http://****@[::1]:8080"
    );
}

// Percent-encoded delimiter bytes: native decodes, compat keeps verbatim.
// This intentional difference is pinned so it cannot silently converge.
#[test]
fn percent_encoded_password_difference_is_explicit() {
    let native = eggress_uri::parse_proxy_chain("http://user:p%40ss@proxy:8080").unwrap();
    let creds = native.hops[0].credentials.as_ref().unwrap();
    assert_eq!(creds.password, "p@ss");

    let compat = parse_pproxy_uri("http://user:p%40ss@proxy:8080").unwrap();
    assert_eq!(compat.password.as_deref(), Some("p%40ss"));
}

// Malformed inputs fail closed in both grammars.
#[test]
fn malformed_inputs_fail_closed_both() {
    // Unmatched brackets.
    assert!(eggress_uri::parse_proxy_chain("http://[::1:8080").is_err());
    assert!(parse_pproxy_uri("http://[::1:8080").is_err());
    assert!(eggress_uri::parse_proxy_chain("]__http://host:80").is_err());
    assert!(parse_pproxy_chain("http://h1:80__]").is_err());

    // Duplicate separators.
    assert!(eggress_uri::parse_proxy_chain("socks5://a:1080___http://b:8080").is_err());
    assert!(parse_pproxy_chain("http://h1:80____socks5://h2:1080").is_err());

    // Malformed / missing ports.
    assert!(eggress_uri::parse_proxy_chain("http://host:notaport").is_err());
    assert!(parse_pproxy_uri("http://host:notaport").is_err());
    assert!(eggress_uri::parse_proxy_chain("http://host").is_err());

    // Unbracketed IPv6 rejected where required.
    assert!(eggress_uri::parse_proxy_chain("http://::1:8080").is_err());
}

// Empty-host policy intentionally differs: compat listeners allow it,
// native proxy hops reject it.
#[test]
fn empty_host_policy_differs_by_grammar() {
    assert!(eggress_uri::parse_proxy_chain("http://:80").is_err());
    let compat = parse_pproxy_uri("socks5://:1080").unwrap();
    assert_eq!(compat.host, "");
    assert_eq!(compat.port, 1080);
}

// Compat-only constructs stay accepted/diagnosed and never enter native AST.
#[test]
fn compat_only_constructs_remain_explicit() {
    // `+in` backward count.
    let uri = parse_pproxy_uri("socks5+in+in://acceptor:1080").unwrap();
    assert!(uri.is_backward());
    assert_eq!(uri.backward_num(), 2);

    // Reverse pseudo-protocols.
    for scheme in ["bind", "listen", "backward", "rebind"] {
        let uri = parse_pproxy_uri(&format!("{scheme}://0.0.0.0:8080")).unwrap();
        assert!(uri.is_reverse_listener());
        assert!(eggress_uri::ProtocolSpec::parse_name(scheme).is_none());
    }

    // Plugin metadata + auth fragment.
    let uri =
        parse_pproxy_uri("http://proxy:8080/@192.0.2.1,verify_simple,plain#user:pass").unwrap();
    assert_eq!(uri.local_bind.as_deref(), Some("192.0.2.1"));
    assert_eq!(uri.plugins.len(), 2);
    assert_eq!(uri.auth_fragment.as_deref(), Some("user:pass"));

    // Fixed target + rule suffix.
    let uri = parse_pproxy_uri("tunnel://{example.com:443}?example\\.com$").unwrap();
    assert_eq!(uri.fixed_target.as_deref(), Some("example.com:443"));
}
