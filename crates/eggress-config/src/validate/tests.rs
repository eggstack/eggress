//! Validation regression tests (moved verbatim with the split).

use super::composition::{CompositionMatrixMinimal, EMBEDDED_COMPOSITION_MATRIX};
use super::core::validate_timeouts;
use super::listeners::is_loopback_bind;
use super::upstreams::validate_health_config;
use super::*;

#[test]
fn vendored_matrix_matches_canonical() {
    // `docs/parity/composition_matrix.toml` is the canonical contract;
    // the crate ships a vendored copy for `cargo package` self-containment.
    // From-registry checkouts lack `docs/`, so skip there instead of failing.
    let canonical_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/parity/composition_matrix.toml"
    );
    let Ok(canonical) = std::fs::read_to_string(canonical_path) else {
        eprintln!("skipping canonical-matrix sync check (no workspace docs/)");
        return;
    };
    assert_eq!(
        EMBEDDED_COMPOSITION_MATRIX, canonical,
        "crates/eggress-config/composition_matrix.toml is stale; copy docs/parity/composition_matrix.toml over it"
    );
    // The vendored copy must also parse as the expected shape.
    toml::from_str::<CompositionMatrixMinimal>(EMBEDDED_COMPOSITION_MATRIX)
        .expect("vendored composition matrix must parse");
}

#[test]
fn zero_durations_rejected_for_timeouts() {
    let timeouts = crate::model::TimeoutConfig {
        handshake: Some("0s".to_string()),
        connect: Some("0ms".to_string()),
    };
    let mut errors = Vec::new();
    validate_timeouts(&timeouts, &mut errors);
    assert_eq!(errors.len(), 2, "zero handshake and connect must both fail");
    for error in &errors {
        let ConfigError::Validation { message, .. } = error else {
            panic!("expected validation error, got {error:?}");
        };
        assert!(message.contains("greater than 0"), "unexpected: {message}");
    }
}

#[test]
fn nonzero_and_missing_timeouts_accepted() {
    let timeouts = crate::model::TimeoutConfig {
        handshake: Some("5s".to_string()),
        connect: None,
    };
    let mut errors = Vec::new();
    validate_timeouts(&timeouts, &mut errors);
    assert!(errors.is_empty());
}

#[test]
fn zero_health_durations_rejected() {
    let health = crate::model::HealthConfigToml {
        mode: None,
        interval: Some("0s".to_string()),
        timeout: Some("0s".to_string()),
        failures_to_unhealthy: None,
        successes_to_healthy: None,
        initial_state: None,
    };
    let mut errors = Vec::new();
    validate_health_config(&health, "upstreams[0]", &mut errors);
    assert_eq!(errors.len(), 2, "zero interval and timeout must both fail");
    for error in &errors {
        let ConfigError::Validation { message, .. } = error else {
            panic!("expected validation error, got {error:?}");
        };
        assert!(message.contains("greater than 0"), "unexpected: {message}");
    }
}

#[test]
fn loopback_detection() {
    assert!(is_loopback_bind("127.0.0.1:8080"));
    assert!(is_loopback_bind("127.0.0.1:0"));
    assert!(is_loopback_bind("[::1]:8080"));
    assert!(is_loopback_bind("[::ffff:127.0.0.1]:8080"));
    assert!(!is_loopback_bind("0.0.0.0:8080"));
    assert!(!is_loopback_bind("[::]:8080"));
    assert!(!is_loopback_bind("10.0.0.1:8080"));
    assert!(!is_loopback_bind("192.168.1.1:8080"));
    assert!(!is_loopback_bind("not-an-addr"));
}

#[test]
fn warn_non_loopback_listener_without_auth() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "public".to_string(),
            bind: "0.0.0.0:8080".to_string(),
            protocols: vec!["http".to_string()],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: None,
            shadowsocks: None,
            ssr: None,
            trojan: None,
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let warnings = validate_config_security(&config);
    assert!(!warnings.is_empty());
    assert!(warnings[0].message.contains("0.0.0.0:8080"));
}

