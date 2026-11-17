use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use bytes::Bytes;
use http_body_util::Full;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::reverse::ReverseRegistry;
use crate::routes::handle_request;
use crate::AdminError;
use eggress_config::compile::{PacConfig, StaticRoute};

use eggress_config::compile::admin_hosts::{derive_allowed_hosts, normalize_host_entry};

/// Upper bound on concurrently served admin connections. The admin surface is
/// local tooling; an unbounded accept loop would otherwise turn the admin port
/// into a trivially cheap DoS target.
const MAX_ADMIN_CONNECTIONS: usize = 64;
const AUTH_FAILURE_LIMIT: u32 = 5;
const AUTH_FAILURE_WINDOW: Duration = Duration::from_secs(60);
const AUTH_FAILURE_BLOCK: Duration = Duration::from_secs(30);
const MAX_AUTH_FAILURE_ENTRIES: usize = 4096;

#[derive(Default)]
struct AuthFailure {
    window_started: Option<Instant>,
    failures: u32,
    blocked_until: Option<Instant>,
}

#[derive(Default)]
struct AuthFailureLimiter {
    entries: Mutex<std::collections::HashMap<std::net::IpAddr, AuthFailure>>,
}

impl AuthFailureLimiter {
    fn lock_entries(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::HashMap<std::net::IpAddr, AuthFailure>> {
        self.entries.lock().unwrap_or_else(|e| {
            tracing::warn!("auth failure limiter was poisoned; clearing it: {e}");
            let mut entries = e.into_inner();
            entries.clear();
            self.entries.clear_poison();
            entries
        })
    }

    fn blocked_for(&self, ip: std::net::IpAddr) -> Option<Duration> {
        let entries = self.lock_entries();
        entries.get(&ip).and_then(|entry| {
            entry
                .blocked_until
                .and_then(|until| until.checked_duration_since(Instant::now()))
        })
    }

    fn record_failure(&self, ip: std::net::IpAddr) {
        let now = Instant::now();
        let mut entries = self.lock_entries();
        if entries.len() >= MAX_AUTH_FAILURE_ENTRIES && !entries.contains_key(&ip) {
            entries.retain(|_, entry| {
                entry
                    .window_started
                    .is_some_and(|started| now.duration_since(started) < AUTH_FAILURE_WINDOW)
            });
            if entries.len() >= MAX_AUTH_FAILURE_ENTRIES {
                // Evict the entry with the oldest window_started (LRU-by-time)
                // rather than HashMap's arbitrary iteration order.
                let victim = entries
                    .iter()
                    .min_by_key(|(_, entry)| entry.window_started.unwrap_or(now))
                    .map(|(ip, _)| *ip);
                if let Some(victim) = victim {
                    entries.remove(&victim);
                }
            }
        }
        let entry = entries.entry(ip).or_default();
        if entry
            .window_started
            .is_none_or(|started| now.duration_since(started) >= AUTH_FAILURE_WINDOW)
        {
            entry.window_started = Some(now);
            entry.failures = 0;
        }
        entry.failures = entry.failures.saturating_add(1);
        if entry.failures >= AUTH_FAILURE_LIMIT {
            // Extend the block on every further failure during the block so
            // blocked requests cannot poll without cost (sliding window).
            entry.blocked_until = Some(now + AUTH_FAILURE_BLOCK);
        }
    }

    fn record_success(&self, ip: std::net::IpAddr) {
        self.lock_entries().remove(&ip);
    }
}

pub struct AdminServer {
    pub(crate) listener: TcpListener,
    cancel: CancellationToken,
}

fn authorized(
    req: &http::Request<hyper::body::Incoming>,
    auth: &eggress_config::compile::AdminAuthConfig,
) -> bool {
    if let Some(expected) = auth.bearer_token.as_deref() {
        return authorization_payload(req, "Bearer")
            .is_some_and(|token| token.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() == 1);
    }

    let Some(username) = auth.basic_username.as_deref() else {
        return false;
    };
    let Some(password) = auth.basic_password.as_deref() else {
        return false;
    };
    authorization_payload(req, "Basic")
        .and_then(|value| {
            let value = Zeroizing::new(
                base64::engine::general_purpose::STANDARD
                    .decode(value)
                    .ok()?,
            );
            let separator = value.iter().position(|&byte| byte == b':')?;
            Some((value, separator))
        })
        .is_some_and(|(value, separator)| {
            (value[..separator].ct_eq(username.as_bytes())
                & value[separator + 1..].ct_eq(password.as_bytes()))
            .unwrap_u8()
                == 1
        })
}

fn authorization_payload<'a>(
    req: &'a http::Request<hyper::body::Incoming>,
    scheme: &str,
) -> Option<&'a str> {
    req.headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (actual_scheme, payload) = value.split_once(' ')?;
            actual_scheme
                .eq_ignore_ascii_case(scheme)
                .then_some(payload)
        })
}

