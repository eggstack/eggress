//! Private auxiliary-service startup phase for `ServiceSupervisor::run()`.
//!
//! Owns reverse server/client spawning and admin pre-bind/spawn plus the
//! narrow `--sys` compatibility system-proxy application. Failures that the
//! baseline surfaced synchronously before readiness remain synchronous here;
//! nothing is hidden behind detached tasks.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::state::RuntimeState;
use crate::error::RuntimeError;

/// Select the local HTTP/SOCKS5 listener for `--sys` (SOCKS5 preferred).
#[cfg(feature = "operations")]
fn select_compatibility_proxy(
    prepared: &[super::connection::PreparedListener],
) -> Option<(eggress_system_proxy::CompatibilityProxyKind, u16)> {
    use eggress_core::ProtocolId;
    prepared
        .iter()
        .find(|listener| {
            listener.protocols.contains(&ProtocolId::Socks5) && listener.local_addr.port() != 0
        })
        .map(|listener| {
            (
                eggress_system_proxy::CompatibilityProxyKind::Socks5,
                listener.local_addr.port(),
            )
        })
        .or_else(|| {
            prepared
                .iter()
                .find(|listener| {
                    listener.protocols.contains(&ProtocolId::Http)
                        && listener.local_addr.port() != 0
                })
                .map(|listener| {
                    (
                        eggress_system_proxy::CompatibilityProxyKind::Http,
                        listener.local_addr.port(),
                    )
                })
        })
}

/// Apply the `--sys` OS proxy hook after bind but before accept loops.
///
/// Native startup (`None`) never mutates OS state. Failures remain startup
/// errors before readiness, matching the baseline ordering.
#[cfg(feature = "operations")]
pub(crate) fn apply_compatibility_proxy(
    prepared: &[super::connection::PreparedListener],
    compatibility_hooks: &Option<super::CompatibilityRuntimeHooks>,
) -> Result<Option<eggress_system_proxy::AppliedProxy>, RuntimeError> {
    if compatibility_hooks
        .as_ref()
        .and_then(|hooks| hooks.system_proxy)
        .is_none()
    {
        return Ok(None);
    }
    let selected = select_compatibility_proxy(prepared).ok_or_else(|| {
        RuntimeError::Other("--sys requires a usable local HTTP or SOCKS5 listener".to_string())
    })?;
    let address = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        selected.1,
    );
    Ok(Some(
        eggress_system_proxy::apply_compatibility_proxy(selected.0, address)
            .map_err(RuntimeError::Other)?,
    ))
}

