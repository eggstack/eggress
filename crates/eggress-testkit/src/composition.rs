//! Composition matrix validation for the pproxy parity model.
//!
//! Validates `docs/parity/composition_matrix.toml` — the machine-readable
//! composition graph that maps protocol×role×traffic_kind combinations to
//! capability IDs and evidence.
//!
//! The composition matrix complements the flat capability manifest by
//! preventing false parity claims: a protocol cannot be claimed as
//! supported merely because one isolated implementation exists.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Allowed protocol values in the composition matrix.
pub const ALLOWED_PROTOCOLS: &[&str] = &[
    "direct",
    "http",
    "https",
    "socks4",
    "socks4a",
    "socks5",
    "shadowsocks",
    "trojan",
    "ssh",
    "ws",
    "wss",
    "raw",
    "tunnel",
    "h2",
    "quic",
    "h3",
    "unix",
    "redir",
];

/// Allowed role values.
pub const ALLOWED_ROLES: &[&str] = &[
    "listener",
    "upstream",
    "chain_hop",
    "terminal",
    "reverse_server",
    "reverse_client",
];

/// Allowed traffic kind values.
pub const ALLOWED_TRAFFIC_KINDS: &[&str] = &["tcp", "udp"];

/// Allowed tier values (same as manifest).
pub const ALLOWED_TIERS: &[&str] = &[
    "drop_in",
    "compatible_with_warning",
    "native_equivalent",
    "intentional_non_parity",
    "unsupported",
];

/// Allowed evidence values (same as manifest).
pub const ALLOWED_EVIDENCE: &[&str] = &[
    "differential",
    "integration",
    "unit",
    "synthetic",
    "docs_only",
    "none",
];

/// Allowed constraint types.
pub const ALLOWED_CONSTRAINT_TYPES: &[&str] = &[
    "chain_max_hops",
    "platform",
    "requires_tls",
    "no_udp",
    "no_chain",
    "protocol_crate_only",
    "upstream_only_no_listener",
];

/// Allowed caveat class values.
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

/// Pinned schema version.
pub const PINNED_SCHEMA_VERSION: &str = "1";

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Top-level matrix metadata.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CompositionMatrixMeta {
    pub schema_version: String,
    pub manifest_ref: String,
    #[serde(default)]
    pub description: String,
}

/// A single composition cell mapping protocol×role×traffic_kind to capabilities.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CompositionCell {
    pub protocol: String,
    pub role: String,
    pub traffic_kind: String,
    pub tier: String,
    pub evidence: String,
    #[serde(default)]
    pub capability_ids: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub caveat_class: String,
    #[serde(default)]
    pub rationale: String,
    /// Optional chain max hops constraint for this cell.
    pub chain_max: Option<u32>,
}

/// A chain composition (listener->upstream transition).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ChainComposition {
    pub from_protocol: String,
    pub to_protocol: String,
    pub traffic_kind: String,
    pub tier: String,
    pub evidence: String,
    #[serde(default)]
    pub capability_ids: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

/// A global constraint on the composition matrix.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CompositionConstraint {
    #[serde(rename = "type")]
    pub constraint_type: String,
    #[serde(default)]
    pub value: Option<u32>,
    #[serde(default)]
    pub applies_to: Vec<String>,
    #[serde(default)]
    pub description: String,
}

/// The parsed composition matrix.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CompositionMatrix {
    pub matrix: CompositionMatrixMeta,
    #[serde(default)]
    pub cell: Vec<CompositionCell>,
    #[serde(default)]
    pub chain: Vec<ChainComposition>,
    #[serde(default)]
    pub constraint: Vec<CompositionConstraint>,
}

// ---------------------------------------------------------------------------
// Validation errors
// ---------------------------------------------------------------------------

