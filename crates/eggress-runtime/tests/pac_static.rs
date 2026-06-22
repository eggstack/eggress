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

async fn http_get(
    addr: &str,
    path: &str,
) -> (u16, String, std::collections::HashMap<String, String>) {
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

    let mut headers = std::collections::HashMap::new();
    for line in response.lines().skip(1) {
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }

    let body = response.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body, headers)
}

struct AutoShutdown(tokio_util::sync::CancellationToken);
impl Drop for AutoShutdown {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[tokio::test]
async fn pac_endpoint_serves_valid_javascript() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[admin]
bind = "127.0.0.1:0"
enabled = true

[admin.pac]
direct = ["localhost", "127.0.0.1"]
proxy = "PROXY 127.0.0.1:8080"
fallback = "DIRECT"
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound");
    let admin_str = admin_addr.to_string();

    let (status, body, headers) = http_get(&admin_str, "/pac").await;
    assert_eq!(status, 200);
    assert!(
        headers
            .get("content-type")
            .map(|c| c.contains("application/x-ns-proxy-autoconfig"))
            .unwrap_or(false),
        "content-type should be application/x-ns-proxy-autoconfig, got {:?}",
        headers.get("content-type")
    );
    assert!(
        body.contains("function FindProxyForURL"),
        "PAC body should contain FindProxyForURL function"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn pac_not_configured_returns_404() {
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound");
    let admin_str = admin_addr.to_string();

    let (status, _, _) = http_get(&admin_str, "/pac").await;
    assert_eq!(status, 404);

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn static_content_serves_configured_files() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[admin]
bind = "127.0.0.1:0"
enabled = true

[[admin.static_content]]
path = "/status"
content_type = "text/html"
body = "<h1>Status OK</h1>"

[[admin.static_content]]
path = "/version"
content_type = "text/plain"
body = "0.1.0"
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound");
    let admin_str = admin_addr.to_string();

    let (status, body, headers) = http_get(&admin_str, "/status").await;
    assert_eq!(status, 200);
    assert_eq!(body, "<h1>Status OK</h1>");
    assert!(
        headers
            .get("content-type")
            .map(|c| c.contains("text/html"))
            .unwrap_or(false),
        "content-type should be text/html"
    );

    let (status, body, headers) = http_get(&admin_str, "/version").await;
    assert_eq!(status, 200);
    assert_eq!(body, "0.1.0");
    assert!(
        headers
            .get("content-type")
            .map(|c| c.contains("text/plain"))
            .unwrap_or(false),
        "content-type should be text/plain"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn unknown_static_path_returns_404() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[admin]
bind = "127.0.0.1:0"
enabled = true

[[admin.static_content]]
path = "/exists"
content_type = "text/plain"
body = "here"
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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound");
    let admin_str = admin_addr.to_string();

    let (status, _, _) = http_get(&admin_str, "/exists").await;
    assert_eq!(status, 200);

    let (status, _, _) = http_get(&admin_str, "/not-exists").await;
    assert_eq!(status, 404);

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn pac_reload_serves_new_content() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[admin]
bind = "127.0.0.1:0"
enabled = true

[admin.pac]
direct = ["localhost"]
proxy = "PROXY 127.0.0.1:8080"
fallback = "DIRECT"
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[admin]
bind = "127.0.0.1:0"
enabled = true

[admin.pac]
direct = ["localhost"]
proxy = "PROXY 127.0.0.1:9999"
fallback = "DIRECT"
"#;

    let f = write_config(config1);
    let path = f.path().to_str().unwrap().to_string();
    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();

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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let (_status, body, _) = http_get(&admin_addr, "/pac").await;
    assert!(body.contains("PROXY 127.0.0.1:8080"));

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

    let (_status, body, _) = http_get(&admin_addr, "/pac").await;
    assert!(
        body.contains("PROXY 127.0.0.1:9999"),
        "admin should serve new PAC after reload, got: {body}"
    );

    token.cancel();
    jh.await.ok();
}

#[tokio::test]
async fn static_content_reload_serves_new_body() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[admin]
bind = "127.0.0.1:0"
enabled = true

[[admin.static_content]]
path = "/version"
content_type = "text/plain"
body = "0.1.0"
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[admin]
bind = "127.0.0.1:0"
enabled = true

[[admin.static_content]]
path = "/version"
content_type = "text/plain"
body = "0.2.0"
"#;

    let f = write_config(config1);
    let path = f.path().to_str().unwrap().to_string();
    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();

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

    let admin_addr = state
        .admin_local_addr
        .lock()
        .unwrap()
        .expect("admin should have bound")
        .to_string();

    let (_status, body, _) = http_get(&admin_addr, "/version").await;
    assert_eq!(body, "0.1.0");

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

    let (_status, body, _) = http_get(&admin_addr, "/version").await;
    assert_eq!(
        body, "0.2.0",
        "admin should serve new static content after reload"
    );

    token.cancel();
    jh.await.ok();
}
