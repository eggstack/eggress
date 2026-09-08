//! Translation regression tests (moved verbatim with the module split).

use super::*;
use crate::args::PproxyArgs;

#[test]
fn test_translate_socks5_direct() {
    let args = PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("socks5"));
    assert!(output.toml.contains("127.0.0.1:1080"));
    assert!(!output.has_unsupported());
    assert!(output.toml.contains("pproxy-default"));
    assert!(output.toml.contains("direct = true"));
    eprintln!("{}", output.toml);
}

#[test]
fn test_translate_http_direct() {
    let args = PproxyArgs::parse(&["-l".into(), "http://0.0.0.0:8080".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("http"));
    assert!(output.toml.contains("0.0.0.0:8080"));
}

#[test]
fn test_translate_socks5_through_http_upstream() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy:8080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("pproxy-upstream-0"));
    assert!(output.toml.contains("pproxy-chain"));
    assert!(output.toml.contains("http://proxy:8080"));
}

#[test]
fn test_translate_explicit_tls_upstream_uses_scheme_suffix() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5+tls://proxy:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("socks5+tls://proxy:1080"));
    assert!(!output.toml.contains("proxy:1080+tls"));
}

#[test]
fn test_translate_ipv6_upstream_brackets_host() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5://[::1]:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("socks5://[::1]:1080"));
}

#[test]
fn test_translate_trojan_password_only_upstream() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "trojan://secret@proxy:443".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("trojan://secret@proxy:443"));
    assert!(!output.toml.contains("trojan://:secret@proxy:443"));
}

#[test]
fn test_translate_chain() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy1:8080".into(),
        "-r".into(),
        "socks5://proxy2:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("pproxy-upstream-0"));
    assert!(output.toml.contains("pproxy-upstream-1"));
    assert!(output.toml.contains("first-available"));
}

#[test]
fn test_translate_auth_credentials_redacted() {
    let args =
        PproxyArgs::parse(&["-l".into(), "socks5://user:secret@127.0.0.1:1080".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    // Auth should be present
    assert!(output.toml.contains("password"));
    // Warning about plaintext creds
    assert!(output
        .warnings()
        .iter()
        .any(|w| w.category == "credential-in-toml"));
}

#[cfg(feature = "ssh")]
#[test]
fn test_translate_ssh_fragment_credentials_to_native_uri() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "ssh://host/#login:password".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("ssh://login:password@host:22"));
    assert!(!output.toml.contains("#login:password"));
    assert!(!output
        .unsupported()
        .iter()
        .any(|unsupported| unsupported.feature == "ssh-upstream"));
}

#[test]
fn test_translate_shadowsocks_listener_supported() {
    let args =
        PproxyArgs::parse(&["-l".into(), "ss://aes-256-gcm:secret@proxy:8388".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(
        !output
            .unsupported()
            .iter()
            .any(|u| u.feature == "shadowsocks-listener"),
        "shadowsocks listener should be supported"
    );
}

#[test]
fn test_translate_daemon_feature_state() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--daemon".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    #[cfg(feature = "daemon")]
    assert!(!output.has_unsupported());
    #[cfg(not(feature = "daemon"))]
    assert!(output.has_unsupported());
}

#[test]
fn test_no_local_listener_error() {
    let args = PproxyArgs::parse(&[]).unwrap();
    let result = translate_pproxy_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_valid_toml_roundtrip() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy:8080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    // Should be valid TOML
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    assert_eq!(parsed["version"].as_integer(), Some(1));
    let listeners = parsed["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 1);
    let upstreams = parsed["upstreams"].as_array().unwrap();
    assert_eq!(upstreams.len(), 1);
}

#[test]
fn test_verbose_flag_emits_warning() {
    let args =
        PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-v".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output
        .warnings()
        .iter()
        .any(|w| w.category == "verbose-mode"));
}

#[test]
fn test_debug_flag_emits_compatible_warning() {
    let args =
        PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-d".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.warnings().iter().any(|w| w.category == "debug-mode"));
    assert_eq!(
        crate::classify_aggregate_tier(&output.warnings(), &[]),
        crate::ManifestTier::CompatibleWithWarning
    );
}

#[test]
fn test_scheduler_flag_maps_to_toml() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy:8080".into(),
        "-s".into(),
        "rr".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("round-robin"));
}

