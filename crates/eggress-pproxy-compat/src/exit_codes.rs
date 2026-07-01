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
