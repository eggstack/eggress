# Phase 43: parity report generator polish

## Goal

Fix the remaining documentation-generator polish issue from Phase 42: `docs/parity/PPROXY_PARITY_REPORT.md` now has correct tier counts and is generated from the manifest, but the generated `Protocol-Crate-Only Caveats` section is over-broad. It currently groups every capability with `config = "refused"` or `runtime = "refused"` under a heading that implies the feature is implemented in a protocol crate and merely blocked by config/runtime wiring.

That is not true for several entries, including daemon mode, SOCKS BIND, Trojan server, SSH, QUIC/H3, and rulefile translation. This phase should make the report generator classify caveats accurately without weakening the manifest validator or reintroducing hand-edited report drift.

## Current problem

The generated report currently has a section titled `Protocol-Crate-Only Caveats` and populates it with every manifest capability whose `config` or `runtime` layer is `refused`. This creates false implications:

- `cli.daemon` is not a protocol crate feature.
- `protocol.socks4_bind` and `protocol.socks5_bind` are missing command implementations, not protocol-crate-only runtime refusals.
- `protocol.trojan_server` is not present as a complete protocol crate feature.
- `protocol.ssh_upstream` is not merely waiting for config/runtime promotion; it needs transport implementation and policy decisions.
- `protocol.quic` and `protocol.http3` are deferred by ADR, not protocol-crate-only completed work.
- `routing.rulefile_translation` is a translator/routing compatibility matter, not a protocol crate caveat.

The section should be split into precise buckets.

## Deliverables

### D1: Add manifest metadata for caveat class

Preferred: add an optional field to relevant manifest entries:

```toml
caveat_class = "protocol_crate_only"
```

Allowed values:

- `protocol_crate_only` — parser/protocol crate exists, but config/runtime refuses it.
- `missing_protocol_command` — protocol exists but a command/mode is missing, such as SOCKS BIND.
- `missing_protocol_role` — protocol client exists but server/listener role is missing, such as Trojan server.
- `deferred_by_adr` — deliberately deferred design area, such as QUIC/H3.
- `intentional_non_parity` — deliberate non-parity, such as SSR or reuse.
- `cli_process_model` — CLI/process behavior, such as daemon mode.
- `translator_scope_gap` — translator/rule compatibility gap, such as full rulefile parity.

If adding a new manifest field is too much, implement report-side predicate lists keyed by capability ID. Prefer the manifest field because the report should remain data-driven.

### D2: Update report generator sections

Replace the single over-broad section with these generated sections when non-empty:

1. `Protocol-Crate-Only Runtime Refusals`
2. `Missing Protocol Commands or Roles`
3. `Deferred Design Areas`
4. `Intentional Non-Parity`
5. `CLI / Translator Scope Gaps`

Each section should include:

- capability ID;
- current tier;
- refused layers, if applicable;
- short note from manifest;
- next phase pointer when known.

### D3: Add validator rule for caveat classification

Add a validation rule:

- If `config = "refused"` or `runtime = "refused"`, require either `caveat_class` or an explicit `rationale`/`notes` marker that explains why the refusal exists.
- If `caveat_class = "protocol_crate_only"`, require notes to say which protocol crate or module exists and which layer refuses it.
- If `caveat_class = "deferred_by_adr"`, require an ADR path in notes/rationale.

The rule can be warning by default and promoted under `--strict`.

### D4: Regenerate and check the parity report

Run:

```bash
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --write-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

Fix command ordering if the script expects manifest path before flags.

## Files to inspect/change

- `scripts/validate_pproxy_parity_manifest.py`
- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PARITY_REPORT.md`
- `docs/parity/README.md`
- `docs/PARITY_MATRIX.md` if it mirrors caveat language
- `AGENTS.md` and `.skills/testing/skill.md` only if commands change

## Classification targets

Suggested classifications:

- `cli.daemon` -> `cli_process_model`
- `cli.get` -> `cli_process_model` or `translator_scope_gap`
- `uri.scheme_raw`, `uri.scheme_tunnel`, `uri.scheme_ws`, `uri.scheme_wss`, `uri.scheme_h2`, `protocol.ws_runtime`, `protocol.raw_runtime`, `protocol.h2_runtime` -> `protocol_crate_only`
- `protocol.socks4_bind`, `protocol.socks5_bind` -> `missing_protocol_command`
- `protocol.trojan_server` -> `missing_protocol_role`
- `protocol.ssh_upstream`, `uri.scheme_ssh` -> `missing_protocol_transport`
- `protocol.quic`, `protocol.http3` -> `deferred_by_adr`
- `protocol.ssr`, `uri.scheme_ssr` -> `intentional_non_parity`
- `routing.rulefile_translation` -> `translator_scope_gap`

If adding `missing_protocol_transport`, include it in allowed values.

## Tests

Add or update tests for the report generator:

- generated report no longer places `cli.daemon` under `Protocol-Crate-Only Runtime Refusals`;
- generated report places WS/raw/H2 entries under protocol-crate-only;
- generated report places SOCKS BIND under missing commands;
- generated report places QUIC/H3 under deferred design areas;
- `--check-report` still catches tier-count drift;
- validator catches unknown `caveat_class` values.

A Python unit test for the script is preferred if the repo already has script tests. Otherwise add a small fixture-driven test under `tests` or `crates/eggress-testkit` that shells the script.

## Acceptance criteria

- The generated parity report no longer implies all refused features are protocol-crate-only.
- The report remains generated from the manifest, not hand-edited.
- Manifest validation still passes in normal and strict mode.
- CI still checks report consistency.
- README/PARITY_MATRIX language remains consistent with the refined caveat categories.

## Non-goals

- Do not implement any missing protocols.
- Do not change parity tiers unless this classification audit reveals an actual tier bug.
- Do not remove the report generator.
- Do not collapse the manifest back into a hand-written report.
