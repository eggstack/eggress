//! Private listener-preparation phase for `ServiceSupervisor::run()`.
//!
//! ## Implementation map (7 run phases)
//!
//! 1. runtime-context initialization — clones supervisor handles/tokens into
//!    the async driver (`supervisor.rs::run` prelude);
//! 2. health startup — stores the Tokio handle and starts probes in-runtime;
//! 3. listener preparation — this module (`prepare_listener_set` +
//!    `publish_listener_addresses`);
//! 4. listener/admin/reverse task spawn — accept loops in `run()`, auxiliary
//!    services in `services.rs`, UDP relay prep via `udp_runtime.rs`;
//! 5. compatibility operations — `--sys` selection/application in
//!    `services.rs` (`operations` feature);
//! 6. readiness/signal run loop — `signals.rs` (`run_signal_loop`);
//! 7. ordered shutdown — `shutdown.rs::shutdown_ordered` (single authority).
//!
//! Values crossing phases are grouped per phase: preparation consumes only
//! listener configs, auth reuse, timeouts, shared state/routing, and the
//! listener cancellation token; it returns a typed [`PreparedListenerSet`].
//! No generic context bag is introduced.

use std::sync::Arc;
use std::time::Duration;

use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::ProtocolId;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "quic")]
use super::connection::PreparedQuicListener;
use super::connection::{prepare_tls_server_config, PreparedListener};
use super::state::RuntimeState;
use super::udp_runtime::make_udp_service;
use crate::error::RuntimeError;
use crate::platform::{check_capability, PlatformCapability};

/// Prepared Unix-domain listener state (named replacement for the former
/// 11-tuple in `run()`).
#[cfg(unix)]
#[allow(dead_code)]
pub(crate) struct PreparedUnixListener {
    pub(crate) name: String,
    pub(crate) listener: eggress_server::listener::unix::UnixListener,
    pub(crate) protocols: Vec<ProtocolId>,
    pub(crate) auth: eggress_server::accept::InboundAuthentication,
    pub(crate) handshake_timeout: Duration,
    pub(crate) connection_limit: u64,
    pub(crate) tls: Option<Arc<rustls::ServerConfig>>,
    pub(crate) shadowsocks: Option<eggress_config::model::ShadowsocksListenerConfig>,
    pub(crate) trojan: Option<eggress_config::model::ListenerTrojanConfig>,
    pub(crate) udp: Option<eggress_config::compile::CompiledListenerUdpConfig>,
    pub(crate) udp_service: Option<Arc<dyn eggress_server::UdpService>>,
}

/// Prepared transparent listener state (named replacement for the former
/// 11-tuple in `run()`).
#[allow(dead_code)]
pub(crate) struct PreparedTransparentListener {
    pub(crate) name: String,
    pub(crate) listener: eggress_server::listener::transparent::TransparentListener,
    pub(crate) protocols: Vec<ProtocolId>,
    pub(crate) auth: eggress_server::accept::InboundAuthentication,
    pub(crate) handshake_timeout: Duration,
    pub(crate) connection_limit: u64,
    pub(crate) tls: Option<Arc<rustls::ServerConfig>>,
    pub(crate) shadowsocks: Option<eggress_config::model::ShadowsocksListenerConfig>,
    pub(crate) trojan: Option<eggress_config::model::ListenerTrojanConfig>,
    pub(crate) udp: Option<eggress_config::compile::CompiledListenerUdpConfig>,
    pub(crate) udp_service: Option<Arc<dyn eggress_server::UdpService>>,
}

/// Typed aggregate returned by listener preparation.
///
/// Replaces the former tuple-with-many-fields pattern: standard TCP
/// listeners, feature-gated QUIC listeners, Unix listeners, and transparent
/// listeners each have explicit ownership here. Callers consume the set to
/// spawn accept loops and publish address metadata.
pub(crate) struct PreparedListenerSet {
    pub(crate) prepared: Vec<PreparedListener>,
    #[cfg(feature = "quic")]
    pub(crate) prepared_quic: Vec<PreparedQuicListener>,
    #[cfg(unix)]
    pub(crate) unix: Vec<PreparedUnixListener>,
    #[cfg(not(unix))]
    pub(crate) unix: Vec<PreparedTransparentListenerPlaceholder>,
    pub(crate) transparent: Vec<PreparedTransparentListener>,
}

