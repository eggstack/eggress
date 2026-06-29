use std::process::Command;

fn eggress_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_eggress"));
    cmd.env("RUST_LOG", "error");
    cmd
}

#[test]
fn translate_socks5_direct() {
    let output = eggress_bin()
        .args(["pproxy", "translate", "--", "-l", "socks5://127.0.0.1:1080"])
        .output()
        .expect("failed to run eggress");
    assert!(
        output.status.success(),
        "translate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("socks5"));
    assert!(stdout.contains("127.0.0.1:1080"));
    assert!(stdout.contains("version = 1"));
}

#[test]
fn translate_with_upstream() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "translate",
            "--",
            "-l",
            "socks5://127.0.0.1:1080",
            "-r",
            "http://proxy:8080",
        ])
        .output()
        .expect("failed to run eggress");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pproxy-upstream-0"));
    assert!(stdout.contains("pproxy-chain"));
    assert!(stdout.contains("http://proxy:8080"));
}

#[test]
fn translate_with_scheduler() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "translate",
            "--",
            "-l",
            "socks5://127.0.0.1:1080",
            "-r",
            "http://proxy:8080",
            "-s",
            "rr",
        ])
        .output()
        .expect("failed to run eggress");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("round-robin"));
}

#[test]
fn translate_verbose_flag_warns() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "translate",
            "--",
            "-l",
            "socks5://127.0.0.1:1080",
            "-v",
        ])
        .output()
        .expect("failed to run eggress");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("verbose-mode"));
}

#[test]
fn translate_unknown_flag_warns() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "translate",
            "--",
            "-l",
            "socks5://127.0.0.1:1080",
            "--bogus-flag",
        ])
        .output()
        .expect("failed to run eggress");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown-flag"));
}

#[test]
fn translate_ssl_unsupported() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "translate",
            "--",
            "-l",
            "socks5://127.0.0.1:1080",
            "--ssl",
            "cert.pem,key.pem",
        ])
        .output()
        .expect("failed to run eggress");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ssl-listener"));
}

#[test]
fn translate_block_unsupported() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "translate",
            "--",
            "-l",
            "socks5://127.0.0.1:1080",
            "-b",
            ".*\\.example\\.com",
        ])
        .output()
        .expect("failed to run eggress");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("block-rules"));
}

#[test]
fn check_compatible_tier() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "check",
            "--",
            "-l",
            "socks5://127.0.0.1:1080",
            "-r",
            "http://proxy:8080",
        ])
        .output()
        .expect("failed to run eggress");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("parity tier: Compatible"));
}

#[test]
fn check_compatible_tier_for_shadowsocks() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "check",
            "--",
            "-l",
            "ss://aes-256-gcm:pass@proxy:8388",
        ])
        .output()
        .expect("failed to run eggress");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("parity tier: Compatible"));
}

#[test]
fn translate_no_local_listener_fails() {
    let output = eggress_bin()
        .args(["pproxy", "translate", "--"])
        .output()
        .expect("failed to run eggress");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no local listener"));
}

#[test]
fn translate_annotate_flag() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "translate",
            "--annotate",
            "--",
            "-l",
            "socks5://127.0.0.1:1080",
        ])
        .output()
        .expect("failed to run eggress");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("# Generated by eggress pproxy translate"));
}

#[test]
fn translate_chain_of_two_upstreams() {
    let output = eggress_bin()
        .args([
            "pproxy",
            "translate",
            "--",
            "-l",
            "socks5://127.0.0.1:1080",
            "-r",
            "http://proxy1:8080",
            "-r",
            "socks5://proxy2:1080",
        ])
        .output()
        .expect("failed to run eggress");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pproxy-upstream-0"));
    assert!(stdout.contains("pproxy-upstream-1"));
    assert!(stdout.contains("round-robin"));
}
