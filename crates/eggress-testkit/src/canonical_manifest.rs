//! Canonical manifest validation for the pproxy capability manifest.
//!
//! Validates `docs/parity/pproxy_capability_manifest.toml` — the single
//! authoritative parity contract.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Allowed `tier` values for canonical manifest entries.
pub const ALLOWED_TIERS: &[&str] = &[
    "drop_in",
    "compatible_with_warning",
    "native_equivalent",
    "intentional_non_parity",
    "unsupported",
];

/// Allowed `layer` values (parser, translator, config, runtime, cli, python, docs).
pub const ALLOWED_LAYERS: &[&str] = &[
    "complete",
    "partial",
    "not_started",
    "not_applicable",
    "refused",
];

/// Allowed `evidence` values.
pub const ALLOWED_EVIDENCE: &[&str] = &[
    "differential",
    "integration",
    "unit",
    "synthetic",
    "docs_only",
    "none",
];

/// Allowed `category` values.
pub const ALLOWED_CATEGORIES: &[&str] = &["cli", "uri", "protocol", "routing", "python"];

/// Allowed `caveat_class` values (Rule 14).
pub const ALLOWED_CAVEAT_CLASSES: &[&str] = &[
    "protocol_crate_only",
    "missing_protocol_command",
    "missing_protocol_role",
    "missing_protocol_transport",
    "deferred_by_adr",
    "intentional_non_parity",
    "cli_process_model",
    "translator_scope_gap",
];

/// Pinned pproxy version that manifest metadata must reference.
pub const PINNED_PPROXY_VERSION: &str = "2.7.9";

/// Pinned manifest_version.
pub const PINNED_MANIFEST_VERSION: &str = "1";

/// Pinned schema name.
pub const PINNED_SCHEMA: &str = "phase_37";

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Top-level metadata section of the canonical manifest.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CanonicalManifestMeta {
    pub manifest_version: String,
    pub pproxy_version: String,
    pub schema: String,
}

/// A single capability entry in the canonical manifest.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CanonicalCapability {
    pub id: String,
    pub category: String,
    #[serde(default)]
    pub pproxy_surface: String,
    #[serde(default)]
    pub pproxy_behavior: String,
    /// Correct field name used by the manifest.
    #[serde(default)]
    pub eggress_behavior: String,
    /// Typo variant that should be flagged as a warning.
    #[serde(default)]
    pub egress_behavior: String,
    pub tier: String,
    pub parser: String,
    pub translator: String,
    pub config: String,
    pub runtime: String,
    pub cli: String,
    pub python: String,
    pub docs: String,
    pub evidence: String,
    #[serde(default)]
    pub tests: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub diagnostic: Option<String>,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub caveat_class: Option<String>,
    #[serde(default)]
    pub differential_exception: Option<bool>,
}

/// The complete canonical manifest structure.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CanonicalManifest {
    pub meta: CanonicalManifestMeta,
    pub capability: Vec<CanonicalCapability>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A single validation error or warning with rule number and context.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CanonicalValidationError {
    #[error("TOML parse error: {message}")]
    TomlParse { message: String },

    #[error("file I/O error: {message}")]
    Io { message: String },

    // Rule 1: Unknown tier/layer/evidence/category/caveat_class
    #[error("unknown tier \"{value}\" (valid: {allowed:?})")]
    UnknownTier { value: String, allowed: Vec<String> },

    #[error("unknown layer value \"{value}\" for {layer} (valid: {allowed:?})")]
    UnknownLayer {
        value: String,
        layer: String,
        allowed: Vec<String>,
    },

    #[error("unknown evidence \"{value}\" (valid: {allowed:?})")]
    UnknownEvidence { value: String, allowed: Vec<String> },

    #[error("unknown category \"{value}\" (valid: {allowed:?})")]
    UnknownCategory { value: String, allowed: Vec<String> },

    #[error("unknown caveat_class \"{value}\" (valid: {allowed:?})")]
    UnknownCaveatClass { value: String, allowed: Vec<String> },

    // Rule 2: Duplicate IDs
    #[error("duplicate capability id: \"{id}\"")]
    DuplicateId { id: String },

    // Meta validation
    #[error("meta.manifest_version=\"{actual}\" does not match expected \"{expected}\"")]
    ManifestVersionMismatch { actual: String, expected: String },

    #[error("meta.pproxy_version=\"{actual}\" does not match expected \"{expected}\"")]
    PproxyVersionMismatch { actual: String, expected: String },

    #[error("meta.schema=\"{actual}\" does not match expected \"{expected}\"")]
    SchemaMismatch { actual: String, expected: String },

    // Rule 3: Drop-in layer requirements
    #[error("drop_in requires {layer}=\"complete\", got \"{value}\"")]
    DropInLayerIncomplete {
        id: String,
        layer: String,
        value: String,
    },

    // Rule 4: Drop-in evidence weakness
    #[error(
        "drop_in with evidence \"{evidence}\" weaker than {threshold} (no differential_exception)"
    )]
    DropInEvidenceWeak {
        id: String,
        evidence: String,
        threshold: String,
    },

    // Rule 5: compatible_with_warning without diagnostic or notes
    #[error("compatible_with_warning without diagnostic code or non-empty notes")]
    CompatibleWithoutDiagnostic { id: String },

    // Rule 6: intentional_non_parity without rationale
    #[error("intentional_non_parity without rationale")]
    IntentionalNonParityWithoutRationale { id: String },

    // Rule 7: unsupported with runtime=complete
    #[error("unsupported tier but runtime=\"complete\" (contradictory)")]
    UnsupportedWithRuntime { id: String },

    // Rule 8: drop_in with runtime=refused
    #[error("drop_in with runtime=\"refused\" (contradictory)")]
    DropInWithRuntimeRefused { id: String },

    // Rule 9: protocol-crate-only drop_in contradiction
    #[error("drop_in but protocol-crate-only (config=\"{config}\", runtime=\"{runtime}\")")]
    DropInProtocolCrateOnly {
        id: String,
        config: String,
        runtime: String,
    },

    // Rule 10: CLI without tests
    #[error("CLI capability with empty tests and empty notes (warning)")]
    CliWithoutTests { id: String },

    // Rule 11: Python drop_in with no test evidence
    #[error("Python drop_in capability with evidence=\"{evidence}\" (requires integration or differential)")]
    PythonDropInNoEvidence { id: String, evidence: String },

    // Rule 13: Typo detection
    #[error("egress_behavior typo detected (should be \"eggress_behavior\")")]
    EgressBehaviorTypo { id: String },
}