/// Spawn reverse servers and clients (feature `reverse`).
///
/// Configuration is cloned from the startup-captured snapshot; routing stays
/// an authorization gate via `RouteEngineTargetResolver`. Validation failures
/// skip the offending server at startup, preserving baseline behavior.
#[cfg(feature = "reverse")]
pub(crate) fn spawn_reverse_services(
    snapshot: &Arc<arc_swap::ArcSwap<crate::snapshot::CompiledRuntimeSnapshot>>,
    routing: &Arc<eggress_routing::SharedRoutingService>,
    state_ref: &Arc<RuntimeState>,
    tasks: &TaskTracker,
    cancel: &CancellationToken,
) {
    let current_snapshot = snapshot.load();
    let reverse_servers = current_snapshot.reverse_servers.clone();
    let reverse_clients = current_snapshot.reverse_clients.clone();
    drop(current_snapshot);

    for rs_cfg in reverse_servers {
        if rs_cfg.pproxy_compat {
            let server_config =
                eggress_protocol_reverse::compat_pproxy::PproxyBackwardServerConfig {
                    control_bind: rs_cfg.control_bind,
                    external_bind: rs_cfg.external_bind,
                    auth: eggress_protocol_reverse::compat_pproxy::raw_auth(
                        rs_cfg.auth_username.as_deref(),
                        rs_cfg.auth_password.as_deref(),
                    ),
                    max_control_connections: rs_cfg.max_control_connections as usize,
                    max_pending_external: rs_cfg.max_pending_external as usize,
                    read_timeout_ms: rs_cfg.read_timeout_ms,
                    socks5_target: None,
                    client_framing:
                        eggress_protocol_reverse::compat_pproxy::PproxyBackwardFraming::Raw,
                };
            let server =
                eggress_protocol_reverse::compat_pproxy::PproxyBackwardServer::new(server_config);
            let server_cancel = server.cancel_token();
            let cancel_clone = cancel.clone();
            tasks.spawn(async move {
                let result = tokio::select! {
                    r = server.run() => r,
                    _ = cancel_clone.cancelled() => {
                        server_cancel.cancel();
                        Ok(())
                    }
                };
                if let Err(e) = result {
                    tracing::error!(error = %e, "pproxy backward server error");
                }
            });
            continue;
        }
        let server_tls =
            rs_cfg
                .tls
                .as_ref()
                .map(|t| eggress_protocol_reverse::tls::ReverseServerTlsConfig {
                    cert_pem: t.cert_pem.clone(),
                    key_pem: t.key_pem.clone(),
                    client_ca_pem: t.client_ca_pem.clone(),
                    require_client_cert: t.require_client_cert,
                });
        let server_config = eggress_protocol_reverse::server::ReverseServerConfig {
            control_bind: rs_cfg.control_bind,
            external_bind: Some(rs_cfg.external_bind),
            auth_username: rs_cfg.auth_username.clone(),
            auth_password: rs_cfg.auth_password.clone(),
            max_control_connections: rs_cfg.max_control_connections,
            read_timeout_ms: rs_cfg.read_timeout_ms,
            allow_bind: rs_cfg.allow_bind.clone(),
            max_listeners_per_client: rs_cfg.max_listeners_per_client,
            max_streams_per_listener: rs_cfg.max_streams_per_listener,
            max_pending_external: rs_cfg.max_pending_external,
            tls: server_tls,
        };
        if let Err(e) = server_config.validate() {
            tracing::error!(
                server_id = %rs_cfg.id,
                error = %e,
                "reverse server configuration validation failed; skipping",
            );
            continue;
        }
        let mut server = eggress_protocol_reverse::server::ReverseServer::new(server_config);
        server.set_metrics(state_ref.reverse_metrics.clone());
        let server_state = server.state_handle();
        let server_cancel = server.cancel_token();

        state_ref
            .reverse_registry
            .register(eggress_admin::ReverseServerEntry {
                id: eggress_admin::ReverseServerId::from(rs_cfg.id.as_str()),
                control_bind: rs_cfg.control_bind.to_string(),
                state: server_state,
            });

        let cancel_clone = cancel.clone();
        tasks.spawn(async move {
            let result = tokio::select! {
                r = server.run() => r,
                _ = cancel_clone.cancelled() => {
                    server_cancel.cancel();
                    Ok(())
                }
            };
            if let Err(e) = result {
                tracing::error!(error = %e, "reverse server error");
            }
        });
    }

    for rc_cfg in reverse_clients {
        let host = rc_cfg
            .default_target_host
            .clone()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let port = rc_cfg.default_target_port.unwrap_or(0);

        let parallel = rc_cfg.parallel_connections.max(1);
        for conn_idx in 0..parallel {
            if rc_cfg.pproxy_compat {
                let client_config =
                    eggress_protocol_reverse::compat_pproxy::PproxyBackwardClientConfig {
                        server_addr: rc_cfg.server_addr,
                        server_chain: rc_cfg.server_chain.clone(),
                        auth: eggress_protocol_reverse::compat_pproxy::raw_auth(
                            rc_cfg.auth_username.as_deref(),
                            rc_cfg.auth_password.as_deref(),
                        ),
                        reconnect_initial_ms: rc_cfg.reconnect_initial_ms,
                        reconnect_max_ms: rc_cfg.reconnect_max_ms,
                        read_timeout_ms: rc_cfg.read_timeout_ms,
                        target_connect_timeout_ms: 10_000,
                        server_framing:
                            eggress_protocol_reverse::compat_pproxy::PproxyBackwardFraming::Raw,
                    };
                let client = eggress_protocol_reverse::compat_pproxy::PproxyBackwardClient::new(
                    client_config,
                    std::sync::Arc::new(crate::reverse::RouteEngineTargetResolver::new(
                        routing.clone(),
                        host.clone(),
                        port,
                        std::sync::Arc::from(rc_cfg.id.as_str()),
                        Some(rc_cfg.server_addr),
                    )),
                );
                let cancel_clone = cancel.clone();
                let client_cancel = client.cancel_token();
                let client_id = rc_cfg.id.clone();
                let server_addr = rc_cfg.server_addr;
                tasks.spawn(async move {
                    let result = tokio::select! {
                        r = client.run() => r,
                        _ = cancel_clone.cancelled() => {
                            client_cancel.cancel();
                            Ok(())
                        }
                    };
                    if let Err(e) = result {
                        tracing::error!(error = %e, client_id = %client_id, server = %server_addr, conn = conn_idx, "pproxy backward client error");
                    }
                });
                continue;
            }
            let client_tls = rc_cfg.tls.as_ref().map(|t| {
                eggress_protocol_reverse::tls::ReverseClientTlsConfig {
                    ca_pem: t.ca_pem.clone(),
                    server_name: t.server_name.clone(),
                    client_cert_pem: t.client_cert_pem.clone(),
                    client_key_pem: t.client_key_pem.clone(),
                }
            });
            let client_config = eggress_protocol_reverse::client::ReverseClientConfig {
                server_addr: rc_cfg.server_addr,
                auth_username: rc_cfg.auth_username.clone(),
                auth_password: rc_cfg.auth_password.clone(),
                reconnect_initial_ms: rc_cfg.reconnect_initial_ms,
                reconnect_max_ms: rc_cfg.reconnect_max_ms,
                default_target_host: rc_cfg.default_target_host.clone(),
                default_target_port: rc_cfg.default_target_port,
                read_timeout_ms: rc_cfg.read_timeout_ms,
                drain_grace_ms: rc_cfg.drain_grace_ms,
                target_connect_timeout_ms: 10_000,
                tls: client_tls,
            };
            let mut client = eggress_protocol_reverse::client::ReverseClient::new(client_config);
            client.set_metrics(state_ref.reverse_metrics.clone());

            let resolver = crate::reverse::RouteEngineTargetResolver::new(
                routing.clone(),
                host.clone(),
                port,
                std::sync::Arc::from(rc_cfg.id.as_str()),
                Some(rc_cfg.server_addr),
            );
            client.set_resolver(std::sync::Arc::new(resolver));

            let cancel_clone = cancel.clone();
            let client_cancel = client.cancel_token();
            let client_id = rc_cfg.id.clone();
            let server_addr = rc_cfg.server_addr;

            tasks.spawn(async move {
                let result = tokio::select! {
                    r = client.run() => r,
                    _ = cancel_clone.cancelled() => {
                        client_cancel.cancel();
                        Ok(())
                    }
                };
                if let Err(e) = result {
                    tracing::error!(error = %e, client_id = %client_id, server = %server_addr, conn = conn_idx, "reverse client error");
                }
            });
        }
    }
}

