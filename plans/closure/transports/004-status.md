# Transports Milestone 004 — Closure Status

Status: closed

Source implementation plan:

- `plans/implementation/transports/004-tls-trust-and-dependency-advisory-remediation.md`

Source subsystem roadmap:

- `plans/subsystems/transports-roadmap.md` §7 Milestone 4, §12 milestone status
- `plans/subsystems/security-hardening-roadmap.md` H1 and L11

Repository baseline reviewed: `ba64e16` (branch `feat/auto-20261116-652bb64d`, workspace `1.0.12`)

Implementation commits or pull requests:

- None. This pass is uncommitted working-tree work in the shared corrective
  wave; no commit, tag, or publication was created by this milestone.

## 1. Executive finding

The release-blocking default-graph defect is closed. Default outbound TLS now
resolves `rustls 0.23.46`, `rustls-webpki 0.103.17`, `webpki-roots 1.0.12`
(reached through the `webpki-roots 0.26.11` compatibility re-export),
`rustls-platform-verifier 0.7.1`, `webpki-root-certs 1.0.12`,
`quinn 0.11.12`, `quinn-proto 0.11.19`, `quinn-udp 0.5.16`, and
`tokio 1.54.1`. `cargo audit` no longer reports RUSTSEC-2026-0381/-0382 (the
distrusted emSign root and the compromised-intermediate acceptance path) or any
rustls/quinn/tokio advisory. No source adaptation was required: the only
compile-affecting change outside the lockfile is documentation and the two
new behavioural guards.

The two truth/doc defects and the latent secret exposure found by the same
audit are also closed: the bundled-Mozilla root set is now described as such
instead of as a "system root store", `?insecure` is documented as reachable
from configuration builds rather than API-only, config compilation emits one
redacted warning per upstream that selects it, and
`CompiledListenerTlsConfig` no longer prints PEM or key bytes.

Closure is conditional on one item only: the coordinator's workspace-wide
`fmt`/`clippy`/`test` gate, which runs after every corrective wave has landed.

## 2. Requirement-to-evidence matrix

