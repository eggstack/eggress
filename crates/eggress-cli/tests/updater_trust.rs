//! Updater trust-boundary contracts exercised through the real binary.
//!
//! These tests cover what only a built binary can prove:
//!
//! - a default/release build ignores `EGRESS_UPDATE_BASE_URL` and therefore
//!   cannot be redirected at a `file://` tree;
//! - a build with the non-default `update-fixtures` feature does honor the
//!   fixture base URL (the offline test hook still exists, behind the
//!   feature).

use std::path::PathBuf;
use std::process::Command;

fn eggress_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_eggress"));
    cmd.env("RUST_LOG", "error");
    cmd
}

fn fixture_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "eggress-updater-trust-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // Older than the running build, so a fixture-aware updater exits before
    // any installation or replacement step.
    std::fs::write(root.join("latest.json"), "v0.0.1").unwrap();
    root
}

/// Default/release builds must ignore the fixture hook: the only permitted
/// discovery URL is the HTTPS GitHub API.
#[cfg(not(feature = "update-fixtures"))]
#[test]
fn default_build_ignores_update_base_url_and_rejects_file_urls() {
    let root = fixture_root("release");
    let output = eggress_bin()
        .arg("update")
        .env(
            "EGRESS_UPDATE_BASE_URL",
            format!("file://{}", root.display()),
        )
        // Force any real HTTPS attempt to fail immediately and locally. If the
        // fixture base URL were honored, curl would read the local file and
        // report "already the latest stable version" instead.
        .env("https_proxy", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("all_proxy", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .env("no_proxy", "")
        .env("NO_PROXY", "")
        .output()
        .expect("failed to run eggress update");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stdout.contains("already the latest stable version"),
        "release updater honored EGRESS_UPDATE_BASE_URL/file://: stdout={stdout}"
    );
    assert_eq!(
        output.status.code(),
        Some(eggress_cli::EXIT_EXTERNAL_DEPENDENCY),
        "release updater must fail on the unreachable HTTPS discovery URL \
         (not read the fixture):\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("download failed"),
        "expected a discovery/download failure against GitHub, got: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// The offline fixture hook still works, but only when the non-default
/// `update-fixtures` feature is enabled at build time.
#[cfg(all(unix, feature = "update-fixtures"))]
#[test]
fn update_fixtures_feature_honors_file_base_url() {
    let root = fixture_root("feature");
    let output = eggress_bin()
        .arg("update")
        .env(
            "EGRESS_UPDATE_BASE_URL",
            format!("file://{}", root.display()),
        )
        .output()
        .expect("failed to run eggress update");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "fixture updater should read the local release metadata:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("already the latest stable version"),
        "fixture base URL was not honored: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
