//! Rule validation: matchers, group references, and UDP-match analysis.

use std::collections::HashSet;

use super::composition::{VALID_PROTOCOLS, VALID_REJECT_REASONS};
use crate::error::ConfigError;
use crate::model::{LeafMatcher, MatchExprConfig};
pub(crate) fn rule_upstream_group_could_match_udp(rule: &crate::model::RuleConfig) -> bool {
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

pub(crate) const MAX_MATCH_EXPR_DEPTH: usize = 10;

pub(crate) fn matcher_could_match_udp(matcher: &MatchExprConfig) -> bool {
    matcher_could_match_udp_limited(matcher, 0)
}

pub(crate) fn matcher_could_match_udp_limited(matcher: &MatchExprConfig, depth: usize) -> bool {
    // The full validator rejects deeper expressions. Stay conservative here
    // so this pre-validation diagnostic never suppresses a useful warning.
    if depth >= MAX_MATCH_EXPR_DEPTH {
        return true;
    }
    match matcher {
        MatchExprConfig::Leaf(leaf) => leaf_could_match_udp(leaf),
        MatchExprConfig::Composite(composite) => {
            if let Some(ref all) = composite.all {
                return all
                    .iter()
                    .all(|child| matcher_could_match_udp_limited(child, depth + 1));
            }
            if let Some(ref any_of) = composite.any_of {
                return any_of
                    .iter()
                    .any(|child| matcher_could_match_udp_limited(child, depth + 1));
            }
            if let Some(ref not) = composite.not {
                return matcher_could_match_udp_limited(not, depth + 1);
            }
            true
        }
    }
}

pub(crate) fn leaf_could_match_udp(leaf: &LeafMatcher) -> bool {
    if let Some(ref transport) = leaf.transport {
        return transport == "udp";
    }
    true
}

pub(crate) fn validate_rules(
    rules: &[crate::model::RuleConfig],
    groups: Option<&[crate::model::UpstreamGroupConfig]>,
    errors: &mut Vec<ConfigError>,
) {
    let group_ids: HashSet<&str> = groups
        .map(|g| g.iter().map(|g| g.id.as_str()).collect())
        .unwrap_or_default();

    for (i, rule) in rules.iter().enumerate() {
        let path = format!("rules[{}]", i);

        let matcher_count = [
            rule.host_exact.is_some(),
            rule.host_suffix.is_some(),
            rule.host_regex.is_some(),
            rule.destination_port.is_some(),
            rule.destination_port_regex.is_some(),
            rule.any.unwrap_or(false),
        ]
        .iter()
        .filter(|&&b| b)
        .count();

        if rule.match_expr.is_none() {
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
            if matcher_count > 0 {
                errors.push(ConfigError::validation(
                    &path,
                    "rule must not combine match with legacy matcher fields",
                ));
            }
            validate_match_expr(match_expr, &path, errors, 0);
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

pub(crate) fn validate_match_expr(
    expr: &crate::model::MatchExprConfig,
    path: &str,
    errors: &mut Vec<ConfigError>,
    depth: usize,
) {
    if depth >= MAX_MATCH_EXPR_DEPTH {
        errors.push(ConfigError::validation(
            path,
            &format!(
                "expression exceeds maximum depth ({})",
                MAX_MATCH_EXPR_DEPTH
            ),
        ));
        return;
    }
    match expr {
        crate::model::MatchExprConfig::Composite(composite) => {
            let has_all = composite.all.is_some();
            let has_any = composite.any_of.is_some();
            let has_not = composite.not.is_some();
            if !has_all && !has_any && !has_not {
                errors.push(ConfigError::validation(
                    &format!("{}.match", path),
                    "composite must have exactly one of: all, any_of, not",
                ));
            }
            if let Some(ref all) = composite.all {
                if all.is_empty() {
                    errors.push(ConfigError::validation(
                        &format!("{}.match.all", path),
                        "must not be empty",
                    ));
                }
                for (j, item) in all.iter().enumerate() {
                    validate_match_expr(
                        item,
                        &format!("{}.match.all[{}]", path, j),
                        errors,
                        depth + 1,
                    );
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
                    validate_match_expr(
                        item,
                        &format!("{}.match.any_of[{}]", path, j),
                        errors,
                        depth + 1,
                    );
                }
            }
            if let Some(ref not) = composite.not {
                validate_match_expr(not, &format!("{}.match.not", path), errors, depth + 1);
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
                if !VALID_PROTOCOLS.contains(&proto.as_str()) && proto != "httponly" {
                    errors.push(ConfigError::validation(
                        &format!("{}.protocol", path),
                        &format!("unknown protocol: {}", proto),
                    ));
                }
            }
        }
    }
}
