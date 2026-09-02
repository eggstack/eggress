use std::process::Command;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;

static LISTENER_MUTEX: Mutex<()> = Mutex::new(());
static NEXT_LISTENER_PORT: AtomicU16 = AtomicU16::new(20_000);

fn next_listener_port() -> u16 {
    NEXT_LISTENER_PORT.fetch_add(1, Ordering::Relaxed)
}

fn eggress_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_eggress"));
    cmd.env("RUST_LOG", "error");
    cmd
}

#[allow(dead_code)]
struct ProcessGuard(std::process::Child);

#[allow(dead_code)]
impl ProcessGuard {
    fn new(child: std::process::Child) -> Self {
        Self(child)
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_with_timeout(args: &[&str], timeout_ms: u64) -> std::process::Output {
    let _guard = LISTENER_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
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

fn run_and_kill(args: &[&str], timeout_ms: u64) -> (Option<i32>, String) {
    let _guard = LISTENER_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let mut child = eggress_bin()
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn eggress");

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stderr = child.stderr.take().expect("stderr pipe missing");
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut stderr, &mut bytes).unwrap();
                return (status.code(), String::from_utf8_lossy(&bytes).into_owned());
            }
            Ok(None) if start.elapsed().as_millis() <= timeout_ms as u128 => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let status = child.wait().expect("failed to wait for killed process");
                let mut stderr = child.stderr.take().expect("stderr pipe missing");
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut stderr, &mut bytes).unwrap();
                return (status.code(), String::from_utf8_lossy(&bytes).into_owned());
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
fn test_pproxy_run_version_uses_compatibility_action() {
    let output = run_with_timeout(&["pproxy", "run", "--", "--version"], 5000);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("eggress-pproxy-compat {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("started"));
}

#[test]
fn test_pproxy_run_help_uses_compatibility_action() {
    let output = run_with_timeout(&["pproxy", "run", "--", "--help"], 5000);
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("pproxy compatibility binary"));
}

#[test]
fn test_pproxy_run_bind_failure() {
    let port = next_listener_port();
    let listener = format!("socks5://127.0.0.1:{port}");
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            listener.as_str(),
            "-l",
            listener.as_str(),
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
    let listener = format!("http://:{}", next_listener_port());
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            listener.as_str(),
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
    let listener = format!("http://:{}", next_listener_port());
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            listener.as_str(),
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
fn test_pproxy_run_auth_starts_listener() {
    let listener = format!("http://:{}", next_listener_port());
    let (status, stderr) = run_and_kill(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            listener.as_str(),
            "-r",
            "socks5://127.0.0.1:1080",
            "--auth",
            "30",
        ],
        1500,
    );
    assert!(
        status.is_none(),
        "expected listener to remain running with --auth, got {status:?}\nstderr: {stderr}"
    );
    assert!(
        (stderr.contains("listen:") || stderr.contains("auth-timeout"))
            && !stderr.contains("unsupported"),
        "expected supported auth startup in stderr, got: {stderr}",
    );
}

#[test]
fn test_pproxy_run_sys_uses_compatibility_operation() {
    let listener = format!("http://:{}", next_listener_port());
    let (status, stderr) = run_and_kill(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            listener.as_str(),
            "-r",
            "socks5://127.0.0.1:1080",
            "--sys",
        ],
        1500,
    );
    assert!(
        stderr.contains("listen:") || stderr.contains("sys") || stderr.contains("proxy"),
        "expected system-proxy startup or operation output, got status {status:?}: {stderr}",
    );
    assert!(
        !stderr.contains("unsupported"),
        "--sys must not be rejected as unsupported: {stderr}",
    );
}

#[test]
fn test_pproxy_run_malformed_auth_fails() {
    let listener = format!("http://:{}", next_listener_port());
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            listener.as_str(),
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
    let _guard = LISTENER_MUTEX.lock().unwrap();
    let port = next_listener_port();
    let listener = format!("http://127.0.0.1:{port}");
    let mut child = eggress_bin()
        .args([
            "pproxy",
            "run",
            "--",
            "-l",
            listener.as_str(),
            "-r",
            "socks5://127.0.0.1:1080",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn eggress pproxy run");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                panic!(
                    "eggress pproxy run exited before listener readiness with status: {:?}",
                    status.code()
                );
            }
            Ok(None) => {}
            Err(e) => panic!("failed to check process status: {e}"),
        }

        if std::net::TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            std::time::Duration::from_millis(50),
        )
        .is_ok()
        {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "eggress pproxy run did not become ready before the deadline"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[test]
fn test_pproxy_run_unsupported_exit_code_matches_standalone() {
    // Both entry points must return exit code 5 (EXIT_UNSUPPORTED_FEATURE)
    // for the same unsupported feature.
    let listener = format!("http://:{}", next_listener_port());
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            listener.as_str(),
            "-r",
            "socks5://127.0.0.1:1080",
            "--daemon",
        ],
        5000,
    );
    assert_eq!(
        output.status.code(),
        Some(5),
        "eggress pproxy run should exit 5 for unsupported --daemon, got {:?}",
        output.status.code(),
    );
}

#[test]
fn test_pproxy_run_test_mode_in_process() {
    // The nested compatibility entry point must use the same in-process
    // upstream-test implementation as standalone pproxy --test.
    let listener = format!("http://:{}", next_listener_port());
    let upstream = format!("socks5://127.0.0.1:{}", next_listener_port());
    let output = run_with_timeout(
        &[
            "pproxy",
            "run",
            "--",
            "-l",
            listener.as_str(),
            "-r",
            upstream.as_str(),
            "--test",
            "http://example.com/eggress-test-target",
        ],
        5000,
    );
    assert_ne!(
        output.status.code(),
        Some(0),
        "expected unreachable upstream test to fail, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}
