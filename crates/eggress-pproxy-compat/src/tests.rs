use crate::args::PproxyArgs;
use crate::translate::translate_pproxy_args;

#[test]
fn test_translate_produces_valid_toml_for_all_supported_local_protocols() {
    for scheme in &["http", "socks4", "socks5"] {
        let args =
            PproxyArgs::parse(&["-l".into(), format!("{}://127.0.0.1:1080", scheme)]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.toml.is_empty(), "empty TOML for scheme {}", scheme);
        let parsed: toml::Value = toml::from_str(&output.toml)
            .unwrap_or_else(|e| panic!("invalid TOML for scheme {}: {}", scheme, e));
        assert_eq!(parsed["version"].as_integer(), Some(1));
    }
}

#[test]
fn test_translate_all_supported_upstream_protocols() {
    for scheme in &["http", "socks4", "socks5", "trojan"] {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            format!("{}://proxy:8080", scheme),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let upstreams = parsed["upstreams"].as_array().unwrap();
        assert_eq!(upstreams.len(), 1, "expected 1 upstream for {}", scheme);
    }
}

#[test]
fn test_credentials_never_in_warnings_display() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://admin:hunter2@127.0.0.1:1080".into(),
        "-r".into(),
        "http://user:secret@proxy:8080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let warnings_str = output.warnings_to_string();
    assert!(
        !warnings_str.contains("hunter2"),
        "password leaked in warnings"
    );
    assert!(
        !warnings_str.contains("secret"),
        "password leaked in warnings"
    );
}

#[test]
fn test_unsupported_shadowsocks_returns_feature_not_panic() {
    let args =
        PproxyArgs::parse(&["-l".into(), "ss://aes-256-gcm:pass@proxy:8388".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.has_unsupported());
    assert!(output
        .unsupported
        .iter()
        .any(|u| u.feature == "shadowsocks-listener"));
}

#[test]
fn test_malformed_uri_gives_structured_error() {
    let args = PproxyArgs::parse(&["-l".into(), "not-a-uri".into()]);
    assert!(args.is_ok()); // parsing args itself succeeds
    let output = translate_pproxy_args(&args.unwrap());
    assert!(output.is_err());
}

#[test]
fn test_toml_has_stable_naming() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy:8080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("pproxy-local-0"));
    assert!(output.toml.contains("pproxy-upstream-0"));
    assert!(output.toml.contains("pproxy-chain"));
    assert!(output.toml.contains("pproxy-default"));
}

#[test]
fn test_direct_mode_warning() {
    let args = PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.warnings.iter().any(|w| w.category == "direct-mode"));
}