#[test]
fn no_warn_loopback_listener() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "local".to_string(),
            bind: "127.0.0.1:8080".to_string(),
            protocols: vec!["http".to_string()],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: None,
            shadowsocks: None,
            ssr: None,
            trojan: None,
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let warnings = validate_config_security(&config);
    assert!(warnings.is_empty());
}

#[test]
fn no_warn_authed_listener() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "public-ss".to_string(),
            bind: "0.0.0.0:8388".to_string(),
            protocols: vec!["shadowsocks".to_string()],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: None,
            shadowsocks: Some(crate::model::ShadowsocksListenerConfig {
                method: "aes-256-gcm".to_string(),
                password: "secret".to_string(),
                auth_prefix: None,
                plugins: Vec::new(),
            }),
            ssr: None,
            trojan: None,
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let warnings = validate_config_security(&config);
    // Shadowsocks provides its own auth, so no warning
    assert!(warnings.is_empty());
}

#[test]
fn warn_non_loopback_admin() {
    let config = ConfigFile {
        version: Some(1),
        listeners: None,
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: Some(crate::model::AdminConfig {
            bind: Some("0.0.0.0:9090".to_string()),
            enabled: None,
            metrics: None,
            allowed_hosts: None,
            auth: None,
            pac: None,
            static_content: None,
        }),
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let warnings = validate_config_security(&config);
    assert!(!warnings.is_empty());
    assert!(warnings.iter().any(|w| w.path == "admin.bind"));
}

#[test]
fn no_warn_loopback_admin() {
    let config = ConfigFile {
        version: Some(1),
        listeners: None,
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: Some(crate::model::AdminConfig {
            bind: Some("127.0.0.1:9090".to_string()),
            enabled: None,
            metrics: None,
            allowed_hosts: None,
            auth: None,
            pac: None,
            static_content: None,
        }),
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let warnings = validate_config_security(&config);
    assert!(warnings.is_empty());
}

#[test]
fn warn_reverse_control_bind_without_auth() {
    let config = ConfigFile {
        version: Some(1),
        listeners: None,
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: Some(vec![crate::model::ReverseServerConfig {
            id: "rs1".to_string(),
            control_bind: "0.0.0.0:8443".to_string(),
            external_bind: "0.0.0.0:9000".to_string(),
            auth_username: None,
            auth_password: None,
            auth_password_env: None,
            max_streams: None,
            heartbeat_interval: None,
            pproxy_compat: false,
            tls: None,
        }]),
        reverse_clients: None,
    };
    let warnings = validate_config_security(&config);
    assert!(!warnings.is_empty());
    assert!(warnings.iter().any(|w| w.path.contains("control_bind")));
}

#[test]
fn no_warn_reverse_control_bind_with_auth() {
    let config = ConfigFile {
        version: Some(1),
        listeners: None,
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: Some(vec![crate::model::ReverseServerConfig {
            id: "rs1".to_string(),
            control_bind: "0.0.0.0:8443".to_string(),
            external_bind: "0.0.0.0:9000".to_string(),
            auth_username: Some("user".to_string()),
            auth_password: Some("pass".to_string()),
            auth_password_env: None,
            max_streams: None,
            heartbeat_interval: None,
            pproxy_compat: false,
            tls: None,
        }]),
        reverse_clients: None,
    };
    let warnings = validate_config_security(&config);
    assert!(warnings.is_empty());
}

#[test]
fn no_warn_reverse_control_bind_with_env_auth() {
    let config = ConfigFile {
        version: Some(1),
        listeners: None,
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: Some(vec![crate::model::ReverseServerConfig {
            id: "rs1".to_string(),
            control_bind: "0.0.0.0:8443".to_string(),
            external_bind: "0.0.0.0:9000".to_string(),
            auth_username: Some("user".to_string()),
            auth_password: None,
            auth_password_env: Some("MY_SECRET".to_string()),
            max_streams: None,
            heartbeat_interval: None,
            pproxy_compat: false,
            tls: None,
        }]),
        reverse_clients: None,
    };
    let warnings = validate_config_security(&config);
    assert!(warnings.is_empty());
}

#[test]
fn warn_trojan_listener_without_auth() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "public-trojan".to_string(),
            bind: "0.0.0.0:443".to_string(),
            protocols: vec!["trojan".to_string()],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: Some(crate::model::ListenerTlsConfig {
                cert: "/path/cert.pem".to_string(),
                key: "/path/key.pem".to_string(),
                alpn: None,
            }),
            shadowsocks: None,
            ssr: None,
            trojan: Some(crate::model::ListenerTrojanConfig {
                password: "secret".to_string(),
                fallback: None,
            }),
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    // Trojan provides its own auth via password hash, no warning expected
    let warnings = validate_config_security(&config);
    assert!(warnings.is_empty());
}

#[test]
fn validate_trojan_requires_tls() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "trojan-notls".to_string(),
            bind: "127.0.0.1:443".to_string(),
            protocols: vec!["trojan".to_string()],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: None,
            shadowsocks: None,
            ssr: None,
            trojan: Some(crate::model::ListenerTrojanConfig {
                password: "secret".to_string(),
                fallback: None,
            }),
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let result = validate_config(&config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.to_string().contains("requires TLS")));
}

