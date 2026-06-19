use std::collections::HashSet;

use crate::error::ConfigError;
use crate::model::ConfigFile;

const VALID_PROTOCOLS: &[&str] = &["http", "socks4", "socks5"];

const VALID_SCHEDULERS: &[&str] = &[
    "first-available",
    "round-robin",
    "random",
    "least-connections",
];

const VALID_FALLBACKS: &[&str] = &["reject", "direct", "use-unhealthy"];

const VALID_AUTH_TYPES: &[&str] = &["password"];

const VALID_REJECT_REASONS: &[&str] = &[
    "unsupported-protocol",
    "auth-required",
    "access-denied",
    "blocked",
    "internal-error",
];

pub fn validate_config(config: &ConfigFile) -> Result<(), Vec<ConfigError>> {
    let mut errors = Vec::new();

    if let Some(version) = config.version {
        if version != 1 {
            errors.push(ConfigError::UnsupportedVersion(version));
        }
    }

    if let Some(ref listeners) = config.listeners {
        validate_listeners(listeners, &mut errors);
    }

    if let Some(ref upstreams) = config.upstreams {
        validate_upstreams(upstreams, &mut errors);
    }

    if let Some(ref groups) = config.upstream_groups {
        validate_upstream_groups(groups, config.upstreams.as_deref(), &mut errors);
    }

    if let Some(ref rules) = config.rules {
        validate_rules(rules, config.upstream_groups.as_deref(), &mut errors);
    }

    if let Some(ref timeouts) = config.timeouts {
        validate_timeouts(timeouts, &mut errors);
    }

    if let Some(ref process) = config.process {
        validate_process(process, &mut errors);
    }

    if let Some(ref admin) = config.admin {
        validate_admin(admin, &mut errors);
    }

    if let Some(ref routing) = config.routing {
        if let Some(ref default) = routing.default {
            if default != "direct" && default != "reject" {
                let group_ids: Vec<&str> = config
                    .upstream_groups
                    .as_ref()
                    .map(|gs| gs.iter().map(|g| g.id.as_str()).collect())
                    .unwrap_or_default();
                if !group_ids.contains(&default.as_str()) {
                    errors.push(ConfigError::validation(
                        "routing.default",
                        &format!("unknown upstream group or action: {}", default),
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_listeners(listeners: &[crate::model::ListenerConfig], errors: &mut Vec<ConfigError>) {
    let mut names = HashSet::new();

    for (i, listener) in listeners.iter().enumerate() {
        let path = format!("listeners[{}]", i);

        if !names.insert(&listener.name) {
            errors.push(ConfigError::validation(
                &path,
                &format!("duplicate listener name: {}", listener.name),
            ));
        }

        for protocol in &listener.protocols {
            if !VALID_PROTOCOLS.contains(&protocol.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("unknown protocol: {}", protocol),
                ));
            }
        }

        if let Some(ref auth) = listener.auth {
            if !VALID_AUTH_TYPES.contains(&auth.auth_type.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("unknown auth type: {}", auth.auth_type),
                ));
            }
        }
    }
}

fn validate_upstreams(upstreams: &[crate::model::UpstreamConfig], errors: &mut Vec<ConfigError>) {
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
            errors.push(ConfigError::validation(
                &path,
                &format!("invalid upstream URI: {}", upstream.uri),
            ));
        }
    }
}

fn validate_upstream_groups(
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

        for (j, member) in group.members.iter().enumerate() {
            if !upstream_ids.contains(member.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("member {} references unknown upstream: {}", j, member),
                ));
            }
        }
    }
}

fn validate_rules(
    rules: &[crate::model::RuleConfig],
    groups: Option<&[crate::model::UpstreamGroupConfig]>,
    errors: &mut Vec<ConfigError>,
) {
    let group_ids: HashSet<&str> = groups
        .map(|g| g.iter().map(|g| g.id.as_str()).collect())
        .unwrap_or_default();

    for (i, rule) in rules.iter().enumerate() {
        let path = format!("rules[{}]", i);

        if rule.match_expr.is_none() {
            let matcher_count = [
                rule.host_exact.is_some(),
                rule.host_suffix.is_some(),
                rule.host_regex.is_some(),
                rule.destination_port.is_some(),
                rule.any.unwrap_or(false),
            ]
            .iter()
            .filter(|&&b| b)
            .count();

            if matcher_count > 1 {
                errors.push(ConfigError::validation(
                    &path,
                    "rule must have exactly one matcher field",
                ));
            }

            if let Some(ref host_regex) = rule.host_regex {
                if regex::Regex::new(host_regex).is_err() {
                    errors.push(ConfigError::validation(
                        &path,
                        &format!("invalid host regex: {}", host_regex),
                    ));
                }
            }
        } else if let Some(ref match_expr) = rule.match_expr {
            validate_match_expr(match_expr, &path, errors);
        }

        let action_count = [
            rule.direct.is_some(),
            rule.upstream_group.is_some(),
            rule.reject.is_some(),
        ]
        .iter()
        .filter(|&&b| b)
        .count();

        if action_count != 1 {
            errors.push(ConfigError::validation(
                &path,
                "rule must have exactly one action field",
            ));
        }

        if let Some(ref upstream_group) = rule.upstream_group {
            if !group_ids.contains(upstream_group.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!(
                        "action references unknown upstream group: {}",
                        upstream_group
                    ),
                ));
            }
        }

        if let Some(ref reject) = rule.reject {
            if !VALID_REJECT_REASONS.contains(&reject.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("unknown reject reason: {}", reject),
                ));
            }
        }
    }
}

