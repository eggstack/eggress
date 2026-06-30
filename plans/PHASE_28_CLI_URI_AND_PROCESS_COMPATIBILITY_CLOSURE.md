# Phase 28 Plan: CLI, URI, Diagnostics, and Process Compatibility Closure

## Purpose

Phase 28 closes the pproxy compatibility surface around user-facing command behavior: CLI flags, URI grammar, diagnostics, exit codes, logging, process lifecycle, config translation, and Python helper parity for pproxy-shaped invocations.

Earlier phases added substantial protocol behavior and evidence discipline. Phase 28 makes the command surface predictable enough that users can migrate common pproxy invocations with confidence and can understand every unsupported case without reading Rust source.

## Scope

This phase covers:

- Full inventory of pproxy CLI flags and invocation forms for target `pproxy==2.7.9`.
- URI grammar closure for supported and unsupported schemes.
- CLI translation consistency for `eggress pproxy translate`, `check`, and `run`.
- Diagnostics and exit-code compatibility.
- Process signal handling and shutdown behavior.
- Logging verbosity and redaction behavior.
- Config-generation stability and golden tests.
- Python translation helper alignment where helpers expose pproxy argument parsing.
- Documentation and manifest closure for CLI/user-facing behavior.

## Non-goals

Do not implement new protocol capability solely because a CLI flag exists. Unsupported flags may remain unsupported if the manifest and diagnostics are precise.

Do not implement daemonization if the project decides systemd/process managers are the supported path. If daemon mode remains non-parity, document it cleanly.

Do not implement system proxy configuration in this phase unless it is trivial and already planned. `--sys` can remain intentional non-parity with good diagnostics.

Do not claim full Python pproxy API drop-in compatibility. That belongs to later Python API phases.

## Compatibility principle

For every pproxy CLI input, Eggress should do one of three things:

1. Run an equivalent configuration.
2. Translate/check and report a precise supported/partial/unsupported classification.
3. Reject with a stable diagnostic, redacted sensitive values, and documented exit code.

Silent fallback is not acceptable.

## Work items

### 28.1 Full pproxy CLI inventory

Capture `pproxy==2.7.9` CLI behavior.

Inventory:

- `-l` / listen forms;
- `-r` / remote forms;
- `-ul` / UDP listen forms;
- `-ur` / UDP remote forms;
- `-s` scheduler forms;
- auth forms;
- `--ssl` behavior;
- `--pac` behavior;
- `--get` behavior;
- `--sys` behavior;
- `--test` behavior;
- `--version` behavior;
- `--help` output shape;
- daemon/reuse/log/rulefile/block/bind flags;
- config-file or plugin-related flags;
- accepted short/long aliases;
- repeated flag behavior;
- invalid flag diagnostics;
- exit codes for success, config error, bind error, runtime error, interrupted.

Store inventory in:

```text
docs/cli/PPROXY_CLI_INVENTORY.md
docs/PPROXY_PARITY_SPEC.md
```

Update manifest entries for every flag.

### 28.2 URI grammar inventory and golden corpus

Create a golden corpus of accepted and rejected URI forms.

Suggested path:

```text
tests/compat/fixtures/pproxy_uri_corpus.toml
```

Each case should include:

- id;
- raw URI;
- pproxy interpretation;
- Eggress expected interpretation;
- compatibility tier;
- whether credentials are present;
- expected redacted display;
- expected warnings;
- expected generated TOML, if supported.

Cover:

- HTTP/HTTPS;
- SOCKS4/SOCKS4a/SOCKS5;
- Shadowsocks modern AEAD;
- Trojan;
- standalone UDP;
- transparent/Unix/advanced/reverse schemes as they are captured;
- chained `__` syntax;
- TLS wrappers;
- auth in URI;
- percent-encoding;
- IPv6 literals;
- domains;
- local bind forms;
- unsupported SSR/legacy/SSH/QUIC/etc.

### 28.3 Golden CLI translation tests

Add golden tests for `eggress pproxy translate`.

Suggested fixtures:

```text
tests/compat/fixtures/pproxy_cli_cases/*.toml
```

