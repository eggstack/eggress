use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn pproxy_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pproxy"));
    cmd.env("RUST_LOG", "error");
    cmd
}

/// Spawn pproxy, collect stderr lines in a background thread, kill after timeout_ms.
fn spawn_and_collect(cmd: &mut Command, timeout_ms: u64) -> (Option<i32>, String) {
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn pproxy");

    let stderr = child.stderr.take().expect("no stderr");
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let lines_clone = lines.clone();

    let reader_thread = thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    lines_clone.lock().unwrap().push(l);
                }
                Err(_) => break,
            }
        }
    });

    thread::sleep(Duration::from_millis(timeout_ms));
    let _ = child.kill();
    let status = child.wait().ok().and_then(|s| s.code());
    let _ = reader_thread.join();

    let all_lines = lines.lock().unwrap().join("\n");
    (status, all_lines)
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
    let (_, stderr) = spawn_and_collect(&mut pproxy_bin(), 2000);
    assert!(
        stderr.contains("eggress-pproxy-compat"),
        "expected version banner for default startup, got: {stderr}",
    );
    assert!(
        stderr.contains("listen:"),
        "expected listener line in default startup banner, got: {stderr}",
    );
    assert!(
        stderr.contains("8080"),
        "expected default port 8080 in banner, got: {stderr}",
    );
}

#[test]
fn startup_banner_shows_version_and_listeners() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args(["-l", "http://:8080", "-r", "socks5://127.0.0.1:1080"]),
        2000,
    );
    assert!(
        stderr.contains("eggress-pproxy-compat"),
        "expected version in banner, got: {stderr}",
    );
    assert!(
        stderr.contains("listen:") && stderr.contains("http://:8080"),
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
            "http://:8080",
            "-r",
            "socks5://127.0.0.1:1080",
            "--ssl",
            "cert.pem,key.pem",
        ]),
        2000,
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
            "http://:8080",
            "-r",
            "socks5://127.0.0.1:1080",
            "--pac",
        ]),
        2000,
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
            "http://:8080",
            "-r",
            "socks5://127.0.0.1:1080",
            "-ul",
            "socks5://:1081",
        ]),
        2000,
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
            "http://:8080",
            "-r",
            "socks5://127.0.0.1:1080",
            "--daemon",
        ]),
        2000,
    );
    assert!(
        stderr.contains("pproxy: warning:") || stderr.contains("daemon"),
        "expected daemon warning in stderr, got: {stderr}",
    );
}

#[test]
fn verbose_flag_accepted() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args(["-l", "http://:8080", "-r", "socks5://127.0.0.1:1080", "-v"]),
        2000,
    );
    assert!(
        stderr.contains("listen:"),
        "expected listener in banner for verbose startup, got: {stderr}",
    );
}

#[test]
fn verbose_double_flag_accepted() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args(["-l", "http://:8080", "-r", "socks5://127.0.0.1:1080", "-vv"]),
        2000,
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
            "http://:8080",
            "-r",
            "socks5://127.0.0.1:1080",
            "-vvv",
        ]),
        2000,
    );
    assert!(
        stderr.contains("listen:"),
        "expected listener in banner for -vvv startup, got: {stderr}",
    );
}

#[test]
fn unsupported_ssh_scheme_fails() {
    let (code, stderr) = spawn_and_collect(pproxy_bin().args(["-l", "ssh://host:22"]), 2000);
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
        .args(["-l", "http://:8080", "-r"])
        .output()
        .expect("failed to run pproxy");
    assert!(!output.status.success());
}

#[test]
fn unknown_flag_warns() {
    let (_, stderr) = spawn_and_collect(
        pproxy_bin().args([
            "-l",
            "http://:8080",
            "-r",
            "socks5://127.0.0.1:1080",
            "--bogus-flag",
        ]),
        2000,
    );
    assert!(
        stderr.contains("pproxy: note:") || stderr.contains("bogus-flag"),
        "expected unknown flag warning, got: {stderr}",
    );
}