| Requirement | Evidence | Result | Notes |
|---|---|---|---|
| Default graph resolves `rustls >= 0.23.46` | `cargo tree -i rustls@0.23.46` → `rustls v0.23.46` | pass | 0.23.46 exactly |
| Default graph resolves `rustls-webpki >= 0.103.16` | `cargo tree -i rustls-webpki` → `rustls-webpki v0.103.17` | pass | 0.103.17 accepted by plan §16 |
| Default graph resolves `webpki-roots 1.x >= 1.0.10` | `cargo tree -i webpki-roots@1.0.12` → parent is `webpki-roots v0.26.11` → `eggress-transport-tls` | pass | 1.x parent verified, not just the 0.26 string |
| `rustls-platform-verifier` / `webpki-root-certs` fixed | lock: `0.7.1` / `1.0.12` (`cargo update` output) | pass | |
| Fixed quinn graph | `cargo tree -p eggress-transport-quic -i quinn` → `0.11.12`; `-i quinn-proto` → `0.11.19`; `-i quinn-udp` → `0.5.16` | pass | quinn is not in the root package graph, so the inverse tree is taken from its owner crate |
| `tokio` fixed | `cargo tree -i tokio` → `tokio v1.54.1` | pass | |
| `cargo audit` no longer reports RUSTSEC-2026-0381/-0382 | `cargo audit --ignore RUSTSEC-2023-0071` | pass | Neither ID appears in the report |
| `cargo audit` no longer reports the rustls/quinn/tokio IDs | same run; grep for `-0346/-0347/-0348/-0349`, `quinn`, `tokio` returns nothing | pass | |
| Default outbound TLS verifies; no insecure fallback | `eggress-transport-tls` test `default_verified_config_rejects_self_signed_server` | pass | Self-signed peer rejected by the process-shared default config |
| Default configuration is built from the current root set | `eggress-transport-tls` test `builder_system_roots` asserts the root count equals `webpki_roots::TLS_SERVER_ROOTS.len()` and is non-empty | pass | |
| Custom CA replacement unchanged | pre-existing `builder_with_custom_ca_pem`, `client_config_with_alpn_preserves_trust_policy` | pass | Retained, unmodified |
| mTLS identity unchanged | pre-existing `client_config_with_alpn_preserves_mtls_identity` | pass | Retained, unmodified |
| ALPN preservation unchanged | pre-existing `client_config_with_alpn_*` (3 tests) | pass | Retained, unmodified |
| `tls_override` + per-hop `insecure=true` fails closed | pre-existing `eggress-outbound` tests `tls_override_plus_insecure_fails_closed{,_with_insecure_tls_feature}` | not run in this wave | No code on that path changed; deferred to the coordinator workspace gate (see §10) |
| H2 pool scoping unchanged | pre-existing `eggress-outbound` pool tests | not run in this wave | Unchanged code path; deferred to the coordinator workspace gate |
| Trust-store wording corrected | `architecture/transports-tls.md`, `docs/security/SECURE_CONFIGURATION.md`, `docs/SECURITY_REVIEW.md`, `docs/security/THREAT_MODEL.md` | pass | "bundled Mozilla roots via webpki-roots", not "system root store" |
| `?insecure` documented as config-reachable | `docs/URI_GRAMMAR.md`, `docs/security/SECURE_CONFIGURATION.md`, `docs/SECURITY_REVIEW.md` | pass | "API-only" claim removed |
| Insecure selection warns once, before first handshake, redacted | `eggress-config` tests `compile_warns_once_for_the_insecure_upstream_only`, `compile_does_not_warn_when_every_hop_verifies` | pass | One event per upstream at compile time; no endpoint/credential/URI echo |
| Warning shape names the policy decision | `insecure_hop_summary_*` tests + emitted fields `upstream`, `insecure_hops`, `policy=insecure-tls` | pass | |
| TLS private key never appears in `Debug` | `eggress-config` tests `compiled_listener_tls_debug_redacts_pem_material`, `compiled_listener_tls_debug_reports_absent_material`, `listener_config_debug_redacts_nested_tls_key` | pass | Reports `has_cert`/`has_key`/`key_len` only |
| Reverse-TLS types already redact | `crates/eggress-config/src/compile/model.rs` `CompiledReverseServerTls`/`CompiledReverseClientTls` custom `Debug`; `eggress-protocol-reverse` `ReverseServerTlsConfig`/`ReverseClientTlsConfig` | pass | Inspected; no change required |
| No OpenSSL/native-tls/aws-lc/C crypto introduced | lock scan: `aws-lc-sys` 0, `aws-lc-rs` 0, `openssl-sys` 0, `native-tls` 0; `cargo tree -e features -i rustls@0.23.46` shows `ring` only | pass | |
| MSRV 1.89 and edition 2021 remain valid | `rustc 1.89.0` / `cargo 1.89.0` compiled every checked crate | pass | |
| Insecure QUIC stays behind `insecure-quic` | existing compile-time rejection in `compile_upstreams` retained; `insecure-quic` feature untouched | pass | |
| No pproxy compatibility claim changed | no manifest, tier, or oracle/differential evidence touched | pass | |

## 3. Production implementation evidence

Implemented:

- **Lockfile.** `Cargo.lock` moved exactly ten package versions and gained one
  dependency edge (`eggress-config` → `tracing`). No manifest version
  requirement was changed; no gated version location was touched.
  - `rustls 0.23.45 → 0.23.46`
  - `rustls-webpki 0.103.15 → 0.103.17`
  - `webpki-roots 1.0.8 → 1.0.12` (the `0.26.11` re-export is unchanged)
  - `rustls-platform-verifier 0.7.0 → 0.7.1` and
    `rustls-platform-verifier-android 0.1.1 → 0.2.0`
  - `webpki-root-certs 1.0.9 → 1.0.12`
  - `quinn 0.11.11 → 0.11.12`, `quinn-proto 0.11.17 → 0.11.19`,
    `quinn-udp 0.5.15 → 0.5.16`
  - `tokio 1.52.3 → 1.54.1`
