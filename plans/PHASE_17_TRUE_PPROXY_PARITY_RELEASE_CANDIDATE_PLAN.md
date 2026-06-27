# Phase 17 Detailed Plan: True `pproxy` Parity Release Candidate Audit

## Purpose

Phase 17 is the final release-candidate audit before Eggress can credibly claim to be a Rust-native pproxy alternative with Python bindings. This phase should not add broad new functionality. It verifies the full system, corrects documentation, classifies remaining non-parity, and defines the release candidate boundary.

The release claim must be precise:

- Rust-native pproxy-style proxy service;
- Python package that embeds Rust networking/runtime;
- compatibility for documented common pproxy use cases;
- explicit non-parity for unsupported or intentionally rejected behavior.

Do not claim full pproxy parity if any material pproxy feature remains unclassified or falsely marked compatible.

---

# Prerequisites

Required:

- Phase 7 parity spec complete;
- Phase 8 pproxy CLI/URI compatibility complete;
- Phase 9/10 Shadowsocks status corrected by audit;
- Phase 11 remaining protocol matrix complete;
- Phase 12 scheduler/chain/failure semantics complete;
- corrective parity audit complete;
- Phase 13 Rust embed API complete;
- Phase 14 Python bindings complete;
- Phase 15 PyPI/wheel release pipeline complete;
- Phase 16 Python pproxy library helpers complete.

If any phase is incomplete, Phase 17 should record blockers rather than rubber-stamp release readiness.

---

# Non-goals

Do not implement:

- large protocol rewrites;
- standard Shadowsocks TCP rework;
- new Python APIs beyond release blockers;
- new scheduler families;
- new admin UI features;
- public-internet-dependent tests;
- unsafe Rust;
- OpenSSL/native-tls.

This is a release audit and targeted fix phase.

---

# Workstream 1: Final parity matrix audit

## Goal

Make `docs/PARITY_MATRIX.md` the authoritative release candidate truth source.

## Required review

Every row must have:

- feature;
- pproxy behavior;
- Eggress behavior;
- tier;
- Rust runtime test reference;
- differential/interop evidence if compatible;
- Python binding support if exposed;
- notes/non-parity rationale.

## Required tier rules

- `Compatible`: tested against pproxy or known-good implementation where applicable.
- `Supported`: works in Eggress, but pproxy equivalence is not claimed.
- `Partial`: useful subset but missing documented behavior.
- `Experimental`: code exists but no compatibility promise.
- `Intentional non-parity`: deliberately rejected with rationale.
- `Unsupported`: not implemented.

## Blocker examples

- Shadowsocks TCP cannot be `Compatible` while non-standard framing remains.
- Unrun gated tests cannot count as compatibility proof.
- Python helper support cannot be claimed if bindings do not expose it.

## Acceptance criteria

- No row uses vague status language.
- No compatible claim lacks test evidence.

---

# Workstream 2: Rust runtime release audit

## Goal

Verify the Rust service remains stable after all parity and embedding work.

## Required checks

- listener startup/shutdown;
- direct TCP relay;
- HTTP CONNECT;
- HTTP forward path status, including known limitation for persistent forwarding if still present;
- SOCKS4/SOCKS4a;
- SOCKS5 CONNECT;
- SOCKS5 UDP ASSOCIATE direct;
- SOCKS5 UDP through supported upstreams;
- HTTP/SOCKS/Trojan upstream TCP;
- Shadowsocks UDP if claimed supported;
- Shadowsocks TCP only as experimental/non-standard;
- multi-hop TCP for documented combinations;
- scheduler/fallback/retry behavior;
- failure mapping;
- metrics/admin/reload;
- security invariants.

## Acceptance criteria

- Runtime tests cover all release-claimed supported behavior.
- Known gaps are in release candidate docs.

---

# Workstream 3: Python package release audit

## Goal

Verify the Python package is installable, usable, and honest.

## Required checks

- `pip install` from local wheel;
- import package;
- version matches Rust crate;
- type hints included;
- service starts/stops;
- bound addresses visible;
- local SOCKS5 direct traffic works;
- local HTTP CONNECT direct traffic works if exposed;
- pproxy translation helpers work;
- start from pproxy args works;
- metrics/status/reload work;
- errors map to Python exceptions;
- credentials redacted;
- context manager cleanup;
- async helper if implemented;
- multiple service behavior tested or documented.

## Acceptance criteria

- Python package can replace common pproxy subprocess/library usage for documented features.

---

# Workstream 4: Differential and interop evidence audit

## Goal

Separate proven compatibility from unverified implementation.

## Evidence classes

Use these terms consistently:

- `unit-tested`;
- `runtime-tested`;
- `synthetic-tested`;
- `known-good interop-tested`;
- `pproxy differential-tested`;
- `gated, not run`;
- `documented non-parity`.

## Required audit documents

Update:

```text
docs/DIFFERENTIAL_TESTING.md
docs/PARITY_MATRIX.md
docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md
```

## Gated tests

