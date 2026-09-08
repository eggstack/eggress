use std::fmt;

use crate::diagnostics::StructuredDiagnostic;
use crate::issues::{CompatIssue, IssueSeverity};

/// A warning emitted during pproxy compatibility translation.
///
/// Legacy view over [`CompatIssue`]: [`TranslationOutput`] stores typed
/// issues and derives these on demand. The `category` tag round-trips
/// losslessly through the issue model.
#[derive(Debug, Clone, PartialEq)]
pub struct CompatWarning {
    /// Short category tag (e.g. "unsupported-scheme", "partial-behavior").
    pub category: &'static str,
    /// Human-readable message (credentials are redacted).
    pub message: String,
}

impl fmt::Display for CompatWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.category, self.message)
    }
}

/// An unsupported feature detected during translation.
///
/// Legacy view over [`CompatIssue`]: the `feature` tag round-trips
/// losslessly through the issue model.
#[derive(Debug, Clone, PartialEq)]
pub struct UnsupportedFeature {
    /// Feature name.
    pub feature: &'static str,
    /// Details about the input that triggered this.
    pub detail: String,
}

impl fmt::Display for UnsupportedFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported {}: {}", self.feature, self.detail)
    }
}

/// Result of translating pproxy-compatible input.
///
/// Stores one typed [`CompatIssue`] per finding. The legacy `warnings` /
/// `unsupported` collections are views (filters) over those issues, and the
/// human/JSON renderings derive from the same model, so classifications
/// cannot drift between outputs.
#[derive(Debug, Clone, Default)]
pub struct TranslationOutput {
    /// Generated Eggress TOML configuration.
    pub toml: String,
    /// Canonical typed findings; all other views derive from these.
    pub issues: Vec<CompatIssue>,
}

impl TranslationOutput {
    pub fn new(toml: String) -> Self {
        Self {
            toml,
            issues: Vec::new(),
        }
    }

    pub fn with_warning(mut self, category: &'static str, message: impl Into<String>) -> Self {
        self.issues
            .push(CompatIssue::warning_category(category, message));
        self
    }

    pub fn with_unsupported(mut self, feature: &'static str, detail: impl Into<String>) -> Self {
        self.issues
            .push(CompatIssue::unsupported_feature(feature, detail));
        self
    }

    pub fn with_warnings(mut self, warnings: Vec<CompatWarning>) -> Self {
        self.issues
            .extend(warnings.into_iter().map(CompatIssue::warning));
        self
    }

    pub fn with_unsupported_features(mut self, features: Vec<UnsupportedFeature>) -> Self {
        self.issues
            .extend(features.into_iter().map(CompatIssue::unsupported));
        self
    }

    /// Append already-typed issues (e.g. merging a sub-translation).
    pub fn with_issues(mut self, issues: Vec<CompatIssue>) -> Self {
        self.issues.extend(issues);
        self
    }

    /// Legacy warning view: all `Warning`-severity issues, in order.
    pub fn warnings(&self) -> Vec<CompatWarning> {
        self.issues.iter().filter_map(|i| i.to_warning()).collect()
    }

    /// Legacy unsupported view: all `Unsupported`-severity issues, in order.
    pub fn unsupported(&self) -> Vec<UnsupportedFeature> {
        self.issues
            .iter()
            .filter_map(|i| i.to_unsupported())
            .collect()
    }

    /// Structured diagnostics view for JSON/human rendering.
    pub fn diagnostics(&self) -> Vec<StructuredDiagnostic> {
        self.issues.iter().map(|i| i.to_diagnostic()).collect()
    }

    /// Whether any blocking (`Unsupported`) issue is present.
    pub fn has_unsupported(&self) -> bool {
        self.issues
            .iter()
            .any(|i| i.severity == IssueSeverity::Unsupported)
    }

    pub fn warnings_to_string(&self) -> String {
        let mut out = String::new();
        for w in self.warnings() {
            out.push_str(&format!("{w}\n"));
        }
        for u in self.unsupported() {
            out.push_str(&format!("{u}\n"));
        }
        out
    }
}
