use std::io::Write;
use tempfile::NamedTempFile;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[test]
fn reload_increments_generation() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();

    let config = eggress_embed::EggressConfig::from_toml_file(path).unwrap();
    let toml_source = config.source_toml().to_string();
    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let gen_before = handle.status().generation;
    assert_eq!(gen_before, 0);

    let result = handle.reload_toml_str(&toml_source);
    let outcome = result.unwrap();
    match outcome {
        eggress_embed::ReloadOutcome::Applied { generation, .. } => {
            assert_eq!(generation, 1);
        }
    }

    let gen_after = handle.status().generation;
    assert_eq!(gen_after, 1);

    handle.shutdown_blocking().unwrap();
}

#[test]
fn reload_invalid_config_returns_error() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();

    let config = eggress_embed::EggressConfig::from_toml_file(path).unwrap();
    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let gen_before = handle.status().generation;

    let result = handle.reload_toml_str("not valid toml {{{");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, eggress_embed::EggressError::Reload(_)));

    let gen_after = handle.status().generation;
    assert_eq!(
        gen_before, gen_after,
        "failed reload should not change generation"
    );

    handle.shutdown_blocking().unwrap();
}

#[test]
fn reload_rejects_listener_count_change() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();

    let config = eggress_embed::EggressConfig::from_toml_file(path1).unwrap();
    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;

    let result = handle.reload_toml_str(config2);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("listener count"),
        "should mention listener count: {err_msg}"
    );

    handle.shutdown_blocking().unwrap();
}

#[test]
fn reload_rejects_listener_bind_change() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();

    let config = eggress_embed::EggressConfig::from_toml_file(path1).unwrap();
    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:9999"
protocols = ["http"]
"#;

    let result = handle.reload_toml_str(config2);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("bind"),
        "should mention bind change: {err_msg}"
    );

    handle.shutdown_blocking().unwrap();
}

#[test]
fn reload_from_file() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();

    let eggress_config = eggress_embed::EggressConfig::from_toml_file(path).unwrap();
    let handle = eggress_embed::EggressService::new(eggress_config)
        .start_blocking()
        .unwrap();

    let result = handle.reload_toml_file(path);
    assert!(result.is_ok());
    let outcome = result.unwrap();
    match outcome {
        eggress_embed::ReloadOutcome::Applied { generation, .. } => {
            assert_eq!(generation, 1);
        }
    }

    handle.shutdown_blocking().unwrap();
}
