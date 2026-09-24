//! Concrete outbound chain-executor construction.
//!
//! This module owns the single implementation authority for the reusable
//! proxy-hop registry and TLS composition used by both listener-bound
//! sessions (`eggress-server`) and listener-free execution
//! (`OutboundConnector` in this crate).
//!
//! The generic `ChainExecutor`/`HopHandler` machinery lives in
//! `eggress-core`; this module wires the concrete protocol handlers in a
//! fixed order with shared TLS configuration.

use std::sync::Arc;

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};

use eggress_core::chain::{ChainExecutor, HopHandler};

#[cfg(feature = "pproxy-legacy")]
use crate::hops::ShadowsocksRHopHandler;
#[cfg(feature = "ssh")]
use crate::hops::SshHopHandler;
use crate::hops::{
    H2HopHandler, HttpHopHandler, HttpOnlyHopHandler, RawHopHandler, Socks4HopHandler,
    Socks5HopHandler, UnixHopHandler,
};

/// Runtime builds a ChainExecutor per listener connection, so bind H2 pools
/// to the identity of the shared TLS policy object. Retaining that Arc also
/// prevents pointer reuse from making a later, different trust policy collide.
/// The bounded cache preserves pooling across requests while separating
/// independent TLS configurations.
static H2_POOLS_BY_TLS_POLICY: LazyLock<Mutex<H2PoolScopes>> =
    LazyLock::new(|| Mutex::new(H2PoolScopes::default()));

#[derive(Default)]
struct H2PoolScopes {
    pools: HashMap<
        usize,
        (
            Arc<rustls::ClientConfig>,
            Arc<eggress_protocol_http::H2PoolRegistry>,
        ),
    >,
    order: VecDeque<usize>,
}

fn h2_pool_registry_for_tls_policy(
    tls_config: Option<&Arc<rustls::ClientConfig>>,
) -> Arc<eggress_protocol_http::H2PoolRegistry> {
    let Some(tls_config) = tls_config else {
        return Arc::new(eggress_protocol_http::H2PoolRegistry::new());
    };
    let policy_id = Arc::as_ptr(tls_config) as usize;
    let mut scopes = H2_POOLS_BY_TLS_POLICY
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some((_, registry)) = scopes.pools.get(&policy_id) {
        return registry.clone();
    }
    if scopes.order.len() >= 64 {
        if let Some(oldest) = scopes.order.pop_front() {
            scopes.pools.remove(&oldest);
        }
    }
    let registry = Arc::new(eggress_protocol_http::H2PoolRegistry::new());
    scopes
        .pools
        .insert(policy_id, (tls_config.clone(), registry.clone()));
    scopes.order.push_back(policy_id);
    registry
}

/// Clear Eggress chain H2 pool scopes, for example after runtime reload.
#[doc(hidden)]
pub fn clear_h2_pool_registries() {
    let mut scopes = H2_POOLS_BY_TLS_POLICY
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for (_, registry) in scopes.pools.values() {
        registry.clear();
    }
    scopes.pools.clear();
    scopes.order.clear();
}
#[cfg(feature = "quic")]
use crate::hops::{H3HopHandler, QuicHopHandler};
#[cfg(feature = "extended")]
use crate::hops::{ShadowsocksHopHandler, TrojanHopHandler, WebSocketHopHandler};

/// Options for outbound chain-executor construction.
///
/// Reusable, server-independent composition input: an optional TLS override,
/// optional Shadowsocks metrics sink (protocol-crate type, never
/// `eggress-metrics`), and an optional SSH session cache. No server
/// `ConnectionConfig` crosses this boundary.
#[derive(Clone, Default)]
pub struct OutboundExecutorOptions {
    /// Optional TLS client-config override for upstream hops (e.g. tests).
    /// When `None`, a system-roots config is built once and shared.
    pub tls_override: Option<Arc<rustls::ClientConfig>>,
    /// Optional Shadowsocks metrics sink (protocol-crate type).
    #[cfg(feature = "extended")]
    pub shadowsocks_metrics: Option<Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>>,
    /// Optional shared SSH session cache for SSH upstreams.
    #[cfg(feature = "ssh")]
    pub ssh_sessions: Option<Arc<eggress_transport_ssh::SshSessionCache>>,
}

impl OutboundExecutorOptions {
    /// Create default options (system TLS roots, no metrics, no SSH cache).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the TLS client-config override.
    pub fn with_tls_override(mut self, config: Arc<rustls::ClientConfig>) -> Self {
        self.tls_override = Some(config);
        self
    }

