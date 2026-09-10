//! Routing rule compilation.
//!
//! Converts validated TOML rule/matcher/action shapes into
//! `eggress_routing` matchers without duplicating validation.

use std::sync::Arc;

use eggress_core::{ProtocolId, RejectReason};
use eggress_routing::UpstreamGroupId;

use crate::error::ConfigError;
use crate::model::{ConfigFile, LeafMatcher, MatchExprConfig, RuleConfig};

pub(crate) fn compile_reject_reason(s: &str) -> Result<RejectReason, ConfigError> {
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

pub(crate) fn compile_protocol(s: &str) -> Result<ProtocolId, ConfigError> {
    match s {
        "http" => Ok(ProtocolId::Http),
        "httponly" => Ok(ProtocolId::Http),
        "socks4" => Ok(ProtocolId::Socks4),
        "socks5" => Ok(ProtocolId::Socks5),
        "shadowsocks" => Ok(ProtocolId::Shadowsocks),
        "ssr" => Ok(ProtocolId::ShadowsocksR),
        "trojan" => Ok(ProtocolId::Trojan),
        "h2" => Ok(ProtocolId::Http2),
        "h3" => {
            #[cfg(feature = "quic")]
            {
                Ok(ProtocolId::Http3)
            }
            #[cfg(not(feature = "quic"))]
            {
                Err(ConfigError::validation(
                    "protocols",
                    "HTTP/3 requires the optional 'quic' feature",
                ))
            }
        }
        "quic" => {
            #[cfg(feature = "quic")]
            {
                Ok(ProtocolId::Quic)
            }
            #[cfg(not(feature = "quic"))]
            {
                Err(ConfigError::validation(
                    "protocols",
                    "QUIC requires the optional 'quic' feature",
                ))
            }
        }
        "websocket" | "ws" | "wss" => Ok(ProtocolId::WebSocket),
        "raw" | "tunnel" => Ok(ProtocolId::Raw),
        "echo" => Ok(ProtocolId::Echo),
        _ => Err(ConfigError::validation(
            "protocols",
            &format!("unknown protocol: {}", s),
        )),
    }
}

pub(crate) fn compile_transport(s: &str) -> Result<eggress_routing::TransportKind, ConfigError> {
    match s {
        "tcp" => Ok(eggress_routing::TransportKind::Tcp),
        "udp" => Ok(eggress_routing::TransportKind::Udp),
        "reverse_tcp" => Ok(eggress_routing::TransportKind::ReverseTcp),
        _ => Err(ConfigError::validation(
            "transport",
            &format!("unknown transport: {}", s),
        )),
    }
}

pub(crate) fn compile_matcher(
    rule: &RuleConfig,
) -> Result<eggress_routing::MatchExpr, ConfigError> {
    if let Some(ref match_expr) = rule.match_expr {
        return compile_match_config(match_expr);
    }

    if let Some(ref exact) = rule.host_exact {
        if rule.host_suffix.is_none()
            && rule.host_regex.is_none()
            && rule.destination_port.is_none()
            && rule.destination_port_regex.is_none()
            && !rule.any.unwrap_or(false)
        {
            return Ok(eggress_routing::MatchExpr::HostExact(Arc::from(
                eggress_routing::normalize_host_for_exact(exact),
            )));
        }
    }
    if let Some(ref suffix) = rule.host_suffix {
        if rule.host_exact.is_none()
            && rule.host_regex.is_none()
            && rule.destination_port.is_none()
            && rule.destination_port_regex.is_none()
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
            && rule.destination_port_regex.is_none()
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
    if let Some(ref regex_str) = rule.destination_port_regex {
        let re = regex::Regex::new(regex_str).map_err(|e| {
            ConfigError::validation(
                "destination_port_regex",
                &format!("invalid regex '{}': {}", regex_str, e),
            )
        })?;
        if rule.host_exact.is_none()
            && rule.host_suffix.is_none()
            && rule.host_regex.is_none()
            && rule.destination_port.is_none()
            && !rule.any.unwrap_or(false)
        {
            return Ok(eggress_routing::MatchExpr::DestinationPortRegex(re));
        }
    }
    if let Some(port) = rule.destination_port {
        if rule.host_exact.is_none()
            && rule.host_suffix.is_none()
            && rule.host_regex.is_none()
            && rule.destination_port_regex.is_none()
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

pub(crate) fn compile_match_config(
    config: &MatchExprConfig,
) -> Result<eggress_routing::MatchExpr, ConfigError> {
    let mut node_count = 0;
    compile_match_config_limited(config, 0, &mut node_count)
}

pub(crate) fn compile_match_config_limited(
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
    if depth >= MAX_EXPRESSION_DEPTH {
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

pub(crate) fn compile_leaf_matcher(
    leaf: &LeafMatcher,
) -> Result<eggress_routing::MatchExpr, ConfigError> {
    let mut matchers = Vec::new();

    if let Some(ref exact) = leaf.host_exact {
        matchers.push(eggress_routing::MatchExpr::HostExact(Arc::from(
            eggress_routing::normalize_host_for_exact(exact),
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
    if let Some(ref regex_str) = leaf.destination_port_regex {
        let re = regex::Regex::new(regex_str).map_err(|e| {
            ConfigError::validation(
                "destination_port_regex",
                &format!("invalid regex '{}': {}", regex_str, e),
            )
        })?;
        matchers.push(eggress_routing::MatchExpr::DestinationPortRegex(re));
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
        let matcher = eggress_routing::PortMatcher::new_range(range[0], range[1])
            .map_err(|e| ConfigError::validation("destination_port_range", &e))?;
        matchers.push(eggress_routing::MatchExpr::DestinationPort(matcher));
    }
    if let Some(ref ports) = leaf.destination_port_set {
        if ports.is_empty() {
            return Err(ConfigError::validation(
                "destination_port_set",
                "must not be empty",
            ));
        }
        let matcher = eggress_routing::PortMatcher::new_set(ports.clone());
        matchers.push(eggress_routing::MatchExpr::DestinationPort(matcher));
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
    if let Some(ref name) = leaf.reverse_listener {
        matchers.push(eggress_routing::MatchExpr::ReverseListener(Arc::from(
            name.as_str(),
        )));
    }

    match matchers.len() {
        0 => Ok(eggress_routing::MatchExpr::Any),
        1 => Ok(matchers.into_iter().next().expect("len checked to be 1")),
        _ => Ok(eggress_routing::MatchExpr::All(matchers)),
    }
}

pub(crate) fn compile_action(
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

pub(crate) fn compile_rules(
    config: &ConfigFile,
) -> Result<Vec<eggress_routing::CompiledRule>, ConfigError> {
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
        if group_ids.len() > 1 {
            return Err(ConfigError::validation(
                "rules_file",
                "rules_file routes all rules to a single group; multiple groups are not supported with rules_file — use explicit [[rules]] instead",
            ));
        }
        let content = crate::file::load_rules_file(rules_file_path).map_err(|e| {
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

pub(crate) fn compile_default_action(config: &ConfigFile) -> eggress_routing::RouteActionSpec {
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
