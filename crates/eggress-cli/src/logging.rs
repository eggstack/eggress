//! Log format/value policy for both CLI binaries.
//!
//! The only decisions made here are which tracing-subscriber formatter to
//! install and which default filter to use. No command logic lives here.

use tracing_subscriber::{fmt, EnvFilter};

use crate::cli::LogFormat;

/// Install process-wide logging with an explicit [`LogFormat`].
/// An explicit `RUST_LOG` environment value remains authoritative.
pub fn init_logging(format: LogFormat) {
    let builder = fmt().with_env_filter(
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
    );

    match format {
        LogFormat::Json => builder.json().init(),
        LogFormat::Compact => builder.compact().init(),
        LogFormat::Pretty => builder.pretty().init(),
    }
}

#[cfg(feature = "pproxy-compat")]
/// Install logging for compatibility execution.
///
/// Compatibility verbosity/debug defaults come from the parsed pproxy flags
/// (`-d`/`-v` via [`eggress_pproxy_compat::PproxyArgs::default_log_level`]).
/// An explicit `RUST_LOG` remains authoritative.
pub fn init_pproxy_logging(pproxy_args: &eggress_pproxy_compat::PproxyArgs, format: LogFormat) {
    let builder = fmt().with_env_filter(
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(pproxy_args.default_log_level())),
    );

    match format {
        LogFormat::Json => builder.json().init(),
        // Compatibility output historically defaults to the compact
        // formatter; preserve that for both `compact` and `pretty` requests.
        LogFormat::Compact | LogFormat::Pretty => builder.compact().init(),
    }
}
