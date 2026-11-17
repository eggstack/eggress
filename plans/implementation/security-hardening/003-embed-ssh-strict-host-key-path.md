# Security Hardening Milestone 003 — Embed SSH strict host-key path

Status: closed

Repository baseline: `ba64e166474ede7caeba736a885322323d965911` (workspace 1.0.12 / `origin/main`)

Source roadmap:

- `plans/subsystems/security-hardening-roadmap.md` (Milestone 003)

Long-term requirements:

- `plans/000-long-term-specification.md` §2–§3 (transports, pproxy practical compatibility)
- `plans/001-terminology-and-domain-model.md` (OutboundConnector)

Applicable ADRs:

- `plans/adrs/ADR-0002-native-compat-security-hardening.md`

Primary class: capability

Audit findings addressed: **#4** (`from_pproxy_uri` always `InsecureCompatibility`), **#12** (`EGRESS_SSH_INSECURE_HOST_KEYS` operator misuse).

## 1. Objective

Provide an **additive** embed/outbound construction path that uses verified SSH known-hosts policy, while preserving `OutboundConnector::from_pproxy_uri()` compatibility host-key behavior, and clarify/harden runtime env-gate misuse messaging.

## 2. Why this milestone is ready

- `SshHostKeyPolicy` already distinguishes `KnownHosts` / `KnownHostsFile` / `InsecureCompatibility`.
- `from_toml()` already uses verified policy; `from_pproxy_uri()` uses `SshSessionCache::new_compatibility()` by design (`docs/EMBED_API.md`, AGENTS.md).
- Runtime supervisor already gates insecure keys with `EGRESS_SSH_INSECURE_HOST_KEYS` + compatibility mode.
- OpenSSH regression harness exists for embed/transport-ssh.

## 3. Current implementation evidence

- `crates/eggress-outbound/src/connector.rs` — `from_pproxy_uri` → compatibility executor/SSH cache.
- `crates/eggress-transport-ssh/src/lib.rs` — policy enum; insecure policy accepts any host key.
- `crates/eggress-runtime` — `ssh_insecure_acknowledged()` for compat startup only.
- Python: `OutboundConnector.from_pproxy_uri` mirrors Rust.

Gap: no first-class `from_pproxy_uri_strict` / host-key policy parameter; embed path ignores the runtime env gate (intentional today, but easy to misuse); docs warn but API discoverability is weak.

## 4. Invariants that must not regress

- `from_pproxy_uri()` default SSH host-key behavior remains insecure-compat when `ssh` + `pproxy-compat` are enabled.
- Native TOML / `from_toml` / `from_chain` never weaken to insecure policy because `pproxy-compat` is enabled.
- Passwords/private-key paths never appear in Debug/errors/diagnostics.
- SSH session cache lifetime and unpooled nested-hop rules unchanged (`architecture/outbound.md`).
- Enabling `pproxy-compat` never weakens native SSH.

## 5. Scope

### In scope

- Additive API (name bikeshed allowed): e.g. `from_pproxy_uri_with_host_key_policy(...)`, builder flag, or `from_pproxy_uri_strict(...)` that requires KnownHosts/KnownHostsFile.
- Embed + Python mirror if the method is part of the published embed/Python surface (`docs/RUST_API.md` / `docs/EMBED_API.md` / Python bindings).
- Runtime: clearer fail/warn if `EGRESS_SSH_INSECURE_HOST_KEYS` is set outside compatibility mode (optional fail-closed); docs/runbook text.
- OpenSSH tests proving strict path rejects unknown keys and compat path still accepts (feature-gated).

### Explicitly out of scope

- Changing `from_pproxy_uri` default to KnownHosts.
- SSH listeners, SFTP, agent forwarding, remote command execution.
- M001/M002 config knobs.

## 6. Required production changes

### Protocols/transports/chain

- `eggress-outbound` construction paths; thin embed re-export.
- No change to wire SSH algorithms beyond policy selection.

### Delivery (CLI/embed/Python/release)

- Python binding exposure if applicable; keep feature gates `ssh`, `pproxy-compat`.
- Update EMBED_API + RUST_API qualification notes (documentation only for existing items; new methods are additive).

### Documentation and static guards

- `docs/EMBED_API.md`, `architecture/outbound.md`, `architecture/transports-ssh-quic-h3.md`, `.skills/security-dev/skill.md`, threat/hardening docs cross-links.

## 7. Ordered work packages

### Work package A — Additive Rust API

Intent: Strict construction without changing default `from_pproxy_uri`.

Acceptance evidence: unit tests for policy selection; compile feature slices per CI_STATUS.

### Work package B — Embed/Python surface + OpenSSH regression

Intent: Public facade parity; required-mode OpenSSH test covers strict reject + compat accept.

Acceptance evidence: `EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed ...` (and/or transport-ssh openssh) as appropriate.

### Work package C — Runtime env misuse + docs

Intent: Operators understand env gate applies to runtime compat, not as a silent global; optional harden if env set in native mode.

Acceptance evidence: runtime test + doc updates.

## 8. Failure, cancellation, restart, and contention semantics

- Strict construction fails closed at connector build or first SSH hop when host key verification fails.
- Cached SSH sessions remain policy-scoped; strict and compat connectors must not share an insecure cache.
- Shutdown clears connector-owned SSH cache (existing invariant).

## 9. Compatibility and migration

- Additive API only; existing callers unchanged.
- No manifest change for SSH URI capabilities unless behavior of default `from_pproxy_uri` changes (forbidden here).
- Document migration: security-sensitive embedders switch to strict API or `from_toml`.

## 10. Required tests

### Focused unit tests

- Policy selection on construct; redaction on auth failure.

### Integration / OpenSSH tests

- Strict path rejects untrusted host key.
- Compat `from_pproxy_uri` still accepts under insecure policy.

### Security and negative tests

- No password/path leakage; feature-off builds reject SSH URIs as today.

### Migration and compatibility tests

- Existing `from_pproxy_uri` tests remain green.

## 11. Required verification commands

```bash
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat
EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat --test ssh -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Skip OpenSSH required mode only when documenting fixture unavailability for local optional runs; CI must keep tools installed.

## 12. Documentation updates

- `docs/EMBED_API.md`
- `docs/RUST_API.md` (qualification note for new method)
- `docs/security/HARDENING_GUIDE.md` / `THREAT_MODEL.md` cross-link
- `architecture/outbound.md`, `architecture/transports-ssh-quic-h3.md`
- `.skills/security-dev/skill.md`

## 13. Acceptance criteria

- Embedders can build a pproxy-URI-derived or equivalent chain with verified host keys without using insecure compat policy.
- `from_pproxy_uri` default behavior and docs still describe insecure compat SSH.
- Runtime env-gate semantics are tested/documented; native mode does not silently enable insecure keys.
- No parity claim retier.

## 14. Stop conditions

- Pressure to change `from_pproxy_uri` default for “security” without manifest work.
- Public API rename/removal of existing methods (forbidden by RUST_API stability rule).
- OpenSSH fixtures unavailable in CI (must not merge by weakening required CI).

## 15. Closure evidence required

- Commits/PR; findings #4/#12 matrix; feature-slice checks; OpenSSH evidence; docs diff summary; residual risk: default `from_pproxy_uri` still MITM-capable by design.

## 16. Handoff notes

- Prefer explicit policy parameter over a proliferating set of constructors if both URI and TOML need the same knob.
- Python surface must not invent a second SSH stack.
- Never log host-key material or private key paths in failure messages.
