# Transports Milestone 005 — Closure Status

Status: closed

Source implementation plan:

- `plans/implementation/transports/005-russh-065-advisory-migration.md`

Source subsystem roadmap:

- `plans/subsystems/transports-roadmap.md` §7 Milestone 5, §12 milestone status
- `plans/subsystems/security-hardening-roadmap.md` Milestone S2

Repository baseline reviewed: `ba64e16` (branch `feat/auto-20261116-652bb64d`, workspace `1.0.12`)

Implementation commits or pull requests:

- None. This pass is uncommitted working-tree work in the shared corrective
  wave; no commit, tag, or publication was created by this milestone.

## 1. Executive finding

The optional SSH transport now resolves `russh 0.65.2`, `crypto-bigint 0.7.11`,
and `ctutils 0.4.8`. `cargo audit` no longer reports RUSTSEC-2026-0492,
RUSTSEC-2026-0495, RUSTSEC-2026-0387 (crypto-bigint), or RUSTSEC-2026-0385
(ctutils) for the SSH graph.

The breaking surface was smaller than the plan anticipated. The only
compiler-required adaptation was `russh::client::Handler::check_server_key`:
the parameter became `&russh::keys::PublicKeyOrCertificate` inside the existing
async RPITIT method. Session connect, authentication, channel open, remote
forward, cancellation, and cache logic compiled unchanged, so the session/cache
model was not touched.

The pre-0.65 crate could not even parse a certificate host key (the KEX host-key
blob was decoded with `parse_public_key`), so such a handshake failed before a
policy decision. `check_server_key` now rejects
`PublicKeyOrCertificate::Certificate(_)` with `Ok(false)` and a warning under
every policy, which preserves that fail-closed outcome rather than projecting a
certificate onto the certified plain key.

The policy branch moved verbatim into a private `verify_host_key()` helper so it
can be unit-tested without a live session; the decision logic, logging, and
`InsecureCompatibility` scoping are otherwise unchanged. Eight policy/redaction
unit tests were added (nine with `pproxy-compat`).

Real-server evidence is green: the crate OpenSSH suite (7 tests) and the required
embed regression with `ssh,pproxy-compat` (6 tests) both ran against a local
`sshd`, not a skip. No pproxy claim, manifest entry, public Eggress type, or
default feature set changed.

Closure is conditional on one item only: the coordinator's workspace-wide
`fmt`/`clippy`/`test` gate after all corrective waves land.

## 2. Requirement-to-evidence matrix

