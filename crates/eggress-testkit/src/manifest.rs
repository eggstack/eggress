//! Manifest validation for pproxy feature parity tracking.
//!
//! Parses `tests/compat/pproxy_manifest.toml` and validates structural
//! invariants that prevent regressions in the evidence index.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

/// Pinned pproxy version that manifest metadata must reference.
pub const PINNED_PPROXY_VERSION: &str = "2.7.9";

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Top-level metadata section of the manifest.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ManifestMeta {
    pub pproxy_version: String,
    pub manifest_version: String,
    #[serde(default)]
    pub last_updated: Option<String>,
}

/// A single feature entry in the manifest.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FullManifestEntry {
    pub id: String,
    pub category: String,
    pub pproxy_version: String,
    pub egress_status: String,
    pub evidence_level: String,
    #[serde(default)]
    pub tests: Vec<String>,
    #[serde(default)]
    pub divergence: String,
    #[serde(default)]
    pub external_dependency: Option<String>,
}

/// The complete manifest structure.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FullManifest {
    pub meta: ManifestMeta,
    pub features: Vec<FullManifestEntry>,
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Represents the egress implementation status for a feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EgressStatus {
    Compatible,
    Supported,
    Partial,
    IntentionalNonParity,
    Experimental,
    Unsupported,
}

impl FromStr for EgressStatus {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "compatible" => Ok(Self::Compatible),
            "supported" => Ok(Self::Supported),
            "partial" => Ok(Self::Partial),
            "intentional_non_parity" => Ok(Self::IntentionalNonParity),
            "experimental" => Ok(Self::Experimental),
            "unsupported" => Ok(Self::Unsupported),
            other => Err(ValidationError::InvalidEgressStatus {
                value: other.to_string(),
            }),
        }
    }
}

impl fmt::Display for EgressStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compatible => write!(f, "compatible"),
            Self::Supported => write!(f, "supported"),
            Self::Partial => write!(f, "partial"),
            Self::IntentionalNonParity => write!(f, "intentional_non_parity"),
            Self::Experimental => write!(f, "experimental"),
            Self::Unsupported => write!(f, "unsupported"),
        }
    }
}

/// Represents the evidence level for a feature claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvidenceLevel {
    Unimplemented,
    ImplementedSynthetic,
    ImplementedDifferential,
    ImplementedInterop,
    Compatible,
    IntentionalNonParity,
}

impl FromStr for EvidenceLevel {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unimplemented" => Ok(Self::Unimplemented),
            "implemented_synthetic" => Ok(Self::ImplementedSynthetic),
            "implemented_differential" => Ok(Self::ImplementedDifferential),
            "implemented_interop" => Ok(Self::ImplementedInterop),
            "compatible" => Ok(Self::Compatible),
            "intentional_non_parity" => Ok(Self::IntentionalNonParity),
            other => Err(ValidationError::InvalidEvidenceLevel {
                value: other.to_string(),
            }),
        }
    }
}

impl fmt::Display for EvidenceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unimplemented => write!(f, "unimplemented"),
            Self::ImplementedSynthetic => write!(f, "implemented_synthetic"),
            Self::ImplementedDifferential => write!(f, "implemented_differential"),
            Self::ImplementedInterop => write!(f, "implemented_interop"),
            Self::Compatible => write!(f, "compatible"),
            Self::IntentionalNonParity => write!(f, "intentional_non_parity"),
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A single validation error with context.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("TOML parse error: {message}")]
    TomlParse { message: String },

    #[error("file I/O error: {message}")]
    Io { message: String },

    #[error("invalid egress_status value: \"{value}\"")]
    InvalidEgressStatus { value: String },

    #[error("invalid evidence_level value: \"{value}\"")]
    InvalidEvidenceLevel { value: String },

    #[error(
        "feature \"{id}\" has egress_status=\"compatible\" but evidence_level=\"{evidence}\" (must be \"compatible\")"
    )]
    CompatibleStatusRequiresCompatibleEvidence { id: String, evidence: String },

    #[error(
        "feature \"{id}\" has evidence_level=\"compatible\" but no test names (at least one required)"
    )]
    CompatibleEvidenceRequiresTests { id: String },

    #[error(
        "feature \"{id}\" has evidence_level=\"implemented_synthetic\" with egress_status=\"compatible\" (not allowed)"
    )]
    SyntheticCannotPairWithCompatible { id: String },

    #[error("feature \"{id}\" has egress_status=\"intentional_non_parity\" but empty divergence")]
    IntentionalNonParityRequiresDivergence { id: String },

    #[error("duplicate feature id: \"{id}\"")]
    DuplicateFeatureId { id: String },

    #[error(
        "meta.pproxy_version=\"{actual}\" does not match expected pinned version \"{expected}\""
    )]
    PproxyVersionMismatch { actual: String, expected: String },

    #[error("meta.last_updated appears to be a current/recent date: \"{date}\" (non-fatal, warning only)")]
    StaleLastUpdated { date: String },
}

