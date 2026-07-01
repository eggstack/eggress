pub mod pac;
pub mod reverse;
pub mod routes;
pub mod server;
pub mod static_content;

pub use reverse::{ReverseRegistry, ReverseServerEntry, ReverseServerId};
pub use server::{
    AdminServer, AdminSnapshot, AdminSnapshotProvider, AdminState, ListenerInfo, PacConfig,
    StaticAdminSnapshot, StaticRoute,
};

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
    use std::sync::Arc;
    use std::time::Instant;

    fn snapshot_with_router(router: Arc<eggress_routing::Router>) -> AdminSnapshot {
        AdminSnapshot {
            generation: 42,
            router,
            pac: Some(PacConfig {
                path: "/pac".to_string(),
                proxy_directive: "127.0.0.1:8080".to_string(),
                direct_fallback: true,
                direct_hosts: vec!["localhost".to_string()],
                direct_suffixes: vec!["local".to_string()],
            }),
            static_routes: vec![StaticRoute {
                path: "/test".to_string(),
                content_type: "text/html".to_string(),
                body: "<h1>Hello</h1>".to_string(),
            }],
            listeners: Vec::new(),
        }
    }

    fn test_state() -> AdminState {
        let router = Arc::new(eggress_routing::Router::new(
            vec![],
            eggress_routing::RouteActionSpec::Direct,
        ));
        let snap = snapshot_with_router(router);
        AdminState {
            metrics: Arc::new(eggress_metrics::MetricsRegistry::new()),
            start_time: Instant::now(),
            readiness: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            active_connections: None,
            provider: Arc::new(StaticAdminSnapshot { snapshot: snap }),
            udp_registry: Arc::new(eggress_udp::registry::UdpAssociationRegistry::new(
                eggress_udp::limits::UdpLimits::default(),
            )),
            reverse_registry: Arc::new(ReverseRegistry::new()),
        }
    }

    fn test_state_no_pac() -> AdminState {
        let router = Arc::new(eggress_routing::Router::new(
            vec![],
            eggress_routing::RouteActionSpec::Direct,
        ));
        let snap = AdminSnapshot {
            generation: 0,
            router,
            pac: None,
            static_routes: Vec::new(),
            listeners: Vec::new(),
        };
        AdminState {
            metrics: Arc::new(eggress_metrics::MetricsRegistry::new()),
            start_time: Instant::now(),
            readiness: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            active_connections: None,
            provider: Arc::new(StaticAdminSnapshot { snapshot: snap }),
            udp_registry: Arc::new(eggress_udp::registry::UdpAssociationRegistry::new(
                eggress_udp::limits::UdpLimits::default(),
            )),
            reverse_registry: Arc::new(ReverseRegistry::new()),
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
        assert!(json.get("rules").is_some());
        assert!(json.get("default_action").is_some());
        assert_eq!(json["rule_count"], 0);
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

    #[tokio::test]
    async fn reverse_endpoint_empty_when_no_servers() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/-/reverse").await;
        assert_eq!(status, 200);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["totals"]["server_count"], 0);
        assert!(json["servers"].is_array());
        assert_eq!(json["servers"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn reverse_endpoint_reports_registered_server_state() {
        use crate::reverse::{ReverseRegistry, ReverseServerEntry, ReverseServerId};
        use eggress_protocol_reverse::server::ReverseServerState;

        let state = test_state();
        let registry = ReverseRegistry::new();
        let server_state = Arc::new(ReverseServerState::default());
        server_state
            .active_control
            .store(3, std::sync::atomic::Ordering::Relaxed);
        server_state
            .active_streams
            .store(7, std::sync::atomic::Ordering::Relaxed);
        server_state
            .denied_bind
            .store(1, std::sync::atomic::Ordering::Relaxed);
        server_state
            .dropped_stream_limit
            .store(2, std::sync::atomic::Ordering::Relaxed);
        registry.register(ReverseServerEntry {
            id: ReverseServerId::from("rev-1"),
            control_bind: "127.0.0.1:8080".to_string(),
            state: server_state,
        });
        let mut state_with_rev = state;
        state_with_rev.reverse_registry = Arc::new(registry);

        let addr = start_server(state_with_rev).await;
        let (status, body) = http_get(&addr, "/-/reverse").await;
        assert_eq!(status, 200);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["totals"]["server_count"], 1);
        let servers = json["servers"].as_array().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0]["id"], "rev-1");
        assert_eq!(servers[0]["control_bind"], "127.0.0.1:8080");
        assert_eq!(servers[0]["active_control"], 3);
        assert_eq!(servers[0]["active_streams"], 7);
        assert_eq!(servers[0]["denied_bind"], 1);
        assert_eq!(servers[0]["dropped_stream_limit"], 2);
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
