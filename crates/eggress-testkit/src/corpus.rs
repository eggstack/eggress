//! Validation helpers for the pproxy URI corpus and CLI fixture files.
//!
//! `tests/compat/fixtures/pproxy_uri_corpus.toml` is the canonical pproxy URI
//! input corpus. Each case must have all required fields so downstream tests
//! can rely on the corpus structure.
//!
//! `tests/compat/fixtures/pproxy_cli_cases/*.toml` are CLI translation
//! fixtures validated by [`validate_cli_cases`].
//!
//! Tier taxonomy (from `docs/PARITY_MATRIX.md`):
//! - `compatible` — eggress behavior matches pproxy for tested scenarios
//! - `supported` — eggress supports the feature, pproxy equivalence not claimed
//! - `partial` — usable subset exists but not full compatibility
//! - `intentional_non_parity` — deliberately not replicated with rationale
//! - `unsupported` — not implemented

use std::collections::HashSet;
use std::path::Path;

const REQUIRED_FIELDS: &[&str] = &[
    "id",
    "raw_uri",
    "pproxy_interpretation",
    "expected_interpretation",
    "compatibility_tier",
    "has_credentials",
    "expected_redacted_display",
    "expected_warnings",
];

const VALID_TIERS: &[&str] = &[
    "compatible",
    "supported",
    "partial",
    "intentional_non_parity",
    "unsupported",
];

const CLI_REQUIRED_FIELDS: &[&str] = &[
    "id",
    "args",
    "expected_exit_code",
    "expected_warnings",
    "toml_content_must_contain",
];

/// Validation error from a corpus case check.
#[derive(Debug, thiserror::Error)]
pub enum CorpusValidationError {
    #[error("failed to read file {path}: {error}")]
    FileRead { path: String, error: std::io::Error },
    #[error("failed to parse TOML in {path}: {error}")]
    TomlParse { path: String, error: String },
    #[error("case '{case_id}' in {path} is missing required field '{field}'")]
    MissingField {
        case_id: String,
        path: String,
        field: &'static str,
    },
    #[error(
        "case '{case_id}' in {path} has invalid compatibility_tier '{tier}'; valid: {valid:?}"
    )]
    InvalidTier {
        case_id: String,
        path: String,
        tier: String,
        valid: Vec<&'static str>,
    },
    #[error("case '{case_id}' in {path} has non-string id")]
    NonStringId { case_id: String, path: String },
    #[error("corpus has duplicate case id '{id}' in {path}")]
    DuplicateId { id: String, path: String },
    #[error("{path} has zero cases")]
    Empty { path: String },
    #[error("case '{case_id}' in {path} has_credentials=true but expected_redacted_display does not contain '****'")]
    MissingRedaction { case_id: String, path: String },
    #[error("case '{case_id}' in {path} has expected_toml but it is empty")]
    EmptyToml { case_id: String, path: String },
    #[error("case '{case_id}' in {path} has unsupported/intentional_non_parity tier but no manifest feature maps to it")]
    UnmappedFeature { case_id: String, path: String },
}

/// Load a TOML file and parse it as a `toml::Value`.
fn load_toml(path: &Path) -> Result<toml::Value, CorpusValidationError> {
    let path_str = path.display().to_string();
    let content = std::fs::read_to_string(path).map_err(|e| CorpusValidationError::FileRead {
        path: path_str.clone(),
        error: e,
    })?;
    toml::from_str(&content).map_err(|e| CorpusValidationError::TomlParse {
        path: path_str,
        error: e.to_string(),
    })
}

/// Load all manifest feature IDs from the canonical pproxy manifest.
fn load_manifest_feature_ids(workspace_root: &Path) -> HashSet<String> {
    let manifest_path = workspace_root.join("tests/compat/pproxy_manifest.toml");
    let Ok(value) = load_toml(&manifest_path) else {
        return HashSet::new();
    };
    let mut ids = HashSet::new();
    if let Some(features) = value.get("features").and_then(|v| v.as_array()) {
        for f in features {
            if let Some(id) = f.get("id").and_then(|v| v.as_str()) {
                ids.insert(id.to_string());
            }
        }
    }
    ids
}

/// Extract the raw_uri scheme (portion before `://`).
fn uri_scheme(raw_uri: &str) -> Option<&str> {
    raw_uri.find("://").map(|i| &raw_uri[..i])
}