#[test]
fn test_scheduler_flag_all_values() {
    for (input, expected) in &[
        ("fa", "first-available"),
        ("first_available", "first-available"),
        ("rr", "round-robin"),
        ("round_robin", "round-robin"),
        ("rc", "random"),
        ("random_choice", "random"),
        ("lc", "least-connections"),
        ("least_connection", "least-connections"),
    ] {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy:8080".into(),
            "-s".into(),
            input.to_string(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(
            output.toml.contains(expected),
            "expected '{}' for scheduler input '{}', got:\n{}",
            expected,
            input,
            output.toml
        );
    }
}

#[test]
fn test_alive_flag_emits_warning() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-a".into(),
        "10".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output
        .warnings()
        .iter()
        .any(|w| w.category == "alive-check"));
}

#[test]
fn test_ssl_flag_generates_tls_config() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--ssl".into(),
        "cert.pem,key.pem".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("cert.pem"));
    assert!(output.toml.contains("key.pem"));
    assert!(!output
        .unsupported()
        .iter()
        .any(|u| u.feature == "ssl-listener"));
}

#[test]
fn test_ssl_cert_only_generates_tls_config() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--ssl".into(),
        "cert.pem".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("cert.pem"));
    assert!(!output.has_unsupported());
}

#[test]
fn test_ssl_flag_applies_to_all_listeners() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-l".into(),
        "http://127.0.0.1:8080".into(),
        "--ssl".into(),
        "cert.pem,key.pem".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    let listener_count = output.toml.matches("[[listeners]]").count();
    assert_eq!(
        listener_count, 2,
        "expected 2 listeners, got: {}",
        output.toml
    );
    let tls_block_count = output.toml.matches("[listeners.tls]").count();
    assert_eq!(
        tls_block_count, 2,
        "expected 2 [listeners.tls] blocks (one per listener), got: {}",
        output.toml
    );
}

#[test]
fn test_block_flag_generates_reject_rule() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-b".into(),
        "{.*\\.example\\.com}".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("pproxy-block-0"));
    assert!(output.toml.contains("reject"));
    assert!(output.toml.contains(".*\\.example\\.com"));
    assert!(!output.has_unsupported());
}

#[test]
fn test_block_flag_toml_roundtrip() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-b".into(),
        "{.*\\.blocked\\.com}".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let rules = parsed["rules"].as_array().unwrap();
    let block_rule = rules
        .iter()
        .find(|r| {
            r["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("pproxy-block-0-pattern="))
        })
        .unwrap();
    assert_eq!(
        block_rule["host_regex"].as_str(),
        Some("^(?:.*\\.blocked\\.com)")
    );
    assert_eq!(block_rule["reject"].as_str(), Some("blocked"));
}

#[test]
fn test_rulefile_missing_file_fails_translation() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--rulefile".into(),
        "/nonexistent/rules.txt".into(),
    ])
    .unwrap();
    let error = translate_pproxy_args(&args).unwrap_err();
    assert!(error
        .to_string()
        .contains("failed to load pproxy rule file"));
}

#[test]
fn test_rulefile_generates_block_rules() {
    use std::io::Write;
    let dir = std::env::temp_dir().join("eggress_test_rulefile");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rules.txt");
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(
        f,
        "# comment\n.*\\.blocked\\.com -> reject\nother\\.com -> http://proxy:8080"
    )
    .unwrap();

    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--rulefile".into(),
        path.to_str().unwrap().into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("pproxy-block-0"));
    assert!(output.toml.contains(".*\\.blocked\\.com"));
    // Complex rule should emit a warning
    assert!(output
        .warnings()
        .iter()
        .any(|w| w.category == "rulefile-partial"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_translate_ssr_listener_supported() {
    let args =
        PproxyArgs::parse(&["-l".into(), "ssr://aes-256-ctr:secret@proxy:8388".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    assert!(output.toml.contains("protocols = [\"ssr\"]"));
}

#[test]
fn test_translate_ssr_upstream_supported() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "ssr://aes-256-ctr:secret@proxy:8388".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    assert!(output.toml.contains("ssr://"));
}

#[test]
fn test_translate_legacy_cipher_feature_state() {
    let args =
        PproxyArgs::parse(&["-l".into(), "ss://aes-128-ctr:secret@proxy:8388".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    #[cfg(feature = "legacy-crypto")]
    assert!(!output.has_unsupported());
    #[cfg(not(feature = "legacy-crypto"))]
    assert!(output.has_unsupported());
    #[cfg(not(feature = "legacy-crypto"))]
    assert!(output
        .unsupported()
        .iter()
        .any(|u| u.feature == "legacy-cipher"));
}

#[test]
fn test_unknown_flags_emitted_as_warnings() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--totally-unknown".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output
        .warnings()
        .iter()
        .any(|w| w.category == "unknown-flag" && w.message.contains("--totally-unknown")));
}

#[test]
fn test_scheduler_default_first_available() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy:8080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("first-available"));
}

#[test]
fn test_scheduler_default_first_available_for_multiple_remotes() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy1:8080".into(),
        "-r".into(),
        "socks5://proxy2:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("first-available"));
}

