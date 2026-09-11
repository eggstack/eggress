//! Argument-level entry points: pproxy args become TOML and/or native config.
//!
//! [`translate_pproxy_args`] (TOML output) and
//! [`translate_pproxy_args_to_native`] (TOML plus compiled native config)
//! share the intermediates builder; warnings and unsupported findings are
//! identical across both renderers because both wrap the same issue list.

use super::intermediates::build_intermediates;
use super::native::intermediates_to_config_file;
use super::toml_render::{generate_toml, TomlInput};
use crate::args::PproxyArgs;
use crate::error::CompatError;
use crate::uri::{PproxyChain, PproxyUri};
use crate::warnings::TranslationOutput;

/// Translate pproxy-style arguments into Eggress TOML configuration.
pub fn translate_pproxy_args(args: &PproxyArgs) -> Result<TranslationOutput, CompatError> {
    let local_uris = args.parse_local_uris()?;

    // Parse remote URIs as chains (supports __ hop separator)
    let mut remote_chains = Vec::new();
    let mut chain_warnings = TranslationOutput::new(String::new());
    for raw_remote in args.remotes.iter() {
        match crate::uri::parse_pproxy_chain(raw_remote) {
            Ok(chain) => remote_chains.push(chain),
            Err(e) => {
                return Err(e);
            }
        }
    }

    // Validate chain hops for unsupported protocols
    for chain in &remote_chains {
        let unsupported = crate::uri::validate_chain_hops(chain);
        for (idx, scheme) in unsupported {
            chain_warnings = chain_warnings.with_unsupported(
                "chain-unsupported-hop",
                format!(
                    "chain hop {} in '{}' uses unsupported scheme '{}'",
                    idx + 1,
                    chain.redacted_display(),
                    scheme
                ),
            );
        }
    }

    // Allow empty local_uris when -ul is present (standalone UDP mode).
    // Reads the structured `-ul` field, not the legacy string bucket.
    let has_udp_listen = !args.udp_listen.is_empty();

    if local_uris.is_empty() && !has_udp_listen {
        return Err(CompatError::InvalidArgs {
            message: "no local listener specified (use -l or positional args)".to_string(),
        });
    }

    let mut output = translate_from_uris(args, &local_uris, &remote_chains)?;

    // Merge chain validation warnings
    output = output.with_issues(chain_warnings.issues);

    // Merge unknown-flag diagnostics
    let unknown_warnings = args.unknown_flag_diagnostics();
    output = output.with_warnings(unknown_warnings);

    Ok(output)
}

/// Combined TOML + native translation from pproxy args.
///
/// Builds intermediates once via the shared builder, then renders both the
/// TOML string (for `--dump-config` / migration / display) and the compiled
/// native `RuntimeConfig` (for embed/runtime startup) without any
/// TOML serialize/parse round trip in the native path. Warnings/unsupported
/// are identical across both renderers.
#[derive(Debug, Clone)]
pub struct CombinedTranslation {
    /// TOML string for display/migration.
    pub toml: String,
    /// Compiled native config for startup.
    pub runtime: eggress_config::compile::RuntimeConfig,
    /// Canonical typed findings, identical across renderers.
    pub issues: Vec<crate::issues::CompatIssue>,
}

impl CombinedTranslation {
    /// Whether translation has no blockers.
    pub fn has_unsupported(&self) -> bool {
        use crate::issues::IssueSeverity;
        self.issues
            .iter()
            .any(|i| i.severity == IssueSeverity::Unsupported)
    }

    /// Legacy warning view: all `Warning`-severity issues, in order.
    pub fn warnings(&self) -> Vec<crate::warnings::CompatWarning> {
        self.issues.iter().filter_map(|i| i.to_warning()).collect()
    }

    /// Legacy unsupported view: all `Unsupported`-severity issues, in order.
    pub fn unsupported(&self) -> Vec<crate::warnings::UnsupportedFeature> {
        self.issues
            .iter()
            .filter_map(|i| i.to_unsupported())
            .collect()
    }
}

