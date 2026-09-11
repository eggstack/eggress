//! `eggress version`: deterministic version rendering.

use eggress_cli::EXIT_SUCCESS;

/// Print `eggress X.Y.Z` and return 0.
///
/// Single version source: the built package version. No runtime
/// initialization, config loading, or network access; output contains no
/// timestamps or nondeterministic state.
pub fn handle_version() -> i32 {
    println!("eggress {}", env!("CARGO_PKG_VERSION"));
    EXIT_SUCCESS
}
