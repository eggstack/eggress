use std::sync::Arc;

use eggress_core::{ProtocolId, RejectReason};
use eggress_routing::scheduler::SchedulerKind;
use eggress_routing::UpstreamGroupId;

use crate::error::ConfigError;
use crate::model::{ConfigFile, HealthConfigToml, LeafMatcher, MatchExprConfig, RuleConfig};
use crate::validate::validate_duration;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub process: ProcessConfig,
    pub timeouts: TimeoutConfig,
    pub listeners: Vec<ListenerConfig>,
    pub upstreams: Vec<UpstreamConfig>,
    pub groups: Vec<UpstreamGroupConfig>,
    pub rules: Vec<eggress_routing::CompiledRule>,
    pub default_action: eggress_routing::RouteActionSpec,
    pub admin: Option<AdminConfig>,
}

#[derive(Debug, Clone)]
pub struct ProcessConfig {
    pub log_format: String,
    pub log_level: String,
    pub shutdown_grace: std::time::Duration,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            log_format: "text".to_string(),
            log_level: "info".to_string(),
            shutdown_grace: std::time::Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    pub handshake: std::time::Duration,
    pub connect: std::time::Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            handshake: std::time::Duration::from_secs(10),
            connect: std::time::Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListenerConfig {
    pub name: String,
    pub bind: String,
    pub protocols: Vec<ProtocolId>,
    pub connection_limit: Option<u32>,
    pub auth: Option<crate::model::AuthConfig>,
}

#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    pub id: String,
    pub chain: eggress_uri::ProxyChainSpec,
    pub health: eggress_routing::health::HealthConfig,
}

