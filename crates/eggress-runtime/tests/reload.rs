use std::io::Write;

use tempfile::NamedTempFile;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[test]
fn valid_reload_changes_routing() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Applied {
            generation,
            upstreams,
        } => {
            assert_eq!(generation, 1);
            assert_eq!(upstreams, 0);
        }
        other => panic!("expected Applied, got {:?}", other),
    }
}

#[test]
fn invalid_reload_preserves_old_routing() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    let gen_before = sup.state().generation();

    // Write an invalid config to the same file path
    let invalid = "this is not valid toml {{{";
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path1)
            .unwrap();
        f.write_all(invalid.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Failed { error } => {
            assert!(
                error.contains("config") || error.contains("load"),
                "error should mention config issue: {}",
                error
            );
        }
        other => panic!("expected Failed for invalid config, got {:?}", other),
    }

    let gen_after = sup.state().generation();
    assert_eq!(
        gen_before, gen_after,
        "generation should not change on failed reload"
    );
}

#[test]
fn admin_generation_increments_on_reload() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    assert_eq!(sup.state().generation(), 0);

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Applied { generation, .. } => {
            assert_eq!(generation, 1);
            assert_eq!(sup.state().generation(), 1);
        }
        other => panic!("expected Applied, got {:?}", other),
    }

    let result2 = sup.reload_config();
    match result2 {
        eggress_runtime::supervisor::ReloadResult::Applied { generation, .. } => {
            assert_eq!(generation, 2);
            assert_eq!(sup.state().generation(), 2);
        }
        other => panic!("expected Applied, got {:?}", other),
    }
}

#[test]
fn unsupported_topology_change_is_rejected() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
    let f1 = write_config(config1);
    let f2 = write_config(config2);
    let path1 = f1.path().to_str().unwrap();
    let path2 = f2.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    // Write config2 content to the path that reload_config reads
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path1)
            .unwrap();
        let content = std::fs::read_to_string(path2).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Rejected { reason } => {
            assert!(
                reason.contains("listener count") || reason.contains("restart required"),
                "reason should mention topology change: {}",
                reason
            );
        }
        other => panic!("expected Rejected, got {:?}", other),
    }
}

#[test]
fn reload_rejects_listener_name_change() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "http-changed"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let f2 = write_config(config2);
    let path1 = f1.path().to_str().unwrap();
    let path2 = f2.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path1)
            .unwrap();
        let content = std::fs::read_to_string(path2).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Rejected { reason } => {
            assert!(
                reason.contains("name changed"),
                "reason should mention name change: {}",
                reason
            );
        }
        other => panic!("expected Rejected for name change, got {:?}", other),
    }
}

#[test]
fn reload_rejects_bind_address_change() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:9090"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let f2 = write_config(config2);
    let path1 = f1.path().to_str().unwrap();
    let path2 = f2.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path1)
            .unwrap();
        let content = std::fs::read_to_string(path2).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Rejected { reason } => {
            assert!(
                reason.contains("bind"),
                "reason should mention bind change: {}",
                reason
            );
        }
        other => panic!("expected Rejected for bind change, got {:?}", other),
    }
}

#[test]
fn reload_rejects_removing_enabled_transparent_configuration() {
    let old = eggress_config::validate_and_compile_toml(
        r#"
version = 1

[[listeners]]
name = "redir-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.transparent]
enabled = true
protocol = "redir"
"#,
    )
    .unwrap();
    let new = eggress_config::validate_and_compile_toml(
        r#"
version = 1

[[listeners]]
name = "redir-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#,
    )
    .unwrap();

    let result = eggress_runtime::classify_reload_config(
        &old.listeners,
        &old.timeouts,
        old.admin.as_ref(),
        &new,
    );
    let error = result.expect_err("enabled transparent removal must require restart");
    assert!(error.contains("transparent") && error.contains("restart required"));
}

// ---------------------------------------------------------------------------
// Phase 1 correctness: startup-captured listener fields are restart-required.
// The running accept loops clone these values from startup-prepared listener
// state, so accepting them on reload would publish a snapshot generation the
// data plane never adopts.
// ---------------------------------------------------------------------------

