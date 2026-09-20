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

use eggress_core::chain::{ChainExecutor, HopHandler};

#[cfg(feature = "pproxy-legacy")]
use crate::hops::ShadowsocksRHopHandler;
#[cfg(feature = "ssh")]
use crate::hops::SshHopHandler;
use crate::hops::{
    H2HopHandler, HttpHopHandler, HttpOnlyHopHandler, RawHopHandler, Socks4HopHandler,
    Socks5HopHandler, UnixHopHandler,
};
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
    handlers.push(Box::new(H2HopHandler));

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
    let tls_wrapper: eggress_core::chain::TlsWrapper =
        Box::new(move |stream, server_name, alpn, insecure| {
            let default = tls_wrapper_default.clone();
            let h2_cfg = tls_wrapper_h2.clone();
            let insecure_default = insecure_wrapper_default.clone();
            let insecure_h2_cfg = insecure_wrapper_h2.clone();
            Box::pin(async move {
                let config = if insecure {
                    match insecure_default.clone() {
                        Some(c) => {
                            if let Some(ref protocols) = alpn {
                                if c.alpn_protocols == *protocols {
                                    c
                                } else if let Some(h2) = insecure_h2_cfg.clone() {
                                    if *protocols == vec![b"h2".to_vec(), b"http/1.1".to_vec()] {
                                        h2
                                    } else {
                                        build_insecure_alpn_config(Some(protocols.clone()))?
                                    }
                                } else {
                                    build_insecure_alpn_config(Some(protocols.clone()))?
                                }
                            } else {
                                c
                            }
                        }
                        None => build_insecure_alpn_config(alpn)?,
                    }
                } else {
                    match default {
                        Some(c) => {
                            if let Some(ref protocols) = alpn {
                                if c.alpn_protocols == *protocols {
                                    c
                                } else if let Some(h2) = h2_cfg {
                                    if *protocols == vec![b"h2".to_vec(), b"http/1.1".to_vec()] {
                                        h2
                                    } else {
                                        build_alpn_config(Some(protocols.clone()))?
                                    }
                                } else {
                                    build_alpn_config(Some(protocols.clone()))?
                                }
                            } else {
                                c
                            }
                        }
                        None => build_alpn_config(alpn)?,
                    }
                };
                eggress_transport_tls::tls_connect(stream, config, &server_name)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) as _ })
            })
        });

    ChainExecutor::new(handlers)
        .with_tls_wrapper(tls_wrapper)
        .with_shared_tls_config(shared_tls_config)
        .with_insecure_shared_tls_config(insecure_shared_tls_config)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
