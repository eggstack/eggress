# Corrective Parity Audit Plan

## Purpose

Phases 7–12 landed a substantial pproxy-parity implementation pass: parity documentation, pproxy-compatible CLI/URI translation, Shadowsocks TCP and UDP work, remaining protocol classification, scheduler/chain/failure semantics tests, and completion records.

The pass is meaningful, but it introduced several areas where compatibility claims may be stronger than the evidence. This corrective audit is designed to prevent Eggress from overclaiming true pproxy/Shadowsocks parity while retaining the useful implementation work.

This is an audit and correction plan. Do not add broad new features. Fix claims, tests, and targeted correctness gaps.

---

# Audit targets

Primary risk areas:

1. Shadowsocks TCP AEAD stream framing may not match the standard Shadowsocks AEAD TCP format.
2. Shadowsocks TCP tests appear mostly Eggress-to-Eggress synthetic rather than known-good interop.
3. Shadowsocks capability classification now marks both TCP and UDP supported; this may overclaim.
4. Shadowsocks UDP looks structurally plausible but still lacks known-good interop evidence.
5. pproxy repeated `-r`/remote semantics may have been translated as upstream-group failover rather than ordered chain semantics.
6. Phase 12 claims broad chain/failure parity while documenting unresolved gaps.
7. Trojan multihop remains untested because of TLS trust configuration.
8. pproxy differential tests are gated and not run due to local Python/pproxy compatibility issues.
9. Completion docs may describe attempted phases as closed when they should say partial/pending audit.

---

# Non-goals

Do not implement:

- Python bindings;
- PyPI packaging;
- new protocols;
- inbound Shadowsocks listener;
- legacy Shadowsocks ciphers;
- plugin transports;
- QUIC/MASQUE/CONNECT-UDP;
- transparent proxying;
- unsafe Rust;
- native TLS/OpenSSL.

Do not rewrite the entire Shadowsocks implementation unless the audit proves it is incompatible and a minimal correction path is clear.

---

# Workstream 1: Establish authoritative Shadowsocks TCP framing contract

## Goal

Determine whether the current Shadowsocks TCP implementation matches the standard AEAD TCP format used by interoperable implementations.

## Current concern

The current `ShadowsocksAeadStream` documents and implements a frame shaped like:

```text
[2-byte plaintext ciphertext length]
[AEAD(2-byte plaintext length + plaintext)]
```

It also uses nonce 0 for the encrypted address header and starts stream nonces at 1.

This may be internally coherent, but it may not match standard Shadowsocks AEAD TCP framing. If not standard, it must not be called Shadowsocks TCP parity.

## Tasks

1. Review `docs/protocols/SHADOWSOCKS_PARITY.md` and verify its TCP framing section against a known Shadowsocks AEAD TCP specification.
2. Inspect:
   - `crates/eggress-protocol-shadowsocks/src/aead.rs`;
   - `crates/eggress-protocol-shadowsocks/src/tcp.rs`;
   - `crates/eggress-protocol-shadowsocks/src/tcp_stream.rs`.
3. Write a local note in `docs/protocols/SHADOWSOCKS_TCP_AUDIT.md` with:
   - expected standard format;
   - current Eggress format;
   - exact differences;
   - compatibility impact;
   - correction decision.
4. If the implementation is non-standard, immediately downgrade public support claims before any larger refactor.

## Acceptance criteria

- The repo contains a clear Shadowsocks TCP audit document.
- The audit states whether current TCP framing is standard-compatible, Eggress-specific, or inconclusive.
- No doc claims true Shadowsocks TCP parity unless the audit supports it.

---

# Workstream 2: Add known-good Shadowsocks interop tests

## Goal

Replace Eggress-to-Eggress synthetic confidence with interop evidence.

## Acceptable known-good targets

Use one or more, gated behind an environment variable:

- `ssserver` / `sslocal` from shadowsocks-rust;
- shadowsocks-libev if available;
- another maintained Shadowsocks implementation;
- official or community test vectors if a live server is unavailable.

## Required environment gate

Use explicit gates so normal tests remain hermetic:

