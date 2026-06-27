#[test]
fn blocking_start_and_shutdown() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#,
    )
    .unwrap();

    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let addrs = handle.bound_addresses();
    assert_eq!(addrs.listeners.len(), 1);
    assert_eq!(addrs.listeners[0].name, "socks");
    assert!(addrs.listeners[0].addr.port() > 0);

    let status = handle.status();
    assert!(status.readiness);
    assert_eq!(status.generation, 0);
    assert_eq!(status.active_connections, 0);

    handle.shutdown_blocking().unwrap();
}

#[tokio::test]
async fn async_start_and_shutdown() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "http"
bind = "127.0.0.1:0"
protocols = ["http"]
"#,
    )
    .unwrap();

    let handle = eggress_embed::EggressService::new(config)
        .start()
        .await
        .unwrap();

    let addrs = handle.bound_addresses();
    assert_eq!(addrs.listeners.len(), 1);
    assert_eq!(addrs.listeners[0].name, "http");

    let status = handle.status();
    assert!(status.readiness);

    handle.shutdown().await.unwrap();
}

#[test]
fn multiple_listeners() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#,
    )
    .unwrap();

    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let addrs = handle.bound_addresses();
    assert_eq!(addrs.listeners.len(), 2);

    let http_addr = addrs.listener("http-in").expect("http-in listener");
    let socks_addr = addrs.listener("socks-in").expect("socks-in listener");
    assert!(http_addr.port() > 0);
    assert!(socks_addr.port() > 0);
    assert_ne!(http_addr, socks_addr);

    handle.shutdown_blocking().unwrap();
}

#[test]
fn invalid_config_rejected() {
    let result = eggress_embed::EggressConfig::from_toml_str("not valid toml {{{");
    assert!(result.is_err());
    match result.err().unwrap() {
        eggress_embed::EggressError::Config(_) => {}
        other => panic!("expected Config error, got: {other}"),
    }
}

#[test]
fn empty_config_is_valid() {
    // An empty TOML config is valid because all fields are optional.
    // It simply results in a service with no listeners.
    let result = eggress_embed::EggressConfig::from_toml_str("");
    assert!(result.is_ok());
}

#[test]
fn unsupported_version_rejected() {
    let result = eggress_embed::EggressConfig::from_toml_str("version = 2");
    assert!(result.is_err());
    let err_msg = result.err().unwrap().to_string();
    assert!(err_msg.contains("version"));
}