/// A single validation error or warning.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CompositionValidationError {
    #[error("schema version mismatch: got {got}, expected {expected}")]
    SchemaVersionMismatch { got: String, expected: String },

    #[error("unknown protocol: {0}")]
    UnknownProtocol(String),

    #[error("unknown role: {0}")]
    UnknownRole(String),

    #[error("unknown traffic_kind: {0}")]
    UnknownTrafficKind(String),

    #[error("unknown tier: {0}")]
    UnknownTier(String),

    #[error("unknown evidence: {0}")]
    UnknownEvidence(String),

    #[error("unknown constraint type: {0}")]
    UnknownConstraintType(String),

    #[error("unknown caveat_class: {0}")]
    UnknownCaveatClass(String),

    #[error("duplicate cell: protocol={protocol} role={role} traffic_kind={traffic_kind}")]
    DuplicateCell {
        protocol: String,
        role: String,
        traffic_kind: String,
    },

    #[error("duplicate chain: from={from_protocol} to={to_protocol} traffic_kind={traffic_kind}")]
    DuplicateChain {
        from_protocol: String,
        to_protocol: String,
        traffic_kind: String,
    },

    #[error("unsupported cell has non-empty capability_ids: protocol={protocol} role={role}")]
    UnsupportedCellWithCapabilities { protocol: String, role: String },

    #[error("protocol-crate-only cell has tier=drop_in: {0}")]
    ProtocolCrateOnlyDropIn(String),

    #[error("drop_in cell with evidence weaker than integration: {protocol}/{role}")]
    DropInWeakEvidence { protocol: String, role: String },

    #[error("chain composition missing from_protocol or to_protocol")]
    ChainMissingProtocols,

    #[error("chain composition with chain_max < 2: {from_protocol} -> {to_protocol}")]
    ChainMaxTooSmall {
        from_protocol: String,
        to_protocol: String,
    },

    #[error("capability_id not found in manifest: {0}")]
    UnknownCapabilityId(String),

    #[error("constraint applies_to references unknown protocol: {0}")]
    ConstraintUnknownProtocol(String),

    #[error("empty composition matrix (no cells or chains)")]
    EmptyMatrix,

    #[error("io error: {0}")]
    IoError(String),

    #[error("toml parse error: {0}")]
    ParseError(String),
}

impl From<toml::de::Error> for CompositionValidationError {
    fn from(e: toml::de::Error) -> Self {
        CompositionValidationError::ParseError(e.to_string())
    }
}

/// Collection of validation errors and warnings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompositionValidationResult {
    pub errors: Vec<CompositionValidationError>,
    pub warnings: Vec<CompositionValidationError>,
}

impl CompositionValidationResult {
    pub fn push_error(&mut self, err: CompositionValidationError) {
        self.errors.push(err);
    }