Each fixture:

```toml
id = "socks5_http_chain"
args = ["-l", "socks5://127.0.0.1:1080", "-r", "http://127.0.0.1:8080"]
expected_status = "compatible"
expected_warnings = []
expected_toml = "..."
```

Requirements:

- generated TOML is deterministic;
- redacted output is deterministic;
- warnings are stable;
- unsupported forms produce exact diagnostic snapshots;
- fixtures are easy to review.

### 28.4 `eggress pproxy check` classification closure

Ensure `check` output is consistent with manifest and compatibility evidence.

Requirements:

- every parsed listen/remote/scheme maps to a manifest feature id;
- status uses manifest terminology;
- supported-but-not-compatible features are not reported as compatible;
- partial features list missing behavior;
- intentional non-parity features show rationale;
- output has human and machine-readable modes if useful.

Potential command:

```bash
eggress pproxy check --json -l socks5://127.0.0.1:1080 -r http://127.0.0.1:8080
```

JSON output should be stable enough for tests and automation.

### 28.5 `eggress pproxy run` process behavior

Audit process behavior against pproxy.

Cases:

- successful startup;
- bind failure;
- invalid config;
- unsupported feature;
- upstream resolution failure;
- interrupt/SIGINT;
- SIGTERM;
- active connection drain;
- reload if applicable;
- stdout/stderr output;
- exit code.

Requirements:

- process exits nonzero on config/parse/startup failure;
- signal handling is deterministic;
- listeners shut down cleanly;
- partial config startup does not occur silently;
- errors redact credentials;
- admin endpoints or runtime tasks do not linger after shutdown.

Add tests using subprocess harnesses where possible.

### 28.6 Diagnostics taxonomy

Standardize pproxy compatibility errors.

Error categories:

- unsupported protocol;
- unsupported transport wrapper;
- unsupported flag;
- unsupported platform;
- unsupported security-sensitive legacy feature;
- invalid URI syntax;
- invalid chain composition;
- missing target;
- missing credential;
- invalid cipher/method;
- bind failure;
- privilege/capability missing;
- external dependency missing for tests.

Requirements:

- each diagnostic has a stable code;
- sensitive values are redacted;
- diagnostics include manifest feature id where applicable;
- diagnostics include suggested Eggress-native alternative when useful;
- tests cover exact messages or structured codes.

Example structured diagnostic:

```json
{
  "code": "unsupported_protocol",
  "feature_id": "ssh_upstream",
  "tier": "unsupported",
  "message": "ssh:// upstreams are not implemented",
  "suggestion": "Use socks5:// or http:// upstreams, or track Phase 24/SSH parity work"
}
```

### 28.7 Exit code policy

Define and document exit codes.

Suggested policy:

```text
0  success / clean shutdown
1  generic runtime failure
2  CLI parse error
3  config validation error
4  bind/listen startup failure
5  unsupported pproxy compatibility feature
6  platform capability missing
7  external dependency missing for test/diagnostic command
130 interrupted by SIGINT
143 terminated by SIGTERM
```

Tasks:

- implement constants rather than ad hoc returns;
- update CLI paths;
- add subprocess tests;
- document in `docs/cli/EXIT_CODES.md`.

Do not overfit to pproxy if pproxy uses inconsistent exit codes; document any deliberate divergence.

### 28.8 Logging and verbosity compatibility

Inventory pproxy logging flags and map to Eggress tracing behavior.

Tasks:

- document pproxy `--log` or verbosity behavior;
- decide whether to implement aliases or reject;
- ensure `RUST_LOG`/tracing options are documented;
- add redaction tests for logs;
- add `--quiet`/`--verbose` compatibility only if pproxy supports equivalent behavior or Eggress already exposes it.

Acceptance:

- unsupported logging flags have clear diagnostics;
- Eggress-native logging is documented as migration path;
- no credential leaks in logs.

### 28.9 PAC, `--get`, and static helper behavior

pproxy includes convenience behaviors like PAC generation/serving and URL fetch/test helpers. Eggress already has PAC/static/admin pieces, but CLI compatibility must be audited.

