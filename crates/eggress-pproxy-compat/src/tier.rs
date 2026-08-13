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

/// Map an unsupported feature id to its manifest-aligned tier.
///
/// This is the **single executable source of truth** for unsupported-feature
/// classification. It reuses the per-diagnostic tier owned by
/// [`crate::diagnostics::classify_unsupported_feature_tier`] so the
/// aggregate classifier and the per-diagnostic reporter never disagree.
pub fn manifest_tier_for_unsupported_feature(feature: &'static str) -> ManifestTier {
    match crate::diagnostics::classify_unsupported_feature_tier(feature) {
        "drop_in" => ManifestTier::DropIn,
        "compatible_with_warning" => ManifestTier::CompatibleWithWarning,
        "native_equivalent" => ManifestTier::NativeEquivalent,
        "intentional_non_parity" => ManifestTier::IntentionalNonParity,
        // Any unknown feature id (and the default "unsupported" branch
        // already covers it) must fail closed to `Unsupported`.
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
///
/// The aggregate classifier consults the native per-diagnostic tier of
/// every unsupported feature id, so a known intentional exclusion
/// (e.g. SSH, SSR, legacy cipher) reports as `intentional_non_parity`
/// rather than being collapsed into generic `unsupported`. Unknown
/// unsupported feature ids and unknown warning categories fail closed
/// to `Unsupported`.
pub fn classify_aggregate_tier(
    warnings: &[CompatWarning],
    unsupported: &[UnsupportedFeature],
) -> ManifestTier {
    // Any unsupported feature whose native tier is `unsupported` (including
    // unknown feature ids) collapses the aggregate to `unsupported`.
    if unsupported
        .iter()
        .any(|u| manifest_tier_for_unsupported_feature(u.feature) == ManifestTier::Unsupported)
    {
        return ManifestTier::Unsupported;
    }
    // Any unknown warning category also fails closed to `unsupported`,
    // because `manifest_tier_for_category` defaults to `Unsupported` for
    // categories the classifier does not recognize.
    if warnings
        .iter()
        .any(|w| manifest_tier_for_category(w.category) == ManifestTier::Unsupported)
    {
        return ManifestTier::Unsupported;
    }
    if unsupported.iter().any(|u| {
        manifest_tier_for_unsupported_feature(u.feature) == ManifestTier::IntentionalNonParity
    }) || warnings
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

    fn unsupported(feature: &'static str) -> UnsupportedFeature {
        UnsupportedFeature {
            feature,
            detail: String::new(),
        }
    }

    #[test]
    fn empty_input_is_drop_in() {
        let tier = classify_aggregate_tier(&[], &[]);
        assert_eq!(tier, ManifestTier::DropIn);
    }

    #[test]
    fn native_equivalent_warning_only() {
        let tier = classify_aggregate_tier(&[warn("alive-check")], &[]);
        assert_eq!(tier, ManifestTier::NativeEquivalent);
    }

    #[test]
    fn compatible_warning_only() {
        let tier = classify_aggregate_tier(&[warn("direct-mode")], &[]);
        assert_eq!(tier, ManifestTier::CompatibleWithWarning);
    }

    #[test]
    fn compatible_with_warning_dominates_native_equivalent() {
        // The native_equivalent + compatible_with_warning case is the
        // exact regression: a material compatibility warning must NOT be
        // hidden by a better `native_equivalent` result.
        let tier = classify_aggregate_tier(&[warn("alive-check"), warn("direct-mode")], &[]);
        assert_eq!(tier, ManifestTier::CompatibleWithWarning);
    }

    #[test]
    fn intentional_non_parity_unsupported_feature_only() {
        // SSH, SSR, and legacy-cipher are intentional non-parity. With no
        // harder feature present, the aggregate must be intentional_non_parity
        // and NOT generic unsupported.
        for feature in [
            "ssh-listener",
            "ssh-upstream",
            "ssr-listener",
            "ssr-upstream",
            "legacy-cipher",
        ] {
            let tier = classify_aggregate_tier(&[], &[unsupported(feature)]);
            assert_eq!(
                tier,
                ManifestTier::IntentionalNonParity,
                "expected intentional_non_parity for {feature}, got {tier:?}"
            );
        }
    }

    #[test]
    fn intentional_non_parity_dominates_compatible_warning() {
        let tier = classify_aggregate_tier(
            &[],
            &[unsupported("ssh-listener"), unsupported("ssh-upstream")],
        );
        // Add a compatible warning
        let tier_with_warning =
            classify_aggregate_tier(&[warn("direct-mode")], &[unsupported("ssh-listener")]);
        assert_eq!(tier, ManifestTier::IntentionalNonParity);
        assert_eq!(tier_with_warning, ManifestTier::IntentionalNonParity);
    }

    #[test]
    fn hard_unsupported_dominates_intentional_non_parity() {
        let tier =
            classify_aggregate_tier(&[], &[unsupported("ssh-listener"), unsupported("daemon")]);
        assert_eq!(tier, ManifestTier::Unsupported);
    }

    #[test]
    fn hard_unsupported_dominates_compatible_warning() {
        let tier = classify_aggregate_tier(&[warn("direct-mode")], &[unsupported("daemon")]);
        assert_eq!(tier, ManifestTier::Unsupported);
    }

    #[test]
    fn unknown_warning_category_is_unsupported() {
        let tier = classify_aggregate_tier(&[warn("totally-new-category")], &[]);
        assert_eq!(tier, ManifestTier::Unsupported);
    }

    #[test]
    fn unknown_unsupported_feature_id_is_unsupported() {
        // An unknown unsupported feature id must fail closed to `unsupported`.
        let tier = classify_aggregate_tier(&[], &[unsupported("made-up-feature")]);
        assert_eq!(tier, ManifestTier::Unsupported);
    }

    #[test]
    fn socks4_bind_unsupported_aggregates_to_unsupported() {
        // `socks4-bind` is a generic unsupported feature, not an
        // intentional exclusion, so the aggregate is `unsupported`.
        let tier = classify_aggregate_tier(&[], &[unsupported("socks4-bind")]);
        assert_eq!(tier, ManifestTier::Unsupported);
    }

    #[test]
    fn socks5_bind_unsupported_aggregates_to_unsupported() {
        let tier = classify_aggregate_tier(&[], &[unsupported("socks5-bind")]);
        assert_eq!(tier, ManifestTier::Unsupported);
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
    fn log_file_is_compatible_with_warning() {
        assert_eq!(
            manifest_tier_for_category("log-file"),
            ManifestTier::CompatibleWithWarning
        );
    }

    #[test]
    fn socks4_bind_category_maps_to_unsupported() {
        // socks4-bind and socks5-bind categories are unsupported. Unknown
        // categories also default to unsupported to surface new gaps.
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

    #[test]
    fn manifest_tier_for_unsupported_feature_uses_native_classification() {
        assert_eq!(
            manifest_tier_for_unsupported_feature("ssh-listener"),
            ManifestTier::IntentionalNonParity
        );
        assert_eq!(
            manifest_tier_for_unsupported_feature("ssr-upstream"),
            ManifestTier::IntentionalNonParity
        );
        assert_eq!(
            manifest_tier_for_unsupported_feature("legacy-cipher"),
            ManifestTier::IntentionalNonParity
        );
        assert_eq!(
            manifest_tier_for_unsupported_feature("daemon"),
            ManifestTier::Unsupported
        );
        assert_eq!(
            manifest_tier_for_unsupported_feature("made-up-feature"),
            ManifestTier::Unsupported
        );
    }
}
