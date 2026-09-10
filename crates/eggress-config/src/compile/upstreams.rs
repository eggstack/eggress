//! Upstream chain/group/health compilation.

use std::sync::Arc;

use eggress_routing::scheduler::SchedulerKind;
use eggress_routing::UpstreamGroupId;

use crate::error::ConfigError;
use crate::model::{ConfigFile, HealthConfigToml};
use crate::validate::validate_duration;

use super::model::*;

pub(crate) fn compile_health_config(
    health: Option<&HealthConfigToml>,
) -> Result<eggress_routing::health::HealthConfig, ConfigError> {
    let defaults = eggress_routing::health::HealthConfig::default();
    let Some(h) = health else {
        return Ok(defaults);
    };

    let interval = h
        .interval
        .as_deref()
        .map(validate_duration)
        .transpose()?
        .unwrap_or(defaults.interval);

    let timeout = h
        .timeout
        .as_deref()
        .map(validate_duration)
        .transpose()?
        .unwrap_or(defaults.timeout);

    let failures_to_unhealthy = h
        .failures_to_unhealthy
        .unwrap_or(defaults.failures_to_unhealthy);
    if failures_to_unhealthy == 0 {
        return Err(ConfigError::validation(
            "health.failures_to_unhealthy",
            "must be greater than 0",
        ));
    }

    let successes_to_healthy = h
        .successes_to_healthy
        .unwrap_or(defaults.successes_to_healthy);
    if successes_to_healthy == 0 {
        return Err(ConfigError::validation(
            "health.successes_to_healthy",
            "must be greater than 0",
        ));
    }

    let initial_state = match h.initial_state.as_deref() {
        Some("unknown") | None => defaults.initial_state,
        Some("healthy") => eggress_routing::health::HealthState::Healthy,
        Some("unhealthy") => eggress_routing::health::HealthState::Unhealthy,
        Some("disabled") => eggress_routing::health::HealthState::Disabled,
        Some(other) => {
            return Err(ConfigError::validation(
                "health.initial_state",
                &format!(
                    "unknown state '{}', must be one of: unknown, healthy, unhealthy, disabled",
                    other
                ),
            ));
        }
    };

    Ok(eggress_routing::health::HealthConfig {
        interval,
        timeout,
        failures_to_unhealthy,
        successes_to_healthy,
        initial_state,
    })
}

