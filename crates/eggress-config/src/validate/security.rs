//! Security validation: warnings for dangerous combinations and
//! pproxy-compatibility alias notices. Non-fatal by design.

use eggress_core::connector::is_reserved_or_private_ip;

use super::listeners::is_loopback_bind;
use super::rules::MAX_MATCH_EXPR_DEPTH;
use crate::error::ConfigWarning;
use crate::model::{ConfigFile, MatchExprConfig, RuleConfig};

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
                let has_shadowsocks = listener.shadowsocks.is_some();
                let has_ssr = listener.ssr.is_some();
                let has_trojan = listener.trojan.is_some();
                if !has_auth && !has_shadowsocks && !has_ssr && !has_trojan {
                    warnings.push(ConfigWarning {
                        path,
                        message: format!(
                            "listener '{}' binds to {} without authentication — \
                             this may expose the proxy to untrusted networks",
                            listener.name, listener.bind,
                        ),
                    });
                }
                // Password-authenticated HTTP/SOCKS on a non-loopback bind
                // without TLS sends the credential in clear on the wire.
                // Shadowsocks and Trojan carry their own encryption, and a
                // configured [listeners.tls] terminates TLS in front of the
                // whole listener, so neither needs this warning.
                if has_auth
                    && listener.tls.is_none()
                    && listener
                        .protocols
                        .iter()
                        .any(|protocol| matches!(protocol.as_str(), "http" | "socks4" | "socks5"))
                {
                    warnings.push(ConfigWarning {
                        path: format!("listeners[{}].tls", i),
                        message: format!(
                            "listener '{}' binds to {} with password authentication but \
                             no [listeners.tls] — HTTP/SOCKS credentials may be transmitted \
                             in clear; configure TLS, bind to loopback, or use a trusted tunnel",
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
                // M8: the admin protocol is plaintext HTTP; there is no
                // native TLS listener (`eggress-admin` rejects `https://`
                // admin client URLs too). Warn regardless of auth so an
                // operator never assumes the credential protects the wire.
                warnings.push(ConfigWarning {
                    path: "admin.bind".to_string(),
                    message: format!(
                        "admin server binds to {} over plaintext HTTP — Basic/Bearer \
                         credentials, status, metrics, and routing topology are exposed to \
                         the network; bind to loopback, use a VPN or SSH tunnel, or place a \
                         TLS-terminating reverse proxy in front and set [admin] \
                         allowed_hosts to the proxy host name",
                        bind,
                    ),
                });
                if admin.auth.is_none() {
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
    }

    // 35.5 / 35.7: Warn about non-loopback reverse control_bind without auth
    if let Some(ref servers) = config.reverse_servers {
        for (i, server) in servers.iter().enumerate() {
            let path = format!("reverse_servers[{}].control_bind", i);
            if !is_loopback_bind(&server.control_bind) {
                let has_auth = server.auth_username.is_some()
                    && (server.auth_password.is_some() || server.auth_password_env.is_some());
                if !has_auth {
                    warnings.push(ConfigWarning {
                        path,
                        message: format!(
                            "reverse server '{}' control channel binds to {} without authentication — \
                             any client can connect and request proxying",
                            server.id, server.control_bind,
                        ),
                    });
                }
                // M-11: reverse control auth is sent in plaintext without TLS
                // at the protocol level; callers must layer TLS externally when
                // binding non-loopback, even with auth. Documented here as
                // best-effort advisory — no additional warning emitted to avoid
                // breaking existing valid configurations that rely on external TLS
                // termination.
            }
        }
    }

    warn_protocol_aliases(config, &mut warnings);
    warn_literal_only_cidr_reject_rules(config, &mut warnings);

    warnings
}

/// Warn when a `reject` rule relies on a private/reserved `destination_cidr`
/// without any host condition.
///
/// `DestinationCidr` matches literal IP targets only and never resolves a
/// name, so such a rule is not a complete private-network ACL: a hostname that
/// resolves into the range bypasses it, and a request sent through an upstream
/// group is resolved by the upstream. The warning is advisory — rejecting
/// literal private targets on purpose stays valid — and never includes a
/// credential or upstream URI.
fn warn_literal_only_cidr_reject_rules(config: &ConfigFile, warnings: &mut Vec<ConfigWarning>) {
    let Some(ref rules) = config.rules else {
        return;
    };
    for (i, rule) in rules.iter().enumerate() {
        if rule.reject.is_none() {
            continue;
        }
        let mut host_condition = rule_has_host_condition(rule);
        let mut private_cidr = None;
        if let Some(ref expr) = rule.match_expr {
            collect_private_destination_cidr(
                expr,
                &format!("rules[{}].match", i),
                0,
                &mut host_condition,
                &mut private_cidr,
            );
        }
        let Some((path, cidr)) = private_cidr else {
            continue;
        };
        if host_condition {
            // The rule already scopes names; an operator combining a CIDR
            // with a host condition is not relying on CIDR alone.
            continue;
        }
        warnings.push(ConfigWarning {
            path,
            message: format!(
                "rule '{}' rejects destinations in {} by destination_cidr, but CIDR \
                 matching applies to literal IP targets only — a hostname that resolves \
                 into {} is not matched by this rule, and a name routed through an \
                 upstream group is resolved by the upstream, not locally; add a \
                 host_exact/host_suffix/host_regex condition, or use allow_private_egress \
                 or a trusted upstream for name-based private-network policy",
                rule.id, cidr, cidr,
            ),
        });
    }
}

fn rule_has_host_condition(rule: &RuleConfig) -> bool {
    rule.host_exact.is_some() || rule.host_suffix.is_some() || rule.host_regex.is_some()
}

/// Walk a match expression, recording (a) whether any host condition scopes
/// the rule to names and (b) the path and normalized form of the first
/// `destination_cidr` that overlaps a private/reserved range.
fn collect_private_destination_cidr(
    expr: &MatchExprConfig,
    path: &str,
    depth: usize,
    host_condition: &mut bool,
    private_cidr: &mut Option<(String, String)>,
) {
    if depth >= MAX_MATCH_EXPR_DEPTH {
        return;
    }
    match expr {
        MatchExprConfig::Composite(composite) => {
            if let Some(ref all) = composite.all {
                for (i, child) in all.iter().enumerate() {
                    collect_private_destination_cidr(
                        child,
                        &format!("{path}.all[{i}]"),
                        depth + 1,
                        host_condition,
                        private_cidr,
                    );
                }
            }
            if let Some(ref any_of) = composite.any_of {
                for (i, child) in any_of.iter().enumerate() {
                    collect_private_destination_cidr(
                        child,
                        &format!("{path}.any_of[{i}]"),
                        depth + 1,
                        host_condition,
                        private_cidr,
                    );
                }
            }
            if let Some(ref not) = composite.not {
                collect_private_destination_cidr(
                    not,
                    &format!("{path}.not"),
                    depth + 1,
                    host_condition,
                    private_cidr,
                );
            }
        }
        MatchExprConfig::Leaf(leaf) => {
            if leaf.host_exact.is_some() || leaf.host_suffix.is_some() || leaf.host_regex.is_some()
            {
                *host_condition = true;
            }
            if private_cidr.is_none() {
                if let Some(ref raw) = leaf.destination_cidr {
                    if let Ok(cidr) = raw.parse::<ipnet::IpNet>() {
                        if cidr_touches_reserved_range(&cidr) {
                            *private_cidr =
                                Some((format!("{path}.destination_cidr"), cidr.to_string()));
                        }
                    }
                }
            }
        }
    }
}

/// True when a CIDR overlaps the private/reserved address space.
///
/// Both endpoints of the range are classified: a supernet such as
/// `0.0.0.0/0` or `128.0.0.0/1` encloses private/reserved space even though
/// its network address is not itself RFC 1918. IPv4-mapped IPv6 forms are
/// classified as their IPv4 equivalent by the shared helper.
fn cidr_touches_reserved_range(cidr: &ipnet::IpNet) -> bool {
    is_reserved_or_private_ip(&cidr.network()) || is_reserved_or_private_ip(&cidr.broadcast())
}

pub(crate) fn warn_protocol_aliases(config: &ConfigFile, warnings: &mut Vec<ConfigWarning>) {
    if let Some(ref rules) = config.rules {
        for (i, rule) in rules.iter().enumerate() {
            let base = format!("rules[{i}]");
            if let Some(ref expr) = rule.match_expr {
                walk_match_expr_for_alias(expr, &format!("{base}.match"), warnings, 0);
            }
        }
    }
}

pub(crate) fn walk_match_expr_for_alias(
    expr: &MatchExprConfig,
    path: &str,
    warnings: &mut Vec<ConfigWarning>,
    depth: usize,
) {
    if depth >= MAX_MATCH_EXPR_DEPTH {
        return;
    }
    match expr {
        MatchExprConfig::Composite(composite) => {
            if let Some(ref all) = composite.all {
                for (i, child) in all.iter().enumerate() {
                    walk_match_expr_for_alias(
                        child,
                        &format!("{path}.all[{i}]"),
                        warnings,
                        depth + 1,
                    );
                }
            }
            if let Some(ref any) = composite.any_of {
                for (i, child) in any.iter().enumerate() {
                    walk_match_expr_for_alias(
                        child,
                        &format!("{path}.any_of[{i}]"),
                        warnings,
                        depth + 1,
                    );
                }
            }
            if let Some(ref not) = composite.not {
                walk_match_expr_for_alias(not, &format!("{path}.not"), warnings, depth + 1);
            }
        }
        MatchExprConfig::Leaf(leaf) => {
            if leaf.protocol.as_deref() == Some("httponly") {
                warnings.push(ConfigWarning {
                    path: format!("{path}.protocol"),
                    message: "'httponly' is a pproxy compatibility alias for 'http' \
                              and does not select distinct protocol semantics"
                        .to_string(),
                });
            }
        }
    }
}
