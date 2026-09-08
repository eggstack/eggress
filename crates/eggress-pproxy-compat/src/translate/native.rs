//! Native compilation: intermediates become `ConfigFile`/`RuntimeConfig` and
//! native [`eggress_uri::ProxyChainSpec`] chains without a TOML round trip.
//!
//! Field-for-field agreement with the TOML renderer keeps the two paths
//! equivalent (see `native_equivalence` tests).

use super::intermediates::{
    build_chain_config_uri, build_intermediates, validate_pproxy_plugins, TranslationIntermediates,
};
use super::model::MatchToml;
use crate::args::PproxyArgs;
use crate::uri::{PproxyChain, PproxyUri};

/// Direct native compilation of a pproxy chain to a native [`eggress_uri::ProxyChainSpec`]
/// without TOML serialization.
///
/// This is the canonical outbound path: semantic translation produces a typed
/// native chain, while TOML rendering remains only for `--dump-config` /
/// migration / debugging. Validation (backward roles, unsupported hops,
/// plugins, local-bind, schemes) mirrors `translate_from_uris` remote handling
/// so direct and TOML-render/reparse paths agree on warnings/unsupported and
/// redaction.
pub fn compile_chain_to_native(
    chain: &PproxyChain,
) -> Result<eggress_uri::ProxyChainSpec, crate::error::CompatError> {
    use crate::error::CompatError;

    if chain.hops.iter().any(|hop| hop.is_backward()) {
        return Err(CompatError::UnsupportedFeature {
            feature: "backward-upstream",
            detail: format!(
                "pproxy chain '{}' uses a backward (+in) role which cannot execute outbound",
                chain.redacted_display()
            ),
        });
    }

    let unsupported = crate::uri::validate_chain_hops(chain);
    if !unsupported.is_empty() {
        let roles = unsupported
            .iter()
            .map(|(_, scheme)| scheme.clone())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(CompatError::UnsupportedFeature {
            feature: "chain-unsupported-hop",
            detail: format!(
                "pproxy chain '{}' contains unsupported hop role(s): {}",
                chain.redacted_display(),
                roles
            ),
        });
    }

    for hop in &chain.hops {
        if let Some(bind) = hop.local_bind.as_deref() {
            if hop.scheme == "unix" {
                return Err(CompatError::UnsupportedFeature {
                    feature: "local-bind",
                    detail: format!(
                        "local bind '{bind}' cannot be applied to Unix upstream '{}'",
                        hop.redacted_display()
                    ),
                });
            }
            if bind.parse::<std::net::IpAddr>().is_err() {
                return Err(CompatError::UnsupportedFeature {
                    feature: "local-bind",
                    detail: format!(
                        "local bind '{bind}' must be an IP address for upstream '{}'",
                        hop.redacted_display()
                    ),
                });
            }
        }
        if let Err(error) = validate_pproxy_plugins(&hop.plugins) {
            return Err(CompatError::UnsupportedFeature {
                feature: "plugin",
                detail: error,
            });
        }
        match hop.scheme.as_str() {
            "ss" | "shadowsocks" | "ssr" => {}
            "http" | "https" | "httponly" | "socks4" | "socks4a" | "socks5" | "trojan"
            | "direct" | "h2" | "h3" | "quic" | "quic+http" | "http+quic" | "ws" | "wss"
            | "raw" | "tunnel" => {}
            "ssh" if cfg!(feature = "ssh") => {}
            "ssh" => {
                return Err(CompatError::UnsupportedFeature {
                    feature: "ssh-upstream",
                    detail: format!(
                        "SSH upstream '{}': SSH transport is not supported",
                        hop.redacted_display()
                    ),
                });
            }
            "unix" => {}
            "redir" => {
                return Err(CompatError::UnsupportedFeature {
                    feature: "redir-upstream",
                    detail: format!(
                        "Redir upstream '{}': transparent proxy redirect is not supported as upstream",
                        hop.redacted_display()
                    ),
                });
            }
            other => {
                return Err(CompatError::UnsupportedFeature {
                    feature: "scheme",
                    detail: format!("unknown scheme '{other}' in upstream URI"),
                });
            }
        }
    }

    let native_uri = build_chain_config_uri(chain);
    eggress_uri::parse_proxy_chain(&native_uri).map_err(|error| CompatError::ConfigValidation {
        message: format!(
            "native chain parse failed for '{}': {error}",
            chain.redacted_display()
        ),
    })
}

/// Native translation result for service/listener mode.
///
/// Semantic translation produces this typed result; TOML rendering
/// (`translate_from_uris` / `TranslationOutput.toml`) is a presentation of the
/// same intermediates for `--dump-config` / migration / debugging.
#[derive(Debug, Clone)]
pub struct NativeTranslation {
    /// Compiled runtime configuration ready for embed/runtime startup.
    pub runtime: eggress_config::compile::RuntimeConfig,
    /// Canonical typed findings, identical to the TOML-render path.
    pub issues: Vec<crate::issues::CompatIssue>,
}