#[test]
fn validate_trojan_requires_trojan_section() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "trojan-nosection".to_string(),
            bind: "127.0.0.1:443".to_string(),
            protocols: vec!["trojan".to_string()],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: Some(crate::model::ListenerTlsConfig {
                cert: "/path/cert.pem".to_string(),
                key: "/path/key.pem".to_string(),
                alpn: None,
            }),
            shadowsocks: None,
            ssr: None,
            trojan: None,
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let result = validate_config(&config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.to_string().contains("requires [listeners.trojan]")));
}

#[test]
fn validate_trojan_empty_password_rejected() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "trojan-empty".to_string(),
            bind: "127.0.0.1:443".to_string(),
            protocols: vec!["trojan".to_string()],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: Some(crate::model::ListenerTlsConfig {
                cert: "/path/cert.pem".to_string(),
                key: "/path/key.pem".to_string(),
                alpn: None,
            }),
            shadowsocks: None,
            ssr: None,
            trojan: Some(crate::model::ListenerTrojanConfig {
                password: String::new(),
                fallback: None,
            }),
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let result = validate_config(&config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.to_string().contains("password must not be empty")));
}

#[test]
fn validate_empty_protocols_rejected() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "bad".to_string(),
            bind: "127.0.0.1:0".to_string(),
            protocols: vec![],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: None,
            shadowsocks: None,
            ssr: None,
            trojan: None,
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let result = validate_config(&config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.to_string().contains("protocols must not be empty")));
}

#[test]
fn validate_trojan_with_tls_and_password_passes() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "trojan-valid".to_string(),
            bind: "127.0.0.1:443".to_string(),
            protocols: vec!["trojan".to_string()],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: Some(crate::model::ListenerTlsConfig {
                cert: "/path/cert.pem".to_string(),
                key: "/path/key.pem".to_string(),
                alpn: None,
            }),
            shadowsocks: None,
            ssr: None,
            trojan: Some(crate::model::ListenerTrojanConfig {
                password: "my-secret".to_string(),
                fallback: None,
            }),
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let result = validate_config(&config);
    assert!(
        result.is_ok(),
        "valid trojan config should pass: {:?}",
        result.err()
    );
}