fn classify_old_new(old_toml: &str, new_toml: &str) -> Result<(), String> {
    let old = eggress_config::validate_and_compile_toml(old_toml).expect("old config must compile");
    let new = eggress_config::validate_and_compile_toml(new_toml).expect("new config must compile");
    eggress_runtime::classify_reload_config(&old.listeners, &old.timeouts, old.admin.as_ref(), &new)
}

fn expect_rejected(old_toml: &str, new_toml: &str, fragment: &str) {
    let error = match classify_old_new(old_toml, new_toml) {
        Err(reason) => reason,
        Ok(()) => panic!("expected rejection containing '{fragment}'"),
    };
    assert!(
        error.contains(fragment) && error.contains("restart required"),
        "rejection should mention '{fragment}' and restart: {error}"
    );
}

const BASE_LISTENER: &str = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;

#[test]
fn reload_rejects_protocol_change() {
    let new = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http", "socks5"]
"#;
    expect_rejected(BASE_LISTENER, new, "protocols");
}

#[test]
fn reload_rejects_auth_add_remove_and_material_change() {
    let authed = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.auth]
type = "password"
username = "admin"
password = "s3cret"
"#;
    expect_rejected(BASE_LISTENER, authed, "auth");
    expect_rejected(authed, BASE_LISTENER, "auth");

    let authed_other_user = authed.replace("admin", "other");
    expect_rejected(authed, &authed_other_user, "auth");

    let authed_other_pass = authed.replace("s3cret", "different");
    expect_rejected(authed, &authed_other_pass, "auth");
}

#[test]
fn reload_rejects_tls_presence_and_material_change() {
    let cert_params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let cert_der = cert_params.self_signed(&key_pair).unwrap();
    let cert_file = write_config(&cert_der.pem());
    let key_file = write_config(&key_pair.serialize_pem());
    let cert_path = cert_file.path().to_str().unwrap().replace('\\', "/");
    let key_path = key_file.path().to_str().unwrap().replace('\\', "/");

    let tls_listener = format!(
        r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.tls]
cert = "{cert_path}"
key = "{key_path}"
"#
    );
    expect_rejected(BASE_LISTENER, &tls_listener, "TLS");

    // Different key material must also require a restart, not just presence.
    let cert_params2 = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    let key_pair2 = rcgen::KeyPair::generate().unwrap();
    let cert_der2 = cert_params2.self_signed(&key_pair2).unwrap();
    let cert_file2 = write_config(&cert_der2.pem());
    let key_file2 = write_config(&key_pair2.serialize_pem());
    let tls_listener2 = format!(
        r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.tls]
cert = "{}"
key = "{}"
"#,
        cert_file2.path().to_str().unwrap().replace('\\', "/"),
        key_file2.path().to_str().unwrap().replace('\\', "/")
    );
    expect_rejected(&tls_listener, &tls_listener2, "TLS");
}

#[test]
fn reload_rejects_shadowsocks_and_trojan_changes() {
    let ss = r#"
version = 1

[[listeners]]
name = "ss-in"
bind = "127.0.0.1:0"
protocols = ["shadowsocks"]

[listeners.shadowsocks]
method = "aes-256-gcm"
password = "ss-password"
"#;
    let ss_base = r#"
version = 1

[[listeners]]
name = "ss-in"
bind = "127.0.0.1:0"
protocols = ["shadowsocks"]
"#;
    // Presence change requires restart.
    expect_rejected(ss_base, ss, "shadowsocks");
    // Method change requires restart.
    expect_rejected(
        ss,
        &ss.replace("aes-256-gcm", "chacha20-poly1305"),
        "shadowsocks",
    );
    // Password change requires restart.
    expect_rejected(
        ss,
        &ss.replace("ss-password", "other-password"),
        "shadowsocks",
    );
}

