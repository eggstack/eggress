//! Listener compilation: TCP/TLS/UDP/transparent/Unix.
//!
//! Reuses `validate/` modules; no validation is duplicated here.

use eggress_core::ProtocolId;

use crate::error::ConfigError;
use crate::model::{ConfigFile, ListenerUdpConfig};
use crate::validate::validate_duration;

use super::model::*;
use super::resolve_password;
use super::rules::compile_protocol;

pub(crate) fn compile_listeners(config: &ConfigFile) -> Result<Vec<ListenerConfig>, ConfigError> {
    let listeners = match &config.listeners {
        Some(l) => l,
        None => return Ok(vec![]),
    };

    listeners
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let path = format!("listeners[{}]", i);

            let protocols: Vec<ProtocolId> = l
                .protocols
                .iter()
                .map(|p| compile_protocol(p))
                .collect::<Result<Vec<_>, _>>()?;

            if protocols.is_empty() {
                return Err(ConfigError::validation(
                    &path,
                    "protocols must not be empty",
                ));
            }

            if protocols.contains(&ProtocolId::ShadowsocksR) {
                let ssr = l.ssr.as_ref().ok_or_else(|| {
                    ConfigError::validation(
                        &format!("{}.ssr", path),
                        "ssr protocol requires an [listeners.ssr] section",
                    )
                })?;
                for (plugin_index, plugin) in ssr.plugins.iter().enumerate() {
                    if !matches!(
                        plugin.as_str(),
                        "plain"
                            | "origin"
                            | "http_simple"
                            | "tls1.2_ticket_auth"
                            | "verify_simple"
                            | "verify_deflate"
                    ) {
                        return Err(ConfigError::validation(
                            &format!("{}.ssr.plugins[{}]", path, plugin_index),
                            "unknown pproxy plugin; expected plain, origin, http_simple, tls1.2_ticket_auth, verify_simple, or verify_deflate",
                        ));
                    }
                }
            }

            let udp = match (l.udp_enabled, l.udp.as_ref()) {
                (None, None) => None,
                (None, Some(udp_cfg)) => {
                    Some(compile_listener_udp_config(udp_cfg, &protocols, &path)?)
                }
                (Some(true), None) => Some(compile_listener_udp_defaults(&protocols, &path)?),
                (Some(true), Some(udp_cfg)) => {
                    Some(compile_listener_udp_config(udp_cfg, &protocols, &path)?)
                }
                (Some(false), None) => None,
                (Some(false), Some(udp_cfg)) => {
                    if udp_cfg.enabled.unwrap_or(true) {
                        return Err(ConfigError::validation(
                            &path,
                            "udp_enabled = false conflicts with [listeners.udp] enabled = true",
                        ));
                    }
                    Some(compile_listener_udp_config(udp_cfg, &protocols, &path)?)
                }
            };

            if let Some(ref udp_cfg) = udp {
                if udp_cfg.mode == eggress_udp::UdpMode::ShadowsocksUdp {
                    let ss = l.shadowsocks.as_ref().ok_or_else(|| {
                        ConfigError::validation(
                            &path,
                            "shadowsocks_udp mode requires [listeners.shadowsocks] section with method and password",
                        )
                    })?;
                    if ss.method.is_empty() {
                        return Err(ConfigError::validation(
                            &format!("{}.shadowsocks.method", path),
                            "shadowsocks method must not be empty",
                        ));
                    }
                    if ss.password.is_empty() {
                        return Err(ConfigError::validation(
                            &format!("{}.shadowsocks.password", path),
                            "shadowsocks password must not be empty",
                        ));
                    }
                }
            }

            let tls = match l.tls.as_ref() {
                Some(tls_cfg) => {
                    let cert_pem = std::fs::read(&tls_cfg.cert).map_err(|e| {
                        ConfigError::validation(
                            &format!("{}.tls.cert", path),
                            &format!("failed to read cert file: {}", e),
                        )
                    })?;
                    let key_pem = std::fs::read(&tls_cfg.key).map_err(|e| {
                        ConfigError::validation(
                            &format!("{}.tls.key", path),
                            &format!("failed to read key file: {}", e),
                        )
                    })?;
                    // Validate PEM at compile time
                    let mut builder = eggress_transport_tls::TlsServerConfigBuilder::new()
                        .with_certificate_pem(&cert_pem)
                        .and_then(|b| b.with_key_pem(&key_pem));
                    if let Some(ref alpn) = tls_cfg.alpn {
                        let alpn_bytes: Vec<Vec<u8>> =
                            alpn.iter().map(|s| s.as_bytes().to_vec()).collect();
                        builder = builder.map(|b| b.with_alpn(alpn_bytes));
                    }
                    builder.map_err(|e| {
                        ConfigError::validation(
                            &format!("{}.tls", path),
                            &format!("invalid TLS config: {}", e),
                        )
                    })?;
                    let alpn = tls_cfg
                        .alpn
                        .as_ref()
                        .map(|protocols| protocols.iter().map(|p| p.as_bytes().to_vec()).collect())
                        .unwrap_or_default();
                    Some(CompiledListenerTlsConfig {
                        cert_pem,
                        key_pem,
                        alpn,
                    })
                }
                None => None,
            };

            if protocols.contains(&ProtocolId::Quic) || protocols.contains(&ProtocolId::Http3) {
                if tls.is_none() {
                    return Err(ConfigError::validation(
                        &format!("{}.tls", path),
                        "QUIC/HTTP3 listeners require certificate and key material",
                    ));
                }
                if l.unix.is_some() || l.transparent.as_ref().is_some_and(|t| t.enabled.unwrap_or(false)) {
                    return Err(ConfigError::validation(
                        &path,
                        "QUIC/HTTP3 listeners cannot use unix or transparent listener modes",
                    ));
                }
                let application_protocols = protocols
                    .iter()
                    .filter(|protocol| !matches!(protocol, ProtocolId::Quic | ProtocolId::Http3))
                    .count();
                if protocols.contains(&ProtocolId::Http3) && protocols.len() != 1 {
                    return Err(ConfigError::validation(
                        &format!("{}.protocols", path),
                        "HTTP/3 listeners must use exactly the h3 protocol",
                    ));
                }
                if protocols.contains(&ProtocolId::Quic) && application_protocols == 0 {
                    return Err(ConfigError::validation(
                        &format!("{}.protocols", path),
                        "raw QUIC listeners require an application protocol such as http or socks5",
                    ));
                }
                if udp.is_some() {
                    return Err(ConfigError::validation(
                        &format!("{}.udp", path),
                        "QUIC and HTTP/3 listeners do not provide UDP association mode",
                    ));
                }
                if l.fixed_target.is_some() && protocols.contains(&ProtocolId::Http3) {
                    return Err(ConfigError::validation(
                        &format!("{}.fixed_target", path),
                        "HTTP/3 CONNECT uses the request authority instead of a fixed target",
                    ));
                }
            }

            let transparent = compile_transparent_config(l.transparent.as_ref())?;

            let unix = compile_unix_listener_config(l.unix.as_ref())?;
            let fixed_target = l.fixed_target.as_deref().map(|value| value.parse().map_err(|e: String| ConfigError::validation(&format!("{}.fixed_target", path), &e))).transpose()?;

            let auth = l.auth.as_ref().map(|a| -> Result<_, ConfigError> {
                let resolved_password = resolve_password(
                    a.password.as_deref(),
                    a.password_env.as_deref(),
                    &path,
                )?;
                Ok(crate::model::AuthConfig {
                    auth_type: a.auth_type.clone(),
                    username: a.username.clone(),
                    password: resolved_password,
                    password_env: None,
                })
            })
            .transpose()?;

            Ok(ListenerConfig {
                name: l.name.clone(),
                bind: l.bind.clone(),
                protocols,
                reuse_port: l.reuse_port,
                connection_limit: l.connection_limit,
                auth,
                udp,
                tls,
                shadowsocks: l.shadowsocks.clone().or_else(|| {
                    l.ssr.as_ref().map(|ssr| crate::model::ShadowsocksListenerConfig {
                        method: "ssr".to_string(),
                        password: String::new(),
                        auth_prefix: ssr.auth_prefix.clone(),
                        plugins: ssr.plugins.clone(),
                    })
                }),
                trojan: l.trojan.clone(),
                transparent,
                unix,
                fixed_target,
                local_bind: l.local_bind.clone(),
            })
        })
        .collect()
}