/// True when the host is a normalized entry of the allowlist.
///
/// Both sides are normalized defensively so a hand-built [`AdminState`] that
/// stores `Admin.Example:9090` still matches `admin.example`.
fn allowed_host_contains(allowed_hosts: &[String], host: &str) -> bool {
    allowed_hosts
        .iter()
        .any(|entry| normalize_host_entry(entry).as_deref() == Some(host))
}

/// Validate the request `Host` header against the admin allowlist.
///
/// Only the actual `Host` header counts — `X-Forwarded-Host` and friends are
/// ignored, so a client cannot widen the allowlist with a header it controls.
/// A missing, unparseable, or unlisted host fails closed. The rejected value is
/// never echoed into the response.
pub(crate) fn host_allowed(
    req: &http::Request<hyper::body::Incoming>,
    allowed_hosts: &[String],
) -> bool {
    let Some(value) = req.headers().get(http::header::HOST) else {
        return false;
    };
    let Ok(raw) = value.to_str() else {
        return false;
    };
    normalize_host_entry(raw).is_some_and(|host| allowed_host_contains(allowed_hosts, &host))
}

/// True when the request carries no `Origin`, or carries an `Origin` whose
/// host is allowlisted.
///
/// `Origin: null` (a sandboxed or `file://` document) is rejected unless the
/// operator lists the literal entry `null` in `admin.allowed_hosts`.
pub(crate) fn request_origin_allowed(
    req: &http::Request<hyper::body::Incoming>,
    allowed_hosts: &[String],
) -> bool {
    match req.headers().get(http::header::ORIGIN) {
        None => true,
        Some(value) => origin_value_allowed(value, allowed_hosts),
    }
}

fn origin_value_allowed(value: &http::HeaderValue, allowed_hosts: &[String]) -> bool {
    let Ok(raw) = value.to_str() else {
        return false;
    };
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("null") {
        return allowed_host_contains(allowed_hosts, "null");
    }
    let Some((scheme, rest)) = raw.split_once("://") else {
        return false;
    };
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")) {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // An origin never carries userinfo; reject rather than parse it away.
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    normalize_host_entry(authority).is_some_and(|host| allowed_host_contains(allowed_hosts, &host))
}

/// True when a request body is declared as JSON.
///
/// The media type is compared case-insensitively and parameters (for example
/// `; charset=utf-8`) are ignored.
pub(crate) fn request_content_type_is_json(req: &http::Request<hyper::body::Incoming>) -> bool {
    req.headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
        })
}

impl AdminServer {
    pub async fn new(bind: &str, cancel: CancellationToken) -> Result<Self, AdminError> {
        let listener = TcpListener::bind(bind).await?;
        if let Ok(addr) = listener.local_addr() {
            if !addr.ip().is_loopback() {
                tracing::warn!(
                    "admin listener bound to non-loopback address {addr} over plaintext \
                     HTTP: status, metrics, and topology are exposed to the network and \
                     Basic/Bearer credentials travel unprotected; prefer a loopback bind, \
                     a VPN/SSH tunnel, or a TLS-terminating reverse proxy (set \
                     [admin] allowed_hosts to the proxy host name)"
                );
            }
        }
        Ok(Self { listener, cancel })
    }

