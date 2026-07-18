use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tempfile::NamedTempFile;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

async fn http_get(addr: &str, path: &str) -> (u16, String) {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::flush(&mut stream).await.unwrap();

    let mut response = Vec::new();
    loop {
        let mut buf = [0u8; 4096];
        match tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    let response = String::from_utf8_lossy(&response);
    let status_line = response.lines().next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let body = response.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}

struct AutoShutdown(tokio_util::sync::CancellationToken);
impl Drop for AutoShutdown {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[tokio::test]
async fn upstream_appears_in_status() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "upstream1"
uri = "http://127.0.0.1:1"

[[upstream_groups]]
id = "main"
scheduler = "round-robin"
members = ["upstream1"]
fallback = "reject"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let token = sup.shutdown_token();
    let _shutdown = AutoShutdown(token.clone());
    let state = sup.state().clone();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        state.readiness.load(Ordering::Relaxed),
        "service should be ready"
    );

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound");
    let admin_str = admin_addr.to_string();

    let (status, body) = http_get(&admin_str, "/-/upstreams").await;
    assert_eq!(status, 200);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    let groups = json.as_array().unwrap();
    assert!(!groups.is_empty(), "should have at least one group");
    let members = groups[0]["members"].as_array().unwrap();
    assert!(!members.is_empty(), "group should have at least one member");
    assert_eq!(members[0]["id"], "upstream1");

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn health_probe_updates_upstream_status() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "upstream1"
uri = "http://127.0.0.1:1"

[upstreams.health]
mode = "tcp_connect"
interval = "500ms"
timeout = "200ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "main"
scheduler = "round-robin"
members = ["upstream1"]
fallback = "reject"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let token = sup.shutdown_token();
    let _shutdown = AutoShutdown(token.clone());
    let state = sup.state().clone();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        state.readiness.load(Ordering::Relaxed),
        "service should be ready"
    );

    tokio::time::sleep(Duration::from_secs(2)).await;

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound");
    let admin_str = admin_addr.to_string();

    let (status, body) = http_get(&admin_str, "/-/upstreams").await;
    assert_eq!(status, 200);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    let groups = json.as_array().unwrap();
    assert!(!groups.is_empty(), "should have at least one group");
    let members = groups[0]["members"].as_array().unwrap();
    assert!(!members.is_empty(), "group should have at least one member");
    assert_eq!(members[0]["id"], "upstream1");

    token.cancel();
    jh.await.ok();
}

#[cfg(unix)]
#[tokio::test]
async fn reload_preserves_service_running() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "upstream1"
uri = "http://127.0.0.1:1"

[[upstream_groups]]
id = "main"
scheduler = "round-robin"
members = ["upstream1"]
fallback = "reject"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let gen_before = state.generation();
    assert_eq!(gen_before, 0);

    let token = sup.shutdown_token();
    let _shutdown = AutoShutdown(token.clone());
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed));

    std::process::Command::new("kill")
        .arg("-HUP")
        .arg(std::process::id().to_string())
        .output()
        .ok();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let gen_after = state.generation();
    assert!(
        gen_after > gen_before,
        "generation should increment on reload"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn health_affects_routing() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "upstream1"
uri = "http://127.0.0.1:1"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "main"
scheduler = "round-robin"
members = ["upstream1"]
fallback = "reject"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let token = sup.shutdown_token();
    let _shutdown = AutoShutdown(token.clone());
    let state = sup.state().clone();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        state.readiness.load(Ordering::Relaxed),
        "service should be ready"
    );

    // Wait for health probes to fail (127.0.0.1:1 is unreachable)
    tokio::time::sleep(Duration::from_secs(1)).await;

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound");
    let admin_str = admin_addr.to_string();

    // Verify upstream is unhealthy and not eligible
    let (status, body) = http_get(&admin_str, "/-/upstreams").await;
    assert_eq!(status, 200);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let groups = json.as_array().unwrap();
    let members = groups[0]["members"].as_array().unwrap();
    assert_eq!(members[0]["id"], "upstream1");
    assert_eq!(members[0]["health"], "Unhealthy");
    assert_eq!(members[0]["eligible"], false);

    token.cancel();
    jh.await.ok();
}