#[test]
fn reload_rejects_connection_behavior_changes() {
    let limited = BASE_LISTENER.replace(
        "protocols = [\"http\"]",
        "protocols = [\"http\"]\nconnection_limit = 100",
    );
    expect_rejected(BASE_LISTENER, &limited, "connection_limit");

    let with_target = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
fixed_target = "127.0.0.1:8080"
"#;
    expect_rejected(BASE_LISTENER, with_target, "fixed_target");

    let with_bind = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
local_bind = "127.0.0.1"
"#;
    expect_rejected(BASE_LISTENER, with_bind, "local_bind");

    let with_reuse = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
reuse_port = true
"#;
    expect_rejected(BASE_LISTENER, with_reuse, "restart required");
}

#[test]
fn reload_rejects_startup_captured_udp_changes() {
    let udp_base = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.udp]
enabled = true
"#;
    // Limit change requires restart (limits feed startup-captured relay state).
    let udp_limits = udp_base.replace("enabled = true", "enabled = true\nmax_associations = 16");
    expect_rejected(udp_base, &udp_limits, "UDP");
    // Advertise change requires restart.
    let udp_advertise =
        udp_base.replace("enabled = true", "enabled = true\nadvertise = \"10.0.0.1\"");
    expect_rejected(udp_base, &udp_advertise, "UDP");
    // Idle-timeout change requires restart.
    let udp_timeout =
        udp_base.replace("enabled = true", "enabled = true\nidle_timeout = \"200ms\"");
    expect_rejected(udp_base, &udp_timeout, "UDP");
    // Removal still requires restart.
    let no_udp = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
    expect_rejected(udp_base, no_udp, "UDP");
}

#[test]
fn reload_rejects_transparent_protocol_and_unix_option_changes() {
    let redir = r#"
version = 1

[[listeners]]
name = "redir-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.transparent]
enabled = true
protocol = "redir"
"#;
    let pf = redir.replace("protocol = \"redir\"", "protocol = \"pf\"");
    expect_rejected(redir, &pf, "transparent");

    let unix_base = r#"
version = 1

[[listeners]]
name = "unix-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.unix]
path = "/tmp/eggress-reload-test.sock"
"#;
    let unix_mode = unix_base.replace(
        "path = \"/tmp/eggress-reload-test.sock\"",
        "path = \"/tmp/eggress-reload-test.sock\"\nmode = 438",
    );
    expect_rejected(unix_base, &unix_mode, "unix");
}

#[test]
fn reload_accepts_routing_only_change() {
    let with_rule = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[rules]]
id = "block-example"
host_exact = "blocked.example.com"
reject = "blocked"
"#;
    classify_old_new(BASE_LISTENER, with_rule)
        .expect("routing-only change must remain hot-reloadable");
}

#[test]
fn reload_accepts_upstream_only_change() {
    let base = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "up1"
uri = "http://127.0.0.1:8080"
"#;
    let added = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "up1"
uri = "http://127.0.0.1:8080"

[[upstreams]]
id = "up2"
uri = "http://127.0.0.1:8081"
"#;
    classify_old_new(base, added).expect("upstream-only change must remain hot-reloadable");
}

// ---------------------------------------------------------------------------
// End-to-end reload behavior: rejected listener changes leave generation and
// data-plane behavior unchanged; accepted routing changes are observed by new
// connections.
// ---------------------------------------------------------------------------

struct ShutdownGuard {
    token: tokio_util::sync::CancellationToken,
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        self.token.cancel();
    }
}

async fn wait_ready(state: &eggress_runtime::RuntimeState) {
    for _ in 0..100 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("timeout waiting for readiness");
}

async fn socks5_no_auth_greeting(addr: std::net::SocketAddr) -> [u8; 2] {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    resp
}

