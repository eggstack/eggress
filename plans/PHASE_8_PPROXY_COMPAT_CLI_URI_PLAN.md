# Phase 8 Detailed Plan: `pproxy`-Compatible CLI and URI Translation

## Purpose

Phase 8 adds a compatibility layer for common Python `pproxy` invocation and URI shapes. The goal is migration ergonomics: users should be able to translate or run common `pproxy` commands through Eggress without manually writing full Eggress TOML.

This phase must not implement new proxy protocols. Unsupported `pproxy` features should produce precise diagnostics and parity-tier warnings.

---

# Inputs from Phase 7

Required before serious implementation:

- `docs/PPROXY_PARITY_SPEC.md`;
- expanded `docs/PARITY_MATRIX.md`;
- tier taxonomy;
- reusable differential harness helpers;
- list of intentionally unsupported pproxy behaviors.

If Phase 7 is incomplete, implement Phase 8 behind conservative tests and mark unsupported/unknown items explicitly.

---

# Non-goals

Do not implement:

- Shadowsocks support;
- new scheduler semantics beyond translation to existing schedulers;
- Python bindings;
- PyPI packaging;
- multi-hop UDP;
- new runtime architecture.

---

# Workstream 1: Add compatibility crate

## Target

Create:

```text
crates/eggress-pproxy-compat/
```

## Responsibilities

The crate should parse and translate pproxy-compatible inputs into Eggress-native structures.

Suggested modules:

```text
src/lib.rs
src/error.rs
src/args.rs
src/uri.rs
src/translate.rs
src/warnings.rs
src/tests.rs
```

## Public API sketch

```rust
pub struct PproxyCompatInput {
    pub local: Vec<String>,
    pub remotes: Vec<String>,
    pub flags: Vec<String>,
}

pub struct TranslationOutput {
    pub toml: String,
    pub warnings: Vec<CompatWarning>,
    pub unsupported: Vec<UnsupportedFeature>,
}

pub fn translate_pproxy_args(args: &[String]) -> Result<TranslationOutput, CompatError>;
pub fn translate_pproxy_uri(local: &str, remotes: &[String]) -> Result<TranslationOutput, CompatError>;
```

## Acceptance criteria

- Crate compiles independently.
- All emitted TOML validates through `eggress-config`.
- Errors are structured and redact credentials.

---

# Workstream 2: URI grammar support

## Goal

Parse common pproxy URI forms and map them to Eggress config.

## Minimum supported schemes

For Phase 8:

- local `http://`, `socks4://`, `socks5://` where applicable;
- upstream `http://`, `socks4://`, `socks5://`, `trojan://` only if already supported by Eggress;
- direct/no-upstream mode;
- username/password auth where Eggress supports it;
- bind host/port;
- chained remote list for TCP-compatible protocols.

## Explicitly unsupported in this phase

- Shadowsocks URI compatibility;
- SSH or other protocols not yet supported;
- UDP through non-SOCKS5 upstreams;
- multi-hop UDP;
- unsafe transparent/system modes.

## Translation behavior

For each URI:

- parse into a typed compatibility AST;
- validate known scheme;
- redact credentials in warnings/errors;
- emit Eggress listener/upstream/rule TOML;
- include warnings for partial behavior.

## Tests

Add tests for:

- valid local SOCKS5 direct;
- valid local HTTP direct;
- valid local SOCKS4 direct;
- SOCKS5 local through HTTP upstream;
- SOCKS5 local through SOCKS5 upstream;
- auth credentials redacted;
- unsupported Shadowsocks returns `UnsupportedFeature` not panic;
- malformed URI gives structured error.

## Acceptance criteria

- Common supported pproxy URI examples translate to valid Eggress TOML.

---

# Workstream 3: CLI compatibility command

## Goal

Expose pproxy compatibility through the existing Eggress CLI.

## Proposed commands

```bash
eggress pproxy run ...
eggress pproxy translate ...
eggress pproxy check ...
```

Alternative acceptable shape:

```bash
eggress compat pproxy run ...
eggress compat pproxy translate ...
```

## Command behavior

### `translate`

