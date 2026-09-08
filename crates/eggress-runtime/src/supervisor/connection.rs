//! Prepared listener state shared by all stream listener types.
//!
//! [`PreparedListener`] captures the per-listener behavior that accept loops
//! clone into each connection task at startup. QUIC/H3 listeners use
//! [`PreparedQuicListener`]. Per-connection construction helpers live here so
//! standard, transparent, Unix, and QUIC paths share one implementation
//! instead of duplicating TLS wrapping and `ConnectionConfig` assembly.

use std::time::Duration;

#[cfg(feature = "quic")]
use std::sync::Arc;

use eggress_core::listener::TcpListener;
use eggress_core::ProtocolId;

pub(crate) struct PreparedListener {
    pub(crate) name: String,
    #[allow(dead_code)] // used only with `operations` feature
    pub(crate) bind: String,
    pub(crate) protocols: Vec<ProtocolId>,
    pub(crate) listener: TcpListener,
    pub(crate) local_addr: std::net::SocketAddr,
    pub(crate) auth: eggress_server::accept::InboundAuthentication,
    pub(crate) handshake_timeout: Duration,
    pub(crate) udp: Option<eggress_config::compile::CompiledListenerUdpConfig>,
    pub(crate) tls: Option<eggress_config::compile::CompiledListenerTlsConfig>,
    pub(crate) shadowsocks: Option<eggress_config::model::ShadowsocksListenerConfig>,
    pub(crate) trojan: Option<eggress_config::model::ListenerTrojanConfig>,
    pub(crate) fixed_target: Option<eggress_core::TargetAddr>,
    pub(crate) local_bind: Option<String>,
}

#[cfg(feature = "quic")]
pub(crate) struct PreparedQuicListener {
    pub(crate) name: String,
    pub(crate) protocols: Vec<ProtocolId>,
    pub(crate) listener: Arc<eggress_transport_quic::QuicListener>,
    pub(crate) local_addr: std::net::SocketAddr,
    pub(crate) auth: eggress_server::accept::InboundAuthentication,
    pub(crate) handshake_timeout: Duration,
    pub(crate) connection_limit: u64,
}

/// Security material cloned from prepared listener state into each connection.
pub(crate) struct InboundSecurity {
    pub(crate) shadowsocks: Option<eggress_config::model::ShadowsocksListenerConfig>,
    pub(crate) trojan: Option<eggress_config::model::ListenerTrojanConfig>,
}

/// Parameters for [`build_connection_config`], assembled once per accepted
/// connection from startup-prepared listener state plus per-connection values
/// (`conn_id` is intentionally *not* part of this struct: only some listener
/// types tag spans with it, so call sites keep ownership of counter values).
///
/// Every accepted standard/Unix/transparent stream increments the global
/// `active_connections` counter exactly once via `ActiveConnectionGuard`
/// held for the whole session; TCP listeners additionally hold a
/// `PermitStream` semaphore permit. Generation comes from the active snapshot
/// at accept time so new connections always observe one consistent snapshot.
pub(crate) struct ConnectionBuildParams {
    pub(crate) routing: std::sync::Arc<dyn eggress_routing::RouteService>,
    pub(crate) listener: String,
    pub(crate) peer: Option<std::net::SocketAddr>,
    pub(crate) generation: u64,
    pub(crate) handshake_timeout: Duration,
    pub(crate) connect_timeout: Duration,
    pub(crate) protocols: std::sync::Arc<[ProtocolId]>,
    pub(crate) authentication: eggress_server::accept::InboundAuthentication,
    pub(crate) metrics: std::sync::Arc<dyn eggress_server::SessionMetrics>,
    pub(crate) udp: Option<std::sync::Arc<dyn eggress_server::UdpService>>,
    pub(crate) tls_client_config: Option<std::sync::Arc<rustls::ClientConfig>>,
    pub(crate) security: InboundSecurity,
    pub(crate) fixed_target: Option<eggress_core::TargetAddr>,
    pub(crate) local_bind: Option<String>,
    #[cfg(feature = "extended")]
    pub(crate) shadowsocks_metrics:
        std::sync::Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>,
    #[cfg(feature = "ssh")]
    pub(crate) ssh_sessions: Option<std::sync::Arc<eggress_transport_ssh::SshSessionCache>>,
}

/// Build an `eggress_server::ConnectionConfig` from prepared listener state.
///
/// Protocol-specific differences are supplied explicitly by the caller; the
/// Shadowsocks/Trojan inbound mapping, metrics wiring, and snapshot
/// generation plumbing are shared so behavior cannot drift between listener
/// types.
pub(crate) fn build_connection_config(
    params: ConnectionBuildParams,
) -> eggress_server::ConnectionConfig {
    eggress_server::ConnectionConfig {
        routing: params.routing,
        context: eggress_server::ConnectionContext {
            source: params.peer,
            listener: params.listener,
            generation: params.generation,
        },
        handshake_timeout: params.handshake_timeout,
        connect_timeout: params.connect_timeout,
        protocols: params.protocols,
        authentication: params.authentication,
        metrics: Some(params.metrics),
        udp: params.udp,
        tls_client_config: params.tls_client_config,
        shadowsocks: params.security.shadowsocks.map(|ss| {
            eggress_server::accept::InboundShadowsocksConfig {
                method: ss.method.clone(),
                password: ss.password.clone(),
                #[cfg(feature = "pproxy-legacy")]
                auth_prefix: ss.auth_prefix.clone().map(String::into_bytes),
                #[cfg(feature = "pproxy-legacy")]
                plugins: ss.plugins.clone(),
            }
        }),
        #[cfg(feature = "extended")]
        shadowsocks_metrics: Some(params.shadowsocks_metrics),
        #[cfg(not(feature = "extended"))]
        shadowsocks_metrics: None,
        trojan: params
            .security
            .trojan
            .map(|t| eggress_server::accept::InboundTrojanConfig {
                password: t.password.clone(),
                fallback: t.fallback.clone(),
            }),
        fixed_target: params.fixed_target,
        local_bind: params.local_bind,
        #[cfg(feature = "ssh")]
        ssh_sessions: params.ssh_sessions,
    }
}

/// Apply TLS server wrapping with the listener's prepared TLS material.
///
/// Returns `None` when the connection must be dropped (invalid TLS material
/// or failed TLS accept). Call sites translate `None` into an early task
/// return, preserving the previous per-listener behavior exactly.
pub(crate) async fn wrap_tls_server(
    stream: eggress_core::BoxStream,
    tls: Option<&eggress_config::compile::CompiledListenerTlsConfig>,
    peer: std::net::SocketAddr,
) -> Option<eggress_core::BoxStream> {
    let tls_cfg = match tls {
        Some(cfg) => cfg,
        None => return Some(stream),
    };
    let server_config = match eggress_transport_tls::TlsServerConfigBuilder::new()
        .with_certificate_pem(&tls_cfg.cert_pem)
        .and_then(|b| b.with_key_pem(&tls_cfg.key_pem))
        .and_then(|b| {
            let b = if tls_cfg.alpn.is_empty() {
                b
            } else {
                b.with_alpn(tls_cfg.alpn.clone())
            };
            b.build()
        }) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(%peer, "TLS config error: {e}");
            return None;
        }
    };
    match eggress_transport_tls::tls_accept(stream, server_config).await {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::debug!(%peer, "TLS accept failed: {e}");
            None
        }
    }
}
