//! Upstream validation: chains, health probes, H2, groups, transports.

use std::collections::HashSet;

use super::composition::{
    VALID_FALLBACKS, VALID_HEALTH_INITIAL_STATES, VALID_HEALTH_MODES, VALID_SCHEDULERS,
};
use super::core::parse_duration;
use super::rules::rule_upstream_group_could_match_udp;
use crate::error::ConfigError;
use crate::model::ConfigFile;
pub(crate) fn validate_upstreams(
    upstreams: &[crate::model::UpstreamConfig],
    errors: &mut Vec<ConfigError>,
) {
    let mut ids = HashSet::new();

    for (i, upstream) in upstreams.iter().enumerate() {
        let path = format!("upstreams[{}]", i);

        if !ids.insert(&upstream.id) {
            errors.push(ConfigError::validation(
                &path,
                &format!("duplicate upstream ID: {}", upstream.id),
            ));
        }

        if eggress_uri::parse_proxy_chain(&upstream.uri).is_err() {
            errors.push(ConfigError::validation(&path, "invalid upstream URI"));
        }

        if let Some(ref health) = upstream.health {
            validate_health_config(health, &path, errors);
        }

        if let Some(ref h2) = upstream.h2 {
            validate_h2_config(h2, &path, errors);
        }
    }
}

pub(crate) fn validate_health_config(
    health: &crate::model::HealthConfigToml,
    parent_path: &str,
    errors: &mut Vec<ConfigError>,
) {
    if let Some(ref mode) = health.mode {
        if !VALID_HEALTH_MODES.contains(&mode.as_str()) {
            errors.push(ConfigError::validation(
                &format!("{}.health.mode", parent_path),
                &format!(
                    "unknown health mode '{}', must be one of: {}",
                    mode,
                    VALID_HEALTH_MODES.join(", ")
                ),
            ));
        }
    }
    if let Some(ref interval) = health.interval {
        if let Ok(d) = parse_duration(interval) {
            if d.is_zero() {
                errors.push(ConfigError::validation(
                    &format!("{}.health.interval", parent_path),
                    &format!("must be greater than 0, got: {}", interval),
                ));
            }
        } else {
            errors.push(ConfigError::validation(
                &format!("{}.health.interval", parent_path),
                &format!("invalid duration: {}", interval),
            ));
        }
    }
    if let Some(ref timeout) = health.timeout {
        if let Ok(d) = parse_duration(timeout) {
            if d.is_zero() {
                errors.push(ConfigError::validation(
                    &format!("{}.health.timeout", parent_path),
                    &format!("must be greater than 0, got: {}", timeout),
                ));
            }
        } else {
            errors.push(ConfigError::validation(
                &format!("{}.health.timeout", parent_path),
                &format!("invalid duration: {}", timeout),
            ));
        }
    }
    if let Some(failures) = health.failures_to_unhealthy {
        if failures == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.health.failures_to_unhealthy", parent_path),
                "must be greater than 0",
            ));
        }
    }
    if let Some(successes) = health.successes_to_healthy {
        if successes == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.health.successes_to_healthy", parent_path),
                "must be greater than 0",
            ));
        }
    }
    if let Some(ref initial_state) = health.initial_state {
        if !VALID_HEALTH_INITIAL_STATES.contains(&initial_state.as_str()) {
            errors.push(ConfigError::validation(
                &format!("{}.health.initial_state", parent_path),
                &format!(
                    "unknown state '{}', must be one of: {}",
                    initial_state,
                    VALID_HEALTH_INITIAL_STATES.join(", ")
                ),
            ));
        }
    }
}

pub(crate) fn validate_h2_config(
    h2: &crate::model::H2UpstreamConfig,
    parent_path: &str,
    errors: &mut Vec<ConfigError>,
) {
    if let Some(max) = h2.max_concurrent_streams {
        if max == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.h2.max_concurrent_streams", parent_path),
                "must be greater than 0",
            ));
        }
    }
    if let Some(pool) = h2.pool_size {
        if pool == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.h2.pool_size", parent_path),
                "must be greater than 0",
            ));
        }
    }
    if let Some(ref idle) = h2.idle_timeout {
        if parse_duration(idle).is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.h2.idle_timeout", parent_path),
                &format!("invalid duration: {}", idle),
            ));
        }
    }
    if let Some(ref interval) = h2.keepalive_interval {
        if parse_duration(interval).is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.h2.keepalive_interval", parent_path),
                &format!("invalid duration: {}", interval),
            ));
        }
    }
    if let Some(ref timeout) = h2.keepalive_timeout {
        if parse_duration(timeout).is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.h2.keepalive_timeout", parent_path),
                &format!("invalid duration: {}", timeout),
            ));
        }
    }
    if let Some(window) = h2.stream_receive_window {
        if window == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.h2.stream_receive_window", parent_path),
                "must be greater than 0",
            ));
        }
    }
    if let Some(window) = h2.connection_receive_window {
        if window == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.h2.connection_receive_window", parent_path),
                "must be greater than 0",
            ));
        }
    }
    if let Some(size) = h2.max_frame_size {
        if size == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.h2.max_frame_size", parent_path),
                "must be greater than 0",
            ));
        }
    }
    if let Some(size) = h2.max_header_list_size {
        if size == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.h2.max_header_list_size", parent_path),
                "must be greater than 0",
            ));
        }
    }
}

