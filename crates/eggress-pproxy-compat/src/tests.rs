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
fn test_translate_advanced_upstream_protocols_to_native_uris() {
    for (scheme, expected) in [
        ("h2", "h2+tls://proxy:443"),
        ("ws", "ws://proxy:80"),
        ("wss", "ws+tls://proxy:443"),
        ("raw", "raw://target:9000"),
        ("tunnel", "tunnel://target:9000"),
    ] {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            format!(
                "{}://{}",
                scheme,
                if scheme == "h2" || scheme == "wss" {
                    "proxy:443"
                } else if scheme == "ws" {
                    "proxy:80"
                } else {
                    "target:9000"
                }
            ),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(
            !output.has_unsupported(),
            "{}: {:?}",
            scheme,
            output.unsupported
        );
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        assert_eq!(parsed["upstreams"][0]["uri"].as_str(), Some(expected));

        let config: eggress_config::model::ConfigFile = toml::from_str(&output.toml).unwrap();
        let validation = eggress_config::validate::validate_config(&config);
        assert!(
            validation.is_ok(),
            "{} config rejected: {:?}",
            scheme,
            validation
        );
        eggress_config::compile::compile_config(&config).unwrap();
    }
}

#[cfg(feature = "quic")]
#[test]
fn test_translate_quic_compatibility_marks_insecure_explicitly() {
    for (scheme, expected) in [
        ("h3", "h3://proxy:443?insecure=true"),
        ("quic+http", "quic+http://proxy:443?insecure=true"),
    ] {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            format!("{scheme}://proxy:443"),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        assert!(parsed.get("upstreams").is_some(), "{}", output.toml);
        assert_eq!(parsed["upstreams"][0]["uri"].as_str(), Some(expected));
        assert!(output
            .warnings
            .iter()
            .any(|warning| warning.category == "quic-insecure"));
    }
}

#[test]
fn test_raw_fixed_target_is_lowered_to_native_endpoint() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "raw://{127.0.0.1:9000}".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    assert!(output.toml.contains("uri = \"raw://127.0.0.1:9000\""));
}

#[test]
fn test_canonical_fixed_target_and_echo_grammar() {
    let cases = [
        ("tunnel{127.0.0.1:9000}://:1080", "tunnel", "127.0.0.1:9000"),
        ("tunnel{[::1]:9000}://:1080", "tunnel", "[::1]:9000"),
    ];
    for (raw, protocol, target) in cases {
        let uri = crate::uri::parse_pproxy_uri(raw).unwrap();
        assert_eq!(uri.protocol_chain, vec![protocol]);
        assert_eq!(uri.fixed_target.as_deref(), Some(target));
        assert_eq!(uri.host, "");
        assert_eq!(uri.port, 1080);
    }
    let composed = crate::uri::parse_pproxy_uri(
        "trojan+tunnel{127.0.0.1:9000}+ssl://password@proxy.example:443",
    )
    .unwrap();
    assert_eq!(composed.protocol_chain, vec!["trojan", "tunnel"]);
    assert!(composed.ssl && composed.tls);
    assert_eq!(composed.fixed_target.as_deref(), Some("127.0.0.1:9000"));
    assert!(crate::uri::parse_pproxy_uri("echo://127.0.0.1:0").is_ok());
    for malformed in [
        "tunnel{}://:1080",
        "tunnel{127.0.0.1:9000://:1080",
        "tunnel{a}:tunnel{b}://:1080",
        "http{127.0.0.1:9000}://proxy:8080",
    ] {
        assert!(
            crate::uri::parse_pproxy_uri(malformed).is_err(),
            "{malformed}"
        );
    }
}