    pub fn local_addr(&self) -> Result<std::net::SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }

    pub async fn run(self, mut state: AdminState) -> Result<(), AdminError> {
        // `admin.bind` is not hot-reloaded, so the effective allowlist is
        // fixed for the lifetime of this server. A state built without an
        // explicit list derives one from the address that actually bound,
        // which keeps loopback clients working without configuration while
        // still failing closed for a DNS rebinding `Host`.
        if state.allowed_hosts.is_empty() {
            state.allowed_hosts = match self.listener.local_addr() {
                Ok(addr) => derive_allowed_hosts(&addr.to_string()),
                Err(_) => vec!["localhost".to_string()],
            };
        }
        let allowed_hosts: Arc<[String]> = Arc::from(state.allowed_hosts.as_slice());

        let permits = Arc::new(Semaphore::new(MAX_ADMIN_CONNECTIONS));
        let auth_failures = Arc::new(AuthFailureLimiter::default());
        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    let (stream, addr) = result.map_err(|e| AdminError::Accept(e.to_string()))?;
                    let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                        tracing::warn!("admin connection limit ({MAX_ADMIN_CONNECTIONS}) reached; rejecting connection");
                        drop(stream);
                        continue;
                    };
                    let state = state.clone();
                    let auth_failures = auth_failures.clone();
                    let allowed_hosts = Arc::clone(&allowed_hosts);
                    tokio::spawn(async move {
                        let _permit = permit;
                        let service = service_fn(move |req| {
                            let state = state.clone();
                            let auth_failures = auth_failures.clone();
                            let allowed_hosts = Arc::clone(&allowed_hosts);
                            async move {
                                // Host validation runs before auth and before
                                // any body is read: a DNS-rebinding request is
                                // rejected regardless of credentials, and the
                                // rejected value is never echoed back.
                                if !host_allowed(&req, &allowed_hosts) {
                                    // A rejected Host also ends the
                                    // connection: keep-alive would invite a
                                    // probing client to retry on the same
                                    // socket.
                                    let mut response = build_text_response(400, "bad request");
                                    response.headers_mut().insert(
                                        http::header::CONNECTION,
                                        http::HeaderValue::from_static("close"),
                                    );
                                    return Ok::<_, std::convert::Infallible>(response);
                                }
                                let response = match state.auth.as_ref() {
                                    Some(auth) if !authorized(&req, auth) => {
                                        // Record-then-check: the Nth failure that
                                        // trips the limit is itself a 429.
                                        auth_failures.record_failure(addr.ip());
                                        let blocked_for = auth_failures.blocked_for(addr.ip());
                                        let mut response = http::Response::new(Full::new(
                                            Bytes::from_static(if blocked_for.is_some() {
                                                b"too many authentication failures"
                                            } else {
                                                b"unauthorized"
                                            }),
                                        ));
                                        *response.status_mut() = if blocked_for.is_some() {
                                            http::StatusCode::TOO_MANY_REQUESTS
                                        } else {
                                            http::StatusCode::UNAUTHORIZED
                                        };
                                        response.headers_mut().insert(
                                            http::header::WWW_AUTHENTICATE,
                                            http::HeaderValue::from_static("Bearer, Basic"),
                                        );
                                        if let Some(duration) = blocked_for {
                                            let seconds = duration.as_secs().saturating_add(1).max(1);
                                            if let Ok(value) = http::HeaderValue::from_str(&seconds.to_string()) {
                                                response.headers_mut().insert(http::header::RETRY_AFTER, value);
                                            }
                                        }
                                        response.headers_mut().insert(
                                            http::header::CONTENT_TYPE,
                                            http::HeaderValue::from_static("text/plain"),
                                        );
                                        response
                                    }
                                    _ => {
                                        auth_failures.record_success(addr.ip());
                                        handle_request(req, &state).await
                                    }
                                };
                                Ok::<_, std::convert::Infallible>(response)
                            }
                        });
                        let conn = hyper::server::conn::http1::Builder::new()
                            .serve_connection(TokioIo::new(stream), service);
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(30),
                            conn,
                        )
                        .await
                        {
                            Err(_) => {
                                tracing::debug!("admin connection timed out");
                            }
                            Ok(Err(e)) => {
                                tracing::debug!("admin connection error: {e}");
                            }
                            Ok(Ok(())) => {}
                        }
                    });
                }
                _ = self.cancel.cancelled() => {
                    break;
                }
            }
        }
        Ok(())
    }
}