- **Trust-store truth.** `TlsClientConfigBuilder::with_system_roots` rustdoc and
  `default_client_config` rustdoc now state that the roots are the bundled
  Mozilla set from `webpki-roots`, compiled into the binary, not the OS store,
  and that `with_custom_ca_pem` replaces rather than extends them. Method name
  and signature are unchanged for source compatibility.
- **Insecure selection is announced.** `eggress-config` compiles one
  `tracing::warn!` per upstream whose chain contains any `?insecure` hop, from
  `compile_upstreams` — the same funnel used by TOML startup/reload
  (`load_and_compile`) and by the pproxy translator (`compile_config`). The
  event carries `upstream` (config id), `insecure_hops` (`hop <n> (<protocol>)`),
  and `policy = "insecure-tls"`. Endpoints, ports, credentials, and the source
  URI are never included. It fires once per load, before any chain executes.
- **Secret-bearing `Debug` removed.** `CompiledListenerTlsConfig` in
  `eggress-config` no longer derives `Debug`; a manual implementation prints
  `has_cert`, `has_key`, `key_len`, and `alpn`. The reverse-TLS compiled types
  already had redacting `Debug` (plus zeroizing `Drop`) and were verified, not
  modified.
- **`eggress-config` dependency.** Added `tracing.workspace = true` so the
  config crate can emit the security notice. No new external crate entered the
  graph; `tracing` was already a workspace dependency.

Planned but deliberately absent (out of scope, unchanged):

- An OS certificate-store backend. The trust source remains the bundled set.
- Removal of `insecure-tls` from default CLI/runtime builds. That would need a
  separate pproxy `--ssl`/QUIC compatibility assessment.

## 4. Verification executed

### Commands run

```bash
cargo update -p rustls@0.23.45 -p rustls-webpki@0.103.15 -p webpki-roots@1.0.8 \
  -p rustls-platform-verifier@0.7.0 -p webpki-root-certs@1.0.9 \
  -p quinn@0.11.11 -p quinn-proto@0.11.17 -p quinn-udp@0.5.15 -p tokio@1.52.3
cargo tree -i rustls@0.23.46
cargo tree -i rustls-webpki
cargo tree -i webpki-roots@1.0.12
cargo tree -i webpki-roots@0.26.11
cargo tree -i tokio
cargo tree -p eggress-transport-quic -i quinn
cargo tree -p eggress-transport-quic -i quinn-proto
cargo tree -p eggress-transport-quic -i quinn-udp
cargo tree -p eggress-transport-tls -e features -i rustls@0.23.46
cargo test -p eggress-transport-tls
cargo test -p eggress-config
cargo test -p eggress-runtime --test tls
cargo test -p eggress-embed --test error_redaction
cargo clippy -p eggress-transport-tls -p eggress-config --all-targets -- -D warnings
cargo fmt -p eggress-transport-tls -p eggress-config -- --check
cargo audit --ignore RUSTSEC-2023-0071
cargo check -p eggress-cli --locked --no-default-features \
  --features full,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins
```

`cargo fmt --all`, `cargo clippy --workspace`, `cargo test --workspace`, and
`--all-features` were intentionally not run: the coordinator runs the broad gate
once, after every corrective wave has landed.

### Results

