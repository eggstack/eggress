//! Canonical typed compatibility issue model.
//!
//! [`CompatIssue`] is the single stored representation for every translation
//! warning, unsupported feature, and informational diagnostic produced by the
//! compatibility layer. It carries the severity disposition, the stable
//! [`DiagnosticCode`], the original string tag (warning category or feature
//! id) for lossless round-trips to the legacy view types, the manifest
//! feature/tier mapping where applicable, the redacted human-readable
//! message, and an optional remediation suggestion.
//!
//! Human text, JSON output ([`StructuredDiagnostic`]), warning collection
//! ([`CompatWarning`]), and CLI rendering are all views over this model:
//! [`TranslationOutput`] stores `issues` and derives the rest.

use crate::diagnostics::{DiagnosticCode, StructuredDiagnostic};
use crate::warnings::{CompatWarning, UnsupportedFeature};

/// Severity/disposition of a compatibility issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    /// Degraded but runnable behavior; startup proceeds.
    Warning,
    /// Blocking behavior; startup is refused by the execution gate.
    Unsupported,
    /// Informational note; no behavior impact.
    Info,
}

impl std::fmt::Display for IssueSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Warning => f.write_str("warning"),
            Self::Unsupported => f.write_str("unsupported"),
            Self::Info => f.write_str("info"),
        }
    }
}

/// One typed compatibility issue: the canonical stored diagnostic unit.
#[derive(Debug, Clone, PartialEq)]
pub struct CompatIssue {
    /// Warning (proceed) vs unsupported (block) vs info.
    pub severity: IssueSeverity,
    /// Stable diagnostic code used by JSON output, tests, and docs.
    pub code: DiagnosticCode,
    /// Original warning category tag (e.g. `"verbose-mode"`), if warning-born.
    pub category: Option<&'static str>,
    /// Original unsupported feature tag (e.g. `"daemon"`), if blocking.
    pub feature: Option<&'static str>,
    /// Manifest feature id, if this issue maps to a known feature.
    pub feature_id: Option<String>,
    /// Compatibility tier (`drop_in`, `compatible_with_warning`,
    /// `native_equivalent`, `intentional_non_parity`, `unsupported`).
    pub tier: Option<String>,
    /// Human-readable description (credentials are redacted at construction).
    pub message: String,
    /// Suggested eggress-native alternative, if applicable.
    pub suggestion: Option<String>,
}

impl CompatIssue {
    /// Classify a legacy warning into a typed issue (severity `Warning`).
    ///
    /// Classification funnels through the single
    /// `StructuredDiagnostic::from(&CompatWarning)` table so string-category
    /// dispatch exists in exactly one place.
    pub fn warning(warn: CompatWarning) -> Self {
        let category = warn.category;
        let diag = StructuredDiagnostic::from(&warn);
        Self {
            severity: IssueSeverity::Warning,
            code: diag.code,
            category: Some(category),
            feature: None,
            feature_id: diag.feature_id,
            tier: diag.tier,
            message: diag.message,
            suggestion: diag.suggestion,
        }
    }

    /// Classify a legacy unsupported feature into a typed issue (severity
    /// `Unsupported`).
    pub fn unsupported(item: UnsupportedFeature) -> Self {
        let feature = item.feature;
        let code = crate::diagnostics::classify_unsupported_feature_code(feature);
        let tier = crate::diagnostics::classify_unsupported_feature_tier(feature);
        // Reuse the canonical error-to-diagnostic mapping for message and
        // suggestion so all renderings agree.
        let diag = StructuredDiagnostic::from(crate::error::CompatError::unsupported(
            feature,
            item.detail.clone(),
        ));
        debug_assert_eq!(diag.code, code);
        Self {
            severity: IssueSeverity::Unsupported,
            code,
            category: None,
            feature: Some(feature),
            feature_id: Some(feature.to_string()),
            tier: Some(tier.to_string()),
            message: diag.message,
            suggestion: diag.suggestion,
        }
    }

    /// Convenience constructor for warning-category call sites.
    pub fn warning_category(category: &'static str, message: impl Into<String>) -> Self {
        Self::warning(CompatWarning {
            category,
            message: message.into(),
        })
    }

    /// Convenience constructor for unsupported-feature call sites.
    pub fn unsupported_feature(feature: &'static str, detail: impl Into<String>) -> Self {
        Self::unsupported(UnsupportedFeature {
            feature,
            detail: detail.into(),
        })
    }

    /// View as a legacy [`CompatWarning`] (lossless: category round-trips).
    pub fn to_warning(&self) -> Option<CompatWarning> {
        match self.severity {
            IssueSeverity::Warning => Some(CompatWarning {
                category: self.category.unwrap_or("general"),
                message: self.message.clone(),
            }),
            IssueSeverity::Unsupported | IssueSeverity::Info => None,
        }
    }

    /// View as a legacy [`UnsupportedFeature`] (lossless: feature round-trips).
    pub fn to_unsupported(&self) -> Option<UnsupportedFeature> {
        match self.severity {
            IssueSeverity::Unsupported => Some(UnsupportedFeature {
                feature: self.feature.unwrap_or("unknown"),
                detail: self.message.clone(),
            }),
            IssueSeverity::Warning | IssueSeverity::Info => None,
        }
    }

    /// View as a [`StructuredDiagnostic`] for JSON/human rendering.
    pub fn to_diagnostic(&self) -> StructuredDiagnostic {
        StructuredDiagnostic {
            code: self.code,
            feature_id: self.feature_id.clone(),
            tier: self.tier.clone(),
            message: self.message.clone(),
            suggestion: self.suggestion.clone(),
        }
    }
}

impl std::fmt::Display for CompatIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}] {}", self.severity, self.code, self.message)?;
        if let Some(ref tier) = self.tier {
            write!(f, " (tier: {})", tier)?;
        }
        if let Some(ref suggestion) = self.suggestion {
            write!(f, " — suggestion: {}", suggestion)?;
        }
        Ok(())
    }
}
