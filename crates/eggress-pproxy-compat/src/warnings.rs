use std::fmt;

/// A warning emitted during pproxy compatibility translation.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug)]
pub struct TranslationOutput {
    /// Generated Eggress TOML configuration.
    pub toml: String,
    /// Warnings about partial or degraded behavior.
    pub warnings: Vec<CompatWarning>,
    /// Features that are explicitly unsupported.
    pub unsupported: Vec<UnsupportedFeature>,
}

impl TranslationOutput {
    pub fn new(toml: String) -> Self {
        Self {
            toml,
            warnings: Vec::new(),
            unsupported: Vec::new(),
        }
    }

    pub fn with_warning(mut self, category: &'static str, message: impl Into<String>) -> Self {
        self.warnings.push(CompatWarning {
            category,
            message: message.into(),
        });
        self
    }

    pub fn with_unsupported(mut self, feature: &'static str, detail: impl Into<String>) -> Self {
        self.unsupported.push(UnsupportedFeature {
            feature,
            detail: detail.into(),
        });
        self
    }

    pub fn with_warnings(mut self, warnings: Vec<CompatWarning>) -> Self {
        self.warnings.extend(warnings);
        self
    }

    pub fn with_unsupported_features(mut self, features: Vec<UnsupportedFeature>) -> Self {
        self.unsupported.extend(features);
        self
    }

    pub fn has_unsupported(&self) -> bool {
        !self.unsupported.is_empty()
    }

    pub fn warnings_to_string(&self) -> String {
        let mut out = String::new();
        for w in &self.warnings {
            out.push_str(&format!("{w}\n"));
        }
        for u in &self.unsupported {
            out.push_str(&format!("{u}\n"));
        }
        out
    }
}