| Command | Result |
|---|---|
| `cargo update …` | pass — "Locking 10 packages to latest compatible versions" |
| `cargo tree -i rustls@0.23.46` | pass — `rustls v0.23.46` under `eggress-core`, `eggress-outbound`, `eggress-transport-tls`, `eggress-bench`, `tokio-rustls 0.26.4` |
| `cargo tree -i rustls-webpki` | pass — `rustls-webpki v0.103.17` under `rustls v0.23.46` |
| `cargo tree -i webpki-roots@1.0.12` | pass — parent `webpki-roots v0.26.11` → `eggress-transport-tls` |
| `cargo tree -i webpki-roots@0.26.11` | pass — under `eggress-transport-tls`, `eggress-bench`, `eggress-outbound` |
| `cargo tree -i tokio` | pass — `tokio v1.54.1` across the workspace |
| `cargo tree -p eggress-transport-quic -i quinn{-proto,-udp}` | pass — `quinn 0.11.12`, `quinn-proto 0.11.19`, `quinn-udp 0.5.16` |
| `cargo tree -e features -i rustls@0.23.46` | pass — `ring`/`std`/`tls12` only, no aws-lc |
| `cargo test -p eggress-transport-tls` | pass — 24 passed, 0 failed; 0 doc-tests |
| `cargo test -p eggress-config` | pass — 129 lib tests passed, 0 failed; `tests/fuzz_smoke.rs` 1 passed; 0 doc-tests |
| `cargo test -p eggress-runtime --test tls` | pass — 4 passed, 0 failed |
| `cargo test -p eggress-embed --test error_redaction` | **blocked, not run to completion** — the test binary could not link because `crates/eggress-pproxy-compat/src/translate/model.rs:159` derives `Debug` for `ReverseServerToml` while line 187 also defines `impl std::fmt::Debug for ReverseServerToml` (E0119). That file belongs to the concurrent parity-compat corrective wave and was not modified here. |
| `cargo clippy -p eggress-transport-tls -p eggress-config --all-targets -- -D warnings` | pass — finished with no warnings |
| `cargo fmt -p eggress-transport-tls -p eggress-config -- --check` | pass — no diffs after applying `cargo fmt` to the same two packages |
| `cargo audit --ignore RUSTSEC-2023-0071` | partial by design — 8 vulnerabilities remain, all outside this milestone; 3 yanked-crate warnings; 1 `unsound` warning. TLS/QUIC/tokio advisories are gone. |
| `cargo check -p eggress-cli … --features full,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins` | **blocked, not run to completion** — same E0119 error in `eggress-pproxy-compat`. The `pproxy-legacy`/`pproxy-daemon` feature slice pulls that crate in, so this command cannot resolve until the parity-compat wave compiles. |

Remaining `cargo audit` findings, with ownership:

| Crate | Advisory | Owner |
|---|---|---|
| `russh` | RUSTSEC-2026-0492 | Transports M005 (optional `ssh`) |
| `russh` | RUSTSEC-2026-0495 | Transports M005 |
| `crypto-bigint` | RUSTSEC-2026-0387 (also yanked) | Transports M005 (`russh` path) |
| `ctutils` | RUSTSEC-2026-0385 | Transports M005 (`russh` path) |
| `zip` | RUSTSEC-2026-0429 | Delivery M004 |
| `zip` | RUSTSEC-2026-0367 | Delivery M004 |
| `zip` | RUSTSEC-2026-0368 | Delivery M004 |
| `zip` | RUSTSEC-2026-0369 | Delivery M004 |
| `rustix 1.1.4` | RUSTSEC-2026-0486 (`unsound`, xattr) | Pre-existing via `tempfile 3.27.0`; unchanged by this pass and outside the TLS/QUIC/tokio scope |
| `der 0.8.0`, `wnaf 0.14.0` | yanked (warning only, not advisories) | Pre-existing; `russh`/crypto path |

Absent from the report (previously present): RUSTSEC-2026-0381, RUSTSEC-2026-0382,
RUSTSEC-2026-0346, RUSTSEC-2026-0347, RUSTSEC-2026-0348, RUSTSEC-2026-0349.

