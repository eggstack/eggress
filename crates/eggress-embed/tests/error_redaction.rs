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