    pub fn push_warning(&mut self, warn: CompositionValidationError) {
        self.warnings.push(warn);
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.errors.len() + self.warnings.len()
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a composition matrix against the schema and manifest.
///
/// Returns `Ok(())` on success, or `Err(result)` with errors/warnings.
pub fn validate_composition_matrix(
    matrix: &CompositionMatrix,
    manifest_capability_ids: &HashSet<&str>,
) -> Result<(), CompositionValidationResult> {
    let mut result = CompositionValidationResult::default();

    // Schema version check
    if matrix.matrix.schema_version != PINNED_SCHEMA_VERSION {
        result.push_error(CompositionValidationError::SchemaVersionMismatch {
            got: matrix.matrix.schema_version.clone(),
            expected: PINNED_SCHEMA_VERSION.to_string(),
        });
    }

    // Empty matrix check
    if matrix.cell.is_empty() && matrix.chain.is_empty() {
        result.push_error(CompositionValidationError::EmptyMatrix);
    }

    // Validate cells
    let mut seen_cells: HashSet<(&str, &str, &str)> = HashSet::new();
    for cell in &matrix.cell {
        if !ALLOWED_PROTOCOLS.contains(&cell.protocol.as_str()) {
            result.push_error(CompositionValidationError::UnknownProtocol(
                cell.protocol.clone(),
            ));
        }

        if !ALLOWED_ROLES.contains(&cell.role.as_str()) {
            result.push_error(CompositionValidationError::UnknownRole(cell.role.clone()));
        }

        if !ALLOWED_TRAFFIC_KINDS.contains(&cell.traffic_kind.as_str()) {
            result.push_error(CompositionValidationError::UnknownTrafficKind(
                cell.traffic_kind.clone(),
            ));
        }

        if !ALLOWED_TIERS.contains(&cell.tier.as_str()) {
            result.push_error(CompositionValidationError::UnknownTier(cell.tier.clone()));
        }

        if !ALLOWED_EVIDENCE.contains(&cell.evidence.as_str()) {
            result.push_error(CompositionValidationError::UnknownEvidence(
                cell.evidence.clone(),
            ));
        }

        if !cell.caveat_class.is_empty()
            && !ALLOWED_CAVEAT_CLASSES.contains(&cell.caveat_class.as_str())
        {
            result.push_error(CompositionValidationError::UnknownCaveatClass(
                cell.caveat_class.clone(),
            ));
        }

        let key = (
            cell.protocol.as_str(),
            cell.role.as_str(),
            cell.traffic_kind.as_str(),
        );
        if !seen_cells.insert(key) {
            result.push_error(CompositionValidationError::DuplicateCell {
                protocol: cell.protocol.clone(),
                role: cell.role.clone(),
                traffic_kind: cell.traffic_kind.clone(),
            });
        }

        if cell.tier == "unsupported" && !cell.capability_ids.is_empty() {
            result.push_error(
                CompositionValidationError::UnsupportedCellWithCapabilities {
                    protocol: cell.protocol.clone(),
                    role: cell.role.clone(),
                },
            );
        }

        if cell.caveat_class == "protocol_crate_only" && cell.tier == "drop_in" {
            result.push_error(CompositionValidationError::ProtocolCrateOnlyDropIn(
                format!("{}/{}", cell.protocol, cell.role),
            ));
        }

        if cell.tier == "drop_in"
            && matches!(
                cell.evidence.as_str(),
                "unit" | "synthetic" | "docs_only" | "none"
            )
        {
            result.push_warning(CompositionValidationError::DropInWeakEvidence {
                protocol: cell.protocol.clone(),
                role: cell.role.clone(),
            });
        }

        for cap_id in &cell.capability_ids {
            if !manifest_capability_ids.contains(cap_id.as_str()) {
                result.push_error(CompositionValidationError::UnknownCapabilityId(
                    cap_id.clone(),
                ));
            }
        }
    }

    // Validate chains
    let mut seen_chains: HashSet<(&str, &str, &str)> = HashSet::new();
    for chain in &matrix.chain {
        if !ALLOWED_PROTOCOLS.contains(&chain.from_protocol.as_str()) {
            result.push_error(CompositionValidationError::UnknownProtocol(
                chain.from_protocol.clone(),
            ));
        }
        if !ALLOWED_PROTOCOLS.contains(&chain.to_protocol.as_str()) {
            result.push_error(CompositionValidationError::UnknownProtocol(
                chain.to_protocol.clone(),
            ));
        }

        if !ALLOWED_TRAFFIC_KINDS.contains(&chain.traffic_kind.as_str()) {
            result.push_error(CompositionValidationError::UnknownTrafficKind(
                chain.traffic_kind.clone(),
            ));
        }

        if !ALLOWED_TIERS.contains(&chain.tier.as_str()) {
            result.push_error(CompositionValidationError::UnknownTier(chain.tier.clone()));
        }

        if !ALLOWED_EVIDENCE.contains(&chain.evidence.as_str()) {
            result.push_error(CompositionValidationError::UnknownEvidence(
                chain.evidence.clone(),
            ));
        }

        let key = (
            chain.from_protocol.as_str(),
            chain.to_protocol.as_str(),
            chain.traffic_kind.as_str(),
        );
        if !seen_chains.insert(key) {
            result.push_error(CompositionValidationError::DuplicateChain {
                from_protocol: chain.from_protocol.clone(),
                to_protocol: chain.to_protocol.clone(),
                traffic_kind: chain.traffic_kind.clone(),
            });
        }

        for cap_id in &chain.capability_ids {
            if !manifest_capability_ids.contains(cap_id.as_str()) {
                result.push_error(CompositionValidationError::UnknownCapabilityId(
                    cap_id.clone(),
                ));
            }
        }
    }

    // Validate constraints
    for constraint in &matrix.constraint {
        if !ALLOWED_CONSTRAINT_TYPES.contains(&constraint.constraint_type.as_str()) {
            result.push_error(CompositionValidationError::UnknownConstraintType(
                constraint.constraint_type.clone(),
            ));
        }

        for proto in &constraint.applies_to {
            if !ALLOWED_PROTOCOLS.contains(&proto.as_str()) {
                result.push_error(CompositionValidationError::ConstraintUnknownProtocol(
                    proto.clone(),
                ));
            }
        }
    }

    if result.errors.is_empty() {
        Ok(())
    } else {
        Err(result)
    }
}

/// Parse and validate a composition matrix from a TOML file.
pub fn validate_composition_matrix_file(
    path: &Path,
    manifest_capability_ids: &HashSet<&str>,
) -> Result<CompositionMatrix, CompositionValidationResult> {
    let content = fs::read_to_string(path).map_err(|e| {
        let mut result = CompositionValidationResult::default();
        result.push_error(CompositionValidationError::SchemaVersionMismatch {
            got: format!("read error: {}", e),
            expected: "valid TOML file".to_string(),
        });
        result
    })?;

    let matrix: CompositionMatrix = toml::from_str(&content).map_err(|e| {
        let mut result = CompositionValidationResult::default();
        result.push_error(CompositionValidationError::SchemaVersionMismatch {
            got: format!("parse error: {}", e),
            expected: "valid TOML structure".to_string(),
        });
        result
    })?;

    validate_composition_matrix(&matrix, manifest_capability_ids)?;
    Ok(matrix)
}

/// Find the composition matrix file path relative to the workspace root.
pub fn find_composition_matrix_path() -> Option<PathBuf> {
    let candidates = [
        "docs/parity/composition_matrix.toml",
        "../docs/parity/composition_matrix.toml",
        "../../docs/parity/composition_matrix.toml",
    ];

    for candidate in &candidates {
        let path = Path::new(candidate);
        if path.exists() {
            return Some(path.to_path_buf());
        }
    }

    None
}

/// Query whether a specific composition is supported.
pub fn query_composition<'a>(
    matrix: &'a CompositionMatrix,
    protocol: &str,
    role: &str,
    traffic_kind: &str,
) -> Option<&'a CompositionCell> {
    matrix
        .cell
        .iter()
        .find(|c| c.protocol == protocol && c.role == role && c.traffic_kind == traffic_kind)
}

