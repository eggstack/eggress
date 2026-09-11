/// Stable process exit codes shared by the `eggress` and `pproxy` binaries.
/// This module is the single owner of the numeric process contract; the
/// `eggress-cli` crate re-exports these constants instead of defining its
/// own overlapping copies.
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_RUNTIME_FAILURE: i32 = 1;
pub const EXIT_CLI_PARSE_ERROR: i32 = 2;
pub const EXIT_CONFIG_VALIDATION: i32 = 3;
pub const EXIT_BIND_FAILURE: i32 = 4;
pub const EXIT_UNSUPPORTED_FEATURE: i32 = 5;
pub const EXIT_PLATFORM_MISSING: i32 = 6;
pub const EXIT_EXTERNAL_DEPENDENCY: i32 = 7;
pub const EXIT_SIGINT: i32 = 130;
pub const EXIT_SIGTERM: i32 = 143;

/// Typed process outcome. Prefer returning this from command handlers and
/// converting to a numeric code once at the process boundary over threading
/// raw `i32` constants through business logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExit {
    /// Clean run / check passed.
    Success,
    /// Runtime failure (supervisor, connection drain, unexpected I/O).
    RuntimeFailure,
    /// CLI/flag parse error (including strict-gate rejection of unknown flags
    /// and invalid values for closed option domains).
    CliParseError,
    /// Configuration validation failure (file, TOML, or translated config).
    ConfigValidation,
    /// Listener bind failure (address in use, permission denied).
    BindFailure,
    /// Unsupported feature / composition refused fail-closed.
    UnsupportedFeature,
    /// Required platform facility unavailable (e.g. Linux-only `--daemon`
    /// requested on another OS, prebuilt updater on an unsupported target).
    PlatformMissing,
    /// External dependency unavailable (e.g. `curl` missing for download,
    /// release metadata unreachable, checksum tool absent).
    ExternalDependency,
    /// Terminated by SIGINT (Ctrl-C).
    Sigint,
    /// Terminated by SIGTERM.
    Sigterm,
}

impl ProcessExit {
    /// Numeric exit code for this outcome.
    pub const fn code(self) -> i32 {
        match self {
            ProcessExit::Success => EXIT_SUCCESS,
            ProcessExit::RuntimeFailure => EXIT_RUNTIME_FAILURE,
            ProcessExit::CliParseError => EXIT_CLI_PARSE_ERROR,
            ProcessExit::ConfigValidation => EXIT_CONFIG_VALIDATION,
            ProcessExit::BindFailure => EXIT_BIND_FAILURE,
            ProcessExit::UnsupportedFeature => EXIT_UNSUPPORTED_FEATURE,
            ProcessExit::PlatformMissing => EXIT_PLATFORM_MISSING,
            ProcessExit::ExternalDependency => EXIT_EXTERNAL_DEPENDENCY,
            ProcessExit::Sigint => EXIT_SIGINT,
            ProcessExit::Sigterm => EXIT_SIGTERM,
        }
    }

    /// Stable snake_case name for this outcome (matches `exit_code_name`).
    pub const fn name(self) -> &'static str {
        match self {
            ProcessExit::Success => "success",
            ProcessExit::RuntimeFailure => "runtime_failure",
            ProcessExit::CliParseError => "cli_parse_error",
            ProcessExit::ConfigValidation => "config_validation",
            ProcessExit::BindFailure => "bind_failure",
            ProcessExit::UnsupportedFeature => "unsupported_feature",
            ProcessExit::PlatformMissing => "platform_missing",
            ProcessExit::ExternalDependency => "external_dependency",
            ProcessExit::Sigint => "interrupted_by_sigint",
            ProcessExit::Sigterm => "terminated_by_sigterm",
        }
    }
}

impl From<ProcessExit> for i32 {
    fn from(exit: ProcessExit) -> Self {
        exit.code()
    }
}

pub fn exit_code_name(code: i32) -> &'static str {
    match code {
        EXIT_SUCCESS => "success",
        EXIT_RUNTIME_FAILURE => "runtime_failure",
        EXIT_CLI_PARSE_ERROR => "cli_parse_error",
        EXIT_CONFIG_VALIDATION => "config_validation",
        EXIT_BIND_FAILURE => "bind_failure",
        EXIT_UNSUPPORTED_FEATURE => "unsupported_feature",
        EXIT_PLATFORM_MISSING => "platform_missing",
        EXIT_EXTERNAL_DEPENDENCY => "external_dependency",
        EXIT_SIGINT => "interrupted_by_sigint",
        EXIT_SIGTERM => "terminated_by_sigterm",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_outcomes_match_numeric_contract() {
        let cases = [
            (ProcessExit::Success, EXIT_SUCCESS),
            (ProcessExit::RuntimeFailure, EXIT_RUNTIME_FAILURE),
            (ProcessExit::CliParseError, EXIT_CLI_PARSE_ERROR),
            (ProcessExit::ConfigValidation, EXIT_CONFIG_VALIDATION),
            (ProcessExit::BindFailure, EXIT_BIND_FAILURE),
            (ProcessExit::UnsupportedFeature, EXIT_UNSUPPORTED_FEATURE),
            (ProcessExit::PlatformMissing, EXIT_PLATFORM_MISSING),
            (ProcessExit::ExternalDependency, EXIT_EXTERNAL_DEPENDENCY),
            (ProcessExit::Sigint, EXIT_SIGINT),
            (ProcessExit::Sigterm, EXIT_SIGTERM),
        ];
        for (outcome, code) in cases {
            assert_eq!(outcome.code(), code);
            assert_eq!(i32::from(outcome), code);
            assert_eq!(outcome.name(), exit_code_name(code));
        }
    }

    #[test]
    fn signal_exits_remain_stable() {
        assert_eq!(ProcessExit::Sigint.code(), 130);
        assert_eq!(ProcessExit::Sigterm.code(), 143);
    }
}
