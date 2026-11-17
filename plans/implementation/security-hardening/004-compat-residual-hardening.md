# Security Hardening Milestone 004 — Compat residual hardening without parity break

Status: closed

Repository baseline: `ba64e166474ede7caeba736a885322323d965911` (workspace 1.0.12 / `origin/main`)

Source roadmap:

- `plans/subsystems/security-hardening-roadmap.md` (Milestone 004)

Long-term requirements:

- `plans/000-long-term-specification.md` §3 (pproxy practical compatibility)
- `plans/001-terminology-and-domain-model.md`

Applicable ADRs:

- `plans/adrs/ADR-0002-native-compat-security-hardening.md`

Primary class: polish / infrastructure

Audit findings addressed: **#2** (standalone `-ul` UDP without TCP auth), **#3** (`--auth` IP reuse → `AUTH_NONE`), **#11** (`legacy-crypto` / `insecure-tls` / `insecure-quic` residual risk).

## 1. Objective

Improve operator visibility and optional caps for parity-locked residual risks—without changing standalone UDP wire auth, without forcing SOCKS `0x02` after reuse cache hits, and without enabling insecure features by default.

## 2. Why this milestone is ready

- Standalone UDP and auth reuse are documented intentional compat behaviors (`PPROXY_COMPAT_SECURITY_DIFFERENCES.md`; manifest `cli.ul` matched, `cli.auth` supported_difference).
- Security warning channels exist (stronger after M001 if landed).
- Insecure features are already off CLI default `full`; legacy-crypto carries warnings in-tree.

## 3. Current implementation evidence

- `-ul` → `standalone_pproxy_udp` with `client_pin` (architecture/udp.md).
- `UsernamePasswordWithReuse` + peer IP cache → may select `AUTH_NONE` (`eggress-server` accept handlers); default reuse window ~30 days in pproxy-compat args.
- Feature flags: `legacy-crypto`, `insecure-tls`, `insecure-quic` omitted from default CLI `full` (AGENTS.md).

Gap: non-loopback standalone UDP may only be lightly warned; large auth reuse TTLs not capped/warned; insecure-feature residual risk is easy to miss in operator threat models.

## 4. Invariants that must not regress

- Standalone UDP remains usable without a TCP control channel (parity).
- Auth reuse IP-scoped behavior remains available for pproxy `--auth` (parity / supported_difference).
- Default feature sets do not grow insecure-* / legacy-crypto.
- No oracle claim silently diverges.

## 5. Scope

### In scope

- Security warnings (and optional fail-closed via M001 flag) for `standalone_pproxy_udp` on non-loopback binds.
- Warn and/or soft-cap extremely large auth reuse TTLs; document NAT/shared-IP risk; **do not** change post-cache method selection to always `0x02`.
- Docs polish for insecure feature residual risk; ensure startup/build warnings remain visible for legacy-crypto when enabled.
- Threat model / hardening guide updates tying #2/#3/#11 to operator controls (bind, network ACL, prefer `socks5_udp_associate`, native auth without reuse).

### Explicitly out of scope

- New UDP shared-secret/DTLS protocol (**D**).
- Changing reuse handshake to always require username/password method.
- Retiering manifest claims (requires separate ADR + oracle suite).
- Enabling or shipping insecure features in default builds.

## 6. Required production changes

### Server/runtime/config

- Extend `validate_config_security` (or UDP compile validate) for standalone non-loopback warnings.

### Compat translation and manifest

- Optional warn on huge `--auth` timeout values at translate/check time.
- Manifest tiers unchanged; notes may be clarified if needed without status flips.

### Delivery

- CLI check surfaces new warnings; exit codes honor M001 strict mode if present.

### Documentation and static guards

- `PPROXY_COMPAT_SECURITY_DIFFERENCES.md`, `THREAT_MODEL.md`, `HARDENING_GUIDE.md`, `SECURE_CONFIGURATION.md`, skill updates.

## 7. Ordered work packages

### Work package A — Standalone UDP non-loopback warning

Intent: Make open UDP relay risk visible at check/startup.

Acceptance evidence: config validate tests; docs cross-link to prefer associate mode.

### Work package B — Auth reuse TTL warn/cap (non-semantic)

Intent: Reduce accidental 30-day windows without breaking reuse protocol.

Acceptance evidence: pproxy-compat args tests; accept-handler reuse tests still pass unchanged for method selection.

### Work package C — Insecure feature residual docs

Intent: Threat model lists opt-in crypto/TLS/QUIC bypass features as operator risk.

Acceptance evidence: docs updated; no Cargo feature default changes.

## 8. Failure, cancellation, restart, and contention semantics

- Warnings are compile/startup time; they do not alter in-flight sessions mid-reload beyond existing snapshot rules.
- Auth reuse cache bounds and expiry tests remain authoritative for concurrency.

## 9. Compatibility and migration

- No wire or default behavioral change for `-ul` or `--auth` handshake selection.
- If a soft TTL cap is introduced, document the cap and preserve escape hatch only with explicit acknowledgment—or prefer warn-only to avoid parity arguments.

## 10. Required tests

### Focused unit tests

- Security warning emitted for non-loopback standalone UDP.
- Auth timeout warn/cap unit tests in pproxy-compat.

### Security and negative tests

- Reuse still selects AUTH_NONE on cache hit (parity lock).
- Default features exclude insecure-*.

### Migration and compatibility tests

- Existing standalone UDP and auth reuse suites green.

## 11. Required verification commands

```bash
cargo test -p eggress-config
cargo test -p eggress-pproxy-compat
cargo test -p eggress-server
cargo test -p eggress-udp
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Do **not** run external oracle suites unless a claim status changes (forbidden in this milestone).

## 12. Documentation updates

- `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md`
- `docs/security/THREAT_MODEL.md`
- `docs/security/HARDENING_GUIDE.md`
- `docs/security/SECURE_CONFIGURATION.md`
- `.skills/security-dev/skill.md`

## 13. Acceptance criteria

- Non-loopback standalone UDP produces a clear security warning (fatal only if operator enabled fail-closed from M001).
- Auth reuse residual risk is documented; optional TTL warn/cap does not change method selection semantics.
- Insecure feature residual risk is explicit in threat/hardening docs.
- Manifest claim statuses unchanged.

## 14. Stop conditions

- Request to require TCP auth on `-ul` or always-`0x02` after reuse without a new ADR + manifest retier.
- Proposal to enable legacy-crypto in default `full`.

## 15. Closure evidence required

- Commits/PR; findings #2/#3/#11 matrix; proof manifest statuses unchanged; test commands; residual risks explicitly listed as parity-locked.

## 16. Handoff notes

- This milestone is mostly diagnostics + docs; keep code diffs small.
- If M001 is not yet merged, implement warnings as non-fatal first and note strict-mode integration as follow-up in closure.