#[test]
fn test_translate_ul_generates_standalone_udp() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-ul".into(),
        ":1081".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    assert!(output.toml.contains("standalone_pproxy_udp"));
    assert!(output.toml.contains("0.0.0.0:1081"));
}

#[test]
fn test_translate_ur_generates_upstream() {
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
    assert!(!output.has_unsupported());
    assert!(output.toml.contains("pproxy-udp-upstream-0"));
    assert!(output.toml.contains("pproxy-udp-chain"));
    assert!(output.toml.contains("socks5://proxy:1080"));
    assert!(output.toml.contains("transport = \"udp\""));
}

#[test]
fn test_translate_ul_and_ur_together() {
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
    assert!(!output.has_unsupported());
    // TCP upstream group
    assert!(output.toml.contains("pproxy-upstream-0"));
    assert!(output.toml.contains("pproxy-chain"));
    // UDP upstream group
    assert!(output.toml.contains("pproxy-udp-upstream-0"));
    assert!(output.toml.contains("pproxy-udp-chain"));
    // UDP listener config
    assert!(output.toml.contains("standalone_pproxy_udp"));
    // Two rules: default (any) and UDP
    assert!(output.toml.contains("pproxy-default"));
    assert!(output.toml.contains("pproxy-udp-default"));
}

