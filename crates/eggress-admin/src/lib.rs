pub mod pac;
pub mod routes;
pub mod server;
pub mod static_content;

pub use server::AdminServer;
pub use server::{AdminState, ListenerInfo, PacConfig, StaticRoute};

#[derive(Debug, thiserror::Error)]
pub enum AdminError {
    #[error("bind error: {0}")]
    Bind(#[from] std::io::Error),

    #[error("accept error: {0}")]
    Accept(String),

    #[error("server error: {0}")]
    Server(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pac::generate_pac;
    use crate::server::AdminState;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use std::time::Instant;

    fn test_state() -> AdminState {
        AdminState {
            metrics: Arc::new(eggress_metrics::MetricsRegistry::new()),
            generation: Arc::new(AtomicU64::new(42)),
            start_time: Instant::now(),
            static_routes: Arc::new(vec![StaticRoute {
                path: "/test".to_string(),
                content_type: "text/html".to_string(),
                body: "<h1>Hello</h1>".to_string(),
            }]),
            pac_config: Arc::new(Some(PacConfig {
                path: "/pac".to_string(),
                proxy_directive: "127.0.0.1:8080".to_string(),
                direct_fallback: true,
                direct_hosts: vec!["localhost".to_string()],
                direct_suffixes: vec!["local".to_string()],
            })),
            router: None,
            routing: None,
            listeners: Arc::new(vec![]),
            active_connections: None,
            readiness: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }

    fn test_state_no_pac() -> AdminState {
        AdminState {
            metrics: Arc::new(eggress_metrics::MetricsRegistry::new()),
            generation: Arc::new(AtomicU64::new(0)),
            start_time: Instant::now(),
            static_routes: Arc::new(vec![]),
            pac_config: Arc::new(None),
            router: None,
            routing: None,
            listeners: Arc::new(vec![]),
            active_connections: None,
            readiness: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }

    async fn start_server(state: AdminState) -> String {
        let cancel = tokio_util::sync::CancellationToken::new();
        let server = AdminServer::new("127.0.0.1:0", cancel.clone())
            .await
            .unwrap();
        let addr = server.listener.local_addr().unwrap();
        let bind = addr.to_string();
        tokio::spawn(async move { server.run(state).await.unwrap() });
        bind
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

    #[tokio::test]
    async fn health_returns_200() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/-/health").await;
        assert_eq!(status, 200);
        assert_eq!(body, "ok");
    }

    #[tokio::test]
    async fn ready_returns_200() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/-/ready").await;
        assert_eq!(status, 200);
        assert_eq!(body, "ready");
    }

    #[tokio::test]
    async fn status_returns_valid_json() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/-/status").await;
        assert_eq!(status, 200);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["version"], "0.1.0");
        assert_eq!(json["generation"], 42);
        assert!(json["uptime_seconds"].is_number());
    }

    #[tokio::test]
    async fn metrics_returns_prometheus_format() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/metrics").await;
        assert_eq!(status, 200);
        assert!(body.contains("eggress_connections_active"));
        assert!(body.contains("eggress_connections_total"));
    }

    #[tokio::test]
    async fn static_content_serves_correct_content_type_and_body() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/test").await;
        assert_eq!(status, 200);
        assert_eq!(body, "<h1>Hello</h1>");
    }

    #[tokio::test]
    async fn unknown_path_returns_404() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/nonexistent").await;
        assert_eq!(status, 404);
        assert_eq!(body, "not found");
    }

    #[tokio::test]
    async fn pac_endpoint_returns_pac_when_configured() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/pac").await;
        assert_eq!(status, 200);
        assert!(body.contains("function FindProxyForURL"));
    }

    #[tokio::test]
    async fn pac_endpoint_returns_404_when_not_configured() {
        let state = test_state_no_pac();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/pac").await;
        assert_eq!(status, 404);
        assert!(body.contains("pac not configured"));
    }

    #[tokio::test]
    async fn routes_endpoint_returns_json() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/-/routes").await;
        assert_eq!(status, 200);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json.is_array());
        let routes = json.as_array().unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0]["path"], "/test");
    }

    #[tokio::test]
    async fn upstreams_endpoint_returns_json() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/-/upstreams").await;
        assert_eq!(status, 200);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json.is_array());
    }

    #[test]
    fn pac_generation_produces_valid_javascript() {
        let config = PacConfig {
            path: "/pac".to_string(),
            proxy_directive: "127.0.0.1:8080".to_string(),
            direct_fallback: true,
            direct_hosts: vec!["localhost".to_string(), "127.0.0.1".to_string()],
            direct_suffixes: vec!["local".to_string(), "internal".to_string()],
        };
        let pac = generate_pac(&config);
        assert!(pac.contains("function FindProxyForURL(url, host)"));
        assert!(pac.contains("isPlainHostName(host)"));
        assert!(pac.contains("PROXY 127.0.0.1:8080; DIRECT"));
        assert!(pac.contains("localhost"));
        assert!(pac.contains("local"));
        assert!(pac.contains("internal"));
    }

    #[test]
    fn pac_escaping_of_quotes_and_backslashes() {
        let config = PacConfig {
            path: "/pac".to_string(),
            proxy_directive: "proxy\"example.com:8080".to_string(),
            direct_fallback: false,
            direct_hosts: vec!["test\\host".to_string()],
            direct_suffixes: vec![],
        };
        let pac = generate_pac(&config);
        assert!(pac.contains("test\\\\host"));
        assert!(pac.contains("proxy\\\"example.com:8080"));
    }

    #[test]
    fn pac_no_fallback_when_disabled() {
        let config = PacConfig {
            path: "/pac".to_string(),
            proxy_directive: "proxy:8080".to_string(),
            direct_fallback: false,
            direct_hosts: vec![],
            direct_suffixes: vec![],
        };
        let pac = generate_pac(&config);
        assert!(pac.contains("return \"PROXY proxy:8080\";"));
        assert!(!pac.contains("; DIRECT"));
    }

    #[test]
    fn pac_direct_hosts_sorted() {
        let config = PacConfig {
            path: "/pac".to_string(),
            proxy_directive: "proxy:8080".to_string(),
            direct_fallback: true,
            direct_hosts: vec!["z.com".to_string(), "a.com".to_string()],
            direct_suffixes: vec![],
        };
        let pac = generate_pac(&config);
        let a_pos = pac.find("a.com").unwrap();
        let z_pos = pac.find("z.com").unwrap();
        assert!(a_pos < z_pos, "hosts should be sorted");
    }

    #[tokio::test]
    async fn ready_returns_503_when_not_ready() {
        let mut state = test_state();
        state.readiness = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/-/ready").await;
        assert_eq!(status, 503);
        assert_eq!(body, "not ready");
    }

    #[tokio::test]
    async fn readiness_becomes_false_before_drain() {
        let mut state = test_state();
        state.readiness = Arc::new(std::sync::atomic::AtomicBool::new(true));
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
}