/// Direct native compilation for listener/service mode without TOML string.
///
/// Builds the same intermediates as `translate_from_uris` via the shared
/// `build_intermediates` builder, converts them to `ConfigFile` via direct
/// struct mapping (no `toml::to_string` / `toml::from_str` round trip), then
/// validates and compiles to `RuntimeConfig`. Returns warnings/unsupported
/// identical to the TOML path.
impl NativeTranslation {
    /// Whether translation has no blockers.
    pub fn has_unsupported(&self) -> bool {
        use crate::issues::IssueSeverity;
        self.issues
            .iter()
            .any(|i| i.severity == IssueSeverity::Unsupported)
    }

    /// Legacy warning view: all `Warning`-severity issues, in order.
    pub fn warnings(&self) -> Vec<crate::warnings::CompatWarning> {
        self.issues.iter().filter_map(|i| i.to_warning()).collect()
    }

    /// Legacy unsupported view: all `Unsupported`-severity issues, in order.
    pub fn unsupported(&self) -> Vec<crate::warnings::UnsupportedFeature> {
        self.issues
            .iter()
            .filter_map(|i| i.to_unsupported())
            .collect()
    }
}

pub fn translate_to_runtime_config(
    args: &PproxyArgs,
    local_uris: &[PproxyUri],
    remote_chains: &[PproxyChain],
) -> Result<NativeTranslation, crate::error::CompatError> {
    let (intermediates, output) = build_intermediates(args, local_uris, remote_chains)?;
    let config_file = intermediates_to_config_file(&intermediates);
    // Same validation boundary as file-backed startup: structural validate
    // then compile. Security warnings are not needed for internal consumers;
    // translator warnings/unsupported are already in `output`.
    eggress_config::validate::validate_config(&config_file).map_err(|errors| {
        let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        crate::error::CompatError::ConfigValidation {
            message: messages.join("; "),
        }
    })?;
    let runtime = eggress_config::compile::compile_config(&config_file).map_err(|error| {
        crate::error::CompatError::ConfigValidation {
            message: format!("native compile of translated config failed: {error}"),
        }
    })?;
    Ok(NativeTranslation {
        runtime,
        issues: output.issues,
    })
}

