use std::collections::HashSet;

use crate::error::{ConfigError, ConfigWarning};
use crate::model::{ConfigFile, LeafMatcher, MatchExprConfig};

const VALID_PROTOCOLS: &[&str] = &["http", "socks4", "socks5", "shadowsocks", "trojan"];

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

const VALID_HEALTH_MODES: &[&str] = &["tcp_connect"];
const VALID_HEALTH_INITIAL_STATES: &[&str] = &["unknown", "healthy", "unhealthy", "disabled"];

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

    if let Some(ref upstreams) = config.upstreams {
        if let Some(ref groups) = config.upstream_groups {
            validate_upstream_transport(upstreams, groups, config, &mut errors);
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

        if let Some(ref udp) = listener.udp {
            validate_listener_udp(udp, &path, errors);
        }

        // Trojan requires TLS — the TLS layer is part of the protocol
        if listener.protocols.contains(&"trojan".to_string()) && listener.tls.is_none() {
            errors.push(ConfigError::validation(
                &path,
                "trojan protocol requires TLS configuration ([listeners.tls])",
            ));
        }

        // Trojan requires a [listeners.trojan] section with a password
        if listener.protocols.contains(&"trojan".to_string()) && listener.trojan.is_none() {
            errors.push(ConfigError::validation(
                &path,
                "trojan protocol requires [listeners.trojan] section with password",
            ));
        }

        // Trojan password must not be empty if provided
        if let Some(ref trojan) = listener.trojan {
            if trojan.password.is_empty() {
                errors.push(ConfigError::validation(
                    &format!("{}.trojan.password", path),
                    "trojan password must not be empty",
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

        if let Some(ref health) = upstream.health {
            validate_health_config(health, &path, errors);
        }
    }
}

fn validate_health_config(
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
        if parse_duration(interval).is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.health.interval", parent_path),
                &format!("invalid duration: {}", interval),
            ));
        }
    }
    if let Some(ref timeout) = health.timeout {
        if parse_duration(timeout).is_err() {
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

fn validate_upstream_transport(
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

fn rule_upstream_group_could_match_udp(rule: &crate::model::RuleConfig) -> bool {
    if let Some(ref match_expr) = rule.match_expr {
        return matcher_could_match_udp(match_expr);
    }

    if rule.host_exact.is_some()
        || rule.host_suffix.is_some()
        || rule.host_regex.is_some()
        || rule.destination_port.is_some()
    {
        return true;
    }

    if rule.any.unwrap_or(false) {
        return true;
    }

    true
}

fn matcher_could_match_udp(matcher: &MatchExprConfig) -> bool {
    match matcher {
        MatchExprConfig::Leaf(leaf) => leaf_could_match_udp(leaf),
        MatchExprConfig::Composite(composite) => {
            if let Some(ref all) = composite.all {
                return all.iter().any(matcher_could_match_udp);
            }
            if let Some(ref any_of) = composite.any_of {
                return any_of.iter().any(matcher_could_match_udp);
            }
            if let Some(ref not) = composite.not {
                return matcher_could_match_udp(not);
            }
            true
        }
    }
}

fn leaf_could_match_udp(leaf: &LeafMatcher) -> bool {
    if let Some(ref transport) = leaf.transport {
        return transport == "udp";
    }
    true
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
                } else if range[0] > range[1] {
                    errors.push(ConfigError::validation(
                        &format!("{}.destination_port_range", path),
                        &format!("start ({}) must be <= end ({})", range[0], range[1]),
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
        "m" => value
            .checked_mul(60)
            .map(std::time::Duration::from_secs)
            .ok_or_else(|| format!("duration overflow: {}m", value)),
        "h" => value
            .checked_mul(3600)
            .map(std::time::Duration::from_secs)
            .ok_or_else(|| format!("duration overflow: {}h", value)),
        "d" => value
            .checked_mul(86400)
            .map(std::time::Duration::from_secs)
            .ok_or_else(|| format!("duration overflow: {}d", value)),
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

    if let Some(ref pac) = admin.pac {
        if let Some(ref path) = pac.path {
            if !path.starts_with('/') {
                errors.push(ConfigError::validation(
                    "admin.pac.path",
                    &format!("PAC path must start with '/': {}", path),
                ));
            }
        }
    }

    if let Some(ref static_content) = admin.static_content {
        let reserved_paths = [
            "/-/health",
            "/-/ready",
            "/-/status",
            "/-/routes",
            "/-/upstreams",
            "/-/config",
            "/-/route-explain",
            "/metrics",
            "/pac",
        ];
        let mut seen_paths = HashSet::new();

        for (i, entry) in static_content.iter().enumerate() {
            let path = format!("admin.static_content[{}]", i);

            if !entry.path.starts_with('/') {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("static path must start with '/': {}", entry.path),
                ));
            }

            if !seen_paths.insert(&entry.path) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("duplicate static path: {}", entry.path),
                ));
            }

            if reserved_paths.contains(&entry.path.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!(
                        "static path collides with reserved admin endpoint: {}",
                        entry.path
                    ),
                ));
            }

            if let Some(ref body) = entry.body {
                if body.is_empty() {
                    errors.push(ConfigError::validation(
                        &path,
                        "static body must be non-empty if provided",
                    ));
                }
            }
        }
    }
}

