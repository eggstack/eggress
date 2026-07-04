# Phase 37: pproxy parity manifest and validator

## Goal

Create a mechanical, reviewable compatibility contract for pproxy parity. The immediate problem is that current capability claims are spread across README tables, CLI inventory docs, tests, and implementation comments. This phase establishes one source of truth and a validator that prevents parser-only, synthetic-only, or aspirational evidence from being presented as drop-in runtime parity.

This phase should not implement missing proxy protocols. It should make the project honest and measurable so all later phases can change manifest entries from `unsupported` or `native_equivalent` to `drop_in` only when the whole stack is actually complete.

## Current context

The repo already has substantial pproxy compatibility work:

- `crates/eggress-pproxy-compat/` parses pproxy-style args and URI forms.
- `eggress pproxy translate`, `eggress pproxy check`, and `eggress pproxy run` exist in the CLI.
- `docs/cli/PPROXY_CLI_INVENTORY.md` lists many pproxy flags and their current handling.
- README capability checklists mark many core features complete and several protocol/security/packaging items incomplete.
- Some features are implemented only in protocol crates but explicitly refused by runtime/config, such as WebSocket/raw/H2.

The missing piece is a normalized manifest that knows which layer each claim belongs to.

## Primary deliverables

Add these files:

- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/README.md`
- `docs/parity/PPROXY_PARITY_REPORT.md`
- `scripts/validate_pproxy_parity_manifest.py` or a small Rust validator under a suitable crate/binary
- CI hook or cargo/just/check target that runs the validator

If the repo already has a preferred script convention, follow it. If not, keep the validator dependency-free and deterministic.

## Manifest schema

Each manifest entry should represent one pproxy capability, not a broad feature family. Prefer granular IDs like:

- `cli.listen`
- `cli.remote`
- `cli.udp_listen`
- `cli.ssl_listener_flag`
- `uri.chain_separator_double_underscore`
- `uri.modifier_inbound`
- `protocol.http.connect.server`
- `protocol.socks5.udp_associate.server`
- `protocol.socks5.bind.server`
- `protocol.ssh.upstream`
- `python.start_pproxy`
- `python.context_manager`

Required fields:

```toml
[[capability]]
id = "cli.listen"
category = "cli"
pproxy_surface = "-l / --listen"
pproxy_behavior = "Bind one or more TCP listener URIs."
eggress_behavior = "Translates to listener config and runs through supervisor."
tier = "drop_in"
parser = "complete"
translator = "complete"
config = "complete"
runtime = "complete"
cli = "complete"
python = "not_applicable"
docs = "complete"
tests = ["crates/..."]
evidence = "integration"
notes = ""
```

Allowed `tier` values:

- `drop_in`
- `compatible_with_warning`
- `native_equivalent`
- `intentional_non_parity`
- `unsupported`

Allowed layer values:

- `complete`
- `partial`
- `not_started`
- `not_applicable`
- `refused`

Allowed evidence values:

- `differential`
- `integration`
- `unit`
- `synthetic`
- `docs_only`
- `none`

## Validation rules

The validator must fail on these conditions:

1. Unknown tier, layer, or evidence value.
2. Duplicate capability ID.
3. `drop_in` capability with any required layer not equal to `complete`.
4. `drop_in` capability with evidence weaker than `integration`, unless an explicit `differential_exception` field is present.
5. `compatible_with_warning` capability without a diagnostic code or migration note.
6. `intentional_non_parity` capability without a rationale.
7. `unsupported` capability with `runtime = "complete"` or contradictory layer values.
8. Any capability marked `drop_in` while `runtime = "refused"`.
9. Any protocol-crate-only feature marked as `drop_in` before config/compiler/runtime support exists.
10. Any CLI capability with no documented stdout/stderr/exit-code expectation.
11. Any Python capability marked `drop_in` with no Python import/test evidence.

The validator should print precise file/entry IDs and actionable errors.

## Initial manifest coverage

Seed the manifest with at least these categories.

### CLI flags

- `-l` / `--listen`
- `-r` / `--remote`
- `-ul` / `--udp-listen`
- `-ur` / `--udp-remote`
- `-s`
- `-a`
- `--ssl`
- `-b`
- `--rulefile` / `-rulefile`
- `--daemon` / `-d`
- `-v`
- `--log` / `-log`
- `--reuse`
- `--pac`
- `--sys`
- `--get`
- `--test`
- `-f` / `--config`
- `--version`
- `--help`
- positional URI arguments

### URI grammar

- schemes: `http`, `https`, `socks4`, `socks4a`, `socks5`, `ss`, `shadowsocks`, `ssr`, `trojan`, `ssh`, `direct`, `redir`, `unix`, `bind`, `listen`, `backward`, `rebind`, `raw`, `tunnel`, `ws`, `wss`, `h2`
- modifiers: `+ssl`, `+tls`, `+in`, repeated `+in`
- chain separator `__`
- credentials, including password-only Trojan style
- IPv4, IPv6, domain targets
- default ports
- query/rule fragments

### Runtime protocols

- HTTP CONNECT server/client
- HTTP ordinary forward proxying
- SOCKS4/4a CONNECT server/client
- SOCKS4 BIND
- SOCKS5 CONNECT server/client
- SOCKS5 BIND
- SOCKS5 UDP ASSOCIATE server/client
- direct TCP/UDP
- Shadowsocks AEAD TCP/UDP client/server
- SSR
- Trojan client/server/fallback
- SSH upstream
- QUIC
- HTTP/3
- WebSocket/raw/H2 runtime support

### Routing and operations

- routing rules
- block rules
- rulefile translation
- schedulers
- health/alive checks
- route explanation
- upstream test
- PAC generation/serving
- system proxy inspect/apply/rollback
- admin/metrics
- config reload

### Python API

- importable top-level package
- `translate_pproxy_args`
- `translate_pproxy_uri`
- `check_pproxy_args`
- `start_pproxy`
- service class
- context manager
- async compatibility wrappers if planned
- status/metrics/reload/shutdown
- unsupported feature exceptions
- migration aliases

## Generated or maintained report

`docs/parity/PPROXY_PARITY_REPORT.md` should summarize:

- total capabilities by tier
- drop-in capabilities
- compatible-with-warning capabilities
- native-equivalent but non-drop-in capabilities
- intentional non-parity decisions
- unsupported blockers
- protocol-crate-only caveats
- Python drop-in status
- next recommended phase

This report can be generated by the validator or maintained manually for the first pass. If manual, the validator should at least ensure that every capability ID in the report exists in the manifest.

## Integration points to inspect

- `README.md`
- `docs/cli/PPROXY_CLI_INVENTORY.md`
- `docs/cli/EXIT_CODES.md`
- `crates/eggress-pproxy-compat/src/args.rs`
- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-pproxy-compat/src/diagnostics.rs`
- `crates/eggress-cli/src/main.rs`
- `crates/eggress-python/src/lib.rs`
- protocol crates under `crates/eggress-protocol-*`
- runtime/config compiler code that refuses or accepts protocol schemes

