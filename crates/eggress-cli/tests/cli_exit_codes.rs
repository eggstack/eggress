use std::process::Command;

fn eggress_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_eggress"));
    cmd.env("RUST_LOG", "error");
    cmd
}

#[test]
fn test_exit_code_cli_parse_error() {
    let output = eggress_bin()
        .arg("--nonexistent-flag")
        .output()
        .expect("failed to run eggress");
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit code 2 (CLI parse error), got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn test_exit_code_config_validation() {
    let output = eggress_bin()
        .args(["--config", "/nonexistent/path.toml"])
        .output()
        .expect("failed to run eggress");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.code() != Some(0),
        "expected non-zero exit for missing config, got 0\nstderr: {stderr}",
    );
    assert!(
        stderr.contains("error") || stderr.contains("No such file"),
        "expected error about missing file, got: {stderr}",
    );
}

#[test]
fn test_exit_code_unsupported_legacy_cipher() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "translate",
            "--",
            "-l",
            "ss://aes-256-ctr:secret@proxy:8388",
        ])
        .output()
        .expect("failed to run eggress");
    assert_eq!(
        output.status.code(),
        Some(5),
        "expected exit code 5 (unsupported legacy cipher), got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported") || stderr.contains("ssr"),
        "expected unsupported/cipher message in stderr, got: {stderr}",
    );
}

#[test]
fn test_exit_code_success() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "translate",
            "--",
            "-l",
            "socks5://127.0.0.1:1080",
            "-r",
            "http://127.0.0.1:8080",
        ])
        .output()
        .expect("failed to run eggress");
    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit code 0 (success), got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("listeners"),
        "expected TOML output with listeners, got: {stdout}",
    );
}

#[test]
fn test_exit_code_check_bounded_ssr_still_zero() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "check",
            "--",
            "-l",
            "ssr://aes-256-ctr:secret@proxy:8388",
        ])
        .output()
        .expect("failed to run eggress");
    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit code 0 (check reports, doesn't fail), got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Compatible") || stdout.contains("warning"),
        "expected parity report mentioning bounded SSR compatibility, got: {stdout}",
    );
}