#[test]
fn validate_trojan_fallback_invalid_address_rejected() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "trojan-bad-fallback".to_string(),
            bind: "127.0.0.1:443".to_string(),
            protocols: vec!["trojan".to_string()],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: Some(crate::model::ListenerTlsConfig {
                cert: "/path/cert.pem".to_string(),
                key: "/path/key.pem".to_string(),
                alpn: None,
            }),
            shadowsocks: None,
            ssr: None,
            trojan: Some(crate::model::ListenerTrojanConfig {
                password: "secret".to_string(),
                fallback: Some("not-a-valid-address".to_string()),
            }),
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let result = validate_config(&config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.to_string().contains("invalid fallback address")));
}

#[test]
fn validate_trojan_fallback_valid_address_passes() {
    let config = ConfigFile {
        version: Some(1),
        listeners: Some(vec![crate::model::ListenerConfig {
            name: "trojan-good-fallback".to_string(),
            bind: "127.0.0.1:443".to_string(),
            protocols: vec!["trojan".to_string()],
            reuse_port: None,
            connection_limit: None,
            auth: None,
            udp_enabled: None,
            udp: None,
            tls: Some(crate::model::ListenerTlsConfig {
                cert: "/path/cert.pem".to_string(),
                key: "/path/key.pem".to_string(),
                alpn: None,
            }),
            shadowsocks: None,
            ssr: None,
            trojan: Some(crate::model::ListenerTrojanConfig {
                password: "secret".to_string(),
                fallback: Some("127.0.0.1:443".to_string()),
            }),
            transparent: None,
            unix: None,
            fixed_target: None,
            local_bind: None,
        }]),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    };
    let result = validate_config(&config);
    assert!(
        result.is_ok(),
        "valid trojan config with fallback should pass: {:?}",
        result.err()
    );
}

// === Auth-without-TLS warning ===

fn authed_listener(
    bind: &str,
    protocols: &[&str],
    tls: Option<crate::model::ListenerTlsConfig>,
    shadowsocks: Option<crate::model::ShadowsocksListenerConfig>,
    trojan: Option<crate::model::ListenerTrojanConfig>,
) -> crate::model::ListenerConfig {
    crate::model::ListenerConfig {
        name: "proxy".to_string(),
        bind: bind.to_string(),
        protocols: protocols.iter().map(|p| (*p).to_string()).collect(),
        reuse_port: None,
        connection_limit: None,
        auth: Some(crate::model::AuthConfig {
            auth_type: "password".to_string(),
            username: Some("proxyuser".to_string()),
            password: Some("s3cr3t-value".to_string()),
            password_env: None,
        }),
        udp_enabled: None,
        udp: None,
        tls,
        shadowsocks,
        ssr: None,
        trojan,
        transparent: None,
        unix: None,
        fixed_target: None,
        local_bind: None,
    }
}

fn config_with_listeners(listeners: Vec<crate::model::ListenerConfig>) -> ConfigFile {
    ConfigFile {
        version: Some(1),
        listeners: Some(listeners),
        upstreams: None,
        upstream_groups: None,
        rules: None,
        rules_file: None,
        routing: None,
        admin: None,
        process: None,
        timeouts: None,
        reverse_servers: None,
        reverse_clients: None,
    }
}

#[test]
fn warn_non_loopback_http_auth_without_tls() {
    let config = config_with_listeners(vec![authed_listener(
        "0.0.0.0:1080",
        &["http"],
        None,
        None,
        None,
    )]);
    let warnings = validate_config_security(&config);
    let warning = warnings
        .iter()
        .find(|w| w.path == "listeners[0].tls")
        .expect("auth without TLS must warn");
    assert!(warning.message.contains("0.0.0.0:1080"));
    assert!(warning.message.contains("[listeners.tls]"));
    // The warning must never echo the configured credential.
    assert!(!warning.message.contains("proxyuser"));
    assert!(!warning.message.contains("s3cr3t-value"));
}

#[test]
fn warn_non_loopback_socks4_and_socks5_auth_without_tls() {
    for protocol in ["socks4", "socks5"] {
        let config = config_with_listeners(vec![authed_listener(
            "0.0.0.0:1080",
            &[protocol],
            None,
            None,
            None,
        )]);
        let warnings = validate_config_security(&config);
        assert!(
            warnings.iter().any(|w| w.path == "listeners[0].tls"),
            "protocol {protocol} with auth and no TLS must warn"
        );
    }
}