/// Compile default UDP config when `udp_enabled = true` but no `[listeners.udp]` section.
pub(crate) fn compile_listener_udp_defaults(
    protocols: &[ProtocolId],
    path: &str,
) -> Result<CompiledListenerUdpConfig, ConfigError> {
    if !protocols.contains(&ProtocolId::Socks5) {
        return Err(ConfigError::validation(
            path,
            "udp_enabled = true requires socks5 protocol",
        ));
    }
    Ok(CompiledListenerUdpConfig::default())
}

/// Compile a `[listeners.udp]` section into `CompiledListenerUdpConfig`.
pub(crate) fn compile_listener_udp_config(
    udp: &ListenerUdpConfig,
    protocols: &[ProtocolId],
    path: &str,
) -> Result<CompiledListenerUdpConfig, ConfigError> {
    let defaults = CompiledListenerUdpConfig::default();
    let udp_path = format!("{}.udp", path);

    let mode = match udp.mode.as_deref() {
        Some("standalone_pproxy_udp") | Some("standalone") => {
            eggress_udp::UdpMode::StandalonePproxyUdp
        }
        Some("shadowsocks_udp") | Some("shadowsocks") => eggress_udp::UdpMode::ShadowsocksUdp,
        Some("echo") => eggress_udp::UdpMode::Echo,
        Some("fixed_target") | Some("fixed-target") => eggress_udp::UdpMode::FixedTarget,
        Some("socks5_udp_associate") | Some("socks5") | None => {
            eggress_udp::UdpMode::Socks5UdpAssociate
        }
        Some(other) => {
            return Err(ConfigError::validation(
                &format!("{}.mode", udp_path),
                &format!(
                    "unknown UDP mode '{}'; expected 'socks5_udp_associate', 'standalone_pproxy_udp', 'shadowsocks_udp', or 'echo'",
                    other
                ),
            ));
        }
    };

    if mode == eggress_udp::UdpMode::Socks5UdpAssociate && !protocols.contains(&ProtocolId::Socks5)
    {
        return Err(ConfigError::validation(
            path,
            "UDP config requires socks5 protocol",
        ));
    }

    if mode == eggress_udp::UdpMode::Echo && !protocols.contains(&ProtocolId::Echo) {
        return Err(ConfigError::validation(
            path,
            "echo UDP mode requires echo protocol",
        ));
    }

    let fixed_target = udp
        .fixed_target
        .as_deref()
        .map(|value| {
            value.parse().map_err(|e: String| {
                ConfigError::validation(&format!("{}.udp.fixed_target", path), &e)
            })
        })
        .transpose()?;
    if mode == eggress_udp::UdpMode::FixedTarget && fixed_target.is_none() {
        return Err(ConfigError::validation(
            &format!("{}.udp.fixed_target", path),
            "fixed_target UDP mode requires a target",
        ));
    }

    if mode == eggress_udp::UdpMode::ShadowsocksUdp {
        let has_ss_section = protocols.contains(&ProtocolId::Shadowsocks);
        if !has_ss_section {
            return Err(ConfigError::validation(
                path,
                "shadowsocks_udp mode requires shadowsocks protocol",
            ));
        }
        if !udp.client_pin.unwrap_or(true) {
            return Err(ConfigError::validation(
                &format!("{}.udp.client_pin", path),
                "shadowsocks_udp mode requires client_pin = true for security",
            ));
        }
    }

    let enabled = udp.enabled.unwrap_or(defaults.enabled);
    if !enabled {
        return Ok(CompiledListenerUdpConfig {
            enabled: false,
            ..defaults
        });
    }

    let bind_str = udp.bind.as_deref().unwrap_or("127.0.0.1:0");
    let bind: std::net::SocketAddr = bind_str.parse().map_err(|_| {
        ConfigError::validation(
            &format!("{}.bind", udp_path),
            &format!("invalid socket address: {}", bind_str),
        )
    })?;

    let advertise = match &udp.advertise {
        Some(addr_str) => {
            let ip: std::net::IpAddr = addr_str.parse().map_err(|_| {
                ConfigError::validation(
                    &format!("{}.advertise", udp_path),
                    &format!("invalid IP address: {}", addr_str),
                )
            })?;
            Some(ip)
        }
        None => None,
    };

    let idle_timeout = udp
        .idle_timeout
        .as_deref()
        .map(validate_duration)
        .transpose()
        .map_err(|e| {
            ConfigError::validation(&format!("{}.idle_timeout", udp_path), &e.to_string())
        })?
        .unwrap_or(defaults.idle_timeout);

    let target_idle_timeout = udp
        .target_idle_timeout
        .as_deref()
        .map(validate_duration)
        .transpose()
        .map_err(|e| {
            ConfigError::validation(&format!("{}.target_idle_timeout", udp_path), &e.to_string())
        })?
        .unwrap_or(defaults.target_idle_timeout);

    let max_associations = udp.max_associations.unwrap_or(defaults.max_associations);
    if max_associations == 0 {
        return Err(ConfigError::validation(
            &format!("{}.max_associations", udp_path),
            "must be greater than 0",
        ));
    }

    let max_targets_per_association = udp
        .max_targets_per_association
        .unwrap_or(defaults.max_targets_per_association);
    if max_targets_per_association == 0 {
        return Err(ConfigError::validation(
            &format!("{}.max_targets_per_association", udp_path),
            "must be greater than 0",
        ));
    }

    let max_datagram_size = udp.max_datagram_size.unwrap_or(defaults.max_datagram_size);
    if !(257..=65535).contains(&max_datagram_size) {
        return Err(ConfigError::validation(
            &format!("{}.max_datagram_size", udp_path),
            &format!("must be between 257 and 65535, got {}", max_datagram_size),
        ));
    }

    let client_pin = udp.client_pin.unwrap_or(defaults.client_pin);

    let allow_private_egress = udp
        .allow_private_egress
        .unwrap_or(defaults.allow_private_egress);

    let max_associations_global = udp
        .max_associations_global
        .unwrap_or(defaults.max_associations_global);
    if max_associations_global == 0 {
        return Err(ConfigError::validation(
            &format!("{}.max_associations_global", udp_path),
            "must be greater than 0",
        ));
    }

    let upstream_connect_timeout = udp
        .upstream_connect_timeout
        .as_deref()
        .map(validate_duration)
        .transpose()
        .map_err(|e| {
            ConfigError::validation(
                &format!("{}.upstream_connect_timeout", udp_path),
                &e.to_string(),
            )
        })?
        .unwrap_or(defaults.upstream_connect_timeout);

    let upstream_udp_bind_str = udp.upstream_udp_bind.as_deref().unwrap_or("127.0.0.1:0");
    let upstream_udp_bind: std::net::SocketAddr = upstream_udp_bind_str.parse().map_err(|_| {
        ConfigError::validation(
            &format!("{}.upstream_udp_bind", udp_path),
            &format!("invalid socket address: {}", upstream_udp_bind_str),
        )
    })?;

    Ok(CompiledListenerUdpConfig {
        mode,
        enabled,
        bind,
        advertise,
        idle_timeout,
        target_idle_timeout,
        max_associations,
        max_targets_per_association,
        max_datagram_size,
        client_pin,
        allow_private_egress,
        max_associations_global,
        fixed_target,
        upstream_connect_timeout,
        upstream_udp_bind,
    })
}

