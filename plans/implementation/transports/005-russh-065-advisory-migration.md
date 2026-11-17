# Transports M005 — `russh` 0.65 Advisory Migration

Status: implemented

Repository baseline: `feat/auto-20261116-652bb64d` at `ba64e16` (workspace 1.0.12)

Source roadmaps:

- `plans/subsystems/transports-roadmap.md` (new milestone M005)
- `plans/subsystems/security-hardening-roadmap.md` (S2)

Long-term requirements:

- `plans/000-long-term-specification.md` §4 (invariants 1–3, 7, 10)
- `plans/001-terminology-and-domain-model.md` §3 (policy-scoped transport reuse)

Applicable ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

Primary class: invariant (optional feature)

## 1. Objective

Migrate `eggress-transport-ssh` from `russh 0.62.6` to the fixed `0.65.x`
line and remove RUSTSEC-2026-0492 (client handler callbacks for channels the
client never opened, CVSS 7.5) and RUSTSEC-2026-0495 (agent signing without
confirm), plus the `crypto-bigint`/`ctutils` timing advisories pulled through
`russh`/`rsa`.

The migration must preserve the established SSH compatibility boundary:
known-hosts verification by default, insecure host-key acceptance only in the
explicit `ssh` + `pproxy-compat` compatibility cache, and only direct TCP,
remote Unix-socket, and explicitly requested remote TCP forwarding.

## 2. Why this milestone is ready

- `russh 0.65.2` is published and requires Rust 1.89, matching the workspace
  MSRV.
- `russh 0.65.2` still exposes the client APIs this crate uses:
  `client::connect_stream`, `client::Handle`,
  `channel_open_direct_tcpip`, `channel_open_direct_streamlocal`,
  `authenticate_password`, and `authenticate_publickey`.
- The breaking changes are localized to the `client::Handler` trait and key
  types; the crate owns that adapter in one file
  (`crates/eggress-transport-ssh/src/lib.rs`).
- SSH is optional (`ssh` feature); the default CLI/Python graph is unaffected,
  so this can land after the default-graph TLS fix.

`cargo update` alone only moves `russh 0.62.6 -> 0.62.7`, which does not carry
the fixed versions; the workspace pin at `Cargo.toml:93` must be changed.

## 3. Current implementation evidence

- Workspace pin: `russh = { version = "0.62.6", default-features = false,
  features = ["ring", "flate2", "rsa"] }` (`Cargo.toml:93`).
- `CompatClient` implements `russh::client::Handler`
  (`crates/eggress-transport-ssh/src/lib.rs:129-200`).
- `check_server_key` currently takes `&russh::keys::PublicKey` and returns
  `Result<bool, Self::Error>`; 0.65 takes `&PublicKeyOrCertificate` and returns
  a future.
- `server_channel_open_forwarded_tcpip` uses `russh::client::ChannelOpenHandle`
  (`lib.rs:176-200`); 0.65 retains this type but the trait method is async.
- Session auth uses `authenticate_password` / `authenticate_publickey`
  (`lib.rs:529-570`); key loading uses `russh::keys::load_secret_key`.
- Host-key checks use `russh::keys::check_known_hosts` and
  `check_known_hosts_path` (`lib.rs:136-166`).
- `SshAuth` already has a redacting `Debug` (`lib.rs:49-56`), and errors avoid
  echoing password/private-key paths.
- The OpenSSH regression suite is
  `cargo test -p eggress-transport-ssh --test openssh -- --nocapture` and the
  embed-level required regression is
  `EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked
  --no-default-features --features ssh,pproxy-compat --test ssh -- --nocapture`.

## 4. Invariants that must not regress

- Native/default SSH verifies host keys through `KnownHosts`; missing or
  unreadable known-hosts files reject the host key.
- `SshHostKeyPolicy::InsecureCompatibility` remains available only with the
  `pproxy-compat` feature and is scoped to its own cache; it never authorizes a
  session created under `KnownHosts`.
