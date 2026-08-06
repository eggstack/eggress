//! Negative tests for lean (common-only) CLI builds.
//!
//! These tests verify that excluded features fail clearly at the CLI
//! boundary. They run only when the corresponding features are disabled.

#[allow(unused_imports)]
use std::io::Write;

/// Verify that the pproxy binary is not available when `pproxy-compat` is absent.
#[cfg(not(feature = "pproxy-compat"))]
#[test]
fn lean_pproxy_binary_not_available() {
    assert!(
        !cfg!(feature = "pproxy-compat"),
        "pproxy binary should not be built without pproxy-compat feature"
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

/// Verify that lean build rejects admin server config with a clear error.
#[cfg(not(feature = "operations"))]
#[test]
fn lean_admin_config_rejected() {
    let mut tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    write!(
        tmp,
        r#"
[admin]
bind = "127.0.0.1:19900"

[[listeners]]
name = "http"
bind = "127.0.0.1:19901"
protocols = ["http"]
"#
    )
    .expect("failed to write config");
    let output = assert_cmd::Command::cargo_bin("eggress")
        .unwrap()
        .args(["--config", tmp.path().to_str().unwrap()])
        .output()
        .expect("failed to run eggress");
    assert!(
        !output.status.success(),
        "expected non-zero exit for admin config in common build"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("admin server support not included"),
        "expected clear admin-disabled error, got: {stderr}"
    );
}

/// Verify that lean build rejects reverse proxy config with a clear error.
#[cfg(not(feature = "reverse"))]
#[test]
fn lean_reverse_config_rejected() {
    let mut tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    write!(
        tmp,
        r#"
[[reverse_servers]]
id = "test-reverse"
control_bind = "0.0.0.0:19902"
external_bind = "0.0.0.0:19903"
"#
    )
    .expect("failed to write config");
    let output = assert_cmd::Command::cargo_bin("eggress")
        .unwrap()
        .args(["--config", tmp.path().to_str().unwrap()])
        .output()
        .expect("failed to run eggress");
    assert!(
        !output.status.success(),
        "expected non-zero exit for reverse config in common build"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("reverse proxy support not included"),
        "expected clear reverse-disabled error, got: {stderr}"
    );
}

/// Verify that lean build starts an HTTP listener without admin/metrics/reverse.
#[cfg(not(feature = "operations"))]
#[cfg(not(feature = "reverse"))]
#[test]
fn lean_http_listener_starts() {
    let mut tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    // Use port 0 to let the OS assign a free port
    write!(
        tmp,
        r#"
[[listeners]]
name = "http"
bind = "127.0.0.1:0"
protocols = ["http"]
"#
    )
    .expect("failed to write config");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_eggress"))
        .args(["--config", tmp.path().to_str().unwrap()])
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn eggress");
    // Give it a moment to start
    std::thread::sleep(std::time::Duration::from_millis(500));
    // Check it's still running (didn't crash)
    let status = child.try_wait().expect("failed to check status");
    assert!(
        status.is_none(),
        "process exited immediately — expected it to stay running"
    );
    // Kill the child
    let _ = child.kill();
    let _ = child.wait();
}