#[test]
fn test_canonical_fixed_target_listener_keeps_tcp_target() {
    let args = PproxyArgs::parse(&["-l".into(), "tunnel{127.0.0.1:9000}://:1080".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    assert_eq!(
        parsed["listeners"][0]["fixed_target"].as_str(),
        Some("127.0.0.1:9000")
    );
    assert!(parsed["listeners"][0].get("udp").is_none());
    let config: eggress_config::model::ConfigFile = toml::from_str(&output.toml).unwrap();
    let compiled = eggress_config::compile::compile_config(&config).unwrap();
    assert_eq!(
        compiled.listeners[0].fixed_target.as_ref().unwrap().port,
        9000
    );
    assert!(compiled.listeners[0].udp.is_none());
}

#[test]
fn test_fixed_target_udp_is_explicit_and_separate() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "tunnel{127.0.0.1:9000}://:1080".into(),
        "-ul".into(),
        "tunnel{127.0.0.1:9000}://:1081".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    assert_eq!(
        parsed["listeners"][0]["fixed_target"].as_str(),
        Some("127.0.0.1:9000")
    );
    assert_eq!(
        parsed["listeners"][0]["udp"]["fixed_target"].as_str(),
        Some("127.0.0.1:9000")
    );
}

#[test]
fn test_local_bind_reaches_native_chain_and_httponly_listener_is_rejected() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://user:pass@proxy:8080?rule=example/@127.0.0.1".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported(), "{:?}", output.unsupported);
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let uri = parsed["upstreams"][0]["uri"].as_str().unwrap();
    let chain = eggress_uri::parse_proxy_chain(uri).unwrap();
    assert_eq!(chain.hops[0].local_bind.as_deref(), Some("127.0.0.1"));

    let args = PproxyArgs::parse(&["-l".into(), "httponly://:1080".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output
        .unsupported
        .iter()
        .any(|item| item.feature == "unsupported-role"
            && item.detail.contains("upstream request adapter")));
}

#[test]
fn test_advanced_transport_listener_roles_are_translated() {
    let h2 = PproxyArgs::parse(&["-l".into(), "h2://:1080".into()]).unwrap();
    let h2_output = translate_pproxy_args(&h2).unwrap();
    assert!(!h2_output.has_unsupported(), "{:?}", h2_output.unsupported);
    assert!(h2_output.toml.contains("protocols = [\"h2\"]"));

    let ws = PproxyArgs::parse(&["-l".into(), "ws{127.0.0.1:80}://:1080".into()]).unwrap();
    let ws_output = translate_pproxy_args(&ws).unwrap();
    assert!(!ws_output.has_unsupported(), "{:?}", ws_output.unsupported);
    assert!(ws_output.toml.contains("protocols = [\"websocket\"]"));

    let wss = PproxyArgs::parse(&[
        "-l".into(),
        "wss{127.0.0.1:80}://:1080".into(),
        "--ssl".into(),
        "cert.pem,key.pem".into(),
    ])
    .unwrap();
    let wss_output = translate_pproxy_args(&wss).unwrap();
    assert!(!wss_output
        .unsupported
        .iter()
        .any(|u| u.feature == "unsupported-role"));
}

#[test]
fn test_advanced_transport_udp_is_rejected_as_tcp_only() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-ul".into(),
        ":1081".into(),
        "-ur".into(),
        "h2://proxy:443".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output
        .unsupported
        .iter()
        .any(|u| u.feature == "unsupported-role"));
    assert!(!output.toml.contains("pproxy-udp-upstream-0"));
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
fn test_shadowsocks_listener_is_supported() {
    let args =
        PproxyArgs::parse(&["-l".into(), "ss://aes-256-gcm:pass@proxy:8388".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(
        !output
            .unsupported
            .iter()
            .any(|u| u.feature == "shadowsocks-listener"),
        "shadowsocks listener should no longer be unsupported"
    );
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
    assert_eq!(uri, "http+tls://proxy:443");
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
    #[cfg(feature = "ssh")]
    assert!(!output.has_unsupported());
    #[cfg(not(feature = "ssh"))]
    {
        assert!(output.has_unsupported());
        assert!(output
            .unsupported
            .iter()
            .any(|u| u.feature == "ssh-upstream"));
    }
}

#[test]
fn test_unix_upstream_unsupported() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "unix:///tmp/eggress-phase5.sock".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(
        !output.has_unsupported(),
        "unexpected diagnostics: {:?}",
        output.unsupported
    );
    assert!(output.toml.contains("unix:///tmp/eggress-phase5.sock"));
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

#[test]
fn test_redir_colon_port_listener_translation() {
    let args = PproxyArgs::parse(&["-l".into(), "redir://:12345".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let listeners = parsed["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 1);
    let listener = &listeners[0];
    assert_eq!(listener["bind"].as_str(), Some("0.0.0.0:12345"));
    assert!(listener["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p.as_str() == Some("http")));
    let transparent = &listener["transparent"];
    assert_eq!(transparent["enabled"].as_bool(), Some(true));
    assert_eq!(transparent["protocol"].as_str(), Some("redir"));
}

#[test]
fn test_redir_host_port_listener_translation() {
    let args = PproxyArgs::parse(&["-l".into(), "redir://127.0.0.1:12345".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let listeners = parsed["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 1);
    let listener = &listeners[0];
    assert_eq!(listener["bind"].as_str(), Some("127.0.0.1:12345"));
    let transparent = &listener["transparent"];
    assert_eq!(transparent["enabled"].as_bool(), Some(true));
    assert_eq!(transparent["protocol"].as_str(), Some("redir"));
}

#[test]
fn test_unix_socket_listener_translation() {
    let args = PproxyArgs::parse(&["-l".into(), "unix:///tmp/test.sock".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let listeners = parsed["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 1);
    let listener = &listeners[0];
    assert!(listener["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p.as_str() == Some("socks5")));
    let unix = &listener["unix"];
    assert_eq!(unix["path"].as_str(), Some("/tmp/test.sock"));
    assert_eq!(unix["unlink_existing"].as_bool(), Some(false));
}

#[test]
fn test_redir_upstream_still_unsupported() {
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
fn test_unix_upstream_supported() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "unix:///tmp/eggress-phase5.sock".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
}

#[test]
fn test_unix_socket_path_redacted_in_display() {
    let args = PproxyArgs::parse(&["-l".into(), "unix:///tmp/secret.sock".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    // The warning should not contain the actual socket path name
    let warnings_str = output.warnings_to_string();
    assert!(
        !warnings_str.contains("secret"),
        "unix socket path leaked in warnings"
    );
}

#[test]
fn test_redir_listener_valid_toml() {
    let args = PproxyArgs::parse(&["-l".into(), "redir://:8080".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    assert_eq!(parsed["version"].as_integer(), Some(1));
}

#[test]
fn test_unix_listener_valid_toml() {
    let args = PproxyArgs::parse(&["-l".into(), "unix:///tmp/test.sock".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    assert_eq!(parsed["version"].as_integer(), Some(1));
}

#[test]
fn test_redir_and_socks5_combined() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "redir://:12345".into(),
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let listeners = parsed["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 2);
    // One should have transparent config, the other should not
    let has_transparent = listeners.iter().any(|l| l.get("transparent").is_some());
    assert!(has_transparent);
    let has_unix = listeners.iter().any(|l| l.get("unix").is_some());
    assert!(!has_unix);
}

// --- PAC/get/test boundary tests (28.9) ---
//
// pproxy --pac, --get, and --test are value-taking options. Their values must
// remain owned by the option rather than becoming positional URIs.

#[test]
fn test_pac_flag_generates_unknown_warning() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy:8080".into(),
        "--pac".into(),
        "/path/to/proxy.pac".into(),
    ])
    .unwrap();
    assert!(
        args.known_unsupported
            .iter()
            .any(|f| f == "pac=/path/to/proxy.pac"),
        "pac should be captured as known_unsupported"
    );
    // Should NOT produce an unknown-flag warning for --pac
    let warnings = args.unknown_flag_diagnostics();
    assert!(
        !warnings.iter().any(|w| w.message.contains("pac")),
        "pac should not produce unknown-flag warning: {:?}",
        warnings
    );
}

#[test]
fn test_get_flag_generates_unknown_warning() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy:8080".into(),
        "--get".into(),
        "http://example.com".into(),
    ])
    .unwrap();
    assert!(
        args.known_unsupported
            .iter()
            .any(|f| f == "get=http://example.com"),
        "get should be captured as known_unsupported"
    );
    // Should NOT produce an unknown-flag warning for --get
    let warnings = args.unknown_flag_diagnostics();
    assert!(
        !warnings.iter().any(|w| w.message.contains("get")),
        "get should not produce unknown-flag warning: {:?}",
        warnings
    );
}

#[test]
fn test_valid_get_static_content_is_supported() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "hello static content").unwrap();
    let get_value = format!("/index.html,{}", file.path().display());
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--get".into(),
        get_value,
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(
        !output.has_unsupported(),
        "unexpected diagnostics: {:?}",
        output.unsupported
    );
    assert!(output
        .warnings
        .iter()
        .any(|w| w.category == "get-static-content"));
    assert!(output.toml.contains("/index.html"));
    assert!(output.toml.contains("hello static content"));
}

#[test]
fn test_malformed_get_static_content_fails_closed() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--get".into(),
        "relative.html,body.txt".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.has_unsupported());
    assert!(output.unsupported.iter().any(|u| u.feature == "get-file"));
}

#[test]
fn test_unreadable_get_static_content_fails_closed() {
    let missing_file = tempfile::NamedTempFile::new().unwrap();
    let missing_path = missing_file.path().to_path_buf();
    drop(missing_file);
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--get".into(),
        format!("/index.html,{}", missing_path.display()),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.has_unsupported());
    assert!(output.unsupported.iter().any(|u| u.feature == "get-file"));
}

#[test]
fn test_test_flag_generates_unknown_warning() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy:8080".into(),
        "--test".into(),
        "http://example.com".into(),
    ])
    .unwrap();
    // --test is now a known flag; should NOT produce unknown-flag warning
    let warnings = args.unknown_flag_diagnostics();
    assert!(
        !warnings.iter().any(|w| w.message.contains("test")),
        "test should not produce unknown-flag warning: {:?}",
        warnings
    );
    // Translation should produce a test-mode warning instead
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.warnings.iter().any(|w| w.category == "test-mode"));
}

#[test]
fn test_pac_get_test_flags_do_not_produce_unsupported_features() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy:8080".into(),
        "--test".into(),
        "http://example.com".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(
        !output.has_unsupported(),
        "--test should not produce unsupported features"
    );
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    assert_eq!(parsed["version"].as_integer(), Some(1));
}

// ---------------------------------------------------------------------------
// Cross-surface aggregate-tier regressions.
//
// These tests exercise the public `translate_pproxy_args()` path rather than
// hand-constructing `CompatWarning` values. They prove the aggregate tier
// ordering is correct for representative pproxy arguments.
// ---------------------------------------------------------------------------

fn aggregate_tier(args: &[&str]) -> crate::ManifestTier {
    let raw: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let parsed = PproxyArgs::parse(&raw).expect("parse");
    let output = translate_pproxy_args(&parsed).expect("translate");
    crate::classify_aggregate_tier(&output.warnings, &output.unsupported)
}

#[test]
fn cross_surface_aggregate_tier_compatible_with_warning_for_direct_mode() {
    // `-l socks5://...` with no `-r` produces a `direct-mode` warning
    // (compatible_with_warning). There is no upstream so the warning is
    // unavoidable for a plain listen-only invocation.
    let raw: Vec<String> = ["-l", "socks5://127.0.0.1:0"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let parsed = PproxyArgs::parse(&raw).expect("parse");
    let output = translate_pproxy_args(&parsed).expect("translate");
    let tier = crate::classify_aggregate_tier(&output.warnings, &output.unsupported);
    assert_eq!(tier, crate::ManifestTier::CompatibleWithWarning);
}

#[test]
fn cross_surface_aggregate_tier_native_equivalent_for_reuse_port() {
    // `--reuse` triggers a `reuse-port` warning (native_equivalent).
    let tier = aggregate_tier(&[
        "-l",
        "socks5://127.0.0.1:0",
        "-r",
        "http://proxy:8080",
        "--reuse",
    ]);
    assert_eq!(tier, crate::ManifestTier::NativeEquivalent);
}

#[test]
fn cross_surface_aggregate_tier_compatible_with_warning_for_log_file() {
    // `--log` triggers a `log-file` warning (compatible_with_warning).
    let tier = aggregate_tier(&["-l", "socks5://127.0.0.1:0", "--log", "/dev/null"]);
    assert_eq!(tier, crate::ManifestTier::CompatibleWithWarning);
}

#[test]
fn cross_surface_aggregate_tier_compatible_dominates_native_equivalent() {
    // Combine `--reuse` (native_equivalent) with `--log` (compatible_with_warning).
    // The aggregate must be compatible_with_warning, not native_equivalent.
    let tier = aggregate_tier(&[
        "-l",
        "socks5://127.0.0.1:0",
        "-r",
        "http://proxy:8080",
        "--reuse",
        "--log",
        "/dev/null",
    ]);
    assert_eq!(tier, crate::ManifestTier::CompatibleWithWarning);
}

#[test]
fn cross_surface_aggregate_tier_intentional_non_parity_for_ssh_listener() {
    // SSH listener is classified as `intentional_non_parity`, NOT generic
    // `unsupported`. The service remains non-runnable (ok == false) because
    // the translation report still contains the unsupported feature record.
    let raw: Vec<String> = ["-l", "ssh://user@host:22"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let parsed = PproxyArgs::parse(&raw).expect("parse");
    let output = translate_pproxy_args(&parsed).expect("translate");
    assert!(output.has_unsupported());
    let tier = crate::classify_aggregate_tier(&output.warnings, &output.unsupported);
    assert_eq!(tier, crate::ManifestTier::IntentionalNonParity);
}

#[test]
fn cross_surface_aggregate_tier_unsupported_for_hard_unsupported_feature() {
    // `--daemon` is a hard unsupported feature (not an intentional exclusion),
    // so the aggregate must be `unsupported` even when other warnings exist.
    let raw: Vec<String> = [
        "-l",
        "socks5://127.0.0.1:0",
        "-r",
        "http://proxy:8080",
        "--daemon",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let parsed = PproxyArgs::parse(&raw).expect("parse");
    let output = translate_pproxy_args(&parsed).expect("translate");
    #[cfg(feature = "daemon")]
    {
        assert!(!output.has_unsupported());
        return;
    }
    #[cfg(not(feature = "daemon"))]
    {
        assert!(output.has_unsupported());
        let tier = crate::classify_aggregate_tier(&output.warnings, &output.unsupported);
        assert_eq!(tier, crate::ManifestTier::Unsupported);
    }
}

#[test]
fn cross_surface_aggregate_tier_drop_in_for_ssr_listener() {
    let raw: Vec<String> = ["-l", "ssr://aes-256-ctr:secret@proxy:8388"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let parsed = PproxyArgs::parse(&raw).expect("parse");
    let output = translate_pproxy_args(&parsed).expect("translate");
    assert!(!output.has_unsupported());
    let tier = crate::classify_aggregate_tier(&output.warnings, &output.unsupported);
    assert_eq!(tier, crate::ManifestTier::CompatibleWithWarning);
}

#[test]
fn cross_surface_aggregate_tier_unsupported_for_ssh_upstream_in_chain() {
    // `-r ssh://...` is always wrapped in a chain. The chain validator
    // reports `chain-unsupported-hop` (hard `unsupported`) AND the per-hop
    // validator reports `ssh-upstream` (`intentional_non_parity`). The
    // aggregate must be `unsupported` because chain composition is the
    // hard failure; the per-diagnostic tier still reflects the underlying
    // intentional exclusion.
    let raw: Vec<String> = ["-l", "socks5://127.0.0.1:0", "-r", "ssh://user@hop:22"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let parsed = PproxyArgs::parse(&raw).expect("parse");
    let output = translate_pproxy_args(&parsed).expect("translate");
    #[cfg(feature = "ssh")]
    {
        assert!(!output.has_unsupported());
        return;
    }
    #[cfg(not(feature = "ssh"))]
    {
        assert!(output.has_unsupported());
        let tier = crate::classify_aggregate_tier(&output.warnings, &output.unsupported);
        assert_eq!(tier, crate::ManifestTier::Unsupported);
    }
}