```bash
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1
```

Optional tool path variables:

```bash
EGRESS_SSSERVER_BIN=/path/to/ssserver
EGRESS_SSLOCAL_BIN=/path/to/sslocal
```

## TCP scenarios

Add gated tests, for example:

```text
crates/eggress-cli/tests/interoperability_shadowsocks.rs
```

Required scenarios:

1. Eggress Shadowsocks upstream client -> known-good Shadowsocks server -> local TCP echo.
2. Known-good Shadowsocks client -> Eggress Shadowsocks server helper if inbound server mode is intentionally test-only; otherwise skip and document upstream-only.
3. Wrong password failure.
4. All supported methods, or at minimum `aes-256-gcm`.

## UDP scenarios

1. Eggress SOCKS5 UDP -> Eggress Shadowsocks UDP upstream client -> known-good Shadowsocks UDP server -> local UDP echo.
2. Wrong password/drop behavior.
3. At least one supported method.

## Acceptance criteria

- Interop tests exist and skip cleanly when tool binaries are absent unless the gate is set.
- If tests fail against a known-good implementation, docs and capability classifier are downgraded.

---

# Workstream 3: Correct Shadowsocks TCP implementation or downgrade claims

## Goal

Ensure the code and docs do not overstate Shadowsocks TCP support.

## Decision A: implementation is standard-compatible

If Workstream 1 and known-good interop prove compatibility:

- keep capability classifier as supported;
- add interop evidence to completion docs;
- add exact method list and interop command examples;
- preserve runtime tests.

## Decision B: implementation is non-standard but easy to correct

If framing is wrong but correction is local:

1. Replace stream framing with the standard format.
2. Add unit tests that inspect raw frame structure.
3. Add known-good interop tests.
4. Update docs to describe the corrected standard format.
5. Ensure old synthetic tests do not simply mirror the bug.

## Decision C: implementation is non-standard and correction is not feasible in this pass

If correction is too large:

1. Downgrade Shadowsocks TCP to `Experimental` or `Partial`.
2. Change `classify_single_protocol(Shadowsocks)` so TCP is not advertised as compatible.
3. Keep code behind explicit experimental docs.
4. Update README, `PARITY_MATRIX.md`, `SHADOWSOCKS_PARITY.md`, and Phase 9 completion docs.
5. Add a future plan item for real standard AEAD TCP.

## Acceptance criteria

- The capability classifier and docs match evidence.
- No unsupported or non-standard Shadowsocks TCP behavior is marketed as pproxy-compatible.

---

# Workstream 4: Verify Shadowsocks UDP standard compatibility

## Goal

Validate whether the current UDP packet implementation is actually interoperable.

## Current status

The implementation uses:

```text
salt + AEAD(address + payload)
```

This appears plausible for standard Shadowsocks UDP, but the repo still lacks known-good interop evidence.

## Tasks

1. Inspect `crates/eggress-protocol-shadowsocks/src/udp.rs` for:
   - salt size per method;
   - subkey derivation;
   - nonce choice;
   - address format;
   - payload extraction;
   - datagram size handling;
   - error handling.
2. Add structural tests that validate the salt is cleartext and address is encrypted.
3. Add known-good UDP interop if possible.
4. If interop cannot be run, downgrade docs from “interoperable” to “implemented to documented standard format, pending interop.”

## Acceptance criteria

- UDP claims are evidence-aligned.
- If interop is unavailable, docs say so plainly.

---

# Workstream 5: Re-audit pproxy repeated remote/chain semantics

## Goal

Confirm whether Phase 8 maps pproxy remotes correctly.

## Current concern

The translator maps multiple `-r` remotes into one Eggress upstream group with members and scheduler behavior. That is failover/load-balancing semantics, not necessarily ordered chain semantics.

## Tasks

1. Use `docs/PPROXY_PARITY_SPEC.md` to identify documented pproxy behavior for multiple remotes.
2. Add a black-box pproxy probe if needed:
   - start two synthetic upstream proxies;
   - pass repeated `-r` arguments;
   - observe whether traffic traverses both in order or selects one;
   - repeat with scheduler flags.
