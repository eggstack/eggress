//! Reverse server/client + TLS compilation.

use crate::error::ConfigError;
use crate::model::ConfigFile;
use crate::validate::validate_duration;

use super::model::*;
use super::resolve_password;

pub(crate) fn compile_reverse_servers(
    config: &ConfigFile,
) -> Result<Vec<CompiledReverseServerConfig>, ConfigError> {
    let servers = match &config.reverse_servers {
        Some(s) => s,
        None => return Ok(vec![]),
    };

    servers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let path = format!("reverse_servers[{}]", i);

            let control_bind: std::net::SocketAddr = s.control_bind.parse().map_err(|_| {
                ConfigError::validation(
                    &format!("{}.control_bind", path),
                    &format!("invalid socket address: {}", s.control_bind),
                )
            })?;

            let external_bind: std::net::SocketAddr = s.external_bind.parse().map_err(|_| {
                ConfigError::validation(
                    &format!("{}.external_bind", path),
                    &format!("invalid socket address: {}", s.external_bind),
                )
            })?;

            let auth_password = resolve_password(
                s.auth_password.as_deref(),
                s.auth_password_env.as_deref(),
                &path,
            )?;

            if s.auth_username.is_some() != auth_password.is_some() {
                return Err(ConfigError::validation(
                    &path,
                    "reverse server auth requires both auth_username and auth_password",
                ));
            }

            let max_streams = s.max_streams.unwrap_or(1024);

            let heartbeat_interval_ms = s
                .heartbeat_interval
                .as_deref()
                .map(|h| validate_duration(h).map(|d| d.as_millis() as u64))
                .transpose()
                .map_err(|e| {
                    ConfigError::validation(&format!("{}.heartbeat_interval", path), &e.to_string())
                })?
                .unwrap_or(300_000);

            if s.pproxy_compat && s.tls.is_some() {
                return Err(ConfigError::validation(
                    &format!("{}.tls", path),
                    "reverse TLS is not supported with pproxy_compat (wire must remain byte-compatible plaintext)",
                ));
            }

            let tls = compile_reverse_server_tls(s.tls.as_ref(), &path)?;

            Ok(CompiledReverseServerConfig {
                id: s.id.clone(),
                control_bind,
                external_bind,
                auth_username: s.auth_username.clone(),
                auth_password,
                max_control_connections: 256,
                read_timeout_ms: heartbeat_interval_ms,
                allow_bind: None,
                max_listeners_per_client: 1,
                max_streams_per_listener: max_streams,
                max_pending_external: 1024,
                pproxy_compat: s.pproxy_compat,
                tls,
            })
        })
        .collect()
}

pub(crate) fn compile_reverse_server_tls(
    tls: Option<&crate::model::ReverseServerTlsConfig>,
    path: &str,
) -> Result<Option<CompiledReverseServerTls>, ConfigError> {
    let tls = match tls {
        Some(t) => t,
        None => return Ok(None),
    };
    if tls.require_client_cert && tls.client_ca.is_none() {
        return Err(ConfigError::validation(
            &format!("{}.tls", path),
            "reverse server mTLS requires client_ca when require_client_cert is set",
        ));
    }
    let cert_pem = std::fs::read(&tls.cert).map_err(|e| {
        ConfigError::validation(
            &format!("{}.tls.cert", path),
            &format!("failed to read cert file: {}", e),
        )
    })?;
    let key_pem = std::fs::read(&tls.key).map_err(|e| {
        ConfigError::validation(
            &format!("{}.tls.key", path),
            &format!("failed to read key file: {}", e),
        )
    })?;
    let client_ca_pem = tls
        .client_ca
        .as_deref()
        .map(|p| {
            std::fs::read(p).map_err(|e| {
                ConfigError::validation(
                    &format!("{}.tls.client_ca", path),
                    &format!("failed to read client CA file: {}", e),
                )
            })
        })
        .transpose()?;
    // Validate PEM at compile time via the shared TLS builders so malformed
    // material fails before any listener binds.
    {
        let mut builder = eggress_transport_tls::TlsServerConfigBuilder::new()
            .with_certificate_pem(&cert_pem)
            .and_then(|b| b.with_key_pem(&key_pem));
        if let Some(ref ca_pem) = client_ca_pem {
            builder = builder.and_then(|b| b.with_client_ca_pem(ca_pem));
        }
        if tls.require_client_cert {
            builder = builder.map(|b| b.with_require_client_cert(true));
        }
        builder.map_err(|e| {
            ConfigError::validation(
                &format!("{}.tls", path),
                &format!("invalid TLS config: {}", e),
            )
        })?;
    }
    Ok(Some(CompiledReverseServerTls {
        cert_pem,
        key_pem,
        client_ca_pem,
        require_client_cert: tls.require_client_cert,
    }))
}