Run if environment allows:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored
EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 python -m pytest python/tests/test_pproxy_differential.py
```

If not run, mark unverified.

## Acceptance criteria

- Release docs do not count unrun gated tests as proof.

---

# Workstream 5: Security and redaction audit

## Goal

Ensure release candidate does not leak secrets or weaken prior security posture.

## Required audit targets

Rust:

- URI redaction;
- logs;
- metrics labels;
- admin endpoints;
- route explain;
- errors;
- config validation;
- TLS test-only insecure config not reachable from production defaults;
- UDP amplification controls;
- dependency policy;
- no unsafe Rust.

Python:

- exception strings;
- `repr()` output;
- translation warnings;
- generated TOML handling;
- metrics/status;
- context-manager cleanup;
- no import-time side effects.

## Acceptance criteria

- `docs/SECURITY_REVIEW.md` updated with Python-binding surface.
- High-severity issues are release blockers.

---

# Workstream 6: Packaging and supply-chain audit

## Goal

Confirm PyPI artifacts are safe and reproducible enough for release.

## Checks

- wheel contents inspected;
- license included;
- README included;
- `py.typed` included;
- no secrets/test certs accidentally included;
- no prohibited production dependencies;
- no unexpected dynamic libraries;
- `cargo deny check`;
- `cargo audit`;
- Python package metadata valid;
- TestPyPI release status recorded.

## Acceptance criteria

- `docs/PYPI_RELEASE.md` and release-candidate doc include artifact verification results.

---

# Workstream 7: Performance sanity check

## Goal

Demonstrate that Rust offload is not obviously regressed and is suitable as a pproxy alternative.

## Required comparisons

Local-only benchmarks or smoke measurements:

- Eggress SOCKS5 direct TCP echo throughput/latency;
- pproxy SOCKS5 direct TCP echo throughput/latency if available;
- Eggress HTTP CONNECT direct;
- pproxy HTTP CONNECT direct if available;
- UDP direct throughput if comparable;
- Python package embedded service overhead for start/proxy/shutdown.

## Rules

- do not overclaim benchmark significance;
- document hardware and command;
- use results for regression sanity, not marketing unless robust.

## Acceptance criteria

- `docs/PERFORMANCE.md` or release-candidate doc includes sanity results or explains why deferred.

---

# Workstream 8: Documentation consistency audit

## Goal

Make all docs agree.

## Docs to inspect

```text
README.md
docs/ROADMAP.md
docs/PARITY_MATRIX.md
docs/PPROXY_PARITY_SPEC.md
docs/PPROXY_MIGRATION.md
docs/PYTHON_BINDINGS.md
docs/PYPI_RELEASE.md
docs/DIFFERENTIAL_TESTING.md
docs/SECURITY_REVIEW.md
docs/RELEASE_READINESS.md
docs/protocols/SHADOWSOCKS.md
docs/protocols/SHADOWSOCKS_PARITY.md
docs/protocols/SHADOWSOCKS_TCP_AUDIT.md
python/README.md
AGENTS.md
```

## Required consistency points

- Shadowsocks TCP is experimental/non-standard unless fixed;
- Shadowsocks UDP interop status is precise;
- pproxy compatibility is scoped, not absolute;
- Python package claims match implemented API;
- hosted CI status is honest;
- TestPyPI/PyPI status is honest;
- non-parity is visible.

## Acceptance criteria

- No stale claims from earlier phases remain.

---

# Workstream 9: Release candidate document

## Required doc

Create:

```text
docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md
```

## Required sections

1. Release candidate summary.
2. Version/commit under audit.
3. Supported Rust features.
4. Supported Python features.
5. pproxy-compatible features.
6. Supported-but-not-pproxy-compatible features.
7. Experimental features.
8. Intentional non-parity.
9. Unsupported features.
10. Test evidence table.
11. Differential/interop evidence table.
12. Python wheel/install evidence.
13. Security audit summary.
14. Dependency/artifact audit summary.
15. Performance sanity summary.
16. Hosted CI/local verification status.
17. Release blockers.
18. Go/no-go recommendation.

## Acceptance criteria

- Release candidate claim is precise and defensible.

---

# Workstream 10: Final fixes only

## Goal

Allow small targeted fixes discovered during audit.

Allowed fixes:

- doc corrections;
- test assertion tightening;
- Python packaging metadata fixes;
- redaction fixes;
- broken example fixes;
- missing error mapping;
- workflow syntax fixes;
- release script fixes.

Not allowed without a new plan:

- Shadowsocks TCP standard rewrite;
- new protocols;
- major routing semantics changes;
- large runtime refactor.

---

# Recommended commit sequence

1. Parity matrix and evidence taxonomy audit.
2. Rust runtime release audit fixes.
3. Python package release audit fixes.
4. Security/redaction/package artifact fixes.
5. Performance sanity docs.
6. Documentation consistency pass.
7. Add release-candidate document.
8. Add Phase 17 completion record.

---

# Required verification

Rust:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Python dev:

```bash
maturin develop
python -m pytest python/tests
python -m mypy python/eggress
python -m ruff check python
```

Wheel:

```bash
maturin build --release
python -m venv .venv-wheel-test
. .venv-wheel-test/bin/activate
pip install dist/eggress-*.whl
python -m pytest python/tests
```

Gated/differential if available:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored
EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 python -m pytest python/tests/test_pproxy_differential.py
```

---

# Definition of done

Phase 17 is complete only when:

1. Final parity matrix is evidence-backed.
2. Rust runtime release claims are covered by tests.
3. Python package release claims are covered by tests.
4. Wheel install path is verified or blockers are explicit.
5. Security review includes Python surface.
6. Dependency/artifact audit is complete.
7. Performance sanity is recorded or explicitly deferred.
8. Documentation is internally consistent.
9. Release candidate doc exists with go/no-go recommendation.
10. All verification commands that can run locally pass.
11. Gated tests are either run or marked unverified.
12. Remaining blockers are clear.

## Completion record

Add:

```text
docs/PHASE_17_TRUE_PPROXY_PARITY_RELEASE_CANDIDATE_COMPLETION.md
```

Include final commit list, go/no-go status, release blockers, verified commands, unrun gated checks, and recommended next step after the release candidate audit.