3. Document result in `docs/PPROXY_PARITY_SPEC.md`.
4. Fix `eggress-pproxy-compat` translation if wrong.

## Correction rules

If repeated remotes mean ordered chain:

- generated config should create one upstream chain/hop sequence, not a group of alternatives;
- scheduler should only apply where pproxy actually treats remotes as alternatives;
- docs/tests must distinguish chain mode from group mode.

If repeated remotes mean alternatives:

- current group translation may be acceptable;
- add explicit tests and docs proving the mapping.

## Tests

Add or adjust tests in:

```text
crates/eggress-pproxy-compat/src/tests.rs
crates/eggress-cli/tests/pproxy_cli.rs
crates/eggress-cli/tests/differential_pproxy.rs
```

## Acceptance criteria

- Repeated remote semantics are documented and tested.
- Translator does not silently convert chain semantics into failover semantics.

---

# Workstream 6: Repair capability classifier and parity matrix overclaims

## Goal

Keep runtime capability classification and documentation conservative.

## Files to inspect

```text
crates/eggress-core/src/capability.rs
docs/PARITY_MATRIX.md
docs/PPROXY_PARITY_SPEC.md
docs/protocols/SHADOWSOCKS_PARITY.md
docs/protocols/SHADOWSOCKS_UDP_PARITY.md
docs/PHASE_9_SHADOWSOCKS_TCP_PARITY_COMPLETION.md
docs/PHASE_10_SHADOWSOCKS_UDP_AND_UDP_PARITY_COMPLETION.md
README.md
```

## Required correction if evidence is incomplete

Use conservative tiers:

- `Compatible`: only with runtime and interop/differential evidence.
- `Supported`: internal Eggress support, no pproxy/known-good claim.
- `Partial`: usable but missing compatibility behavior.
- `Experimental`: code exists, no compatibility promise.

## Acceptance criteria

- `classify_upstream_chain` does not mark Shadowsocks TCP/UDP supported beyond evidence.
- Docs distinguish Eggress synthetic tests from known-good interop.
- Completion docs include residual caveats instead of absolute closure claims.

---

# Workstream 7: Fix Trojan multihop test gap

## Goal

Close the documented Phase 12 blocker: SOCKS5 -> Trojan multihop test is absent because of TLS trust configuration.

## Tasks

1. Inspect Trojan client config path and TLS options.
2. Decide on a test-only insecure/self-signed trust option.
3. Prefer a test-only connector or rustls root store injection over a production insecure default.
4. Add runtime test:

```text
crates/eggress-runtime/tests/multihop_tcp.rs
```

Scenario:

```text
inbound SOCKS5 -> upstream SOCKS5 or HTTP -> Trojan upstream -> TCP echo
```

or, if chain semantics make this shape awkward:

```text
inbound SOCKS5 -> Trojan upstream -> TCP echo
```

as a minimum exported runtime path.

5. Assert metrics and session cleanup.
6. Update Phase 12 completion docs.

## Acceptance criteria

- Trojan multihop or at least Trojan-in-chain test exists.
- No production insecure TLS default is introduced.

---

# Workstream 8: Differential-test environment repair

## Goal

Make pproxy differential testing runnable despite local Python compatibility issues.

## Current problem

Completion docs mention pproxy incompatibility with the local Python 3.14 environment.

## Options

Choose one:

1. Use a pinned Python version known to run pproxy, such as 3.11 or 3.12, via uv/pyenv in docs.
2. Add a Docker/devcontainer recipe for pproxy differential tests.
3. Vendor a minimal pproxy test runner environment script.
4. Continue gating but mark all unrun differential tests as unverified, not compatible.

## Required docs

Update or create:

```text
docs/DIFFERENTIAL_TESTING.md
```

Include:

- Python version requirement;
- pproxy install command;
- exact environment variables;
- command to run all gated differential tests;
- known failures/skips.

## Acceptance criteria

- A maintainer has a reproducible path to run pproxy differential tests.
- Docs do not treat unrun gated tests as evidence.

