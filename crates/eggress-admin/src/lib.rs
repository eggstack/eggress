pub mod client;
pub mod pac;
pub mod reverse;
pub mod routes;
pub mod server;
pub mod static_content;

pub use eggress_config::compile::{PacConfig, StaticRoute};
pub use reverse::{ReverseRegistry, ReverseServerEntry, ReverseServerId};
pub use server::{
    AdminServer, AdminSnapshot, AdminSnapshotProvider, AdminState, ListenerInfo,
    StaticAdminSnapshot,
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
    use crate::server::{build_response, AdminState};
    use std::sync::atomic::AtomicUsize;
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
            metrics_enabled: true,
            auth: None,
            allowed_hosts: eggress_config::compile::derive_allowed_hosts("127.0.0.1:9090"),
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
            metrics_enabled: true,
            auth: None,
            allowed_hosts: eggress_config::compile::derive_allowed_hosts("127.0.0.1:9090"),
        }
    }

    struct PacProvider {
        snapshot: AdminSnapshot,
        calls: AtomicUsize,
    }

    impl AdminSnapshotProvider for PacProvider {
        fn snapshot(&self) -> AdminSnapshot {
            let mut snapshot = self.snapshot.clone();
            if self
                .calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                > 0
            {
                snapshot.pac = None;
            }
            snapshot
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

    #[test]
    fn invalid_response_status_falls_back_to_internal_error() {
        let response = build_response(700, "body", "text/plain");
        assert_eq!(response.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn invalid_response_content_type_falls_back_without_panicking() {
        let response = build_response(200, "body", "text/plain\r\nX-Injected: true");
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "application/octet-stream"
        );
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
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
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
    async fn configured_auth_rejects_unauthenticated_requests() {
        let mut state = test_state();
        state.auth = Some(eggress_config::compile::AdminAuthConfig {
            bearer_token: Some("secret".to_string()),
            basic_username: None,
            basic_password: None,
        });
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/-/status").await;
        assert_eq!(status, 401);
        assert_eq!(body, "unauthorized");
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
        // With pac=None the PAC branch must not intercept: `/pac` falls
        // through to static routes and then the generic 404.
        let state = test_state_no_pac();
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/pac").await;
        assert_eq!(status, 404);
        assert_eq!(body, "not found");
    }

    #[tokio::test]
    async fn pac_path_static_route_served_when_pac_unconfigured() {
        // B42 pin: a static route at `/pac` stays reachable when pac=None.
        let router = Arc::new(eggress_routing::Router::new(
            vec![],
            eggress_routing::RouteActionSpec::Direct,
        ));
        let snap = AdminSnapshot {
            generation: 0,
            router,
            pac: None,
            static_routes: vec![StaticRoute {
                path: "/pac".to_string(),
                content_type: "text/plain".to_string(),
                body: "static pac placeholder".to_string(),
            }],
            listeners: Vec::new(),
        };
        let mut state = test_state_no_pac();
        state.provider = Arc::new(StaticAdminSnapshot { snapshot: snap });
        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/pac").await;
        assert_eq!(status, 200);
        assert_eq!(body, "static pac placeholder");
    }

    #[tokio::test]
    async fn pac_endpoint_uses_one_snapshot_for_path_and_content() {
        let mut state = test_state();
        let snapshot = state.snapshot();
        state.provider = Arc::new(PacProvider {
            snapshot,
            calls: AtomicUsize::new(0),
        });

        let addr = start_server(state).await;
        let (status, body) = http_get(&addr, "/pac").await;
        assert_eq!(status, 200);
        assert!(body.contains("function FindProxyForURL"));
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
        server_state
            .dropped_pending_limit
            .store(3, std::sync::atomic::Ordering::Relaxed);
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
        assert_eq!(servers[0]["dropped_pending_limit"], 3);
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
    fn pac_escapes_line_separators_and_control_characters() {
        let config = PacConfig {
            path: "/pac".to_string(),
            proxy_directive: "a\u{2028}b\u{2029}c\u{0001}d".to_string(),
            direct_fallback: false,
            direct_hosts: vec![],
            direct_suffixes: vec![],
        };
        let pac = generate_pac(&config);
        assert!(pac.contains("a\\u2028b\\u2029c\\u0001d"));
        for raw in ['\u{2028}', '\u{2029}', '\u{0001}'] {
            assert!(!pac.contains(raw));
        }
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

    // === Host / Origin / Content-Type validation ===

    async fn http_raw(addr: &str, request: &str) -> (u16, String) {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
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

    async fn get_with_host(addr: &str, path: &str, host: &str) -> (u16, String) {
        http_raw(
            addr,
            &format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"),
        )
        .await
    }

    const ROUTE_EXPLAIN_BODY: &str =
        r#"{"target":"example.com:443","listener":"l","protocol":"http"}"#;

    async fn post_route_explain(
        addr: &str,
        host: &str,
        extra_headers: &str,
        body: &str,
    ) -> (u16, String) {
        let mut request = format!("POST /-/route-explain HTTP/1.1\r\nHost: {host}\r\n");
        request.push_str(extra_headers);
        request.push_str(&format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ));
        http_raw(addr, &request).await
    }

    #[tokio::test]
    async fn default_loopback_hosts_are_accepted() {
        let state = test_state();
        let addr = start_server(state).await;
        // `http_get` already sends `Host: 127.0.0.1:<port>`; `localhost` and a
        // bracketed IPv6 with port must also pass with no configuration.
        assert_eq!(http_get(&addr, "/-/health").await.0, 200);
        assert_eq!(
            get_with_host(&addr, "/-/health", "localhost:9090").await.0,
            200
        );
        assert_eq!(
            get_with_host(
                &addr,
                "/-/health",
                &format!("[::1]:{}", addr.rsplit(':').next().unwrap())
            )
            .await
            .0,
            200
        );
    }

    #[tokio::test]
    async fn dns_rebinding_host_is_rejected() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = get_with_host(&addr, "/-/status", "attacker.example").await;
        assert_eq!(status, 400);
        assert_eq!(body, "bad request");
        // The rejected value and internal topology must not be echoed.
        assert!(!body.contains("attacker"));
        assert!(!body.contains("generation"));
    }

    #[tokio::test]
    async fn forwarded_host_header_does_not_widen_the_allowlist() {
        let state = test_state();
        let addr = start_server(state).await;
        let request = concat!(
            "GET /-/status HTTP/1.1\r\n",
            "Host: attacker.example\r\n",
            "X-Forwarded-Host: 127.0.0.1\r\n",
            "Connection: close\r\n\r\n",
        );
        let (status, body) = http_raw(&addr, request).await;
        assert_eq!(status, 400);
        assert_eq!(body, "bad request");
    }

    #[tokio::test]
    async fn missing_or_unparseable_host_is_rejected() {
        let state = test_state();
        let addr = start_server(state).await;
        // HTTP/1.1 without a Host header.
        let (status, body) =
            http_raw(&addr, "GET /-/health HTTP/1.1\r\nConnection: close\r\n\r\n").await;
        assert_eq!(status, 400);
        assert_eq!(body, "bad request");

        // A wildcard is not a host name.
        let (status, _) = get_with_host(&addr, "/-/health", "*").await;
        assert_eq!(status, 400);
    }

    #[tokio::test]
    async fn host_rejection_precedes_authentication() {
        let mut state = test_state();
        state.auth = Some(eggress_config::compile::AdminAuthConfig {
            bearer_token: Some("secret-token".to_string()),
            basic_username: None,
            basic_password: None,
        });
        let addr = start_server(state).await;
        // A bad Host fails with 400 before auth would produce a 401: the
        // validation must not depend on possessing a credential.
        let (status, body) = get_with_host(&addr, "/-/status", "attacker.example").await;
        assert_eq!(status, 400);
        assert_eq!(body, "bad request");

        let (status, _) = get_with_host(&addr, "/-/status", "localhost:9090").await;
        assert_eq!(status, 401);
    }

    #[tokio::test]
    async fn configured_allowed_host_is_accepted() {
        let mut state = test_state();
        state.allowed_hosts = vec!["admin.example".to_string()];
        let addr = start_server(state).await;
        assert_eq!(
            get_with_host(&addr, "/-/health", "admin.example:9090")
                .await
                .0,
            200
        );
        // With an explicit allowlist the bind-derived defaults are replaced.
        assert_eq!(
            get_with_host(&addr, "/-/health", "127.0.0.1:9090").await.0,
            400
        );
    }

    #[tokio::test]
    async fn empty_allowed_hosts_falls_back_to_the_bound_address() {
        let mut state = test_state();
        state.allowed_hosts = Vec::new();
        let addr = start_server(state).await;
        // The server derives the loopback defaults from its bound address.
        assert_eq!(http_get(&addr, "/-/health").await.0, 200);
        assert_eq!(
            get_with_host(&addr, "/-/health", "localhost:9090").await.0,
            200
        );
    }

    #[tokio::test]
    async fn route_explain_accepts_json_content_type() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = post_route_explain(
            &addr,
            "127.0.0.1:9090",
            "Content-Type: application/json\r\n",
            ROUTE_EXPLAIN_BODY,
        )
        .await;
        assert_eq!(status, 200, "body: {body}");
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["target"], "example.com:443");

        // Media type matching is case-insensitive and tolerates parameters.
        let (status, _) = post_route_explain(
            &addr,
            "127.0.0.1:9090",
            "Content-Type: Application/JSON; charset=utf-8\r\n",
            ROUTE_EXPLAIN_BODY,
        )
        .await;
        assert_eq!(status, 200);
    }

    #[tokio::test]
    async fn route_explain_requires_json_content_type() {
        let state = test_state();
        let addr = start_server(state).await;

        let (status, body) =
            post_route_explain(&addr, "127.0.0.1:9090", "", ROUTE_EXPLAIN_BODY).await;
        assert_eq!(status, 415, "missing content type must be rejected");
        assert!(body.contains("application/json"));

        let (status, _) = post_route_explain(
            &addr,
            "127.0.0.1:9090",
            "Content-Type: text/plain\r\n",
            ROUTE_EXPLAIN_BODY,
        )
        .await;
        assert_eq!(status, 415);
    }

    #[tokio::test]
    async fn route_explain_rejects_cross_origin_post() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, body) = post_route_explain(
            &addr,
            "127.0.0.1:9090",
            "Content-Type: application/json\r\nOrigin: https://attacker.example\r\n",
            ROUTE_EXPLAIN_BODY,
        )
        .await;
        assert_eq!(status, 403);
        assert_eq!(body, r#"{"error":"origin not allowed"}"#);
        assert!(!body.contains("attacker"));
    }

    #[tokio::test]
    async fn route_explain_accepts_same_origin_post() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, _) = post_route_explain(
            &addr,
            "127.0.0.1:9090",
            "Content-Type: application/json\r\nOrigin: http://localhost:9090\r\n",
            ROUTE_EXPLAIN_BODY,
        )
        .await;
        assert_eq!(status, 200);
    }

    #[tokio::test]
    async fn route_explain_rejects_null_origin_unless_allowed() {
        let state = test_state();
        let addr = start_server(state).await;
        let (status, _) = post_route_explain(
            &addr,
            "127.0.0.1:9090",
            "Content-Type: application/json\r\nOrigin: null\r\n",
            ROUTE_EXPLAIN_BODY,
        )
        .await;
        assert_eq!(status, 403);

        let mut allowed = test_state();
        allowed.allowed_hosts.push("null".to_string());
        let addr = start_server(allowed).await;
        let (status, _) = post_route_explain(
            &addr,
            "127.0.0.1:9090",
            "Content-Type: application/json\r\nOrigin: null\r\n",
            ROUTE_EXPLAIN_BODY,
        )
        .await;
        assert_eq!(status, 200);
    }
}
