# Eggress CLI Exit Codes

## Reference

| Code | Constant | Name | Description |
|------|----------|------|-------------|
| 0 | `EXIT_SUCCESS` | success | Success or clean shutdown |
| 1 | `EXIT_RUNTIME_FAILURE` | runtime_failure | Generic runtime failure |
| 2 | `EXIT_CLI_PARSE_ERROR` | cli_parse_error | CLI argument parse error |
| 3 | `EXIT_CONFIG_VALIDATION` | config_validation | Config validation error |
| 4 | `EXIT_BIND_FAILURE` | bind_failure | Bind/listen startup failure |
| 5 | `EXIT_UNSUPPORTED_FEATURE` | unsupported_feature | Unsupported pproxy compatibility feature |
| 6 | `EXIT_PLATFORM_MISSING` | platform_missing | Platform capability missing |
| 7 | `EXIT_EXTERNAL_DEPENDENCY` | external_dependency | External dependency missing (for tests/diagnostics) |
| 130 | `EXIT_SIGINT` | interrupted_by_sigint | Interrupted by SIGINT |
| 143 | `EXIT_SIGTERM` | terminated_by_sigterm | Terminated by SIGTERM |

## Programmatic Access

All constants are defined in the `eggress_pproxy_compat::exit_codes` module and are available for use in Rust code:

```rust
use eggress_pproxy_compat::exit_codes::*;

// Use constants directly
std::process::exit(EXIT_CONFIG_VALIDATION);

// Map a code to its human-readable name
let name = exit_code_name(5); // returns "unsupported_feature"
```

The `exit_code_name()` function maps any code to its canonical name (returns `"unknown"` for unrecognized values).

## Usage in Subcommands

### `eggress pproxy translate`
- Exit 0: Translation successful, all features compatible or supported
- Exit 5: One or more features are unsupported
- Exit 2: Invalid pproxy arguments

### `eggress pproxy check`
- Always exits 0 (check only reports compatibility, doesn't fail)
- Use `--json` for machine-readable output with tier field

### `eggress pproxy run`
- Exit 0: Clean shutdown
- Exit 2: Invalid CLI arguments
- Exit 3: Invalid config generated from pproxy args
- Exit 4: Bind failure on listener
- Exit 5: Unsupported feature specified
- Exit 130: SIGINT received
- Exit 143: SIGTERM received

### `eggress` (default mode)
- Exit 0: Clean shutdown
- Exit 1: Runtime failure
- Exit 2: CLI argument parse error
- Exit 3: Config validation error
- Exit 4: Bind/listen failure
- Exit 130: SIGINT
- Exit 143: SIGTERM

## Comparison with pproxy

pproxy uses inconsistent exit codes:
- Exits 0 on success
- Exits 1 on most errors (config, bind, runtime)
- No differentiation between error types

Eggress provides more granular exit codes for better scripting and automation.