| Requirement | Evidence | Result | Notes |
|---|---|---|---|
| Workspace resolves `russh >= 0.65.0` | `cargo tree -i russh -p eggress-transport-ssh` → `russh v0.65.2` | pass | 0.65.2 exactly |
| Exactly one `russh` line in the graph | `grep -c 'name = "russh"' Cargo.lock` → `1`; embed `ssh` inverse tree shows only `0.65.2` | pass | No duplicate/leftover 0.62 line |
| `crypto-bigint >= 0.7.6` | lock `crypto-bigint 0.7.11`; inverse tree: `crypto-primes`/`elliptic-curve` → `rsa`/`ecdsa` → `russh 0.65.2` | pass | Was 0.7.5 (advisory + yanked) |
| `ctutils >= 0.4.4` | lock `ctutils 0.4.8`; inverse tree under `crypto-bigint 0.7.11` | pass | Was 0.4.2 |
| Ring-only crypto; no `aws-lc-rs` | `russh` pin keeps `default-features = false, features = ["ring", "flate2", "rsa"]`; `cargo check`/`clippy` resolved no `aws-lc-*` crate | pass | `deny.toml` still bans `aws-lc-sys` |
| `check_server_key` adapted to 0.65 | `crates/eggress-transport-ssh/src/lib.rs` `check_server_key` takes `&russh::keys::PublicKeyOrCertificate` | pass | Only compiler-required change |
| Certificate host keys fail closed | `lib.rs` `Certificate(_)` arm logs and returns `Ok(false)` before any policy branch | pass | Matches pre-0.65 handshake failure |
| `KnownHosts` default still fails closed | unit test `default_known_hosts_policy_rejects_unrecorded_host`; embed test `native_toml_connector_rejects_untrusted_ssh_host_key` | pass | Unknown/unrecorded host is rejected |
| Missing/unreadable known_hosts rejects | unit test `missing_known_hosts_file_rejects_host_key`; lookup-error branch logs `known_hosts lookup failed; rejecting host key` | pass | Same `false`/`Err` path as the default-policy branch; old comment retained |
| `KnownHostsFile` accepts match, rejects mismatch | unit test `known_hosts_file_accepts_matching_key_and_rejects_mismatch` | pass | Changed key returns `Err(KeyChanged)` → `false` |
| `InsecureCompatibility` only reachable via compat cache | `SshSessionCache::new_compatibility()` stays `#[cfg(feature = "pproxy-compat")]`; unit test `compatibility_cache_constructs_the_insecure_policy` runs only with the feature | pass | `cargo test` without the feature compiles 8 tests, with it 9 |
| Session cache key scoped by policy | unit test `session_key_is_scoped_by_host_key_policy` (`HashSet` length 2) | pass | `SshSessionKey.policy` unchanged |
| Redaction preserved | unit tests `ssh_auth_debug_never_shows_secrets`, `session_key_debug_never_shows_secrets`; OpenSSH test `openssh_password_failure_is_redacted_and_reconnect_is_explicit` | pass | Debug emits `****`; errors omit password |
| Forwarded-channel accept path | OpenSSH test `openssh_remote_tcp_forward_accepts_incoming_channel` | pass | Reject path unchanged; not separately unit-tested (see §10) |
| Crate OpenSSH interop | `cargo test -p eggress-transport-ssh --features pproxy-compat --test openssh` → 7 passed | pass | Real `sshd`, not skipped |
| Required embed regression | `EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat --test ssh` → 6 passed | pass | No test adaptation needed |
| `ssh` and `ssh,pproxy-compat` compile | both `cargo check -p eggress-embed --locked --no-default-features --features …` runs | pass | |
| CLI optional-transport slice compiles | `cargo check -p eggress-cli --locked --no-default-features --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins` | pass | |
| SSH graph advisories absent | `cargo audit --ignore RUSTSEC-2023-0071` | pass | -0492/-0495/-0385/-0387 gone |
| MSRV 1.89 sufficient | `rustc 1.89.0` / `cargo 1.89.0` built every checked slice | pass | No MSRV change |
| No public Eggress API change | `SshHostKeyPolicy`, `SshAuth`, `SshSessionKey`, `SshSessionCache`, `SshRemoteForward` signatures untouched; `verify_host_key` is private | pass | `docs/RUST_API.md` unchanged |
| Default feature graph unchanged | `ssh` remains opt-in on CLI/embed; no feature list edited | pass | |
| No pproxy claim change | no manifest, tier, oracle, or differential evidence touched | pass | |

## 3. Production implementation evidence

- **Workspace pin.** `Cargo.toml:93` moved from
  `russh = { version = "0.62.6", default-features = false, features = ["ring", "flate2", "rsa"] }`
  to `0.65.2` with the identical feature set. No other manifest line changed.