#[test]
fn no_warn_authed_http_with_tls() {
    let config = config_with_listeners(vec![authed_listener(
        "0.0.0.0:1080",
        &["http"],
        Some(crate::model::ListenerTlsConfig {
            cert: "/path/cert.pem".to_string(),
            key: "/path/key.pem".to_string(),
            alpn: None,
        }),
        None,
        None,
    )]);
    let warnings = validate_config_security(&config);
    assert!(
        warnings.is_empty(),
        "TLS listener must not warn: {warnings:?}"
    );
}

#[test]
fn no_warn_authed_shadowsocks_without_tls() {
    let config = config_with_listeners(vec![authed_listener(
        "0.0.0.0:8388",
        &["shadowsocks"],
        None,
        Some(crate::model::ShadowsocksListenerConfig {
            method: "aes-256-gcm".to_string(),
            password: "ss-password".to_string(),
            auth_prefix: None,
            plugins: Vec::new(),
        }),
        None,
    )]);
    let warnings = validate_config_security(&config);
    assert!(
        warnings.is_empty(),
        "Shadowsocks carries its own encryption: {warnings:?}"
    );
}

#[test]
fn no_warn_authed_trojan_protocol_without_tls() {
    // Trojan requires TLS structurally (a separate validation error), so it
    // must never produce this warning.
    let config = config_with_listeners(vec![authed_listener(
        "0.0.0.0:443",
        &["trojan"],
        None,
        None,
        Some(crate::model::ListenerTrojanConfig {
            password: "trojan-password".to_string(),
            fallback: None,
        }),
    )]);
    let warnings = validate_config_security(&config);
    assert!(
        warnings.is_empty(),
        "Trojan must not produce the HTTP/SOCKS auth-without-TLS warning: {warnings:?}"
    );
}

#[test]
fn no_warn_loopback_http_auth_without_tls() {
    let config = config_with_listeners(vec![authed_listener(
        "127.0.0.1:1080",
        &["http"],
        None,
        None,
        None,
    )]);
    let warnings = validate_config_security(&config);
    assert!(
        warnings.is_empty(),
        "loopback bind must not warn: {warnings:?}"
    );
}

// === Literal-IP-only CIDR warning ===

fn warnings_for(rules_toml: &str) -> Vec<crate::error::ConfigWarning> {
    let config: ConfigFile = toml::from_str(rules_toml).expect("test config must parse");
    validate_config_security(&config)
}

#[test]
fn warn_private_destination_cidr_reject_rule_without_host_condition() {
    let warnings = warnings_for(
        r#"
version = 1

[[rules]]
id = "block-private"
reject = "blocked"

[rules.match]
any_of = [
  { destination_cidr = "10.0.0.0/8" },
  { destination_cidr = "192.168.0.0/16" },
]
"#,
    );
    let warning = warnings
        .iter()
        .find(|w| w.path.starts_with("rules[0]"))
        .unwrap_or_else(|| panic!("private CIDR reject rule must warn: {warnings:?}"));
    assert_eq!(warning.path, "rules[0].match.any_of[0].destination_cidr");
    assert!(warning.message.contains("10.0.0.0/8"));
    assert!(warning.message.contains("literal IP targets only"));
    assert!(warning.message.contains("resolved by the upstream"));
    assert!(warning.message.contains("allow_private_egress"));
}

#[test]
fn warn_private_destination_cidr_inside_negation() {
    // A negated CIDR is still literal-only: `not { destination_cidr }` cannot
    // see a name that resolves into the range.
    let warnings = warnings_for(
        r#"
version = 1

[[rules]]
id = "block-non-rfc1918"
reject = "access-denied"

[rules.match]
not = { destination_cidr = "10.0.0.0/8" }
"#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.path == "rules[0].match.not.destination_cidr"),
        "negated private CIDR must warn: {warnings:?}"
    );
}