pub(crate) fn compile_transparent_config(
    config: Option<&crate::model::TransparentConfig>,
) -> Result<Option<CompiledTransparentConfig>, ConfigError> {
    let Some(cfg) = config else {
        return Ok(None);
    };

    let enabled = cfg.enabled.unwrap_or(false);
    let protocol = cfg.protocol.as_deref().unwrap_or("redir").to_string();

    match protocol.as_str() {
        "redir" | "pf" => {}
        other => {
            return Err(ConfigError::validation(
                "transparent.protocol",
                &format!(
                    "unknown transparent protocol '{}'; expected 'redir' or 'pf'",
                    other
                ),
            ));
        }
    }

    Ok(Some(CompiledTransparentConfig { enabled, protocol }))
}

pub(crate) fn compile_unix_listener_config(
    config: Option<&crate::model::UnixListenerConfig>,
) -> Result<Option<CompiledUnixListenerConfig>, ConfigError> {
    let Some(cfg) = config else {
        return Ok(None);
    };

    let path = std::path::PathBuf::from(&cfg.path);
    if path.parent().is_none() || path.parent() == Some(std::path::Path::new("")) {
        return Err(ConfigError::validation(
            "unix.path",
            &format!(
                "socket path must be absolute or have a valid parent directory: {}",
                cfg.path
            ),
        ));
    }

    let unlink_existing = cfg.unlink_existing.unwrap_or(true);
    let mode = cfg.mode.unwrap_or(0o660);

    Ok(Some(CompiledUnixListenerConfig {
        path,
        unlink_existing,
        mode,
    }))
}
