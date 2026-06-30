# Phase 23 Plan: Parity Evidence Tightening and Cleanup

## Purpose

The recent Phase 18-22 work substantially improved Eggress: the repo now has a pproxy oracle/testkit direction, a compatibility manifest, expanded HTTP/SOCKS differential coverage, standalone UDP relay support, standard Shadowsocks AEAD framing work, Shadowsocks inbound/UDP support, and clear SSR/legacy rejection diagnostics.

This cleanup pass is not primarily a feature phase. Its purpose is to make the repo's claims, test evidence, CI workflows, manifests, docs, and compatibility labels internally consistent and release-auditable.

The expected outcome is a tighter repo where a reviewer can answer four questions without ambiguity:

1. What is implemented?
2. What has real pproxy differential evidence?
3. What has external protocol interoperability evidence?
4. What is intentionally unsupported despite pproxy support?

## Current observed state

The repo appears to have moved quickly through a large amount of implementation work. The shape is much better, but several cleanup risks remain:

- The compatibility manifest has strong structure, but some entries still use `egress_status = "compatible"` with `evidence_level = "implemented_synthetic"`.
- The manifest metadata contains a stale `last_updated` value.
- README and parity docs include stronger compatibility language than can be verified from visible CI status.
- The pproxy compatibility workflow exists, but there are no visible workflow runs/statuses attached to the latest commit in the observed repository state.
- The pproxy workflow installs pproxy but does not install Shadowsocks tooling, while Shadowsocks docs claim extensive interop coverage.
- `docs/DIFFERENTIAL_TESTING.md` does not fully enumerate the newer Phase 19 HTTP/SOCKS cases recorded in the manifest.
- Standalone UDP exists, but UDP multi-hop remains explicitly unsupported; docs must ensure this distinction stays visible.
- SSR/legacy handling is intentionally unsupported, which is good, but the diagnostics and documentation should be checked for complete coverage.

This phase should treat the codebase as a serious parity-work-in-progress and remove ambiguity before later long-tail protocol phases continue.

## Non-goals

Do not implement Trojan server mode, SSH transport, transparent proxying, HTTP/2, HTTP/3, QUIC, WebSocket tunnels, Python drop-in API parity, or UDP multi-hop in this phase unless a small fix is required to keep existing docs truthful.

Do not expand protocol scope. This pass is about evidence, consistency, CI, and correctness of current claims.

Do not mark additional features compatible unless their tests actually run in the relevant workflow or are explicitly documented as manually gated evidence.

## Work items

### 23.1 Manifest invariant enforcement

Add automated validation for `tests/compat/pproxy_manifest.toml`.

Required invariants:

- `egress_status = "compatible"` requires `evidence_level = "compatible"`.
- `evidence_level = "compatible"` requires at least one named test.
- Any compatible protocol feature that depends on real pproxy must list `external_dependency = "pproxy==2.7.9"` unless the compatibility evidence is strictly third-party interop.
- `implemented_synthetic` cannot be paired with user-facing compatibility claims.
- `intentional_non_parity` must include a non-empty divergence rationale.
- All feature IDs must be unique.
- All evidence levels must be from the allowed enum.
- All statuses must be from the allowed enum.
- `meta.pproxy_version` must match the pinned target config.
- `meta.last_updated` must be a real current date or removed in favor of generated report metadata.

Suggested implementation options:

- Add a Rust test in `crates/eggress-testkit` that parses the TOML manifest and validates invariants.
- Or add a small Python script under `scripts/validate_pproxy_manifest.py` and call it from CI.

Prefer Rust if dependencies are already present; prefer Python if faster to maintain and easier for docs contributors.

Acceptance:

```bash
cargo test -p eggress-testkit manifest
# or
python scripts/validate_pproxy_manifest.py
```

must fail on inconsistent compatibility/evidence combinations.

### 23.2 Normalize manifest statuses

Audit every manifest entry and normalize status/evidence pairs.

Specific expected changes:

