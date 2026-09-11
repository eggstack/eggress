//! Minimal admin control-plane client.
//!
//! Owned by `eggress-admin` (not `eggress-cli`) so the route-explain command
//! cannot silently reimplement the admin HTTP protocol inside a command
//! handler. The implementation is deliberately small and HTTP/1.1-only to
//! match the admin server contract: request construction, status parsing,
//! body extraction, transport errors, and response deserialization all live
//! here. Callers only validate user inputs, choose local vs remote
//! explanation, render output, and map errors to process outcomes.

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Errors from the admin route-explain client. Transport and protocol
/// failures are typed so CLI handlers can map them without parsing strings.
#[derive(Debug, thiserror::Error)]
pub enum AdminClientError {
    /// The `--admin` URL is malformed or uses an unsupported scheme.
    #[error("invalid --admin '{url}': {reason}")]
    InvalidUrl {
        /// The user-supplied URL (never contains credentials in practice;
        /// admin URLs carry no secrets).
        url: String,
        /// Why the URL was rejected.
        reason: String,
    },

    /// TCP connection to the admin endpoint failed.
    #[error("failed to connect to admin at {addr}: {reason}")]
    Connect {
        /// Resolved `host:port` dial string.
        addr: String,
        /// Underlying I/O reason.
        reason: String,
    },

    /// Request send or response read failed.
    #[error("admin request failed: {0}")]
    Transport(String),

    /// The admin server answered with a non-200 status.
    #[error("admin returned {status}: {body}")]
    Status {
        /// HTTP status code.
        status: u16,
        /// Response body (truncated by the server contract).
        body: String,
    },

    /// A 200 response carried a body that is not a route explanation.
    #[error("failed to parse admin response: {0}")]
    Parse(String),
}

/// Parsed admin endpoint: host without brackets, port, and absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminEndpoint {
    /// Host without surrounding `[]` (IPv6 brackets are reapplied at dial).
    pub host: String,
    /// TCP port (default 9090).
    pub port: u16,
    /// Absolute request path (always starts with `/`).
    pub path: String,
}

/// Parse an `--admin` URL into dial parts.
///
/// Accepts `http://host[:port][/path]` and bare `host[:port][/path]`.
/// TLS admin URLs are rejected: the local admin contract is HTTP/1.1-only.
pub fn parse_admin_url(url: &str) -> Result<AdminEndpoint, String> {
    if url.starts_with("https://") {
        return Err("TLS admin URLs are not supported; use http://".to_string());
    }
    let without_proto = url.strip_prefix("http://").unwrap_or(url);
    let (host_port, path) = match without_proto.find('/') {
        Some(i) => (&without_proto[..i], &without_proto[i..]),
        None => (without_proto, "/"),
    };
    if host_port.is_empty() {
        return Err("missing host in admin URL".to_string());
    }
    let (host, port) = if let Some(rest) = host_port.strip_prefix('[') {
        let close = rest
            .find(']')
            .ok_or_else(|| "missing closing ']' in IPv6 admin host".to_string())?;
        let host = rest[..close].to_string();
        if host.is_empty() {
            return Err("missing host in admin URL".to_string());
        }
        let after = &rest[close + 1..];
        let port = match after.strip_prefix(':') {
            Some(port_str) => port_str.parse::<u16>().map_err(|_| {
                format!("invalid port '{port_str}' in admin URL (expected 1-65535)")
            })?,
            None if after.is_empty() => 9090,
            None => {
                return Err(format!(
                    "invalid IPv6 admin host '{host_port}' (expected [host] or [host]:port)"
                ));
            }
        };
        (host, port)
    } else {
        match host_port.rfind(':') {
            Some(i) => {
                let port_str = &host_port[i + 1..];
                let port = port_str.parse::<u16>().map_err(|_| {
                    format!("invalid port '{port_str}' in admin URL (expected 1-65535)")
                })?;
                let host = host_port[..i].to_string();
                if host.is_empty() {
                    return Err("missing host in admin URL".to_string());
                }
                (host, port)
            }
            None => (host_port.to_string(), 9090),
        }
    };
    Ok(AdminEndpoint {
        host,
        port,
        path: path.to_string(),
    })
}