fn validate_listener_udp(
    udp: &crate::model::ListenerUdpConfig,
    parent_path: &str,
    errors: &mut Vec<ConfigError>,
) {
    let udp_path = format!("{}.udp", parent_path);

    if let Some(ref bind) = udp.bind {
        if bind.parse::<std::net::SocketAddr>().is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.bind", udp_path),
                &format!("invalid socket address: {}", bind),
            ));
        }
    }

    if let Some(ref advertise) = udp.advertise {
        if advertise.parse::<std::net::IpAddr>().is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.advertise", udp_path),
                &format!("invalid IP address: {}", advertise),
            ));
        }
    }

    if let Some(ref idle_timeout) = udp.idle_timeout {
        if parse_duration(idle_timeout).is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.idle_timeout", udp_path),
                &format!("invalid duration: {}", idle_timeout),
            ));
        }
    }

    if let Some(ref target_idle_timeout) = udp.target_idle_timeout {
        if parse_duration(target_idle_timeout).is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.target_idle_timeout", udp_path),
                &format!("invalid duration: {}", target_idle_timeout),
            ));
        }
    }

    if let Some(max_associations) = udp.max_associations {
        if max_associations == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.max_associations", udp_path),
                "must be greater than 0",
            ));
        }
    }

    if let Some(max_targets) = udp.max_targets_per_association {
        if max_targets == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.max_targets_per_association", udp_path),
                "must be greater than 0",
            ));
        }
    }

    if let Some(max_datagram_size) = udp.max_datagram_size {
        if !(257..=65535).contains(&max_datagram_size) {
            errors.push(ConfigError::validation(
                &format!("{}.max_datagram_size", udp_path),
                &format!("must be between 257 and 65535, got {}", max_datagram_size),
            ));
        }
    }
}

/// Check if a socket address string binds to loopback.
///
/// Returns `true` for `127.x.x.x` and `::1`. Returns `false` for `0.0.0.0`,
/// `::`, and other non-loopback addresses.
fn is_loopback_bind(addr: &str) -> bool {
    if let Ok(socket) = addr.parse::<std::net::SocketAddr>() {
        match socket.ip() {
            std::net::IpAddr::V4(v4) => v4.is_loopback(),
            std::net::IpAddr::V6(v6) => v6.is_loopback(),
        }
    } else {
        false
    }
}

