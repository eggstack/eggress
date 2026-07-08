pub mod args;
pub mod diagnose;
pub mod diagnostics;
pub mod error;
pub mod exit_codes;
pub mod tier;
pub mod translate;
pub mod uri;
pub mod warnings;

pub use args::PproxyArgs;
pub use diagnostics::{DiagnosticCode, StructuredDiagnostic};
pub use error::CompatError;
pub use tier::{classify_aggregate_tier, manifest_tier_for_category, ManifestTier};
pub use translate::{translate_from_uris, translate_pproxy_args};
pub use uri::{PproxyChain, PproxyUri};
pub use warnings::{CompatWarning, TranslationOutput, UnsupportedFeature};

#[cfg(test)]
mod tests;
