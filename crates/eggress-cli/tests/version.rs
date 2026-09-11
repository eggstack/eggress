use std::process::Command;

fn eggress_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_eggress"));
    cmd.env("RUST_LOG", "error");
    cmd
}

#[test]
fn version_subcommand_prints_stable_version_line() {
    let output = eggress_bin()
        .arg("version")
        .output()
        .expect("failed to run eggress version");
    assert_eq!(
        output.status.code(),
        Some(0),
        "eggress version must exit 0, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected = format!("eggress {}", env!("CARGO_PKG_VERSION"));
    assert_eq!(
        stdout.trim_end(),
        expected,
        "eggress version must print exactly one stable line"
    );
    assert_eq!(
        stdout.lines().count(),
        1,
        "eggress version must print exactly one line, got: {stdout:?}"
    );
}

#[test]
fn version_flag_remains_functional() {
    let output = eggress_bin()
        .arg("--version")
        .output()
        .expect("failed to run eggress --version");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "eggress --version must report the package version, got: {stdout:?}"
    );
}

#[test]
fn pproxy_version_reports_release_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_pproxy"))
        .arg("--version")
        .output()
        .expect("failed to run pproxy --version");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "pproxy --version must report the release version, got: {stdout:?}"
    );
}