fn validate_match_expr(
    expr: &crate::model::MatchExprConfig,
    path: &str,
    errors: &mut Vec<ConfigError>,
) {
    match expr {
        crate::model::MatchExprConfig::Composite(composite) => {
            if let Some(ref all) = composite.all {
                if all.is_empty() {
                    errors.push(ConfigError::validation(
                        &format!("{}.match.all", path),
                        "must not be empty",
                    ));
                }
                for (j, item) in all.iter().enumerate() {
                    validate_match_expr(item, &format!("{}.match.all[{}]", path, j), errors);
                }
            }
            if let Some(ref any_of) = composite.any_of {
                if any_of.is_empty() {
                    errors.push(ConfigError::validation(
                        &format!("{}.match.any_of", path),
                        "must not be empty",
                    ));
                }
                for (j, item) in any_of.iter().enumerate() {
                    validate_match_expr(item, &format!("{}.match.any_of[{}]", path, j), errors);
                }
            }
            if let Some(ref not) = composite.not {
                validate_match_expr(not, &format!("{}.match.not", path), errors);
            }
        }
        crate::model::MatchExprConfig::Leaf(leaf) => {
            if let Some(ref regex_str) = leaf.host_regex {
                if regex::Regex::new(regex_str).is_err() {
                    errors.push(ConfigError::validation(
                        &format!("{}.host_regex", path),
                        &format!("invalid regex: {}", regex_str),
                    ));
                }
            }
            if let Some(ref cidr) = leaf.destination_cidr {
                if cidr.parse::<ipnet::IpNet>().is_err() {
                    errors.push(ConfigError::validation(
                        &format!("{}.destination_cidr", path),
                        &format!("invalid CIDR: {}", cidr),
                    ));
                }
            }
            if let Some(ref cidr) = leaf.source_cidr {
                if cidr.parse::<ipnet::IpNet>().is_err() {
                    errors.push(ConfigError::validation(
                        &format!("{}.source_cidr", path),
                        &format!("invalid CIDR: {}", cidr),
                    ));
                }
            }
            if let Some(ref range) = leaf.destination_port_range {
                if range.len() != 2 {
                    errors.push(ConfigError::validation(
                        &format!("{}.destination_port_range", path),
                        "must have exactly 2 elements [start, end]",
                    ));
                }
            }
            if let Some(ref ports) = leaf.destination_port_set {
                if ports.is_empty() {
                    errors.push(ConfigError::validation(
                        &format!("{}.destination_port_set", path),
                        "must not be empty",
                    ));
                }
            }
            if let Some(ref proto) = leaf.protocol {
                if !VALID_PROTOCOLS.contains(&proto.as_str()) {
                    errors.push(ConfigError::validation(
                        &format!("{}.protocol", path),
                        &format!("unknown protocol: {}", proto),
                    ));
                }
            }
        }
    }
}

fn parse_duration(s: &str) -> Result<std::time::Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration".to_string());
    }

    let (num_part, unit) = if let Some(pos) = s.find(|c: char| c.is_alphabetic()) {
        (&s[..pos], &s[pos..])
    } else {
        return Err(format!("missing unit in duration: {}", s));
    };

    let value: u64 = num_part
        .parse()
        .map_err(|_| format!("invalid duration value: {}", num_part))?;

    match unit {
        "ns" => Ok(std::time::Duration::from_nanos(value)),
        "us" | "μs" => Ok(std::time::Duration::from_micros(value)),
        "ms" => Ok(std::time::Duration::from_millis(value)),
        "s" => Ok(std::time::Duration::from_secs(value)),
        "m" => Ok(std::time::Duration::from_secs(value * 60)),
        "h" => Ok(std::time::Duration::from_secs(value * 3600)),
        "d" => Ok(std::time::Duration::from_secs(value * 86400)),
        _ => Err(format!("unknown duration unit: {}", unit)),
    }
}

pub fn validate_duration(s: &str) -> Result<std::time::Duration, ConfigError> {
    parse_duration(s).map_err(|msg| ConfigError::validation("duration", &msg))
}

fn validate_timeouts(timeouts: &crate::model::TimeoutConfig, errors: &mut Vec<ConfigError>) {
    if let Some(ref handshake) = timeouts.handshake {
        if parse_duration(handshake).is_err() {
            errors.push(ConfigError::validation(
                "timeouts.handshake",
                &format!("invalid duration: {}", handshake),
            ));
        }
    }
    if let Some(ref connect) = timeouts.connect {
        if parse_duration(connect).is_err() {
            errors.push(ConfigError::validation(
                "timeouts.connect",
                &format!("invalid duration: {}", connect),
            ));
        }
    }
}

fn validate_process(process: &crate::model::ProcessConfig, errors: &mut Vec<ConfigError>) {
    if let Some(ref log_level) = process.log_level {
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&log_level.as_str()) {
            errors.push(ConfigError::validation(
                "process.log_level",
                &format!("unknown log level: {}", log_level),
            ));
        }
    }
    if let Some(ref shutdown_grace) = process.shutdown_grace {
        if parse_duration(shutdown_grace).is_err() {
            errors.push(ConfigError::validation(
                "process.shutdown_grace",
                &format!("invalid duration: {}", shutdown_grace),
            ));
        }
    }
}

fn validate_admin(admin: &crate::model::AdminConfig, errors: &mut Vec<ConfigError>) {
    if let Some(ref bind) = admin.bind {
        if bind.parse::<std::net::SocketAddr>().is_err()
            && bind.parse::<std::net::SocketAddrV4>().is_err()
            && bind.parse::<std::net::SocketAddrV6>().is_err()
        {
            errors.push(ConfigError::validation(
                "admin.bind",
                &format!("invalid bind address: {}", bind),
            ));
        }
    }
}
