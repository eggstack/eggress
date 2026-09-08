//! Runtime-side UDP service wiring.
//!
//! [`RuntimeUdpService`] implements `eggress_server::UdpService` by creating
//! associations in the shared UDP registry and spawning per-association relay
//! tasks. [`compute_advertise_ip`] derives the SOCKS5 UDP ASSOCIATE reply
//! address; [`prepare_shadowsocks_udp_relay`] builds standalone Shadowsocks
//! UDP relay state (feature `extended`).

use std::sync::Arc;

use tokio_util::task::TaskTracker;

use eggress_routing::RouteService;
use eggress_routing::SharedRoutingService;

#[cfg(feature = "extended")]
use crate::error::RuntimeError;

#[cfg(feature = "extended")]
use super::connection::PreparedListener;
use super::state::RuntimeState;

#[cfg(feature = "extended")]
pub(crate) type PreparedShadowsocksUdpRelay = (
    Arc<tokio::net::UdpSocket>,
    eggress_udp::standalone_shadowsocks::ShadowsocksStandaloneUdpConfig,
);

#[cfg(feature = "extended")]
pub(crate) async fn prepare_shadowsocks_udp_relay(
    prepared_listener: &PreparedListener,
    udp_cfg: &eggress_config::compile::CompiledListenerUdpConfig,
    routing: Arc<dyn RouteService>,
    state: &RuntimeState,
) -> Result<PreparedShadowsocksUdpRelay, RuntimeError> {
    let ss = prepared_listener.shadowsocks.as_ref().ok_or_else(|| {
        RuntimeError::Other(format!(
            "listener '{}' shadowsocks_udp mode requires shadowsocks config",
            prepared_listener.name
        ))
    })?;
    let method =
        eggress_protocol_shadowsocks::CipherMethod::parse_method(&ss.method).map_err(|e| {
            RuntimeError::Other(format!(
                "listener '{}' has invalid shadowsocks method '{}': {}",
                prepared_listener.name, ss.method, e
            ))
        })?;
    let socket = Arc::new(
        tokio::net::UdpSocket::bind(udp_cfg.bind)
            .await
            .map_err(|e| RuntimeError::ListenerBind {
                addr: udp_cfg.bind.to_string(),
                source: e,
            })?,
    );
    let local_addr = socket
        .local_addr()
        .map_err(|e| RuntimeError::ListenerBind {
            addr: udp_cfg.bind.to_string(),
            source: e,
        })?;
    tracing::info!(
        "shadowsocks UDP relay listening on {local_addr} ({})",
        prepared_listener.name
    );

    let relay_config = eggress_udp::standalone_shadowsocks::ShadowsocksStandaloneUdpConfig {
        routing,
        udp_metrics: state.udp_metrics.clone(),
        shadowsocks_metrics: Some(state.shadowsocks_metrics.clone()),
        limits: eggress_udp::limits::UdpLimits::from_listener_config(
            udp_cfg.max_associations_global,
            udp_cfg.max_associations,
            udp_cfg.max_targets_per_association,
            udp_cfg.max_datagram_size,
            udp_cfg.idle_timeout,
            udp_cfg.client_pin,
            udp_cfg.target_idle_timeout,
        ),
        listener: prepared_listener.name.clone(),
        generation: state.snapshot.load().generation,
        method,
        password: ss.password.clone(),
        allow_private_egress: udp_cfg.allow_private_egress,
    };

    Ok((socket, relay_config))
}

/// Compute the advertised IP for the SOCKS5 UDP ASSOCIATE reply.
///
/// Derivation rules:
/// 1. If `advertise` is configured, use it.
/// 2. Else if UDP bind IP is not unspecified, use UDP bind IP.
/// 3. Else if TCP peer is loopback, use loopback matching address family.
/// 4. Else return a config error requiring explicit `advertise`.
pub(crate) fn compute_advertise_ip(
    configured_advertise: Option<std::net::IpAddr>,
    udp_bind_ip: std::net::IpAddr,
    tcp_peer: Option<std::net::SocketAddr>,
) -> Result<std::net::IpAddr, eggress_udp::error::UdpError> {
    if let Some(ip) = configured_advertise {
        return Ok(ip);
    }

    if !udp_bind_ip.is_unspecified() {
        return Ok(udp_bind_ip);
    }

    if let Some(tcp_peer) = tcp_peer {
        let peer_is_loopback = tcp_peer.ip().is_loopback()
            || matches!(
                tcp_peer.ip(),
                std::net::IpAddr::V6(ipv6)
                    if ipv6
                        .to_ipv4_mapped()
                        .is_some_and(|ipv4| ipv4.is_loopback())
            );
        if peer_is_loopback {
            // Preserve the family selected by the unspecified UDP bind. This
            // matters for dual-stack sockets and IPv4-mapped IPv6 peers.
            match udp_bind_ip {
                std::net::IpAddr::V4(_) => {
                    return Ok(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
                }
                std::net::IpAddr::V6(_) => {
                    return Ok(std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST));
                }
            }
        }
    }

    Err(eggress_udp::error::UdpError::Other(
        "UDP relay requires explicit advertise address when bind is unspecified and client is not loopback".to_string()
    ))
}

