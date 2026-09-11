//! Native `eggress` command implementations.
//!
//! Each module receives typed inputs ([`crate::cli::CliContext`] plus its
//! own argument struct), calls library/runtime APIs, and returns a numeric
//! process exit code. Argument parsing lives in [`crate::cli`], process
//! termination in [`crate::main`]; handlers in between never call
//! `process::exit` themselves.

pub mod pproxy;
pub mod route;
pub mod run;
pub mod system_proxy;
pub mod update;
pub mod upstream;
pub mod version;
