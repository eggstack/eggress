# Security Hardening Milestone 005 — Update trust hardening

Status: closed

Repository baseline: `ba64e166474ede7caeba736a885322323d965911` (workspace 1.0.12 / `origin/main`)

Source roadmap:

- `plans/subsystems/security-hardening-roadmap.md` (Milestone 005)

Long-term requirements:

- `plans/000-long-term-specification.md`
- `plans/002-long-term-roadmap.md` (delivery / release automation as post-milestone work)

Applicable ADRs:

- `plans/adrs/ADR-0002-native-compat-security-hardening.md`
- **Required before implementing finding #9 authenticity scheme:** a new delivery ADR selecting cosign / minisign / TUF / platform code signing (not accepted yet)

Primary class: capability / delivery

Audit findings addressed: **#10** (`EGRESS_UPDATE_BASE_URL` env redirect), **#9** (self-update = HTTPS + SHA-256 only; no signing/TUF).

## 1. Objective

Eliminate easy production abuse of `EGRESS_UPDATE_BASE_URL`, and either (a) ship authenticity beyond SHA-256 sidecars for `eggress update` or (b) close #10 with an explicit, accepted deferral of #9 that keeps threat-model docs honest.

## 2. Why this milestone is ready

- Updater ownership is localized under `crates/eggress-cli/src/update/` with offline fixtures and `release_contract` tests.
- Delivery M003 already adopted eggup archive/pair mechanics; integrity policy remains Eggress-owned (SHA-256 sidecar + staged version checks).
- Finding #10 is small and unblocked; finding #9 is large and needs key custody + scheme ADR.

## 3. Current implementation evidence

- `update/mod.rs` reads `EGRESS_UPDATE_BASE_URL` for discovery/download override (fixture/offline support).
- `verify.rs` — SHA-256 sidecar verification; staged `eggress version` / `pproxy --version` equality with release tag.
- Install scripts also SHA-256 (`packaging/install.sh`, release_contract).
- Module docs state HTTPS release fetch; no code signing/TUF.

Gap: base URL override is available whenever the env var is set; authenticity equals TLS to GitHub + checksum + version checks.

## 4. Invariants that must not regress

- GitHub Releases remains binary update authority unless a future ADR changes distribution.
- `eggress` + `pproxy` remain one version-aligned pair; no mixed-generation success.
- Pre-mutation failure leaves the live pair untouched; rollback semantics unchanged.
- Offline fixture-driven tests remain first-class for CI.
- No Cargo/source fallback, telemetry, or implicit privilege escalation.
- SHA-256 remains integrity evidence; do not market it as authenticity until signing lands.

## 5. Scope

### In scope

**Phase A (required):**

- Gate `EGRESS_UPDATE_BASE_URL` behind `cfg(test)` and/or a dual acknowledgment (e.g. second env `EGRESS_UPDATE_ALLOW_BASE_URL_OVERRIDE=1`) and/or require `file://` only for fixtures.
- Update unit/offline fixture tests and docs (`INSTALLATION.md`, update module docs).

**Phase B (authenticity — choose one outcome):**

- **Outcome B1 — Implement:** After a new ADR, integrate cosign/minisign/TUF (or platform signing) into release + `eggress update` verify path; pin trust root; update release-binaries workflow and install scripts as needed.
- **Outcome B2 — Defer with honesty:** Write ADR or closure deferral stating #9 remains HTTPS+SHA-256; update `THREAT_MODEL.md` / `INSTALLATION.md` with compromised-release-account residual risk; no pretend authenticity.

### Explicitly out of scope

- Changing non-update CLI behavior or pproxy runtime.
- Automated crates.io publish.
- Replacing eggup archive/pair transaction layer.
- Findings #1–#8, #11–#12.

## 6. Required production changes

### Delivery (CLI/embed/Python/release)

- `eggress-cli` update discovery/download/verify.
- Possibly `release-binaries.yml` / packaging scripts for Phase B1.
- `release_contract` and update offline tests.

### Documentation and static guards

- `docs/INSTALLATION.md`, security threat model, update module rustdoc, delivery roadmap cross-link.

## 7. Ordered work packages

### Work package A — Gate base URL override (#10)

Intent: Production processes cannot redirect updates via a single ambient env var.

Required changes: enforce dual gate or cfg(test); keep fixture story.

Acceptance evidence: tests prove override rejected without acknowledgment; fixtures still pass.

### Work package B — Authenticity scheme ADR (#9 gate)

Intent: Choose trust model and key custody before code.

Acceptance evidence: accepted ADR under `plans/adrs/` **or** explicit Outcome B2 deferral record.

### Work package C — Implement or defer (#9)

Intent: B1 implementation with verify path + release evidence, or B2 doc-only closure of residual risk.

Acceptance evidence: matching Outcome B1/B2 checklist in closure record.

## 8. Failure, cancellation, restart, and contention semantics

- Override rejection happens before network mutation.
- Verify failures never install candidates.
- Concurrent `eggress update` behavior remains as today (no new multi-writer protocol).

## 9. Compatibility and migration

- Operators using `EGRESS_UPDATE_BASE_URL` in production (unsupported) must adopt the dual gate or stop.
- Signing (B1) must remain backward compatible for a transition: old releases without signatures either fail closed after a flag day or accept checksum-only with explicit `--allow-checksum-only` — decide in the signing ADR, do not invent silently here.
- No pproxy runtime migration.

## 10. Required tests

### Focused unit tests

- Base URL override rejected without gate; allowed with gate/fixtures.

### Integration tests

- Existing offline update success + checksum mismatch suites.

### Security and negative tests

- Non-HTTPS URLs still rejected; path traversal / oversize archives still bounded.

### Migration and compatibility tests

- If B1: signed artifact happy path + tampered signature fail closed.

## 11. Required verification commands

```bash
cargo test -p eggress-cli --locked update::
cargo test -p eggress-cli --locked --test release_contract
cargo test -p eggress-cli --locked --test cli_exit_codes
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

For B1, add release-pipeline dry-run evidence as specified by the signing ADR (do not claim unrun workflows in closure).

## 12. Documentation updates

- `docs/INSTALLATION.md`
- `docs/security/THREAT_MODEL.md`
- `crates/eggress-cli/src/update/mod.rs` rustdoc
- `plans/subsystems/delivery-roadmap.md` cross-link if authenticity becomes ongoing delivery work
- Hardening guide note for operators who pin installers instead of self-update

## 13. Acceptance criteria

- #10 closed: override cannot redirect production updates without explicit dual acknowledgment or compile-time test gating.
- #9 either implemented per signing ADR (B1) or explicitly deferred with threat-model honesty (B2).
- Pair update integrity invariants preserved.
- No marketing language that SHA-256 alone is authenticity if B2.

## 14. Stop conditions

- Implementing signatures without an accepted scheme ADR / key custody plan.
- Weakening checksum or version checks “temporarily” to land signing.
- Expanding into unrelated release automation (crates.io auto-publish).

## 15. Closure evidence required

- Commits/PR; findings #10/#9 matrix with B1 or B2 outcome; test commands; if B2, link to deferral ADR/record; residual supply-chain risks listed.

## 16. Handoff notes

- Land Work package A even if B1 is blocked on keys.
- Prefer aligning with existing GitHub Release asset layout; avoid second update channel.
- Redact any future signing material from logs and CI transcripts.
- Coordinate with delivery subsystem owners before changing `release-binaries.yml`.
