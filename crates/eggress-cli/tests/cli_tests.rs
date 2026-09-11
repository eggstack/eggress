#[test]
fn test_cli_help() {
    assert_cmd::Command::cargo_bin("eggress")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("eggress"))
        .stdout(predicates::str::contains("--listen"))
        .stdout(predicates::str::contains("--remote"));
}

/// `--config` is a single global option: it is accepted before and after
/// the subcommand with identical effect.
#[test]
fn global_config_placement_before_and_after_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("eggress.toml");
    std::fs::write(
        &config_path,
        r#"
version = 1
[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#,
    )
    .unwrap();

    for args in [
        vec![
            "--config",
            config_path.to_str().unwrap(),
            "route",
            "example.com:443",
        ],
        vec![
            "route",
            "example.com:443",
            "--config",
            config_path.to_str().unwrap(),
        ],
        vec![
            "route",
            "example.com:443",
            "-c",
            config_path.to_str().unwrap(),
        ],
    ] {
        assert_cmd::Command::cargo_bin("eggress")
            .unwrap()
            .args(&args)
            .assert()
            .success()
            .stdout(predicates::str::contains("Target: example.com:443"));
    }
}

/// Closed value domains fail at parse time instead of silently falling back.
#[test]
fn invalid_closed_domain_values_are_parse_errors() {
    // --log-format
    assert_cmd::Command::cargo_bin("eggress")
        .unwrap()
        .args(["--log-format", "jsno", "version"])
        .assert()
        .code(2);
    // upstream test --mode
    assert_cmd::Command::cargo_bin("eggress")
        .unwrap()
        .args(["upstream", "test", "--mode", "socks"])
        .assert()
        .code(2);
    // route --protocol
    assert_cmd::Command::cargo_bin("eggress")
        .unwrap()
        .args(["route", "example.com:443", "--protocol", "quic"])
        .assert()
        .code(2);
}

/// A listener that cannot bind fails closed with the bind exit code.
#[test]
fn occupied_listener_port_exits_bind_failure() {
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupied.local_addr().unwrap().port();
    assert_cmd::Command::cargo_bin("eggress")
        .unwrap()
        .args(["-l", &format!("http://127.0.0.1:{port}")])
        .timeout(std::time::Duration::from_secs(20))
        .assert()
        .code(4)
        .stderr(predicates::str::contains("failed to bind"));
}
