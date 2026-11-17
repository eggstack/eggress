# Transports M004 — TLS Trust Store and Dependency Advisory Remediation

Status: implemented

Repository baseline: `feat/auto-20261116-652bb64d` at `ba64e16` (workspace 1.0.12)

Source roadmaps:

- `plans/subsystems/transports-roadmap.md` (new milestone M004)
- `plans/subsystems/security-hardening-roadmap.md` (S1)

Long-term requirements:

- `plans/000-long-term-specification.md` §4 (invariants 7, 8, 10), §5 (compat contract)
- `plans/001-terminology-and-domain-model.md` §3 (transport policy scoping)

Applicable ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

Primary class: invariant

## 1. Objective

Restore verified outbound TLS in the default build by moving the rustls,
webpki-roots, rustls-webpki, rustls-platform-verifier, quinn, and tokio
dependency graph to versions without the known advisories, and make the
insecure-TLS surface match its documentation.

This is a corrective pass. It does not add a transport, change chain semantics,
or change a pproxy compatibility claim.

## 2. Why this milestone is ready

The affected versions are available on crates.io and `cargo update --dry-run`
resolves them within the current semver constraints:

```text
rustls 0.23.45 -> 0.23.46
rustls-webpki 0.103.15 -> 0.103.17
webpki-roots 1.0.8 -> 1.0.12 (via webpki-roots 0.26.11)
rustls-platform-verifier 0.7.0 -> 0.7.1
webpki-root-certs 1.0.9 -> 1.0.12
quinn 0.11.11 -> 0.11.12
quinn-proto 0.11.17 -> 0.11.19
quinn-udp 0.5.15 -> 0.5.16
tokio 1.52.3 -> 1.54.1
```

The workspace pins are broad enough for these updates:
`rustls = "0.23"`, `webpki-roots = "0.26"`, `quinn = "0.11.11"`,
`tokio = "1"`, `rustls-platform-verifier = "0.7.0"` (`Cargo.toml:48-97`).

`russh` is handled by Transports M005; `zip` is tracked by Delivery M004.

## 3. Current implementation evidence

- `TlsClientConfigBuilder::with_system_roots()` loads
  `webpki_roots::TLS_SERVER_ROOTS` — `crates/eggress-transport-tls/src/client.rs:169-173`.
  The name is misleading: this is a bundled Mozilla root set, not the OS store.
- `build()` installs that root set with
  `ClientConfig::builder().with_root_certificates(...)` —
  `crates/eggress-transport-tls/src/client.rs:279-287`. This is the
  `rustls-webpki` verifier path affected by RUSTSEC-2026-0382.
- The resolved `webpki-roots 1.0.8` parent is affected by
  RUSTSEC-2026-0381 (bundled `emSign ECC Root CA - G3`); the 0.26.11
  compatibility package itself is marked unaffected, but its 1.x parent is not.
- `rustls 0.23.45` carries RUSTSEC-2026-0346/-0347/-0348/-0349. The default
  build does not enable certificate compression or ECH, and no custom
  `ClientSessionStore` was found, but the network-triggered downgrade advisory
  is a reason to upgrade regardless.
- `insecure-tls` is enabled unconditionally by `eggress-cli` and
  `eggress-runtime` (`crates/eggress-cli/Cargo.toml:78`,
  `crates/eggress-runtime/Cargo.toml:72`), and per-hop `?insecure` is parsed at
  `crates/eggress-uri/src/lib.rs:532-536`. The runtime warning exists at
  `crates/eggress-transport-tls/src/client.rs:205-213`, but
  `docs/security/SECURE_CONFIGURATION.md:53` says insecure mode is API-only.
- `CompiledListenerTlsConfig` derives `Debug` with `key_pem: Vec<u8>`
  (`crates/eggress-config/src/compile/model.rs:297-303`); no production log
  path currently prints it, but it is an unredacted secret-bearing `Debug`.

## 4. Invariants that must not regress

- Default outbound TLS verifies certificates; there is no fallback to insecure
  verification on a build, root-load, or handshake error.
- `TlsClientConfigBuilder::with_custom_ca_pem` replaces the root store exactly
  as before; custom CA behavior is unchanged.
- `client_config_with_alpn` preserves trust roots, custom verifiers, mTLS
  identity, and every other `ClientConfig` field; it only mutates ALPN.
