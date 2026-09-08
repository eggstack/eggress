//! Security validation: warnings for dangerous combinations and
//! pproxy-compatibility alias notices. Non-fatal by design.

use super::listeners::is_loopback_bind;
use super::rules::MAX_MATCH_EXPR_DEPTH;
use crate::error::ConfigWarning;
use crate::model::{ConfigFile, MatchExprConfig};
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
            }
        }
    }

    // 35.4 / 35.7: Warn about non-loopback admin bind
    if let Some(ref admin) = config.admin {
        if let Some(ref bind) = admin.bind {
            if !is_loopback_bind(bind) && admin.auth.is_none() {
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

    warnings
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
