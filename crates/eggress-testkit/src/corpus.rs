//! Validation helpers for the pproxy URI corpus fixture.
//!
//! `tests/compat/fixtures/pproxy_uri_corpus.toml` is the canonical pproxy URI
//! input corpus. Each case must have all required fields so downstream tests
//! can rely on the corpus structure.
//!
//! Tier taxonomy (from `docs/PARITY_MATRIX.md`):
//! - `compatible` — eggress behavior matches pproxy for tested scenarios
//! - `supported` — eggress supports the feature, pproxy equivalence not claimed
//! - `partial` — usable subset exists but not full compatibility
//! - `intentional_non_parity` — deliberately not replicated with rationale
//! - `unsupported` — not implemented

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

/// Validation error from a corpus case check.
#[derive(Debug, thiserror::Error)]
pub enum CorpusValidationError {
    #[error("failed to read corpus file {path}: {error}")]
    FileRead { path: String, error: std::io::Error },
    #[error("failed to parse corpus TOML: {0}")]
    TomlParse(String),
    #[error("case '{case_id}' is missing required field '{field}'")]
    MissingField {
        case_id: String,
        field: &'static str,
    },
    #[error("case '{case_id}' has invalid compatibility_tier '{tier}'; valid: {valid:?}")]
    InvalidTier {
        case_id: String,
        tier: String,
        valid: Vec<&'static str>,
    },
    #[error("case '{case_id}' has non-string id")]
    NonStringId { case_id: String },
    #[error("corpus has duplicate case id '{0}'")]
    DuplicateId(String),
    #[error("corpus has zero cases")]
    Empty,
}

/// Validate the corpus file at the given path.
///
/// Returns the number of cases validated on success.
pub fn validate_uri_corpus(path: &Path) -> Result<usize, CorpusValidationError> {
    let path_str = path.display().to_string();
    let content = std::fs::read_to_string(path).map_err(|e| CorpusValidationError::FileRead {
        path: path_str,
        error: e,
    })?;
    let value: toml::Value =
        toml::from_str(&content).map_err(|e| CorpusValidationError::TomlParse(e.to_string()))?;

    let cases = value
        .get("cases")
        .and_then(|v| v.as_array())
        .ok_or(CorpusValidationError::Empty)?;

    if cases.is_empty() {
        return Err(CorpusValidationError::Empty);
    }

    let valid_tiers: Vec<&'static str> = VALID_TIERS.to_vec();
    let mut seen_ids = std::collections::HashSet::new();

    for case in cases {
        let raw_id = case
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("<missing>");
        let case_id = raw_id.to_string();

        for &field in REQUIRED_FIELDS {
            if case.get(field).is_none() {
                return Err(CorpusValidationError::MissingField { case_id, field });
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
                tier: tier.to_string(),
                valid: valid_tiers.clone(),
            });
        }

        // Verify id is unique
        if !seen_ids.insert(case_id.clone()) {
            return Err(CorpusValidationError::DuplicateId(case_id));
        }

        // Verify expected_warnings is an array
        if case.get("expected_warnings").and_then(|v| v.as_array()).is_none() {
            return Err(CorpusValidationError::MissingField {
                case_id,
                field: "expected_warnings (must be array)",
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_uri_corpus_is_valid() {
        let n = validate_workspace_uri_corpus().expect("pproxy_uri_corpus.toml should validate");
        assert!(n > 0, "corpus must have at least one case");
        // Sanity floor: the corpus has been growing case by case; verify
        // we still have a meaningful number of cases.
        assert!(n >= 50, "corpus should have at least 50 cases, got {n}");
    }
}