    /// Set the Shadowsocks metrics sink.
    #[cfg(feature = "extended")]
    pub fn with_shadowsocks_metrics(
        mut self,
        metrics: Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>,
    ) -> Self {
        self.shadowsocks_metrics = Some(metrics);
        self
    }

    /// Set the shared SSH session cache.
    #[cfg(feature = "ssh")]
    pub fn with_ssh_sessions(
        mut self,
        sessions: Arc<eggress_transport_ssh::SshSessionCache>,
    ) -> Self {
        self.ssh_sessions = Some(sessions);
        self
    }
}

/// Build a chain executor from owned options.
///
/// Single implementation authority for handler ordering, shared TLS
/// client-config construction, ALPN-specific TLS wrapper caching, per-hop
/// insecure behavior under its existing gated policy, SSH session-cache
/// injection, and extended/legacy/QUIC feature behavior.
pub fn build_chain_executor_with_options(options: OutboundExecutorOptions) -> ChainExecutor {
    let tls_override = options.tls_override;
    #[cfg(feature = "extended")]
    let shadowsocks_metrics = options.shadowsocks_metrics;
    #[cfg(feature = "ssh")]
    let ssh_sessions = options.ssh_sessions;
    build_chain_executor_inner(
        tls_override.as_ref(),
        #[cfg(feature = "extended")]
        shadowsocks_metrics,
        #[cfg(not(feature = "extended"))]
        None,
        #[cfg(feature = "ssh")]
        ssh_sessions,
    )
}

