use std::sync::Arc;

use eggress_core::{ProtocolId, RejectReason};
use eggress_routing::scheduler::SchedulerKind;
use eggress_routing::UpstreamGroupId;

use crate::error::ConfigError;
use crate::model::{ConfigFile, RuleConfig};
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

fn compile_matcher(rule: &RuleConfig) -> Result<eggress_routing::MatchExpr, ConfigError> {
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

fn compile_upstreams(config: &ConfigFile) -> Result<Vec<UpstreamConfig>, ConfigError> {
    let upstreams = match &config.upstreams {
        Some(u) => u,
        None => return Ok(vec![]),
    };

    upstreams
        .iter()
        .map(|u| {
            let chain = eggress_uri::parse_proxy_chain(&u.uri).map_err(|e| {
                ConfigError::validation(
                    &format!("upstream {}", u.id),
                    &format!("invalid URI: {}", e),
                )
            })?;

            Ok(UpstreamConfig {
                id: u.id.clone(),
                chain,
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
    let rules = match &config.rules {
        Some(r) => r,
        None => return Ok(vec![]),
    };

    let group_ids: std::collections::HashSet<&str> = config
        .upstream_groups
        .as_ref()
        .map(|gs| gs.iter().map(|g| g.id.as_str()).collect())
        .unwrap_or_default();

    rules
        .iter()
        .map(|r| {
            let matcher = compile_matcher(r)?;
            let action = compile_action(r, &group_ids)?;

            Ok(eggress_routing::CompiledRule {
                id: eggress_routing::RuleId(Arc::from(r.id.as_str())),
                matcher,
                action,
            })
        })
        .collect()
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

    Some(AdminConfig {
        bind: admin
            .bind
            .clone()
            .unwrap_or_else(|| "127.0.0.1:9090".to_string()),
        enabled: admin.enabled.unwrap_or(true),
        metrics: admin.metrics.unwrap_or(true),
    })
}

fn parse_duration_opt(s: &str) -> Option<std::time::Duration> {
    crate::validate::validate_duration(s).ok()
}

pub fn load_and_compile(path: &str) -> Result<RuntimeConfig, crate::error::ConfigError> {
    crate::load_and_validate(path)
}
