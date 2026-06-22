use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use http_body_util::Full;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::routes::handle_request;
use crate::AdminError;

pub struct AdminServer {
    pub(crate) listener: TcpListener,
    cancel: CancellationToken,
}

impl AdminServer {
    pub async fn new(bind: &str, cancel: CancellationToken) -> Result<Self, AdminError> {
        let listener = TcpListener::bind(bind).await?;
        Ok(Self { listener, cancel })
    }

    pub fn local_addr(&self) -> Result<std::net::SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }

    pub async fn run(self, state: AdminState) -> Result<(), AdminError> {
        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    let (stream, _addr) = result.map_err(|e| AdminError::Accept(e.to_string()))?;
                    let state = state.clone();
                    tokio::spawn(async move {
                        let service = service_fn(move |req| {
                            let state = state.clone();
                            async move { Ok::<_, std::convert::Infallible>(handle_request(req, &state).await) }
                        });
                        if let Err(e) = hyper::server::conn::http1::Builder::new()
                            .serve_connection(TokioIo::new(stream), service)
                            .await
                        {
                            tracing::debug!("admin connection error: {e}");
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

#[derive(Debug, Clone)]
pub struct StaticRoute {
    pub path: String,
    pub content_type: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct PacConfig {
    pub path: String,
    pub proxy_directive: String,
    pub direct_fallback: bool,
    pub direct_hosts: Vec<String>,
    pub direct_suffixes: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ListenerInfo {
    pub name: String,
    pub bind: String,
    pub local_addr: String,
    pub protocols: Vec<String>,
    pub udp_enabled: bool,
}

pub fn build_response(status: u16, body: impl Into<Bytes>, content_type: &str) -> AdminResponse {
    http::Response::builder()
        .status(status)
        .header("content-type", content_type)
        .body(Full::new(body.into()))
        .unwrap()
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