/// Convert shared intermediates to a native `ConfigFile` without TOML string.
///
/// Field-for-field mapping preserves the exact semantics of the TOML renderer
/// (`generate_toml`): omitted TOML keys become `None`, empty upstream-group
/// strings become `None`, `any = false` stays explicit, and match trees are
/// rebuilt recursively. Any divergence surfaces as a validation/compile error
/// here rather than silent behavior change.
pub(crate) fn intermediates_to_config_file(
    intermediates: &TranslationIntermediates,
) -> eggress_config::model::ConfigFile {
    use eggress_config::model;

    let listeners = intermediates
        .listeners
        .iter()
        .map(|listener| model::ListenerConfig {
            name: listener.name.clone(),
            bind: listener.bind.clone(),
            protocols: listener.protocols.clone(),
            reuse_port: listener.reuse_port,
            connection_limit: None,
            auth: listener.auth.as_ref().map(|auth| model::AuthConfig {
                auth_type: auth.r#type.clone(),
                username: auth.username.clone(),
                password: auth.password.clone(),
                password_env: None,
            }),
            udp_enabled: None,
            udp: listener.udp.as_ref().map(|udp| model::ListenerUdpConfig {
                enabled: None,
                mode: udp.mode.clone(),
                bind: udp.bind.clone(),
                advertise: None,
                idle_timeout: None,
                target_idle_timeout: None,
                max_associations: None,
                max_targets_per_association: None,
                max_datagram_size: None,
                client_pin: None,
                allow_private_egress: None,
                max_associations_global: None,
                fixed_target: udp.fixed_target.clone(),
                upstream_connect_timeout: None,
                upstream_udp_bind: None,
            }),
            tls: listener.tls.as_ref().map(|tls| model::ListenerTlsConfig {
                cert: tls.cert.clone(),
                // TOML renderer omits missing keys; model requires `key`.
                // Translators only emit TLS when `--ssl` supplied cert+key, so
                // empty here would fail compile exactly as TOML parse would.
                key: tls.key.clone().unwrap_or_default(),
                alpn: tls.alpn.clone(),
            }),
            shadowsocks: listener
                .shadowsocks
                .as_ref()
                .map(|ss| model::ShadowsocksListenerConfig {
                    method: ss.method.clone(),
                    password: ss.password.clone(),
                    auth_prefix: None,
                    plugins: Vec::new(),
                }),
            ssr: listener.ssr.as_ref().map(|ssr| model::SsrListenerConfig {
                auth_prefix: ssr.auth_prefix.clone(),
                plugins: ssr.plugins.clone(),
            }),
            trojan: listener
                .trojan
                .as_ref()
                .map(|trojan| model::ListenerTrojanConfig {
                    password: trojan.password.clone(),
                    fallback: None,
                }),
            transparent: listener.transparent.as_ref().map(|transparent| {
                model::TransparentConfig {
                    enabled: Some(transparent.enabled),
                    protocol: Some(transparent.protocol.clone()),
                }
            }),
            unix: listener
                .unix
                .as_ref()
                .map(|unix| model::UnixListenerConfig {
                    path: unix.path.clone(),
                    unlink_existing: Some(unix.unlink_existing),
                    mode: None,
                }),
            fixed_target: listener.fixed_target.clone(),
            local_bind: listener.local_bind.clone(),
        })
        .collect();

    let upstreams = intermediates
        .upstreams
        .iter()
        .map(|upstream| model::UpstreamConfig {
            id: upstream.id.clone(),
            uri: upstream.uri.clone(),
            health: upstream
                .health
                .as_ref()
                .map(|health| model::HealthConfigToml {
                    mode: None,
                    interval: Some(health.interval.clone()),
                    timeout: None,
                    failures_to_unhealthy: None,
                    successes_to_healthy: None,
                    initial_state: None,
                }),
            h2: None,
        })
        .collect();

    let upstream_groups = intermediates
        .upstream_groups
        .iter()
        .map(|group| model::UpstreamGroupConfig {
            id: group.id.clone(),
            scheduler: Some(group.scheduler.clone()),
            members: group.members.clone(),
            fallback: Some(group.fallback.clone()),
        })
        .collect();

    fn convert_match(match_toml: &MatchToml) -> model::MatchExprConfig {
        use eggress_config::model;
        if !match_toml.any_of.is_empty() {
            let any_of = match_toml.any_of.iter().map(convert_match).collect();
            model::MatchExprConfig::Composite(model::CompositeMatcher {
                all: None,
                any_of: Some(any_of),
                not: None,
            })
        } else {
            model::MatchExprConfig::Leaf(Box::new(model::LeafMatcher {
                host_exact: None,
                host_suffix: None,
                host_regex: match_toml.host_regex.clone(),
                destination_port_regex: match_toml.destination_port_regex.clone(),
                destination_port: None,
                destination_port_range: None,
                destination_port_set: None,
                destination_cidr: None,
                source_cidr: None,
                source_port: None,
                listener: None,
                protocol: None,
                identity: None,
                transport: match_toml.transport.clone(),
                reverse_listener: None,
            }))
        }
    }

    let rules = intermediates
        .rules
        .iter()
        .map(|rule| model::RuleConfig {
            id: rule.id.clone(),
            host_exact: None,
            host_suffix: None,
            host_regex: rule.host_regex.clone(),
            destination_port_regex: None,
            destination_port: None,
            any: Some(rule.any),
            match_expr: rule.r#match.as_ref().map(convert_match),
            direct: rule.direct,
            upstream_group: if rule.upstream_group.is_empty() {
                None
            } else {
                Some(rule.upstream_group.clone())
            },
            reject: rule.reject.clone(),
        })
        .collect();

    let reverse_servers = intermediates
        .reverse_servers
        .iter()
        .map(|server| model::ReverseServerConfig {
            id: server.id.clone(),
            control_bind: server.control_bind.clone(),
            external_bind: server.external_bind.clone(),
            auth_username: server.auth_username.clone(),
            auth_password: server.auth_password.clone(),
            auth_password_env: None,
            max_streams: None,
            heartbeat_interval: None,
            pproxy_compat: server.pproxy_compat,
        })
        .collect();

    let reverse_clients = intermediates
        .reverse_clients
        .iter()
        .map(|client| model::ReverseClientConfig {
            id: client.id.clone(),
            server_addr: client.server_addr.clone(),
            server_uri: client.server_uri.clone(),
            auth_username: client.auth_username.clone(),
            auth_password: client.auth_password.clone(),
            auth_password_env: None,
            reconnect_initial: None,
            reconnect_max: None,
            heartbeat_interval: None,
            parallel_connections: client.parallel_connections,
            default_target_host: None,
            default_target_port: None,
            pproxy_compat: client.pproxy_compat,
        })
        .collect();

    let admin = if intermediates.pac_enabled || !intermediates.static_content.is_empty() {
        Some(model::AdminConfig {
            bind: None,
            enabled: None,
            metrics: None,
            auth: None,
            pac: Some(model::PacConfigToml {
                path: intermediates.pac_path.clone(),
                // Preserve renderer behavior verbatim (including the known
                // literal directive) so direct and TOML paths stay equivalent.
                proxy: "PROXY {}".to_string(),
                direct_fallback: Some(true),
                direct_hosts: None,
                direct_suffixes: None,
            }),
            static_content: Some(
                intermediates
                    .static_content
                    .iter()
                    .map(|content| model::StaticContentToml {
                        path: content.path.clone(),
                        content_type: None,
                        body: Some(content.body.clone()),
                    })
                    .collect(),
            ),
        })
    } else {
        None
    };

    model::ConfigFile {
        version: Some(1),
        process: None,
        timeouts: None,
        listeners: Some(listeners),
        upstreams: Some(upstreams),
        upstream_groups: Some(upstream_groups),
        rules: Some(rules),
        rules_file: None,
        routing: None,
        admin,
        reverse_servers: Some(reverse_servers),
        reverse_clients: Some(reverse_clients),
    }
}
