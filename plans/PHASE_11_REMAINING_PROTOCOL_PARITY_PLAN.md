# Phase 11 Detailed Plan: Remaining `pproxy` Protocol Parity

## Purpose

Phase 11 closes or explicitly classifies every remaining protocol gap after HTTP/SOCKS/Trojan/Shadowsocks TCP/UDP work. The output is not necessarily “implement every protocol.” The output is that every protocol in the Phase 7 pproxy parity spec is either implemented as compatible, marked partial with a follow-up plan, or declared intentional non-parity with rationale.

This phase prevents the project from claiming true pproxy parity while unexamined protocol gaps remain.

---

# Prerequisites

Required:

- Phase 7 parity spec exists;
- Phase 8 CLI/URI compatibility exists or is in progress;
- Phase 9 Shadowsocks TCP decision is complete;
- Phase 10 Shadowsocks UDP decision is complete;
- `docs/PARITY_MATRIX.md` has all protocol rows.

---

# Non-goals

Do not implement:

- Python bindings;
- PyPI packaging;
- broad scheduler refactors;
- transparent proxying unless parity spec marks it required and safe;
- unsafe Rust;
- native TLS/OpenSSL;
- public-internet-dependent tests.

---

# Workstream 1: Protocol gap audit

## Goal

Create a definitive list of remaining pproxy protocols and transports.

## Required output

Update:

```text
docs/PPROXY_PARITY_SPEC.md
docs/PARITY_MATRIX.md
```

Add a section:

```markdown
## Remaining protocol audit
```

For each protocol/scheme, include:

- scheme name;
- pproxy role: inbound, upstream, both;
- TCP/UDP support;
- auth/encryption behavior;
- current Eggress status;
- decision: implement, partial, intentional non-parity, unsupported;
- rationale.

## Candidate protocol classes

The exact list must come from Phase 7. Likely candidates include:

- TLS/SSL wrapping variants;
- SSH upstream;
- WebSocket/HTTP transport variants if pproxy supports them;
- additional Shadowsocks methods;
- legacy ciphers;
- simple/tunnel protocols;
- any URI aliases pproxy accepts.

## Acceptance criteria

- No pproxy protocol row remains unclassified.

---

# Workstream 2: Decision framework

## Goal

Avoid implementing obsolete or unsafe protocols by default.

## Required decision categories

For each remaining protocol:

### Implement now

Use only if:

- protocol is common in pproxy usage;
- behavior is clear;
- implementation can be secure;
- local tests are feasible;
- dependencies fit policy.

### Defer

Use if:

- protocol is useful but large;
- needs separate phase;
- current pproxy parity claim can exclude it honestly.

### Intentional non-parity

Use if:

- protocol is obsolete/weak;
- requires unacceptable native deps;
- requires unsafe behavior;
- behavior is ambiguous and low-value;
- conflicts with Eggress security model.

## Acceptance criteria

- Every decision includes rationale and user-facing diagnostic behavior.

---

# Workstream 3: Implement selected lightweight protocols or aliases

## Goal

Close small, high-value gaps identified by the audit.

## Examples of acceptable work

- URI scheme aliases that map to existing protocols;
- TLS wrapping aliases that map to existing `+tls` behavior;
- pproxy-compatible auth syntax that maps to existing auth;
- simple TCP tunnel aliases if they map directly to existing direct/upstream behavior.

## Requirements

For each implemented item:

- add parser support;
- add config/URI support;
- add capability matrix row;
- add unit tests;
- add runtime test if it affects traffic;
- add differential test if pproxy-equivalent.

## Acceptance criteria

- Lightweight compatibility improvements do not change core safety policy.

---

# Workstream 4: Implement selected medium protocols only with full plan

## Goal

If the audit identifies a medium-complexity protocol that must be implemented in Phase 11, use the full protocol checklist.

## Required checklist per protocol

1. Spec document section.
2. Wire format parser/encoder.
3. Client/upstream path.
4. Server/inbound path if needed.
5. Auth handling.
6. Timeout handling.
7. Error mapping.
8. Metrics.
9. Runtime test.
10. Differential test or documented reason not possible.
11. Security review update.

## Acceptance criteria

- No medium protocol is half-wired or docs-only.

---

# Workstream 5: Intentional non-parity diagnostics

## Goal

Unsupported pproxy features should fail clearly.

## Required behavior

For CLI/URI compatibility mode:

- unsupported scheme -> `UnsupportedFeature` with scheme name;
- unsupported transport -> clear error;
- insecure legacy cipher -> clear error with rationale;
- unsupported multi-hop UDP -> clear error;
- ambiguous syntax -> structured parse error.

Errors must redact credentials.

## Tests

- unsupported SSH if not implemented;
- unsupported legacy Shadowsocks method if not implemented;
- unsupported obfs/plugin syntax;
- unsupported UDP protocol combination;
- malformed chain syntax.

## Acceptance criteria

- No unsupported protocol silently falls back to direct or a different protocol.

---

# Workstream 6: Differential tests for implemented items

## Goal

Each newly compatible protocol/alias should have a pproxy comparison where feasible.

## Required tests

Extend gated suite for any implemented item:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

Compare:

- payload success for working paths;
- coarse failure class for unsupported/failure paths;
- auth behavior where relevant.

## Acceptance criteria

- Every “compatible” label has a test reference.

---

# Workstream 7: Documentation refresh

## Required updates

- `docs/PARITY_MATRIX.md`;
- `docs/PPROXY_PARITY_SPEC.md`;
- `docs/CONFIG_REFERENCE.md`;
- `docs/PPROXY_MIGRATION.md`;
- `docs/SECURITY_REVIEW.md`;
- README support table.

## Required content

- final protocol status;
- intentional non-parity table;
- unsupported diagnostics;
- migration guidance;
- differential coverage summary.

---

# Recommended commit sequence

1. Protocol gap audit and decision matrix.
2. Implement lightweight aliases/mappings.
3. Implement any selected medium protocol with tests.
4. Add unsupported diagnostics/tests.
5. Add differential tests.
6. Update docs and completion record.

---

# Required verification

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Optional/gated:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

---

# Definition of done

Phase 11 is complete only when:

1. Every pproxy protocol/scheme is classified.
2. Implemented protocols have runtime tests.
3. Compatible protocols have differential tests or documented exception.
4. Unsupported features produce precise diagnostics.
5. Intentional non-parity is documented.
6. No feature silently falls back incorrectly.
7. Security/dependency policy remains intact.
8. Docs and parity matrix are current.
9. Workspace checks pass locally.

## Completion record

Add:

```text
docs/PHASE_11_REMAINING_PROTOCOL_PARITY_COMPLETION.md
```

Include implemented items, deferred items, intentional non-parity, tests, and blockers for Phase 12.
