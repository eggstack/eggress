//! Strict manifest validation for the pproxy 2.7.9 behavioral compatibility manifest.
//!
//! Validates `docs/parity/pproxy_2_7_9_strict_manifest.toml`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const ALLOWED_CATEGORIES: &[&str] = &[
    "python_namespace",
    "cli_option",
    "protocol",
    "cipher",
    "composition",
    "process",
    "failure",
];

pub const ALLOWED_STATUSES: &[&str] = &[
    "gap",
    "drop_in",
    "known_upstream_defect",
    "platform_constraint",
    "not_applicable",
    "intentional_non_parity",
];

pub const ALLOWED_COMPARATORS: &[&str] = &[
    "async_callable_signature",
    "module_existence",
    "constant_value",
    "enum_membership",
    "method_signature",
    "property_existence",
    "class_hierarchy",
    "cli_flag_parse",
    "cli_flag_rejection",
    "protocol_wire",
    "cipher_roundtrip",
    "cipher_kat",
    "process_lifecycle",
    "failure_class",
    "composition_validity",
    "composition_rejection",
];

pub const ALLOWED_OWNERS: &[&str] = &["track-a", "track-b", "track-c"];

pub const ALLOWED_MILESTONES: &[&str] = &["A", "B", "C", "D", "E", "F"];

/// Milestone order for "current milestone" checking.
const MILESTONE_ORDER: &[&str] = &["A", "B", "C", "D", "E", "F"];

/// Current release milestone — records at or below this milestone
/// with non-terminal status are flagged. Set to "A" while milestone B
/// work is still in progress.
const CURRENT_MILESTONE: &str = "A";

/// Terminal statuses that do not represent unresolved progress.
const TERMINAL_STATUSES: &[&str] = &[
    "drop_in",
    "not_applicable",
    "known_upstream_defect",
    "platform_constraint",
    "intentional_non_parity",
];

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Top-level metadata section of the strict manifest.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct StrictManifestMeta {
    pub manifest_version: String,
    pub pproxy_version: String,
    pub schema: String,
    #[serde(default)]
    pub policy_ref: String,
    #[serde(default)]
    pub oracle_ref: String,
}

/// A single record entry in the strict manifest.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct StrictRecord {
    pub id: String,
    pub category: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub oracle_probe: String,
    #[serde(default)]
    pub candidate_probe: String,
    #[serde(default)]
    pub comparator: String,
    pub status: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub milestone: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub python_versions: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub test_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

/// The complete strict manifest structure.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct StrictManifest {
    pub meta: StrictManifestMeta,
    pub record: Vec<StrictRecord>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A single validation error with rule number and context.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum StrictValidationError {
    #[error("TOML parse error: {message}")]
    TomlParse { message: String },

    #[error("file I/O error: {message}")]
    Io { message: String },

    // Rule 1: Unknown enum values
    #[error("unknown category \"{value}\" (valid: {allowed:?})")]
    UnknownCategory { value: String, allowed: Vec<String> },

    #[error("unknown status \"{value}\" (valid: {allowed:?})")]
    UnknownStatus { value: String, allowed: Vec<String> },

    #[error("unknown comparator \"{value}\" (valid: {allowed:?})")]
    UnknownComparator { value: String, allowed: Vec<String> },

    #[error("unknown owner \"{value}\" (valid: {allowed:?})")]
    UnknownOwner { value: String, allowed: Vec<String> },

    #[error("unknown milestone \"{value}\" (valid: {allowed:?})")]
    UnknownMilestone { value: String, allowed: Vec<String> },

    // Rule 2: Duplicate IDs
    #[error("duplicate record id: \"{id}\"")]
    DuplicateId { id: String },

    // Rule 3: Empty ID
    #[error("record has empty id")]
    EmptyId,

    // Rule 4: drop_in without evidence or tests
    #[error("drop_in record \"{id}\" has empty evidence_refs and test_refs")]
    DropInWithoutEvidence { id: String },

    // Rule 5: drop_in without oracle_probe
    #[error("drop_in record \"{id}\" has empty oracle_probe")]
    DropInWithoutOracleProbe { id: String },

    // Rule 6: Unresolved progress state at current milestone
    #[error(
        "record \"{id}\" has non-terminal status \"{status}\" at milestone \"{milestone}\" \
         (at or below current milestone {current})"
    )]
    UnresolvedProgress {
        id: String,
        status: String,
        milestone: String,
        current: String,
    },
}

/// A collection of validation errors.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{errors:#?}")]
pub struct StrictValidationErrors {
    pub errors: Vec<StrictValidationError>,
}

impl StrictValidationErrors {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn push(&mut self, err: StrictValidationError) {
        self.errors.push(err);
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.errors.len()
    }
}

