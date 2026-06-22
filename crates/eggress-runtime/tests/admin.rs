use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::Instant;

use eggress_admin::{
    AdminServer, AdminSnapshot, AdminState, ListenerInfo, StaticAdminSnapshot, StaticRoute,
};
use eggress_routing::Router;

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

async fn http_post(addr: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
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

async fn start_server(state: AdminState) -> String {
    let cancel = tokio_util::sync::CancellationToken::new();
    let server = AdminServer::new("127.0.0.1:0", cancel.clone())
        .await
        .unwrap();
    let addr = server.local_addr().unwrap().to_string();
    tokio::spawn(async move { server.run(state).await.unwrap() });
    addr
}

fn test_state_with_listeners() -> AdminState {
    let router = Arc::new(Router::new(
        vec![],
        eggress_routing::RouteActionSpec::Direct,
    ));
    let snap = AdminSnapshot {
        generation: 3,
        router,
        pac: None,
        static_routes: vec![StaticRoute {
            path: "/test".to_string(),
            content_type: "text/html".to_string(),
            body: "<h1>Hello</h1>".to_string(),
        }],
        listeners: vec![
            ListenerInfo {
                name: "http-in".to_string(),
                bind: "0.0.0.0:8080".to_string(),
                local_addr: "0.0.0.0:8080".to_string(),
                protocols: vec!["http".to_string()],
            },
            ListenerInfo {
                name: "socks-in".to_string(),
                bind: "0.0.0.0:1080".to_string(),
                local_addr: "0.0.0.0:1080".to_string(),
                protocols: vec!["socks5".to_string()],
            },
        ],
    };
    AdminState {
        metrics: Arc::new(eggress_metrics::MetricsRegistry::new()),
        start_time: Instant::now(),
        readiness: Arc::new(AtomicBool::new(true)),
        active_connections: Some(Arc::new(AtomicU64::new(5))),
        provider: Arc::new(StaticAdminSnapshot { snapshot: snap }),
    }
}

#[tokio::test]
async fn status_lists_listeners() {
    let state = test_state_with_listeners();
    let addr = start_server(state).await;
    let (status, body) = http_get(&addr, "/-/status").await;
    assert_eq!(status, 200);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let listeners = json["listeners"].as_array().unwrap();
    assert_eq!(listeners.len(), 2);
    assert_eq!(listeners[0]["name"], "http-in");
    assert_eq!(listeners[1]["name"], "socks-in");
}

#[tokio::test]
async fn status_shows_active_connections() {
    let state = test_state_with_listeners();
    let addr = start_server(state).await;
    let (status, body) = http_get(&addr, "/-/status").await;
    assert_eq!(status, 200);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["active_connections"], 5);
}

#[tokio::test]
async fn status_shows_generation() {
    let state = test_state_with_listeners();
    let addr = start_server(state).await;
    let (status, body) = http_get(&addr, "/-/status").await;
    assert_eq!(status, 200);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["generation"], 3);
}

#[tokio::test]
async fn readiness_returns_503_during_shutdown() {
    let mut state = test_state_with_listeners();
    state.readiness = Arc::new(AtomicBool::new(false));
    let addr = start_server(state).await;
    let (status, body) = http_get(&addr, "/-/ready").await;
    assert_eq!(status, 503);
    assert_eq!(body, "not ready");
}

#[tokio::test]
async fn readiness_transitions_from_ready_to_not_ready() {
    let mut state = test_state_with_listeners();
    state.readiness = Arc::new(AtomicBool::new(true));
    let addr = start_server(state.clone()).await;

    let (status, body) = http_get(&addr, "/-/ready").await;
    assert_eq!(status, 200);
    assert_eq!(body, "ready");

    state
        .readiness
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let (status, body) = http_get(&addr, "/-/ready").await;
    assert_eq!(status, 503);
    assert_eq!(body, "not ready");
}

#[tokio::test]
async fn oversized_route_explain_body_returns_413() {
    let state = test_state_with_listeners();
    let addr = start_server(state).await;
    let large_body = "x".repeat(17 * 1024);
    let (status, _) = http_post(&addr, "/-/route-explain", &large_body).await;
    assert_eq!(status, 413);
}

#[tokio::test]
async fn health_endpoint_returns_200() {
    let state = test_state_with_listeners();
    let addr = start_server(state).await;
    let (status, body) = http_get(&addr, "/-/health").await;
    assert_eq!(status, 200);
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn static_content_returns_correct_response() {
    let state = test_state_with_listeners();
    let addr = start_server(state).await;
    let (status, body) = http_get(&addr, "/test").await;
    assert_eq!(status, 200);
    assert_eq!(body, "<h1>Hello</h1>");
}

#[tokio::test]
async fn unknown_path_returns_404() {
    let state = test_state_with_listeners();
    let addr = start_server(state).await;
    let (status, _) = http_get(&addr, "/nonexistent").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn metrics_endpoint_returns_prometheus() {
    let state = test_state_with_listeners();
    let addr = start_server(state).await;
    let (status, body) = http_get(&addr, "/metrics").await;
    assert_eq!(status, 200);
    assert!(body.contains("eggress_connections_active"));
}

#[tokio::test]
async fn admin_routes_reflect_reload() {
    use std::io::Write as _;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[rules]]
id = "first"
any = true
direct = true

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[rules]]
id = "replaced"
any = true
direct = true

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(config1.as_bytes()).unwrap();
    f.flush().unwrap();
    let path = f.path().to_str().unwrap().to_string();

    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let token = sup.shutdown_token();
    struct AutoShutdown(tokio_util::sync::CancellationToken);
    impl Drop for AutoShutdown {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let (_status, body) = http_get(&admin_addr, "/-/routes").await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["rules"][0]["id"], "first");

    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        f.write_all(config2.as_bytes()).unwrap();
        f.flush().unwrap();
    }
    std::process::Command::new("kill")
        .arg("-HUP")
        .arg(std::process::id().to_string())
        .output()
        .ok();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let (_status, body) = http_get(&admin_addr, "/-/routes").await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        json["rules"][0]["id"], "replaced",
        "routes endpoint should reflect reloaded rule"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn admin_route_explain_reflects_reload() {
    use std::io::Write as _;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[rules]]