**Coordinator broad gate (2026-11-16, after all six corrective waves):** pass —
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace --locked`, the documented CLI/embed feature slices, the
embed OpenSSH regression (6/6), and `cargo deny --workspace check`
(advisories/bans/licenses/sources). `cargo audit --ignore RUSTSEC-2023-0071`
reports only the four tracked `zip 6.0.0` advisories. The condition recorded in
§11 is therefore satisfied.

## 5. Invariant review

- **Default outbound TLS verifies; no insecure fallback.** `build()` still
  selects `with_root_certificates(...)` unless `self.insecure` was explicitly
  requested. The new
  `default_verified_config_rejects_self_signed_server` test proves the shared
  default rejects an unverifiable peer, so an accidental downgrade in the
  verifier path would fail a test rather than silently succeed.
- **Custom CA replaces the root store.** `with_custom_ca_pem` still assigns
  `builder.root_store = roots`; `builder_with_custom_ca_pem` and
  `client_config_with_alpn_preserves_trust_policy` remain green.
- **ALPN adaptation preserves policy.** `client_config_with_alpn` is unchanged;
  the three ALPN tests and the mTLS-preservation test remain green.
- **`tls_override` + per-hop `insecure=true` fails closed.** No code on the
  outbound executor path changed; the pre-existing fail-closed tests were not
  executed in this wave and are deferred to the workspace gate.
- **Insecure QUIC stays behind `insecure-quic`.** The compile-time rejection in
  `compile_upstreams` is untouched and the feature was not enabled.
- **No C crypto backend.** `aws-lc-sys`, `aws-lc-rs`, `openssl-sys`, and
  `native-tls` are absent from the lock; rustls resolves `ring` only.
- **MSRV/edition.** Every check ran on `rustc 1.89.0`, edition 2021, with
  `unsafe_code = "deny"` unchanged.
- **No pproxy claim change.** No manifest, tier, or oracle evidence was edited.

## 6. Failure and recovery review

- **Root-load/parse failure does not downgrade.** A failing root set still
  yields a build/handshake error; nothing in this pass added a fallback path.
  The `OnceLock` caches still store only successful builds.
- **Warning idempotency and no spam.** The notice is emitted from configuration
  compilation, i.e. once per load or hot reload, not per connection, packet, or
  hop. Hot reload re-emits it once per reload, which matches "once before the
  first handshake" for the new generation.
- **Cancellation/restart.** No lifecycle, listener-topology, or shutdown-order
  code changed; the warning adds no state.
- **Malformed input.** The redaction helpers consume already-parsed
  `ProxyChainSpec` values, so a malformed URI still fails in
  `parse_proxy_chain` before any notice is produced.
- **Contention.** No new locks or shared mutable state were introduced;
  `CompiledListenerTlsConfig` remains `Clone` with the same fields.

## 7. Migration and compatibility review

- No API removal or signature change. `with_system_roots` keeps its name and
  behavior; only its documentation is corrected.
- No configuration schema change: `?insecure` parsing is untouched, and no TOML
  key was added or removed.
- Existing `?insecure` configs continue to work but now produce one warning per
  upstream at compile time.
- `eggress-config` gained a `tracing` dependency edge; this is not a published
  API change and `tracing` was already a workspace dependency.
- No version bump: the workspace stays at `1.0.12`, and the four gated version
  locations plus `python/pyproject.toml` are untouched.
- Rollback is a normal `Cargo.lock` revert; no schema, on-disk, or on-wire
  migration exists.

## 8. Security review

- **Advisory exposure removed from the default graph.** The distrusted emSign
  root (RUSTSEC-2026-0381) and the compromised-intermediate acceptance path
  (RUSTSEC-2026-0382) are gone, together with the rustls and quinn/tokio
  advisories listed for this milestone.
- **Redaction.** The compile-time notice carries only the upstream id, hop
  indexes, and protocol names; a test asserts that credentials, host, and port
  never appear. `CompiledListenerTlsConfig` prints presence flags and a key
  length only, asserted directly and through the enclosing `ListenerConfig`.
- **No new trust surface.** The root source is unchanged and now described
  accurately; there is still no OS-store backend and no automatic trust of
  locally installed CAs.
- **Insecure mode remains explicit.** The feature is still required, the QUIC
  gate is unchanged, and no error path substitutes an insecure verifier.
- **Denial-of-service bounds.** No parsing, buffering, or relay path changed.

## 9. Documentation and operations

- `architecture/transports-tls.md` — root source is "bundled Mozilla roots",
  the `with_system_roots` name is described as historical, the insecure
  dual-gate is qualified (compiled into shipped CLI builds, not test-only), and
  the compile-time warning is documented.
- `docs/security/SECURE_CONFIGURATION.md` — upstream trust-root paragraph and
  a corrected insecure-mode paragraph covering `?insecure` reachability, the
  warning, and the `insecure-quic` gate.
- `docs/SECURITY_REVIEW.md` — TLS insecure-mode section rewritten; the
  "Mitigations Already Implemented" entry no longer claims an OS root store or
  an API-only insecure mode.
- `docs/security/THREAT_MODEL.md` — A2 mitigation states the bundled-root
  behavior and CA-pinning consequence.
- `docs/URI_GRAMMAR.md` — new "Query parameters" section documenting `?rule`
  and `?insecure`, plus a `?insecure` example.
- `plans/implementation/transports/004-…md` — status `ready` → `implemented`.
- `plans/subsystems/transports-roadmap.md` — M004 status recorded as
  implemented / closed.

## 10. Unresolved findings

| Severity | Finding | Impact | Required action |
|---|---|---|---|
| low | Workspace-wide `fmt`/`clippy`/`test` gate not run in this wave | Cross-crate regressions outside the touched crates would be invisible here | Coordinator runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --locked` after all waves land |
| low | `cargo test -p eggress-embed --test error_redaction` and the `eggress-cli` locked feature check could not complete | Two acceptance commands have no result for this wave | Re-run both after the parity-compat wave compiles; the failure is E0119 in `eggress-pproxy-compat`, not in this milestone's code |
| low | `eggress-outbound` `tls_override`/H2-pool suites not executed here | Plan §10 integration evidence is inherited, not re-measured | Covered by the coordinator workspace gate; no code on those paths changed |
| low | `rustix 1.1.4` RUSTSEC-2026-0486 remains (via `tempfile 3.27.0`) | `unsound` xattr advisory in a dev/test-only dependency | Track separately; outside the TLS/QUIC/tokio graph and unchanged by this pass |
| low | `russh`, `crypto-bigint`, `ctutils`, `zip` advisories remain | Optional `ssh` graph and archive path | Transports M005 and Delivery M004 respectively |