impl Default for StrictValidationErrors {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn milestone_index(milestone: &str) -> Option<usize> {
    MILESTONE_ORDER.iter().position(|&m| m == milestone)
}

/// Locate the strict manifest file relative to CARGO_MANIFEST_DIR.
pub fn find_strict_manifest_path() -> Option<PathBuf> {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let candidate = PathBuf::from(&manifest_dir)
            .join("../../docs/parity/pproxy_2_7_9_strict_manifest.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let cwd = std::env::current_dir().ok()?;
    let mut dir = cwd.as_path();
    loop {
        let candidate = dir.join("docs/parity/pproxy_2_7_9_strict_manifest.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

// ---------------------------------------------------------------------------
// Oracle Provenance Verification
// ---------------------------------------------------------------------------

/// Oracle hashes as loaded from `compat/pproxy-2.7.9/hashes.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OracleHashes {
    pub package: OracleHashPackage,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OracleHashPackage {
    pub version: String,
    pub source: String,
    pub sha256_sdist: Option<String>,
    pub sha256_wheel: Option<String>,
}

/// Oracle provenance as loaded from `compat/pproxy-2.7.9/provenance.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OracleProvenance {
    pub oracle: OracleProvenanceInner,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OracleProvenanceInner {
    pub package: String,
    pub version: String,
    pub source: String,
    pub license: String,
    pub retrieval_date: String,
}

/// Errors from oracle provenance verification.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum OracleError {
    #[error("file I/O error: {message}")]
    Io { message: String },

    #[error("TOML parse error: {message}")]
    TomlParse { message: String },

    #[error("expected pproxy version \"{expected}\" but found \"{found}\"")]
    VersionMismatch { expected: String, found: String },

    #[error("expected pproxy package \"{expected}\" but found \"{found}\"")]
    PackageMismatch { expected: String, found: String },

    #[error("wheel hash mismatch: expected \"{expected}\" but computed \"{found}\"")]
    WheelHashMismatch { expected: String, found: String },

    #[error("no wheel hash recorded in hashes.toml")]
    MissingWheelHash,

    #[error("hash computation failed: {message}")]
    HashComputationFailed { message: String },
}

/// Load and validate oracle hashes from the canonical path.
pub fn load_oracle_hashes(manifest_dir: &Path) -> Result<OracleHashes, OracleError> {
    let path = manifest_dir.join("compat/pproxy-2.7.9/hashes.toml");
    let content = fs::read_to_string(&path).map_err(|e| OracleError::Io {
        message: format!("failed to read {}: {}", path.display(), e),
    })?;
    let hashes: OracleHashes = toml::from_str(&content).map_err(|e| OracleError::TomlParse {
        message: e.to_string(),
    })?;
    Ok(hashes)
}

/// Load and validate oracle provenance from the canonical path.
pub fn load_oracle_provenance(manifest_dir: &Path) -> Result<OracleProvenance, OracleError> {
    let path = manifest_dir.join("compat/pproxy-2.7.9/provenance.toml");
    let content = fs::read_to_string(&path).map_err(|e| OracleError::Io {
        message: format!("failed to read {}: {}", path.display(), e),
    })?;
    let prov: OracleProvenance = toml::from_str(&content).map_err(|e| OracleError::TomlParse {
        message: e.to_string(),
    })?;
    Ok(prov)
}

/// Verify that the strict manifest's pproxy_version matches the oracle provenance.
pub fn verify_manifest_oracle_version(
    manifest: &StrictManifest,
    provenance: &OracleProvenance,
) -> Result<(), OracleError> {
    if manifest.meta.pproxy_version != provenance.oracle.version {
        return Err(OracleError::VersionMismatch {
            expected: provenance.oracle.version.clone(),
            found: manifest.meta.pproxy_version.clone(),
        });
    }
    if provenance.oracle.package != "pproxy" {
        return Err(OracleError::PackageMismatch {
            expected: "pproxy".to_string(),
            found: provenance.oracle.package.clone(),
        });
    }
    Ok(())
}

/// Verify a wheel file matches the expected hash.
///
/// Note: This function requires the `sha2` crate to be available at runtime.
/// If SHA256 computation is not available, it returns an error.
pub fn verify_wheel_hash(_wheel_path: &Path, _expected_hash: &str) -> Result<(), OracleError> {
    // SHA256 verification requires the sha2 crate. Since eggress-testkit
    // doesn't depend on sha2, this function is a placeholder that documents
    // the verification interface. Actual hash verification is performed by
    // the oracle runner at runtime.
    Err(OracleError::HashComputationFailed {
        message: "sha2 feature required for hash verification; use oracle runner at runtime"
            .to_string(),
    })
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a parsed strict manifest.
///
/// Returns `Ok(())` when all invariants hold, or `Err(StrictValidationErrors)`
/// listing every violation found.
pub fn validate_strict_manifest(manifest: &StrictManifest) -> Result<(), StrictValidationErrors> {
    let mut errs = StrictValidationErrors::new();

    // Rule 2: Duplicate IDs
    let mut seen_ids = HashSet::new();
    for rec in &manifest.record {
        if !seen_ids.insert(rec.id.clone()) {
            errs.push(StrictValidationError::DuplicateId { id: rec.id.clone() });
        }
    }

    // Per-record validations
    for rec in &manifest.record {
        // Rule 3: Empty ID
        if rec.id.is_empty() {
            errs.push(StrictValidationError::EmptyId);
            continue;
        }

        // Rule 1: Valid category
        if !rec.category.is_empty() && !ALLOWED_CATEGORIES.contains(&rec.category.as_str()) {
            errs.push(StrictValidationError::UnknownCategory {
                value: rec.category.clone(),
                allowed: ALLOWED_CATEGORIES.iter().map(|s| s.to_string()).collect(),
            });
        }

        // Rule 1: Valid status
        if !rec.status.is_empty() && !ALLOWED_STATUSES.contains(&rec.status.as_str()) {
            errs.push(StrictValidationError::UnknownStatus {
                value: rec.status.clone(),
                allowed: ALLOWED_STATUSES.iter().map(|s| s.to_string()).collect(),
            });
        }

        // Rule 1: Valid comparator
        if !rec.comparator.is_empty() && !ALLOWED_COMPARATORS.contains(&rec.comparator.as_str()) {
            errs.push(StrictValidationError::UnknownComparator {
                value: rec.comparator.clone(),
                allowed: ALLOWED_COMPARATORS.iter().map(|s| s.to_string()).collect(),
            });
        }

        // Rule 1: Valid owner
        if !rec.owner.is_empty() && !ALLOWED_OWNERS.contains(&rec.owner.as_str()) {
            errs.push(StrictValidationError::UnknownOwner {
                value: rec.owner.clone(),
                allowed: ALLOWED_OWNERS.iter().map(|s| s.to_string()).collect(),
            });
        }

        // Rule 1: Valid milestone
        if !rec.milestone.is_empty() && !ALLOWED_MILESTONES.contains(&rec.milestone.as_str()) {
            errs.push(StrictValidationError::UnknownMilestone {
                value: rec.milestone.clone(),
                allowed: ALLOWED_MILESTONES.iter().map(|s| s.to_string()).collect(),
            });
        }

        // Rule 4: drop_in requires evidence_refs or test_refs
        if rec.status == "drop_in" && rec.evidence_refs.is_empty() && rec.test_refs.is_empty() {
            errs.push(StrictValidationError::DropInWithoutEvidence { id: rec.id.clone() });
        }

        // Rule 5: drop_in requires oracle_probe
        if rec.status == "drop_in" && rec.oracle_probe.is_empty() {
            errs.push(StrictValidationError::DropInWithoutOracleProbe { id: rec.id.clone() });
        }

        // Rule 6: Unresolved progress at current milestone
        if let Some(rec_idx) = milestone_index(&rec.milestone) {
            if let Some(cur_idx) = milestone_index(CURRENT_MILESTONE) {
                if rec_idx <= cur_idx && !TERMINAL_STATUSES.contains(&rec.status.as_str()) {
                    errs.push(StrictValidationError::UnresolvedProgress {
                        id: rec.id.clone(),
                        status: rec.status.clone(),
                        milestone: rec.milestone.clone(),
                        current: CURRENT_MILESTONE.to_string(),
                    });
                }
            }
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Parse and validate a strict manifest from a filesystem path.
pub fn validate_strict_manifest_file(
    path: &Path,
) -> Result<StrictManifest, StrictValidationErrors> {
    let content = fs::read_to_string(path).map_err(|e| {
        let mut errs = StrictValidationErrors::new();
        errs.push(StrictValidationError::Io {
            message: format!("failed to read {}: {}", path.display(), e),
        });
        errs
    })?;

    let manifest: StrictManifest = toml::from_str(&content).map_err(|e| {
        let mut errs = StrictValidationErrors::new();
        errs.push(StrictValidationError::TomlParse {
            message: e.to_string(),
        });
        errs
    })?;

    validate_strict_manifest(&manifest)?;
    Ok(manifest)
}

// ---------------------------------------------------------------------------
// Report Generation
// ---------------------------------------------------------------------------

/// Summary statistics for a strict manifest.
#[derive(Debug, Clone)]
pub struct StrictManifestSummary {
    pub total: usize,
    pub by_status: Vec<(String, usize)>,
    pub by_category: Vec<(String, usize)>,
    pub by_owner: Vec<(String, usize)>,
    pub by_milestone: Vec<(String, usize)>,
    pub terminal_count: usize,
    pub gap_count: usize,
}

/// Compute summary statistics from a manifest.
pub fn summarize_manifest(manifest: &StrictManifest) -> StrictManifestSummary {
    use std::collections::HashMap;

    let mut status_counts: HashMap<String, usize> = HashMap::new();
    let mut category_counts: HashMap<String, usize> = HashMap::new();
    let mut owner_counts: HashMap<String, usize> = HashMap::new();
    let mut milestone_counts: HashMap<String, usize> = HashMap::new();
    let mut terminal = 0;
    let mut gaps = 0;

    for rec in &manifest.record {
        *status_counts.entry(rec.status.clone()).or_insert(0) += 1;
        *category_counts.entry(rec.category.clone()).or_insert(0) += 1;
        if !rec.owner.is_empty() {
            *owner_counts.entry(rec.owner.clone()).or_insert(0) += 1;
        }
        if !rec.milestone.is_empty() {
            *milestone_counts.entry(rec.milestone.clone()).or_insert(0) += 1;
        }
        if TERMINAL_STATUSES.contains(&rec.status.as_str()) {
            terminal += 1;
        } else {
            gaps += 1;
        }
    }

    let mut by_status: Vec<_> = status_counts.into_iter().collect();
    by_status.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let mut by_category: Vec<_> = category_counts.into_iter().collect();
    by_category.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let mut by_owner: Vec<_> = owner_counts.into_iter().collect();
    by_owner.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let mut by_milestone: Vec<_> = milestone_counts.into_iter().collect();
    by_milestone.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

    StrictManifestSummary {
        total: manifest.record.len(),
        by_status,
        by_category,
        by_owner,
        by_milestone,
        terminal_count: terminal,
        gap_count: gaps,
    }
}

/// Generate a Markdown report from a manifest.
pub fn generate_strict_report(manifest: &StrictManifest) -> String {
    let summary = summarize_manifest(manifest);
    let mut out = String::with_capacity(4096);

    out.push_str("# pproxy 2.7.9 Strict Compatibility Report\n\n");
    out.push_str(&format!(
        "**Oracle version:** pproxy=={}\n",
        manifest.meta.pproxy_version
    ));
    out.push_str(&format!("**Manifest schema:** {}\n", manifest.meta.schema));
    out.push_str(&format!("**Policy:** {}\n", manifest.meta.policy_ref));
    out.push_str(&format!("**Oracle ref:** {}\n\n", manifest.meta.oracle_ref));

    out.push_str("## Summary\n\n");
    out.push_str("| Metric | Count |\n");
    out.push_str("|--------|-------|\n");
    out.push_str(&format!("| Total records | {} |\n", summary.total));
    out.push_str(&format!(
        "| Terminal (resolved) | {} |\n",
        summary.terminal_count
    ));
    out.push_str(&format!("| Gap (unresolved) | {} |\n", summary.gap_count));
    out.push_str(&format!(
        "| Certification readiness | {:.0}% |\n\n",
        if summary.total > 0 {
            (summary.terminal_count as f64 / summary.total as f64) * 100.0
        } else {
            0.0
        }
    ));

    out.push_str("### By Status\n\n");
    out.push_str("| Status | Count |\n");
    out.push_str("|--------|-------|\n");
    for (status, count) in &summary.by_status {
        out.push_str(&format!("| {} | {} |\n", status, count));
    }
    out.push('\n');

    out.push_str("### By Category\n\n");
    out.push_str("| Category | Count |\n");
    out.push_str("|----------|-------|\n");
    for (category, count) in &summary.by_category {
        out.push_str(&format!("| {} | {} |\n", category, count));
    }
    out.push('\n');

    out.push_str("### By Owner\n\n");
    out.push_str("| Owner | Count |\n");
    out.push_str("|-------|-------|\n");
    for (owner, count) in &summary.by_owner {
        out.push_str(&format!("| {} | {} |\n", owner, count));
    }
    out.push('\n');

    out.push_str("### By Milestone\n\n");
    out.push_str("| Milestone | Count |\n");
    out.push_str("|-----------|-------|\n");
    for (milestone, count) in &summary.by_milestone {
        out.push_str(&format!("| {} | {} |\n", milestone, count));
    }
    out.push('\n');

    out.push_str("## Gap Records\n\n");
    out.push_str("Records with non-terminal status requiring resolution:\n\n");
    let gaps: Vec<_> = manifest
        .record
        .iter()
        .filter(|r| !TERMINAL_STATUSES.contains(&r.status.as_str()))
        .collect();
    if gaps.is_empty() {
        out.push_str("_No unresolved gaps._\n\n");
    } else {
        out.push_str("| ID | Status | Category | Owner | Milestone |\n");
        out.push_str("|----|--------|----------|-------|----------|\n");
        for rec in &gaps {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                rec.id, rec.status, rec.category, rec.owner, rec.milestone
            ));
        }
        out.push('\n');
    }

    out.push_str("## Terminal Records\n\n");
    let terminals: Vec<_> = manifest
        .record
        .iter()
        .filter(|r| TERMINAL_STATUSES.contains(&r.status.as_str()))
        .collect();
    if terminals.is_empty() {
        out.push_str("_No terminal records._\n\n");
    } else {
        out.push_str("| ID | Status | Category | Notes |\n");
        out.push_str("|----|--------|----------|-------|\n");
        for rec in &terminals {
            let notes = if rec.notes.is_empty() {
                "-".to_string()
            } else if rec.notes.len() > 80 {
                format!("{}...", &rec.notes[..77])
            } else {
                rec.notes.clone()
            };
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                rec.id, rec.status, rec.category, notes
            ));
        }
        out.push('\n');
    }

    out
}

/// Write the strict report to a file.
pub fn write_strict_report(
    manifest: &StrictManifest,
    output_path: &Path,
) -> Result<(), std::io::Error> {
    let report = generate_strict_report(manifest);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, report)
}

#[cfg(test)]
#[allow(clippy::all)]
mod tests {
    use super::*;

    fn make_meta() -> StrictManifestMeta {
        StrictManifestMeta {
            manifest_version: "1".to_string(),
            pproxy_version: "2.7.9".to_string(),
            schema: "strict_1".to_string(),
            policy_ref: String::new(),
            oracle_ref: String::new(),
        }
    }

    fn make_manifest(records: Vec<StrictRecord>) -> StrictManifest {
        StrictManifest {
            meta: make_meta(),
            record: records,
        }
    }

    fn default_record(id: &str) -> StrictRecord {
        StrictRecord {
            id: id.to_string(),
            category: "protocol".to_string(),
            kind: "role".to_string(),
            module: "http".to_string(),
            name: format!("Test {id}"),
            oracle_probe: "test.probe".to_string(),
            candidate_probe: "test.probe".to_string(),
            comparator: "protocol_wire".to_string(),
            status: "drop_in".to_string(),
            owner: "track-b".to_string(),
            milestone: "B".to_string(),
            platforms: vec!["linux".to_string()],
            python_versions: vec![],
            depends_on: vec![],
            test_refs: vec!["test_ref".to_string()],
            evidence_refs: vec!["evidence_ref".to_string()],
            notes: String::new(),
        }
    }

    #[test]
    fn valid_manifest_passes() {
        let rec = default_record("test.ok");
        let manifest = make_manifest(vec![rec]);
        let result = validate_strict_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn duplicate_ids_fail() {
        let manifest = make_manifest(vec![default_record("dup"), default_record("dup")]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, StrictValidationError::DuplicateId { id, .. } if id == "dup")),
            "expected DuplicateId"
        );
    }

    #[test]
    fn empty_id_fails() {
        let rec = StrictRecord {
            id: String::new(),
            ..default_record("unused")
        };
        let manifest = make_manifest(vec![rec]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, StrictValidationError::EmptyId)),
            "expected EmptyId"
        );
    }