/// Validate the corpus file at the given path.
///
/// Returns the number of cases validated on success.
pub fn validate_uri_corpus(path: &Path) -> Result<usize, CorpusValidationError> {
    let path_str = path.display().to_string();
    let value = load_toml(path)?;

    let cases =
        value
            .get("cases")
            .and_then(|v| v.as_array())
            .ok_or(CorpusValidationError::Empty {
                path: path_str.clone(),
            })?;

    if cases.is_empty() {
        return Err(CorpusValidationError::Empty {
            path: path_str.clone(),
        });
    }

    let valid_tiers: Vec<&'static str> = VALID_TIERS.to_vec();
    let mut seen_ids = HashSet::new();

    for case in cases {
        let raw_id = case
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("<missing>");
        let case_id = raw_id.to_string();

        for &field in REQUIRED_FIELDS {
            if case.get(field).is_none() {
                return Err(CorpusValidationError::MissingField {
                    case_id,
                    path: path_str.clone(),
                    field,
                });
            }
        }

        // Verify compatibility_tier value
        let tier = case
            .get("compatibility_tier")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !valid_tiers.contains(&tier) {
            return Err(CorpusValidationError::InvalidTier {
                case_id,
                path: path_str.clone(),
                tier: tier.to_string(),
                valid: valid_tiers.clone(),
            });
        }

        // Verify id is unique
        if !seen_ids.insert(case_id.clone()) {
            return Err(CorpusValidationError::DuplicateId {
                id: case_id,
                path: path_str.clone(),
            });
        }

        // Verify expected_warnings is an array
        if case
            .get("expected_warnings")
            .and_then(|v| v.as_array())
            .is_none()
        {
            return Err(CorpusValidationError::MissingField {
                case_id,
                path: path_str.clone(),
                field: "expected_warnings (must be array)",
            });
        }

        // When has_credentials is true, expected_redacted_display must contain ****
        let has_creds = case
            .get("has_credentials")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if has_creds {
            let display = case
                .get("expected_redacted_display")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !display.contains("****") {
                return Err(CorpusValidationError::MissingRedaction {
                    case_id,
                    path: path_str.clone(),
                });
            }
        }

        // When expected_toml is present, it must not be empty
        if let Some(toml_val) = case.get("expected_toml") {
            if toml_val
                .as_str()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(CorpusValidationError::EmptyToml {
                    case_id,
                    path: path_str.clone(),
                });
            }
        }
    }

    Ok(cases.len())
}

/// Validate the corpus file at the canonical location relative to the
/// workspace root.
pub fn validate_workspace_uri_corpus() -> Result<usize, CorpusValidationError> {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let path = workspace_root.join("tests/compat/fixtures/pproxy_uri_corpus.toml");
    validate_uri_corpus(&path)
}

/// Validate every `pproxy_cli_cases/*.toml` fixture has the required schema.
///
/// Returns the number of fixtures validated on success.
pub fn validate_cli_cases(workspace_root: &Path) -> Result<usize, CorpusValidationError> {
    let cli_dir = workspace_root.join("tests/compat/fixtures/pproxy_cli_cases");
    let path_str = cli_dir.display().to_string();

    let mut entries: Vec<_> = std::fs::read_dir(&cli_dir)
        .map_err(|e| CorpusValidationError::FileRead {
            path: path_str.clone(),
            error: e,
        })?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
        .collect();
    entries.sort_by_key(|e| e.path());

    if entries.is_empty() {
        return Err(CorpusValidationError::Empty { path: path_str });
    }

    let mut seen_ids = HashSet::new();

    for entry in &entries {
        let path = entry.path();
        let path_str = path.display().to_string();
        let value = load_toml(&path)?;

        let raw_id = value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("<missing>");
        let case_id = raw_id.to_string();

        for &field in CLI_REQUIRED_FIELDS {
            if value.get(field).is_none() {
                return Err(CorpusValidationError::MissingField {
                    case_id,
                    path: path_str,
                    field,
                });
            }
        }

        // Verify args is an array
        if value.get("args").and_then(|v| v.as_array()).is_none() {
            return Err(CorpusValidationError::MissingField {
                case_id,
                path: path_str,
                field: "args (must be array)",
            });
        }

        // Verify expected_exit_code is an integer
        if value
            .get("expected_exit_code")
            .and_then(|v| v.as_integer())
            .is_none()
        {
            return Err(CorpusValidationError::MissingField {
                case_id,
                path: path_str,
                field: "expected_exit_code (must be integer)",
            });
        }

        // Verify expected_warnings is an array
        if value
            .get("expected_warnings")
            .and_then(|v| v.as_array())
            .is_none()
        {
            return Err(CorpusValidationError::MissingField {
                case_id,
                path: path_str,
                field: "expected_warnings (must be array)",
            });
        }

        // Verify toml_content_must_contain is an array
        if value
            .get("toml_content_must_contain")
            .and_then(|v| v.as_array())
            .is_none()
        {
            return Err(CorpusValidationError::MissingField {
                case_id,
                path: path_str,
                field: "toml_content_must_contain (must be array)",
            });
        }

        // Verify id is unique
        if !seen_ids.insert(case_id.clone()) {
            return Err(CorpusValidationError::DuplicateId {
                id: case_id,
                path: path_str,
            });
        }
    }

    Ok(entries.len())
}