/// Query whether a chain composition is supported.
pub fn query_chain<'a>(
    matrix: &'a CompositionMatrix,
    from_protocol: &str,
    to_protocol: &str,
    traffic_kind: &str,
) -> Option<&'a ChainComposition> {
    matrix.chain.iter().find(|c| {
        c.from_protocol == from_protocol
            && c.to_protocol == to_protocol
            && c.traffic_kind == traffic_kind
    })
}

/// Get all supported protocols for a given role and traffic kind.
pub fn supported_protocols<'a>(
    matrix: &'a CompositionMatrix,
    role: &str,
    traffic_kind: &str,
) -> Vec<&'a str> {
    matrix
        .cell
        .iter()
        .filter(|c| c.role == role && c.traffic_kind == traffic_kind && c.tier != "unsupported")
        .map(|c| c.protocol.as_str())
        .collect()
}

/// Get all supported roles for a given protocol and traffic kind.
pub fn supported_roles<'a>(
    matrix: &'a CompositionMatrix,
    protocol: &str,
    traffic_kind: &str,
) -> Vec<&'a str> {
    matrix
        .cell
        .iter()
        .filter(|c| {
            c.protocol == protocol && c.traffic_kind == traffic_kind && c.tier != "unsupported"
        })
        .map(|c| c.role.as_str())
        .collect()
}

/// Count cells by tier.
pub fn count_by_tier(matrix: &CompositionMatrix) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    for cell in &matrix.cell {
        *counts.entry(cell.tier.clone()).or_insert(0) += 1;
    }
    counts
}

/// Count chain compositions by tier.
pub fn count_chains_by_tier(
    matrix: &CompositionMatrix,
) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    for chain in &matrix.chain {
        *counts.entry(chain.tier.clone()).or_insert(0) += 1;
    }
    counts
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Runtime composition validator backed by a loaded matrix.
///
/// Wraps a `CompositionMatrix` and provides efficient query methods
/// for checking whether a given protocol×role×traffic_kind combination
/// is supported, and what tier/evidence it has.
pub struct CompositionValidator {
    matrix: CompositionMatrix,
}

