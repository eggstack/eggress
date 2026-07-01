use std::process::Command;

fn eggress_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_eggress"));
    cmd.env("RUST_LOG", "error");
    cmd
}

fn fixture_dir() -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../tests/compat/fixtures/pproxy_cli_cases");
    path
}

fn run_translate(args: &[String]) -> (String, String, i32) {
    let output = eggress_bin()
        .args(["pproxy", "translate", "--"])
        .args(args)
        .output()
        .expect("failed to run eggress");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

fn extract_warnings(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("warning: ")
                .map(|rest| rest.to_string())
        })
        .collect()
}

fn load_fixture(name: &str) -> toml::Value {
    let path = fixture_dir().join(format!("{name}.toml"));
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {}", path.display(), e));
    toml::from_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse fixture {}: {}", path.display(), e))
}

fn run_golden_test(fixture_name: &str) {
    let fixture = load_fixture(fixture_name);
    let id = fixture["id"].as_str().expect("missing id");
    let args: Vec<String> = fixture["args"]
        .as_array()
        .expect("missing args")
        .iter()
        .map(|v| v.as_str().expect("arg must be string").to_string())
        .collect();
    let expected_exit = fixture["expected_exit_code"]
        .as_integer()
        .expect("missing expected_exit_code") as i32;
    let expected_warnings: Vec<String> = fixture["expected_warnings"]
        .as_array()
        .expect("missing expected_warnings")
        .iter()
        .map(|v| v.as_str().expect("warning must be string").to_string())
        .collect();
    let must_contain: Vec<String> = fixture["toml_content_must_contain"]
        .as_array()
        .expect("missing toml_content_must_contain")
        .iter()
        .map(|v| {
            v.as_str()
                .expect("must_contain entry must be string")
                .to_string()
        })
        .collect();

    let (stdout, stderr, exit_code) = run_translate(&args);

    assert_eq!(
        exit_code, expected_exit,
        "[{id}] unexpected exit code: got {exit_code}, expected {expected_exit}\nstderr: {stderr}"
    );

    let actual_warnings = extract_warnings(&stderr);
    assert_eq!(
        actual_warnings.len(),
        expected_warnings.len(),
        "[{id}] warning count mismatch: got {:?}, expected {:?}\nstderr: {stderr}",
        actual_warnings,
        expected_warnings
    );
    for (i, (actual, expected)) in actual_warnings
        .iter()
        .zip(expected_warnings.iter())
        .enumerate()
    {
        assert_eq!(
            actual, expected,
            "[{id}] warning[{i}] mismatch: got '{actual}', expected '{expected}'"
        );
    }

    for pattern in &must_contain {
        assert!(
            stdout.contains(pattern.as_str()),
            "[{id}] TOML output missing '{pattern}'\nstdout:\n{stdout}"
        );
    }

    let (stdout2, stderr2, exit_code2) = run_translate(&args);
    assert_eq!(
        exit_code, exit_code2,
        "[{id}] non-deterministic exit code: {exit_code} vs {exit_code2}"
    );
    assert_eq!(stdout, stdout2, "[{id}] non-deterministic stdout output");
    assert_eq!(stderr, stderr2, "[{id}] non-deterministic stderr output");
}

#[test]
fn golden_socks5_http_chain() {
    run_golden_test("socks5_http_chain");
}

#[test]
fn golden_ss_listener() {
    run_golden_test("ss_listener");
}

#[test]
fn golden_standalone_udp() {
    run_golden_test("standalone_udp");
}

#[test]
fn golden_ssr_rejection() {
    run_golden_test("ssr_rejection");
}

#[test]
fn golden_ssh_unsupported() {
    run_golden_test("ssh_unsupported");
}

#[test]
fn golden_scheduler_flags() {
    run_golden_test("scheduler_flags");
}

#[test]
fn golden_auth_flags() {
    run_golden_test("auth_flags");
}

#[test]
fn golden_backward_reverse() {
    run_golden_test("backward_reverse");
}

#[test]
fn all_fixtures_are_valid_toml() {
    let dir = fixture_dir();
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to read fixture dir {}: {}", dir.display(), e));
    let mut count = 0;
    for entry in entries {
        let entry = entry.expect("failed to read dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
            let parsed: toml::Value = toml::from_str(&content)
                .unwrap_or_else(|e| panic!("invalid TOML in {}: {}", path.display(), e));
            assert!(
                parsed.get("id").is_some(),
                "fixture {} missing 'id' field",
                path.display()
            );
            assert!(
                parsed.get("args").is_some(),
                "fixture {} missing 'args' field",
                path.display()
            );
            count += 1;
        }
    }
    assert!(count > 0, "no fixture files found in {}", dir.display());
}