- **Lockfile.** `cargo update -p russh` ("Locking 11 packages to latest
  compatible versions") moved: `argon2 0.6.0-rc.8 → 0.6.0`,
  `blake2 0.11.0-rc.6 → 0.11.0`, `crypto-bigint 0.7.5 → 0.7.11`,
  `ctutils 0.4.2 → 0.4.8`, `der 0.8.0 → 0.8.3`, `num-bigint 0.4.8 → 0.5.1`,
  `pkcs1 0.8.0-rc.4 → 0.8.0`, `rsa 0.10.0-rc.18 → 0.10.0`,
  `russh 0.62.6 → 0.65.2`, `spki 0.8.0 → 0.8.1`,
  `ssh-key 0.7.0-rc.11 → 0.7.0-rc.12`, and removed
  `internal-russh-num-bigint 0.5.0`. No version requirement was edited elsewhere;
  the workspace stays at `1.0.12`.
- **Adapter.** `crates/eggress-transport-ssh/src/lib.rs`:
  - `check_server_key` signature/parameter migrated; `PublicKeyOrCertificate::Certificate`
    is rejected with a warning before the policy decision.
  - the three-way policy branch moved unchanged into private
    `verify_host_key(host, port, policy, &PublicKey) -> bool`, with the same
    `tracing::warn!` calls for a failed lookup and the same
    `InsecureCompatibility => true` arm.
  - `server_channel_open_forwarded_tcpip`, `connect_stream`,
    `authenticate_password`, `authenticate_publickey`,
    `channel_open_direct_tcpip`, `channel_open_direct_streamlocal`,
    `tcpip_forward`, `cancel_tcpip_forward`, `keepalive_*`, and the whole cache
    required no change.
- **Tests.** A `#[cfg(test)] mod tests` was added to the same file with eight
  tests (nine with `pproxy-compat`): `SshAuth`/`SshSessionKey` redaction,
  policy-scoped cache key, known_hosts match/mismatch/absent,
  default-policy rejection, insecure acceptance, cache policy accessors, and the
  feature-gated compatibility constructor. Test host keys are public-key vectors
  only.
- **Not implemented (deliberately).** No certificate host-key support, no new
  auth method, no agent forwarding, no SFTP/remote command, no listener role,
  no change to cache bounds, hop-zero reuse rules, or `local_bind` handling.
- **Embed test.** `crates/eggress-embed/tests/ssh.rs` needed no adaptation and
  is unmodified.

## 4. Verification executed

### Commands run

| Command | Result |
|---|---|
| `cargo update -p russh` | pass — "Locking 11 packages to latest compatible versions"; the 12 moves/removal in §3 |
| `cargo update -p crypto-bigint -p ctutils` | pass — "Locking 0 packages to latest compatible versions" (already at `0.7.11` / `0.4.8` from the `russh` update) |
| `cargo tree -i russh -p eggress-transport-ssh` | pass — `russh v0.65.2` → `eggress-transport-ssh` |
| `cargo tree -i russh -p eggress-embed --no-default-features --features ssh` | pass — single `russh v0.65.2` under the embed/outbound/runtime/server chain |
| `cargo tree -i crypto-bigint -p eggress-transport-ssh` | pass — `crypto-bigint v0.7.11` |
| `cargo tree -i ctutils -p eggress-transport-ssh` | pass — `ctutils v0.4.8` |
| `cargo test -p eggress-transport-ssh` | pass — 8 lib tests passed, 0 failed; `tests/openssh.rs` 0 tests (feature-gated file), 0 doc-tests |
| `cargo test -p eggress-transport-ssh --features pproxy-compat --lib` | pass — 9 passed, 0 failed |
| `cargo test -p eggress-transport-ssh --features pproxy-compat --test openssh -- --nocapture` | pass — 7 passed, 0 failed |
| `EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat --test ssh -- --nocapture` | pass — 6 passed, 0 failed |
| `cargo check -p eggress-embed --locked --no-default-features --features ssh` | pass |
| `cargo check -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat` | pass |
| `cargo check -p eggress-cli --locked --no-default-features --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins` | pass |
| `cargo clippy -p eggress-transport-ssh -p eggress-embed --all-targets -- -D warnings` | pass — finished with no warnings |
| `cargo fmt -p eggress-transport-ssh -p eggress-embed -- --check` | pass — clean |
| `cargo audit --ignore RUSTSEC-2023-0071` | partial by design — 4 vulnerabilities remain, all `zip`; 2 warnings (`rustix` unsound, `wnaf` yanked) |
| `cargo deny check advisories` | `advisories ok` — but its debug trace filters `russh 0.65.2` and the `eggup`/`zip` subtree out of the graph (200 crates filtered), so it is not evidence for this milestone |

OpenSSH availability: `command -v sshd` → `/usr/sbin/sshd`,
`command -v ssh-keygen` → `/usr/bin/ssh-keygen`; both suites ran against a
fixture `sshd` and did not skip for missing tools. Two environment gates are
pre-existing and unchanged: the transport password-success test prints
`skipping SSH password-auth success test: EGRESS_SSH_TEST_PASSWORD is unset`
(public-key and password-failure coverage still ran), and nothing in this
milestone added a skip.

The brief's literal command `cargo test -p eggress-transport-ssh --test openssh`
was also run without the feature and reports `running 0 tests`, because
`tests/openssh.rs` is `#![cfg(feature = "pproxy-compat")]`. The feature-enabled
run above is the one that executes the suite.

`cargo audit` headings absent compared with the pre-migration baseline:
RUSTSEC-2026-0385 (`ctutils`), RUSTSEC-2026-0387 (`crypto-bigint`),
RUSTSEC-2026-0492 and RUSTSEC-2026-0495 (`russh`). The `crypto-bigint` and
`der` yanked warnings also disappeared.

Remaining `cargo audit` findings:

| Crate | Advisory | Owner |
|---|---|---|
| `zip 6.0.0` | RUSTSEC-2026-0429 | Delivery M004 |
| `zip 6.0.0` | RUSTSEC-2026-0367 | Delivery M004 |
| `zip 6.0.0` | RUSTSEC-2026-0368 | Delivery M004 |
| `zip 6.0.0` | RUSTSEC-2026-0369 | Delivery M004 |
| `rustix 1.1.4` | RUSTSEC-2026-0486 (`unsound` warning, not a vulnerability) | Pre-existing via `tempfile`; unchanged here |
| `wnaf 0.14.0` | yanked (warning only) | Pre-existing; not on the SSH path |

**Coordinator broad gate (2026-11-16, after all six corrective waves):** pass —
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace --locked`, the documented CLI/embed feature slices, the
embed OpenSSH regression (6/6), and `cargo deny --workspace check`
(advisories/bans/licenses/sources). `cargo audit --ignore RUSTSEC-2023-0071`
reports only the four tracked `zip 6.0.0` advisories. The condition recorded in
§11 is therefore satisfied.

## 5. Invariant review

- **Verified host keys by default.** `SshSessionCache::new()` still selects
  `KnownHosts`; `verify_host_key` still calls
  `russh::keys::check_known_hosts`. The embed test
  `native_toml_connector_rejects_untrusted_ssh_host_key` proves a TOML chain
  fails against a fixture host key absent from known_hosts.
- **Missing/unreadable file rejects.** The `Err` branch logs
  `known_hosts lookup failed; rejecting host key` and returns `false`; nothing
  maps an error to acceptance. `missing_known_hosts_file_rejects_host_key`
  asserts the absent-file case, which reaches the same branch through
  `check_known_hosts_path`.
- **Insecure compatibility stays scoped.** `InsecureCompatibility` is still
  constructed only by `SshSessionCache::new_compatibility()` under
  `pproxy-compat`, still emits the per-connection warning in
  `get_or_connect`/`connect_fresh`, and `SshSessionKey.policy` still separates
  the two caches (unit-tested).
- **No credential leak.** `SshAuth::Debug` and `SshSessionKey::Debug` are
  untouched and unit-tested (`****`, no secret substring). Errors still use
  static strings: `AuthenticationFailed`,
  `PrivateKey("could not load configured private key")`, and
  `Connection(error.to_string())` from `russh`, which does not echo the
  password or key path.
- **Restricted forwarding surface.** No new channel type, listener role, auth
  method, remote command, SFTP, or agent forwarding was added. The crate still
  exposes only direct TCP, remote Unix-socket, and explicitly requested remote
  TCP forwarding.
- **Cache bounds and lifecycle.** `Arc<Mutex<HashMap<…>>>` with the same
  hop-zero rules, `shutdown()` clear, and `invalidate()` eviction are unchanged.
- **No new SSH listener or auth mechanism.**
- **No pproxy claim change.** No manifest tier, oracle, or differential
  evidence was touched.

## 6. Failure and recovery review

- **Host-key rejection fails the connection.** `Ok(false)` from
  `check_server_key` makes `russh` return `Error::UnknownKey`; there is no
  fallback to `InsecureCompatibility` and no retry with relaxed policy.
- **Certificate host keys fail closed.** Rejected in the handler before the
  policy branch under every policy, matching the pre-0.65 parse failure.
- **Cache isolation.** Lookups are keyed by host/port/username/auth/hop/policy;
  a live session created under one policy cannot satisfy a request under
  another. A closed handle is removed before reconnect.
- **Cancellation.** Authentication and channel-open paths are unchanged; a
  dropped future still drops the handle before cache insertion, so no partial
  entry remains.
- **Concurrency.** Cache access remains mutex-protected; the remote-forward
  receiver keeps depth 128 and no lock is held across an await.
- **Shutdown.** `shutdown()` still clears every cached handle; the OpenSSH
  reconnect-after-shutdown assertions remain green.
- **Malformed input.** Empty Unix socket path and target port `0` guards are
  unchanged, as is the non-loopback remote-forward warning.

## 7. Migration and compatibility review

- **Breaking dependency change is contained.** The only public-facing effect is
  the dependency version; `russh` types still do not appear in Eggress's public
  API, so no `docs/RUST_API.md` edit was required.
- **No config/schema change.** TOML and pproxy URI translation are untouched.
- **Feature graph unchanged.** `ssh` remains an opt-in feature on
  `eggress-cli`/`eggress-embed`/`eggress-runtime`; the default CLI, embed, and
  Python graphs do not include `russh`.
- **Observable behavior delta.** A server that presents an SSH host certificate
  previously failed the handshake in KEX; it now fails host-key verification
  with a clearer warning. The result (connection refused) is identical.
- **Rollback.** Reverting the pin and `Cargo.lock` restores 0.62.6; the adapter
  would need the old `&PublicKey` parameter. There is no on-disk or on-wire
  migration.
- **Test contract.** The OpenSSH suites still gate on the `pproxy-compat`
  feature and on OpenSSH tool availability, unchanged.

## 8. Security review

- **Advisories closed.** RUSTSEC-2026-0492 (client handler callbacks for
  unopened channels, CVSS 7.5), RUSTSEC-2026-0495 (agent signing without
  `confirm`), RUSTSEC-2026-0387 (`crypto-bigint` timing variability), and
  RUSTSEC-2026-0385 (`ctutils` non-constant-time masks) are no longer reported.
  The two `russh` advisories are reachable in principle even though this
  crate exposes no agent surface; the migration removes them rather than
  relying on reachability reasoning.
- **Host-key trust boundary.** `KnownHosts`/`KnownHostsFile` remain
  fail-closed; only the explicitly constructed compatibility cache accepts any
  key. Certificate host keys are refused instead of being downgraded to their
  certified plain key, which prevents a certificate from satisfying a
  known_hosts entry that was never recorded for it.
- **Redaction.** Password and private-key path never appear in `Debug`; the new
  certificate warning logs only host and port.
- **Supply chain.** The `ring` feature set is retained and `aws-lc-rs` remains
  disabled and banned by `deny.toml`; no C toolchain dependency was added.
- **Denial-of-service bounds.** No parsing, buffering, channel-depth, or relay
  path changed.

## 9. Documentation and operations

- `architecture/transports-ssh-quic-h3.md` — module-map line count, public API
  line references, host-key flow (`verify_host_key` + certificate rejection),
  the pinned `russh` line, and the review entry-point commands updated.
- `docs/SECURITY_REVIEW.md` — mitigation 13 now records the pinned `russh`
  0.65.x line, the fixed advisories, fail-closed known_hosts lookup, and
  certificate host-key rejection.
- `plans/implementation/transports/005-russh-065-advisory-migration.md` —
  status `ready` → `implemented`.
- `plans/subsystems/transports-roadmap.md` — M005 status fields and status-table
  row updated; M004 rows untouched.
- No operator command, CLI flag, config key, exit code, or diagnostic format
  changed; no recovery instruction changed.
- No release, tag, publication, or version bump was performed.

## 10. Unresolved findings

| Severity | Finding | Impact | Required action |
|---|---|---|---|
| low | Workspace-wide `fmt`/`clippy`/`test` gate not run in this wave | Cross-crate regressions outside the owned crates would be invisible here | Coordinator runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --locked` after all waves land |
| low | `zip 6.0.0` advisories (RUSTSEC-2026-0429/-0367/-0368/-0369) remain | Optional updater path only; not an SSH dependency | Delivery M004 / upstream `eggup-archive` with `zip >= 9.0.0` |
| low | Forwarded-channel reject branch (no registered forward, or full queue) has no dedicated test | A regression there would only be caught by inspection, not by a test | The accept path is covered by `openssh_remote_tcp_forward_accepts_incoming_channel`; a reject test would need a live session that requests a forward the client never registered |
| low | `cargo deny check advisories` reports ok while filtering the optional `ssh` graph (and `eggup`/`zip`) out of its feature resolution | A CI gate built only on this command would miss optional-feature advisories | Delivery M004 owns the advisory gate; the gate should account for optional features or run against `--all-features`-equivalent scope |
| low | `rustix 1.1.4` RUSTSEC-2026-0486 (`unsound`) and `wnaf 0.14.0` yanked warnings remain | Warning-only, outside the SSH path | Track separately; unchanged by this pass |
| low | Transport password-success test stays skipped without `EGRESS_SSH_TEST_PASSWORD` | One auth mode is not exercised by default | Pre-existing environment gate; public-key auth and password-failure coverage ran |

Post-wave note (2026-11-16): the `rustix` advisory was cleared by the `rustix
1.1.6` update in Delivery M004, and the `cargo deny` graph-scope finding was
fixed by the `cargo deny --workspace check` gate in M004. The four `zip`
advisories remain tracked there.

## 11. Roadmap disposition

**Closed.** Production implementation for Transports M005 is complete: the
optional SSH transport is on the fixed `russh` line, its policy and cache
invariants are intact, and real-`sshd` regressions are green. The coordinator
broad gate, including the embed OpenSSH regression, passed on 2026-11-16. No
commit, tag, or release was created; publication remains a separate
maintainer-authorized action.

## 12. Registry updates

Required, not performed here because `plans/registry.md` and `docs/ROADMAP.md`
are owned by the wave coordinator:

- `plans/registry.md`: move Transports M005 from `ready` to
  `closed`, link
  `plans/closure/transports/005-status.md`, and drop the "`russh 0.65.x` API
  migration" blocker note.
- `plans/registry.md`: narrow the security-corrective gate note — S2 is
  implemented; S5 still owns the `zip` exception, and the note should record
  that `cargo deny check advisories` resolves a feature-filtered graph that
  omits the optional `ssh` line.
- `plans/registry.md`: the residual-advisory entry can be reduced to the four
  `zip` IDs plus the `rustix`/`wnaf` warnings; the four SSH-graph IDs are gone.
- `plans/subsystems/transports-roadmap.md`: M005 status fields applied in this
  pass; the roadmap header still reads "M004/M005 security correctives ready"
  and should be reconciled with the coordinator's final state.
- `docs/ROADMAP.md`: reflect the M005 state alongside the other corrective
  waves once the coordinator reconciles all of them.
