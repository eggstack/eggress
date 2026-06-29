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
    for scheme in &["http", "socks4", "socks5", "trojan", "ss"] {
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

#[test]
fn test_socks4a_upstream_translates() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks4a://proxy:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let upstreams = parsed["upstreams"].as_array().unwrap();
    assert_eq!(upstreams.len(), 1);
    assert!(upstreams[0]["uri"].as_str().unwrap().contains("socks4://"));
}

#[test]
fn test_https_upstream_translates_to_http_tls() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "https://proxy:443".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let upstreams = parsed["upstreams"].as_array().unwrap();
    assert_eq!(upstreams.len(), 1);
    let uri = upstreams[0]["uri"].as_str().unwrap();
    assert!(uri.starts_with("http://"));
    assert!(uri.contains("+tls"));
}

#[test]
fn test_ssh_upstream_unsupported() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "ssh://proxy:22".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.has_unsupported());
    assert!(output
        .unsupported
        .iter()
        .any(|u| u.feature == "ssh-upstream"));
}

#[test]
fn test_unix_upstream_unsupported() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "unix://host:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.has_unsupported());
    assert!(output
        .unsupported
        .iter()
        .any(|u| u.feature == "unix-upstream"));
}

#[test]
fn test_redir_upstream_unsupported() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "redir://proxy:8080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.has_unsupported());
    assert!(output
        .unsupported
        .iter()
        .any(|u| u.feature == "redir-upstream"));
}

#[test]
fn test_ul_generates_standalone_udp_listener() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-ul".into(),
        ":1081".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let listeners = parsed["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 1);
    let udp = &listeners[0]["udp"];
    assert_eq!(udp["mode"].as_str(), Some("standalone_pproxy_udp"));
    assert_eq!(udp["bind"].as_str(), Some("0.0.0.0:1081"));
    // TCP listener should NOT have udp_enabled
    assert!(listeners[0].get("udp_enabled").is_none());
}

#[test]
fn test_ul_and_ur_generates_udp_upstream_group() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-ul".into(),
        ":1081".into(),
        "-ur".into(),
        "socks5://proxy:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    // Should have UDP upstream
    let upstreams = parsed["upstreams"].as_array().unwrap();
    assert!(upstreams
        .iter()
        .any(|u| u["id"].as_str().unwrap().starts_with("pproxy-udp-upstream")));
    // Should have UDP upstream group
    let groups = parsed["upstream_groups"].as_array().unwrap();
    assert!(groups
        .iter()
        .any(|g| g["id"].as_str() == Some("pproxy-udp-chain")));
    // Should have UDP rule
    let rules = parsed["rules"].as_array().unwrap();
    let udp_rule = rules
        .iter()
        .find(|r| r["id"].as_str() == Some("pproxy-udp-default"))
        .expect("missing pproxy-udp-default rule");
    let match_expr = &udp_rule["match"];
    assert_eq!(match_expr["transport"].as_str(), Some("udp"));
}

#[test]
fn test_ul_and_ur_with_tcp_remotes() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://tcp-proxy:8080".into(),
        "-ul".into(),
        ":1081".into(),
        "-ur".into(),
        "socks5://udp-proxy:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    // Two upstream groups: TCP and UDP
    let groups = parsed["upstream_groups"].as_array().unwrap();
    assert!(groups
        .iter()
        .any(|g| g["id"].as_str() == Some("pproxy-chain")));
    assert!(groups
        .iter()
        .any(|g| g["id"].as_str() == Some("pproxy-udp-chain")));
    // Two rules: default and UDP
    let rules = parsed["rules"].as_array().unwrap();
    assert!(rules
        .iter()
        .any(|r| r["id"].as_str() == Some("pproxy-default")));
    assert!(rules
        .iter()
        .any(|r| r["id"].as_str() == Some("pproxy-udp-default")));
}

#[test]
fn test_ul_without_listen_adds_default_socks5() {
    let args = PproxyArgs::parse(&["-ul".into(), ":1081".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let listeners = parsed["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 1);
    assert!(listeners[0]["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p.as_str() == Some("socks5")));
    let udp = &listeners[0]["udp"];
    assert_eq!(udp["mode"].as_str(), Some("standalone_pproxy_udp"));
    assert!(output
        .warnings
        .iter()
        .any(|w| w.category == "ul-no-listener"));
}

#[test]
fn test_ul_address_format_colon_port() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-ul".into(),
        ":1081".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("0.0.0.0:1081"));
}

#[test]
fn test_ul_address_format_uri() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-ul".into(),
        "socks5://:1081".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("0.0.0.0:1081"));
}

#[test]
fn test_ul_address_format_plain_port() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-ul".into(),
        "1081".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("0.0.0.0:1081"));
}
