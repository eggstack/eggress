//! Configuration validation, split by validated domain.
//!
//! [`validate_config`] orchestrates per-domain validators; each submodule
//! owns one coherent responsibility (composition matrix, listeners,
//! upstreams/groups, rules/matchers, scalar sections, security warnings).
//! No validation semantics changed in the split.

use crate::error::ConfigError;
use crate::model::ConfigFile;

pub(crate) mod composition;
pub(crate) mod core;
pub(crate) mod listeners;
pub(crate) mod rules;
pub(crate) mod security;
#[cfg(test)]
mod tests;
pub(crate) mod upstreams;

pub use composition::validate_config_composition;
pub use core::validate_duration;
pub use security::validate_config_security;

use core::{validate_admin, validate_process, validate_timeouts};
use listeners::validate_listeners;
use rules::validate_rules;
use upstreams::{validate_upstream_groups, validate_upstream_transport, validate_upstreams};

pub fn validate_config(config: &ConfigFile) -> Result<(), Vec<ConfigError>> {
    let mut errors = Vec::new();

    if let Some(version) = config.version {
        if version != 1 {
            errors.push(ConfigError::UnsupportedVersion(version));
        }
    }

    if let Some(ref listeners) = config.listeners {
        validate_listeners(listeners, &mut errors);
    }

    if let Some(ref upstreams) = config.upstreams {
        validate_upstreams(upstreams, &mut errors);
    }

    if let Some(ref groups) = config.upstream_groups {
        validate_upstream_groups(groups, config.upstreams.as_deref(), &mut errors);
    }

    if let Some(ref rules) = config.rules {
        validate_rules(rules, config.upstream_groups.as_deref(), &mut errors);
    }

    if let Some(ref timeouts) = config.timeouts {
        validate_timeouts(timeouts, &mut errors);
    }

    if let Some(ref process) = config.process {
        validate_process(process, &mut errors);
    }

    if let Some(ref admin) = config.admin {
        validate_admin(admin, &mut errors);
    }

    if let Some(ref routing) = config.routing {
        if let Some(ref default) = routing.default {
            if default != "direct" && default != "reject" {
                let group_ids: Vec<&str> = config
                    .upstream_groups
                    .as_ref()
                    .map(|gs| gs.iter().map(|g| g.id.as_str()).collect())
                    .unwrap_or_default();
                if !group_ids.contains(&default.as_str()) {
                    errors.push(ConfigError::validation(
                        "routing.default",
                        &format!("unknown upstream group or action: {}", default),
                    ));
                }
            }
        }
    }

    if let Some(ref upstreams) = config.upstreams {
        if let Some(ref groups) = config.upstream_groups {
            validate_upstream_transport(upstreams, groups, config, &mut errors);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