- URI syntax entries such as `http_scheme`, `socks5_scheme`, `socks4_scheme`, `chain_separator`, and `auth_in_uri` should not say `egress_status = "compatible"` unless their evidence is upgraded from synthetic to compatible by real pproxy parser/differential tests.
- If those tests do not exist yet, downgrade `egress_status` to `supported` or `implemented_synthetic` wording according to the manifest schema.
- For Shadowsocks entries, distinguish `compatible with standard Shadowsocks interop` from `compatible with pproxy` unless pproxy differential tests actually cover the path.
- For `socks5_udp_associate_server` and standalone UDP, separate standards-compliant SOCKS5 UDP ASSOCIATE from pproxy standalone UDP relay evidence.
- For SSR and legacy stream ciphers, keep `intentional_non_parity`, but ensure diagnostics tests are named and present.

Acceptance:

- No manifest row overclaims compatibility.
- Every `compatible` row has test evidence that can be run or is explicitly marked manually gated.
- Manifest comments explain any feature that has `supported` but not `compatible` status.

### 23.3 Add a manifest-to-doc consistency check

Add a check that prevents README and parity matrix drift.

Minimum viable approach:

- Add a generated or semi-generated `docs/COMPATIBILITY_EVIDENCE.md` from the manifest.
- README and `docs/PARITY_MATRIX.md` should link to this generated evidence table.
- Add CI that verifies generated evidence output is current.

Better approach:

- Create a small generator that reads the manifest and emits tables grouped by category.
- Use generated tables for `docs/COMPATIBILITY_EVIDENCE.md` only, not necessarily for README.
- Add a test that fails if README claims a feature is `pproxy-compatible` while the manifest does not mark it compatible.

Acceptance:

- There is one canonical evidence source.
- Documentation cannot silently drift ahead of the manifest.

### 23.4 Fix CI visibility and workflow execution

The new `.github/workflows/pproxy-compat.yml` is the right shape but must be made visibly useful.

Tasks:

- Confirm workflow triggers on `push`, `pull_request`, and `workflow_dispatch`.
- Confirm repository Actions permissions/billing state allows the workflow to run.
- Add a workflow badge or documented note only after the workflow actually runs.
- Ensure the workflow runs manifest validation before differential tests.
- Ensure parity report generation is not silently ignored in normal operation. The current `|| true` around report generation is acceptable for early bootstrapping only; convert to required once stable.
- Upload logs/artifacts even on failure.
- Add a short `docs/CI_STATUS.md` update with the exact expected workflow names.

Acceptance:

- Latest commit or a new cleanup commit has visible status checks or a documented reason why hosted CI cannot run.
- The pproxy compatibility workflow produces artifacts or fails loudly.

### 23.5 Split pproxy differential CI from Shadowsocks interop CI

The pproxy workflow installs pproxy but not Shadowsocks tooling. Shadowsocks interop should be a separate workflow or clearly separate job.

Add one of the following:

Option A: separate workflow:

```text
.github/workflows/shadowsocks-interop.yml
```

Option B: separate job in the compatibility workflow:

```yaml
shadowsocks-interop:
  runs-on: ubuntu-latest
```

Requirements:

- Install Rust stable.
- Install or build `shadowsocks-rust` tools providing `ssserver` and `sslocal`.
- Verify `ssserver --help` and `sslocal --help`.
- Run `EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1`.
- Upload logs on failure.
- Keep it allowed-to-fail only if the docs clearly say Shadowsocks interop is manually gated, not release evidence.

Acceptance:

- Shadowsocks interop claims are backed by a runnable workflow or downgraded to manually gated support language.

### 23.6 Update `docs/DIFFERENTIAL_TESTING.md`

Bring the differential testing docs in line with the current test suite and manifest.

Required updates:

