pub mod args;
pub mod diagnose;
pub mod error;
pub mod translate;
pub mod uri;
pub mod warnings;

pub use args::PproxyArgs;
pub use error::CompatError;
pub use translate::{translate_from_uris, translate_pproxy_args};
pub use uri::PproxyUri;
pub use warnings::{CompatWarning, TranslationOutput, UnsupportedFeature};

#[cfg(test)]
mod tests;
