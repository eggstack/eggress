# Phase 38: pproxy CLI native-equivalent closure

## Goal

Close pproxy CLI compatibility gaps where eggress already has an equivalent native capability but the pproxy compatibility layer still reports the flag as unsupported, unknown, or only manually configurable through TOML.

This phase should materially improve drop-in command-line replacement. A user with a common pproxy command should be able to run:

```bash
eggress pproxy check --json -- <existing pproxy args>
eggress pproxy run -- <existing pproxy args>
```

and receive either working equivalent behavior or a precise, stable diagnostic.

## Current context

The pproxy compatibility parser currently recognizes a bounded set of raw flag keys in `crates/eggress-pproxy-compat/src/args.rs`, including daemon, log, udp-listen, udp-remote, rulefile, verbose, scheduler, alive, ssl, and block. The translator handles some of these by generating TOML and others by warning or marking unsupported. The CLI inventory currently identifies several native-equivalent gaps:

- `--ssl` is unsupported in compat mode even though eggress has rustls listener support through TOML.
- `-b` and `--rulefile` are unsupported in compat mode even though eggress has routing/reject rules.
- `--pac` is partial or not recognized through pproxy compat even though admin PAC serving exists.
- `--test` is partial because `eggress upstream test` exists but the pproxy flag is not translated.
- `--sys` is unsupported in pproxy compat even though system-proxy inspection/dry-run/apply work exists or is planned natively.
- `--log` is unknown/unsupported even though logging can be configured or redirected.
- `-v` is only mapped to an environment-variable warning.
- `-a` alive interval is only mapped to a warning rather than health config.
- `--daemon` and `--reuse` are likely intentional non-parity, but they need stable diagnostic codes and manifest entries.

## Primary deliverables

- Expand pproxy arg parsing and translation for native-equivalent CLI flags.
- Add stable diagnostic codes for all remaining unsupported/intentional-non-parity flags.
- Extend `eggress pproxy check --json` to report feature IDs and tiers aligned with the parity manifest from Phase 37.
- Add CLI tests for translation, check output, and run-path config compilation where feasible.
- Update `docs/cli/PPROXY_CLI_INVENTORY.md` and parity manifest entries.

## Files to inspect