- Enumerate Phase 19 HTTP CONNECT cases: auth success/rejection, IPv4, domain, IPv6, refused target.
- Enumerate ordinary HTTP forward cases: GET, persistent connection, POST body, HEAD, Connection: close.
- Enumerate SOCKS4 and SOCKS4a cases.
- Enumerate SOCKS5 IPv6, domain, refused target, auth, and malformed/unsupported cases if implemented.
- Enumerate standalone UDP cases if they exist.
- Separate pproxy differential tests from standard protocol interop tests.
- For every gated test group, state whether it is run in CI, manually run, or skipped by default.
- Remove phrases that imply tests pass in CI if they only pass manually.

Acceptance:

- `docs/DIFFERENTIAL_TESTING.md` can be used by a new contributor to run the exact same evidence suite.
- Test names in the doc match actual test function names.

### 23.7 Audit README language for claim discipline

Review all README capability claims introduced in the recent implementation rush.

Rules:

- Use `[x]` only for implemented behavior that has tests.
- Use `pproxy-compatible` only when the manifest says compatible and the evidence is real.
- Use `standard-compatible` or `interoperable with standard implementations` only when the Shadowsocks interop job or manual report is present.
- For Shadowsocks, distinguish:
  - standard SIP003 wire format;
  - interop with `shadowsocks-rust`;
  - interop with Python pproxy;
  - pproxy CLI/API parity.
- For standalone UDP, distinguish:
  - standalone pproxy-compatible UDP listener;
  - one-hop upstream support;
  - no UDP multi-hop chains.
- For SSR, keep the intentional non-parity section visible and explicit.

Acceptance:

- README does not overstate evidence.
- Terms `compatible`, `supported`, `interop`, `synthetic`, and `intentional non-parity` are used consistently.

### 23.8 Audit parity matrix for internal contradictions

Review `docs/PARITY_MATRIX.md` and ensure it agrees with the manifest and README.

Specific checks:

- `SOCKS5 UDP ASSOCIATE` should not be described as pproxy-compatible merely because standalone UDP is compatible; these are distinct modes.
- `Standalone UDP relay` should point to standalone UDP tests, not only `differential_socks5_udp_associate`, unless that test actually exercises standalone `-ul` semantics.
- `Shadowsocks TCP` should say `Supported` or `Compatible` according to actual pproxy differential evidence, not only standard interop.
- `Shadowsocks UDP` should distinguish standard interop from pproxy parity.
- `HTTP forward proxy` should remain compatible only if persistent differential tests are implemented and passing.
- UDP chain should remain partial/unsupported until multi-hop exists.
- Python API should remain `supported` rather than `drop-in compatible`.

Acceptance:

- Every parity-matrix `Compatible` row maps to a manifest `compatible` row.
- Every `Partial` row has a note describing the remaining gap.

### 23.9 Audit generated or declared test names

The manifest and docs now name many tests. Ensure they actually exist.

Tasks:

- Add a script/test that scans Rust test files and Python test files for declared test names.
- Compare against `tests = [...]` entries in the manifest.
- Permit group aliases like `integration_tests` only if explicitly whitelisted.
- Fail CI if a manifest references a nonexistent concrete test name.

Acceptance:

- No stale test references in the manifest.
- Group aliases are intentional and documented.

### 23.10 Harden gated test skip semantics

Gated tests must be clear about whether they are skipped, passed, or unavailable.

Tasks:

- Ensure each gated test prints or records a skip reason when env vars or external tools are missing.
- Avoid docs saying `PASS` for tests that were skipped.
- In CI, use required env vars so tests fail if external tools are unavailable in a required job.
- In local default test runs, skip gracefully.
- Add a report field distinguishing `skipped_missing_gate`, `skipped_missing_tool`, `passed`, `failed`, and `not_run`.

Acceptance:

- Gated test output cannot be mistaken for passing evidence when tests did not run.

### 23.11 Validate standalone UDP claims with targeted tests

Standalone UDP is an important new compatibility claim. Add or audit targeted tests.

Required cases:

- standalone direct UDP echo without any TCP control channel;
- standalone UDP malformed short datagram;
- standalone UDP nonzero FRAG behavior according to captured pproxy behavior;
- two clients on same UDP listener;
- two targets from one client;
- standalone UDP through one-hop SOCKS5 upstream, if claimed;
- standalone UDP through one-hop Shadowsocks upstream, if claimed;
- explicit rejection of UDP multi-hop with clear diagnostics.

