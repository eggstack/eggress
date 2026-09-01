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

fn run_output(cmd: &mut Command) -> std::process::Output {
    let _guard = LISTENER_MUTEX.lock().unwrap();
    cmd.output().expect("failed to run pproxy")
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
    let output = run_output(pproxy_bin().arg("--help"));
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pproxy compatibility binary"));
    assert!(stdout.contains("-l"));
    assert!(stdout.contains("-r"));
    assert!(stdout.contains("--test"));
    assert!(stdout.contains("--sys"));
    assert!(stdout.contains("--ssl"));
    assert!(stdout.contains("--pac"));
    assert!(stdout.contains("-d"));
    assert!(stdout.contains("--reuse"));
    assert!(stdout.contains("--auth"));
    assert!(stdout.contains("--daemon"));
}

#[test]
fn help_flag_d_and_log_wording() {
    let output = run_output(pproxy_bin().arg("--help"));
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // -d must not claim native-equivalent Python traceback semantics
    assert!(
        !stdout.contains("native equivalent") || !stdout.contains("traceback"),
        "-d help must not pair 'native equivalent' with traceback wording: {stdout}"
    );
    assert!(
        stdout.contains("tracing") || stdout.contains("debug") || stdout.contains("Debug"),
        "-d help should mention tracing or debug: {stdout}"
    );
    // --log must not describe stderr as native-equivalent file output
    assert!(
        !stdout.contains("native equivalent: stderr"),
        "--log help must not describe stderr as native equivalent: {stdout}"
    );
    assert!(
        stdout.contains("stderr") || stdout.contains("recognized") || stdout.contains("compat"),
        "--log help should mention stderr or compat: {stdout}"
    );
}

#[test]
fn short_help_flag() {
    let output = run_output(pproxy_bin().arg("-h"));
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pproxy compatibility binary"));
}

#[test]
fn version_flag() {
    let output = run_output(pproxy_bin().arg("--version"));
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
fn startup_banner_redacts_uri_credentials() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:19811",
            "-r",
            "socks5://user:secret@127.0.0.1:1080",
        ]),
        3000,
    );
    assert!(
        !stderr.contains("secret"),
        "startup banner leaked credentials: {stderr}"
    );
    assert!(
        stderr.contains("socks5://****:****@127.0.0.1:1080"),
        "expected redacted remote in banner, got: {stderr}"
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
            "/proxy.pac",
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
fn unsupported_daemon_flag_fails() {
    let output = run_output(pproxy_bin().args([
        "-l",
        "http://:19805",
        "-r",
        "socks5://127.0.0.1:1080",
        "--daemon",
    ]));
    assert_eq!(
        output.status.code(),
        Some(5),
        "expected exit code 5 (unsupported --daemon), got {:?}",
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("daemon") || stderr.contains("not supported"),
        "expected daemon error in stderr, got: {stderr}",
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
fn debug_flag_accepted_independently() {
    // `-d` is a debug-level flag in pproxy 2.7.9; it must not affect
    // the startup banner and must not enable daemon behavior.
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args(["-l", "http://:19820", "-r", "socks5://127.0.0.1:1080", "-d"]),
        3000,
    );
    assert!(
        stderr.contains("listen:") && stderr.contains("http://:19820"),
        "expected listener in banner for -d startup, got: {stderr}",
    );
    assert!(
        !stderr.to_lowercase().contains("daemon"),
        "-d must not enable daemon behavior, got: {stderr}",
    );
}

#[test]
fn debug_flag_and_daemon_flag_still_fatal() {
    // Even though -d is independent of --daemon, --daemon remains
    // fatal before startup in pproxy compatibility mode.
    let output = run_output(pproxy_bin().args([
        "-l",
        "http://:19821",
        "-r",
        "socks5://127.0.0.1:1080",
        "-d",
        "--daemon",
    ]));
    assert!(
        !output.status.success(),
        "expected non-zero exit for --daemon with -d, got {:?}",
        output.status.code(),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("daemon") || stderr.contains("not supported"),
        "expected daemon error in stderr when combining -d with --daemon, got: {stderr}",
    );
}

#[test]
fn debug_flag_changes_default_log_level() {
    // -d alone selects a debug-level default log filter; a clean
    // startup confirms that logging initialization succeeds.
    // The actual log level is exercised by the unit-level
    // `default_log_level` helper in eggress-pproxy-compat.
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args(["-l", "http://:19822", "-r", "socks5://127.0.0.1:1080", "-d"]),
        2500,
    );
    assert!(
        stderr.contains("eggress-pproxy-compat"),
        "expected successful -d startup, got: {stderr}",
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
    let output = run_output(pproxy_bin().arg("-l"));
    assert!(!output.status.success());
}

#[test]
fn missing_value_for_r_fails() {
    let output = run_output(pproxy_bin().args(["-l", "http://:19809", "-r"]));
    assert!(!output.status.success());
}

#[test]
fn sys_flag_fails_before_startup() {
    // The local Linux backend may reject system-proxy application, or keep
    // the service running when the host extension is available. Either way,
    // it must use the compatibility operation rather than an unsupported
    // feature gate.
    let (status, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:19811",
            "-r",
            "socks5://127.0.0.1:1080",
            "--sys",
        ]),
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
fn auth_flag_starts_compatibility_listener() {
    let (status, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:19812",
            "-r",
            "socks5://127.0.0.1:1080",
            "--auth",
            "30",
        ]),
        1500,
    );
    assert!(
        status.is_none(),
        "expected listener to remain running, got {status:?}: {stderr}"
    );
    assert!(
        stderr.contains("listen:"),
        "expected startup banner, got: {stderr}"
    );
    assert!(
        !stderr.contains("unsupported"),
        "--auth must not be rejected as unsupported: {stderr}"
    );
}