/// Emit security warnings for dangerous config combinations.
///
/// This runs after structural validation succeeds and produces non-fatal
/// warnings about configurations that could expose services to untrusted
/// networks without authentication.
pub fn validate_config_security(config: &ConfigFile) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();

    // 35.2 / 35.7: Warn about non-loopback listener binds without auth
    if let Some(ref listeners) = config.listeners {
        for (i, listener) in listeners.iter().enumerate() {
            let path = format!("listeners[{}].bind", i);
            if !is_loopback_bind(&listener.bind) {
                let has_auth = listener.auth.is_some();
                let has_tls = listener.tls.is_some();
                let has_shadowsocks = listener.shadowsocks.is_some();
                let has_trojan = listener.trojan.is_some();
                if !has_auth && !has_tls && !has_shadowsocks && !has_trojan {
                    warnings.push(ConfigWarning {
                        path,
                        message: format!(
                            "listener '{}' binds to {} without authentication or TLS — \
                             this may expose the proxy to untrusted networks",
                            listener.name, listener.bind,
                        ),
                    });
                }
            }
        }
    }

    // 35.4 / 35.7: Warn about non-loopback admin bind
    if let Some(ref admin) = config.admin {
        if let Some(ref bind) = admin.bind {
            if !is_loopback_bind(bind) {
                warnings.push(ConfigWarning {
                    path: "admin.bind".to_string(),
                    message: format!(
                        "admin server binds to {} without authentication — \
                         this may expose admin endpoints to untrusted networks",
                        bind,
                    ),
                });
            }
        }
    }

    // 35.5 / 35.7: Warn about non-loopback reverse control_bind without auth
    if let Some(ref servers) = config.reverse_servers {
        for (i, server) in servers.iter().enumerate() {
            let path = format!("reverse_servers[{}].control_bind", i);
            if !is_loopback_bind(&server.control_bind) {
                let has_auth = server.auth_username.is_some() || server.auth_password.is_some();
                let has_env_auth = server.auth_password_env.is_some();
                if !has_auth && !has_env_auth {
                    warnings.push(ConfigWarning {
                        path,
                        message: format!(
                            "reverse server '{}' control channel binds to {} without authentication — \
                             any client can connect and request proxying",
                            server.id, server.control_bind,
                        ),
                    });
                }
            }
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_detection() {
        assert!(is_loopback_bind("127.0.0.1:8080"));
        assert!(is_loopback_bind("127.0.0.1:0"));
        assert!(is_loopback_bind("[::1]:8080"));
        assert!(!is_loopback_bind("0.0.0.0:8080"));
        assert!(!is_loopback_bind("[::]:8080"));
        assert!(!is_loopback_bind("10.0.0.1:8080"));
        assert!(!is_loopback_bind("192.168.1.1:8080"));
        assert!(!is_loopback_bind("not-an-addr"));
    }

    #[test]
    fn warn_non_loopback_listener_without_auth() {
        let config = ConfigFile {
            version: Some(1),
            listeners: Some(vec![crate::model::ListenerConfig {
                name: "public".to_string(),
                bind: "0.0.0.0:8080".to_string(),
                protocols: vec!["http".to_string()],
                connection_limit: None,
                auth: None,
                udp_enabled: None,
                udp: None,
                tls: None,
                shadowsocks: None,
                trojan: None,
                transparent: None,
                unix: None,
            }]),
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: None,
            reverse_clients: None,
        };
        let warnings = validate_config_security(&config);
        assert!(!warnings.is_empty());
        assert!(warnings[0].message.contains("0.0.0.0:8080"));
    }

    #[test]
    fn no_warn_loopback_listener() {
        let config = ConfigFile {
            version: Some(1),
            listeners: Some(vec![crate::model::ListenerConfig {
                name: "local".to_string(),
                bind: "127.0.0.1:8080".to_string(),
                protocols: vec!["http".to_string()],
                connection_limit: None,
                auth: None,
                udp_enabled: None,
                udp: None,
                tls: None,
                shadowsocks: None,
                trojan: None,
                transparent: None,
                unix: None,
            }]),
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: None,
            reverse_clients: None,
        };
        let warnings = validate_config_security(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn no_warn_authed_listener() {
        let config = ConfigFile {
            version: Some(1),
            listeners: Some(vec![crate::model::ListenerConfig {
                name: "public-ss".to_string(),
                bind: "0.0.0.0:8388".to_string(),
                protocols: vec!["shadowsocks".to_string()],
                connection_limit: None,
                auth: None,
                udp_enabled: None,
                udp: None,
                tls: None,
                shadowsocks: Some(crate::model::ShadowsocksListenerConfig {
                    method: "aes-256-gcm".to_string(),
                    password: "secret".to_string(),
                }),
                trojan: None,
                transparent: None,
                unix: None,
            }]),
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: None,
            reverse_clients: None,
        };
        let warnings = validate_config_security(&config);
        // Shadowsocks provides its own auth, so no warning
        assert!(warnings.is_empty());
    }

    #[test]
    fn warn_non_loopback_admin() {
        let config = ConfigFile {
            version: Some(1),
            listeners: None,
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: Some(crate::model::AdminConfig {
                bind: Some("0.0.0.0:9090".to_string()),
                enabled: None,
                metrics: None,
                pac: None,
                static_content: None,
            }),
            process: None,
            timeouts: None,
            reverse_servers: None,
            reverse_clients: None,
        };
        let warnings = validate_config_security(&config);
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.path == "admin.bind"));
    }

    #[test]
    fn no_warn_loopback_admin() {
        let config = ConfigFile {
            version: Some(1),
            listeners: None,
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: Some(crate::model::AdminConfig {
                bind: Some("127.0.0.1:9090".to_string()),
                enabled: None,
                metrics: None,
                pac: None,
                static_content: None,
            }),
            process: None,
            timeouts: None,
            reverse_servers: None,
            reverse_clients: None,
        };
        let warnings = validate_config_security(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn warn_reverse_control_bind_without_auth() {
        let config = ConfigFile {
            version: Some(1),
            listeners: None,
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: Some(vec![crate::model::ReverseServerConfig {
                id: "rs1".to_string(),
                control_bind: "0.0.0.0:8443".to_string(),
                auth_username: None,
                auth_password: None,
                auth_password_env: None,
                max_streams: None,
                heartbeat_interval: None,
            }]),
            reverse_clients: None,
        };
        let warnings = validate_config_security(&config);
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.path.contains("control_bind")));
    }

    #[test]
    fn no_warn_reverse_control_bind_with_auth() {
        let config = ConfigFile {
            version: Some(1),
            listeners: None,
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: Some(vec![crate::model::ReverseServerConfig {
                id: "rs1".to_string(),
                control_bind: "0.0.0.0:8443".to_string(),
                auth_username: Some("user".to_string()),
                auth_password: Some("pass".to_string()),
                auth_password_env: None,
                max_streams: None,
                heartbeat_interval: None,
            }]),
            reverse_clients: None,
        };
        let warnings = validate_config_security(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn no_warn_reverse_control_bind_with_env_auth() {
        let config = ConfigFile {
            version: Some(1),
            listeners: None,
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: Some(vec![crate::model::ReverseServerConfig {
                id: "rs1".to_string(),
                control_bind: "0.0.0.0:8443".to_string(),
                auth_username: None,
                auth_password: None,
                auth_password_env: Some("MY_SECRET".to_string()),
                max_streams: None,
                heartbeat_interval: None,
            }]),
            reverse_clients: None,
        };
        let warnings = validate_config_security(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn warn_trojan_listener_without_auth() {
        let config = ConfigFile {
            version: Some(1),
            listeners: Some(vec![crate::model::ListenerConfig {
                name: "public-trojan".to_string(),
                bind: "0.0.0.0:443".to_string(),
                protocols: vec!["trojan".to_string()],
                connection_limit: None,
                auth: None,
                udp_enabled: None,
                udp: None,
                tls: Some(crate::model::ListenerTlsConfig {
                    cert: "/path/cert.pem".to_string(),
                    key: "/path/key.pem".to_string(),
                    alpn: None,
                }),
                shadowsocks: None,
                trojan: Some(crate::model::ListenerTrojanConfig {
                    password: "secret".to_string(),
                }),
                transparent: None,
                unix: None,
            }]),
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: None,
            reverse_clients: None,
        };
        // Trojan provides its own auth via password hash, no warning expected
        let warnings = validate_config_security(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_trojan_requires_tls() {
        let config = ConfigFile {
            version: Some(1),
            listeners: Some(vec![crate::model::ListenerConfig {
                name: "trojan-notls".to_string(),
                bind: "127.0.0.1:443".to_string(),
                protocols: vec!["trojan".to_string()],
                connection_limit: None,
                auth: None,
                udp_enabled: None,
                udp: None,
                tls: None,
                shadowsocks: None,
                trojan: Some(crate::model::ListenerTrojanConfig {
                    password: "secret".to_string(),
                }),
                transparent: None,
                unix: None,
            }]),
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: None,
            reverse_clients: None,
        };
        let result = validate_config(&config);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.to_string().contains("requires TLS")));
    }

    #[test]
    fn validate_trojan_requires_trojan_section() {
        let config = ConfigFile {
            version: Some(1),
            listeners: Some(vec![crate::model::ListenerConfig {
                name: "trojan-nosection".to_string(),
                bind: "127.0.0.1:443".to_string(),
                protocols: vec!["trojan".to_string()],
                connection_limit: None,
                auth: None,
                udp_enabled: None,
                udp: None,
                tls: Some(crate::model::ListenerTlsConfig {
                    cert: "/path/cert.pem".to_string(),
                    key: "/path/key.pem".to_string(),
                    alpn: None,
                }),
                shadowsocks: None,
                trojan: None,
                transparent: None,
                unix: None,
            }]),
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: None,
            reverse_clients: None,
        };
        let result = validate_config(&config);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.to_string().contains("requires [listeners.trojan]")));
    }

    #[test]
    fn validate_trojan_empty_password_rejected() {
        let config = ConfigFile {
            version: Some(1),
            listeners: Some(vec![crate::model::ListenerConfig {
                name: "trojan-empty".to_string(),
                bind: "127.0.0.1:443".to_string(),
                protocols: vec!["trojan".to_string()],
                connection_limit: None,
                auth: None,
                udp_enabled: None,
                udp: None,
                tls: Some(crate::model::ListenerTlsConfig {
                    cert: "/path/cert.pem".to_string(),
                    key: "/path/key.pem".to_string(),
                    alpn: None,
                }),
                shadowsocks: None,
                trojan: Some(crate::model::ListenerTrojanConfig {
                    password: String::new(),
                }),
                transparent: None,
                unix: None,
            }]),
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: None,
            reverse_clients: None,
        };
        let result = validate_config(&config);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.to_string().contains("password must not be empty")));
    }

    #[test]
    fn validate_trojan_with_tls_and_password_passes() {
        let config = ConfigFile {
            version: Some(1),
            listeners: Some(vec![crate::model::ListenerConfig {
                name: "trojan-valid".to_string(),
                bind: "127.0.0.1:443".to_string(),
                protocols: vec!["trojan".to_string()],
                connection_limit: None,
                auth: None,
                udp_enabled: None,
                udp: None,
                tls: Some(crate::model::ListenerTlsConfig {
                    cert: "/path/cert.pem".to_string(),
                    key: "/path/key.pem".to_string(),
                    alpn: None,
                }),
                shadowsocks: None,
                trojan: Some(crate::model::ListenerTrojanConfig {
                    password: "my-secret".to_string(),
                }),
                transparent: None,
                unix: None,
            }]),
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
            process: None,
            timeouts: None,
            reverse_servers: None,
            reverse_clients: None,
        };
        let result = validate_config(&config);
        assert!(
            result.is_ok(),
            "valid trojan config should pass: {:?}",
            result.err()
        );
    }
}
