//! Compiled runtime configuration facade.
//!
//! `parse -> validate -> compile` is one-way. [`compile_config`] wires the
//! domain modules below; [`load_and_compile`] adds file I/O for file-backed
//! startup. Public compiled types stay reachable at
//! `eggress_config::compile::…` via re-exports.

pub mod listeners;
pub mod model;
pub mod process;
pub mod reverse;
pub mod rules;
pub mod upstreams;

pub use model::{
    AdminAuthConfig, AdminConfig, CompiledH2Config, CompiledListenerTlsConfig,
    CompiledListenerUdpConfig, CompiledReverseClientConfig, CompiledReverseClientTls,
    CompiledReverseServerConfig, CompiledReverseServerTls, CompiledTransparentConfig,
    CompiledUnixListenerConfig, GroupFallback, ListenerConfig, PacConfig, ProcessConfig,
    RuntimeConfig, StaticRoute, TimeoutConfig, UpstreamConfig, UpstreamGroupConfig,
};

use crate::error::ConfigError;
use crate::model::ConfigFile;

use listeners::compile_listeners;
use process::{compile_admin, compile_process, compile_timeouts};
use reverse::{compile_reverse_clients, compile_reverse_servers};
use rules::{compile_default_action, compile_rules};
use upstreams::{compile_groups, compile_upstreams};

pub fn compile_config(config: &ConfigFile) -> Result<RuntimeConfig, ConfigError> {
    let process = compile_process(config);
    let timeouts = compile_timeouts(config)?;
    let listeners = compile_listeners(config)?;
    let upstreams = compile_upstreams(config)?;
    let groups = compile_groups(config)?;
    let rules = compile_rules(config)?;
    let default_action = compile_default_action(config);
    let admin = compile_admin(config)?;
    let reverse_servers = compile_reverse_servers(config)?;
    let reverse_clients = compile_reverse_clients(config)?;

    Ok(RuntimeConfig {
        process,
        timeouts,
        listeners,
        upstreams,
        groups,
        rules,
        default_action,
        admin,
        reverse_servers,
        reverse_clients,
    })
}

fn resolve_password(
    password: Option<&str>,
    password_env: Option<&str>,
    path: &str,
) -> Result<Option<String>, ConfigError> {
    if let Some(env_var) = password_env {
        std::env::var(env_var).map(Some).map_err(|_| {
            ConfigError::validation(
                path,
                &format!(
                    "environment variable '{}' not set (referenced by auth_password_env)",
                    env_var
                ),
            )
        })
    } else {
        Ok(password.map(|s| s.to_string()))
    }
}

fn parse_duration_opt(s: &str) -> Option<std::time::Duration> {
    crate::validate::validate_duration(s).ok()
}

pub fn load_and_compile(path: &str) -> Result<RuntimeConfig, crate::error::ConfigError> {
    crate::load_and_validate(path)
}