    #[test]
    fn unknown_category_fails() {
        let mut rec = default_record("bad_cat");
        rec.category = "bogus".to_string();
        let manifest = make_manifest(vec![rec]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                StrictValidationError::UnknownCategory { value, .. } if value == "bogus"
            )),
            "expected UnknownCategory"
        );
    }

    #[test]
    fn unknown_status_fails() {
        let mut rec = default_record("bad_status");
        rec.status = "bogus".to_string();
        let manifest = make_manifest(vec![rec]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                StrictValidationError::UnknownStatus { value, .. } if value == "bogus"
            )),
            "expected UnknownStatus"
        );
    }

    #[test]
    fn unknown_comparator_fails() {
        let mut rec = default_record("bad_comp");
        rec.comparator = "bogus".to_string();
        let manifest = make_manifest(vec![rec]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                StrictValidationError::UnknownComparator { value, .. } if value == "bogus"
            )),
            "expected UnknownComparator"
        );
    }

    #[test]
    fn unknown_owner_fails() {
        let mut rec = default_record("bad_owner");
        rec.owner = "bogus".to_string();
        let manifest = make_manifest(vec![rec]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                StrictValidationError::UnknownOwner { value, .. } if value == "bogus"
            )),
            "expected UnknownOwner"
        );
    }

    #[test]
    fn unknown_milestone_fails() {
        let mut rec = default_record("bad_ms");
        rec.milestone = "Z".to_string();
        let manifest = make_manifest(vec![rec]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                StrictValidationError::UnknownMilestone { value, .. } if value == "Z"
            )),
            "expected UnknownMilestone"
        );
    }

    #[test]
    fn drop_in_without_evidence_fails() {
        let mut rec = default_record("no_ev");
        rec.status = "drop_in".to_string();
        rec.evidence_refs = vec![];
        rec.test_refs = vec![];
        let manifest = make_manifest(vec![rec]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                StrictValidationError::DropInWithoutEvidence { id, .. } if id == "no_ev"
            )),
            "expected DropInWithoutEvidence"
        );
    }

    #[test]
    fn drop_in_without_oracle_probe_fails() {
        let mut rec = default_record("no_probe");
        rec.status = "drop_in".to_string();
        rec.oracle_probe = String::new();
        let manifest = make_manifest(vec![rec]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                StrictValidationError::DropInWithoutOracleProbe { id, .. } if id == "no_probe"
            )),
            "expected DropInWithoutOracleProbe"
        );
    }

    #[test]
    fn drop_in_with_evidence_passes() {
        let mut rec = default_record("with_ev");
        rec.status = "drop_in".to_string();
        rec.evidence_refs = vec!["some_evidence".to_string()];
        let manifest = make_manifest(vec![rec]);
        let result = validate_strict_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn drop_in_with_tests_passes() {
        let mut rec = default_record("with_tests");
        rec.status = "drop_in".to_string();
        rec.test_refs = vec!["some_test".to_string()];
        rec.evidence_refs = vec![];
        let manifest = make_manifest(vec![rec]);
        let result = validate_strict_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn gap_at_current_milestone_fails() {
        let mut rec = default_record("gap_ms");
        rec.status = "gap".to_string();
        rec.milestone = "A".to_string(); // A <= A (current)
        let manifest = make_manifest(vec![rec]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors.iter().any(|e| matches!(
                e,
                StrictValidationError::UnresolvedProgress { id, status, .. }
                    if id == "gap_ms" && status == "gap"
            )),
            "expected UnresolvedProgress for gap at milestone A"
        );
    }

    #[test]
    fn drop_in_at_current_milestone_passes() {
        let mut rec = default_record("di_ms");
        rec.status = "drop_in".to_string();
        rec.milestone = "A".to_string();
        let manifest = make_manifest(vec![rec]);
        let result = validate_strict_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn gap_at_future_milestone_passes() {
        let mut rec = default_record("gap_future");
        rec.status = "gap".to_string();
        rec.milestone = "E".to_string(); // E > C (current)
        let manifest = make_manifest(vec![rec]);
        let result = validate_strict_manifest(&manifest);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn multiple_errors_collected() {
        let mut rec = default_record("dup");
        rec.category = "bogus".to_string();
        let manifest = make_manifest(vec![rec, default_record("dup")]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.len() >= 2,
            "expected at least 2 errors, got {}",
            errs.len()
        );
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, StrictValidationError::DuplicateId { .. })));
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, StrictValidationError::UnknownCategory { .. })));
    }

    #[test]
    fn all_allowed_categories_are_valid() {
        for cat in ALLOWED_CATEGORIES {
            let mut rec = default_record(&format!("cat_{cat}"));
            rec.category = cat.to_string();
            rec.status = "gap".to_string();
            rec.milestone = "F".to_string(); // future milestone to avoid unresolved
            let manifest = make_manifest(vec![rec]);
            let errs = validate_strict_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, StrictValidationError::UnknownCategory { .. })),
                    "category \"{cat}\" should be valid"
                );
            }
        }
    }

    #[test]
    fn all_allowed_statuses_are_valid() {
        for status in ALLOWED_STATUSES {
            let mut rec = default_record(&format!("status_{status}"));
            rec.status = status.to_string();
            rec.milestone = "F".to_string();
            let manifest = make_manifest(vec![rec]);
            let errs = validate_strict_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, StrictValidationError::UnknownStatus { .. })),
                    "status \"{status}\" should be valid"
                );
            }
        }
    }

    #[test]
    fn all_allowed_comparators_are_valid() {
        for comp in ALLOWED_COMPARATORS {
            let mut rec = default_record(&format!("comp_{comp}"));
            rec.comparator = comp.to_string();
            rec.status = "gap".to_string();
            rec.milestone = "F".to_string();
            let manifest = make_manifest(vec![rec]);
            let errs = validate_strict_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, StrictValidationError::UnknownComparator { .. })),
                    "comparator \"{comp}\" should be valid"
                );
            }
        }
    }

    #[test]
    fn all_allowed_owners_are_valid() {
        for owner in ALLOWED_OWNERS {
            let mut rec = default_record(&format!("owner_{owner}"));
            rec.owner = owner.to_string();
            rec.status = "gap".to_string();
            rec.milestone = "F".to_string();
            let manifest = make_manifest(vec![rec]);
            let errs = validate_strict_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, StrictValidationError::UnknownOwner { .. })),
                    "owner \"{owner}\" should be valid"
                );
            }
        }
    }

    #[test]
    fn all_allowed_milestones_are_valid() {
        for ms in ALLOWED_MILESTONES {
            let mut rec = default_record(&format!("ms_{ms}"));
            rec.milestone = ms.to_string();
            rec.status = "gap".to_string();
            let manifest = make_manifest(vec![rec]);
            let errs = validate_strict_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, StrictValidationError::UnknownMilestone { .. })),
                    "milestone \"{ms}\" should be valid"
                );
            }
        }
    }

    #[test]
    fn validation_errors_collection() {
        let mut errs = StrictValidationErrors::new();
        assert!(errs.is_empty());
        assert_eq!(errs.len(), 0);
        errs.push(StrictValidationError::EmptyId);
        errs.push(StrictValidationError::DuplicateId {
            id: "a".to_string(),
        });
        assert!(!errs.is_empty());
        assert_eq!(errs.len(), 2);
    }

    #[test]
    fn validate_strict_manifest_file_missing_path() {
        let path = Path::new("/nonexistent/path/strict_manifest.toml");
        let result = validate_strict_manifest_file(path);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs
            .errors
            .iter()
            .any(|e| matches!(e, StrictValidationError::Io { .. })));
    }

    #[test]
    fn toml_parse_error() {
        let bad_toml = "this is not [valid toml {{{{";
        let result: Result<StrictManifest, _> = toml::from_str(bad_toml);
        assert!(result.is_err());
    }

    #[test]
    fn validate_real_strict_manifest() {
        let path = match find_strict_manifest_path() {
            Some(p) => p,
            None => {
                eprintln!("strict manifest not found, skipping");
                return;
            }
        };
        eprintln!("Validating strict manifest at: {}", path.display());
        match validate_strict_manifest_file(&path) {
            Ok(manifest) => {
                eprintln!(
                    "Strict manifest OK: {} records, meta.schema={}",
                    manifest.record.len(),
                    manifest.meta.schema
                );
            }
            Err(errs) => {
                eprintln!(
                    "Strict manifest validation FAILED with {} errors:",
                    errs.len()
                );
                for (i, err) in errs.errors.iter().enumerate() {
                    eprintln!("  ERROR {}: {}", i + 1, err);
                }
                panic!(
                    "strict manifest validation failed with {} errors (see above)",
                    errs.len()
                );
            }
        }
    }

    #[test]
    fn milestone_index_ordering() {
        assert!(milestone_index("A") < milestone_index("B"));
        assert!(milestone_index("B") < milestone_index("C"));
        assert!(milestone_index("C") < milestone_index("D"));
        assert!(milestone_index("D") < milestone_index("E"));
        assert!(milestone_index("E") < milestone_index("F"));
    }

    #[test]
    fn terminal_statuses_not_flagged_at_current_milestone() {
        for status in TERMINAL_STATUSES {
            let mut rec = default_record(&format!("term_{status}"));
            rec.status = status.to_string();
            rec.milestone = "B".to_string();
            let manifest = make_manifest(vec![rec]);
            let errs = validate_strict_manifest(&manifest);
            if let Err(ref e) = errs {
                assert!(
                    !e.errors
                        .iter()
                        .any(|e| matches!(e, StrictValidationError::UnresolvedProgress { .. })),
                    "terminal status \"{status}\" should not be flagged at current milestone"
                );
            }
        }
    }

    #[test]
    fn gap_at_milestone_a_also_flagged() {
        let mut rec = default_record("gap_a");
        rec.status = "gap".to_string();
        rec.milestone = "A".to_string();
        let manifest = make_manifest(vec![rec]);
        let errs = validate_strict_manifest(&manifest).unwrap_err();
        assert!(
            errs.errors
                .iter()
                .any(|e| matches!(e, StrictValidationError::UnresolvedProgress { .. })),
            "gap at milestone A should be flagged"
        );
    }

    // -- Oracle provenance verification tests --

    #[test]
    fn verify_manifest_oracle_version_matches() {
        let manifest = make_manifest(vec![]);
        let provenance = OracleProvenance {
            oracle: OracleProvenanceInner {
                package: "pproxy".to_string(),
                version: "2.7.9".to_string(),
                source: "pypi".to_string(),
                license: "MIT".to_string(),
                retrieval_date: "2026-07-16".to_string(),
            },
        };
        assert!(verify_manifest_oracle_version(&manifest, &provenance).is_ok());
    }

    #[test]
    fn verify_manifest_oracle_version_mismatch() {
        let mut meta = make_meta();
        meta.pproxy_version = "2.7.8".to_string();
        let manifest = StrictManifest {
            meta,
            record: vec![],
        };
        let provenance = OracleProvenance {
            oracle: OracleProvenanceInner {
                package: "pproxy".to_string(),
                version: "2.7.9".to_string(),
                source: "pypi".to_string(),
                license: "MIT".to_string(),
                retrieval_date: "2026-07-16".to_string(),
            },
        };
        let err = verify_manifest_oracle_version(&manifest, &provenance).unwrap_err();
        assert!(matches!(err, OracleError::VersionMismatch { .. }));
    }

    #[test]
    fn verify_manifest_oracle_package_mismatch() {
        let manifest = make_manifest(vec![]);
        let provenance = OracleProvenance {
            oracle: OracleProvenanceInner {
                package: "not_pproxy".to_string(),
                version: "2.7.9".to_string(),
                source: "pypi".to_string(),
                license: "MIT".to_string(),
                retrieval_date: "2026-07-16".to_string(),
            },
        };
        let err = verify_manifest_oracle_version(&manifest, &provenance).unwrap_err();
        assert!(matches!(err, OracleError::PackageMismatch { .. }));
    }

    #[test]
    fn load_oracle_hashes_from_workspace() {
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let path = PathBuf::from(&manifest_dir);
            match load_oracle_hashes(&path) {
                Ok(hashes) => {
                    assert_eq!(hashes.package.version, "2.7.9");
                    assert!(hashes.package.sha256_wheel.is_some());
                    let wheel_hash = hashes.package.sha256_wheel.unwrap();
                    assert_ne!(wheel_hash, "placeholder_update_on_first_run");
                    assert_eq!(wheel_hash.len(), 64); // SHA256 hex = 64 chars
                }
                Err(e) => {
                    eprintln!("load_oracle_hashes: {}", e);
                }
            }
        }
    }

    #[test]
    fn load_oracle_provenance_from_workspace() {
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let path = PathBuf::from(&manifest_dir);
            match load_oracle_provenance(&path) {
                Ok(prov) => {
                    assert_eq!(prov.oracle.package, "pproxy");
                    assert_eq!(prov.oracle.version, "2.7.9");
                    assert_eq!(prov.oracle.source, "pypi");
                }
                Err(e) => {
                    eprintln!("load_oracle_provenance: {}", e);
                }
            }
        }
    }

    #[test]
    fn manifest_version_matches_provenance() {
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let path = PathBuf::from(&manifest_dir);
            let hashes = load_oracle_hashes(&path);
            let provenance = load_oracle_provenance(&path);
            if let (Ok(h), Ok(p)) = (hashes, provenance) {
                assert_eq!(h.package.version, p.oracle.version);
            }
        }
    }

    // -- Report generation tests --

    #[test]
    fn summarize_manifest_basic() {
        let mut r1 = default_record("a");
        r1.status = "drop_in".to_string();
        r1.category = "protocol".to_string();
        r1.owner = "track-b".to_string();
        let mut r2 = default_record("b");
        r2.status = "gap".to_string();
        r2.category = "python_namespace".to_string();
        r2.owner = "track-a".to_string();
        let manifest = make_manifest(vec![r1, r2]);
        let summary = summarize_manifest(&manifest);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.terminal_count, 1);
        assert_eq!(summary.gap_count, 1);
    }

    #[test]
    fn summarize_manifest_all_terminal() {
        let mut r1 = default_record("a");
        r1.status = "drop_in".to_string();
        let mut r2 = default_record("b");
        r2.status = "not_applicable".to_string();
        let manifest = make_manifest(vec![r1, r2]);
        let summary = summarize_manifest(&manifest);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.terminal_count, 2);
        assert_eq!(summary.gap_count, 0);
    }

    #[test]
    fn generate_strict_report_basic() {
        let mut r1 = default_record("a");
        r1.status = "drop_in".to_string();
        r1.category = "protocol".to_string();
        r1.notes = "Test note".to_string();
        let mut r2 = default_record("b");
        r2.status = "gap".to_string();
        r2.category = "python_namespace".to_string();
        let manifest = make_manifest(vec![r1, r2]);
        let report = generate_strict_report(&manifest);
        assert!(report.contains("# pproxy 2.7.9 Strict Compatibility Report"));
        assert!(report.contains("pproxy==2.7.9"));
        assert!(report.contains("Total records"));
        assert!(report.contains("Gap Records"));
        assert!(report.contains("Terminal Records"));
    }

    #[test]
    fn generate_strict_report_empty() {
        let manifest = make_manifest(vec![]);
        let report = generate_strict_report(&manifest);
        assert!(report.contains("Total records"));
        assert!(report.contains("_No unresolved gaps._"));
        assert!(report.contains("_No terminal records._"));
    }

    #[test]
    fn write_strict_report_to_temp() {
        let manifest = make_manifest(vec![]);
        let dir = std::env::temp_dir().join("eggress_strict_report_test");
        let path = dir.join("report.md");
        let result = write_strict_report(&manifest, &path);
        assert!(result.is_ok());
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("pproxy 2.7.9 Strict Compatibility Report"));
        // cleanup
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn generate_strict_report_truncates_long_notes() {
        let mut rec = default_record("long_notes");
        rec.notes = "A".repeat(200);
        rec.status = "drop_in".to_string();
        let manifest = make_manifest(vec![rec]);
        let report = generate_strict_report(&manifest);
        assert!(report.contains("..."));
    }
}
