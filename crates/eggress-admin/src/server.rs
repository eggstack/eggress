use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::Instant;

use base64::Engine;
use bytes::Bytes;
use http_body_util::Full;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::reverse::ReverseRegistry;
use crate::routes::handle_request;
use crate::AdminError;
use eggress_config::compile::{PacConfig, StaticRoute};

/// Upper bound on concurrently served admin connections. The admin surface is
/// local tooling; an unbounded accept loop would otherwise turn the admin port
/// into a trivially cheap DoS target.
const MAX_ADMIN_CONNECTIONS: usize = 64;

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
        .and_then(|value| base64::engine::general_purpose::STANDARD.decode(value).ok())
        .and_then(|value| String::from_utf8(value).ok())
        .and_then(|value| {
            value
                .split_once(':')
                .map(|(user, pass)| (user.to_string(), pass.to_string()))
        })
        .is_some_and(|(user, pass)| {
            (user.as_bytes().ct_eq(username.as_bytes())
                & pass.as_bytes().ct_eq(password.as_bytes()))
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

impl AdminServer {
    pub async fn new(bind: &str, cancel: CancellationToken) -> Result<Self, AdminError> {
        let listener = TcpListener::bind(bind).await?;
        if let Ok(addr) = listener.local_addr() {
            if !addr.ip().is_loopback() {
                tracing::warn!(
                    "admin listener bound to non-loopback address {addr}: \
                     status, metrics, and topology are exposed to the network; \
                     prefer a loopback bind or configure admin auth"
                );
            }
        }
        Ok(Self { listener, cancel })
    }

    pub fn local_addr(&self) -> Result<std::net::SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }

    pub async fn run(self, state: AdminState) -> Result<(), AdminError> {
        let permits = Arc::new(Semaphore::new(MAX_ADMIN_CONNECTIONS));
        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    let (stream, _addr) = result.map_err(|e| AdminError::Accept(e.to_string()))?;
                    let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                        tracing::warn!("admin connection limit ({MAX_ADMIN_CONNECTIONS}) reached; rejecting connection");
                        drop(stream);
                        continue;
                    };
                    let state = state.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        let service = service_fn(move |req| {
                            let state = state.clone();
                            async move {
                                let response = match state.auth.as_ref() {
                                    Some(auth) if !authorized(&req, auth) => {
                                        let mut response = http::Response::new(Full::new(
                                            Bytes::from_static(b"unauthorized"),
                                        ));
                                        *response.status_mut() = http::StatusCode::UNAUTHORIZED;
                                        response.headers_mut().insert(
                                            http::header::WWW_AUTHENTICATE,
                                            http::HeaderValue::from_static("Bearer, Basic"),
                                        );
                                        response.headers_mut().insert(
                                            http::header::CONTENT_TYPE,
                                            http::HeaderValue::from_static("text/plain"),
                                        );
                                        response
                                    }
                                    _ => handle_request(req, &state).await,
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
}

impl AdminState {
    pub fn snapshot(&self) -> AdminSnapshot {
        self.provider.snapshot()
    }

    pub fn generation(&self) -> u64 {
        self.provider.snapshot().generation
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
        http::StatusCode::from_u16(status).expect("bounded admin status is valid")
    } else {
        http::StatusCode::INTERNAL_SERVER_ERROR
    };
    http::Response::builder()
        .status(status)
        .header("content-type", content_type)
        .body(Full::new(body.into()))
        .expect("admin response content type is valid")
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
