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
