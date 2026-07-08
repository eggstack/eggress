#[test]
fn error_messages_do_not_contain_credentials() {
    // Invalid TOML should not leak credentials in error
    let result = eggress_embed::EggressConfig::from_toml_str("not valid {{{");
    assert!(result.is_err());
    let err_msg = result.err().unwrap().to_string();
    assert!(
        !err_msg.contains("secret_user"),
        "error should not contain username"
    );
    assert!(
        !err_msg.contains("super_secret_password_123"),
        "error should not contain password"
    );
}

#[test]
fn error_category_labels() {
    let config_err = eggress_embed::EggressConfig::from_toml_str("bad");
    assert!(config_err.is_err());
    match config_err.err().unwrap() {
        eggress_embed::EggressError::Config(_) => {}
        other => panic!("expected Config error, got: {other}"),
    }
}

#[test]
fn startup_error_category() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "bad"
bind = "not-a-valid-address"
protocols = ["http"]
"#,
    )
    .unwrap();

    let result = eggress_embed::EggressService::new(config).start_blocking();
    assert!(result.is_err());
    match result.err().unwrap() {
        eggress_embed::EggressError::Startup(_) => {}
        other => panic!("expected Startup error, got: {other}"),
    }
}

#[test]
fn unsupported_feature_error_category() {
    let err = eggress_embed::EggressError::UnsupportedFeature {
        feature: "masque".to_string(),
        message: "not yet implemented".to_string(),
    };
    assert_eq!(err.category(), "unsupported_feature");
    assert!(err.to_string().contains("masque"));
}

#[test]
fn to_redacted_toml_hides_listener_password() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "admin"
password = "super_secret_123"
"#,
    )
    .unwrap();

    let redacted = config.to_redacted_toml().unwrap();
    assert!(
        !redacted.contains("super_secret_123"),
        "redacted TOML should not contain password"
    );
    assert!(
        redacted.contains("****"),
        "redacted TOML should contain placeholder"
    );
    // Username is not a secret, should remain
    assert!(
        redacted.contains("admin"),
        "redacted TOML should still contain username"
    );
}

#[test]
fn to_redacted_toml_hides_password_env() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.auth]
type = "password"
password_env = "MY_SECRET_ENV_VAR"
"#,
    )
    .unwrap();

    let redacted = config.to_redacted_toml().unwrap();
    assert!(
        !redacted.contains("MY_SECRET_ENV_VAR"),
        "redacted TOML should not contain password_env"
    );
    assert!(redacted.contains("****"));
}

#[test]
fn to_redacted_toml_hides_upstream_uri_credentials() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[upstreams]]
id = "up1"
uri = "socks5://myuser:my_password_42@10.0.0.1:1080"
"#,
    )
    .unwrap();

    let redacted = config.to_redacted_toml().unwrap();
    assert!(
        !redacted.contains("my_password_42"),
        "redacted TOML should not contain URI password"
    );
    assert!(
        !redacted.contains("myuser"),
        "redacted TOML should not contain URI username"
    );
    assert!(redacted.contains("****:****@"));
    assert!(redacted.contains("10.0.0.1:1080"));
}

#[test]
fn to_redacted_toml_preserves_uri_without_credentials() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[upstreams]]
id = "up1"
uri = "socks5://10.0.0.1:1080"
"#,
    )
    .unwrap();

    let redacted = config.to_redacted_toml().unwrap();
    assert!(redacted.contains("socks5://10.0.0.1:1080"));
}

#[test]
fn to_redacted_toml_is_valid_toml() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "admin"
password = "secret"

[[upstreams]]
id = "up1"
uri = "socks5://user:pass@10.0.0.1:1080"
"#,
    )
    .unwrap();

    let redacted = config.to_redacted_toml().unwrap();
    // Should parse without error
    let parsed: toml::Value = toml::from_str(&redacted).unwrap();
    assert_eq!(parsed["version"].as_integer(), Some(1));
}

#[test]
fn to_redacted_toml_preserves_password_containing_at_sign() {
    // Regression: a raw '@' inside the password must not be treated as
    // the userinfo/host separator. The userinfo separator is the LAST
    // unbracketed '@' after the scheme.
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[upstreams]]
id = "up1"
uri = "socks5://admin:s3cret_p@ssw0rd@10.0.0.1:1080"
"#,
    )
    .unwrap();

    let redacted = config.to_redacted_toml().unwrap();
    assert!(
        !redacted.contains("s3cret_p"),
        "redacted TOML leaked password prefix: {redacted}"
    );
    assert!(
        !redacted.contains("ssw0rd"),
        "redacted TOML leaked password suffix: {redacted}"
    );
    assert!(
        !redacted.contains("admin"),
        "redacted TOML leaked username: {redacted}"
    );
    assert!(redacted.contains("****:****@10.0.0.1:1080"));
}
