# Phase A1 — Baseline correction and source-of-truth repair

## Objective

Make the repository's published parity claims exactly match executable behavior on current `main`. This phase is a correctness and governance pass, not a feature-expansion pass. It must repair the reverse-configuration regression, reconcile all parity documents and generated outputs, and ensure no parser-only, translation-only, protocol-crate-only, or synthetic-only capability is presented as runtime drop-in parity.

## Context

The current repository has a mature manifest and generated parity report, but several sources can drift independently: `README.md`, `docs/ROADMAP.md`, `docs/PARITY_MATRIX.md`, `docs/parity/pproxy_capability_manifest.toml`, generated reports, Python capability introspection, CLI compatibility output, and implementation comments. The latest reverse-server compiler path also constructs `external_bind = None` and then rejects reverse-server configurations, while the README presents reverse support as operationally complete.

## Scope

### Workstream 1: establish the audited baseline

1. Pin the audit to the current `main` SHA and `pproxy==2.7.9`.
2. Enumerate every parity capability and its claimed tier.
3. For each capability, inspect the referenced implementation and evidence.
4. Record whether evidence is unit, integration, differential, external interoperability, platform-gated, synthetic, parser-only, translation-only, or documentation-only.
5. Produce a temporary audit table under `target/` or a generated artifact; do not create another manually maintained source of truth.

### Workstream 2: repair reverse configuration

1. Add `external_bind` to the reverse-server TOML model using the repository's established address representation.
2. Define whether it is required or has a safe default. Prefer explicit requirement unless pproxy compatibility requires a default.
3. Thread it through deserialization, validation, compilation, redacted serialization, examples, and runtime construction.
4. Verify reverse-client `default_target_host` and `default_target_port` are represented, validated, compiled, and consumed.
5. Remove placeholder assignments that force valid configurations to fail.
6. Ensure bind allowlist/security policy is evaluated against the actual configured external bind.
7. Add negative tests for missing bind, malformed address, unsafe wildcard bind without required policy, partial target configuration, and invalid credentials.
8. Add an end-to-end test in which an external client reaches the configured target through the reverse server/client pair.

### Workstream 3: reconcile capability classifications

Review at minimum:

- reverse/backward capabilities;
- WebSocket/WSS;
- raw/tunnel;
- H2;
- Trojan server/fallback/interoperability;
- transparent proxying;
- Python async wrappers;
- rulefile translation;
- `--reuse`, `--daemon`, and `--get`;
- UDP multi-hop and pproxy-specific framing;
- package/release claims.

Demote any capability where an applicable layer is incomplete. A protocol crate implementation refused by config or runtime must not be `drop_in`. A synthetic test without real protocol interoperability must be described accurately.

### Workstream 4: unify generated and user-facing outputs

1. Make the manifest the canonical machine-readable source.
2. Ensure the generated parity report is reproducible and checked in CI.
3. Ensure Python `supported_features()` and `CompatibilityReport` derive from or are validated against the manifest.
4. Ensure `eggress pproxy check --json` uses the same tier vocabulary and feature IDs.
5. Update README and roadmap text to avoid stale counts or claims.
6. Add a validator rule that rejects README/runtime claims exceeding the manifest tier.
7. Add a validator rule that rejects `drop_in` entries with an applicable layer marked incomplete or refused.
8. Add a validator rule requiring a named evidence reference for every release-blocking drop-in capability.

### Workstream 5: regression evidence for recent fixes

Verify tests exist for:

- empty or partial listener credentials;
- password environment resolution;
- reverse authentication framing and bounds;
- reverse control counter underflow/races;
- private IPv6 ULA classification;
- HTTP transfer-encoding tokenization;
- chunk extensions and trailers;
- premature EOF for fixed-length bodies;
- control characters in HTTP headers;
- decoded request-body limits;
- SOCKS4a domain forwarding;
- percent-decoded URI credentials;
- Python redaction and `wait_closed()` behavior.

Add or strengthen tests where the fix is only indirectly covered.

## Expected files

Likely areas include:

- `crates/eggress-config/src/model.rs`
- `crates/eggress-config/src/validate.rs`
- `crates/eggress-config/src/compile.rs`
- reverse runtime/server/client modules
- Python compatibility module and stubs
- `docs/parity/pproxy_capability_manifest.toml`
- `scripts/validate_pproxy_parity_manifest.py`
- `docs/parity/PPROXY_PARITY_REPORT.md`
- `README.md`
- `docs/ROADMAP.md`
- parity and reverse integration tests

The implementer should follow actual repository layout rather than forcing these exact paths.

## Testing requirements

Run and require success for:

- `cargo fmt --all -- --check`
- workspace build/check on all features used in CI
- workspace unit and integration tests
- targeted reverse tests
- parity manifest validation and report consistency checks
- Python tests across supported minimum and current Python versions
- pproxy differential suite with its explicit environment gate
- Windows compile checks for platform-gated changes
- Linux reverse end-to-end test

No broad assertion such as "stderr is non-empty" may substitute for a semantic assertion.

## Acceptance criteria

- A valid reverse-server configuration can be expressed in TOML and starts successfully.
- A real external connection traverses reverse server and client to a target.
- Missing or unsafe reverse configuration fails before runtime with stable diagnostics.
- Every parity source reports the same tiers and counts.
- No runtime-refused transport is described as runtime drop-in.
- Every drop-in capability has implementation, applicable-layer completeness, and named evidence.
- Recent bug fixes have direct regression coverage.
- All required CI-equivalent commands pass.
- Documentation clearly distinguishes implemented, protocol-crate-only, platform-gated, intentional non-parity, and unsupported behavior.

## Out of scope

- adding new transports;
- promoting WS/raw/H2 into the supervisor;
- completing full Python `Connection`/`Server` compatibility;
- redesigning reverse protocol semantics beyond what is needed to make current behavior correctly configurable;
- implementing SSH, QUIC/H3, SSR, or multi-hop UDP.

## Handoff notes

Prefer small, reviewable commits grouped by model/config repair, tests, manifest corrections, and documentation regeneration. Do not combine demotions with unrelated feature additions. If audit findings reveal a high-severity runtime defect, fix it with a regression test before finalizing the parity classification.