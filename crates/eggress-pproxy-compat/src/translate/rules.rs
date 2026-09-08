//! Rule translation: pproxy patterns/block rules become structured matchers.
//!
//! Used by the intermediates builder; TOML and native renderers consume the
//! resulting [`MatchToml`]/[`RuleToml`] model structs unchanged.

use super::model::MatchToml;
use crate::error::CompatError;
use crate::warnings::TranslationOutput;

#[derive(Debug, Clone)]
pub(crate) struct TcpCompatRoute {
    pub(crate) declaration_index: usize,
    pub(crate) upstream_id: String,
    pub(crate) predicate: Option<(String, String)>,
}

pub(crate) fn inline_pproxy_pattern(pattern: &str) -> String {
    let pattern = pattern
        .strip_prefix('{')
        .and_then(|p| p.strip_suffix('}'))
        .unwrap_or(pattern);
    format!("^(?:{pattern})")
}

pub(crate) fn combine_pproxy_patterns(patterns: &[String]) -> String {
    if patterns.len() == 1 {
        return inline_pproxy_pattern(&patterns[0]);
    }
    let joined = patterns
        .iter()
        .map(|pattern| format!("(?:{pattern})"))
        .collect::<Vec<_>>()
        .join("|");
    format!("^(?:{joined})$")
}

pub(crate) fn load_pproxy_rule_file(
    path: &str,
    output: &mut TranslationOutput,
) -> Result<Vec<String>, CompatError> {
    let rule_file =
        crate::regex_compat::PproxyRuleFile::load(std::path::Path::new(path)).map_err(|error| {
            CompatError::ConfigValidation {
                message: format!("failed to load pproxy rule file '{}': {}", path, error),
            }
        })?;
    for diagnostic in &rule_file.diagnostics {
        match diagnostic.severity {
            crate::regex_compat::RuleSeverity::Error => {
                return Err(CompatError::ConfigValidation {
                    message: format!(
                        "pproxy rule file '{}' is invalid: {}",
                        path, diagnostic.message
                    ),
                });
            }
            crate::regex_compat::RuleSeverity::Warning => {
                *output = output
                    .clone()
                    .with_warning("rulefile-partial", diagnostic.message.clone());
            }
            crate::regex_compat::RuleSeverity::Info => {
                *output = output
                    .clone()
                    .with_warning("rulefile-fancy-regex", diagnostic.message.clone());
            }
        }
    }
    if rule_file.entries.iter().any(|entry| entry.uses_fancy) {
        return Err(CompatError::ConfigValidation {
            message: format!(
                "pproxy rule file '{}' uses regex features unavailable in native routing",
                path
            ),
        });
    }
    Ok(rule_file
        .entries
        .iter()
        .map(|entry| entry.raw.clone())
        .collect())
}

pub(crate) fn pproxy_rule_match(pattern: &str, transport: &str) -> MatchToml {
    MatchToml {
        transport: None,
        host_regex: None,
        destination_port_regex: None,
        any_of: vec![
            MatchToml {
                transport: Some(transport.to_string()),
                host_regex: Some(pattern.to_string()),
                destination_port_regex: None,
                any_of: Vec::new(),
            },
            MatchToml {
                transport: Some(transport.to_string()),
                host_regex: None,
                destination_port_regex: Some(pattern.to_string()),
                any_of: Vec::new(),
            },
        ],
    }
}
