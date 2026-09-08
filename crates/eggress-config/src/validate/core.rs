//! Scalar section validation: durations, timeouts, process, admin.

use std::collections::HashSet;

use super::listeners::is_loopback_bind;
use crate::error::ConfigError;
pub(crate) fn parse_duration(s: &str) -> Result<std::time::Duration, String> {
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

pub(crate) fn validate_timeouts(
    timeouts: &crate::model::TimeoutConfig,
    errors: &mut Vec<ConfigError>,
) {
    if let Some(ref handshake) = timeouts.handshake {
        if let Ok(d) = parse_duration(handshake) {
            if d.is_zero() {
                errors.push(ConfigError::validation(
                    "timeouts.handshake",
                    &format!("must be greater than 0, got: {}", handshake),
                ));
            }
        } else {
            errors.push(ConfigError::validation(
                "timeouts.handshake",
                &format!("invalid duration: {}", handshake),
            ));
        }
    }
    if let Some(ref connect) = timeouts.connect {
        if let Ok(d) = parse_duration(connect) {
            if d.is_zero() {
                errors.push(ConfigError::validation(
                    "timeouts.connect",
                    &format!("must be greater than 0, got: {}", connect),
                ));
            }
        } else {
            errors.push(ConfigError::validation(
                "timeouts.connect",
                &format!("invalid duration: {}", connect),
            ));
        }
    }
}

pub(crate) fn validate_process(
    process: &crate::model::ProcessConfig,
    errors: &mut Vec<ConfigError>,
) {
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

pub(crate) fn validate_admin(admin: &crate::model::AdminConfig, errors: &mut Vec<ConfigError>) {
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

    if admin.enabled.unwrap_or(true)
        && admin
            .bind
            .as_deref()
            .is_some_and(|bind| !is_loopback_bind(bind))
        && admin.auth.is_none()
    {
        errors.push(ConfigError::validation(
            "admin.auth",
            "non-loopback admin binds require authentication",
        ));
    }

    if let Some(auth) = &admin.auth {
        if auth.bearer_token.as_deref().is_some_and(str::is_empty) {
            errors.push(ConfigError::validation(
                "admin.auth.bearer_token",
                "bearer token must not be empty",
            ));
        }
        if auth.bearer_token.is_some() && auth.bearer_token_env.is_some() {
            errors.push(ConfigError::validation(
                "admin.auth",
                "configure either bearer_token or bearer_token_env, not both",
            ));
        }
        if let Some(basic) = &auth.basic_auth {
            if basic.user.is_empty() {
                errors.push(ConfigError::validation(
                    "admin.auth.basic_auth.user",
                    "basic auth username must not be empty",
                ));
            }
            if basic.password.as_deref().is_some_and(str::is_empty) {
                errors.push(ConfigError::validation(
                    "admin.auth.basic_auth.password",
                    "basic auth password must not be empty",
                ));
            }
            if basic.password.is_some() && basic.password_env.is_some() {
                errors.push(ConfigError::validation(
                    "admin.auth.basic_auth",
                    "configure either password or password_env, not both",
                ));
            }
        }
        if auth.bearer_token.is_some() && auth.basic_auth.is_some() {
            errors.push(ConfigError::validation(
                "admin.auth",
                "configure either bearer_token or basic_auth, not both",
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