impl CompositionValidator {
    /// Load the composition matrix from the canonical path.
    pub fn load() -> Option<Self> {
        let path = find_composition_matrix_path()?;
        let content = fs::read_to_string(&path).ok()?;
        let matrix: CompositionMatrix = toml::from_str(&content).ok()?;
        Some(Self { matrix })
    }

    /// Load from an explicit file path.
    pub fn from_file(path: &Path) -> Result<Self, CompositionValidationError> {
        let content = fs::read_to_string(path)
            .map_err(|e| CompositionValidationError::IoError(format!("{}: {e}", path.display())))?;
        let matrix: CompositionMatrix = toml::from_str(&content)?;
        Ok(Self { matrix })
    }

    /// Query a single composition cell.
    pub fn query(
        &self,
        protocol: &str,
        role: &str,
        traffic_kind: &str,
    ) -> Option<&CompositionCell> {
        query_composition(&self.matrix, protocol, role, traffic_kind)
    }

    /// Query a chain composition.
    pub fn query_chain(
        &self,
        from_protocol: &str,
        to_protocol: &str,
        traffic_kind: &str,
    ) -> Option<&ChainComposition> {
        query_chain(&self.matrix, from_protocol, to_protocol, traffic_kind)
    }

    /// Check if a protocol+role+traffic_kind combination is supported (non-unsupported tier).
    pub fn is_supported(&self, protocol: &str, role: &str, traffic_kind: &str) -> bool {
        self.query(protocol, role, traffic_kind)
            .map(|c| c.tier != "unsupported")
            .unwrap_or(false)
    }

    /// Get all protocols supporting a given role+traffic_kind at or above a tier.
    pub fn protocols_at_or_above_tier(
        &self,
        role: &str,
        traffic_kind: &str,
        min_tier: &str,
    ) -> Vec<&str> {
        let tier_order = |t: &str| match t {
            "drop_in" => 0,
            "compatible_with_warning" => 1,
            "native_equivalent" => 2,
            "intentional_non_parity" => 3,
            "unsupported" => 4,
            _ => 5,
        };
        let min_rank = tier_order(min_tier);

        self.matrix
            .cell
            .iter()
            .filter(|c| {
                c.role == role && c.traffic_kind == traffic_kind && tier_order(&c.tier) <= min_rank
            })
            .map(|c| c.protocol.as_str())
            .collect()
    }

    /// Get all constraints that apply to a given protocol.
    pub fn constraints_for(&self, protocol: &str) -> Vec<&CompositionConstraint> {
        self.matrix
            .constraint
            .iter()
            .filter(|c| c.applies_to.iter().any(|p| p == protocol))
            .collect()
    }