/// Translate pproxy args to both TOML and native in one intermediates build.
pub fn translate_pproxy_args_to_native(
    args: &PproxyArgs,
) -> Result<CombinedTranslation, CompatError> {
    let local_uris = args.parse_local_uris()?;

    let mut remote_chains = Vec::new();
    let mut chain_unsupported = Vec::new();
    for raw_remote in args.remotes.iter() {
        match crate::uri::parse_pproxy_chain(raw_remote) {
            Ok(chain) => remote_chains.push(chain),
            Err(error) => {
                return Err(error);
            }
        }
    }

    for chain in &remote_chains {
        let unsupported = crate::uri::validate_chain_hops(chain);
        for (idx, scheme) in unsupported {
            chain_unsupported.push(crate::warnings::UnsupportedFeature {
                feature: "chain-unsupported-hop",
                detail: format!(
                    "chain hop {} in '{}' uses unsupported scheme '{}'",
                    idx + 1,
                    chain.redacted_display(),
                    scheme
                ),
            });
        }
    }

    // Allow empty local_uris when -ul is present (standalone UDP mode).
    // Reads the structured `-ul` field, not the legacy string bucket.
    let has_udp_listen = !args.udp_listen.is_empty();

    if local_uris.is_empty() && !has_udp_listen {
        return Err(CompatError::InvalidArgs {
            message: "no local listener specified (use -l or positional args)".to_string(),
        });
    }

    let (intermediates, mut output) = build_intermediates(args, &local_uris, &remote_chains)?;
    output = output.with_unsupported_features(chain_unsupported);
    let unknown_warnings = args.unknown_flag_diagnostics();
    output = output.with_warnings(unknown_warnings);

    let toml_str = generate_toml(TomlInput {
        listeners: &intermediates.listeners,
        upstreams: &intermediates.upstreams,
        upstream_groups: &intermediates.upstream_groups,
        rules: &intermediates.rules,
        reverse_servers: &intermediates.reverse_servers,
        reverse_clients: &intermediates.reverse_clients,
        pac_enabled: intermediates.pac_enabled,
        pac_path: intermediates.pac_path.clone(),
        static_content: &intermediates.static_content,
    })?;

    let config_file = intermediates_to_config_file(&intermediates);
    eggress_config::validate::validate_config(&config_file).map_err(|errors| {
        let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        CompatError::ConfigValidation {
            message: messages.join("; "),
        }
    })?;
    let runtime = eggress_config::compile::compile_config(&config_file).map_err(|error| {
        CompatError::ConfigValidation {
            message: format!("native compile failed: {error}"),
        }
    })?;

    Ok(CombinedTranslation {
        toml: toml_str,
        runtime,
        issues: output.issues,
    })
}

/// Translate pproxy-style local and remote URIs into Eggress TOML.
pub fn translate_from_uris(
    args: &PproxyArgs,
    local_uris: &[PproxyUri],
    remote_chains: &[PproxyChain],
) -> Result<TranslationOutput, CompatError> {
    let (intermediates, output) = build_intermediates(args, local_uris, remote_chains)?;
    // TOML rendering is presentation only; native compilation uses
    // `build_intermediates` + `intermediates_to_config_file` without string.
    let toml_str = generate_toml(TomlInput {
        listeners: &intermediates.listeners,
        upstreams: &intermediates.upstreams,
        upstream_groups: &intermediates.upstream_groups,
        rules: &intermediates.rules,
        reverse_servers: &intermediates.reverse_servers,
        reverse_clients: &intermediates.reverse_clients,
        pac_enabled: intermediates.pac_enabled,
        pac_path: intermediates.pac_path,
        static_content: &intermediates.static_content,
    })?;

    Ok(TranslationOutput::new(toml_str).with_issues(output.issues))
}