## Implementation steps

1. Create `docs/parity/`.
2. Add a concise `README.md` explaining tiers, layers, and evidence levels.
3. Seed `pproxy_capability_manifest.toml` with all high-value capability entries listed above.
4. Add the validator with strict schema checks.
5. Add tests for the validator itself using a deliberately invalid mini-manifest fixture if the repo has a test-fixture convention.
6. Wire the validator into CI or the repo's standard verification command.
7. Create the initial parity report.
8. Update README and CLI inventory references to point at the parity manifest/report as the authoritative source.

## Acceptance criteria

- `docs/parity/pproxy_capability_manifest.toml` exists and covers at least CLI, URI, runtime protocol, routing/ops, and Python API categories.
- Validator rejects malformed entries and contradictory parity claims.
- Protocol-crate-only WebSocket/raw/H2 capabilities cannot be marked `drop_in` while runtime refuses them.
- SSH, SSR, QUIC/H3, SOCKS BIND, and incomplete Trojan modes are not mislabeled as complete.
- The README no longer implies a final parity state that is not backed by manifest entries.
- `eggress pproxy check --json` has a clear future mapping to manifest tiers, even if that mapping is not fully implemented in this phase.

## Tests and verification

Run the normal workspace checks after changes. At minimum:

```bash
cargo fmt --all -- --check
cargo test --workspace
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml
```

Adjust the validator invocation if implemented in Rust.

## Non-goals

- Do not implement missing protocols in this phase.
- Do not change runtime behavior except docs/report wiring.
- Do not promote existing features to `drop_in` without evidence.
- Do not remove intentional non-parity decisions; classify them.

## Handoff notes

Be conservative. It is better for the initial manifest to underclaim than overclaim. Later phases should earn `drop_in` by adding runtime, CLI, Python, and differential evidence.