pub(crate) struct RuntimeUdpService {
    pub(crate) _listener_name: String,
    pub(crate) udp_config: eggress_config::compile::CompiledListenerUdpConfig,
    pub(crate) registry: Arc<eggress_udp::registry::UdpAssociationRegistry>,
    pub(crate) udp_metrics: Arc<eggress_udp::metrics::UdpMetrics>,
    pub(crate) routing: Arc<SharedRoutingService>,
    pub(crate) udp_tasks: TaskTracker,
}

impl eggress_server::UdpService for RuntimeUdpService {
    fn create_association(
        &self,
        listener: &str,
        client_tcp_peer: Option<std::net::SocketAddr>,
        identity: eggress_core::ClientIdentity,
        generation: u64,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        eggress_server::UdpAssociationHandle,
                        eggress_udp::error::UdpError,
                    >,
                > + Send
                + 'static,
        >,
    > {
        let registry = self.registry.clone();
        let udp_metrics = self.udp_metrics.clone();
        let routing = self.routing.clone();
        let udp_tasks = self.udp_tasks.clone();
        let udp_config = self.udp_config.clone();
        let listener = listener.to_string();
        Box::pin(async move {
            let assoc = registry
                .create_association(&listener, client_tcp_peer, identity, generation)
                .await?;
            // No direct registry increment here: the spawned relay records
            // the canonical subsystem counter, promoted exactly once by the
            // metrics bridge at render time (see eggress-metrics docs).

            let relay_socket =
                std::sync::Arc::new(tokio::net::UdpSocket::bind(udp_config.bind).await?);
            let local_addr = relay_socket.local_addr()?;

            let advertised_ip =
                compute_advertise_ip(udp_config.advertise, local_addr.ip(), client_tcp_peer)?;
            let relay_addr = std::net::SocketAddr::new(advertised_ip, local_addr.port());

            let relay_config = eggress_udp::relay::RelayConfig {
                routing: routing as Arc<dyn RouteService>,
                udp_metrics: udp_metrics.clone(),
                limits: eggress_udp::limits::UdpLimits::from_listener_config(
                    udp_config.max_associations_global,
                    udp_config.max_associations,
                    udp_config.max_targets_per_association,
                    udp_config.max_datagram_size,
                    udp_config.idle_timeout,
                    udp_config.client_pin,
                    udp_config.target_idle_timeout,
                ),
                listener: listener.clone(),
                generation,
                identity: assoc.meta.identity.clone(),
                client_tcp_peer,
                registry: registry.clone(),
                allow_private_egress: udp_config.allow_private_egress,
                upstream_connect_timeout: udp_config.upstream_connect_timeout,
                upstream_udp_bind: udp_config.upstream_udp_bind,
            };

            let relay_assoc = assoc.clone();
            let relay_cancel = assoc.cancel.clone();
            let assoc_id = assoc.id;
            let relay_udp_metrics = udp_metrics.clone();
            udp_tasks.spawn(async move {
                let result = eggress_udp::relay::udp_relay_loop(
                    relay_socket,
                    relay_assoc,
                    relay_config,
                    relay_cancel,
                )
                .await;
                if let Err(error) = result {
                    relay_udp_metrics.record_association_failure();
                    tracing::warn!(
                        %error,
                        association_id = ?assoc_id,
                        "UDP relay ended with error"
                    );
                }
            });

            Ok(eggress_server::UdpAssociationHandle {
                id: assoc.id,
                relay_addr,
                cancel: assoc.cancel.clone(),
            })
        })
    }

    fn is_enabled(&self) -> bool {
        self.udp_config.enabled
    }

    fn active_count(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = usize> + Send + 'static>> {
        let registry = self.registry.clone();
        Box::pin(async move { registry.active_count().await })
    }
}

/// Build the per-listener UDP association service from startup-prepared
/// listener state, or `None` when the listener has no UDP configuration.
///
/// Shared by standard, transparent, and Unix accept paths so relay limits,
/// registry wiring, and metrics plumbing cannot drift between them.
pub(crate) fn make_udp_service(
    state: &RuntimeState,
    routing: &Arc<SharedRoutingService>,
    listener_name: &str,
    udp_config: Option<&eggress_config::compile::CompiledListenerUdpConfig>,
) -> Option<Arc<dyn eggress_server::UdpService>> {
    udp_config.map(|udp_config| {
        Arc::new(RuntimeUdpService {
            _listener_name: listener_name.to_string(),
            udp_config: udp_config.clone(),
            registry: state.udp_registry.clone(),
            udp_metrics: state.udp_metrics.clone(),
            routing: routing.clone(),
            udp_tasks: state.udp_tasks.clone(),
        }) as Arc<dyn eggress_server::UdpService>
    })
}