/// A collection of validation errors and warnings.
///
/// Only errors (in `errors`) cause `validate_manifest` to return `Err`.
/// Warnings (in `warnings`) are informational and never cause failure.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{errors:#?}")]
pub struct ValidationErrors {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
}

impl ValidationErrors {
    /// Create an empty collection.
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Add a hard error to the collection.
    pub fn push(&mut self, err: ValidationError) {
        self.errors.push(err);
    }

    /// Add a non-fatal warning.
    pub fn warn(&mut self, warning: ValidationError) {
        self.warnings.push(warning);
    }

    /// Returns `true` if no hard errors were recorded (warnings are ignored).
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Number of hard errors.
    pub fn len(&self) -> usize {
        self.errors.len()
    }
}

impl Default for ValidationErrors {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a manifest parsed from TOML.
///
/// Returns `Ok(())` when all invariants hold, or `Err(ValidationErrors)`
/// listing every violation found. Warnings (e.g. stale `last_updated`) are
/// recorded but do **not** cause a failure.
pub fn validate_manifest(manifest: &FullManifest) -> Result<(), ValidationErrors> {
    let mut errs = ValidationErrors::new();

    // 1. meta.pproxy_version must match pinned version
    if manifest.meta.pproxy_version != PINNED_PPROXY_VERSION {
        errs.push(ValidationError::PproxyVersionMismatch {
            actual: manifest.meta.pproxy_version.clone(),
            expected: PINNED_PPROXY_VERSION.to_string(),
        });
    }

    // 2. last_updated freshness warning (non-fatal)
    if let Some(ref date) = manifest.meta.last_updated {
        if is_recent_date(date) {
            errs.warn(ValidationError::StaleLastUpdated { date: date.clone() });
        }
    }

    // 3. Collect IDs and check for duplicates
    let mut seen_ids = HashSet::new();
    for feature in &manifest.features {
        if !seen_ids.insert(feature.id.clone()) {
            errs.push(ValidationError::DuplicateFeatureId {
                id: feature.id.clone(),
            });
        }
    }

    // 4. Per-feature validations
    for feature in &manifest.features {
        // Parse enums (validates allowed values)
        let status = EgressStatus::from_str(&feature.egress_status);
        let evidence = EvidenceLevel::from_str(&feature.evidence_level);

        if let Err(ref e) = status {
            errs.push(e.clone());
        }
        if let Err(ref e) = evidence {
            errs.push(e.clone());
        }

        // Remaining cross-field checks require valid enum values
        let status = match status {
            Ok(s) => s,
            Err(_) => continue,
        };
        let evidence = match evidence {
            Ok(e) => e,
            Err(_) => continue,
        };

        // compatible status → evidence must also be compatible
        if status == EgressStatus::Compatible && evidence != EvidenceLevel::Compatible {
            errs.push(
                ValidationError::CompatibleStatusRequiresCompatibleEvidence {
                    id: feature.id.clone(),
                    evidence: feature.evidence_level.clone(),
                },
            );
        }

        // compatible evidence → at least one non-empty test name required
        if evidence == EvidenceLevel::Compatible
            && feature.tests.iter().all(|t| t.trim().is_empty())
        {
            errs.push(ValidationError::CompatibleEvidenceRequiresTests {
                id: feature.id.clone(),
            });
        }

        // implemented_synthetic cannot pair with compatible status
        if evidence == EvidenceLevel::ImplementedSynthetic && status == EgressStatus::Compatible {
            errs.push(ValidationError::SyntheticCannotPairWithCompatible {
                id: feature.id.clone(),
            });
        }

        // intentional_non_parity requires non-empty divergence
        if status == EgressStatus::IntentionalNonParity && feature.divergence.trim().is_empty() {
            errs.push(ValidationError::IntentionalNonParityRequiresDivergence {
                id: feature.id.clone(),
            });
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Parse and validate a manifest from a filesystem path.
pub fn validate_manifest_file(path: &Path) -> Result<FullManifest, ValidationErrors> {
    let content = fs::read_to_string(path).map_err(|e| {
        let mut errs = ValidationErrors::new();
        errs.push(ValidationError::Io {
            message: format!("failed to read {}: {}", path.display(), e),
        });
        errs
    })?;

    let manifest: FullManifest = toml::from_str(&content).map_err(|e| {
        let mut errs = ValidationErrors::new();
        errs.push(ValidationError::TomlParse {
            message: e.to_string(),
        });
        errs
    })?;

    validate_manifest(&manifest)?;
    Ok(manifest)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Locate the pproxy manifest file relative to the workspace root.
///
/// Searches upward from `start` looking for `tests/compat/pproxy_manifest.toml`,
/// then falls back to `CARGO_MANIFEST_DIR`-relative paths.
pub fn find_manifest_path() -> Option<PathBuf> {
    // Try CARGO_MANIFEST_DIR → ../../tests/compat/pproxy_manifest.toml
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let candidate =
            PathBuf::from(&manifest_dir).join("../../tests/compat/pproxy_manifest.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // Try walking up from current directory
    let cwd = std::env::current_dir().ok()?;
    let mut dir = cwd.as_path();
    loop {
        let candidate = dir.join("tests/compat/pproxy_manifest.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

/// Check if a date string looks like it refers to the current or very recent
/// calendar year. This is heuristic — it produces warnings, never hard failures.
fn is_recent_date(date_str: &str) -> bool {
    // Simple heuristic: if the year is 2025 or later, flag it.
    // The manifest should be updated periodically but "last_updated" is
    // informational. We only warn on obviously current dates.
    //
    // Parse the first 4-digit year-like token from the string.
    let year_str: String = date_str.chars().take(4).collect();
    if let Ok(year) = year_str.parse::<u32>() {
        year >= 2025
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest(meta: ManifestMeta, features: Vec<FullManifestEntry>) -> FullManifest {
        FullManifest { meta, features }
    }

    fn default_meta() -> ManifestMeta {
        ManifestMeta {
            pproxy_version: PINNED_PPROXY_VERSION.to_string(),
            manifest_version: "1".to_string(),
            last_updated: Some("2025-01-01".to_string()),
        }
    }

    fn compatible_feature(id: &str) -> FullManifestEntry {
        FullManifestEntry {
            id: id.to_string(),
            category: "protocol".to_string(),
            pproxy_version: PINNED_PPROXY_VERSION.to_string(),
            egress_status: "compatible".to_string(),
            evidence_level: "compatible".to_string(),
            tests: vec!["test_a".to_string()],
            divergence: "some divergence".to_string(),
            external_dependency: None,
        }
    }

    #[test]
    fn valid_manifest_passes() {
        let manifest = make_manifest(
            default_meta(),
            vec![
                compatible_feature("feat_a"),
                FullManifestEntry {
                    id: "feat_b".to_string(),
                    category: "udp".to_string(),
                    pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                    egress_status: "supported".to_string(),
                    evidence_level: "implemented_synthetic".to_string(),
                    tests: vec!["unit_tests".to_string()],
                    divergence: "different entry points".to_string(),
                    external_dependency: None,
                },
            ],
        );
        // Note: last_updated "2025-01-01" triggers a warning (non-fatal)
        let result = validate_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn compatible_status_with_synthetic_evidence_fails() {
        let manifest = make_manifest(
            default_meta(),
            vec![FullManifestEntry {
                id: "bad_feat".to_string(),
                category: "protocol".to_string(),
                pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                egress_status: "compatible".to_string(),
                evidence_level: "implemented_synthetic".to_string(),
                tests: vec!["some_test".to_string()],
                divergence: "n/a".to_string(),
                external_dependency: None,
            }],
        );
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, ValidationError::CompatibleStatusRequiresCompatibleEvidence { id, .. } if id == "bad_feat")),
            "expected CompatibleStatusRequiresCompatibleEvidence"
        );
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, ValidationError::SyntheticCannotPairWithCompatible { id, .. } if id == "bad_feat")),
            "expected SyntheticCannotPairWithCompatible"
        );
    }

    #[test]
    fn duplicate_ids_fail() {
        let manifest = make_manifest(
            default_meta(),
            vec![compatible_feature("dup"), compatible_feature("dup")],
        );
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(
                |e| matches!(e, ValidationError::DuplicateFeatureId { id, .. } if id == "dup")
            ),
            "expected DuplicateFeatureId"
        );
    }

    #[test]
    fn compatible_evidence_with_empty_tests_fails() {
        let manifest = make_manifest(
            default_meta(),
            vec![FullManifestEntry {
                id: "no_tests".to_string(),
                category: "protocol".to_string(),
                pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                egress_status: "compatible".to_string(),
                evidence_level: "compatible".to_string(),
                tests: vec![],
                divergence: "n/a".to_string(),
                external_dependency: None,
            }],
        );
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, ValidationError::CompatibleEvidenceRequiresTests { id, .. } if id == "no_tests")),
            "expected CompatibleEvidenceRequiresTests"
        );
    }

    #[test]
    fn compatible_evidence_with_whitespace_only_tests_fails() {
        let manifest = make_manifest(
            default_meta(),
            vec![FullManifestEntry {
                id: "blank_tests".to_string(),
                category: "protocol".to_string(),
                pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                egress_status: "compatible".to_string(),
                evidence_level: "compatible".to_string(),
                tests: vec!["   ".to_string(), "".to_string()],
                divergence: "n/a".to_string(),
                external_dependency: None,
            }],
        );
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, ValidationError::CompatibleEvidenceRequiresTests { id, .. } if id == "blank_tests")),
            "expected CompatibleEvidenceRequiresTests for whitespace-only tests"
        );
    }

    #[test]
    fn intentional_non_parity_without_divergence_fails() {
        let manifest = make_manifest(
            default_meta(),
            vec![FullManifestEntry {
                id: "no_div".to_string(),
                category: "cli".to_string(),
                pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                egress_status: "intentional_non_parity".to_string(),
                evidence_level: "intentional_non_parity".to_string(),
                tests: vec![],
                divergence: String::new(),
                external_dependency: None,
            }],
        );
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, ValidationError::IntentionalNonParityRequiresDivergence { id, .. } if id == "no_div")),
            "expected IntentionalNonParityRequiresDivergence"
        );
    }

    #[test]
    fn intentional_non_parity_with_whitespace_divergence_fails() {
        let manifest = make_manifest(
            default_meta(),
            vec![FullManifestEntry {
                id: "ws_div".to_string(),
                category: "cli".to_string(),
                pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                egress_status: "intentional_non_parity".to_string(),
                evidence_level: "intentional_non_parity".to_string(),
                tests: vec![],
                divergence: "   ".to_string(),
                external_dependency: None,
            }],
        );
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, ValidationError::IntentionalNonParityRequiresDivergence { id, .. } if id == "ws_div")),
            "expected IntentionalNonParityRequiresDivergence for whitespace divergence"
        );
    }

    #[test]
    fn invalid_egress_status_fails() {
        let manifest = make_manifest(
            default_meta(),
            vec![FullManifestEntry {
                id: "bad_status".to_string(),
                category: "protocol".to_string(),
                pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                egress_status: "bogus".to_string(),
                evidence_level: "compatible".to_string(),
                tests: vec!["test".to_string()],
                divergence: "n/a".to_string(),
                external_dependency: None,
            }],
        );
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, ValidationError::InvalidEgressStatus { value, .. } if value == "bogus")),
            "expected InvalidEgressStatus"
        );
    }

    #[test]
    fn invalid_evidence_level_fails() {
        let manifest = make_manifest(
            default_meta(),
            vec![FullManifestEntry {
                id: "bad_evidence".to_string(),
                category: "protocol".to_string(),
                pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                egress_status: "supported".to_string(),
                evidence_level: "not_real".to_string(),
                tests: vec!["test".to_string()],
                divergence: "n/a".to_string(),
                external_dependency: None,
            }],
        );
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, ValidationError::InvalidEvidenceLevel { value, .. } if value == "not_real")),
            "expected InvalidEvidenceLevel"
        );
    }

    #[test]
    fn pproxy_version_mismatch_fails() {
        let mut meta = default_meta();
        meta.pproxy_version = "1.2.3".to_string();
        let manifest = make_manifest(meta, vec![compatible_feature("f")]);
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, ValidationError::PproxyVersionMismatch { .. })),
            "expected PproxyVersionMismatch"
        );
    }

    #[test]
    fn compatible_evidence_without_tests_all_variants_fail() {
        // Tests with only empty/whitespace entries
        let manifest = make_manifest(
            default_meta(),
            vec![FullManifestEntry {
                id: "mixed_blanks".to_string(),
                category: "protocol".to_string(),
                pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                egress_status: "compatible".to_string(),
                evidence_level: "compatible".to_string(),
                tests: vec!["".to_string(), "  ".to_string(), "\t".to_string()],
                divergence: "n/a".to_string(),
                external_dependency: None,
            }],
        );
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(!errs.is_empty());
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::CompatibleEvidenceRequiresTests { .. })));
    }

    #[test]
    fn multiple_errors_collected() {
        let manifest = make_manifest(
            ManifestMeta {
                pproxy_version: "0.0.1".to_string(),
                manifest_version: "1".to_string(),
                last_updated: None,
            },
            vec![
                FullManifestEntry {
                    id: "dup".to_string(),
                    category: "protocol".to_string(),
                    pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                    egress_status: "bogus".to_string(),
                    evidence_level: "also_bogus".to_string(),
                    tests: vec![],
                    divergence: String::new(),
                    external_dependency: None,
                },
                FullManifestEntry {
                    id: "dup".to_string(),
                    category: "protocol".to_string(),
                    pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                    egress_status: "intentional_non_parity".to_string(),
                    evidence_level: "intentional_non_parity".to_string(),
                    tests: vec![],
                    divergence: String::new(),
                    external_dependency: None,
                },
            ],
        );
        let errs = validate_manifest(&manifest).unwrap_err();
        assert!(
            errs.len() >= 4,
            "expected at least 4 errors, got {}",
            errs.len()
        );
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::PproxyVersionMismatch { .. })));
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidEgressStatus { .. })));
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidEvidenceLevel { .. })));
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::DuplicateFeatureId { .. })));
        assert!(errs.errors.iter().any(|e| matches!(
            e,
            ValidationError::IntentionalNonParityRequiresDivergence { .. }
        )));
    }

    #[test]
    fn intentional_non_parity_with_divergence_passes() {
        let manifest = make_manifest(
            default_meta(),
            vec![FullManifestEntry {
                id: "ok_innp".to_string(),
                category: "cli".to_string(),
                pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                egress_status: "intentional_non_parity".to_string(),
                evidence_level: "intentional_non_parity".to_string(),
                tests: vec![],
                divergence: "Deliberate design choice".to_string(),
                external_dependency: None,
            }],
        );
        let result = validate_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn supported_with_synthetic_passes() {
        let manifest = make_manifest(
            default_meta(),
            vec![FullManifestEntry {
                id: "ok_sup".to_string(),
                category: "protocol".to_string(),
                pproxy_version: PINNED_PPROXY_VERSION.to_string(),
                egress_status: "supported".to_string(),
                evidence_level: "implemented_synthetic".to_string(),
                tests: vec!["unit_tests".to_string()],
                divergence: "n/a".to_string(),
                external_dependency: None,
            }],
        );
        let result = validate_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn toml_parse_error() {
        let bad_toml = "this is not [valid toml {{{{";
        let manifest: Result<FullManifest, _> = toml::from_str(bad_toml);
        assert!(manifest.is_err());
    }

    #[test]
    fn from_str_roundtrip_egress_status() {
        for variant in &[
            "compatible",
            "supported",
            "partial",
            "intentional_non_parity",
            "experimental",
            "unsupported",
        ] {
            let status = EgressStatus::from_str(variant).unwrap();
            assert_eq!(status.to_string(), *variant);
        }
    }

    #[test]
    fn from_str_roundtrip_evidence_level() {
        for variant in &[
            "unimplemented",
            "implemented_synthetic",
            "implemented_differential",
            "implemented_interop",
            "compatible",
            "intentional_non_parity",
        ] {
            let level = EvidenceLevel::from_str(variant).unwrap();
            assert_eq!(level.to_string(), *variant);
        }
    }

    #[test]
    fn from_str_invalid_egress_status() {
        let result = EgressStatus::from_str("nope");
        assert!(result.is_err());
        match result.unwrap_err() {
            ValidationError::InvalidEgressStatus { value } => assert_eq!(value, "nope"),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn from_str_invalid_evidence_level() {
        let result = EvidenceLevel::from_str("nope");
        assert!(result.is_err());
        match result.unwrap_err() {
            ValidationError::InvalidEvidenceLevel { value } => assert_eq!(value, "nope"),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn validation_errors_collection() {
        let mut errs = ValidationErrors::new();
        assert!(errs.is_empty());
        assert_eq!(errs.len(), 0);

        errs.push(ValidationError::DuplicateFeatureId {
            id: "a".to_string(),
        });
        errs.push(ValidationError::DuplicateFeatureId {
            id: "b".to_string(),
        });
        assert!(!errs.is_empty());
        assert_eq!(errs.len(), 2);
    }

    #[test]
    fn validate_manifest_file_missing_path() {
        let path = Path::new("/nonexistent/path/manifest.toml");
        let result = validate_manifest_file(path);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::Io { .. })));
    }

    #[test]
    fn validate_real_manifest() {
        let path = match find_manifest_path() {
            Some(p) => p,
            None => {
                eprintln!("manifest file not found, skipping");
                return;
            }
        };
        eprintln!("Validating manifest at: {}", path.display());
        match validate_manifest_file(&path) {
            Ok(manifest) => {
                eprintln!(
                    "Manifest OK: {} features, meta.pproxy_version={}",
                    manifest.features.len(),
                    manifest.meta.pproxy_version
                );
            }
            Err(errs) => {
                eprintln!("Manifest validation FAILED with {} errors:", errs.len());
                for (i, err) in errs.errors.iter().enumerate() {
                    eprintln!("  ERROR {}: {}", i + 1, err);
                }
                for (i, warn) in errs.warnings.iter().enumerate() {
                    eprintln!("  WARNING {}: {}", i + 1, warn);
                }
                panic!(
                    "manifest validation failed with {} errors (see above)",
                    errs.len()
                );
            }
        }
    }

    #[test]
    fn manifest_test_names_exist() {
        const GROUP_ALIASES: &[&str] = &[
            "integration_tests",
            "unit_tests",
            "cli_tests",
            "scheduler_runtime_tests",
            "udp_tests",
            "udp_upstream_tests",
            "tls_tests",
            "shadowsocks_tcp_tests",
            "shadowsocks_udp_tests",
            "pproxy_compat_tests",
            "pproxy_cli_tests",
            "pproxy_redaction_tests",
            "wheel_tests",
            "reload_tests",
            "interoperability_shadowsocks_tcp",
            "implicit_in_echo_tests",
            "test_pproxy_compat.py",
            "test_pproxy_redaction.py",
            "test_pproxy_concurrency.py",
        ];

        let manifest_path = match find_manifest_path() {
            Some(p) => p,
            None => {
                eprintln!("manifest not found, skipping");
                return;
            }
        };
        let manifest = validate_manifest_file(&manifest_path).expect("manifest should be valid");

        let workspace_root = manifest_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .expect("should have workspace root");

        let mut source_files = Vec::new();
        let crates_dir = workspace_root.join("crates");
        if crates_dir.exists() {
            for entry in walk_dir_recursive(&crates_dir) {
                if entry.extension().is_some_and(|e| e == "rs" || e == "py") {
                    source_files.push(entry);
                }
            }
        }

        let mut missing = Vec::new();
        for feature in &manifest.features {
            for test_name in &feature.tests {
                if GROUP_ALIASES.contains(&test_name.as_str()) {
                    continue;
                }
                let found = source_files.iter().any(|path| {
                    std::fs::read_to_string(path)
                        .map(|content| content.contains(test_name.as_str()))
                        .unwrap_or(false)
                });
                if !found {
                    missing.push((feature.id.clone(), test_name.clone()));
                }
            }
        }

        if !missing.is_empty() {
            eprintln!("Manifest references test names not found in codebase:");
            for (feat, test) in &missing {
                eprintln!("  feature \"{}\" references \"{}\"", feat, test);
            }
            panic!(
                "{} manifest test name(s) not found in codebase",
                missing.len()
            );
        }
    }

    fn walk_dir_recursive(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut results = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    results.extend(walk_dir_recursive(&path));
                } else {
                    results.push(path);
                }
            }
        }
        results
    }
}