pub(crate) fn compile_reverse_clients(
    config: &ConfigFile,
) -> Result<Vec<CompiledReverseClientConfig>, ConfigError> {
    let clients = match &config.reverse_clients {
        Some(c) => c,
        None => return Ok(vec![]),
    };

    clients
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let path = format!("reverse_clients[{}]", i);

            let server_addr: std::net::SocketAddr = c.server_addr.parse().map_err(|_| {
                ConfigError::validation(
                    &format!("{}.server_addr", path),
                    &format!("invalid socket address: {}", c.server_addr),
                )
            })?;
            let server_chain = c
                .server_uri
                .as_deref()
                .map(eggress_uri::parse_proxy_chain)
                .transpose()
                .map_err(|error| {
                    ConfigError::validation(
                        &format!("{}.server_uri", path),
                        &format!("invalid backward chain: {error}"),
                    )
                })?;

            let auth_password = resolve_password(
                c.auth_password.as_deref(),
                c.auth_password_env.as_deref(),
                &path,
            )?;

            let reconnect_initial_ms = c
                .reconnect_initial
                .as_deref()
                .map(|d| validate_duration(d).map(|dur| dur.as_millis() as u64))
                .transpose()
                .map_err(|e| {
                    ConfigError::validation(&format!("{}.reconnect_initial", path), &e.to_string())
                })?
                .unwrap_or(1_000);

            let reconnect_max_ms = c
                .reconnect_max
                .as_deref()
                .map(|d| validate_duration(d).map(|dur| dur.as_millis() as u64))
                .transpose()
                .map_err(|e| {
                    ConfigError::validation(&format!("{}.reconnect_max", path), &e.to_string())
                })?
                .unwrap_or(30_000);

            let heartbeat_interval_ms = c
                .heartbeat_interval
                .as_deref()
                .map(|d| validate_duration(d).map(|dur| dur.as_millis() as u64))
                .transpose()
                .map_err(|e| {
                    ConfigError::validation(&format!("{}.heartbeat_interval", path), &e.to_string())
                })?
                .unwrap_or(60_000);

            let parallel_connections = c.parallel_connections.unwrap_or(1);

            if c.default_target_host.is_none() || c.default_target_port.is_none() {
                return Err(ConfigError::validation(
                    &path,
                    "reverse client requires default_target_host and default_target_port",
                ));
            }

            if c.pproxy_compat && c.tls.is_some() {
                return Err(ConfigError::validation(
                    &format!("{}.tls", path),
                    "reverse TLS is not supported with pproxy_compat (wire must remain byte-compatible plaintext)",
                ));
            }

            let tls = compile_reverse_client_tls(c.tls.as_ref(), &path)?;

            Ok(CompiledReverseClientConfig {
                id: c.id.clone(),
                server_addr,
                server_chain,
                auth_username: c.auth_username.clone(),
                auth_password,
                reconnect_initial_ms,
                reconnect_max_ms,
                default_target_host: c.default_target_host.clone(),
                default_target_port: c.default_target_port,
                // The reverse control protocol uses the client's heartbeat
                // interval as its read timeout; retain this compatibility
                // mapping until the schema gains a separate timeout field.
                read_timeout_ms: heartbeat_interval_ms,
                drain_grace_ms: 5_000,
                parallel_connections,
                pproxy_compat: c.pproxy_compat,
                tls,
            })
        })
        .collect()
}

pub(crate) fn compile_reverse_client_tls(
    tls: Option<&crate::model::ReverseClientTlsConfig>,
    path: &str,
) -> Result<Option<CompiledReverseClientTls>, ConfigError> {
    let tls = match tls {
        Some(t) => t,
        None => return Ok(None),
    };
    if tls.server_name.is_empty() {
        return Err(ConfigError::validation(
            &format!("{}.tls.server_name", path),
            "reverse client TLS requires a server_name for SNI/verification",
        ));
    }
    // Validate SNI now rather than during the first reconnect.
    {
        let _ = rustls::pki_types::ServerName::try_from(tls.server_name.clone()).map_err(|_| {
            ConfigError::validation(
                &format!("{}.tls.server_name", path),
                &format!("invalid server_name '{}'", tls.server_name),
            )
        })?;
    }
    if tls.client_cert.is_some() != tls.client_key.is_some() {
        return Err(ConfigError::validation(
            &format!("{}.tls", path),
            "reverse client mTLS requires both client_cert and client_key",
        ));
    }
    let ca_pem = tls
        .ca
        .as_deref()
        .map(|p| {
            std::fs::read(p).map_err(|e| {
                ConfigError::validation(
                    &format!("{}.tls.ca", path),
                    &format!("failed to read CA file: {}", e),
                )
            })
        })
        .transpose()?;
    let client_cert_pem = tls
        .client_cert
        .as_deref()
        .map(|p| {
            std::fs::read(p).map_err(|e| {
                ConfigError::validation(
                    &format!("{}.tls.client_cert", path),
                    &format!("failed to read client cert file: {}", e),
                )
            })
        })
        .transpose()?;
    let client_key_pem = tls
        .client_key
        .as_deref()
        .map(|p| {
            std::fs::read(p).map_err(|e| {
                ConfigError::validation(
                    &format!("{}.tls.client_key", path),
                    &format!("failed to read client key file: {}", e),
                )
            })
        })
        .transpose()?;
    // Validate PEM via shared builders.
    {
        let mut builder = eggress_transport_tls::TlsClientConfigBuilder::new();
        builder = match ca_pem.as_deref() {
            Some(ca) => builder.with_custom_ca_pem(ca).map_err(|e| {
                ConfigError::validation(
                    &format!("{}.tls.ca", path),
                    &format!("invalid CA PEM: {}", e),
                )
            })?,
            None => builder.with_system_roots().map_err(|e| {
                ConfigError::validation(
                    &format!("{}.tls", path),
                    &format!("TLS system roots unavailable: {}", e),
                )
            })?,
        };
        if let (Some(cert), Some(key)) = (client_cert_pem.as_ref(), client_key_pem.as_ref()) {
            builder = builder.with_client_cert_pem(cert, key);
        }
        builder.build().map_err(|e| {
            ConfigError::validation(
                &format!("{}.tls", path),
                &format!("invalid TLS config: {}", e),
            )
        })?;
    }
    Ok(Some(CompiledReverseClientTls {
        ca_pem,
        server_name: tls.server_name.clone(),
        client_cert_pem,
        client_key_pem,
    }))
}