#[test]
fn test_ul_without_listen_adds_default_socks5() {
    let args = PproxyArgs::parse(&["-ul".into(), ":1081".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    // Should have added a default SOCKS5 listener
    assert!(output.toml.contains("pproxy-local-0"));
    assert!(output.toml.contains("socks5"));
    assert!(output.toml.contains("standalone_pproxy_udp"));
    assert!(output
        .warnings()
        .iter()
        .any(|w| w.category == "ul-no-listener"));
}

#[test]
fn test_ul_address_formats() {
    // Test various -ul address formats
    for (input, expected_bind) in &[
        (":1081", "0.0.0.0:1081"),
        ("0.0.0.0:1081", "0.0.0.0:1081"),
        ("127.0.0.1:1081", "127.0.0.1:1081"),
        ("1081", "0.0.0.0:1081"),
        ("socks5://:1081", "0.0.0.0:1081"),
        ("socks5://[::1]:1081", "[::1]:1081"),
        ("socks5://user:pass@[::1]:1081?ignored=true", "[::1]:1081"),
    ] {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-ul".into(),
            input.to_string(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(
            output.toml.contains(expected_bind),
            "expected bind '{}' for -ul input '{}', got:\n{}",
            expected_bind,
            input,
            output.toml
        );
    }
}

#[test]
fn test_ul_no_tcp_direct_warning_when_ur_present() {
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
    // No direct-mode warning when UDP upstream is specified
    assert!(!output
        .warnings()
        .iter()
        .any(|w| w.category == "direct-mode"));
}

#[test]
fn test_valid_toml_roundtrip_with_udp() {
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
    assert_eq!(parsed["version"].as_integer(), Some(1));
    let listeners = parsed["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 1);
    let udp = &listeners[0]["udp"];
    assert_eq!(udp["mode"].as_str(), Some("standalone_pproxy_udp"));
    assert_eq!(udp["bind"].as_str(), Some("0.0.0.0:1081"));
    let upstreams = parsed["upstreams"].as_array().unwrap();
    assert_eq!(upstreams.len(), 1);
    let groups = parsed["upstream_groups"].as_array().unwrap();
    assert!(groups
        .iter()
        .any(|g| g["id"].as_str() == Some("pproxy-udp-chain")));
    let rules = parsed["rules"].as_array().unwrap();
    assert!(rules
        .iter()
        .any(|r| r["id"].as_str() == Some("pproxy-udp-default")));
}

#[test]
fn test_translate_socks5_backward_emits_reverse_client() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5+in://user:pass@acceptor:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let clients = parsed["reverse_clients"].as_array().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0]["server_addr"].as_str(), Some("acceptor:1080"));
    assert_eq!(clients[0]["auth_username"].as_str(), Some("user"));
    assert_eq!(clients[0]["auth_password"].as_str(), Some("pass"));
    // Should NOT appear in regular upstreams
    assert!(
        parsed.get("upstreams").is_none()
            || parsed["upstreams"].as_array().is_none_or(|a| a.is_empty())
    );
}

#[test]
fn test_translate_bind_listener_emits_reverse_server() {
    let args = PproxyArgs::parse(&["-l".into(), "bind://0.0.0.0:8080".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let servers = parsed["reverse_servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["control_bind"].as_str(), Some("0.0.0.0:8080"));
    assert_eq!(servers[0]["external_bind"].as_str(), Some("0.0.0.0:8080"));
    // Should NOT appear in regular listeners
    let listeners = parsed["listeners"].as_array().unwrap();
    assert!(listeners.is_empty());
}

#[test]
fn test_translate_backward_listener_emits_reverse_server() {
    let args = PproxyArgs::parse(&["-l".into(), "backward://0.0.0.0:8080".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let servers = parsed["reverse_servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["control_bind"].as_str(), Some("0.0.0.0:8080"));
    assert_eq!(servers[0]["external_bind"].as_str(), Some("0.0.0.0:8080"));
}

#[test]
fn test_translate_backward_with_parallel_connections() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5+in+in://acceptor:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let clients = parsed["reverse_clients"].as_array().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0]["parallel_connections"].as_integer(), Some(2));
}

#[test]
fn test_translate_backward_with_jump_chain() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5+in://a:1__http://b:2".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let clients = parsed["reverse_clients"].as_array().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(
        clients[0]["server_uri"].as_str(),
        Some("socks5://a:1__http://b:2")
    );
    assert_eq!(clients[0]["pproxy_compat"].as_bool(), Some(true));
}

#[test]
fn test_translate_backward_tls_unsupported() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5+in+ssl://acceptor:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.has_unsupported());
    assert!(
        output
            .unsupported()
            .iter()
            .any(|u| u.feature == "backward-tls"),
        "expected backward-tls unsupported, got: {:?}",
        output.unsupported()
    );
}

#[test]
fn test_translate_reverse_server_with_auth() {
    let args = PproxyArgs::parse(&["-l".into(), "bind://user:pass@0.0.0.0:8080".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let servers = parsed["reverse_servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["auth_username"].as_str(), Some("user"));
    assert_eq!(servers[0]["auth_password"].as_str(), Some("pass"));
    // Credential warning emitted
    assert!(output
        .warnings()
        .iter()
        .any(|w| w.category == "credential-in-toml"));
}

#[test]
fn test_translate_backward_no_parallel_when_single() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5+in://acceptor:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let clients = parsed["reverse_clients"].as_array().unwrap();
    assert_eq!(clients.len(), 1);
    // parallel_connections should not be present for single +in
    assert!(clients[0].get("parallel_connections").is_none());
}

#[test]
fn test_translate_backward_toml_parses() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5+in+in://user:pass@acceptor:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    // Verify TOML is valid
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    assert_eq!(parsed["version"].as_integer(), Some(1));
    // Verify structure matches eggress ConfigFile expectations
    let clients = parsed["reverse_clients"].as_array().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0]["id"].as_str(), Some("pproxy-reverse-client-0"));
    assert_eq!(clients[0]["server_addr"].as_str(), Some("acceptor:1080"));
    assert_eq!(clients[0]["auth_username"].as_str(), Some("user"));
    assert_eq!(clients[0]["auth_password"].as_str(), Some("pass"));
    assert_eq!(clients[0]["parallel_connections"].as_integer(), Some(2));
}

#[test]
fn test_pac_flag_emits_warning() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--pac".into(),
        "/proxy.pac".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output
        .warnings()
        .iter()
        .any(|w| w.category == "pac-serving"));
}

#[test]
fn test_test_flag_emits_warning() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--test".into(),
        "http://example.com".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.warnings().iter().any(|w| w.category == "test-mode"));
}

#[test]
fn test_sys_flag_emits_warning() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--sys".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    assert!(output
        .warnings()
        .iter()
        .any(|w| w.category == "system-proxy"));
}