#[derive(Debug, Clone)]
pub struct UpstreamGroupConfig {
    pub id: UpstreamGroupId,
    pub scheduler: SchedulerKind,
    pub members: Vec<String>,
    pub fallback: GroupFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupFallback {
    Reject,
    Direct,
    UseUnhealthy,
}

#[derive(Debug, Clone)]
pub struct AdminConfig {
    pub bind: String,
    pub enabled: bool,
    pub metrics: bool,
    pub pac: Option<eggress_admin::PacConfig>,
    pub static_content: Vec<eggress_admin::StaticRoute>,
}

fn compile_reject_reason(s: &str) -> Result<RejectReason, ConfigError> {
    match s {
        "unsupported-protocol" => Ok(RejectReason::UnsupportedProtocol),
        "auth-required" => Ok(RejectReason::AuthRequired),
        "access-denied" => Ok(RejectReason::AccessDenied),
        "blocked" => Ok(RejectReason::Blocked),
        "internal-error" => Ok(RejectReason::InternalError),
        _ => Err(ConfigError::validation(
            "reject",
            &format!("unknown reject reason: {}", s),
        )),
    }
}

fn compile_protocol(s: &str) -> Result<ProtocolId, ConfigError> {
    match s {
        "http" => Ok(ProtocolId::Http),
        "socks4" => Ok(ProtocolId::Socks4),
        "socks5" => Ok(ProtocolId::Socks5),
        _ => Err(ConfigError::validation(
            "protocols",
            &format!("unknown protocol: {}", s),
        )),
    }
}

fn compile_transport(s: &str) -> Result<eggress_routing::TransportKind, ConfigError> {
    match s {
        "tcp" => Ok(eggress_routing::TransportKind::Tcp),
        "udp" => Ok(eggress_routing::TransportKind::Udp),
        _ => Err(ConfigError::validation(
            "transport",
            &format!("unknown transport: {}", s),
        )),
    }
}

fn compile_matcher(rule: &RuleConfig) -> Result<eggress_routing::MatchExpr, ConfigError> {
    if let Some(ref match_expr) = rule.match_expr {
        return compile_match_config(match_expr);
    }

    if let Some(ref exact) = rule.host_exact {
        if rule.host_suffix.is_none()
            && rule.host_regex.is_none()
            && rule.destination_port.is_none()
            && !rule.any.unwrap_or(false)
        {
            return Ok(eggress_routing::MatchExpr::HostExact(Arc::from(
                exact.as_str(),
            )));
        }
    }
    if let Some(ref suffix) = rule.host_suffix {
        if rule.host_exact.is_none()
            && rule.host_regex.is_none()
            && rule.destination_port.is_none()
            && !rule.any.unwrap_or(false)
        {
            return Ok(eggress_routing::MatchExpr::HostSuffix(Arc::from(
                suffix.as_str(),
            )));
        }
    }
    if let Some(ref regex_str) = rule.host_regex {
        if rule.host_exact.is_none()
            && rule.host_suffix.is_none()
            && rule.destination_port.is_none()
            && !rule.any.unwrap_or(false)
        {
            let re = regex::Regex::new(regex_str).map_err(|e| {
                ConfigError::validation(
                    "host_regex",
                    &format!("invalid regex '{}': {}", regex_str, e),
                )
            })?;
            return Ok(eggress_routing::MatchExpr::HostRegex(re));
        }
    }
    if let Some(port) = rule.destination_port {
        if rule.host_exact.is_none()
            && rule.host_suffix.is_none()
            && rule.host_regex.is_none()
            && !rule.any.unwrap_or(false)
        {
            return Ok(eggress_routing::MatchExpr::DestinationPort(
                eggress_routing::PortMatcher::Exact(port),
            ));
        }
    }
    if rule.any.unwrap_or(false)
        || (rule.host_exact.is_none()
            && rule.host_suffix.is_none()
            && rule.host_regex.is_none()
            && rule.destination_port.is_none())
    {
        return Ok(eggress_routing::MatchExpr::Any);
    }
    Err(ConfigError::validation(&rule.id, "ambiguous matcher"))
}

const MAX_EXPRESSION_DEPTH: usize = 10;
const MAX_NODE_COUNT: usize = 100;

fn compile_match_config(
    config: &MatchExprConfig,
) -> Result<eggress_routing::MatchExpr, ConfigError> {
    let mut node_count = 0;
    compile_match_config_limited(config, 0, &mut node_count)
}

fn compile_match_config_limited(
    config: &MatchExprConfig,
    depth: usize,
    node_count: &mut usize,
) -> Result<eggress_routing::MatchExpr, ConfigError> {
    *node_count += 1;
    if *node_count > MAX_NODE_COUNT {
        return Err(ConfigError::validation(
            "match",
            &format!("expression exceeds maximum node count ({})", MAX_NODE_COUNT),
        ));
    }
    if depth > MAX_EXPRESSION_DEPTH {
        return Err(ConfigError::validation(
            "match",
            &format!(
                "expression exceeds maximum depth ({})",
                MAX_EXPRESSION_DEPTH
            ),
        ));
    }

    match config {
        MatchExprConfig::Composite(composite) => {
            if let Some(ref all) = composite.all {
                if all.is_empty() {
                    return Err(ConfigError::validation("match.all", "must not be empty"));
                }
                let exprs: Vec<eggress_routing::MatchExpr> = all
                    .iter()
                    .map(|c| compile_match_config_limited(c, depth + 1, node_count))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(eggress_routing::MatchExpr::All(exprs));
            }
            if let Some(ref any_of) = composite.any_of {
                if any_of.is_empty() {
                    return Err(ConfigError::validation("match.any_of", "must not be empty"));
                }
                let exprs: Vec<eggress_routing::MatchExpr> = any_of
                    .iter()
                    .map(|c| compile_match_config_limited(c, depth + 1, node_count))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(eggress_routing::MatchExpr::AnyOf(exprs));
            }
            if let Some(ref not) = composite.not {
                let inner = compile_match_config_limited(not, depth + 1, node_count)?;
                return Ok(eggress_routing::MatchExpr::Not(Box::new(inner)));
            }
            Err(ConfigError::validation(
                "match",
                "composite must have exactly one of: all, any_of, not",
            ))
        }
        MatchExprConfig::Leaf(leaf) => compile_leaf_matcher(leaf),
    }
}

fn compile_leaf_matcher(leaf: &LeafMatcher) -> Result<eggress_routing::MatchExpr, ConfigError> {
    let mut matchers = Vec::new();

    if let Some(ref exact) = leaf.host_exact {
        matchers.push(eggress_routing::MatchExpr::HostExact(Arc::from(
            exact.as_str(),
        )));
    }
    if let Some(ref suffix) = leaf.host_suffix {
        matchers.push(eggress_routing::MatchExpr::HostSuffix(Arc::from(
            suffix.as_str(),
        )));
    }
    if let Some(ref regex_str) = leaf.host_regex {
        let re = regex::Regex::new(regex_str).map_err(|e| {
            ConfigError::validation(
                "host_regex",
                &format!("invalid regex '{}': {}", regex_str, e),
            )
        })?;
        matchers.push(eggress_routing::MatchExpr::HostRegex(re));
    }
    if let Some(port) = leaf.destination_port {
        matchers.push(eggress_routing::MatchExpr::DestinationPort(
            eggress_routing::PortMatcher::Exact(port),
        ));
    }
    if let Some(ref range) = leaf.destination_port_range {
        if range.len() != 2 {
            return Err(ConfigError::validation(
                "destination_port_range",
                "must have exactly 2 elements [start, end]",
            ));
        }
        matchers.push(eggress_routing::MatchExpr::DestinationPort(
            eggress_routing::PortMatcher::Range {
                start: range[0],
                end: range[1],
            },
        ));
    }
    if let Some(ref ports) = leaf.destination_port_set {
        if ports.is_empty() {
            return Err(ConfigError::validation(
                "destination_port_set",
                "must not be empty",
            ));
        }
        matchers.push(eggress_routing::MatchExpr::DestinationPort(
            eggress_routing::PortMatcher::Set(Arc::from(ports.as_slice())),
        ));
    }
    if let Some(ref cidr) = leaf.destination_cidr {
        let net: ipnet::IpNet = cidr.parse().map_err(|e: ipnet::AddrParseError| {
            ConfigError::validation(
                "destination_cidr",
                &format!("invalid CIDR '{}': {}", cidr, e),
            )
        })?;
        matchers.push(eggress_routing::MatchExpr::DestinationCidr(net));
    }
    if let Some(ref cidr) = leaf.source_cidr {
        let net: ipnet::IpNet = cidr.parse().map_err(|e: ipnet::AddrParseError| {
            ConfigError::validation("source_cidr", &format!("invalid CIDR '{}': {}", cidr, e))
        })?;
        matchers.push(eggress_routing::MatchExpr::SourceCidr(net));
    }
    if let Some(source_port) = leaf.source_port {
        matchers.push(eggress_routing::MatchExpr::SourcePort(
            eggress_routing::PortMatcher::Exact(source_port),
        ));
    }
    if let Some(ref name) = leaf.listener {
        matchers.push(eggress_routing::MatchExpr::Listener(Arc::from(
            name.as_str(),
        )));
    }
    if let Some(ref proto) = leaf.protocol {
        let protocol_id = compile_protocol(proto)?;
        matchers.push(eggress_routing::MatchExpr::Protocol(protocol_id));
    }
    if let Some(ref ident) = leaf.identity {
        matchers.push(eggress_routing::MatchExpr::Identity(Arc::from(
            ident.as_str(),
        )));
    }
    if let Some(ref transport_str) = leaf.transport {
        let transport_kind = compile_transport(transport_str)?;
        matchers.push(eggress_routing::MatchExpr::Transport(transport_kind));
    }

    match matchers.len() {
        0 => Ok(eggress_routing::MatchExpr::Any),
        1 => Ok(matchers.into_iter().next().unwrap()),
        _ => Ok(eggress_routing::MatchExpr::All(matchers)),
    }
}

fn compile_action(
    rule: &RuleConfig,
    group_ids: &std::collections::HashSet<&str>,
) -> Result<eggress_routing::RouteActionSpec, ConfigError> {
    if let Some(direct) = rule.direct {
        if direct {
            return Ok(eggress_routing::RouteActionSpec::Direct);
        }
        return Err(ConfigError::validation(
            &rule.id,
            "direct action must be true",
        ));
    }
    if let Some(ref group) = rule.upstream_group {
        if !group_ids.contains(group.as_str()) {
            return Err(ConfigError::validation(
                &rule.id,
                &format!("unknown upstream group: {}", group),
            ));
        }
        return Ok(eggress_routing::RouteActionSpec::UpstreamGroup(
            UpstreamGroupId(Arc::from(group.as_str())),
        ));
    }
    if let Some(ref reject) = rule.reject {
        let reason = compile_reject_reason(reject)?;
        return Ok(eggress_routing::RouteActionSpec::Reject(reason));
    }
    Err(ConfigError::validation(&rule.id, "missing action"))
}

pub fn compile_config(config: &ConfigFile) -> Result<RuntimeConfig, ConfigError> {
    let process = compile_process(config);
    let timeouts = compile_timeouts(config)?;
    let listeners = compile_listeners(config)?;
    let upstreams = compile_upstreams(config)?;
    let groups = compile_groups(config)?;
    let rules = compile_rules(config)?;
    let default_action = compile_default_action(config);
    let admin = compile_admin(config);

    Ok(RuntimeConfig {
        process,
        timeouts,
        listeners,
        upstreams,
        groups,
        rules,
        default_action,
        admin,
    })
}

fn compile_process(config: &ConfigFile) -> ProcessConfig {
    let defaults = ProcessConfig::default();
    let process = config.process.as_ref();

    ProcessConfig {
        log_format: process
            .and_then(|p| p.log_format.clone())
            .unwrap_or(defaults.log_format),
        log_level: process
            .and_then(|p| p.log_level.clone())
            .unwrap_or(defaults.log_level),
        shutdown_grace: process
            .and_then(|p| p.shutdown_grace.as_ref())
            .and_then(|s| parse_duration_opt(s))
            .unwrap_or(defaults.shutdown_grace),
    }
}

fn compile_timeouts(config: &ConfigFile) -> Result<TimeoutConfig, ConfigError> {
    let defaults = TimeoutConfig::default();
    let timeouts = config.timeouts.as_ref();

    Ok(TimeoutConfig {
        handshake: timeouts
            .and_then(|t| t.handshake.as_ref())
            .map(|s| validate_duration(s))
            .transpose()?
            .unwrap_or(defaults.handshake),
        connect: timeouts
            .and_then(|t| t.connect.as_ref())
            .map(|s| validate_duration(s))
            .transpose()?
            .unwrap_or(defaults.connect),
    })
}

fn compile_listeners(config: &ConfigFile) -> Result<Vec<ListenerConfig>, ConfigError> {
    let listeners = match &config.listeners {
        Some(l) => l,
        None => return Ok(vec![]),
    };

    listeners
        .iter()
        .map(|l| {
            let protocols: Vec<ProtocolId> = l
                .protocols
                .iter()
                .map(|p| compile_protocol(p))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(ListenerConfig {
                name: l.name.clone(),
                bind: l.bind.clone(),
                protocols,
                connection_limit: l.connection_limit,
                auth: l.auth.clone(),
            })
        })
        .collect()
}

fn compile_health_config(
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

fn compile_upstreams(config: &ConfigFile) -> Result<Vec<UpstreamConfig>, ConfigError> {
    let upstreams = match &config.upstreams {
        Some(u) => u,
        None => return Ok(vec![]),
    };

    upstreams
        .iter()
        .map(|u| {
            eggress_routing::upstream::validate_upstream_id(&u.id)
                .map_err(|e| ConfigError::validation(&format!("upstream {}", u.id), &e))?;

            let chain = eggress_uri::parse_proxy_chain(&u.uri).map_err(|e| {
                ConfigError::validation(
                    &format!("upstream {}", u.id),
                    &format!("invalid URI: {}", e),
                )
            })?;

            let health = compile_health_config(u.health.as_ref()).map_err(|e| match e {
                ConfigError::Validation { path, message } => {
                    ConfigError::validation(&format!("upstream {}.{}", u.id, path), &message)
                }
                other => other,
            })?;

            Ok(UpstreamConfig {
                id: u.id.clone(),
                chain,
                health,
            })
        })
        .collect()
}

fn compile_groups(config: &ConfigFile) -> Result<Vec<UpstreamGroupConfig>, ConfigError> {
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

fn compile_rules(config: &ConfigFile) -> Result<Vec<eggress_routing::CompiledRule>, ConfigError> {
    let mut compiled_rules = Vec::new();

    let group_ids: std::collections::HashSet<&str> = config
        .upstream_groups
        .as_ref()
        .map(|gs| gs.iter().map(|g| g.id.as_str()).collect())
        .unwrap_or_default();

    if let Some(ref rules) = config.rules {
        for r in rules {
            let matcher = compile_matcher(r)?;
            let action = compile_action(r, &group_ids)?;

            compiled_rules.push(eggress_routing::CompiledRule {
                id: eggress_routing::RuleId(Arc::from(r.id.as_str())),
                matcher,
                action,
            });
        }
    }

    if let Some(ref rules_file_path) = config.rules_file {
        let content = std::fs::read_to_string(rules_file_path).map_err(|e| {
            ConfigError::validation(
                "rules_file",
                &format!("failed to read '{}': {}", rules_file_path, e),
            )
        })?;
        let compat_rules = eggress_routing::CompatRegexRule::parse_file(&content).map_err(|e| {
            ConfigError::validation(
                "rules_file",
                &format!("failed to parse '{}': {}", rules_file_path, e),
            )
        })?;
        for (idx, compat) in compat_rules.into_iter().enumerate() {
            compiled_rules.push(eggress_routing::CompiledRule {
                id: eggress_routing::RuleId(Arc::from(format!("rules-file-{}", idx + 1).as_str())),
                matcher: eggress_routing::MatchExpr::HostRegex(compat.pattern),
                action: group_ids
                    .iter()
                    .next()
                    .map(|g| {
                        eggress_routing::RouteActionSpec::UpstreamGroup(
                            eggress_routing::UpstreamGroupId(Arc::from(*g)),
                        )
                    })
                    .unwrap_or(eggress_routing::RouteActionSpec::Direct),
            });
        }
    }

    Ok(compiled_rules)
}

fn compile_default_action(config: &ConfigFile) -> eggress_routing::RouteActionSpec {
    let default_str = config.routing.as_ref().and_then(|r| r.default.as_deref());

    match default_str {
        Some("direct") => eggress_routing::RouteActionSpec::Direct,
        Some("reject") => eggress_routing::RouteActionSpec::Reject(RejectReason::Blocked),
        Some(group_id) => {
            eggress_routing::RouteActionSpec::UpstreamGroup(UpstreamGroupId(Arc::from(group_id)))
        }
        None => eggress_routing::RouteActionSpec::Direct,
    }
}

fn compile_admin(config: &ConfigFile) -> Option<AdminConfig> {
    let admin = config.admin.as_ref()?;

    let pac = admin.pac.as_ref().map(|pac_toml| {
        let path = pac_toml.path.clone().unwrap_or_else(|| "/pac".to_string());
        eggress_admin::PacConfig {
            path,
            proxy_directive: pac_toml.proxy.clone(),
            direct_fallback: pac_toml.direct_fallback.unwrap_or(true),
            direct_hosts: pac_toml.direct_hosts.clone().unwrap_or_default(),
            direct_suffixes: pac_toml.direct_suffixes.clone().unwrap_or_default(),
        }
    });

    let static_content = admin
        .static_content
        .as_ref()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| eggress_admin::StaticRoute {
                    path: entry.path.clone(),
                    content_type: entry
                        .content_type
                        .clone()
                        .unwrap_or_else(|| "text/plain".to_string()),
                    body: entry.body.clone().unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    Some(AdminConfig {
        bind: admin
            .bind
            .clone()
            .unwrap_or_else(|| "127.0.0.1:9090".to_string()),
        enabled: admin.enabled.unwrap_or(true),
        metrics: admin.metrics.unwrap_or(true),
        pac,
        static_content,
    })
}

fn parse_duration_opt(s: &str) -> Option<std::time::Duration> {
    crate::validate::validate_duration(s).ok()
}

pub fn load_and_compile(path: &str) -> Result<RuntimeConfig, crate::error::ConfigError> {
    crate::load_and_validate(path)
}