- accepts pproxy-like arguments;
- prints Eggress TOML to stdout;
- prints warnings to stderr;
- exits nonzero on unsupported required features;
- never prints credentials in warnings unless explicitly requested with a dangerous debug flag.

### `check`

- validates pproxy-like arguments;
- prints parity tier and warnings;
- does not start service.

### `run`

- translates arguments;
- starts Eggress with generated config;
- preserves existing logging/metrics behavior;
- handles Ctrl-C/shutdown cleanly.

## Acceptance criteria

- CLI can translate and run common scenarios.
- Unsupported inputs fail deterministically with useful messages.

---

# Workstream 4: Config emission

## Goal

Generate canonical Eggress TOML that is readable and debuggable.

## Requirements

- stable listener names;
- stable upstream IDs;
- stable upstream group IDs;
- explicit `version = 1`;
- explicit rules;
- optional comments explaining compatibility warnings if `--annotate` is set;
- no plaintext credentials in diagnostic output outside TOML itself.

## Example output

```toml
version = 1

[[listeners]]
name = "pproxy-local-0"
bind = "127.0.0.1:1080"
protocols = ["socks5"]

[[upstreams]]
id = "pproxy-upstream-0"
uri = "http://proxy.example:8080"

[[upstream_groups]]
id = "pproxy-chain"
scheduler = "first-available"
members = ["pproxy-upstream-0"]
fallback = "reject"

[[rules]]
id = "pproxy-default"
any = true
upstream_group = "pproxy-chain"
```

## Acceptance criteria

- Generated config round-trips through Eggress config validation.
- Generated config does not depend on hidden process state.

---

# Workstream 5: Differential migration tests

## Goal

Prove the compatibility command matches pproxy for top migration paths.

## Test file

Extend:

```text
crates/eggress-cli/tests/differential_pproxy.rs
```

or add:

```text
crates/eggress-cli/tests/pproxy_compat_cli.rs
```

## Required scenarios

All gated behind `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`:

1. pproxy-style local SOCKS5 direct.
2. pproxy-style local HTTP CONNECT direct.
3. pproxy-style local SOCKS4 direct.
4. pproxy-style SOCKS5 local through HTTP upstream.
5. pproxy-style SOCKS5 local through SOCKS5 upstream.
6. pproxy-style SOCKS5 UDP direct if CLI grammar supports it.
7. auth success.
8. auth failure coarse equivalence.

## Acceptance criteria

- Compatibility mode is tested against pproxy, not just unit translation.

---

# Workstream 6: Docs and migration guide

## Required docs

Add:

```text
docs/PPROXY_MIGRATION.md
```

Update:

```text
README.md
docs/PARITY_MATRIX.md
docs/PPROXY_PARITY_SPEC.md
docs/CONFIG_REFERENCE.md
```

## Required content

- common pproxy commands and Eggress equivalents;
- `eggress pproxy translate` examples;
- supported/unsupported URI schemes;
- warnings and parity tiers;
- troubleshooting;
- credential redaction behavior;
- examples of generated TOML.

---

# Recommended commit sequence

1. Add `eggress-pproxy-compat` crate skeleton and error types.
2. Implement URI parser and TOML translator for direct local listeners.
3. Add upstream translation for HTTP/SOCKS4/SOCKS5/Trojan.
4. Add CLI `translate` and `check`.
5. Add CLI `run`.
6. Add differential compatibility tests.
7. Add migration docs and completion record.

---

# Required verification

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli pproxy
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Optional/gated:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

---

# Definition of done

Phase 8 is complete only when:

1. A pproxy compatibility crate exists.
2. Common supported pproxy URI forms translate to valid Eggress TOML.
3. CLI exposes translate/check/run behavior.
4. Unsupported features return structured diagnostics.
5. Credentials are redacted in warnings/errors.
6. Common migration paths have tests.
7. Gated differential tests exist for top paths.
8. Docs explain migration and unsupported behavior.
9. No new protocol support is claimed.
10. Workspace checks pass locally.

## Completion record

Add:

```text
docs/PHASE_8_PPROXY_COMPAT_CLI_URI_COMPLETION.md
```

Include supported URI forms, unsupported forms, test list, differential coverage, and remaining blockers for Phase 9.
