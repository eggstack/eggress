# Track A.02: CLI Drop-in Compatibility

## Objective

Make pproxy-style command-line use a first-class compatibility path. The current `eggress pproxy translate/check/run -- ...` flow is useful for migration, but full replacement requires a pproxy-shaped invocation path that behaves like pproxy for common commands and reports precise diagnostics for unsupported surfaces.

Track A should not implement every pproxy flag fully, but it must make the command shape, defaults, translation, diagnostics, and manifest reporting reliable.

## Required Compatibility Modes

Support two command shapes:

1. Native egress mode:

```text
eggress -l http://:8080 -r socks5://127.0.0.1:1080
```

2. pproxy compatibility mode:

```text
pproxy -l http://:8080 -r socks5://127.0.0.1:1080
```

The compatibility mode may be implemented as a separate binary target, install alias, Python console script, or wrapper that invokes egress's pproxy run path. The important behavior is that users can run pproxy-shaped commands without prefixing `eggress pproxy run --`.

## Default Behavior

pproxy's no-argument default is an autodetect listener on `:8080` for `http,socks4,socks5`, with direct upstream. Compatibility mode should match this behavior:

```text
pproxy
```

Expected result:

- bind `0.0.0.0:8080` or pproxy-equivalent default bind;
- accept HTTP CONNECT/forward, SOCKS4, SOCKS4a, and SOCKS5;
- route direct when no upstream is configured;
- print a pproxy-compatible startup line or a clearly documented equivalent.

If native egress default behavior differs, keep it separate from compatibility mode.

## CLI Flags To Normalize In Track A

Track A should handle or precisely diagnose:

- `-l`, `--listen`
- `-r`, `--remote`
- `-ul`, `--udp-listen`
- `-ur`, `--udp-remote`
- `-b`, `--block`
- `-a`, `--alive`
- `-s`, `--scheduler`
- `-v`, `-vv`, `-vvv`, `--verbose`
- `--ssl`
- `--pac`
- `--test`
- `--sys`
- `--get`
- `--log`
- `--version`
- `-h`, `--help`
- `--daemon`, `-d`
- `--reuse`

Do not let known pproxy flags fall through as unknown flags. If a flag is not implemented, emit a manifest-backed diagnostic with a stable code.

## Parser and Dispatcher Work

Add a compatibility CLI parser that preserves pproxy argument semantics before translating to egress config. It should support trailing positional/default behavior, repeated flags, combined verbosity flags, and pproxy's common short/long aliases.

The dispatcher should support:

- `pproxy` compatibility binary/alias;
- `eggress pproxy run -- ...` migration command;
- `eggress pproxy translate -- ...` config generation;
- `eggress pproxy check --json -- ...` machine-readable compatibility report;
- `eggress pproxy check -- ...` human-readable compatibility report.

## Diagnostics

Every unsupported or partially supported CLI surface should produce:

- stable diagnostic code;
- manifest capability ID;
- tier;
- clear message;
- suggested egress-native equivalent if any;
- whether the command can still run.

Examples:

- `daemon`: unsupported or native-equivalent via process manager.
- `reuse`: intentional non-parity until pooling exists.
- `get`: unsupported until implemented.
- `sys`: native-equivalent or compatible-with-warning depending on whether compatibility mode mutates global system proxy state.
- `rulefile`: compatible-with-warning until full pproxy rule semantics are implemented.

## Verbosity and Logs

Map `-v`, `-vv`, and `-vvv` to tracing levels, but in compatibility mode also provide pproxy-shaped connection logs where feasible. Avoid forcing users to know `RUST_LOG` for pproxy mode.

Track A requirements:

- `-v` enables connection-level logs.
- `-vv` enables debug-level compatibility logs.
- `-vvv` enables trace-level compatibility logs.
- `--log FILE` writes logs to file or emits a precise diagnostic if not implemented.

## Startup Output

pproxy users often rely on visible startup output. Add stable startup text in compatibility mode. It does not need to be byte-identical in Track A, but it should include:

- listener address;
- protocol set;
- upstream mode;
- warnings for compatibility divergences.

The oracle harness should validate that startup is detectable without relying on arbitrary sleep.

## `--test` Behavior

Track A should decide whether `--test URL` is implemented in compatibility mode or remains a precise diagnostic. The better target is to implement it as a wrapper over `eggress upstream test` so pproxy users can test remotes and exit.

Minimum Track A behavior:

- parse `--test`;
- generate config;
- run upstream probes if runtime support is ready;
- return nonzero on failed probes;
- include JSON-compatible result path through `pproxy check --json`.

## `--sys` Behavior

Native egress should remain safer than pproxy by default. Compatibility mode can still preserve pproxy semantics only when explicitly invoked through the `pproxy` alias or a compatibility flag.

Track A decision point:

- either implement `--sys` as explicit inspect/dry-run warning only, and classify as `native_equivalent`;
- or implement actual system proxy mutation in compatibility mode with safeguards and classify as `compatible_with_warning` or `drop_in` depending on behavior.

Record this decision in the manifest.

## Packaging Hook

If the Python package already builds wheels, add a console-script entry point plan for Track B:

```text
pproxy = eggress.pproxy_cli:main
```

For Rust binary releases, decide whether to ship a second binary name or document symlink creation.

## Tests

Add tests for:

- no-argument default translation/run config;
- mixed listener default `http+socks4+socks5://:8080`;
- repeated `-l` and `-r` flags;
- scheduler mapping `fa`, `rr`, `rc`, `lc`;
- `-v`, `-vv`, `-vvv` parsing;
- known unsupported flags are not unknown;
- JSON check output includes manifest tiers;
- compatibility startup output includes listener/protocol summary;
- exit code behavior for parse error, config validation error, unsupported feature, and runtime failure.

## Acceptance Criteria

- A pproxy-shaped command path exists.
- No-argument pproxy compatibility mode starts the default mixed listener or produces an explicitly tracked blocker.
- All known pproxy flags are parsed.
- Unsupported flags emit stable diagnostics tied to manifest capabilities.
- `pproxy check --json` reports canonical tiers and can be consumed by tests.
- README migration examples use the compatibility alias where appropriate.

## Non-goals

This task does not implement SSR, SSH, QUIC/H3, or full Python API compatibility. It only makes the CLI compatibility contract accurate and usable.