/// Build the concrete outbound chain executor (server-compatible signature).
///
/// Preserves handler ordering, shared TLS client-config construction,
/// ALPN-specific TLS wrapper caching, per-hop insecure behavior under its
/// existing gated policy, SSH session-cache injection, extended/legacy/QUIC
/// feature behavior, and failure behavior when TLS configuration cannot be
/// built. `eggress-server` consumes this authority rather than maintaining a
/// second registry.
pub fn build_chain_executor(
    tls_override: Option<&Arc<rustls::ClientConfig>>,
    #[cfg(feature = "extended")] shadowsocks_metrics: Option<
        Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>,
    >,
    #[cfg(not(feature = "extended"))] _shadowsocks_metrics: Option<()>,
    #[cfg(feature = "ssh")] ssh_sessions: Option<Arc<eggress_transport_ssh::SshSessionCache>>,
) -> ChainExecutor {
    build_chain_executor_inner(
        tls_override,
        #[cfg(feature = "extended")]
        shadowsocks_metrics,
        #[cfg(not(feature = "extended"))]
        _shadowsocks_metrics,
        #[cfg(feature = "ssh")]
        ssh_sessions,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_chain_executor_inner(
    tls_override: Option<&Arc<rustls::ClientConfig>>,
    #[cfg(feature = "extended")] shadowsocks_metrics: Option<
        Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>,
    >,
    #[cfg(not(feature = "extended"))] _shadowsocks_metrics: Option<()>,
    #[cfg(feature = "ssh")] ssh_sessions: Option<Arc<eggress_transport_ssh::SshSessionCache>>,
) -> ChainExecutor {
    // Build shared TLS client config for upstream hops
    let shared_tls_config = match tls_override {
        Some(config) => Some(config.clone()),
        None => match eggress_transport_tls::default_client_config() {
            Ok(config) => Some(config),
            Err(e) => {
                tracing::warn!("failed to build shared TLS config: {e}");
                None
            }
        },
    };

    #[cfg(feature = "extended")]
    let shared_tls_config_arc = shared_tls_config.clone();
    #[cfg(not(feature = "extended"))]
    let _shared_tls_config_arc = shared_tls_config.clone();

    // Per-hop `?insecure` requires an insecure verifier. Build it only when
    // the `insecure-tls` feature is available; otherwise per-hop insecure hops
    // will be rejected in `ChainExecutor::validate_chain` / `execute` with an
    // explicit error. The transport's `with_insecure` is feature-gated, so
    // `cargo test` without the feature intentionally leaves this as `None`.
    #[cfg(feature = "insecure-tls")]
    let insecure_shared_tls_config: Option<Arc<rustls::ClientConfig>> = if tls_override.is_some() {
        None
    } else {
        match eggress_transport_tls::default_insecure_client_config() {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                tracing::debug!("failed to build insecure TLS config: {e}");
                None
            }
        }
    };
    #[cfg(not(feature = "insecure-tls"))]
    let insecure_shared_tls_config: Option<Arc<rustls::ClientConfig>> = None;

    let mut handlers: Vec<Box<dyn HopHandler>> = vec![
        Box::new(HttpHopHandler),
        Box::new(HttpOnlyHopHandler),
        Box::new(Socks5HopHandler),
        Box::new(Socks4HopHandler),
    ];

    #[cfg(feature = "extended")]
    {
        handlers.push(Box::new(ShadowsocksHopHandler {
            metrics: shadowsocks_metrics,
        }));
        handlers.push(Box::new(TrojanHopHandler {
            tls_config: shared_tls_config_arc.clone(),
            insecure_tls_config: insecure_shared_tls_config.clone(),
            tls_override: tls_override.cloned(),
        }));
        handlers.push(Box::new(WebSocketHopHandler));
    }

    #[cfg(feature = "pproxy-legacy")]
    handlers.push(Box::new(ShadowsocksRHopHandler));

    handlers.push(Box::new(RawHopHandler));
    handlers.push(Box::new(UnixHopHandler));
    #[cfg(feature = "ssh")]
    if let Some(sessions) = ssh_sessions {
        handlers.push(Box::new(SshHopHandler { sessions }));
    }
    // Resolve the shared TLS-policy scope. Listener execution creates an
    // executor per route, so this preserves reuse between those executors
    // while keeping distinct ClientConfig objects in separate registries.
    handlers.push(Box::new(H2HopHandler {
        pool_registry: h2_pool_registry_for_tls_policy(shared_tls_config.as_ref()),
    }));

    #[cfg(feature = "quic")]
    {
        handlers.push(Box::new(QuicHopHandler));
        handlers.push(Box::new(H3HopHandler));
    }

    // Pre-build TLS configs per distinct ALPN set so we don't re-read
    // and re-parse system roots on every handshake (O-05).
    let tls_wrapper_default = shared_tls_config.clone();
    let tls_wrapper_h2: Option<Arc<rustls::ClientConfig>> = if tls_override.is_none() {
        match eggress_transport_tls::default_h2_client_config() {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                tracing::debug!("failed to build h2 TLS config: {e}");
                None
            }
        }
    } else {
        None
    };
    #[cfg(feature = "insecure-tls")]
    let insecure_wrapper_default = insecure_shared_tls_config.clone();
    #[cfg(not(feature = "insecure-tls"))]
    let insecure_wrapper_default: Option<Arc<rustls::ClientConfig>> = None;
    #[cfg(feature = "insecure-tls")]
    let insecure_wrapper_h2: Option<Arc<rustls::ClientConfig>> =
        if tls_override.is_none() && insecure_shared_tls_config.is_some() {
            match eggress_transport_tls::default_insecure_h2_client_config() {
                Ok(cfg) => Some(cfg),
                Err(e) => {
                    tracing::debug!("failed to build insecure h2 TLS config: {e}");
                    None
                }
            }
        } else {
            None
        };
    #[cfg(not(feature = "insecure-tls"))]
    let insecure_wrapper_h2: Option<Arc<rustls::ClientConfig>> = None;
    fn build_alpn_config(
        alpn: Option<Vec<Vec<u8>>>,
    ) -> Result<Arc<rustls::ClientConfig>, Box<dyn std::error::Error + Send + Sync>> {
        let mut builder = eggress_transport_tls::TlsClientConfigBuilder::new();
        builder = builder.with_system_roots()?;
        if let Some(protocols) = alpn {
            builder = builder.with_alpn(protocols);
        }
        Ok(builder.build()?)
    }
    #[cfg(feature = "insecure-tls")]
    fn build_insecure_alpn_config(
        alpn: Option<Vec<Vec<u8>>>,
    ) -> Result<Arc<rustls::ClientConfig>, Box<dyn std::error::Error + Send + Sync>> {
        let mut builder = eggress_transport_tls::TlsClientConfigBuilder::new();
        builder = builder.with_system_roots()?;
        builder = builder.with_insecure();
        if let Some(protocols) = alpn {
            builder = builder.with_alpn(protocols);
        }
        Ok(builder.build()?)
    }
    #[cfg(not(feature = "insecure-tls"))]
    fn build_insecure_alpn_config(
        _alpn: Option<Vec<Vec<u8>>>,
    ) -> Result<Arc<rustls::ClientConfig>, Box<dyn std::error::Error + Send + Sync>> {
        Err("insecure TLS requires the insecure-tls feature".into())
    }
    // Whether the caller supplied `tls_override`. Used to fail closed when
    // a per-hop `insecure=true` request would otherwise substitute an
    // Eggress-built insecure config (which discards the caller's trust
    // policy). Eggress chain TLS composition keeps this state local to the
    // wrapper closure so the public `build_chain_executor*` signatures
    // remain unchanged.
    let caller_supplied_override = tls_override.is_some();
    let tls_wrapper: eggress_core::chain::TlsWrapper = Box::new(
        move |stream, server_name, alpn, insecure| {
            let default = tls_wrapper_default.clone();
            let h2_cfg = tls_wrapper_h2.clone();
            let insecure_default = insecure_wrapper_default.clone();
            let insecure_h2_cfg = insecure_wrapper_h2.clone();
            Box::pin(async move {
                let config = if insecure {
                    match insecure_default.clone() {
                        Some(c) => {
                            // Eggress-owned insecure policy: adapt ALPN
                            // only via the policy-preserving helper.
                            if let Some(ref protocols) = alpn {
                                if c.alpn_protocols == *protocols {
                                    c
                                } else if let Some(h2) = insecure_h2_cfg.clone() {
                                    if *protocols == vec![b"h2".to_vec(), b"http/1.1".to_vec()] {
                                        h2
                                    } else {
                                        eggress_transport_tls::client_config_with_alpn(
                                            &c,
                                            Some(protocols.clone()),
                                        )
                                    }
                                } else {
                                    eggress_transport_tls::client_config_with_alpn(
                                        &c,
                                        Some(protocols.clone()),
                                    )
                                }
                            } else {
                                c
                            }
                        }
                        None => {
                            // No Eggress insecure config is available.
                            // When the caller supplied `tls_override`,
                            // substituting an Eggress insecure config
                            // would silently drop the caller's trust
                            // policy. Fail closed until Eggress grows an
                            // additive caller-supplied insecure override;
                            // until then, the caller must remove
                            // `insecure=true` or build their own
                            // insecure verifier and wrap it in a
                            // custom `tls_override`.
                            if caller_supplied_override {
                                return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                                    "caller-supplied tls_override cannot be combined with insecure=true; supply an explicit insecure override or remove insecure=true",
                                ));
                            }
                            build_insecure_alpn_config(alpn.clone())?
                        }
                    }
                } else {
                    match default {
                        Some(c) => {
                            // Verified policy: adapt ALPN only by
                            // cloning the existing `ClientConfig` via
                            // the policy-preserving helper. Never call
                            // `build_alpn_config` here, because that
                            // rebuilds a fresh system-roots config and
                            // discards the caller's trust/identity
                            // policy when `tls_override.is_some()`.
                            // The precomputed H2 fast-path still serves
                            // the common hop-zero H2 case without
                            // allocating.
                            if let Some(ref protocols) = alpn {
                                if c.alpn_protocols == *protocols {
                                    c
                                } else if let Some(h2) = h2_cfg.clone() {
                                    if *protocols == vec![b"h2".to_vec(), b"http/1.1".to_vec()] {
                                        h2
                                    } else {
                                        eggress_transport_tls::client_config_with_alpn(
                                            &c,
                                            Some(protocols.clone()),
                                        )
                                    }
                                } else {
                                    eggress_transport_tls::client_config_with_alpn(
                                        &c,
                                        Some(protocols.clone()),
                                    )
                                }
                            } else {
                                c
                            }
                        }
                        // No shared verified config: the upstream
                        // could never be built (typically a missing
                        // crypto provider). Fall back to building a
                        // fresh system-roots config; this branch is
                        // unreachable when `tls_override` is supplied.
                        None => build_alpn_config(alpn.clone())?,
                    }
                };
                eggress_transport_tls::tls_connect(stream, config, &server_name)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) as _ })
            })
        },
    );

    ChainExecutor::new(handlers)
        .with_tls_wrapper(tls_wrapper)
        .with_shared_tls_config(shared_tls_config)
        .with_insecure_shared_tls_config(insecure_shared_tls_config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::TargetAddr;

    #[test]
    fn default_executor_configs_share_process_cached_tls_state() {
        eggress_transport_tls::install_default_crypto_provider();
        let first = build_chain_executor_with_options(OutboundExecutorOptions::new());
        let second = build_chain_executor_with_options(OutboundExecutorOptions::new());
        assert!(std::ptr::eq(
            first.shared_tls_config().unwrap().as_ref(),
            second.shared_tls_config().unwrap().as_ref()
        ));
    }

    #[test]
    fn custom_tls_override_bypasses_process_default_cache() {
        eggress_transport_tls::install_default_crypto_provider();
        let default = eggress_transport_tls::default_client_config().unwrap();
        let custom = eggress_transport_tls::TlsClientConfigBuilder::new()
            .with_system_roots()
            .unwrap()
            .build()
            .unwrap();
        let executor = build_chain_executor_with_options(
            OutboundExecutorOptions::new().with_tls_override(custom.clone()),
        );
        let configured = executor.shared_tls_config().unwrap();
        assert!(Arc::ptr_eq(configured, &custom));
        assert!(!Arc::ptr_eq(configured, &default));
    }

    #[test]
    fn h2_pool_is_scoped_to_executor_tls_policy() {
        let trusted_policy = eggress_transport_tls::default_client_config().unwrap();
        let other_policy = eggress_transport_tls::TlsClientConfigBuilder::new()
            .with_system_roots()
            .unwrap()
            .build()
            .unwrap();
        let first_executor_scope = h2_pool_registry_for_tls_policy(Some(&trusted_policy));
        let same_policy_scope = h2_pool_registry_for_tls_policy(Some(&trusted_policy.clone()));
        let other_policy_scope = h2_pool_registry_for_tls_policy(Some(&other_policy));
        assert!(Arc::ptr_eq(&first_executor_scope, &same_policy_scope));
        assert!(!Arc::ptr_eq(&first_executor_scope, &other_policy_scope));
    }

    #[tokio::test]
    async fn h2_distinct_tls_policy_scopes_use_distinct_physical_connections() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let handshakes = Arc::new(AtomicUsize::new(0));
        let (first_client, first_server) = tokio::io::duplex(16 * 1024);
        let first_count = handshakes.clone();
        tokio::spawn(async move {
            let Ok(mut connection) = h2::server::handshake(first_server).await else {
                return;
            };
            first_count.fetch_add(1, Ordering::Relaxed);
            while let Some(Ok((_request, mut response))) = connection.accept().await {
                if response
                    .send_response(
                        http::Response::builder().status(200).body(()).unwrap(),
                        false,
                    )
                    .is_err()
                {
                    break;
                }
            }
        });

        let first_tls = eggress_transport_tls::default_client_config().unwrap();
        let second_tls = eggress_transport_tls::TlsClientConfigBuilder::new()
            .with_system_roots()
            .unwrap()
            .build()
            .unwrap();
        let first_registry = h2_pool_registry_for_tls_policy(Some(&first_tls));
        let second_registry = h2_pool_registry_for_tls_policy(Some(&second_tls));
        let pool_key = eggress_protocol_http::H2PoolKey::new(
            "127.0.0.1",
            443,
            true,
            Some("proxy.example"),
            None,
        );
        let target: TargetAddr = "target.example:443".parse().unwrap();
        let (_send, _recv, guard) = eggress_protocol_http::h2_connect_client_pooled_in_registry(
            &first_registry,
            first_client,
            &target,
            None,
            &pool_key,
        )
        .await
        .unwrap();
        drop(guard);

        let (second_client, second_server) = tokio::io::duplex(16 * 1024);
        let second_count = handshakes.clone();
        tokio::spawn(async move {
            let Ok(mut connection) = h2::server::handshake(second_server).await else {
                return;
            };
            second_count.fetch_add(1, Ordering::Relaxed);
            while let Some(Ok((_request, mut response))) = connection.accept().await {
                if response
                    .send_response(
                        http::Response::builder().status(200).body(()).unwrap(),
                        false,
                    )
                    .is_err()
                {
                    break;
                }
            }
        });
        let (_send, _recv, guard) = eggress_protocol_http::h2_connect_client_pooled_in_registry(
            &second_registry,
            second_client,
            &target,
            None,
            &pool_key,
        )
        .await
        .unwrap();
        drop(guard);

        assert_eq!(handshakes.load(Ordering::Relaxed), 2);
    }

    fn start_local_tls_h2_server(
        cert_pem: String,
        key_pem: String,
        h2_alpn: bool,
        accept_log: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        use eggress_transport_tls::TlsServerConfigBuilder;

        let mut builder = TlsServerConfigBuilder::new()
            .with_certificate_pem(cert_pem.as_bytes())
            .unwrap()
            .with_key_pem(key_pem.as_bytes())
            .unwrap();
        if h2_alpn {
            builder = builder.with_h2_alpn();
        }
        let server_config = builder.build().unwrap();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();

        let accept_log_inner = accept_log.clone();
        let handle = tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let accept_log = accept_log_inner.clone();
                let server_config = server_config.clone();
                tokio::spawn(async move {
                    let boxed: eggress_core::BoxStream = Box::new(stream);
                    let tls = match eggress_transport_tls::tls_accept(boxed, server_config).await {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    accept_log.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Ok(mut connection) = h2::server::handshake(tls).await else {
                        return;
                    };
                    while let Some(Ok((_request, mut response))) = connection.accept().await {
                        if response
                            .send_response(
                                http::Response::builder().status(200).body(()).unwrap(),
                                false,
                            )
                            .is_err()
                        {
                            break;
                        }
                    }
                });
            }
        });
        (addr, handle)
    }

    fn generate_cert_and_key(sans: &[&str]) -> (String, String) {
        let cert_params =
            rcgen::CertificateParams::new(sans.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                .unwrap();
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = cert_params.self_signed(&key_pair).unwrap();
        (cert.pem(), key_pair.serialize_pem())
    }

    #[tokio::test]
    async fn custom_ca_tls_override_survives_h2_alpn_adaptation() {
        // The H2 server presents a self-signed certificate that is not in
        // the system roots. A client config that explicitly trusts that
        // certificate as a custom CA must complete TLS+H2 against the
        // server, even though the client's `ClientConfig` does not carry
        // H2 ALPN. This proves that Eggress's outbound TLS wrapper adapts
        // the caller-supplied `ClientConfig` to the requested H2 ALPN list
        // without rebuilding a fresh system-roots config that would lose
        // the custom CA trust.
        eggress_transport_tls::install_default_crypto_provider();
        let (server_cert_pem, server_key_pem) = generate_cert_and_key(&["127.0.0.1"]);
        let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (addr, _server_handle) = start_local_tls_h2_server(
            server_cert_pem.clone(),
            server_key_pem,
            true,
            accepted.clone(),
        );

        // Client config explicitly trusts the server cert as a custom CA
        // but does NOT carry H2 ALPN. The outbound TLS wrapper must clone
        // this config and add H2 ALPN rather than synthesize a
        // system-roots config.
        let trusted_override = eggress_transport_tls::TlsClientConfigBuilder::new()
            .with_custom_ca_pem(server_cert_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();
        let executor = build_chain_executor_with_options(
            OutboundExecutorOptions::new().with_tls_override(trusted_override.clone()),
        );
        let chain = eggress_uri::parse_proxy_chain(&format!("h2+tls://{}", addr)).unwrap();
        let target: TargetAddr = "target.example:443".parse().unwrap();

        let (_stream, metadata) = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            executor.execute_with_metadata(&chain.hops, &target),
        )
        .await
        .expect("custom-CA H2 ALPN adaptation must complete within 5s")
        .expect("custom-CA H2 ALPN adaptation must succeed");
        let _ = metadata;
        assert!(
            accepted.load(std::sync::atomic::Ordering::Relaxed) >= 1,
            "the trusted server must have completed a TLS handshake"
        );
        // Caller override is preserved untouched (different Arc, same policy).
        assert!(
            std::sync::Arc::ptr_eq(executor.shared_tls_config().unwrap(), &trusted_override,),
            "the caller's tls_override Arc must be retained as the shared config"
        );
    }

    #[tokio::test]
    async fn h2_pool_does_not_cross_tls_trust_policy() {
        // Two TLS policies share endpoint/SNI/auth but trust different CAs.
        // The executor that trusts the server's self-signed cert must
        // establish its own TLS handshake successfully; the executor that
        // does NOT trust that cert must fail its own TLS handshake rather
        // than reuse the trusted executor's pooled physical connection.
        // The trust-boundary invariant is enforced by the executor-scoped
        // H2 pool registry introduced by the 1.0.10 corrective.
        eggress_transport_tls::install_default_crypto_provider();
        let (server_cert_pem, server_key_pem) = generate_cert_and_key(&["127.0.0.1"]);
        let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (addr, _server_handle) = start_local_tls_h2_server(
            server_cert_pem.clone(),
            server_key_pem,
            true,
            accepted.clone(),
        );

        // Executor A trusts the server cert via a custom CA store.
        let trusted_override = eggress_transport_tls::TlsClientConfigBuilder::new()
            .with_custom_ca_pem(server_cert_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();
        let executor_a = build_chain_executor_with_options(
            OutboundExecutorOptions::new().with_tls_override(trusted_override.clone()),
        );

        // Executor B uses system roots, which do not trust the test cert.
        let untrusted_override = eggress_transport_tls::TlsClientConfigBuilder::new()
            .with_system_roots()
            .unwrap()
            .build()
            .unwrap();
        let executor_b = build_chain_executor_with_options(
            OutboundExecutorOptions::new().with_tls_override(untrusted_override),
        );

        let chain = eggress_uri::parse_proxy_chain(&format!("h2+tls://{}", addr)).unwrap();
        let target: TargetAddr = "target.example:443".parse().unwrap();

        // Executor A: TLS handshake succeeds against the trusted cert.
        let _stream = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            executor_a.execute(&chain.hops, &target),
        )
        .await
        .expect("executor A must complete within timeout")
        .expect("executor A must succeed against the trusted cert");

        // Executor B: TLS handshake must fail certificate verification and
        // must NOT consume executor A's pooled physical connection.
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            executor_b.execute(&chain.hops, &target),
        )
        .await
        .expect("executor B must complete within timeout (with handshake error)");
        assert!(
            result.is_err(),
            "executor B must fail certificate verification, not reuse executor A's pooled connection"
        );

        // The trusted server observed at least one accepted handshake
        // (executor A). The critical invariant is that executor B's
        // failure is a TLS verification error, not a pool hit. Each
        // executor owns its pool registry, so executor B cannot see
        // executor A's pooled physical H2 connection even when they
        // share endpoint/SNI/auth fields.
        assert!(
            accepted.load(std::sync::atomic::Ordering::Relaxed) >= 1,
            "executor A must have completed its own TLS handshake"
        );
    }

    #[tokio::test]
    async fn mtls_identity_survives_h2_alpn_adaptation() {
        // mTLS: the server requires and validates a client certificate.
        // The client's `tls_override` carries the matching identity and is
        // not pre-populated with H2 ALPN. Eggress must adapt the override's
        // ALPN list without dropping the client-auth material.
        eggress_transport_tls::install_default_crypto_provider();
        let (client_cert_pem, client_key_pem) = generate_cert_and_key(&["127.0.0.1"]);
        let (server_cert_pem, server_key_pem) =
            generate_cert_and_key(&["127.0.0.1", "proxy.example"]);
        let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (addr, _server_handle) = start_local_tls_h2_server(
            server_cert_pem.clone(),
            server_key_pem,
            true,
            accepted.clone(),
        );
        // NOTE: The shared `TlsServerConfigBuilder::with_require_client_cert`
        // helper is not yet wired into `start_local_tls_h2_server`, so this
        // test exercises the structural mTLS preservation path via the
        // outbound TLS wrapper with an mTLS `tls_override` against a TLS
        // server. We do not require the server to validate the client cert;
        // that stronger behavior is covered by the unit-level
        // `client_config_with_alpn_preserves_mtls_identity` regression in
        // `eggress-transport-tls`, which guarantees `ClientConfig::clone()`
        // preserves client-auth material through ALPN adaptation.
        let mtls_override = eggress_transport_tls::TlsClientConfigBuilder::new()
            .with_custom_ca_pem(server_cert_pem.as_bytes())
            .unwrap()
            .with_client_cert_pem(client_cert_pem.as_bytes(), client_key_pem.as_bytes())
            .build()
            .unwrap();
        let executor = build_chain_executor_with_options(
            OutboundExecutorOptions::new().with_tls_override(mtls_override.clone()),
        );
        let chain = eggress_uri::parse_proxy_chain(&format!("h2+tls://{}", addr)).unwrap();
        let target: TargetAddr = "target.example:443".parse().unwrap();
        let _stream = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            executor.execute(&chain.hops, &target),
        )
        .await
        .expect("mTLS H2 ALPN adaptation must complete within 5s")
        .expect("mTLS H2 ALPN adaptation must succeed");
        assert!(
            accepted.load(std::sync::atomic::Ordering::Relaxed) >= 1,
            "the server must have completed a TLS handshake with the mTLS client"
        );
        assert!(
            std::sync::Arc::ptr_eq(executor.shared_tls_config().unwrap(), &mtls_override,),
            "the caller's mTLS tls_override Arc must be retained as the shared config"
        );
    }

    #[tokio::test]
    async fn tls_override_plus_insecure_fails_closed() {
        // A caller-supplied `tls_override` combined with a hop requesting
        // `insecure=true` would otherwise substitute an Eggress-built
        // insecure config, discarding the caller's trust policy. Eggress
        // fails closed: when the `insecure-tls` feature is not enabled,
        // chain validation rejects `insecure=true` outright. When the
        // feature is enabled, the outbound TLS wrapper rejects the
        // `tls_override + insecure=true` combination explicitly so the
        // caller's trust policy is never silently substituted.
        eggress_transport_tls::install_default_crypto_provider();
        let (server_cert_pem, server_key_pem) = generate_cert_and_key(&["127.0.0.1"]);
        let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (addr, _server_handle) = start_local_tls_h2_server(
            server_cert_pem.clone(),
            server_key_pem,
            true,
            accepted.clone(),
        );
        let trusted_override = eggress_transport_tls::TlsClientConfigBuilder::new()
            .with_custom_ca_pem(server_cert_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();
        let executor = build_chain_executor_with_options(
            OutboundExecutorOptions::new().with_tls_override(trusted_override),
        );
        // Build a chain with `?insecure=true` on the H2 hop.
        let chain =
            eggress_uri::parse_proxy_chain(&format!("h2+tls://{}?insecure=true", addr)).unwrap();
        let target: TargetAddr = "target.example:443".parse().unwrap();
        let result = executor.execute(&chain.hops, &target).await;
        assert!(
            result.is_err(),
            "tls_override + insecure=true must fail closed rather than silently substituting"
        );
        let err = match result {
            Err(e) => format!("{}", e),
            Ok(_) => String::new(),
        };
        // Without the `insecure-tls` feature, chain validation rejects
        // the chain outright with a `requires the insecure-tls feature`
        // diagnostic. With the feature enabled, the wrapper fails closed
        // with a `tls_override cannot be combined with insecure=true`
        // diagnostic. Either way, the failure must surface an `insecure`
        // marker so the caller recognizes the policy boundary.
        assert!(
            err.contains("insecure"),
            "error must reference `insecure` to be a useful configuration diagnostic, got: {err}"
        );
        assert_eq!(
            accepted.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "no TLS handshake must be attempted when the override+insecure combination is rejected"
        );
    }

    #[cfg(feature = "insecure-tls")]
    #[tokio::test]
    async fn tls_override_plus_insecure_fails_closed_with_insecure_tls_feature() {
        // With the `insecure-tls` feature enabled, the chain validation
        // accepts `insecure=true`; the outbound TLS wrapper is the line
        // of defense and must reject the `tls_override + insecure=true`
        // combination explicitly.
        eggress_transport_tls::install_default_crypto_provider();
        let (server_cert_pem, server_key_pem) = generate_cert_and_key(&["127.0.0.1"]);
        let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (addr, _server_handle) = start_local_tls_h2_server(
            server_cert_pem.clone(),
            server_key_pem,
            true,
            accepted.clone(),
        );
        let trusted_override = eggress_transport_tls::TlsClientConfigBuilder::new()
            .with_custom_ca_pem(server_cert_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();
        let executor = build_chain_executor_with_options(
            OutboundExecutorOptions::new().with_tls_override(trusted_override),
        );
        let chain =
            eggress_uri::parse_proxy_chain(&format!("h2+tls://{}?insecure=true", addr)).unwrap();
        let target: TargetAddr = "target.example:443".parse().unwrap();
        let result = executor.execute(&chain.hops, &target).await;
        assert!(
            result.is_err(),
            "tls_override + insecure=true must fail closed even with the insecure-tls feature"
        );
        let err = match result {
            Err(e) => format!("{}", e),
            Ok(_) => String::new(),
        };
        assert!(
            err.contains("tls_override") && err.contains("insecure"),
            "wrapper error must name both `tls_override` and `insecure`, got: {err}"
        );
        assert_eq!(
            accepted.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "no TLS handshake must be attempted when the wrapper rejects the override+insecure combination"
        );
    }
}