#[cfg(unix)]
#[tokio::test]
async fn reload_preserves_health_state() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "upstream1"
uri = "http://127.0.0.1:1"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "main"
scheduler = "round-robin"
members = ["upstream1"]
fallback = "reject"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let token = sup.shutdown_token();
    let _shutdown = AutoShutdown(token.clone());
    let state = sup.state().clone();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed));

    // Wait for health probes to fail
    tokio::time::sleep(Duration::from_secs(1)).await;

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound");
    let admin_str = admin_addr.to_string();

    // Verify unhealthy before reload
    let (_, body) = http_get(&admin_str, "/-/upstreams").await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let members = json[0]["members"].as_array().unwrap();
    assert_eq!(members[0]["health"], "Unhealthy");

    // Reload with same config
    std::process::Command::new("kill")
        .arg("-HUP")
        .arg(std::process::id().to_string())
        .output()
        .ok();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify health state preserved after reload
    let (_, body) = http_get(&admin_str, "/-/upstreams").await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let members = json[0]["members"].as_array().unwrap();
    assert_eq!(
        members[0]["health"], "Unhealthy",
        "health state should be preserved after reload"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn different_upstreams_use_different_health_thresholds() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "fast-fail"
uri = "http://127.0.0.1:1"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstreams]]
id = "slow-fail"
uri = "http://127.0.0.1:2"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 3
successes_to_healthy = 1

[[upstream_groups]]
id = "main"
scheduler = "round-robin"
members = ["fast-fail", "slow-fail"]
fallback = "reject"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let token = sup.shutdown_token();
    let _shutdown = AutoShutdown(token.clone());
    let state = sup.state().clone();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        state.readiness.load(Ordering::Relaxed),
        "service should be ready"
    );

    tokio::time::sleep(Duration::from_millis(400)).await;

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound");
    let admin_str = admin_addr.to_string();

    let (status, body) = http_get(&admin_str, "/-/upstreams").await;
    assert_eq!(status, 200);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let groups = json.as_array().unwrap();
    let members = groups[0]["members"].as_array().unwrap();

    let fast = members.iter().find(|m| m["id"] == "fast-fail").unwrap();
    let slow = members.iter().find(|m| m["id"] == "slow-fail").unwrap();

    assert_eq!(
        fast["health"], "Unhealthy",
        "fast-fail (threshold=1) should be Unhealthy after probe failures"
    );
    assert!(
        slow["health"] != "Unhealthy",
        "slow-fail (threshold=3) should not be Unhealthy yet"
    );

    token.cancel();
    jh.await.ok();
}

#[cfg(unix)]
#[tokio::test]
async fn reload_changes_health_config_and_manager_uses_new_values() {
    let config_threshold_1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "upstream1"
uri = "http://127.0.0.1:1"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 1
successes_to_healthy = 1

[[upstream_groups]]
id = "main"
scheduler = "round-robin"
members = ["upstream1"]
fallback = "reject"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let f = write_config(config_threshold_1);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let token = sup.shutdown_token();
    let _shutdown = AutoShutdown(token.clone());
    let state = sup.state().clone();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed));

    tokio::time::sleep(Duration::from_secs(1)).await;

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound");
    let admin_str = admin_addr.to_string();

    let (_, body) = http_get(&admin_str, "/-/upstreams").await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let members = json[0]["members"].as_array().unwrap();
    assert_eq!(
        members[0]["health"], "Unhealthy",
        "should be Unhealthy with threshold=1"
    );

    let config_threshold_3 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "upstream1"
uri = "http://127.0.0.1:1"

[upstreams.health]
mode = "tcp_connect"
interval = "200ms"
timeout = "100ms"
failures_to_unhealthy = 3
successes_to_healthy = 1

[[upstream_groups]]
id = "main"
scheduler = "round-robin"
members = ["upstream1"]
fallback = "reject"

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    std::fs::write(f.path(), config_threshold_3).unwrap();
    std::process::Command::new("kill")
        .arg("-HUP")
        .arg(std::process::id().to_string())
        .output()
        .ok();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let (_, body) = http_get(&admin_str, "/-/upstreams").await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let members = json[0]["members"].as_array().unwrap();
    assert_ne!(
        members[0]["health"], "Unhealthy",
        "after reload with threshold=3, upstream should not be Unhealthy yet"
    );

    token.cancel();
    jh.await.ok();
}