---

# Workstream 9: Completion-doc truth pass

## Goal

Make the phase completion records reflect the corrected audit outcome.

## Files

```text
docs/PHASE_7_PPROXY_PARITY_SPEC_COMPLETION.md
docs/PHASE_8_PPROXY_COMPAT_CLI_URI_COMPLETION.md
docs/PHASE_9_SHADOWSOCKS_TCP_PARITY_COMPLETION.md
docs/PHASE_10_SHADOWSOCKS_UDP_AND_UDP_PARITY_COMPLETION.md
docs/PHASE_11_REMAINING_PROTOCOL_PARITY_COMPLETION.md
docs/PHASE_12_SCHEDULER_CHAIN_FAILURE_PARITY_COMPLETION.md
```

## Required wording rules

- Use `implemented` only for code that exists.
- Use `verified compatible` only for behavior proven against pproxy or a known-good implementation.
- Use `runtime-tested` only for full ServiceSupervisor/end-to-end tests.
- Use `synthetic-tested` for Eggress-owned client/server fixtures.
- Use `gated, not yet run` for differential tests not actually run.
- Keep blockers visible.

## Acceptance criteria

- Completion docs no longer overstate parity or interop.

---

# Recommended commit sequence

## Commit 1: Add audit docs and downgrade unsupported claims if immediately necessary

- Add `docs/protocols/SHADOWSOCKS_TCP_AUDIT.md`.
- Update capability/docs if current evidence is already insufficient.

## Commit 2: Shadowsocks TCP spec correction or downgrade

- Either fix standard AEAD TCP framing or mark it partial/experimental.
- Add structural tests.

## Commit 3: Known-good Shadowsocks interop harness

- Add gated tests and docs.
- Do not make normal CI depend on external binaries.

## Commit 4: Shadowsocks UDP interop/claim correction

- Add interop if possible.
- Otherwise revise docs to “pending interop.”

## Commit 5: pproxy repeated remote semantics correction

- Add probes.
- Fix translator if needed.
- Update docs/tests.

## Commit 6: Trojan chain test gap

- Add test-only trust handling.
- Add runtime test.

## Commit 7: Differential test environment docs

- Add `docs/DIFFERENTIAL_TESTING.md`.
- Update AGENTS/README if useful.

## Commit 8: Completion-doc truth pass

- Update phase completion records and parity matrix.

---

# Required verification

Normal checks:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Focused checks:

```bash
cargo test -p eggress-protocol-shadowsocks
cargo test -p eggress-runtime shadowsocks_tcp
cargo test -p eggress-runtime shadowsocks_udp
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli pproxy
cargo test -p eggress-runtime multihop_tcp
cargo test -p eggress-runtime failure_semantics
cargo test -p eggress-runtime scheduler_runtime
```

Gated checks:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored
```

If gated checks cannot run, completion docs must say they are unverified.

---

# Definition of done

This corrective audit is complete only when:

1. Shadowsocks TCP framing is audited against a known standard.
2. Shadowsocks TCP is either corrected or downgraded from parity claims.
3. Shadowsocks UDP is either known-good interop-tested or documented as pending interop.
4. Capability classifier matches evidence.
5. pproxy repeated remote semantics are verified and translator behavior is corrected if needed.
6. Trojan multihop or Trojan-in-chain runtime test gap is addressed or explicitly deferred with a narrower claim.
7. Differential-test environment has reproducible setup docs.
8. Completion records distinguish implemented, runtime-tested, synthetic-tested, interop-tested, and unverified.
9. No unsupported behavior silently falls back to direct or claims compatibility.
10. Normal local verification passes.

## Completion record

Add:

```text
docs/CORRECTIVE_PARITY_AUDIT_COMPLETION.md
```

Required sections:

- commit list;
- Shadowsocks TCP audit outcome;
- Shadowsocks UDP audit outcome;
- pproxy repeated remote semantics outcome;
- capability/doc changes;
- Trojan test outcome;
- differential-test environment status;
- remaining blockers for Phase 13;
- exact local verification commands and results.