- Passwords and private-key paths never appear in `Debug`, errors, diagnostics,
  or evidence output.
- Only direct TCP, remote Unix-socket, and explicitly requested remote TCP
  forwarding are exposed. Remote command, SFTP, agent forwarding, and implicit
  listener exposure remain unavailable.
- The SSH session cache remains bounded, policy-keyed, and cleared on service
  shutdown.
- No new SSH listener role or auth mechanism is added.
- No pproxy compatibility claim changes.

## 5. Scope

### In scope

- Workspace `russh` pin and lockfile update to `0.65.x`.
- `crates/eggress-transport-ssh/src/lib.rs` adapter migration.
- `crypto-bigint`/`ctutils` resolution through the updated `russh`/`rsa` graph.
- Focused SSH unit tests, OpenSSH regression, and feature-slice checks.
- Documentation updates for any changed API/behavior.

### Explicitly out of scope

- SSH listener support.
- New auth methods (certificates, agent forwarding, keyboard-interactive).
- Changing host-key policy defaults.
- TLS/QUIC/tokio upgrades (Transports M004).
- Release/tag/publish.

## 6. Required production changes

### A. Bump the dependency

Change `Cargo.toml:93` to a `0.65.x` line (prefer `0.65.2` or the latest
`0.65.x` patch) while keeping `default-features = false` and the existing
`ring`, `flate2`, and `rsa` features. Do not enable `aws-lc-rs`; the workspace
is ring-only.

Run `cargo update -p russh` and verify the lock resolves `russh >= 0.65.0`,
`crypto-bigint`/`ctutils` at fixed versions, and no duplicate `russh` line.

### B. Adapt `CompatClient`

Migrate the `russh::client::Handler` implementation:

- `check_server_key` must accept `&russh::keys::PublicKeyOrCertificate` and
  return the new future type. Extract the public key (reject certificate
  variants unless the current behavior intentionally supports them) before
  calling `check_known_hosts`/`check_known_hosts_path`.
- Keep the exact policy branch: `KnownHosts`, `KnownHostsFile`, and
  `InsecureCompatibility`. Preserve the warning on new insecure sessions.
- Adapt `server_channel_open_forwarded_tcpip` and any `ChannelOpenHandle`
  accept/reject calls to the 0.65 signature.
- Adapt `connect_stream`, `authenticate_*`, and channel-open calls only where
  the compiler requires it; do not replace the session model.

### C. Keep the compatibility boundary

Do not use the migration as an opportunity to enable agent forwarding, remote
commands, SFTP, or certificate-based host keys. Keep the existing feature gates
and the cache policy key.

### D. Verification

Run both the crate-level OpenSSH regression and the embed feature-boundary
regression. If OpenSSH tools are unavailable, record the skip; CI installs
`openssh-server` and treats the embed test as required.

## 7. Ordered work packages

### Work package A — Pin, compile, and resolve the fixed graph

Intent: get the new API compiling with no behavior change.

Acceptance evidence: `cargo check` for `eggress-transport-ssh`,
`eggress-embed` with `ssh` and `ssh,pproxy-compat`, and the CLI feature slice;
`cargo tree -i russh` shows `0.65.x`; `cargo audit` no longer reports the SSH
advisories.

### Work package B — Host-key and channel API adaptation

Intent: preserve the known-hosts and forwarded-channel policy under the new
trait signatures.

Acceptance evidence: unit tests for known_hosts file policy, insecure policy
scoping, rejected host key, and forwarded channel accept/reject.

### Work package C — OpenSSH regression and redaction

Intent: prove real-server compatibility and no secret leak.

Acceptance evidence: OpenSSH tests green (or explicitly skipped with reason);
`error_redaction` remains green; `SshAuth`/session-key Debug still redacted.

## 8. Failure, cancellation, restart, and contention semantics

- Host-key rejection fails the connection; it never falls back to
  `InsecureCompatibility`.