/// Live data the admin server reads per request.
///
/// Implementations wrap the current `CompiledRuntimeSnapshot` so reloads are
/// reflected on the next request without restarting the admin server.
#[derive(Clone)]
pub struct AdminSnapshot {
    pub generation: u64,
    pub router: Arc<eggress_routing::Router>,
    pub pac: Option<PacConfig>,
    pub static_routes: Vec<StaticRoute>,
    pub listeners: Vec<ListenerInfo>,
}

/// Source of admin-visible live data. Implemented by the runtime so that
/// reloads immediately take effect on admin endpoints.
pub trait AdminSnapshotProvider: Send + Sync + 'static {
    fn snapshot(&self) -> AdminSnapshot;

    fn generation(&self) -> u64 {
        self.snapshot().generation
    }
}

/// A `AdminSnapshotProvider` backed by a fixed snapshot. Useful in tests
/// that exercise admin endpoints without a full runtime.
pub struct StaticAdminSnapshot {
    pub snapshot: AdminSnapshot,
}

impl AdminSnapshotProvider for StaticAdminSnapshot {
    fn snapshot(&self) -> AdminSnapshot {
        self.snapshot.clone()
    }
}

#[derive(Clone)]
pub struct AdminState {
    pub metrics: Arc<eggress_metrics::MetricsRegistry>,
    pub start_time: Instant,
    pub readiness: Arc<AtomicBool>,
    pub active_connections: Option<Arc<AtomicU64>>,
    pub provider: Arc<dyn AdminSnapshotProvider>,
    pub udp_registry: Arc<eggress_udp::registry::UdpAssociationRegistry>,
    /// Registry of reverse servers. Empty by default — populating it
    /// enables the `/-/reverse` admin route.
    pub reverse_registry: Arc<ReverseRegistry>,
    /// Whether the `/metrics` endpoint is enabled.
    pub metrics_enabled: bool,
    pub auth: Option<eggress_config::compile::AdminAuthConfig>,
    /// `Host` header values accepted by this server, normalized (lowercase,
    /// no brackets or port). Empty means "derive from the bound address when
    /// the server starts".
    pub allowed_hosts: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{AuthFailureLimiter, AUTH_FAILURE_LIMIT};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn auth_failure_limiter_blocks_and_resets() {
        let limiter = AuthFailureLimiter::default();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

        for _ in 0..AUTH_FAILURE_LIMIT {
            limiter.record_failure(ip);
        }
        assert!(limiter.blocked_for(ip).is_some());

        limiter.record_success(ip);
        assert!(limiter.blocked_for(ip).is_none());
    }
}

impl AdminState {
    pub fn snapshot(&self) -> AdminSnapshot {
        self.provider.snapshot()
    }

    pub fn generation(&self) -> u64 {
        self.provider.generation()
    }
}

pub type AdminResponse = http::Response<Full<Bytes>>;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ListenerInfo {
    pub name: String,
    pub bind: String,
    pub local_addr: String,
    pub protocols: Vec<String>,
    pub udp_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_dst_support: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unix_socket_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unix_socket_unlink_existing: Option<bool>,
}

pub fn build_response(status: u16, body: impl Into<Bytes>, content_type: &str) -> AdminResponse {
    let status = if (100..=599).contains(&status) {
        match http::StatusCode::from_u16(status) {
            Ok(status) => status,
            Err(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    } else {
        http::StatusCode::INTERNAL_SERVER_ERROR
    };
    let content_type = http::HeaderValue::from_str(content_type)
        .unwrap_or_else(|_| http::HeaderValue::from_static("application/octet-stream"));
    let mut response = http::Response::new(Full::new(body.into()));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(http::header::CONTENT_TYPE, content_type);
    response
}

pub fn build_json_response(status: u16, body: impl Into<Bytes>) -> AdminResponse {
    build_response(status, body, "application/json")
}

pub fn build_text_response(status: u16, body: impl Into<Bytes>) -> AdminResponse {
    build_response(status, body, "text/plain")
}

pub fn build_not_found() -> AdminResponse {
    build_text_response(404, "not found")
}