/// Validate the CLI cases at the canonical workspace location.
pub fn validate_workspace_cli_cases() -> Result<usize, CorpusValidationError> {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    validate_cli_cases(&workspace_root)
}

/// Validate corpus feature-to-manifest mapping.
///
/// For every corpus case with `unsupported` or `intentional_non_parity` tier,
/// verify that the manifest has a corresponding feature ID based on the URI
/// scheme.
pub fn validate_corpus_manifest_mapping(path: &Path) -> Result<usize, CorpusValidationError> {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let manifest_ids = load_manifest_feature_ids(&workspace_root);
    let path_str = path.display().to_string();
    let value = load_toml(path)?;

    let cases =
        value
            .get("cases")
            .and_then(|v| v.as_array())
            .ok_or(CorpusValidationError::Empty {
                path: path_str.clone(),
            })?;

    let mut checked = 0;
    for case in cases {
        let raw_id = case
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("<missing>");
        let case_id = raw_id.to_string();
        let tier = case
            .get("compatibility_tier")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Only check unsupported/intentional_non_parity cases for manifest mapping
        if tier != "unsupported" && tier != "intentional_non_parity" {
            continue;
        }

        let raw_uri = case.get("raw_uri").and_then(|v| v.as_str()).unwrap_or("");
        let scheme = uri_scheme(raw_uri).unwrap_or("");

        // Map well-known schemes to expected manifest feature IDs.
        // Only check schemes that clearly correspond to manifest features.
        // Edge-case URIs (invalid syntax, scheme+transport combos like socks5+in+ssl)
        // are intentionally excluded since they test parser rejection, not feature parity.
        let expected_features: Vec<&str> = match scheme {
            "h2" => vec!["h2_connect_server", "h2_connect_upstream", "h2_uri_scheme"],
            "ws" | "wss" => vec![
                "websocket_tunnel_server",
                "websocket_tunnel_upstream",
                "websocket_uri_scheme",
            ],
            "raw" => vec!["raw_tunnel", "raw_uri_scheme"],
            "quic" | "h3" => vec!["quic_h3_transport"],
            "ssr" => vec!["shadowsocks_r", "cli_translate_ssr_rejection"],
            "ssh" => vec!["cli_translate_ssh_rejection"],
            "ftp" => vec!["unsupported_protocol_diagnostics"],
            _ => vec![],
        };

        if !expected_features.is_empty() {
            let has_mapping = expected_features.iter().any(|f| manifest_ids.contains(*f));
            if !has_mapping {
                return Err(CorpusValidationError::UnmappedFeature {
                    case_id,
                    path: path_str.clone(),
                });
            }
        }

        checked += 1;
    }

    Ok(checked)
}

/// Run the full workspace corpus validation suite:
/// 1. URI corpus schema validation
/// 2. CLI cases schema validation
/// 3. Corpus-to-manifest feature mapping
///
/// Returns `(corpus_cases, cli_cases, manifest_mapped)` on success.
pub fn validate_workspace_corpus_full() -> Result<(usize, usize, usize), CorpusValidationError> {
    let corpus = validate_workspace_uri_corpus()?;
    let cli = validate_workspace_cli_cases()?;
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let corpus_path = workspace_root.join("tests/compat/fixtures/pproxy_uri_corpus.toml");
    let mapped = validate_corpus_manifest_mapping(&corpus_path)?;
    Ok((corpus, cli, mapped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_uri_corpus_is_valid() {
        let n = validate_workspace_uri_corpus().expect("pproxy_uri_corpus.toml should validate");
        assert!(n > 0, "corpus must have at least one case");
        assert!(n >= 50, "corpus should have at least 50 cases, got {n}");
    }

    #[test]
    fn workspace_cli_cases_are_valid() {
        let n = validate_workspace_cli_cases().expect("cli_cases should validate");
        assert!(n > 0, "cli_cases must have at least one fixture");
    }

    #[test]
    fn corpus_manifest_mapping_is_valid() {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let corpus_path = workspace_root.join("tests/compat/fixtures/pproxy_uri_corpus.toml");
        let n = validate_corpus_manifest_mapping(&corpus_path)
            .expect("corpus-to-manifest mapping should validate");
        assert!(
            n > 0,
            "should have at least one unsupported/intentional_non_parity case"
        );
    }

    #[test]
    fn full_corpus_validation() {
        let (corpus, cli, mapped) =
            validate_workspace_corpus_full().expect("full corpus validation should pass");
        assert!(corpus >= 50);
        assert!(cli >= 1);
        assert!(mapped >= 1);
    }
}