/// Pre-bind the admin listener before readiness and spawn its task.
///
/// Bind failures are startup errors (before readiness), preserving the
/// baseline invariant. The admin task stops last via the ordered shutdown
/// path.
#[cfg(feature = "operations")]
pub(crate) async fn prebind_and_spawn_admin(
    admin_config: Option<eggress_config::compile::AdminConfig>,
    admin_cancel: &CancellationToken,
    admin_tasks: &TaskTracker,
    state_ref: &Arc<RuntimeState>,
    metrics_registry: &Arc<eggress_metrics::MetricsRegistry>,
    provider: Arc<super::operations::RuntimeAdminListenerInfos>,
) -> Result<(), RuntimeError> {
    let pre_bound_admin = if let Some(ref cfg) = admin_config {
        if cfg.enabled {
            let bind = cfg.bind.clone();
            let token = admin_cancel.clone();
            match eggress_admin::AdminServer::new(&bind, token).await {
                Ok(s) => Some(s),
                Err(e) => {
                    return Err(RuntimeError::ListenerBind {
                        addr: bind,
                        source: std::io::Error::new(std::io::ErrorKind::AddrInUse, e.to_string()),
                    });
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    if let (Some(server), Some(cfg)) = (pre_bound_admin, admin_config.as_ref()) {
        let metrics_enabled = cfg.metrics;
        let state = state_ref.clone();
        let provider: Arc<dyn eggress_admin::AdminSnapshotProvider> = provider;
        if let Ok(addr) = server.local_addr() {
            match state.admin_local_addr.lock() {
                Ok(mut guard) => *guard = Some(addr),
                Err(error) => {
                    tracing::warn!(
                        "admin listener address state was poisoned; resetting it: {error}"
                    );
                    let mut guard = error.into_inner();
                    *guard = Some(addr);
                    state.admin_local_addr.clear_poison();
                }
            }
        }
        let admin_auth = cfg.auth.clone();
        let metrics = metrics_registry.clone();
        admin_tasks.spawn(async move {
            let admin_state = eggress_admin::AdminState {
                metrics,
                start_time: state.start_time,
                readiness: state.readiness.clone(),
                active_connections: Some(state.active_connections.clone()),
                provider,
                udp_registry: state.udp_registry.clone(),
                #[cfg(feature = "reverse")]
                reverse_registry: state.reverse_registry.clone(),
                #[cfg(not(feature = "reverse"))]
                reverse_registry: std::sync::Arc::new(eggress_admin::ReverseRegistry::new()),
                metrics_enabled,
                auth: admin_auth,
            };
            if let Err(e) = server.run(admin_state).await {
                tracing::error!("admin server error: {e}");
            }
        });
    }
    Ok(())
}