Acceptance:

- Parity docs point to standalone-specific tests, not only SOCKS5 UDP ASSOCIATE tests.
- UDP multi-hop remains visibly unchecked/partial.

### 23.12 Validate Shadowsocks standardization claims with targeted tests

Audit whether the new Shadowsocks implementation is fully covered.

Required coverage:

- standard SIP003 TCP AEAD chunk framing unit tests;
- encrypted length block and encrypted payload block tests;
- nonce increment tests;
- address header encryption/decryption tests;
- large payload multi-chunk test;
- domain target test;
- IPv4 and IPv6 target tests;
- wrong password test;
- inbound server test;
- outbound/upstream client test;
- UDP client/server interop tests;
- standard implementation interop through `ssserver`/`sslocal`.

Acceptance:

- Shadowsocks README/docs claims cite the test groups.
- If standard external interop is not in CI, docs say manually gated rather than CI-backed.

### 23.13 Update completion docs or add corrective note

Recent work may have completion docs that now overstate certainty. Add a corrective completion document for this cleanup phase.

Suggested path:

```text
docs/PHASE_23_PARITY_EVIDENCE_TIGHTENING_COMPLETION.md
```

Once implemented, it should list:

- manifest invariants added;
- docs normalized;
- workflows added/fixed;
- claims downgraded or upgraded;
- gated tests confirmed;
- known remaining gaps.

Do not write this completion doc until the cleanup pass is actually complete.

### 23.14 Re-run full local validation suite

Final validation should include normal, gated, and documentation checks.

Baseline:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Compatibility:

```bash
python3 -m pip install "pproxy==2.7.9"
python3 -m pproxy --help
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test interoperability_pproxy -- --ignored --test-threads=1
```

Shadowsocks interop, if external tools are available:

```bash
cargo install shadowsocks-rust
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
```

Docs/evidence:

```bash
cargo test -p eggress-testkit manifest
cargo test -p eggress-testkit report -- --ignored
# or equivalent scripts if implemented in Python
```

Python package, if touched:

```bash
python -m ruff check python/
python -m mypy python/eggress --ignore-missing-imports
python -m pytest python/tests
```

## Acceptance criteria for the phase

This cleanup phase is complete when:

- Manifest invariants are enforced by tests or scripts.
- No manifest entry claims `compatible` with only synthetic evidence.
- Manifest metadata is current and agrees with the pinned target config.
- README, parity matrix, migration guide, and differential testing docs agree with the manifest.
- pproxy differential CI is visible, runnable, and documented.
- Shadowsocks interop is either CI-backed or explicitly marked manually gated.
- Standalone UDP docs point to standalone-specific evidence and still show UDP multi-hop as incomplete.
- SSR/legacy non-parity is consistently documented and diagnostically enforced.
- Test names referenced by docs/manifest exist or are whitelisted group aliases.
- A completion doc records what was tightened and what remains.

## Remaining gaps expected after this phase

This phase should not pretend to finish the entire pproxy parity roadmap. Expected remaining gaps include:

- UDP multi-hop chains.
- Trojan server/listener and Trojan interop tests.
- SSH upstream transport.
- Transparent proxy/redir/PF and Unix sockets.
- HTTP/2, HTTP/3, QUIC, WebSocket, raw tunnel, reverse/backward proxying.
- System proxy configuration.
- True pproxy-shaped Python API drop-in replacement.
- Potential legacy Shadowsocks/SSR final non-parity depending on ADR policy.

## Handoff notes

Treat this as an evidence-hardening pass, not a feature sprint. The implementation work from Phases 18-22 is valuable, but it moved fast enough that documentation and manifest rigor now matter as much as code. The core success condition is that future contributors cannot accidentally overclaim parity by editing README or `PARITY_MATRIX.md` without corresponding manifest and test evidence.