- `tls_override` + per-hop `insecure=true` continues to fail closed.
- Insecure QUIC remains behind `insecure-quic`; insecure TLS remains explicit.
- No OpenSSL, native-tls, aws-lc-sys, or C dependency is introduced.
- MSRV 1.89 and edition 2021 remain valid.
- No pproxy compatibility claim changes.

## 5. Scope

### In scope

- `Cargo.lock` and workspace dependency resolution for the TLS/QUIC/tokio graph.
- `eggress-transport-tls` default roots/verifier and insecure-mode documentation.
- `eggress-transport-quic` only if the quinn/platform-verifier update requires a
  source adjustment.
- `eggress-config` TLS listener config `Debug` redaction.
- Focused TLS/QUIC/runtime tests and the relevant feature-slice checks.
- Security documentation corrections for root-store and insecure-mode claims.

### Explicitly out of scope

- `russh` migration (Transports M005).
- `zip`/`eggup-archive` upgrade (Delivery M004; currently blocked upstream).
- Admin TLS implementation.
- New trust-store providers or a move to the OS certificate store.
- Changing pproxy compatibility tiers.

## 6. Required production changes

### A. Upgrade the default TLS/QUIC/tokio graph

Update the lockfile with the narrowest commands that resolve the advisories:

```bash
cargo update -p rustls@0.23.45
cargo update -p rustls-webpki@0.103.15
cargo update -p webpki-roots@1.0.8
cargo update -p rustls-platform-verifier@0.7.0
cargo update -p quinn@0.11.11
cargo update -p quinn-proto@0.11.17
cargo update -p quinn-udp@0.5.15
cargo update -p tokio@1.52.3
```

Verify with `cargo tree -i` that the default graph resolves the fixed versions.
Do not use `--all-features` for validation; use the documented feature slices.

If `rustls 0.23.46` or `quinn 0.11.12` requires a source adaptation, keep it
mechanical and preserve the invariants above. Do not bump to a new major/minor
line (for example `rustls 0.24-dev`) in this milestone.

### B. Make the trust-store description truthful

Update the `with_system_roots` documentation and the security docs to say
“bundled Mozilla roots via webpki-roots”, not “system root store”. If a future
OS-store backend is desired, it is a separate decision.

### C. Make insecure TLS explicit and documented

Choose the smallest safe correction:

1. Keep the compiled feature but add a config-load warning when any hop has
   `insecure=true`, naming the redacted chain and the policy decision; and
2. Correct `docs/security/SECURE_CONFIGURATION.md`, `docs/SECURITY_REVIEW.md`,
   and `docs/URI_GRAMMAR.md` so `?insecure` is documented as reachable from
   CLI/config builds.

Do not remove `insecure-tls` from default builds in this milestone if that would
break pproxy-compatible `--ssl`/QUIC translation. A feature-gate change requires
a separate compatibility assessment.

### D. Redact TLS private-key `Debug`

Replace the derived `Debug` for `CompiledListenerTlsConfig` with a custom impl
that reports `has_cert`, `has_key`, and key length only. Add a unit/compile-time
guard asserting the PEM/key bytes do not appear in `format!("{:?}", config)`.
Check the reverse-TLS types as well; `eggress-protocol-reverse` already has a
redacting `Debug`, so preserve that pattern.

### E. Tests

Add focused tests that:

- assert the default client config is not insecure and builds from the current
  root set;
- assert `?insecure` still requires the expected feature path and does not
  silently substitute insecure verification when a `tls_override` is present;
- assert `CompiledListenerTlsConfig` Debug does not contain PEM/key material;
- retain the existing custom-CA, mTLS, ALPN-preservation, and H2 pool tests.

## 7. Ordered work packages

### Work package A — Dependency upgrade and tree verification

Intent: move the default graph to fixed versions without source churn.

Required changes: lockfile/workspace resolution; any mechanical API adaptation;
`cargo tree` evidence.

Acceptance evidence: `cargo tree -i` for rustls/rustls-webpki/webpki-roots/
quinn/tokio; `cargo audit` no longer reports the TLS/QUIC/tokio advisories; the
default `eggress-cli` feature graph is unchanged except for versions.

### Work package B — Trust-store and insecure-mode truth

Intent: stop overstating the root store and make insecure selection visible.

Required changes: rustdoc, security docs, config warning if selected, tests.

Acceptance evidence: docs review; warning test; no credential echoed.

### Work package C — Secret-bearing Debug redaction

Intent: remove the latent TLS private-key exposure.

Required changes: custom `Debug`; tests.

