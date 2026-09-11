//! TOML renderer: presentation-only serialization of intermediates.
//!
//! Used for `--dump-config`, migration output, and debugging. Internal
//! consumers use the native path instead; semantic classification never lives
//! here.

use super::model::{
    AdminToml, ConfigToml, ListenerToml, PacToml, ReverseClientToml, ReverseServerToml, RuleToml,
    StaticContentToml, UpstreamGroupToml, UpstreamToml,
};
use crate::error::CompatError;

pub(crate) struct TomlInput<'a> {
    pub(crate) listeners: &'a [ListenerToml],
    pub(crate) upstreams: &'a [UpstreamToml],
    pub(crate) upstream_groups: &'a [UpstreamGroupToml],
    pub(crate) rules: &'a [RuleToml],
    pub(crate) reverse_servers: &'a [ReverseServerToml],
    pub(crate) reverse_clients: &'a [ReverseClientToml],
    pub(crate) pac_enabled: bool,
    pub(crate) pac_path: Option<String>,
    pub(crate) static_content: &'a [StaticContentToml],
}

pub(crate) fn generate_toml(input: TomlInput<'_>) -> Result<String, CompatError> {
    let admin = if input.pac_enabled || !input.static_content.is_empty() {
        // PAC `proxy` is the bare host:port directive (the admin renderer
        // prepends `PROXY `); use the first listener bind so the generated
        // PAC points at a real local proxy instead of a literal placeholder.
        let proxy = input
            .listeners
            .first()
            .map(|l| l.bind.clone())
            .unwrap_or_else(|| "127.0.0.1:8080".to_string());
        Some(AdminToml {
            pac: PacToml {
                path: input.pac_path.or_else(|| Some("/proxy.pac".to_string())),
                proxy: Some(proxy),
                direct_fallback: Some(true),
            },
            static_content: input.static_content.to_vec(),
        })
    } else {
        None
    };

    let config = ConfigToml {
        version: 1,
        listeners: input.listeners.to_vec(),
        upstreams: input.upstreams.to_vec(),
        upstream_groups: input.upstream_groups.to_vec(),
        rules: input.rules.to_vec(),
        reverse_servers: input.reverse_servers.to_vec(),
        reverse_clients: input.reverse_clients.to_vec(),
        admin,
    };

    toml::to_string_pretty(&config).map_err(|e| CompatError::ConfigValidation {
        message: format!("failed to serialize config: {e}"),
    })
}
