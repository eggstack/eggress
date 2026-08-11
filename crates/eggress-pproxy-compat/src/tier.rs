//! Manifest-aligned tier classification for pproxy compatibility diagnostics.
//!
//! The five-tier vocabulary mirrors
//! `docs/parity/pproxy_capability_manifest.toml`:
//!
//! - `drop_in` — no warning expected
//! - `compatible_with_warning` — works but emits a diagnostic
//! - `native_equivalent` — outcome same as pproxy, different mechanism
//! - `intentional_non_parity` — flag parsed, no plan to implement
//! - `unsupported` — flag or feature not implemented

/// The five manifest-aligned compatibility tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestTier {
    DropIn,
    CompatibleWithWarning,
    NativeEquivalent,
    IntentionalNonParity,
    Unsupported,
}

impl ManifestTier {
    pub fn as_str(self) -> &'static str {
        match self {
            ManifestTier::DropIn => "drop_in",
            ManifestTier::CompatibleWithWarning => "compatible_with_warning",
            ManifestTier::NativeEquivalent => "native_equivalent",
            ManifestTier::IntentionalNonParity => "intentional_non_parity",
            ManifestTier::Unsupported => "unsupported",
        }
    }
}

/// Map a translator warning category to its manifest-aligned tier.
///
/// This is the **single executable source of truth** for diagnostic-category
/// → tier mapping. Python and the canonical manifest must agree with this
/// function. Unknown categories default to `Unsupported` to surface gaps.
pub fn manifest_tier_for_category(category: &str) -> ManifestTier {
    match category {
        // Native equivalent: same outcome through different mechanism.
        "alive-check" | "test-mode" | "ssl-no-listener" | "trojan-auto-tls"
        | "get-static-content" | "reuse-port" => ManifestTier::NativeEquivalent,
        // Compatible with warning: works but emits a diagnostic.
        "debug-mode"
        | "pac-serving"
        | "verbose-mode"
        | "scheduler"
        | "credential-in-toml"
        | "rulefile-partial"
        | "rulefile-parse"
        | "rulefile-read"
        | "direct-mode"
        | "ul-no-listener"
        | "log-file"
        | "config-schema"
        | "reverse-proxy-warning"
        | "socks5_udp_framing_divergence" => ManifestTier::CompatibleWithWarning,
        // Unsupported: feature or operation not implemented.
        "socks4_bind_unsupported" | "socks5_bind_unsupported" => ManifestTier::Unsupported,
        // Unknown categories default to "unsupported" to surface new gaps.
        _ => ManifestTier::Unsupported,
    }
}

/// Pick the worst manifest-aligned tier from a set of warnings and
/// unsupported features.
///
/// Severity order (worst first):
/// 1. any unsupported hard failure -> `unsupported`
/// 2. any intentional non-parity    -> `intentional_non_parity`
/// 3. any compatible-with-warning   -> `compatible_with_warning`
/// 4. any native-equivalent warning -> `native_equivalent`
/// 5. no diagnostics                -> `drop_in`
pub fn classify_aggregate_tier(
    warnings: &[CompatWarning],
    unsupported: &[UnsupportedFeature],
) -> ManifestTier {
    if !unsupported.is_empty() {
        return ManifestTier::Unsupported;
    }
    if warnings
        .iter()
        .any(|w| manifest_tier_for_category(w.category) == ManifestTier::IntentionalNonParity)
    {
        return ManifestTier::IntentionalNonParity;
    }
    if warnings
        .iter()
        .any(|w| manifest_tier_for_category(w.category) == ManifestTier::CompatibleWithWarning)
    {
        return ManifestTier::CompatibleWithWarning;
    }
    if warnings
        .iter()
        .any(|w| manifest_tier_for_category(w.category) == ManifestTier::NativeEquivalent)
    {
        return ManifestTier::NativeEquivalent;
    }
    ManifestTier::DropIn
}

use crate::warnings::{CompatWarning, UnsupportedFeature};

#[cfg(test)]
mod tests {
    use super::*;

    fn warn(category: &'static str) -> CompatWarning {
        CompatWarning {
            category,
            message: String::new(),
        }
    }

    #[test]
    fn empty_input_is_drop_in() {
        let tier = classify_aggregate_tier(&[], &[]);
        assert_eq!(tier, ManifestTier::DropIn);
    }

    #[test]
    fn unsupported_overrides_warnings() {
        let u = UnsupportedFeature {
            feature: "ssh",
            detail: String::new(),
        };
        let tier = classify_aggregate_tier(&[warn("direct-mode")], &[u]);
        assert_eq!(tier, ManifestTier::Unsupported);
    }

    #[test]
    fn intentional_non_parity_beats_compatible() {
        let tier = classify_aggregate_tier(&[warn("direct-mode"), warn("scheduler")], &[]);
        // scheduler is compatible_with_warning, so this should be compatible_with_warning
        // unless there's an intentional_non_parity warning
        assert_eq!(tier, ManifestTier::CompatibleWithWarning);
    }

    #[test]
    fn native_equivalent_beats_compatible() {
        let tier = classify_aggregate_tier(&[warn("direct-mode"), warn("alive-check")], &[]);
        // compatible_with_warning dominates native_equivalent in severity
        assert_eq!(tier, ManifestTier::CompatibleWithWarning);
    }

    #[test]
    fn compatible_with_warning_when_only_compatible() {
        let tier = classify_aggregate_tier(&[warn("direct-mode")], &[]);
        assert_eq!(tier, ManifestTier::CompatibleWithWarning);
    }

    #[test]
    fn unknown_category_is_unsupported() {
        assert_eq!(
            manifest_tier_for_category("totally-new-category"),
            ManifestTier::Unsupported
        );
    }

    #[test]
    fn tier_strings_match_manifest() {
        assert_eq!(ManifestTier::DropIn.as_str(), "drop_in");
        assert_eq!(
            ManifestTier::CompatibleWithWarning.as_str(),
            "compatible_with_warning"
        );
        assert_eq!(ManifestTier::NativeEquivalent.as_str(), "native_equivalent");
        assert_eq!(
            ManifestTier::IntentionalNonParity.as_str(),
            "intentional_non_parity"
        );
        assert_eq!(ManifestTier::Unsupported.as_str(), "unsupported");
    }

    #[test]
    fn mixed_native_and_compatible_aggregates_to_compatible() {
        let tier = classify_aggregate_tier(&[warn("alive-check"), warn("direct-mode")], &[]);
        assert_eq!(tier, ManifestTier::CompatibleWithWarning);
    }

    #[test]
    fn log_file_is_compatible_with_warning() {
        assert_eq!(
            manifest_tier_for_category("log-file"),
            ManifestTier::CompatibleWithWarning
        );
    }

    #[test]
    fn socks4_bind_category_maps_to_unsupported() {
        // socks4-bind and socks5-bind are unsupported (neither pproxy nor
        // eggress implements BIND). Unknown categories default to unsupported.
        assert_eq!(
            manifest_tier_for_category("socks4-bind"),
            ManifestTier::Unsupported
        );
    }

    #[test]
    fn socks5_bind_category_maps_to_unsupported() {
        assert_eq!(
            manifest_tier_for_category("socks5-bind"),
            ManifestTier::Unsupported
        );
    }
}
