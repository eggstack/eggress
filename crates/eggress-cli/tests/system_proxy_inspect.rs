//! `eggress system-proxy inspect --json` must never print raw proxy
//! credentials. The inspection surface is copy/paste-able diagnostics, and
//! OS/environment proxy settings can carry `user:pass@` userinfo.
#![cfg(feature = "operations")]

use std::process::Command;

fn eggress_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_eggress"));
    cmd.env("RUST_LOG", "error");
    cmd
}

#[test]
fn system_proxy_inspect_json_contains_no_credentials() {
    let output = eggress_bin()
        .args(["system-proxy", "inspect", "--json"])
        // Credential-bearing proxy environment (the env backend reads these
        // verbatim; macOS/Linux backends fall back to them when the native
        // settings source is unavailable).
        .env("http_proxy", "http://user:pass@proxy.invalid:8080")
        .env("HTTP_PROXY", "http://user:pass@proxy.invalid:8080")
        .env("https_proxy", "http://user:pass@proxy.invalid:8443")
        .env("HTTPS_PROXY", "http://user:pass@proxy.invalid:8443")
        .env("all_proxy", "socks5://admin:secret@proxy.invalid:1080")
        .env("ALL_PROXY", "socks5://admin:secret@proxy.invalid:1080")
        .output()
        .expect("failed to run eggress");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "system-proxy inspect --json failed: {stderr}\nstdout: {stdout}"
    );

    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("inspect --json must emit valid JSON");
    assert!(
        value.get("settings").is_none(),
        "raw settings must not be serialized: {stdout}"
    );
    assert!(
        value.get("redacted_settings").is_some(),
        "redacted settings must always be emitted: {stdout}"
    );

    for secret in ["user:pass@", "admin:secret@", "pass@proxy", "secret@proxy"] {
        assert!(
            !stdout.contains(secret),
            "inspect --json leaked proxy credentials ({secret}): {stdout}"
        );
        assert!(
            !stderr.contains(secret),
            "inspect --json stderr leaked proxy credentials ({secret}): {stderr}"
        );
    }
}
