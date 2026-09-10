//! Process/timeout/admin compilation.

use crate::error::ConfigError;
use crate::model::ConfigFile;
use crate::validate::validate_duration;

use super::model::*;
use super::{parse_duration_opt, resolve_password};

pub(crate) fn compile_process(config: &ConfigFile) -> ProcessConfig {
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

pub(crate) fn compile_timeouts(config: &ConfigFile) -> Result<TimeoutConfig, ConfigError> {
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

pub(crate) fn compile_admin(config: &ConfigFile) -> Result<Option<AdminConfig>, ConfigError> {
    let Some(admin) = config.admin.as_ref() else {
        return Ok(None);
    };

    let auth = admin.auth.as_ref().map(compile_admin_auth).transpose()?;

    let pac = admin.pac.as_ref().map(|pac_toml| {
        let path = pac_toml.path.clone().unwrap_or_else(|| "/pac".to_string());
        PacConfig {
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
                .map(|entry| StaticRoute {
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

    Ok(Some(AdminConfig {
        bind: admin
            .bind
            .clone()
            .unwrap_or_else(|| "127.0.0.1:9090".to_string()),
        enabled: admin.enabled.unwrap_or(true),
        metrics: admin.metrics.unwrap_or(true),
        auth,
        pac,
        static_content,
    }))
}

pub(crate) fn compile_admin_auth(
    auth: &crate::model::AdminAuthConfig,
) -> Result<AdminAuthConfig, ConfigError> {
    let bearer_token = resolve_password(
        auth.bearer_token.as_deref(),
        auth.bearer_token_env.as_deref(),
        "admin.auth.bearer_token",
    )?;

    if bearer_token.as_deref().is_some_and(str::is_empty) {
        return Err(ConfigError::validation(
            "admin.auth.bearer_token",
            "bearer token must not be empty",
        ));
    }

    let basic = auth.basic_auth.as_ref();
    if bearer_token.is_some() && basic.is_some() {
        return Err(ConfigError::validation(
            "admin.auth",
            "configure either bearer_token or basic_auth, not both",
        ));
    }

    let (basic_username, basic_password) = if let Some(basic) = basic {
        let password = resolve_password(
            basic.password.as_deref(),
            basic.password_env.as_deref(),
            "admin.auth.basic_auth",
        )?;
        let Some(password) = password else {
            return Err(ConfigError::validation(
                "admin.auth.basic_auth",
                "basic_auth requires password or password_env",
            ));
        };
        if basic.user.is_empty() || password.is_empty() {
            return Err(ConfigError::validation(
                "admin.auth.basic_auth",
                "basic_auth user and password must not be empty",
            ));
        }
        (Some(basic.user.clone()), Some(password))
    } else {
        (None, None)
    };

    if bearer_token.is_none() && basic_username.is_none() {
        return Err(ConfigError::validation(
            "admin.auth",
            "authentication requires bearer_token or basic_auth",
        ));
    }

    Ok(AdminAuthConfig {
        bearer_token,
        basic_username,
        basic_password,
    })
}
