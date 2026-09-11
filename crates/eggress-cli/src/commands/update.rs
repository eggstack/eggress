//! `eggress update` command adapter.
//!
//! The self-update mechanics live in [`crate::update`]; this adapter only
//! connects the parsed (flagless) invocation to that implementation so the
//! command surface stays small: no channel selectors, force/downgrade
//! flags, or update configuration files.

use crate::cli::UpdateArgs;

/// Run `eggress update`. Returns the process exit code.
pub fn handle_update(_args: &UpdateArgs) -> i32 {
    crate::update::handle_update()
}