pub(crate) fn compile_h2_config(
    model: &crate::model::H2UpstreamConfig,
    path_prefix: &str,
) -> Result<CompiledH2Config, ConfigError> {
    let mut compiled = CompiledH2Config::default();

    if let Some(max) = model.max_concurrent_streams {
        if max == 0 {
            return Err(ConfigError::validation(
                &format!("{}.max_concurrent_streams", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.max_concurrent_streams = max;
    }

    if let Some(pool) = model.pool_size {
        if pool == 0 {
            return Err(ConfigError::validation(
                &format!("{}.pool_size", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.pool_size = pool;
    }

    if let Some(ref idle) = model.idle_timeout {
        compiled.idle_timeout = validate_duration(idle).map_err(|e| {
            ConfigError::validation(&format!("{}.idle_timeout", path_prefix), &e.to_string())
        })?;
    }

    if let Some(ref interval) = model.keepalive_interval {
        compiled.keepalive_interval = validate_duration(interval).map_err(|e| {
            ConfigError::validation(
                &format!("{}.keepalive_interval", path_prefix),
                &e.to_string(),
            )
        })?;
    }

    if let Some(ref timeout) = model.keepalive_timeout {
        compiled.keepalive_timeout = validate_duration(timeout).map_err(|e| {
            ConfigError::validation(
                &format!("{}.keepalive_timeout", path_prefix),
                &e.to_string(),
            )
        })?;
    }

    if let Some(window) = model.stream_receive_window {
        if window == 0 {
            return Err(ConfigError::validation(
                &format!("{}.stream_receive_window", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.stream_receive_window = window;
    }

    if let Some(window) = model.connection_receive_window {
        if window == 0 {
            return Err(ConfigError::validation(
                &format!("{}.connection_receive_window", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.connection_receive_window = window;
    }

    if let Some(size) = model.max_frame_size {
        if size == 0 {
            return Err(ConfigError::validation(
                &format!("{}.max_frame_size", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.max_frame_size = size;
    }

    if let Some(size) = model.max_header_list_size {
        if size == 0 {
            return Err(ConfigError::validation(
                &format!("{}.max_header_list_size", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.max_header_list_size = size;
    }

    Ok(compiled)
}

pub(crate) fn compile_upstreams(config: &ConfigFile) -> Result<Vec<UpstreamConfig>, ConfigError> {
    let upstreams = match &config.upstreams {
        Some(u) => u,
        None => return Ok(vec![]),
    };

    upstreams
        .iter()
        .map(|u| {
            eggress_routing::upstream::validate_upstream_id(&u.id)
                .map_err(|e| ConfigError::validation(&format!("upstream {}", u.id), &e))?;

            let chain = eggress_uri::parse_proxy_chain(&u.uri).map_err(|_e| {
                ConfigError::validation(&format!("upstream {}", u.id), "invalid upstream URI")
            })?;

            for (idx, hop) in chain.hops.iter().enumerate() {
                if hop.insecure
                    && (hop.protocols.contains(&eggress_uri::ProtocolSpec::Quic)
                        || hop.protocols.contains(&eggress_uri::ProtocolSpec::Http3))
                {
                    return Err(ConfigError::validation(
                        &format!("upstream {} hop {}", u.id, idx),
                        "insecure QUIC/H3 requires the insecure-quic feature; rebuild with --features insecure-quic",
                    ));
                }
            }

            let health = compile_health_config(u.health.as_ref()).map_err(|e| match e {
                ConfigError::Validation { path, message } => {
                    ConfigError::validation(&format!("upstream {}.{}", u.id, path), &message)
                }
                other => other,
            })?;

            let h2 = match u.h2.as_ref() {
                Some(h2_model) => Some(
                    compile_h2_config(h2_model, &format!("upstream {}", u.id)).map_err(
                        |e| match e {
                            ConfigError::Validation { path, message } => {
                                ConfigError::validation(&path, &message)
                            }
                            other => other,
                        },
                    )?,
                ),
                None => None,
            };

            Ok(UpstreamConfig {
                id: u.id.clone(),
                chain,
                health,
                h2,
            })
        })
        .collect()
}

pub(crate) fn compile_groups(config: &ConfigFile) -> Result<Vec<UpstreamGroupConfig>, ConfigError> {
    let groups = match &config.upstream_groups {
        Some(g) => g,
        None => return Ok(vec![]),
    };

    groups
        .iter()
        .map(|g| {
            let scheduler = match g.scheduler.as_deref() {
                Some("round-robin") | None => SchedulerKind::RoundRobin,
                Some("first-available") => SchedulerKind::FirstAvailable,
                Some("random") => SchedulerKind::Random,
                Some("least-connections") => SchedulerKind::LeastConnections,
                Some(other) => {
                    return Err(ConfigError::validation(
                        &format!("group {}", g.id),
                        &format!("unknown scheduler: {}", other),
                    ))
                }
            };

            let fallback = match g.fallback.as_deref() {
                Some("reject") | None => GroupFallback::Reject,
                Some("direct") => GroupFallback::Direct,
                Some("use-unhealthy") => GroupFallback::UseUnhealthy,
                Some(other) => {
                    return Err(ConfigError::validation(
                        &format!("group {}", g.id),
                        &format!("unknown fallback: {}", other),
                    ))
                }
            };

            Ok(UpstreamGroupConfig {
                id: UpstreamGroupId(Arc::from(g.id.as_str())),
                scheduler,
                members: g.members.clone(),
                fallback,
            })
        })
        .collect()
}