impl AdminEndpoint {
    fn dial_addr(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    fn host_header(&self) -> String {
        self.dial_addr()
    }
}

/// Explain a route through a live admin server.
///
/// Sends `POST <admin>/-/route-explain` with `{target, listener, protocol}`
/// and returns the deserialized [`eggress_routing::RouteExplanation`].
/// `protocol` must be one of `http`, `socks4`, `socks5`; validation stays
/// with the caller so this client never reimplements CLI value policy.
pub async fn route_explain(
    admin_url: &str,
    target: &str,
    listener: &str,
    protocol: &str,
) -> Result<eggress_routing::RouteExplanation, AdminClientError> {
    let endpoint = parse_admin_url(admin_url).map_err(|reason| AdminClientError::InvalidUrl {
        url: admin_url.to_string(),
        reason,
    })?;
    // The admin route-explain handler is mounted at `/-/route-explain`.
    // A caller-supplied path prefix is honored; otherwise the canonical
    // path is used.
    let path = if endpoint.path == "/" {
        "/-/route-explain".to_string()
    } else {
        endpoint.path.clone()
    };
    let body = serde_json::json!({
        "target": target,
        "listener": listener,
        "protocol": protocol,
    });
    let body_str = body.to_string();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body_str}",
        endpoint.host_header(),
        body_str.len(),
    );

    let dial = endpoint.dial_addr();
    let mut stream =
        tokio::net::TcpStream::connect(&dial)
            .await
            .map_err(|e| AdminClientError::Connect {
                addr: dial.clone(),
                reason: e.to_string(),
            })?;

    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| AdminClientError::Transport(format!("failed to send request: {e}")))?;
    stream
        .flush()
        .await
        .map_err(|e| AdminClientError::Transport(format!("failed to send request: {e}")))?;
    // Do not shut down the write half here: the server answers on this same
    // connection (`Connection: close`) and a write-half shutdown races the
    // response on some platforms, surfacing as an empty reply.

    let mut response = Vec::new();
    loop {
        let mut buf = [0u8; 4096];
        match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&buf[..n]),
            Err(e) => {
                return Err(AdminClientError::Transport(format!(
                    "failed to read response: {e}"
                )));
            }
        }
    }
    let text = String::from_utf8_lossy(&response).to_string();
    let body_start = text.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    let body = text[body_start..].to_string();
    let status_line = text.lines().next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    if status != 200 {
        return Err(AdminClientError::Status { status, body });
    }
    serde_json::from_str::<eggress_routing::RouteExplanation>(&body)
        .map_err(|e| AdminClientError::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ipv4_with_default_port() {
        let ep = parse_admin_url("http://127.0.0.1/admin").unwrap();
        assert_eq!(
            ep,
            AdminEndpoint {
                host: "127.0.0.1".to_string(),
                port: 9090,
                path: "/admin".to_string(),
            }
        );
    }

    #[test]
    fn parses_bracketed_ipv6_loopback() {
        let ep = parse_admin_url("http://[::1]/-/route-explain").unwrap();
        assert_eq!(ep.host, "::1");
        assert_eq!(ep.port, 9090);
        assert_eq!(ep.path, "/-/route-explain");
    }

    #[test]
    fn parses_bracketed_ipv6_with_port() {
        let ep = parse_admin_url("http://[2001:db8::1]:8080/-/route-explain").unwrap();
        assert_eq!(ep.host, "2001:db8::1");
        assert_eq!(ep.port, 8080);
    }

    #[test]
    fn parses_domain_with_port() {
        let ep = parse_admin_url("http://admin.example.com:8080/-/x").unwrap();
        assert_eq!(ep.host, "admin.example.com");
        assert_eq!(ep.port, 8080);
    }

    #[test]
    fn rejects_malformed_ports() {
        assert!(parse_admin_url("http://host:notaport/path").is_err());
        assert!(parse_admin_url("http://host:99999/path").is_err());
        assert!(parse_admin_url("http://[::1]:notaport/admin").is_err());
    }

    #[test]
    fn rejects_tls_admin_urls() {
        let err = parse_admin_url("https://127.0.0.1:9090/-/route-explain").unwrap_err();
        assert!(err.contains("TLS"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn reports_connection_failure_without_panicking() {
        // Port 1 is unroutable for TCP connect in tests; the client must
        // return a typed Connect error rather than exiting or panicking.
        let err = route_explain("http://127.0.0.1:1", "example.com:443", "cli", "http")
            .await
            .unwrap_err();
        assert!(
            matches!(err, AdminClientError::Connect { .. }),
            "unexpected error: {err:?}"
        );
    }

    #[tokio::test]
    async fn surfaces_non_200_admin_responses() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).await;
            let body = r#"{"error":"missing 'target' field"}"#;
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
        let err = route_explain(&format!("http://{addr}"), "example.com:443", "cli", "http")
            .await
            .unwrap_err();
        match err {
            AdminClientError::Status { status, .. } => assert_eq!(status, 400),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_malformed_200_bodies() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).await;
            let body = "not-json";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
        let err = route_explain(&format!("http://{addr}"), "example.com:443", "cli", "http")
            .await
            .unwrap_err();
        assert!(
            matches!(err, AdminClientError::Parse(_)),
            "unexpected error: {err:?}"
        );
    }

    #[tokio::test]
    async fn round_trips_route_explain_against_live_admin() {
        use std::sync::Arc;
        use std::time::Instant;

        let router = Arc::new(eggress_routing::Router::new(
            vec![],
            eggress_routing::RouteActionSpec::Direct,
        ));
        let snapshot = crate::server::AdminSnapshot {
            generation: 7,
            router,
            pac: None,
            static_routes: vec![],
            listeners: vec![],
        };
        let state = crate::server::AdminState {
            metrics: Arc::new(eggress_metrics::MetricsRegistry::new()),
            start_time: Instant::now(),
            readiness: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            active_connections: None,
            provider: Arc::new(crate::server::StaticAdminSnapshot { snapshot }),
            udp_registry: Arc::new(eggress_udp::registry::UdpAssociationRegistry::new(
                eggress_udp::limits::UdpLimits::default(),
            )),
            reverse_registry: Arc::new(crate::reverse::ReverseRegistry::new()),
            metrics_enabled: true,
            auth: None,
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        let server = crate::server::AdminServer::new("127.0.0.1:0", cancel.clone())
            .await
            .unwrap();
        let addr = server.listener.local_addr().unwrap().to_string();
        tokio::spawn(async move { server.run(state).await.unwrap() });

        let explanation =
            route_explain(&format!("http://{addr}"), "example.com:443", "cli", "http")
                .await
                .expect("live admin route-explain must succeed");
        assert_eq!(explanation.target, "example.com:443");
    }
}