- `crates/eggress-pproxy-compat/src/args.rs`
- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-pproxy-compat/src/diagnostics.rs`
- `crates/eggress-pproxy-compat/src/warnings.rs`
- `crates/eggress-cli/src/main.rs`
- `crates/eggress-config/src/model.rs`
- `crates/eggress-config/src/compile.rs`
- runtime TLS listener config and listener compiler code
- routing rule model/compiler code
- admin/PAC serving code
- system proxy crate and CLI command code
- `docs/cli/PPROXY_CLI_INVENTORY.md`
- `docs/parity/pproxy_capability_manifest.toml`

## Work item 1: `--ssl cert[,key]` translation

### Desired behavior

pproxy-style:

```bash
pproxy -l socks5://:1080 --ssl cert.pem,key.pem
pproxy -l http://:8080 --ssl cert.pem
```

should translate into a listener TLS config in eggress TOML.

### Implementation notes

- Parse `ssl=<value>` from raw flags.
- Accept `cert.pem,key.pem` and, if pproxy permits it, single-file cert/key combined form.
- Attach TLS config to all pproxy local TCP listeners unless pproxy behavior scopes it differently.
- If multiple listeners exist, verify pproxy semantics and document the chosen mapping.
- Ensure generated TOML compiles and runtime actually wraps the listener in TLS.
- If the existing config model cannot express the pproxy single-file form, add a compatibility conversion or emit a precise diagnostic.

### Tests

- Translation test for socks5 listener plus `--ssl cert,key`.
- Translation test for http listener plus `--ssl cert,key`.
- Config compile test for generated TOML.
- Runtime smoke test with generated self-signed cert if testkit supports it.
- JSON check output should report `cli.ssl_listener_flag` as `drop_in` or `compatible_with_warning`, not unsupported.

## Work item 2: `-b PATTERN` block rules

### Desired behavior

pproxy-style block regexes should translate into eggress reject rules.

### Implementation notes

- Preserve pproxy matching scope as closely as possible. If pproxy matches target host only, do not accidentally match full URI unless documented.
- Support multiple `-b` flags if pproxy allows them.
- Translate into deterministic rule IDs like `pproxy-block-0`.
- Place block rules before default upstream/direct rules.
- Ensure block rules apply to TCP and, if pproxy does, UDP. If behavior differs, mark as compatible-with-warning.

### Tests

- `-b '.*\\.example\\.com'` creates a reject rule before default rule.
- `eggress route` or routing unit test confirms blocked host selects reject.
- Non-blocked host still selects default upstream/direct route.
- Invalid regex returns CLI parse/config validation diagnostic, not panic.

## Work item 3: `--rulefile` / `-rulefile`

### Desired behavior

A pproxy rulefile should be parsed or converted to eggress route rules when the grammar is understood. Unsupported rulefile lines should produce line-numbered diagnostics.

### Implementation notes

- First inspect pproxy rulefile grammar and existing eggress `--rules-file` behavior.
- If the current eggress rules-file grammar differs from pproxy's, implement a pproxy-specific parser in the compat crate rather than overloading native rules.
- Preserve rule ordering.
- Do not silently ignore malformed lines.
- Redact secrets if rulefile lines can contain credentialed URIs.

### Tests

- Fixture with simple block rule.
- Fixture with direct/upstream rule if supported by pproxy grammar.
- Fixture with invalid line and expected diagnostic.
- Generated TOML compiles.

## Work item 4: `--pac`

### Desired behavior

If pproxy `--pac` serves or emits PAC configuration, map it to eggress admin PAC serving where practical.

### Implementation notes

- Determine exact pproxy `--pac` accepted forms.
- Add parser support for value/no-value shape as required.
- Generate admin/PAC config in TOML.
- If pproxy expects a file path but eggress generates dynamic PAC, report compatible-with-warning and document the difference.

### Tests

- `--pac` translation includes admin/PAC config.
- Generated PAC endpoint is reachable in an integration smoke test if the admin test harness exists.
- `pproxy check --json` reports correct tier and diagnostic.

## Work item 5: `--test`

### Desired behavior

pproxy `--test` should map to eggress's upstream test capability rather than being ignored.

### Implementation notes

- Decide whether `eggress pproxy run -- --test ...` should test and exit rather than start the service.
- Implement a clear path in `handle_pproxy_run` or add a dedicated mode in compat output.
- Use the existing upstream test machinery where possible.
- Preserve exit-code semantics as closely as possible, while documenting eggress's more granular exit codes.

### Tests

- `eggress pproxy check --json -- ... --test` reports native-equivalent or drop-in.
- `eggress pproxy run -- ... --test` does not start a long-running listener.
- Reachable and unreachable upstream test cases return deterministic output and exit codes.

## Work item 6: `--sys`

### Desired behavior

Where platform support exists, map pproxy `--sys` to eggress system proxy configuration. Where only inspection/dry-run exists, emit compatible-with-warning or native-equivalent diagnostics.

### Implementation notes

- Inspect `crates/eggress-system-proxy` and existing CLI subcommands.
- Avoid making destructive system changes in tests.
- Add dry-run mode for pproxy compat if not already present.
- Ensure unsupported platforms report `platform_missing`, not generic failure.

### Tests

- Non-mutating parser/translation tests.
- Platform-gated dry-run tests.
- JSON diagnostics include platform caveat.

## Work item 7: `--log`, `-v`, and log format

### Desired behavior

pproxy logging flags should have predictable behavior in compat mode.

### Implementation notes

- `-v` should map to log filter/debug verbosity when possible, rather than only warning about `RUST_LOG`.
- `--log FILE` can either configure file logging or be classified as compatible-with-warning with a reliable stderr redirection recommendation.
- Do not add ad hoc global logger reinitialization bugs.
- Respect `--log-format` already available on `eggress pproxy run`.

### Tests

- `-v` check output includes stable feature ID.
- `--log access.log` does not appear as unknown flag.
- If file logging is implemented, verify file creation in tempdir.

## Work item 8: `-a` alive interval

### Desired behavior

Map pproxy alive interval into eggress health probe config when there is at least one upstream.

### Implementation notes

- Interpret seconds as pproxy does.
- Generate health config on upstreams or upstream groups according to eggress config model.
- If pproxy uses passive/active semantics that differ from eggress, mark compatible-with-warning.
- Ensure no health probe is generated for direct-only configs.

### Tests

- `-a 10` creates health config with 10-second interval.
- Invalid values are rejected with CLI parse error.
- Generated TOML compiles.

## Work item 9: intentional non-parity diagnostics

### Scope

For `--daemon`, `--reuse`, and any other deliberate exclusions, add stable diagnostics with suggestions.

### Requirements

- Diagnostic code must be stable.
- Feature ID must match parity manifest.
- `pproxy check --json` must include tier `intentional_non_parity` when appropriate.
- Human output must be explicit and not imply silent fallback.

## Acceptance criteria

- No native-equivalent pproxy flag remains an unknown flag.
- `--ssl`, `-b`, and `--rulefile` have at least config-compile-level tests.
- `--test` has a non-long-running execution path or a clearly documented diagnostic.
- `-a` and `-v` produce better behavior than a vague warning where feasible.
- `--daemon` and `--reuse` are classified deliberately, not accidentally.
- `docs/cli/PPROXY_CLI_INVENTORY.md` matches actual behavior.
- Parity manifest entries are updated for all touched flags.

## Verification commands

Run at minimum:

```bash
cargo fmt --all -- --check
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli
cargo test --workspace
```

Also run the Phase 37 manifest validator if present.

## Non-goals

- Do not implement SSH, QUIC, H3, SSR, SOCKS BIND, or Trojan server here.
- Do not redesign native TOML config unless required for CLI compatibility.
- Do not silently emulate pproxy behavior with weaker security defaults.

## Handoff notes

Prioritize flags with native equivalents first. Avoid turning this phase into a general protocol parity pass. The main deliverable is fewer false incompatibilities for existing pproxy command lines.