Post-wave note (2026-11-16): the `russh`/`crypto-bigint`/`ctutils` and `zip`
findings listed above are owned by Transports M005 and Delivery M004. M005
cleared the SSH vulnerability set, and M004 added the advisory gate with a
tracked `zip` exception and cleared the `rustix` advisory via `rustix 1.1.6`.
A later unmaintained-only advisory (`chrono`, RUSTSEC-2026-0543, published
2026-11-17) is tracked in M005. No finding in this table remains open for M004
scope.

## 11. Roadmap disposition

**Closed.** Production implementation for Transports M004 is complete and the
default TLS graph no longer carries the advisories that blocked release. The
coordinator broad gate passed on 2026-11-16, including the two commands the
in-flight parity-compat edit had blocked (`eggress-embed` `error_redaction` and
the CLI `--features full,ssh,quic,...` check). No commit, tag, or release was
created by this milestone; publication remains a separate maintainer-authorized
action. Transports M005 and Delivery M004 are unaffected.

## 12. Registry updates

Required, not performed here because `plans/registry.md` is owned by the wave
coordinator:

- `plans/registry.md`: add Transports M004 to the implementation/closure index
  with status `closed` and link
  `plans/closure/transports/004-status.md`.
- `plans/registry.md`: record that the default-graph advisory set dropped from
  21 findings to 8 vulnerabilities (plus 3 yanked warnings and 1 `unsound`
  warning), and list the remaining IDs with their owning milestones.
- `plans/subsystems/transports-roadmap.md`: apply the M004 status update
  (applied in this pass for the M004 status fields only).
- `docs/ROADMAP.md`: reflect the M004 state alongside the other corrective
  waves once the coordinator reconciles all of them.