/// A collection of validation errors and warnings.
///
/// Only errors (in `errors`) cause `validate_canonical_manifest` to return `Err`.
/// Warnings (in `warnings`) are informational and never cause failure.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{errors:#?}")]
pub struct CanonicalValidationErrors {
    pub errors: Vec<CanonicalValidationError>,
    pub warnings: Vec<CanonicalValidationError>,
}

impl CanonicalValidationErrors {
    /// Create an empty collection.
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Add a hard error to the collection.
    pub fn push(&mut self, err: CanonicalValidationError) {
        self.errors.push(err);
    }

    /// Add a non-fatal warning.
    pub fn warn(&mut self, warning: CanonicalValidationError) {
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

impl Default for CanonicalValidationErrors {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Rank evidence strength: lower is stronger.
fn evidence_rank(evidence: &str) -> u8 {
    match evidence {
        "differential" => 0,
        "integration" => 1,
        "unit" => 2,
        "synthetic" => 3,
        "docs_only" => 4,
        "none" => 5,
        _ => 6,
    }
}

/// Return the set of layers that must be "complete" for a drop_in claim in
/// the given category.
fn required_drop_in_layers_for_category(category: &str) -> Vec<&'static str> {
    match category {
        "python" => vec!["python", "docs"],
        "cli" => vec!["cli", "docs"],
        "routing" => vec!["parser", "translator", "config", "runtime", "docs"],
        // protocol, uri
        _ => vec!["parser", "translator", "config", "runtime", "cli", "docs"],
    }
}

/// Get a capability's layer value by name.
fn layer_value<'a>(cap: &'a CanonicalCapability, layer: &str) -> &'a str {
    match layer {
        "parser" => &cap.parser,
        "translator" => &cap.translator,
        "config" => &cap.config,
        "runtime" => &cap.runtime,
        "cli" => &cap.cli,
        "python" => &cap.python,
        "docs" => &cap.docs,
        _ => "",
    }
}