/// Placeholder so `PreparedListenerSet::unix` exists on non-Unix builds
/// without carrying OS-specific listener types.
#[cfg(not(unix))]
#[derive(Default)]
pub(crate) struct PreparedTransparentListenerPlaceholder;

/// Build the inbound authentication handle for one listener config.
///
/// The facade pre-builds the typed reuse handle; runtime only threads it
/// into inbound authentication (no pproxy arg parsing here).
fn build_listener_auth(
    auth_cfg: Option<&eggress_config::model::AuthConfig>,
    reuse: Option<Arc<eggress_server::accept::AuthReuseCache>>,
) -> eggress_server::accept::InboundAuthentication {
    match auth_cfg {
        Some(cfg) => {
            if cfg.auth_type == "password" {
                let username = cfg.username.clone().unwrap_or_default();
                let password = cfg.password.clone().unwrap_or_default();
                if let Some(reuse) = reuse {
                    eggress_server::accept::InboundAuthentication::UsernamePasswordWithReuse {
                        username,
                        password,
                        reuse,
                    }
                } else {
                    eggress_server::accept::InboundAuthentication::UsernamePassword {
                        username,
                        password,
                    }
                }
            } else {
                eggress_server::accept::InboundAuthentication::None
            }
        }
        None => eggress_server::accept::InboundAuthentication::None,
    }
}

