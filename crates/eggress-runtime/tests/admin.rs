use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::Instant;

use eggress_admin::{AdminServer, AdminState, ListenerInfo, StaticRoute};
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
    AdminState {
        metrics: Arc::new(eggress_metrics::MetricsRegistry::new()),
        generation: Arc::new(AtomicU64::new(3)),
        start_time: Instant::now(),
        static_routes: Arc::new(vec![StaticRoute {
            path: "/test".to_string(),
            content_type: "text/html".to_string(),
            body: "<h1>Hello</h1>".to_string(),
        }]),
        pac_config: Arc::new(None),
        router: Some(router.clone()),
        routing: Some(Arc::new(eggress_routing::SharedRoutingService::new_arc(
            router,
        ))),
        listeners: Arc::new(vec![
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
        ]),
        active_connections: Some(Arc::new(AtomicU64::new(5))),
        readiness: Arc::new(AtomicBool::new(true)),
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
