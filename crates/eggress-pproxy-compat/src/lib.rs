pub mod args;
pub mod diagnose;
pub mod diagnostics;
pub mod error;
pub mod exit_codes;
pub mod gate;
pub mod issues;
pub mod regex_compat;
pub mod tier;
pub mod translate;
pub mod uri;
pub mod warnings;

pub use args::PproxyArgs;
pub use diagnostics::{
    classify_unsupported_feature_code, classify_unsupported_feature_tier, DiagnosticCode,
    StructuredDiagnostic,
};
pub use error::CompatError;
pub use gate::{evaluate as evaluate_execution_gate, BlockReason, ExecutionGate};
pub use issues::{CompatIssue, IssueSeverity};
pub use regex_compat::{CompatRegex, PproxyRuleFile, RegexBackend, RegexCompileError};
pub use tier::{classify_aggregate_tier, manifest_tier_for_category, ManifestTier};
pub use translate::{
    compile_chain_to_native, translate_from_uris, translate_pproxy_args,
    translate_pproxy_args_to_native, translate_to_runtime_config, CombinedTranslation,
    NativeTranslation,
};
pub use uri::{PproxyChain, PproxyPluginSpec, PproxyUri};
pub use warnings::{CompatWarning, TranslationOutput, UnsupportedFeature};

#[cfg(test)]
mod tests;
