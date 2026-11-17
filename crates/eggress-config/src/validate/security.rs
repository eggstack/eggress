//! Security validation: warnings for dangerous combinations and
//! pproxy-compatibility alias notices. Non-fatal by default; become errors
//! when `[process.security] fail_on_security_warnings = true`.

use super::listeners::is_loopback_bind;
use super::rules::MAX_MATCH_EXPR_DEPTH;
use crate::error::{ConfigError, ConfigWarning};
use crate::model::{ConfigFile, MatchExprConfig};

/// Emit security warnings for dangerous config combinations.
///
/// This runs after structural validation succeeds and produces non-fatal
/// warnings about configurations that could expose services to untrusted
/// networks without authentication. Callers that honor
/// `[process.security] fail_on_security_warnings` map these to errors via
/// [`fail_closed_on_security_warnings`].
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

            // Standalone pproxy UDP on a non-loopback bind has no TCP auth
            // channel (parity-locked). Surface the open-relay risk at check/
            // startup; fatal only when fail-closed is enabled.
            if let Some(ref udp) = listener.udp {
                let udp_enabled = udp.enabled.or(listener.udp_enabled).unwrap_or(false);
                let is_standalone = udp
                    .mode
                    .as_deref()
                    .is_some_and(|m| m.eq_ignore_ascii_case("standalone_pproxy_udp"));
                if udp_enabled && is_standalone {
                    let udp_bind = udp.bind.as_deref().unwrap_or(&listener.bind);
                    if !is_loopback_bind(udp_bind) {
                        warnings.push(ConfigWarning {
                            path: format!("listeners[{i}].udp.bind"),
                            message: format!(
                                "listener '{}' enables standalone_pproxy_udp on {} without a TCP \
                                 auth channel — prefer socks5_udp_associate or bind loopback; \
                                 set [process.security] fail_on_security_warnings to reject at startup",
                                listener.name, udp_bind,
                            ),
                        });
                    }
                }
            }
        }
    }

    // 35.4 / 35.7: Warn about non-loopback admin bind without auth, and about
    // loopback admin without auth when require_auth is requested.
    if let Some(ref admin) = config.admin {
        let enabled = admin.enabled.unwrap_or(true);
        if enabled && admin.auth.is_none() {
            let bind = admin.bind.as_deref().unwrap_or("127.0.0.1:9090");
            let require_auth = admin.require_auth.unwrap_or(false);
            if !is_loopback_bind(bind) {
                warnings.push(ConfigWarning {
                    path: "admin.bind".to_string(),
                    message: format!(
                        "admin server binds to {bind} without authentication — \
                         this may expose admin endpoints to untrusted networks",
                    ),
                });
            } else if require_auth {
                warnings.push(ConfigWarning {
                    path: "admin.require_auth".to_string(),
                    message: format!(
                        "admin server binds to {bind} without authentication while \
                         admin.require_auth is true — configure [admin.auth] \
                         (bearer_token or basic_auth)",
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

/// Whether the config requests fail-closed security warnings.
pub fn fail_on_security_warnings_enabled(config: &ConfigFile) -> bool {
    config
        .process
        .as_ref()
        .and_then(|p| p.security.as_ref())
        .and_then(|s| s.fail_on_security_warnings)
        .unwrap_or(false)
}

/// Map security warnings to a validation error when fail-closed is enabled.
///
/// Version-omission and protocol-alias notices stay advisory even under
/// fail-closed; only warnings whose path indicates a security surface become
/// fatal (listeners/admin/reverse/udp).
pub fn fail_closed_on_security_warnings(
    config: &ConfigFile,
    warnings: &[ConfigWarning],
) -> Result<(), ConfigError> {
    if !fail_on_security_warnings_enabled(config) {
        return Ok(());
    }
    let fatal: Vec<&ConfigWarning> = warnings
        .iter()
        .filter(|w| is_fail_closed_security_warning(w))
        .collect();
    if fatal.is_empty() {
        return Ok(());
    }
    let messages: Vec<String> = fatal
        .iter()
        .map(|w| format!("{}: {}", w.path, w.message))
        .collect();
    Err(ConfigError::validation(
        "process.security.fail_on_security_warnings",
        &format!(
            "security warnings treated as errors (fail_on_security_warnings=true): {}",
            messages.join("; ")
        ),
    ))
}

fn is_fail_closed_security_warning(warning: &ConfigWarning) -> bool {
    let path = warning.path.as_str();
    path.starts_with("listeners[")
        || path.starts_with("admin.")
        || path.starts_with("reverse_servers[")
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