pub(crate) fn validate_upstream_groups(
    groups: &[crate::model::UpstreamGroupConfig],
    upstreams: Option<&[crate::model::UpstreamConfig]>,
    errors: &mut Vec<ConfigError>,
) {
    let mut ids = HashSet::new();
    let upstream_ids: HashSet<&str> = upstreams
        .map(|u| u.iter().map(|u| u.id.as_str()).collect())
        .unwrap_or_default();

    for (i, group) in groups.iter().enumerate() {
        let path = format!("upstream_groups[{}]", i);

        if !ids.insert(&group.id) {
            errors.push(ConfigError::validation(
                &path,
                &format!("duplicate group ID: {}", group.id),
            ));
        }

        if let Some(ref scheduler) = group.scheduler {
            if !VALID_SCHEDULERS.contains(&scheduler.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("unknown scheduler: {}", scheduler),
                ));
            }
        }

        if let Some(ref fallback) = group.fallback {
            if !VALID_FALLBACKS.contains(&fallback.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("unknown fallback: {}", fallback),
                ));
            }
        }

        if group.members.is_empty() {
            errors.push(ConfigError::validation(
                &path,
                "upstream group must have at least one member",
            ));
        }

        let mut seen_members = HashSet::new();
        for (j, member) in group.members.iter().enumerate() {
            if !seen_members.insert(member.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("duplicate member '{}' at index {}", member, j),
                ));
            }
            if !upstream_ids.contains(member.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("member {} references unknown upstream: {}", j, member),
                ));
            }
        }
    }
}

pub(crate) fn validate_upstream_transport(
    upstreams: &[crate::model::UpstreamConfig],
    groups: &[crate::model::UpstreamGroupConfig],
    config: &ConfigFile,
    errors: &mut Vec<ConfigError>,
) {
    let upstream_chains: std::collections::HashMap<&str, eggress_uri::ProxyChainSpec> = upstreams
        .iter()
        .filter_map(|u| {
            eggress_uri::parse_proxy_chain(&u.uri)
                .ok()
                .map(|chain| (u.id.as_str(), chain))
        })
        .collect();

    let mut group_udp_support: std::collections::HashMap<&str, bool> =
        std::collections::HashMap::new();

    for group in groups {
        let has_udp_upstream = group.members.iter().any(|member_id| {
            upstream_chains
                .get(member_id.as_str())
                .map(|chain| {
                    eggress_core::capability::classify_upstream_chain(chain).is_udp_supported()
                })
                .unwrap_or(false)
        });
        group_udp_support.insert(group.id.as_str(), has_udp_upstream);
    }

    let udp_listener_exists = config
        .listeners
        .as_ref()
        .map(|listeners| {
            listeners.iter().any(|l| {
                l.udp_enabled == Some(true)
                    || l.udp.as_ref().is_some_and(|u| u.enabled != Some(false))
            })
        })
        .unwrap_or(false);

    if let Some(ref rules) = config.rules {
        for rule in rules {
            if let Some(ref upstream_group) = rule.upstream_group {
                let group_id = upstream_group.as_str();
                let group_supports_udp = group_udp_support.get(group_id).copied().unwrap_or(false);

                let rule_could_match_udp = rule_upstream_group_could_match_udp(rule);

                if !group_supports_udp && rule_could_match_udp && udp_listener_exists {
                    errors.push(ConfigError::validation(
                        &format!("rules[{}].upstream_group", rule.id),
                        &format!(
                            "upstream group '{}' contains no UDP-capable upstreams but is referenced by a rule that could match UDP traffic",
                            upstream_group
                        ),
                    ));
                }
            }
        }
    }

    if let Some(ref routing) = config.routing {
        if let Some(ref default) = routing.default {
            if default != "direct" && default != "reject" {
                let group_supports_udp = group_udp_support
                    .get(default.as_str())
                    .copied()
                    .unwrap_or(false);
                if !group_supports_udp && udp_listener_exists {
                    errors.push(ConfigError::validation(
                        "routing.default",
                        &format!(
                            "upstream group '{}' contains no UDP-capable upstreams but is the default route while UDP listeners exist",
                            default
                        ),
                    ));
                }
            }
        }
    }
}