Tasks:

- capture pproxy `--pac` behavior;
- capture `--get` behavior;
- capture `--test` behavior;
- map to existing Eggress admin/PAC/test commands;
- add CLI aliases if low-risk;
- otherwise classify as supported via Eggress-native command or intentional non-parity.

Tests:

- PAC output shape if implemented;
- route test output;
- unsupported diagnostics.

### 28.10 Python helper alignment

If Python bindings expose translation helpers, align them with CLI behavior.

Targets:

- `translate_pproxy_args`;
- `translate_pproxy_uri`;
- `start_pproxy`;
- `EggressService.from_pproxy_args`;
- async variants if present.

Requirements:

- Python helper errors expose the same structured codes as CLI where practical;
- generated config matches CLI translation;
- redaction behavior matches;
- tests share fixture cases with Rust CLI where practical.

Do not claim full pproxy Python API parity in this phase.

### 28.11 Manifest and evidence integration

Tie CLI behavior to the manifest.

Tasks:

- every CLI flag gets a feature id;
- every URI scheme gets a feature id;
- `eggress pproxy check` uses manifest classifications where possible;
- manifest test-name validation includes CLI fixture tests;
- compatibility evidence table includes CLI section with accurate evidence type;
- doc taxonomy distinguishes compatible CLI translation from pproxy runtime/protocol compatibility.

This is especially important for features like `-ul`/`-ur`, where translation compatibility and protocol differential compatibility are related but not identical.

### 28.12 Documentation updates

Create/update:

```text
docs/cli/PPROXY_CLI_INVENTORY.md
docs/cli/URI_GRAMMAR.md
docs/cli/EXIT_CODES.md
docs/PPROXY_MIGRATION.md
docs/PARITY_MATRIX.md
docs/COMPATIBILITY_EVIDENCE.md
docs/PPROXY_PARITY_SPEC.md
README.md
```

Docs must include:

- common pproxy command migrations;
- unsupported features and why;
- equivalent Eggress-native config for unsupported flags;
- exact validation commands;
- distinction between CLI translation compatibility and runtime protocol compatibility.

## Testing strategy

Ungated:

- CLI parser tests;
- URI parser golden tests;
- translation fixture tests;
- diagnostic code tests;
- exit-code subprocess tests for parse/config failures;
- redaction tests;
- Python helper fixture tests.

Gated:

- pproxy differential for runtime-equivalent CLI forms;
- pproxy behavior probes for exit codes and diagnostics where deterministic;
- PAC/get/test helper behavior if implemented.

## Validation commands

Baseline:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-testkit manifest
```

CLI-specific:

```bash
cargo test -p eggress-cli pproxy
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test cli_exit_codes
cargo test -p eggress-cli --test pproxy_translation_golden
```

Python helper tests:

```bash
python -m pytest python/tests/test_pproxy_compat.py
python -m pytest python/tests/test_pproxy_redaction.py
```

Gated:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1 cli
```

## Acceptance criteria

Phase 28 is complete when:

- pproxy CLI inventory exists for target version 2.7.9.
- URI golden corpus exists and covers supported/unsupported forms.
- Translation output is deterministic and fixture-tested.
- `check` output maps to manifest classifications.
- `run` process behavior has subprocess tests for major failure and signal cases.
- Diagnostics have stable codes and redaction tests.
- Exit codes are defined and documented.
- Python pproxy translation helpers align with CLI fixtures where applicable.
- Manifest, parity matrix, compatibility evidence, README, and migration docs agree.

## Remaining expected gaps after this phase

- True pproxy-shaped Python API drop-in replacement.
- System proxy setting integration if still deferred.
- Any protocol features intentionally deferred from phases 25–27.
- Potential legacy Shadowsocks/SSR final non-parity unless policy changes.

## Handoff notes

Phase 28 is a user-surface hardening phase. The value is not a new protocol; the value is making migration predictable. Every unsupported pproxy input should be boring: deterministic error, stable code, redacted output, and a documented alternative or rationale.