/// Prepare all listener sockets for one supervisor generation.
///
/// Owns standard TCP, Unix, transparent, and feature-gated QUIC bind logic
/// plus per-listener TLS preparation and UDP service construction. Bind
/// failures surface as startup errors before readiness; unsupported platform
/// modes retain their existing skip/fallback semantics.
pub(crate) async fn prepare_listener_set(
    listener_configs: &[eggress_config::compile::ListenerConfig],
    compatibility_auth_reuse: Option<Arc<eggress_server::accept::AuthReuseCache>>,
    handshake_timeout: Duration,
    state_ref: &Arc<RuntimeState>,
    routing: &Arc<eggress_routing::SharedRoutingService>,
    listener_cancel: &CancellationToken,
) -> Result<PreparedListenerSet, RuntimeError> {
    let mut prepared = Vec::new();
    #[cfg(feature = "quic")]
    let mut prepared_quic = Vec::<PreparedQuicListener>::new();
    #[cfg(unix)]
    let mut unix_listeners = Vec::new();
    #[cfg(not(unix))]
    let unix_listeners: Vec<PreparedTransparentListenerPlaceholder> = Vec::new();
    let mut transparent_listeners = Vec::new();

    for lcfg in listener_configs {
        let protocols: Vec<ProtocolId> = lcfg.protocols.to_vec();
        let auth = build_listener_auth(lcfg.auth.as_ref(), compatibility_auth_reuse.clone());

        let connection_limit = lcfg.connection_limit.unwrap_or(1024) as usize;
        let prepared_tls = prepare_tls_server_config(lcfg.tls.as_ref()).map_err(|error| {
            RuntimeError::Other(format!(
                "listener '{}' has invalid prepared TLS configuration: {error}",
                lcfg.name
            ))
        })?;

        // Unix domain socket listeners.
        if let Some(ref unix_cfg) = lcfg.unix {
            #[cfg(unix)]
            {
                match eggress_server::listener::unix::create_unix_listener(
                    &eggress_server::listener::unix::UnixListenerConfig::from_compiled(
                        &unix_cfg.path,
                        unix_cfg.unlink_existing,
                        Some(unix_cfg.mode),
                    ),
                ) {
                    Ok(unix_listener) => {
                        tracing::info!(
                            "unix socket listener created at {} ({})",
                            unix_cfg.path.display(),
                            lcfg.name
                        );
                        unix_listeners.push(PreparedUnixListener {
                            name: lcfg.name.clone(),
                            listener: unix_listener,
                            protocols,
                            auth,
                            handshake_timeout,
                            connection_limit: connection_limit as u64,
                            tls: prepared_tls.clone(),
                            shadowsocks: lcfg.shadowsocks.clone(),
                            trojan: lcfg.trojan.clone(),
                            udp: lcfg.udp.clone(),
                            udp_service: make_udp_service(
                                state_ref,
                                routing,
                                &lcfg.name,
                                lcfg.udp.as_ref(),
                            ),
                        });
                        continue;
                    }
                    Err(e) => {
                        tracing::error!(
                            "failed to bind unix socket at {} for listener '{}': {e}",
                            unix_cfg.path.display(),
                            lcfg.name
                        );
                        continue;
                    }
                }
            }
            #[cfg(not(unix))]
            {
                tracing::error!(
                    "unix socket listener '{}' skipped: not supported on this platform",
                    lcfg.name
                );
                continue;
            }
        }

        // Transparent TCP listeners.
        if let Some(ref transparent_cfg) = lcfg.transparent {
            if transparent_cfg.enabled {
                let capability = check_capability(PlatformCapability::LinuxOriginalDstIpv4);
                if capability != crate::platform::CapabilityStatus::Available {
                    #[cfg(feature = "operations")]
                    state_ref
                        .runtime_metrics
                        .record_platform_capability_check_failure();
                    let _cap_span = tracing::info_span!(
                        "capability_check_failed",
                        capability = %PlatformCapability::LinuxOriginalDstIpv4,
                        status = %capability,
                        listener = %lcfg.name,
                    );
                    tracing::warn!(
                        "transparent proxy not available for listener '{}' ({}); \
                         falling back to normal TCP listener",
                        lcfg.name,
                        capability
                    );
                } else {
                    let bind_addr: std::net::SocketAddr =
                        lcfg.bind.parse().map_err(|e| RuntimeError::ListenerBind {
                            addr: lcfg.bind.clone(),
                            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
                        })?;

                    let transparent_listener =
                        eggress_server::listener::transparent::TransparentListener::bind(
                            &bind_addr.to_string(),
                        )
                        .await
                        .map_err(|e| RuntimeError::ListenerBind {
                            addr: lcfg.bind.clone(),
                            source: e,
                        })?;

                    let local_addr = transparent_listener.local_addr().map_err(|e| {
                        RuntimeError::ListenerBind {
                            addr: lcfg.bind.clone(),
                            source: e,
                        }
                    })?;

                    tracing::info!(
                        "transparent TCP listener listening on {local_addr} ({})",
                        lcfg.name
                    );

                    transparent_listeners.push(PreparedTransparentListener {
                        name: lcfg.name.clone(),
                        listener: transparent_listener,
                        protocols,
                        auth,
                        handshake_timeout,
                        connection_limit: connection_limit as u64,
                        tls: prepared_tls.clone(),
                        shadowsocks: lcfg.shadowsocks.clone(),
                        trojan: lcfg.trojan.clone(),
                        udp: lcfg.udp.clone(),
                        udp_service: make_udp_service(
                            state_ref,
                            routing,
                            &lcfg.name,
                            lcfg.udp.as_ref(),
                        ),
                    });
                    continue;
                }
            }
        }

        #[cfg(feature = "quic")]
        if protocols.contains(&ProtocolId::Quic) || protocols.contains(&ProtocolId::Http3) {
            let tls = lcfg.tls.clone().ok_or_else(|| {
                RuntimeError::Other(format!(
                    "QUIC/HTTP3 listener '{}' requires certificate and key material",
                    lcfg.name
                ))
            })?;
            let bind_addr: std::net::SocketAddr =
                lcfg.bind.parse().map_err(|e| RuntimeError::ListenerBind {
                    addr: lcfg.bind.clone(),
                    source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
                })?;
            let listener = eggress_transport_quic::QuicListener::bind(
                bind_addr,
                eggress_transport_quic::QuicServerConfig {
                    certificate_pem: tls.cert_pem.clone(),
                    private_key_pem: tls.key_pem.clone(),
                    idle_timeout: Duration::from_secs(60),
                    max_concurrent_streams: lcfg.connection_limit.unwrap_or(1024).max(1),
                    alpn_protocols: if protocols.contains(&ProtocolId::Http3) {
                        vec![b"h3".to_vec()]
                    } else {
                        Vec::new()
                    },
                },
            )
            .await
            .map_err(|e| RuntimeError::ListenerBind {
                addr: lcfg.bind.clone(),
                source: std::io::Error::other(e.to_string()),
            })?;
            let local_addr = listener
                .local_addr()
                .map_err(|e| RuntimeError::ListenerBind {
                    addr: lcfg.bind.clone(),
                    source: std::io::Error::other(e.to_string()),
                })?;
            tracing::info!("QUIC listening on {local_addr} ({})", lcfg.name);
            prepared_quic.push(PreparedQuicListener {
                name: lcfg.name.clone(),
                protocols,
                listener,
                local_addr,
                auth,
                handshake_timeout,
                connection_limit: lcfg.connection_limit.unwrap_or(1024) as u64,
            });
            continue;
        }

        // Standard TCP listener path.
        let bind_addr: std::net::SocketAddr =
            lcfg.bind.parse().map_err(|e| RuntimeError::ListenerBind {
                addr: lcfg.bind.clone(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
            })?;

        let config = TcpListenerConfig {
            bind_addr,
            protocols: protocols.clone(),
            auth_required: false,
            handshake_timeout,
            connection_limit,
        };

        let listener = TcpListener::new_with_reuse_port(
            &config,
            listener_cancel.clone(),
            lcfg.reuse_port.unwrap_or(false),
        )
        .await
        .map_err(|e| RuntimeError::ListenerBind {
            addr: lcfg.bind.clone(),
            source: e,
        })?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| RuntimeError::ListenerBind {
                addr: lcfg.bind.clone(),
                source: e,
            })?;
        tracing::info!("listening on {local_addr} ({})", lcfg.name);
        let udp_service = make_udp_service(state_ref, routing, &lcfg.name, lcfg.udp.as_ref());

        prepared.push(PreparedListener {
            name: lcfg.name.clone(),
            bind: lcfg.bind.clone(),
            protocols,
            listener,
            local_addr,
            auth,
            handshake_timeout,
            udp: lcfg.udp.clone(),
            udp_service,
            tls: prepared_tls,
            shadowsocks: lcfg.shadowsocks.clone(),
            trojan: lcfg.trojan.clone(),
            fixed_target: lcfg.fixed_target.clone(),
            local_bind: lcfg.local_bind.clone(),
        });
    }

    Ok(PreparedListenerSet {
        prepared,
        #[cfg(feature = "quic")]
        prepared_quic,
        unix: unix_listeners,
        transparent: transparent_listeners,
    })
}

