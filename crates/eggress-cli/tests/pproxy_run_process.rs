use std::process::Command;

fn eggress_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_eggress"));
    cmd.env("RUST_LOG", "error");
    cmd
}

fn run_with_timeout(args: &[&str], timeout_ms: u64) -> std::process::Output {
    let mut child = eggress_bin()
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn eggress");

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = child.stdout;
                let stderr = child.stderr;
                let stdout_bytes = stdout
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).unwrap_or(0);
                        buf
                    })
                    .unwrap_or_default();
                let stderr_bytes = stderr
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).unwrap_or(0);
                        buf
                    })
                    .unwrap_or_default();
                return std::process::Output {
                    status,
                    stdout: stdout_bytes,
                    stderr: stderr_bytes,
                };
            }
            Ok(None) => {
                if start.elapsed().as_millis() > timeout_ms as u128 {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("process timed out after {timeout_ms}ms");
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => panic!("failed to check process status: {e}"),
        }
    }
}

#[test]
fn test_pproxy_run_invalid_args() {
    let output = run_with_timeout(&["pproxy", "run", "--", "-l"], 5000);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(0),
        "expected non-zero exit code, got 0\nstderr: {stderr}",
    );
    assert!(
        stderr.contains("error"),
        "expected error message in stderr, got: {stderr}",
    );
}

#[test]
fn test_pproxy_run_bind_failure() {
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            "socks5://127.0.0.1:19876",
            "-l",
            "socks5://127.0.0.1:19876",
        ],
        5000,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(0),
        "expected non-zero exit code for bind failure, got {:?}\nstderr: {stderr}",
        output.status.code(),
    );
    assert!(
        stderr.contains("bind")
            || stderr.contains("address")
            || stderr.contains("in use")
            || stderr.contains("runtime")
            || stderr.contains("error")
            || stderr.contains("Cannot start"),
        "expected bind/runtime error in stderr, got: {stderr}",
    );
}

#[test]
fn test_pproxy_run_unsupported_feature() {
    let output = run_with_timeout(&["pproxy", "run", "--", "-l", "ssh://host:22"], 5000);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(0),
        "expected non-zero exit code for unsupported feature, got {:?}\nstderr: {stderr}",
        output.status.code(),
    );
    assert!(
        stderr.contains("unsupported") || stderr.contains("error"),
        "expected unsupported/error message in stderr, got: {stderr}",
    );
}

#[test]
fn test_pproxy_run_unknown_flag_fails() {
    // `eggress pproxy run` must refuse unknown flags before startup,
    // matching the standalone `pproxy` binary's behavior.
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            "http://:19880",
            "-r",
            "socks5://127.0.0.1:1080",
            "--bogus-flag",
        ],
        5000,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(0),
        "expected non-zero exit code for unknown flag, got {:?}\nstderr: {stderr}",
        output.status.code(),
    );
    assert!(
        stderr.contains("--bogus-flag") || stderr.contains("unknown"),
        "expected unknown flag error in stderr, got: {stderr}",
    );
}

#[test]
fn test_pproxy_run_daemon_fails() {
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            "http://:19881",
            "-r",
            "socks5://127.0.0.1:1080",
            "--daemon",
        ],
        5000,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(0),
        "expected non-zero exit code for --daemon, got {:?}\nstderr: {stderr}",
        output.status.code(),
    );
    assert!(
        stderr.contains("daemon") || stderr.contains("not supported"),
        "expected daemon error in stderr, got: {stderr}",
    );
}

#[test]
fn test_pproxy_run_auth_fails() {
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            "http://:19882",
            "-r",
            "socks5://127.0.0.1:1080",
            "--auth",
            "30",
        ],
        5000,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(0),
        "expected non-zero exit code for --auth, got {:?}\nstderr: {stderr}",
        output.status.code(),
    );
    assert!(
        stderr.contains("auth") || stderr.contains("unsupported"),
        "expected auth/unsupported error in stderr, got: {stderr}",
    );
}

#[test]
fn test_pproxy_run_sys_fails_before_startup() {
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            "http://:19883",
            "-r",
            "socks5://127.0.0.1:1080",
            "--sys",
        ],
        5000,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(0),
        "expected non-zero exit code for --sys, got {:?}\nstderr: {stderr}",
        output.status.code(),
    );
    assert!(
        !stderr.contains("System Proxy Inspection"),
        "--sys must not run inspection in pproxy compatibility mode, got: {stderr}",
    );
    assert!(
        stderr.contains("sys") || stderr.contains("unsupported"),
        "expected sys/unsupported error in stderr, got: {stderr}",
    );
}

#[test]
fn test_pproxy_run_malformed_auth_fails() {
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            "http://:19884",
            "-r",
            "socks5://127.0.0.1:1080",
            "--auth",
            "abc",
        ],
        5000,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(0),
        "expected non-zero exit code for malformed --auth, got {:?}\nstderr: {stderr}",
        output.status.code(),
    );
    assert!(
        stderr.contains("auth") || stderr.contains("error"),
        "expected auth error in stderr, got: {stderr}",
    );
}

#[test]
fn test_pproxy_run_in_memory_config() {
    // Regression: eggress pproxy run must start from in-memory config
    // without writing temporary files. The process should start and run
    // until we kill it (proving the in-memory config worked).
    let mut child = eggress_bin()
        .args([
            "pproxy",
            "run",
            "--",
            "-l",
            "http://127.0.0.1:19893",
            "-r",
            "socks5://127.0.0.1:1080",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn eggress pproxy run");

    // Give it a moment to start
    std::thread::sleep(std::time::Duration::from_millis(2000));
    // If the process is still running, the in-memory config worked.
    // If it exited early with an error, that's a failure.
    match child.try_wait() {
        Ok(Some(status)) => {
            panic!(
                "eggress pproxy run exited early with status: {:?}",
                status.code()
            );
        }
        Ok(None) => {
            // Still running - success, kill it
            let _ = child.kill();
            let _ = child.wait();
        }
        Err(e) => panic!("failed to check process status: {e}"),
    }
}