#[test]
fn test_log_flag_emits_warning() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--log".into(),
        "access.log".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.warnings().iter().any(|w| w.category == "log-file"));
}

#[test]
fn test_reuse_flag_sets_reuse_port() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--reuse".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("reuse_port = true"));
}

#[test]
fn test_alive_flag_includes_interval_in_message() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-a".into(),
        "15".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let alive_warnings = output.warnings();
    let alive_warn = alive_warnings
        .iter()
        .find(|w| w.category == "alive-check")
        .unwrap();
    assert!(alive_warn.message.contains("15"));
}

#[test]
fn test_get_flag_emits_warning() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--get".into(),
        "/index.html,body.txt".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output
        .warnings()
        .iter()
        .any(|w| w.category == "get-static-content"));
}

#[test]
fn test_translate_two_hop_chain_one_upstream() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5://a:1080__http://b:80".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let upstreams = parsed["upstreams"].as_array().unwrap();
    assert_eq!(upstreams.len(), 1);
    // Chain URI should contain __ separator
    let uri = upstreams[0]["uri"].as_str().unwrap();
    assert!(uri.contains("__"), "expected __ in chain URI, got: {}", uri);
    // Verify it parses as a valid eggress chain
    assert!(uri.starts_with("socks5://"));
    assert!(uri.ends_with("http://b:80"));
    // Group should be first-available (single upstream)
    let groups = parsed["upstream_groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["scheduler"].as_str(), Some("first-available"));
}

#[test]
fn test_translate_three_hop_chain() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "http://127.0.0.1:8080".into(),
        "-r".into(),
        "socks5://a:1080__http://b:80__socks5://c:1080".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let upstreams = parsed["upstreams"].as_array().unwrap();
    assert_eq!(upstreams.len(), 1);
    let uri = upstreams[0]["uri"].as_str().unwrap();
    let hop_count = uri.split("__").count();
    assert_eq!(hop_count, 3, "expected 3 hops in chain URI: {}", uri);
}

#[test]
fn test_translate_two_r_flags_two_upstreams() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5://a:1080".into(),
        "-r".into(),
        "http://b:80".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let upstreams = parsed["upstreams"].as_array().unwrap();
    assert_eq!(upstreams.len(), 2);
    // Two separate upstreams, not a chain
    assert!(!upstreams[0]["uri"].as_str().unwrap().contains("__"));
    assert!(!upstreams[1]["uri"].as_str().unwrap().contains("__"));
    // Group should preserve pproxy's first-available declaration order.
    let groups = parsed["upstream_groups"].as_array().unwrap();
    assert_eq!(groups[0]["scheduler"].as_str(), Some("first-available"));
}

#[test]
fn test_per_remote_rules_preserve_order_and_direct_fallback() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://a:80?rule=alpha".into(),
        "-r".into(),
        "socks5://b:1080?rule=beta".into(),
        "-r".into(),
        "http://c:80".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let groups = parsed["upstream_groups"].as_array().unwrap();
    assert_eq!(groups[0]["id"].as_str(), Some("pproxy-route-0"));
    assert_eq!(groups[1]["id"].as_str(), Some("pproxy-route-1"));
    assert_eq!(groups[2]["id"].as_str(), Some("pproxy-chain"));
    assert_eq!(groups[2]["members"][0].as_str(), Some("pproxy-upstream-2"));

    let rules = parsed["rules"].as_array().unwrap();
    assert!(rules[0]["id"]
        .as_str()
        .unwrap()
        .starts_with("pproxy-route-0-inline:0-pattern="));
    assert!(rules[1]["id"]
        .as_str()
        .unwrap()
        .starts_with("pproxy-route-1-inline:1-pattern="));
    assert_eq!(rules[2]["id"].as_str(), Some("pproxy-default"));
    assert!(rules[0]["match"]["any_of"].as_array().unwrap().len() == 2);

    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), &output.toml).unwrap();
    eggress_config::load_and_validate(file.path().to_str().unwrap()).unwrap();
}

#[test]
fn test_explicit_round_robin_only_changes_unruled_group() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://a:80".into(),
        "-r".into(),
        "http://b:80".into(),
        "-s".into(),
        "rr".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("scheduler = \"round-robin\""));
}

