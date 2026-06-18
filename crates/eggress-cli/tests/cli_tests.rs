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