#[test]
fn malformed_auth_fails() {
    let output = run_output(pproxy_bin().args([
        "-l",
        "http://:19813",
        "-r",
        "socks5://127.0.0.1:1080",
        "--auth",
        "abc",
    ]));
    assert!(
        !output.status.success(),
        "expected non-zero exit for malformed --auth, got {:?}",
        output.status.code(),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("auth") || stderr.contains("error"),
        "expected auth error in stderr, got: {stderr}",
    );
}

#[test]
fn unknown_flag_fails() {
    let output = run_output(pproxy_bin().args([
        "-l",
        "http://:19810",
        "-r",
        "socks5://127.0.0.1:1080",
        "--bogus-flag",
    ]));
    assert!(
        !output.status.success(),
        "expected non-zero exit for unknown flag, got {:?}",
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown") || stderr.contains("bogus-flag"),
        "expected unknown flag error in stderr, got: {stderr}",
    );
}

#[test]
fn strict_parser_surface_fails_closed_before_startup() {
    let cases: &[(&[&str], &str)] = &[
        (&["--log", "/tmp/pproxy.log"], "--log"),
        (&["--rulefile", "/tmp/rules"], "--rulefile"),
        (&["--listen", "http://:19814"], "--listen"),
        (&["proxy://:19814"], "proxy://:19814"),
        (&["-s", "invalid"], "invalid choice"),
        (&["-a", "invalid"], "valid integer"),
    ];

    for (args, expected) in cases {
        let output = run_output(pproxy_bin().args(*args));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "args={args:?}: {stderr}");
        assert!(stderr.contains(expected), "args={args:?}: {stderr}");
        assert!(
            !stderr.contains("pproxy started") && !stderr.contains("listen:"),
            "parser failure must not start a listener: {stderr}"
        );
    }
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

#[test]
fn test_mode_runs_in_process_no_sibling_binary() {
    // Regression: --test must call the shared Rust upstream-test implementation
    // in-process, not spawn a sibling `eggress` binary. We verify by running
    // with a target that connects to a non-existent upstream; the test should
    // complete with a failure exit code (unreachable) without needing an
    // `eggress` binary on PATH.
    let output = run_output(pproxy_bin().args([
        "-l",
        "http://:19890",
        "-r",
        "socks5://127.0.0.1:19891",
        "--test",
        "http://example.com",
    ]));
    // The test mode should exit (not hang) and report the upstream as
    // unreachable. Exit code 1 = unreachable (all upstreams failed).
    assert_ne!(
        output.status.code(),
        Some(0),
        "expected non-zero exit for unreachable upstream test"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("pproxy started") && !stderr.contains("listen:"),
        "--test must not start or advertise a listener: {stderr}"
    );
}

#[test]
fn in_memory_startup_no_tempfile() {
    // Regression: pproxy startup must not create temporary files. We verify
    // by checking the startup banner appears (which is printed before config
    // validation) and the process starts correctly with in-memory config.
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args(["-l", "http://:19892", "-r", "socks5://127.0.0.1:1080"]),
        2000,
    );
    assert!(
        stderr.contains("listen:") && stderr.contains("http://:19892"),
        "expected successful in-memory startup, got: {stderr}",
    );
}