#[test]
fn test_translate_chain_with_creds_preserved() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5://user:pass@a:1080__http://b:80".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let upstreams = parsed["upstreams"].as_array().unwrap();
    let uri = upstreams[0]["uri"].as_str().unwrap();
    // Credentials should be preserved in the config URI
    assert!(
        uri.contains("user:pass@"),
        "expected credentials in URI, got: {}",
        uri
    );
}

#[test]
fn test_translate_chain_with_tls_hop() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5+tls://a:1080__http://b:80".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    let upstreams = parsed["upstreams"].as_array().unwrap();
    let uri = upstreams[0]["uri"].as_str().unwrap();
    assert!(
        uri.starts_with("socks5+tls://"),
        "expected TLS modifier in first hop, got: {}",
        uri
    );
}

#[test]
fn test_translate_chain_ssh_hop_unsupported() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5://a:1080__ssh://b:22".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    #[cfg(feature = "ssh")]
    assert!(!output.has_unsupported());
    #[cfg(not(feature = "ssh"))]
    {
        assert!(output.has_unsupported());
        assert!(output
            .unsupported()
            .iter()
            .any(|u| u.feature == "ssh-upstream" || u.feature == "chain-unsupported-hop"));
    }
}

#[test]
fn test_translate_chain_ssr_hop_supported() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5://a:1080__ssr://b:8388".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(!output.has_unsupported());
    assert!(output.toml.contains("__ssr://"));
}

#[test]
fn test_translate_chain_valid_toml_roundtrip() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "socks5://a:1080__http://b:80".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    assert_eq!(parsed["version"].as_integer(), Some(1));
    let listeners = parsed["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 1);
    let upstreams = parsed["upstreams"].as_array().unwrap();
    assert_eq!(upstreams.len(), 1);
    let rules = parsed["rules"].as_array().unwrap();
    assert!(rules
        .iter()
        .any(|r| r["id"].as_str() == Some("pproxy-default")));
}

#[test]
fn test_alive_flag_generates_health_config() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "http://proxy:8080".into(),
        "-a".into(),
        "10".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("[upstreams.health]"));
    assert!(output.toml.contains("interval = \"10s\""));
}

#[test]
fn test_pac_flag_generates_admin_pac_config() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "--pac".into(),
        "/proxy.pac".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("[admin.pac]"));
}

#[test]
fn test_translate_trojan_listener_supported() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "trojan://my-secret@0.0.0.0:443".into(),
        "--ssl".into(),
        "cert.pem,key.pem".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(
        !output
            .unsupported()
            .iter()
            .any(|u| u.feature == "trojan-listener"),
        "trojan listener should be supported now"
    );
    assert!(output.toml.contains("[listeners.trojan]"));
    assert!(output.toml.contains("password = \"my-secret\""));
    assert!(output.toml.contains("[listeners.tls]"));
}

#[test]
fn test_translate_trojan_listener_toml_roundtrip() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "trojan://pass123@0.0.0.0:443".into(),
        "--ssl".into(),
        "cert.pem,key.pem".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
    assert_eq!(parsed["version"].as_integer(), Some(1));
    let listeners = parsed["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 1);
    assert_eq!(
        listeners[0]["protocols"].as_array().unwrap()[0].as_str(),
        Some("trojan")
    );
    assert_eq!(listeners[0]["trojan"]["password"].as_str(), Some("pass123"));
    assert!(
        listeners[0]["tls"].is_table(),
        "configured TLS should be present for trojan"
    );
}

#[test]
fn test_translate_trojan_listener_without_tls_is_unsupported() {
    let args = PproxyArgs::parse(&["-l".into(), "trojan://my-secret@0.0.0.0:443".into()]).unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(output.toml.contains("[listeners.trojan]"));
    assert!(output.toml.contains("password = \"my-secret\""));
    assert!(output
        .unsupported()
        .iter()
        .any(|u| u.feature == "trojan-tls-config"));
    assert!(!output.toml.contains("/path/to/cert.pem"));
}

#[test]
fn test_translate_trojan_upstream_still_works() {
    let args = PproxyArgs::parse(&[
        "-l".into(),
        "socks5://127.0.0.1:1080".into(),
        "-r".into(),
        "trojan://secret@proxy.example:443".into(),
    ])
    .unwrap();
    let output = translate_pproxy_args(&args).unwrap();
    assert!(
        !output.has_unsupported(),
        "trojan upstream should remain supported: {:?}",
        output.unsupported()
    );
    assert!(output.toml.contains("trojan://secret@proxy.example:443"));
}