/// Publish listener addresses for admin snapshot, indexed by config order.
///
/// Builds a lookup map from all listener types (standard, transparent,
/// Unix, QUIC) so admin/readiness observers see startup-captured topology.
pub(crate) fn publish_listener_addresses(
    state_ref: &Arc<RuntimeState>,
    listener_configs: &[eggress_config::compile::ListenerConfig],
    set: &PreparedListenerSet,
) {
    let mut addr_map: std::collections::HashMap<String, Option<std::net::SocketAddr>> =
        std::collections::HashMap::new();
    for p in &set.prepared {
        addr_map.insert(p.name.clone(), Some(p.local_addr));
    }
    #[cfg(feature = "quic")]
    for p in &set.prepared_quic {
        addr_map.insert(p.name.clone(), Some(p.local_addr));
    }
    for p in &set.transparent {
        let addr = p.listener.local_addr().ok();
        addr_map.insert(p.name.clone(), addr);
    }
    #[cfg(unix)]
    for p in &set.unix {
        // Unix domain sockets don't have a meaningful TCP socket address.
        addr_map.insert(p.name.clone(), None);
    }
    let addrs: Vec<Option<std::net::SocketAddr>> = listener_configs
        .iter()
        .map(|lcfg| addr_map.get(&lcfg.name).copied().flatten())
        .collect();
    let admin_addrs = addrs.clone();
    match state_ref.listener_addrs.lock() {
        Ok(mut guard) => *guard = addrs,
        Err(error) => {
            tracing::warn!("listener address state was poisoned; resetting it: {error}");
            let mut guard = error.into_inner();
            *guard = addrs;
            state_ref.listener_addrs.clear_poison();
        }
    }
    #[cfg(feature = "operations")]
    state_ref.publish_admin_listener_addrs(state_ref.snapshot.load_full(), admin_addrs);
}