#[test]
fn warn_private_ipv4_mapped_destination_cidr() {
    let warnings = warnings_for(
        r#"
version = 1

[[rules]]
id = "block-mapped-private"
reject = "blocked"

[rules.match]
destination_cidr = "::ffff:10.0.0.0/104"
"#,
    );
    let warning = warnings
        .iter()
        .find(|w| w.path.starts_with("rules[0]"))
        .unwrap_or_else(|| panic!("mapped private CIDR must warn: {warnings:?}"));
    assert!(warning.message.contains("::ffff:10.0.0.0/104"));
}

#[test]
fn no_warn_private_cidr_when_host_condition_present() {
    let warnings = warnings_for(
        r#"
version = 1

[[rules]]
id = "block-internal-http"
reject = "blocked"

[rules.match]
all = [
  { host_suffix = "internal.example" },
  { destination_cidr = "10.0.0.0/8" },
]
"#,
    );
    assert!(
        warnings.is_empty(),
        "a rule that also scopes names must not warn: {warnings:?}"
    );
}

#[test]
fn no_warn_private_cidr_when_shortcut_host_condition_present() {
    let warnings = warnings_for(
        r#"
version = 1

[[rules]]
id = "block-internal-literal"
host_regex = "^10\\."
reject = "blocked"

[rules.match]
destination_cidr = "10.0.0.0/8"
"#,
    );
    assert!(
        warnings.is_empty(),
        "a shortcut host condition must suppress the warning: {warnings:?}"
    );
}

#[test]
fn no_warn_public_destination_cidr_reject_rule() {
    let warnings = warnings_for(
        r#"
version = 1

[[rules]]
id = "block-doc-range"
reject = "blocked"

[rules.match]
destination_cidr = "8.8.8.0/24"
"#,
    );
    assert!(
        warnings.is_empty(),
        "a public range is not private-network policy: {warnings:?}"
    );
}

#[test]
fn no_warn_private_cidr_for_non_reject_action() {
    let warnings = warnings_for(
        r#"
version = 1

[[rules]]
id = "direct-private"
direct = true

[rules.match]
destination_cidr = "10.0.0.0/8"
"#,
    );
    assert!(
        warnings.is_empty(),
        "only reject rules claim to block private networks: {warnings:?}"
    );
}

#[test]
fn cidr_warning_does_not_echo_credentials_or_upstream_uri() {
    let warnings = warnings_for(
        r#"
version = 1

[[upstreams]]
id = "egress"
uri = "socks5://user:sup3rsecret@203.0.113.10:1080"

[[rules]]
id = "block-private"
reject = "blocked"

[rules.match]
destination_cidr = "10.0.0.0/8"
"#,
    );
    let warning = warnings
        .iter()
        .find(|w| w.path.starts_with("rules[0]"))
        .expect("private CIDR reject rule must warn");
    assert!(!warning.message.contains("sup3rsecret"));
    assert!(!warning.message.contains("203.0.113.10"));
    for warning in &warnings {
        assert!(!warning.message.contains("sup3rsecret"));
    }
}

#[test]
fn warn_plaintext_non_loopback_admin_even_with_auth() {
    let warnings = warnings_for(
        r#"
version = 1

[admin]
bind = "0.0.0.0:9090"

[admin.auth]
bearer_token = "adm1n-s3cret-token"
"#,
    );
    let warning = warnings
        .iter()
        .find(|w| w.message.contains("plaintext HTTP"))
        .unwrap_or_else(|| panic!("non-loopback admin must warn: {warnings:?}"));
    assert_eq!(warning.path, "admin.bind");
    assert!(warning.message.contains("0.0.0.0:9090"));
    assert!(warning.message.contains("allowed_hosts"));
    assert!(warning.message.contains("VPN"));
    for warning in &warnings {
        assert!(!warning.message.contains("adm1n-s3cret-token"));
    }
}
