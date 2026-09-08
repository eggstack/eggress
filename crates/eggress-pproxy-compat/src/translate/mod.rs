//! pproxy-to-eggress translation, split by semantic area.
//!
//! - [`entry`]: argument-level entry points (TOML and combined TOML+native).
//! - [`intermediates`]: the shared semantic builder (args/URIs to
//!   renderer-neutral intermediates).
//! - [`model`]: TOML-serializable structs shared by builder and renderers.
//! - [`rules`]: rule-pattern and rule-file translation.
//! - [`toml_render`]: presentation-only TOML rendering.
//! - [`native`]: native compilation without a TOML round trip.

pub(crate) mod entry;
pub(crate) mod intermediates;
pub(crate) mod model;
pub(crate) mod native;
pub(crate) mod rules;
#[cfg(test)]
mod tests;
pub(crate) mod toml_render;

pub use entry::CombinedTranslation;
pub use entry::{translate_from_uris, translate_pproxy_args, translate_pproxy_args_to_native};
pub use native::{compile_chain_to_native, translate_to_runtime_config, NativeTranslation};