/// End-to-end rejected reload: the serving supervisor receives SIGHUP for a
/// listener-auth change, rejects it, and keeps serving with startup behavior.
///
/// Note: SIGHUP delivery is process-wide, so a cross-delivered reload from a
/// sibling test can only ever hit this supervisor while its file holds the
/// rejected config — every such attempt is itself rejected and cannot advance
/// the generation.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejected_auth_reload_preserves_data_plane() {
    use std::io::Write as _;
    use std::time::Duration;

    let config1 = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
    let f = write_config(config1);
    let path = f.path().to_str().unwrap().to_string();
    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let _guard = ShutdownGuard {
        token: token.clone(),
    };
    let run_handle = tokio::task::spawn_blocking(move || sup.run());
    wait_ready(&state).await;
    let listener_addr = { state.listener_addrs.lock().unwrap()[0].unwrap() };

    // Baseline: no-auth handshake succeeds.
    assert_eq!(socks5_no_auth_greeting(listener_addr).await, [0x05, 0x00]);
    let gen_before = state.generation();

    // Attempt to add listener auth via reload: must be rejected.
    let config2 = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "admin"
password = "s3cret"
"#;
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        f.write_all(config2.as_bytes()).unwrap();
        f.flush().unwrap();
        f.sync_all().unwrap();
    }
    std::process::Command::new("kill")
        .arg("-HUP")
        .arg(std::process::id().to_string())
        .output()
        .ok();
    // Give the signal loop a chance to attempt the reload, then verify the
    // rejected change never became active.
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(
        state.generation(),
        gen_before,
        "rejected listener change must not advance the generation"
    );
    assert_eq!(
        socks5_no_auth_greeting(listener_addr).await,
        [0x05, 0x00],
        "rejected reload must leave active listener behavior unchanged"
    );

    token.cancel();
    run_handle.await.ok();
}

/// End-to-end accepted reload: a routing-only change delivered via SIGHUP is
/// applied, and a new connection observes the new route.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepted_routing_reload_observed_by_new_connection() {
    use std::io::Write as _;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Local echo target so the direct route provably works offline.
    let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = echo.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match echo.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if stream.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });

    let config1 = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
    let f = write_config(config1);
    let path = f.path().to_str().unwrap().to_string();
    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let _guard = ShutdownGuard {
        token: token.clone(),
    };
    let run_handle = tokio::task::spawn_blocking(move || sup.run());
    wait_ready(&state).await;
    let listener_addr = { state.listener_addrs.lock().unwrap()[0].unwrap() };

    async fn socks5_connect_reply(
        listener: std::net::SocketAddr,
        target: std::net::SocketAddr,
    ) -> [u8; 2] {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(listener).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut resp = [0u8; 2];
        stream.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp, [0x05, 0x00]);
        let ip = match target.ip() {
            std::net::IpAddr::V4(v) => v.octets(),
            _ => panic!("test uses IPv4"),
        };
        let mut req = vec![0x05, 0x01, 0x00, 0x01];
        req.extend_from_slice(&ip);
        req.extend_from_slice(&target.port().to_be_bytes());
        stream.write_all(&req).await.unwrap();
        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        [reply[0], reply[1]]
    }

    // Baseline: direct route reaches the echo target.
    assert_eq!(
        socks5_connect_reply(listener_addr, echo_addr).await,
        [0x05, 0x00]
    );

    // Routing-only change: reject the echo port. Listeners are untouched.
    let config2 = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[rules]]
id = "block-echo"
destination_port = {}
reject = "blocked"
"#,
        echo_addr.port()
    );
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        f.write_all(config2.as_bytes()).unwrap();
        f.flush().unwrap();
        f.sync_all().unwrap();
    }
    std::process::Command::new("kill")
        .arg("-HUP")
        .arg(std::process::id().to_string())
        .output()
        .ok();
    // A cross-delivered HUP from a sibling test can only advance the
    // generation with this same routing file, so `>= 1` plus the behavior
    // check below is the robust assertion.
    let gen_before = state.generation();
    for _ in 0..100 {
        if state.generation() > gen_before {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        state.generation() > gen_before,
        "routing-only reload must advance the generation"
    );

    // A new connection must observe the reloaded reject route.
    let reply = socks5_connect_reply(listener_addr, echo_addr).await;
    assert_ne!(
        reply,
        [0x05, 0x00],
        "new connection must observe the reloaded reject route, got {reply:?}"
    );

    token.cancel();
    run_handle.await.ok();
}
