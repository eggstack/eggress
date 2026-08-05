//! Negative tests for lean (common-only) CLI builds.
//!
//! These tests verify that excluded features fail clearly at the CLI
//! boundary. They run only when the corresponding features are disabled.

/// Verify that the pproxy binary is not available when `pproxy-compat` is absent.
#[cfg(not(feature = "pproxy-compat"))]
#[test]
fn lean_pproxy_binary_not_available() {
    let result = assert_cmd::Command::cargo_bin("pproxy");
    assert!(
        result.is_err(),
        "pproxy binary should not exist in builds without pproxy-compat feature"
    );
}

/// Verify that the pproxy subcommand is not recognized when `pproxy-compat` is absent.
#[cfg(not(feature = "pproxy-compat"))]
#[test]
fn lean_pproxy_subcommand_not_recognized() {
    assert_cmd::Command::cargo_bin("eggress")
        .unwrap()
        .args(["pproxy", "translate", "--", "-l", "http://:8080"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("error"));
}

/// Verify that the system-proxy subcommand is not recognized when `operations` is absent.
#[cfg(not(feature = "operations"))]
#[test]
fn lean_system_proxy_subcommand_not_recognized() {
    assert_cmd::Command::cargo_bin("eggress")
        .unwrap()
        .args(["system-proxy", "inspect"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("error"));
}

/// Verify that lean build still supports basic HTTP and SOCKS operations.
#[cfg(not(feature = "extended"))]
#[test]
fn lean_basic_help_works() {
    assert_cmd::Command::cargo_bin("eggress")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("eggress"));
}