id = "rule-direct"
host_exact = "old.example"
direct = true

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[rules]]
id = "rule-new"
host_exact = "old.example"
direct = true

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(config1.as_bytes()).unwrap();
    f.flush().unwrap();
    let path = f.path().to_str().unwrap().to_string();

    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let token = sup.shutdown_token();
    struct AutoShutdown(tokio_util::sync::CancellationToken);
    impl Drop for AutoShutdown {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let body = r#"{"target":"old.example:443","listener":"http-in","protocol":"http"}"#;
    let (_status, body) = http_post(&admin_addr, "/-/route-explain", body).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["matched_rule"], "rule-direct");

    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        f.write_all(config2.as_bytes()).unwrap();
        f.flush().unwrap();
    }
    std::process::Command::new("kill")
        .arg("-HUP")
        .arg(std::process::id().to_string())
        .output()
        .ok();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let body2 = r#"{"target":"old.example:443","listener":"http-in","protocol":"http"}"#;
    let (_status, body) = http_post(&admin_addr, "/-/route-explain", body2).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        json["matched_rule"], "rule-new",
        "route-explain should reflect reloaded rule"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn route_explain_source_field_changes_decision() {
    use std::io::Write as _;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[rules]]
id = "internal-only"
match = { source_cidr = "10.0.0.0/8" }
direct = true

[[rules]]
id = "external"
any = true
direct = true

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(config.as_bytes()).unwrap();
    f.flush().unwrap();
    let path = f.path().to_str().unwrap().to_string();

    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let token = sup.shutdown_token();
    struct AutoShutdown(tokio_util::sync::CancellationToken);
    impl Drop for AutoShutdown {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let internal_body = r#"{"target":"example.com:443","listener":"http-in","protocol":"http","source":"10.1.2.3:5000"}"#;
    let (_status, body) = http_post(&admin_addr, "/-/route-explain", internal_body).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        json["matched_rule"], "internal-only",
        "source=10.1.2.3 should match internal-only rule"
    );

    let external_body = r#"{"target":"example.com:443","listener":"http-in","protocol":"http","source":"192.0.2.10:5000"}"#;
    let (_status, body) = http_post(&admin_addr, "/-/route-explain", external_body).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        json["matched_rule"], "external",
        "source=192.0.2.10 should fall through to external"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn route_explain_identity_field_changes_decision() {
    use std::io::Write as _;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[rules]]
id = "alice"
match = { identity = "alice" }
direct = true

[[rules]]
id = "default"
any = true
direct = true

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(config.as_bytes()).unwrap();
    f.flush().unwrap();
    let path = f.path().to_str().unwrap().to_string();

    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let token = sup.shutdown_token();
    struct AutoShutdown(tokio_util::sync::CancellationToken);
    impl Drop for AutoShutdown {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let alice_body =
        r#"{"target":"example.com:443","listener":"http-in","protocol":"http","identity":"alice"}"#;
    let (_status, body) = http_post(&admin_addr, "/-/route-explain", alice_body).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["matched_rule"], "alice");

    let anon_body = r#"{"target":"example.com:443","listener":"http-in","protocol":"http"}"#;
    let (_status, body) = http_post(&admin_addr, "/-/route-explain", anon_body).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        json["matched_rule"], "default",
        "anonymous identity should fall through to default"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn route_explain_invalid_source_returns_400() {
    use std::io::Write as _;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[admin]
bind = "127.0.0.1:0"
enabled = true
"#;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(config.as_bytes()).unwrap();
    f.flush().unwrap();
    let path = f.path().to_str().unwrap().to_string();

    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let token = sup.shutdown_token();
    struct AutoShutdown(tokio_util::sync::CancellationToken);
    impl Drop for AutoShutdown {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let bad_body = r#"{"target":"example.com:443","listener":"http-in","protocol":"http","source":"not-an-addr"}"#;
    let (status, _body) = http_post(&admin_addr, "/-/route-explain", bad_body).await;
    assert_eq!(status, 400, "invalid source should return 400");

    let oversized = "x".repeat(300);
    let oversized_body = format!(
        r#"{{"target":"example.com:443","listener":"http-in","protocol":"http","identity":"{oversized}"}}"#
    );
    let (status, _body) = http_post(&admin_addr, "/-/route-explain", &oversized_body).await;
    assert_eq!(status, 400, "oversized identity should return 400");

    let empty_body =
        r#"{"target":"example.com:443","listener":"http-in","protocol":"http","identity":""}"#;
    let (status, _body) = http_post(&admin_addr, "/-/route-explain", empty_body).await;
    assert_eq!(status, 400, "empty identity should return 400");

    token.cancel();
    jh.await.ok();
}