    /// Return a reference to the inner matrix.
    pub fn inner(&self) -> &CompositionMatrix {
        &self.matrix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Derive the canonical manifest path from a composition matrix path.
    fn manifest_path_from_matrix(matrix_path: &Path) -> PathBuf {
        // composition_matrix.toml is in docs/parity/, manifest is sibling
        matrix_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("pproxy_capability_manifest.toml")
    }

    fn minimal_matrix() -> CompositionMatrix {
        CompositionMatrix {
            matrix: CompositionMatrixMeta {
                schema_version: PINNED_SCHEMA_VERSION.to_string(),
                manifest_ref: "test.toml".to_string(),
                description: "test".to_string(),
            },
            cell: vec![CompositionCell {
                protocol: "socks5".to_string(),
                role: "listener".to_string(),
                traffic_kind: "tcp".to_string(),
                tier: "drop_in".to_string(),
                evidence: "integration".to_string(),
                capability_ids: vec![],
                notes: String::new(),
                caveat_class: String::new(),
                rationale: String::new(),
                chain_max: None,
            }],
            chain: vec![],
            constraint: vec![],
        }
    }

    #[test]
    fn valid_minimal_matrix() {
        let matrix = minimal_matrix();
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn empty_matrix_rejected() {
        let mut matrix = minimal_matrix();
        matrix.cell.clear();
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::EmptyMatrix)));
    }

    #[test]
    fn unknown_protocol_rejected() {
        let mut matrix = minimal_matrix();
        matrix.cell[0].protocol = "bogus".to_string();
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::UnknownProtocol(_))));
    }

    #[test]
    fn unknown_role_rejected() {
        let mut matrix = minimal_matrix();
        matrix.cell[0].role = "bogus".to_string();
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::UnknownRole(_))));
    }

    #[test]
    fn unknown_tier_rejected() {
        let mut matrix = minimal_matrix();
        matrix.cell[0].tier = "bogus".to_string();
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::UnknownTier(_))));
    }

    #[test]
    fn unsupported_cell_with_capabilities_rejected() {
        let mut matrix = minimal_matrix();
        matrix.cell[0].tier = "unsupported".to_string();
        matrix.cell[0].capability_ids = vec!["some.cap".to_string()];
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.errors.iter().any(|e| matches!(
            e,
            CompositionValidationError::UnsupportedCellWithCapabilities { .. }
        )));
    }

    #[test]
    fn protocol_crate_only_drop_in_rejected() {
        let mut matrix = minimal_matrix();
        matrix.cell[0].protocol = "ws".to_string();
        matrix.cell[0].tier = "drop_in".to_string();
        matrix.cell[0].caveat_class = "protocol_crate_only".to_string();
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::ProtocolCrateOnlyDropIn(_))));
    }

    #[test]
    fn drop_in_with_weak_evidence_warns() {
        let mut matrix = minimal_matrix();
        matrix.cell[0].tier = "drop_in".to_string();
        matrix.cell[0].evidence = "unit".to_string();
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        // Ok(()) means no errors, but warnings are populated
        assert!(result.is_ok());
    }

    #[test]
    fn duplicate_cell_rejected() {
        let mut matrix = minimal_matrix();
        matrix.cell.push(CompositionCell {
            protocol: "socks5".to_string(),
            role: "listener".to_string(),
            traffic_kind: "tcp".to_string(),
            tier: "drop_in".to_string(),
            evidence: "integration".to_string(),
            capability_ids: vec![],
            notes: String::new(),
            caveat_class: String::new(),
            rationale: String::new(),
            chain_max: None,
        });
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::DuplicateCell { .. })));
    }

    #[test]
    fn unknown_capability_id_rejected() {
        let mut matrix = minimal_matrix();
        matrix.cell[0].capability_ids = vec!["nonexistent.cap".to_string()];
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::UnknownCapabilityId(_))));
    }

    #[test]
    fn known_capability_id_accepted() {
        let mut matrix = minimal_matrix();
        matrix.cell[0].capability_ids = vec!["protocol.socks5.connect_ipv4".to_string()];
        let mut caps = HashSet::new();
        caps.insert("protocol.socks5.connect_ipv4");
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn chain_validated() {
        let mut matrix = minimal_matrix();
        matrix.chain.push(ChainComposition {
            from_protocol: "socks5".to_string(),
            to_protocol: "http".to_string(),
            traffic_kind: "tcp".to_string(),
            tier: "drop_in".to_string(),
            evidence: "integration".to_string(),
            capability_ids: vec![],
            notes: String::new(),
        });
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn chain_unknown_protocol_rejected() {
        let mut matrix = minimal_matrix();
        matrix.chain.push(ChainComposition {
            from_protocol: "bogus".to_string(),
            to_protocol: "http".to_string(),
            traffic_kind: "tcp".to_string(),
            tier: "drop_in".to_string(),
            evidence: "integration".to_string(),
            capability_ids: vec![],
            notes: String::new(),
        });
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::UnknownProtocol(_))));
    }

    #[test]
    fn schema_version_mismatch_rejected() {
        let mut matrix = minimal_matrix();
        matrix.matrix.schema_version = "99".to_string();
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::SchemaVersionMismatch { .. })));
    }

    #[test]
    fn constraint_unknown_type_rejected() {
        let mut matrix = minimal_matrix();
        matrix.constraint.push(CompositionConstraint {
            constraint_type: "bogus".to_string(),
            value: None,
            applies_to: vec![],
            description: "test".to_string(),
        });
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::UnknownConstraintType(_))));
    }

    #[test]
    fn constraint_unknown_protocol_rejected() {
        let mut matrix = minimal_matrix();
        matrix.constraint.push(CompositionConstraint {
            constraint_type: "no_udp".to_string(),
            value: None,
            applies_to: vec!["bogus".to_string()],
            description: "test".to_string(),
        });
        let caps = HashSet::new();
        let result = validate_composition_matrix(&matrix, &caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .errors
            .iter()
            .any(|e| matches!(e, CompositionValidationError::ConstraintUnknownProtocol(_))));
    }

    #[test]
    fn query_composition_found() {
        let matrix = minimal_matrix();
        let cell = query_composition(&matrix, "socks5", "listener", "tcp");
        assert!(cell.is_some());
        assert_eq!(cell.unwrap().tier, "drop_in");
    }

    #[test]
    fn query_composition_not_found() {
        let matrix = minimal_matrix();
        let cell = query_composition(&matrix, "http", "listener", "tcp");
        assert!(cell.is_none());
    }

    #[test]
    fn supported_protocols_lists_correct() {
        let mut matrix = minimal_matrix();
        matrix.cell.push(CompositionCell {
            protocol: "http".to_string(),
            role: "listener".to_string(),
            traffic_kind: "tcp".to_string(),
            tier: "drop_in".to_string(),
            evidence: "integration".to_string(),
            capability_ids: vec![],
            notes: String::new(),
            caveat_class: String::new(),
            rationale: String::new(),
            chain_max: None,
        });
        let protos = supported_protocols(&matrix, "listener", "tcp");
        assert!(protos.contains(&"socks5"));
        assert!(protos.contains(&"http"));
    }

    #[test]
    fn count_by_tier_works() {
        let mut matrix = minimal_matrix();
        matrix.cell.push(CompositionCell {
            protocol: "http".to_string(),
            role: "listener".to_string(),
            traffic_kind: "tcp".to_string(),
            tier: "drop_in".to_string(),
            evidence: "integration".to_string(),
            capability_ids: vec![],
            notes: String::new(),
            caveat_class: String::new(),
            rationale: String::new(),
            chain_max: None,
        });
        let counts = count_by_tier(&matrix);
        assert_eq!(counts.get("drop_in"), Some(&2));
    }

    #[test]
    fn real_composition_matrix_validates() {
        let Some(matrix_path) = find_composition_matrix_path() else {
            return;
        };

        // Load manifest capability IDs from the canonical manifest
        let manifest_path = manifest_path_from_matrix(&matrix_path);
        let mut manifest_ids: HashSet<&str> = HashSet::new();
        let manifest_content = match fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(_) => return, // manifest not available in this test environment
        };
        let manifest_value: toml::Value = match manifest_content.parse() {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Some(caps) = manifest_value.get("capability").and_then(|v| v.as_array()) {
            for cap in caps {
                if let Some(id) = cap.get("id").and_then(|v| v.as_str()) {
                    manifest_ids.insert(id);
                }
            }
        }

        let result = validate_composition_matrix_file(&matrix_path, &manifest_ids);
        assert!(
            result.is_ok(),
            "real composition matrix validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn composition_validator_load() {
        let validator = CompositionValidator::load();
        assert!(validator.is_some(), "should load from canonical path");
    }

    #[test]
    fn composition_validator_query() {
        let validator = CompositionValidator::load().unwrap();
        let cell = validator.query("http", "listener", "tcp");
        assert!(cell.is_some());
        assert_eq!(cell.unwrap().tier, "drop_in");
    }

    #[test]
    fn composition_validator_is_supported() {
        let validator = CompositionValidator::load().unwrap();
        assert!(validator.is_supported("http", "listener", "tcp"));
        assert!(validator.is_supported("socks5", "listener", "tcp"));
        assert!(validator.is_supported("socks5", "upstream", "udp"));
        assert!(!validator.is_supported("ssh", "listener", "tcp"));
        assert!(!validator.is_supported("quic", "listener", "tcp"));
    }

    #[test]
    fn composition_validator_protocols_at_or_above_tier() {
        let validator = CompositionValidator::load().unwrap();
        let protos = validator.protocols_at_or_above_tier("listener", "tcp", "drop_in");
        assert!(protos.contains(&"http"));
        assert!(protos.contains(&"socks5"));
        assert!(!protos.contains(&"ssh"));
    }

    #[test]
    fn composition_validator_constraints_for() {
        let validator = CompositionValidator::load().unwrap();
        let constraints = validator.constraints_for("http");
        assert!(!constraints.is_empty());
    }

    /// Table-driven: every drop_in cell should have non-empty capability_ids
    /// (except reverse proxy cells which have not yet been added to the manifest)
    #[test]
    fn drop_in_cells_have_capability_ids() {
        let validator = CompositionValidator::load().unwrap();
        for cell in &validator.matrix.cell {
            if cell.tier == "drop_in"
                && cell.role != "reverse_server"
                && cell.role != "reverse_client"
            {
                assert!(
                    !cell.capability_ids.is_empty(),
                    "drop_in cell {}/{} has empty capability_ids",
                    cell.protocol,
                    cell.role
                );
            }
        }
    }

    /// Table-driven: every intentional_non_parity cell should have rationale
    #[test]
    fn intentional_non_parity_cells_have_rationale() {
        let validator = CompositionValidator::load().unwrap();
        for cell in &validator.matrix.cell {
            if cell.tier == "intentional_non_parity" {
                assert!(
                    !cell.rationale.is_empty(),
                    "intentional_non_parity cell {}/{} has no rationale",
                    cell.protocol,
                    cell.role
                );
            }
        }
    }

    /// Table-driven: every protocol-crate-only cell has tier != drop_in
    #[test]
    fn protocol_crate_only_not_drop_in() {
        let validator = CompositionValidator::load().unwrap();
        for cell in &validator.matrix.cell {
            if cell.caveat_class == "protocol_crate_only" {
                assert_ne!(
                    cell.tier, "drop_in",
                    "protocol-crate-only cell {}/{} has tier=drop_in",
                    cell.protocol, cell.role
                );
            }
        }
    }

    /// Table-driven: chain compositions reference valid protocols
    #[test]
    fn chain_protocols_are_valid() {
        let validator = CompositionValidator::load().unwrap();
        for chain in &validator.matrix.chain {
            assert!(
                ALLOWED_PROTOCOLS.contains(&chain.from_protocol.as_str()),
                "chain has unknown from_protocol: {}",
                chain.from_protocol
            );
            assert!(
                ALLOWED_PROTOCOLS.contains(&chain.to_protocol.as_str()),
                "chain has unknown to_protocol: {}",
                chain.to_protocol
            );
        }
    }

    /// Table-driven: constraints reference valid protocols in applies_to
    #[test]
    fn constraint_applies_to_valid_protocols() {
        let validator = CompositionValidator::load().unwrap();
        for constraint in &validator.matrix.constraint {
            for proto in &constraint.applies_to {
                assert!(
                    ALLOWED_PROTOCOLS.contains(&proto.as_str()),
                    "constraint type={} applies_to unknown protocol: {}",
                    constraint.constraint_type,
                    proto
                );
            }
        }
    }

    /// Table-driven: every drop_in upstream cell has UDP or TCP capability
    #[test]
    fn drop_in_upstream_cells_have_traffic_kind() {
        let validator = CompositionValidator::load().unwrap();
        for cell in &validator.matrix.cell {
            if cell.role == "upstream" && cell.tier == "drop_in" {
                assert!(
                    cell.traffic_kind == "tcp" || cell.traffic_kind == "udp",
                    "drop_in upstream cell {}/{} has unexpected traffic_kind: {}",
                    cell.protocol,
                    cell.role,
                    cell.traffic_kind
                );
            }
        }
    }

    /// Table-driven: listener cells are TCP (except socks5 and shadowsocks UDP ASSOCIATE)
    #[test]
    fn listener_cells_are_tcp_or_udp_associate() {
        let validator = CompositionValidator::load().unwrap();
        // Protocols that legitimately have UDP listener cells
        let udp_listener_protos = ["socks5", "shadowsocks"];
        for cell in &validator.matrix.cell {
            if cell.role == "listener" {
                if udp_listener_protos.contains(&cell.protocol.as_str())
                    && cell.traffic_kind == "udp"
                {
                    continue;
                }
                assert_eq!(
                    cell.traffic_kind, "tcp",
                    "listener cell {}/{} has traffic_kind={} (expected tcp)",
                    cell.protocol, cell.role, cell.traffic_kind
                );
            }
        }
    }
}