- Cache lookup remains policy-keyed; a session authenticated under one policy is
  never reused under another.
- Cancellation during authentication/channel open drops the session without
  leaving a cache entry.
- Concurrent cache access remains mutex-protected and bounded.
- Shutdown clears cached sessions as before.

## 9. Compatibility and migration

- The `ssh` feature remains opt-in; default CLI/embed/Python feature sets are
  unchanged.
- No pproxy tier changes. If the migration changes observable SSH behavior
  (for example certificate host keys), update the manifest/evidence or preserve
  the old behavior.
- The public `SshHostKeyPolicy`/`SshAuth` types remain source-compatible where
  possible; `russh` types are not part of the public Eggress API.

## 10. Required tests

### Focused unit tests

- `KnownHosts` missing/unreadable file rejects.
- `KnownHostsFile` accepts a matching key and rejects a mismatch.
- `InsecureCompatibility` is only constructed under the compatibility feature
  and warns.
- Session cache keys include the policy and cannot cross-reuse.
- Forwarded-channel accept/reject still observes the configured limits.
- `SshAuth`/session-key `Debug` contains `****`.

### Integration tests

- `eggress-transport-ssh --test openssh`.
- `eggress-embed --test ssh` with `ssh,pproxy-compat`.
- Outbound chain execution through an SSH hop (existing runtime tests).

### Security and negative tests

- Host-key mismatch fails closed.
- No password/private-key path in errors.
- No new forwarding surface (remote command/SFTP) is reachable.

## 11. Required verification commands

```bash
cargo test -p eggress-transport-ssh
EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked \
  --no-default-features --features ssh,pproxy-compat --test ssh -- --nocapture
cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-cli --locked --no-default-features \
  --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo audit --ignore RUSTSEC-2023-0071
```

Do not use `--all-features` (drags in test-only `insecure-quic`).

## 12. Documentation updates

- `architecture/transports-ssh-quic-h3.md` — API/session notes if changed.
- `docs/SECURITY_REVIEW.md` — SSH host-key policy statement remains
  accurate; note the fixed `russh` line.
- `docs/RUST_API.md` only if a public SSH type/signature changes.
- `plans/subsystems/transports-roadmap.md` status table.

## 13. Acceptance criteria

- Workspace resolves `russh >= 0.65.0`; `cargo audit` no longer reports
  RUSTSEC-2026-0492/-0495 or the `crypto-bigint`/`ctutils` SSH path.
- `ssh` and `ssh,pproxy-compat` compile; OpenSSH regression is green where
  required.
- Known-hosts default and insecure compatibility scoping are unchanged.
- No credential/private-key leak appears in Debug/errors.
- No new forwarding surface is exposed.
- Default feature graph and pproxy claims are unchanged.

## 14. Stop conditions

Stop and report if:

- `russh 0.65.x` requires a redesign of the session/cache model;
- certificate host keys cannot be handled without changing policy;
- the API migration would require exposing `russh` types in Eggress's public API;
- OpenSSH regression cannot be run and the release gate requires it;
- MSRV 1.89 is insufficient;
- the upgrade pulls an aws-lc/C dependency.

## 15. Closure evidence required

- Dependency diff and `cargo tree -i russh`.
- `cargo audit`/`cargo deny` output for the SSH graph.
- OpenSSH/embed regression output, including skip reasons if applicable.
- Unit tests for host-key policy, cache scoping, and Debug redaction.
- Feature-slice compile output.
- Documentation diff and explicit no-release statement.

## 16. Handoff notes

- SSH is optional; do not block the default TLS fix on this plan.
- Preserve the exact `KnownHosts` fail-closed behavior; do not “simplify” it to
  `known_hosts=None`.
- Keep the `ring` feature set; do not accept `russh` defaults that pull
  `aws-lc-rs`.
- The OpenSSH test fixture may skip only when tools are genuinely unavailable;
  CI installs `openssh-server` and requires it.