/// Locate the canonical parity manifest file relative to CARGO_MANIFEST_DIR.
///
/// Searches for `docs/parity/pproxy_capability_manifest.toml` relative to the
/// crate manifest directory, walking upward if needed.
pub fn find_canonical_manifest_path() -> Option<PathBuf> {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let candidate =
            PathBuf::from(&manifest_dir).join("../../docs/parity/pproxy_capability_manifest.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let cwd = std::env::current_dir().ok()?;
    let mut dir = cwd.as_path();
    loop {
        let candidate = dir.join("docs/parity/pproxy_capability_manifest.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a parsed canonical manifest.
///
/// Returns `Ok(())` when all invariants hold, or `Err(CanonicalValidationErrors)`
/// listing every violation found.
pub fn validate_canonical_manifest(
    manifest: &CanonicalManifest,
) -> Result<(), CanonicalValidationErrors> {
    let mut errs = CanonicalValidationErrors::new();

    // ── Meta validation ────────────────────────────────────────────────
    if manifest.meta.manifest_version != PINNED_MANIFEST_VERSION {
        errs.push(CanonicalValidationError::ManifestVersionMismatch {
            actual: manifest.meta.manifest_version.clone(),
            expected: PINNED_MANIFEST_VERSION.to_string(),
        });
    }
    if manifest.meta.pproxy_version != PINNED_PPROXY_VERSION {
        errs.push(CanonicalValidationError::PproxyVersionMismatch {
            actual: manifest.meta.pproxy_version.clone(),
            expected: PINNED_PPROXY_VERSION.to_string(),
        });
    }
    if manifest.meta.schema != PINNED_SCHEMA {
        errs.push(CanonicalValidationError::SchemaMismatch {
            actual: manifest.meta.schema.clone(),
            expected: PINNED_SCHEMA.to_string(),
        });
    }

    // ── Rule 2: Duplicate IDs ──────────────────────────────────────────
    let mut seen_ids = HashSet::new();
    for cap in &manifest.capability {
        if !seen_ids.insert(cap.id.clone()) {
            errs.push(CanonicalValidationError::DuplicateId { id: cap.id.clone() });
        }
    }

    // ── Per-capability validations ─────────────────────────────────────
    for cap in &manifest.capability {
        // Rule 1: Valid tier
        if !ALLOWED_TIERS.contains(&cap.tier.as_str()) {
            errs.push(CanonicalValidationError::UnknownTier {
                value: cap.tier.clone(),
                allowed: ALLOWED_TIERS.iter().map(|s| s.to_string()).collect(),
            });
        }

        // Rule 1: Valid layers
        for layer_name in &[
            "parser",
            "translator",
            "config",
            "runtime",
            "cli",
            "python",
            "docs",
        ] {
            let val = layer_value(cap, layer_name);
            if !val.is_empty() && !ALLOWED_LAYERS.contains(&val) {
                errs.push(CanonicalValidationError::UnknownLayer {
                    value: val.to_string(),
                    layer: layer_name.to_string(),
                    allowed: ALLOWED_LAYERS.iter().map(|s| s.to_string()).collect(),
                });
            }
        }

        // Rule 1: Valid evidence
        if !cap.evidence.is_empty() && !ALLOWED_EVIDENCE.contains(&cap.evidence.as_str()) {
            errs.push(CanonicalValidationError::UnknownEvidence {
                value: cap.evidence.clone(),
                allowed: ALLOWED_EVIDENCE.iter().map(|s| s.to_string()).collect(),
            });
        }

        // Rule 1: Valid category
        if !cap.category.is_empty() && !ALLOWED_CATEGORIES.contains(&cap.category.as_str()) {
            errs.push(CanonicalValidationError::UnknownCategory {
                value: cap.category.clone(),
                allowed: ALLOWED_CATEGORIES.iter().map(|s| s.to_string()).collect(),
            });
        }

        // Rule 1: Valid caveat_class
        if let Some(ref cc) = cap.caveat_class {
            if !cc.is_empty() && !ALLOWED_CAVEAT_CLASSES.contains(&cc.as_str()) {
                errs.push(CanonicalValidationError::UnknownCaveatClass {
                    value: cc.clone(),
                    allowed: ALLOWED_CAVEAT_CLASSES
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                });
            }
        }

        // Rule 3: Drop-in layer requirements
        if cap.tier == "drop_in" {
            let required = required_drop_in_layers_for_category(&cap.category);
            for layer in required {
                let val = layer_value(cap, layer);
                if val != "complete" {
                    errs.push(CanonicalValidationError::DropInLayerIncomplete {
                        id: cap.id.clone(),
                        layer: layer.to_string(),
                        value: val.to_string(),
                    });
                }
            }
        }

        // Rule 4: Drop-in evidence weakness
        if cap.tier == "drop_in" {
            let has_exception = cap.differential_exception.unwrap_or(false);
            if !has_exception {
                // For uri/cli categories, unit evidence is acceptable
                let min_rank: u8 = if cap.category == "uri" || cap.category == "cli" {
                    2 // unit is the floor
                } else {
                    1 // integration is the floor
                };
                let rank = evidence_rank(&cap.evidence);
                if rank > min_rank {
                    let threshold = if min_rank == 2 { "unit" } else { "integration" };
                    errs.push(CanonicalValidationError::DropInEvidenceWeak {
                        id: cap.id.clone(),
                        evidence: cap.evidence.clone(),
                        threshold: threshold.to_string(),
                    });
                }
            }
        }

        // Rule 5: compatible_with_warning without diagnostic or notes (warning)
        if cap.tier == "compatible_with_warning" {
            let has_diagnostic = cap.diagnostic.as_ref().is_some_and(|d| !d.is_empty());
            let has_notes = !cap.notes.trim().is_empty();
            if !has_diagnostic && !has_notes {
                errs.warn(CanonicalValidationError::CompatibleWithoutDiagnostic {
                    id: cap.id.clone(),
                });
            }
        }

        // Rule 6: intentional_non_parity without rationale
        if cap.tier == "intentional_non_parity" {
            let has_rationale = cap.rationale.as_ref().is_some_and(|r| !r.trim().is_empty());
            if !has_rationale {
                errs.push(
                    CanonicalValidationError::IntentionalNonParityWithoutRationale {
                        id: cap.id.clone(),
                    },
                );
            }
        }

        // Rule 7: unsupported with runtime=complete
        if cap.tier == "unsupported" && cap.runtime == "complete" {
            errs.push(CanonicalValidationError::UnsupportedWithRuntime { id: cap.id.clone() });
        }

        // Rule 8: drop_in with runtime=refused
        if cap.tier == "drop_in" && cap.runtime == "refused" {
            errs.push(CanonicalValidationError::DropInWithRuntimeRefused { id: cap.id.clone() });
        }

        // Rule 9: protocol-crate-only drop_in contradiction
        if cap.tier == "drop_in" && (cap.config == "refused" || cap.runtime == "refused") {
            errs.push(CanonicalValidationError::DropInProtocolCrateOnly {
                id: cap.id.clone(),
                config: cap.config.clone(),
                runtime: cap.runtime.clone(),
            });
        }

        // Rule 10: CLI without tests (warning)
        if cap.category == "cli" && cap.tests.is_empty() && cap.notes.trim().is_empty() {
            errs.warn(CanonicalValidationError::CliWithoutTests { id: cap.id.clone() });
        }

        // Rule 11: Python drop_in with no test evidence
        if cap.tier == "drop_in" && cap.category == "python" && cap.evidence == "none" {
            errs.push(CanonicalValidationError::PythonDropInNoEvidence {
                id: cap.id.clone(),
                evidence: cap.evidence.clone(),
            });
        }

        // Rule 13: Typo detection (egress_behavior instead of eggress_behavior)
        if !cap.egress_behavior.is_empty() {
            errs.warn(CanonicalValidationError::EgressBehaviorTypo { id: cap.id.clone() });
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Parse and validate a canonical manifest from a filesystem path.
pub fn validate_canonical_manifest_file(
    path: &Path,
) -> Result<CanonicalManifest, CanonicalValidationErrors> {
    let content = fs::read_to_string(path).map_err(|e| {
        let mut errs = CanonicalValidationErrors::new();
        errs.push(CanonicalValidationError::Io {
            message: format!("failed to read {}: {}", path.display(), e),
        });
        errs
    })?;

    let manifest: CanonicalManifest = toml::from_str(&content).map_err(|e| {
        let mut errs = CanonicalValidationErrors::new();
        errs.push(CanonicalValidationError::TomlParse {
            message: e.to_string(),
        });
        errs
    })?;

    validate_canonical_manifest(&manifest)?;
    Ok(manifest)
}

#[cfg(test)]
#[allow(clippy::all)]
mod tests {
    use super::*;

    fn make_meta() -> CanonicalManifestMeta {
        CanonicalManifestMeta {
            manifest_version: PINNED_MANIFEST_VERSION.to_string(),
            pproxy_version: PINNED_PPROXY_VERSION.to_string(),
            schema: PINNED_SCHEMA.to_string(),
        }
    }

    fn make_manifest(capabilities: Vec<CanonicalCapability>) -> CanonicalManifest {
        CanonicalManifest {
            meta: make_meta(),
            capability: capabilities,
        }
    }

    fn default_cap(id: &str) -> CanonicalCapability {
        CanonicalCapability {
            id: id.to_string(),
            category: "cli".to_string(),
            pproxy_surface: String::new(),
            pproxy_behavior: String::new(),
            eggress_behavior: String::new(),
            egress_behavior: String::new(),
            tier: "drop_in".to_string(),
            parser: "complete".to_string(),
            translator: "complete".to_string(),
            config: "complete".to_string(),
            runtime: "complete".to_string(),
            cli: "complete".to_string(),
            python: "not_applicable".to_string(),
            docs: "complete".to_string(),
            evidence: "integration".to_string(),
            tests: vec!["cli_tests".to_string()],
            notes: String::new(),
            diagnostic: None,
            rationale: None,
            caveat_class: None,
            differential_exception: None,
        }
    }

    #[test]
    fn valid_manifest_passes() {
        let cap = default_cap("test.ok");
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn meta_version_mismatch() {
        let mut manifest = make_manifest(vec![default_cap("f")]);
        manifest.meta.manifest_version = "2".to_string();
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, CanonicalValidationError::ManifestVersionMismatch { .. })),
            "expected ManifestVersionMismatch"
        );
    }

    #[test]
    fn meta_pproxy_version_mismatch() {
        let mut manifest = make_manifest(vec![default_cap("f")]);
        manifest.meta.pproxy_version = "1.0.0".to_string();
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, CanonicalValidationError::PproxyVersionMismatch { .. })),
            "expected PproxyVersionMismatch"
        );
    }

    #[test]
    fn meta_schema_mismatch() {
        let mut manifest = make_manifest(vec![default_cap("f")]);
        manifest.meta.schema = "old_schema".to_string();
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, CanonicalValidationError::SchemaMismatch { .. })),
            "expected SchemaMismatch"
        );
    }

    #[test]
    fn duplicate_ids_fail() {
        let manifest = make_manifest(vec![default_cap("dup"), default_cap("dup")]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(
                |e| matches!(e, CanonicalValidationError::DuplicateId { id, .. } if id == "dup")
            ),
            "expected DuplicateId"
        );
    }

    #[test]
    fn unknown_tier_fails() {
        let mut cap = default_cap("bad_tier");
        cap.tier = "bogus".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, CanonicalValidationError::UnknownTier { value, .. } if value == "bogus")),
            "expected UnknownTier"
        );
    }

    #[test]
    fn unknown_layer_value_fails() {
        let mut cap = default_cap("bad_layer");
        cap.parser = "bogus".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::UnknownLayer { value, layer, .. }
                    if value == "bogus" && layer == "parser"
            )),
            "expected UnknownLayer for parser"
        );
    }

    #[test]
    fn unknown_evidence_fails() {
        let mut cap = default_cap("bad_ev");
        cap.evidence = "bogus".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, CanonicalValidationError::UnknownEvidence { value, .. } if value == "bogus")),
            "expected UnknownEvidence"
        );
    }

    #[test]
    fn unknown_category_fails() {
        let mut cap = default_cap("bad_cat");
        cap.category = "bogus".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, CanonicalValidationError::UnknownCategory { value, .. } if value == "bogus")),
            "expected UnknownCategory"
        );
    }

    #[test]
    fn unknown_caveat_class_fails() {
        let mut cap = default_cap("bad_cc");
        cap.caveat_class = Some("bogus".to_string());
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::UnknownCaveatClass { value, .. } if value == "bogus"
            )),
            "expected UnknownCaveatClass"
        );
    }

    #[test]
    fn drop_in_layer_incomplete_fails() {
        let mut cap = default_cap("incomplete");
        cap.category = "protocol".to_string();
        cap.config = "partial".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::DropInLayerIncomplete { id, layer, .. }
                    if id == "incomplete" && layer == "config"
            )),
            "expected DropInLayerIncomplete for config"
        );
    }

    #[test]
    fn drop_in_python_layer_requirements() {
        let mut cap = default_cap("py_bad");
        cap.category = "python".to_string();
        cap.python = "partial".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::DropInLayerIncomplete { id, layer, .. }
                    if id == "py_bad" && layer == "python"
            )),
            "expected DropInLayerIncomplete for python"
        );
    }

    #[test]
    fn drop_in_cli_layer_requirements() {
        let mut cap = default_cap("cli_bad");
        cap.cli = "partial".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::DropInLayerIncomplete { id, layer, .. }
                    if id == "cli_bad" && layer == "cli"
            )),
            "expected DropInLayerIncomplete for cli"
        );
    }

    #[test]
    fn drop_in_routing_layer_requirements() {
        let mut cap = default_cap("rt_bad");
        cap.category = "routing".to_string();
        cap.parser = "complete".to_string();
        cap.translator = "complete".to_string();
        cap.config = "complete".to_string();
        cap.runtime = "partial".to_string();
        cap.docs = "complete".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::DropInLayerIncomplete { id, layer, .. }
                    if id == "rt_bad" && layer == "runtime"
            )),
            "expected DropInLayerIncomplete for runtime in routing"
        );
    }

    #[test]
    fn drop_in_evidence_weak_fails() {
        let mut cap = default_cap("weak_ev");
        cap.category = "protocol".to_string();
        cap.evidence = "unit".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::DropInEvidenceWeak { id, .. } if id == "weak_ev"
            )),
            "expected DropInEvidenceWeak"
        );
    }

    #[test]
    fn drop_in_evidence_with_differential_exception_passes() {
        let mut cap = default_cap("exc");
        cap.category = "protocol".to_string();
        cap.evidence = "unit".to_string();
        cap.differential_exception = Some(true);
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn drop_in_uri_unit_evidence_passes() {
        let mut cap = default_cap("uri_unit");
        cap.category = "uri".to_string();
        cap.evidence = "unit".to_string();
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn drop_in_cli_unit_evidence_passes() {
        let mut cap = default_cap("cli_unit");
        cap.evidence = "unit".to_string();
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn compatible_with_warning_without_diagnostic_or_notes_is_not_error() {
        let mut cap = default_cap("cww_bad");
        cap.tier = "compatible_with_warning".to_string();
        cap.diagnostic = None;
        cap.notes = String::new();
        let manifest = make_manifest(vec![cap]);
        // Warning only — should not fail validation
        let result = validate_canonical_manifest(&manifest);
        assert!(
            result.is_ok(),
            "compatible_with_warning without diagnostic should be a warning, not an error: {:?}",
            result.err()
        );
    }

    #[test]
    fn compatible_with_warning_with_diagnostic_passes() {
        let mut cap = default_cap("cww_ok");
        cap.tier = "compatible_with_warning".to_string();
        cap.diagnostic = Some("scheduler".to_string());
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn compatible_with_warning_with_notes_passes() {
        let mut cap = default_cap("cww_notes");
        cap.tier = "compatible_with_warning".to_string();
        cap.diagnostic = None;
        cap.notes = "some migration note".to_string();
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn intentional_non_parity_without_rationale_fails() {
        let mut cap = default_cap("inp_bad");
        cap.tier = "intentional_non_parity".to_string();
        cap.rationale = None;
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::IntentionalNonParityWithoutRationale { id, .. }
                    if id == "inp_bad"
            )),
            "expected IntentionalNonParityWithoutRationale"
        );
    }

    #[test]
    fn intentional_non_parity_with_rationale_passes() {
        let mut cap = default_cap("inp_ok");
        cap.tier = "intentional_non_parity".to_string();
        cap.rationale = Some("Design choice".to_string());
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn intentional_non_parity_with_empty_rationale_fails() {
        let mut cap = default_cap("inp_ws");
        cap.tier = "intentional_non_parity".to_string();
        cap.rationale = Some("   ".to_string());
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::IntentionalNonParityWithoutRationale { id, .. }
                    if id == "inp_ws"
            )),
            "expected IntentionalNonParityWithoutRationale for whitespace rationale"
        );
    }

    #[test]
    fn unsupported_with_runtime_complete_fails() {
        let mut cap = default_cap("uns_bad");
        cap.tier = "unsupported".to_string();
        cap.runtime = "complete".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::UnsupportedWithRuntime { id, .. } if id == "uns_bad"
            )),
            "expected UnsupportedWithRuntime"
        );
    }

    #[test]
    fn unsupported_with_runtime_refused_passes() {
        let mut cap = default_cap("uns_ok");
        cap.tier = "unsupported".to_string();
        cap.runtime = "refused".to_string();
        cap.config = "not_applicable".to_string();
        cap.parser = "not_applicable".to_string();
        cap.translator = "not_applicable".to_string();
        cap.cli = "not_applicable".to_string();
        cap.python = "not_applicable".to_string();
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn drop_in_with_runtime_refused_fails() {
        let mut cap = default_cap("dir_bad");
        cap.tier = "drop_in".to_string();
        cap.runtime = "refused".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::DropInWithRuntimeRefused { id, .. } if id == "dir_bad"
            )),
            "expected DropInWithRuntimeRefused"
        );
    }

    #[test]
    fn drop_in_protocol_crate_only_config_refused_fails() {
        let mut cap = default_cap("pcr_bad");
        cap.tier = "drop_in".to_string();
        cap.config = "refused".to_string();
        cap.runtime = "complete".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::DropInProtocolCrateOnly { id, .. } if id == "pcr_bad"
            )),
            "expected DropInProtocolCrateOnly"
        );
    }

    #[test]
    fn cli_without_tests_is_not_error() {
        let mut cap = default_cap("cli_no_tests");
        cap.category = "cli".to_string();
        cap.tests = vec![];
        cap.notes = String::new();
        let manifest = make_manifest(vec![cap]);
        // Warning only — should not fail validation
        let result = validate_canonical_manifest(&manifest);
        assert!(
            result.is_ok(),
            "CLI without tests should be a warning, not an error: {:?}",
            result.err()
        );
    }

    #[test]
    fn cli_with_notes_no_tests_passes() {
        let mut cap = default_cap("cli_notes");
        cap.category = "cli".to_string();
        cap.tests = vec![];
        cap.notes = "use systemd".to_string();
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn python_drop_in_with_evidence_none_fails() {
        let mut cap = default_cap("py_none");
        cap.category = "python".to_string();
        cap.python = "complete".to_string();
        cap.evidence = "none".to_string();
        cap.docs = "complete".to_string();
        cap.parser = "not_applicable".to_string();
        cap.translator = "not_applicable".to_string();
        cap.config = "not_applicable".to_string();
        cap.runtime = "not_applicable".to_string();
        cap.cli = "not_applicable".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::PythonDropInNoEvidence { id, .. } if id == "py_none"
            )),
            "expected PythonDropInNoEvidence"
        );
    }

    #[test]
    fn python_drop_in_with_integration_evidence_passes() {
        let mut cap = default_cap("py_int");
        cap.category = "python".to_string();
        cap.python = "complete".to_string();
        cap.evidence = "integration".to_string();
        cap.docs = "complete".to_string();
        cap.parser = "not_applicable".to_string();
        cap.translator = "not_applicable".to_string();
        cap.config = "not_applicable".to_string();
        cap.runtime = "not_applicable".to_string();
        cap.cli = "not_applicable".to_string();
        cap.tests = vec!["test_py".to_string()];
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn egress_behavior_typo_is_not_error() {
        let mut cap = default_cap("typo_cap");
        cap.egress_behavior = "some behavior".to_string();
        let manifest = make_manifest(vec![cap]);
        // Warning only — should not fail validation
        let result = validate_canonical_manifest(&manifest);
        assert!(
            result.is_ok(),
            "egress_behavior typo should be a warning, not an error: {:?}",
            result.err()
        );
    }

    #[test]
    fn no_typo_when_egress_behavior_empty() {
        let mut cap = default_cap("clean_cap");
        cap.egress_behavior = String::new();
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn multiple_errors_collected() {
        let mut manifest = make_manifest(vec![default_cap("dup"), default_cap("dup")]);
        manifest.meta.pproxy_version = "0.0.1".to_string();
        manifest.meta.schema = "wrong".to_string();
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.len() >= 3,
            "expected at least 3 errors, got {}",
            errs.len()
        );
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, CanonicalValidationError::PproxyVersionMismatch { .. })));
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, CanonicalValidationError::SchemaMismatch { .. })));
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, CanonicalValidationError::DuplicateId { .. })));
    }

    #[test]
    fn validation_errors_collection() {
        let mut errs = CanonicalValidationErrors::new();
        assert!(errs.is_empty());
        assert_eq!(errs.len(), 0);
        errs.push(CanonicalValidationError::DuplicateId {
            id: "a".to_string(),
        });
        errs.push(CanonicalValidationError::DuplicateId {
            id: "b".to_string(),
        });
        assert!(!errs.is_empty());
        assert_eq!(errs.len(), 2);
    }

    #[test]
    fn validate_canonical_manifest_file_missing_path() {
        let path = Path::new("/nonexistent/path/manifest.toml");
        let result = validate_canonical_manifest_file(path);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, CanonicalValidationError::Io { .. })));
    }

    #[test]
    fn toml_parse_error() {
        let bad_toml = "this is not [valid toml {{{{";
        let result: Result<CanonicalManifest, _> = toml::from_str(bad_toml);
        assert!(result.is_err());
    }

    #[test]
    fn validate_real_canonical_manifest() {
        let path = match find_canonical_manifest_path() {
            Some(p) => p,
            None => {
                eprintln!("canonical manifest not found, skipping");
                return;
            }
        };
        eprintln!("Validating canonical manifest at: {}", path.display());
        match validate_canonical_manifest_file(&path) {
            Ok(manifest) => {
                eprintln!(
                    "Canonical manifest OK: {} capabilities, meta.schema={}",
                    manifest.capability.len(),
                    manifest.meta.schema
                );
            }
            Err(errs) => {
                eprintln!(
                    "Canonical manifest validation FAILED with {} errors:",
                    errs.len()
                );
                for (i, err) in errs.errors.iter().enumerate() {
                    eprintln!("  ERROR {}: {}", i + 1, err);
                }
                for (i, warn) in errs.warnings.iter().enumerate() {
                    eprintln!("  WARNING {}: {}", i + 1, warn);
                }
                panic!(
                    "canonical manifest validation failed with {} errors (see above)",
                    errs.len()
                );
            }
        }
    }

    #[test]
    fn parity_manifest_consistency() {
        let path = match find_canonical_manifest_path() {
            Some(p) => p,
            None => {
                eprintln!("canonical manifest not found, skipping parity consistency test");
                return;
            }
        };
        let manifest =
            validate_canonical_manifest_file(&path).expect("canonical manifest should be valid");

        let workspace_root = path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .expect("should have workspace root");

        // Count capabilities by tier
        let mut tier_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for cap in &manifest.capability {
            *tier_counts.entry(cap.tier.clone()).or_insert(0) += 1;
        }
        let total = manifest.capability.len();

        // ── Check PPROXY_PARITY_REPORT.md ──────────────────────────────
        let report_path = workspace_root.join("docs/parity/PPROXY_PARITY_REPORT.md");
        if report_path.exists() {
            let report = fs::read_to_string(&report_path)
                .expect("should be able to read PPROXY_PARITY_REPORT.md");

            // Verify total count appears in the report
            let total_marker = format!("| **Total** | **{total}** |");
            assert!(
                report.contains(&total_marker),
                "PPROXY_PARITY_REPORT.md does not contain total count {total}; \
                 expected marker: {total_marker}"
            );

            // Verify each tier count appears
            for (tier, count) in &tier_counts {
                let tier_marker = format!("| `{tier}` | {count} |");
                assert!(
                    report.contains(&tier_marker),
                    "PPROXY_PARITY_REPORT.md does not contain tier count for `{tier}` (expected {count}); \
                     expected marker: {tier_marker}"
                );
            }

            // Verify the report references the canonical manifest path
            assert!(
                report.contains("pproxy_capability_manifest.toml"),
                "PPROXY_PARITY_REPORT.md should reference the canonical manifest file"
            );

            // Verify it is NOT referencing the legacy manifest
            assert!(
                !report.contains("tests/compat/pproxy_manifest.toml"),
                "PPROXY_PARITY_REPORT.md should NOT reference the legacy manifest"
            );

            eprintln!(
                "PPROXY_PARITY_REPORT.md consistent: {} total, tiers: {:?}",
                total, tier_counts
            );
        } else {
            eprintln!("PPROXY_PARITY_REPORT.md not found, skipping report check");
        }

        // ── Check README.md references canonical manifest ──────────────
        let readme_path = workspace_root.join("docs/parity/README.md");
        if readme_path.exists() {
            let readme =
                fs::read_to_string(&readme_path).expect("should be able to read parity README.md");
            assert!(
                readme.contains("pproxy_capability_manifest.toml"),
                "docs/parity/README.md should reference the canonical manifest"
            );
            eprintln!("docs/parity/README.md references canonical manifest");
        }

        // ── Check COMPATIBILITY_EVIDENCE.md references canonical manifest ──
        let evidence_path = workspace_root.join("docs/COMPATIBILITY_EVIDENCE.md");
        if evidence_path.exists() {
            let evidence = fs::read_to_string(&evidence_path)
                .expect("should be able to read COMPATIBILITY_EVIDENCE.md");
            // The evidence doc should at least reference the parity concept
            // (it may reference either manifest, but it should exist)
            assert!(
                evidence.contains("manifest") || evidence.contains("Manifest"),
                "COMPATIBILITY_EVIDENCE.md should reference the manifest"
            );
            eprintln!("COMPATIBILITY_EVIDENCE.md exists and references manifest");
        }
    }

    #[test]
    fn evidence_rank_ordering() {
        assert!(evidence_rank("differential") < evidence_rank("integration"));
        assert!(evidence_rank("integration") < evidence_rank("unit"));
        assert!(evidence_rank("unit") < evidence_rank("synthetic"));
        assert!(evidence_rank("synthetic") < evidence_rank("docs_only"));
        assert!(evidence_rank("docs_only") < evidence_rank("none"));
    }

    #[test]
    fn all_allowed_tiers_are_valid() {
        for tier in ALLOWED_TIERS {
            let mut cap = default_cap(&format!("tier_{tier}"));
            cap.tier = tier.to_string();
            // Make it a valid non-drop_in to avoid layer requirements
            if *tier == "drop_in" {
                cap.category = "cli".to_string();
                cap.cli = "complete".to_string();
                cap.docs = "complete".to_string();
            } else {
                cap.tier = tier.to_string();
            }
            let manifest = make_manifest(vec![cap]);
            // Should not produce UnknownTier error
            let errs = validate_canonical_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, CanonicalValidationError::UnknownTier { .. })),
                    "tier \"{tier}\" should be valid"
                );
            }
        }
    }

    #[test]
    fn all_allowed_layers_are_valid() {
        for layer in ALLOWED_LAYERS {
            let mut cap = default_cap(&format!("layer_{layer}"));
            cap.parser = layer.to_string();
            cap.translator = layer.to_string();
            cap.config = layer.to_string();
            cap.runtime = layer.to_string();
            cap.cli = layer.to_string();
            cap.python = layer.to_string();
            cap.docs = layer.to_string();
            // Set tier to avoid layer-requirement errors
            cap.tier = "native_equivalent".to_string();
            let manifest = make_manifest(vec![cap]);
            let errs = validate_canonical_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, CanonicalValidationError::UnknownLayer { .. })),
                    "layer \"{layer}\" should be valid"
                );
            }
        }
    }

    #[test]
    fn all_allowed_evidence_values_are_valid() {
        for ev in ALLOWED_EVIDENCE {
            let mut cap = default_cap(&format!("ev_{ev}"));
            cap.evidence = ev.to_string();
            cap.tier = "native_equivalent".to_string();
            let manifest = make_manifest(vec![cap]);
            let errs = validate_canonical_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, CanonicalValidationError::UnknownEvidence { .. })),
                    "evidence \"{ev}\" should be valid"
                );
            }
        }
    }

    #[test]
    fn all_allowed_categories_are_valid() {
        for cat in ALLOWED_CATEGORIES {
            let mut cap = default_cap(&format!("cat_{cat}"));
            cap.category = cat.to_string();
            cap.tier = "native_equivalent".to_string();
            let manifest = make_manifest(vec![cap]);
            let errs = validate_canonical_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, CanonicalValidationError::UnknownCategory { .. })),
                    "category \"{cat}\" should be valid"
                );
            }
        }
    }

    #[test]
    fn all_allowed_caveat_classes_are_valid() {
        for cc in ALLOWED_CAVEAT_CLASSES {
            let mut cap = default_cap(&format!("cc_{cc}"));
            cap.tier = "intentional_non_parity".to_string();
            cap.caveat_class = Some(cc.to_string());
            cap.rationale = Some("test rationale".to_string());
            cap.config = "refused".to_string();
            cap.runtime = "refused".to_string();
            let manifest = make_manifest(vec![cap]);
            let errs = validate_canonical_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, CanonicalValidationError::UnknownCaveatClass { .. })),
                    "caveat_class \"{cc}\" should be valid"
                );
            }
        }
    }

    #[test]
    fn drop_in_with_differential_evidence_passes() {
        let mut cap = default_cap("diff_ok");
        cap.category = "protocol".to_string();
        cap.evidence = "differential".to_string();
        cap.differential_exception = Some(true);
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn drop_in_synthetic_evidence_without_exception_fails() {
        let mut cap = default_cap("syn_bad");
        cap.category = "protocol".to_string();
        cap.evidence = "synthetic".to_string();
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                CanonicalValidationError::DropInEvidenceWeak { id, .. } if id == "syn_bad"
            )),
            "expected DropInEvidenceWeak for synthetic evidence"
        );
    }

    #[test]
    fn compatible_with_warning_does_not_trigger_layer_rules() {
        let mut cap = default_cap("cww_layers");
        cap.tier = "compatible_with_warning".to_string();
        cap.config = "partial".to_string();
        cap.diagnostic = Some("test".to_string());
        let manifest = make_manifest(vec![cap]);
        let errs = validate_canonical_manifest(&manifest);
        // Should not produce DropInLayerIncomplete
        if let Err(ref e) = errs {
            assert!(
                !e.errors
                    .iter()
                    .any(|e| matches!(e, CanonicalValidationError::DropInLayerIncomplete { .. })),
                "compatible_with_warning should not trigger layer rules"
            );
        }
    }

    #[test]
    fn intentional_non_parity_with_caveat_class_passes() {
        let mut cap = default_cap("inp_cc");
        cap.tier = "intentional_non_parity".to_string();
        cap.rationale = Some("Design decision".to_string());
        cap.caveat_class = Some("intentional_non_parity".to_string());
        let manifest = make_manifest(vec![cap]);
        let result = validate_canonical_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn find_canonical_manifest_path_returns_some() {
        let path = find_canonical_manifest_path();
        if path.is_some() {
            let p = path.unwrap();
            assert!(
                p.ends_with("pproxy_capability_manifest.toml"),
                "path should end with manifest filename"
            );
            assert!(p.exists(), "path should exist");
        }
    }
}