Acceptance evidence: Debug output contains no PEM/key bytes; existing reverse
TLS redaction test remains green.

## 8. Failure, cancellation, restart, and contention semantics

- A root-load/parse failure remains a startup/handshake error; it must not
  downgrade to insecure verification.
- A failed TLS handshake leaves the chain attempt failed; no retry to a
  different policy or root set.
- Insecure-mode warning is emitted before the first handshake, not per packet.
- TLS config caching (`OnceLock`) remains process-wide and policy-separated
  (`default_client_config` vs `default_insecure_client_config`).
- No listener topology/reload semantics change.

## 9. Compatibility and migration

- No API signature removal. `with_system_roots` keeps its name for source
  compatibility; only its documentation/behavioral description is corrected.
- Existing configs with `?insecure` continue to work but warn when selected.
- pproxy compatibility claims are unchanged; this is a dependency/trust-store
  corrective, not a tier change.
- `Cargo.lock` changes are expected; do not alter the four gated version
  locations or publish a release.

## 10. Required tests

### Focused unit tests

- `eggress-transport-tls`: default config not insecure; custom CA still used;
  mTLS key parsing; ALPN preservation; `tls_override` + insecure rejection.
- `eggress-config`: TLS listener `Debug` redaction.
- `eggress-uri`: `?insecure` parsing remains stable.

### Integration tests

- `eggress-runtime` TLS suite (existing `tests/tls.rs`).
- H2/QUIC feature-gated tests if the lockfile update touches quinn.

### Security and negative tests

- Insecure mode is never selected without an explicit request.
- Debug output contains no key bytes.
- Advisory check confirms the fixed versions.

### Migration and compatibility tests

- Existing `tls_override`, custom-CA, and pproxy `--ssl` translation tests remain
  green.

## 11. Required verification commands

```bash
cargo test -p eggress-transport-tls
cargo test -p eggress-config
cargo test -p eggress-runtime --test tls
cargo test -p eggress-embed --test error_redaction
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo audit --ignore RUSTSEC-2023-0071
cargo deny check
cargo check -p eggress-cli --locked --no-default-features \
  --features full,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins
```

`cargo deny check` may still fail on the `russh`/`zip` advisories until
Transports M005/Delivery M004 close; record the remaining IDs explicitly rather
than claiming a clean gate.

## 12. Documentation updates

- `docs/SECURITY_REVIEW.md` — root-store wording, insecure-mode
  reachability, unsafe/handshake claims if touched.
- `docs/security/SECURE_CONFIGURATION.md` — root store and insecure TLS.
- `docs/URI_GRAMMAR.md` — `?insecure` behavior.
- `architecture/transports-tls.md` — root source and policy boundary.
- `docs/security/THREAT_MODEL.md` — correct the “system root store” implication
  if present.

## 13. Acceptance criteria

- Default outbound TLS resolves `rustls >= 0.23.46`, `rustls-webpki >= 0.103.16`,
  `webpki-roots >= 1.0.10`, and a fixed quinn/tokio graph.
- `cargo audit` no longer reports RUSTSEC-2026-0381/-0382 or the listed
  quinn/tokio IDs for the default feature graph.
- No insecure verification is selected without an explicit request.
- TLS private keys never appear in `Debug`.
- Existing TLS/ALPN/custom-CA/H2 tests remain green.
- Documentation states the bundled-root behavior accurately.

## 14. Stop conditions

Stop and report rather than improvise if:

- the fixed versions require a semver-breaking API change in rustls/quinn;
- MSRV 1.89 is insufficient;
- `cargo update` cannot resolve a fixed version without a source change;
- the insecure-mode correction would require a compatibility-tier change;
- `cargo deny` cannot be interpreted because another milestone is mid-flight.

## 15. Closure evidence required

- Lockfile diff and `cargo tree -i` output for every affected crate.
- `cargo audit`/`cargo deny` output, with any remaining IDs explained.
- Focused test output and broad gate output.
- Documentation diff.
- Explicit statement that no release/tag was created.

## 16. Handoff notes

- Do not run `cargo update` broadly; update only the named packages.
- Do not edit `Cargo.toml` version numbers; this is not a release.
- Preserve user changes; check `git status` before editing.
- If `rustls-webpki` resolves to `0.103.17`, that is acceptable (>=0.103.16).
- `webpki-roots 0.26.11` is a compatibility re-export; verify its 1.x parent,
  not only the 0.26 version string.
