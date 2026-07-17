use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

static LISTENER_MUTEX: Mutex<()> = Mutex::new(());

fn pproxy_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pproxy"));
    cmd.env("RUST_LOG", "error");
    cmd
}

/// Spawn pproxy, capture stderr via temp file, kill after timeout_ms.
/// Holds LISTENER_MUTEX to prevent port/resource conflicts under parallel execution.
fn spawn_and_collect(cmd: &mut Command, timeout_ms: u64) -> (Option<i32>, String) {
    let _guard = LISTENER_MUTEX.lock().unwrap();

    let tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    let stderr_path = tmp.path().to_path_buf();

    let stderr_file = std::fs::File::create(&stderr_path).expect("failed to create stderr file");
    let mut child = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(stderr_file))
        .spawn()
        .expect("failed to spawn pproxy");

    thread::sleep(Duration::from_millis(timeout_ms));
    let _ = child.kill();
    let status = child.wait().ok().and_then(|s| s.code());

    let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
    (status, stderr)
}

#[test]
fn help_flag() {
    let output = pproxy_bin()
        .arg("--help")
        .output()
        .expect("failed to run pproxy");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pproxy compatibility binary"));
    assert!(stdout.contains("-l"));
    assert!(stdout.contains("-r"));
    assert!(stdout.contains("--test"));
    assert!(stdout.contains("--sys"));
    assert!(stdout.contains("--ssl"));
    assert!(stdout.contains("--pac"));
}

#[test]
fn short_help_flag() {
    let output = pproxy_bin()
        .arg("-h")
        .output()
        .expect("failed to run pproxy");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pproxy compatibility binary"));
}

#[test]
fn version_flag() {
    let output = pproxy_bin()
        .arg("--version")
        .output()
        .expect("failed to run pproxy");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("eggress-pproxy-compat"));
}

#[test]
fn no_args_starts_with_default_listener() {
    let (_, stderr) = spawn_and_collect(&mut pproxy_bin(), 3000);
    assert!(
        stderr.contains("eggress-pproxy-compat"),
        "expected version banner for default startup, got: {stderr}",
    );
    assert!(
        stderr.contains("listen:"),
        "expected listener line in default startup banner, got: {stderr}",
    );
}

#[test]
fn startup_banner_shows_version_and_listeners() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args(["-l", "http://:19800", "-r", "socks5://127.0.0.1:1080"]),
        3000,
    );
    assert!(
        stderr.contains("eggress-pproxy-compat"),
        "expected version in banner, got: {stderr}",
    );
    assert!(
        stderr.contains("listen:") && stderr.contains("http://:19800"),
        "expected listener in banner, got: {stderr}",
    );
    assert!(
        stderr.contains("remote:") && stderr.contains("socks5://127.0.0.1:1080"),
        "expected remote in banner, got: {stderr}",
    );
}

#[test]
fn startup_banner_shows_tls_when_ssl() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:19801",
            "-r",
            "socks5://127.0.0.1:1080",
            "--ssl",
            "cert.pem,key.pem",
        ]),
        3000,
    );
    assert!(
        stderr.contains("tls:      enabled"),
        "expected TLS enabled in banner, got: {stderr}",
    );
}

#[test]
fn startup_banner_shows_pac() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:19802",
            "-r",
            "socks5://127.0.0.1:1080",
            "--pac",
        ]),
        3000,
    );
    assert!(
        stderr.contains("pac:      enabled"),
        "expected PAC enabled in banner, got: {stderr}",
    );
}

#[test]
fn startup_banner_shows_udp() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:19803",
            "-r",
            "socks5://127.0.0.1:1080",
            "-ul",
            "socks5://:19804",
        ]),
        3000,
    );
    assert!(
        stderr.contains("udp:"),
        "expected UDP in banner, got: {stderr}",
    );
}

#[test]
fn unsupported_daemon_flag_warns() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:19805",
            "-r",
            "socks5://127.0.0.1:1080",
            "--daemon",
        ]),
        3000,
    );
    assert!(
        stderr.contains("pproxy: warning:") || stderr.contains("daemon"),
        "expected daemon warning in stderr, got: {stderr}",
    );
}

#[test]
fn verbose_flag_accepted() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args(["-l", "http://:19806", "-r", "socks5://127.0.0.1:1080", "-v"]),
        3000,
    );
    assert!(
        stderr.contains("listen:"),
        "expected listener in banner for verbose startup, got: {stderr}",
    );
}

#[test]
fn verbose_double_flag_accepted() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:19807",
            "-r",
            "socks5://127.0.0.1:1080",
            "-vv",
        ]),
        3000,
    );
    assert!(
        stderr.contains("listen:"),
        "expected listener in banner for -vv startup, got: {stderr}",
    );
}

#[test]
fn verbose_triple_flag_accepted() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:19808",
            "-r",
            "socks5://127.0.0.1:1080",
            "-vvv",
        ]),
        3000,
    );
    assert!(
        stderr.contains("listen:"),
        "expected listener in banner for -vvv startup, got: {stderr}",
    );
}

#[test]
fn unsupported_ssh_scheme_fails() {
    let (code, stderr) = spawn_and_collect_inner(pproxy_bin().args(["-l", "ssh://host:22"]), 2000);
    assert!(
        stderr.contains("unsupported") || stderr.contains("not supported") || code != Some(0),
        "expected unsupported diagnostic for SSH scheme, got: code={code:?}, stderr={stderr}",
    );
}

#[test]
fn missing_value_for_l_fails() {
    let output = pproxy_bin()
        .arg("-l")
        .output()
        .expect("failed to run pproxy");
    assert!(!output.status.success());
}

#[test]
fn missing_value_for_r_fails() {
    let output = pproxy_bin()
        .args(["-l", "http://:19809", "-r"])
        .output()
        .expect("failed to run pproxy");
    assert!(!output.status.success());
}

#[test]
fn unknown_flag_warns() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:19810",
            "-r",
            "socks5://127.0.0.1:1080",
            "--bogus-flag",
        ]),
        3000,
    );
    assert!(
        stderr.contains("pproxy: note:") || stderr.contains("bogus-flag"),
        "expected unknown flag warning, got: {stderr}",
    );
}

/// Inner helper that does NOT acquire the mutex (caller is responsible).
fn spawn_and_collect_inner(cmd: &mut Command, timeout_ms: u64) -> (Option<i32>, String) {
    let tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    let stderr_path = tmp.path().to_path_buf();

    let stderr_file = std::fs::File::create(&stderr_path).expect("failed to create stderr file");
    let mut child = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(stderr_file))
        .spawn()
        .expect("failed to spawn pproxy");

    thread::sleep(Duration::from_millis(timeout_ms));
    let _ = child.kill();
    let status = child.wait().ok().and_then(|s| s.code());

    let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
    (status, stderr)
}
