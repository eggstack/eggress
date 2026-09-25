# Lean Runtime and Delivery Roadmap

## Status

**CORRECTIVE PASS IMPLEMENTED; TESTPYPI GATE BLOCKED**

The original three phases were implemented through `d54b744`. The corrective
implementation is complete through `e9ba98e`: feature propagation, bounded UDP
Shadowsocks gating, HTTP early-response safety, exact release validation, and
installed artifact smokes are verified. Production release remains blocked
until the repository owner configures the TestPyPI trusted publisher and a
complete publish/install proof succeeds.

Production PyPI tagging is blocked while this roadmap remains open.

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Original audit baseline: `f809c7d8c83e19955e77e02879d2d2f160222efe`
- Original planning head: `b05d7db3657cb570ae5afbc886aa51f8e7dc00b8`
- Reviewed implementation head: `d54b74446a7f238720583511f9fc22d150b6ca10`
- Product contract: preserve the current Rust CLI, embeddable Rust API, Python package, and bounded `pproxy==2.7.9` compatibility surface.

## Purpose

Reduce avoidable binary and dependency weight, correct Python wheel delivery, and add focused reliability coverage without adding proxy features, redesigning the codebase, or rebuilding the repository's former verification ceremony.

This remains a reductive line of work. It is not a new parity phase.

## Governing constraints

1. The current default/full build retains the existing supported feature surface and both CLI binaries.
2. The public Cargo feature vocabulary remains bounded to `common`, `extended`, `operations`, `reverse`, `pproxy-compat`, and `full`.
3. Public Rust and Python APIs remain source-compatible unless an unsafe or inaccurate behavior must be corrected.
4. Default `cargo install --path crates/eggress-cli` continues to install both `eggress` and `pproxy`.
5. Crates.io publication remains manual.
6. PyPI wheel construction and publication may remain in GitHub Actions because platform wheels require multiple build hosts, but release automation stays limited to artifact construction, clean-install smoke verification, and registry publication.
7. Routine hosted CI remains one Rust smoke workflow and one path-scoped Python smoke workflow.
8. Existing tests and observability are reused. No test daemon, generic task registry, custom evidence framework, benchmark system, or generalized fault-injection system is introduced.
9. No new protocol, transport, scheduler, routing primitive, admin endpoint, metrics backend, daemonization mode, or system integration is in scope.
10. Net maintenance complexity must decrease or remain nearly flat.
11. HTTP safety may use explicit rejection and connection close rather than a full-duplex architecture rewrite.
12. No new completion report or evidence bundle is created; this roadmap and its implementation plans are updated in place.

## Historical implementation summary

### Phase 1 — Feature boundaries and binary size

Implemented principally in `9b1ed30` and `5f43042`.

Delivered:

- public feature groups `common`, `extended`, `operations`, `reverse`, `pproxy-compat`, and `full`;
- `pproxy` binary `required-features` handling;
- explicit Tokio features instead of `full`;
- normal and size-oriented release profiles;
- feature-boundary documentation and negative CLI tests.

Previously reported measurements:

| Artifact | Full | Reported lean | Reported reduction |
|---|---:|---:|---:|
| `eggress` | 9.3M | 8.8M | about 5.4% |
| dependency count | 492 | 478 | about 2.8% |

These measurements are now considered provisional because internal `eggress-runtime` dependency defaults were not disabled. They must be repeated after the corrective pass establishes a truthful common-only graph.

Admin and metrics remain required runtime dependencies because optionalizing them would violate the shared snapshot invariant and require disproportionate redesign.

### Phase 2 — Focused reliability

Implemented principally in `092ed77`, `83d1b18`, and `5c75b55`.

Delivered targeted tests and fixes around:

- HTTP request framing and hop-by-hop filtering;
- UDP association and target-flow cleanup;
- reload generation ownership;
- shutdown, malformed-client isolation, and resource bounds;
- IPv4-mapped IPv6 UDP response matching;
- response status exposure through `ForwardResult`.

Most of this phase remains valid. Post-review identified a remaining HTTP gap: `Expect: 100-continue`, informational responses, and early upstream final responses are not safely coordinated.

### Phase 3 — Python ABI and delivery

Implemented principally in `b650ab6`, `a87748b`, and `d54b744`.

Delivered:

- PyO3 `abi3-py39` configuration;
- removal of the inaccurate OS-independent classifier;
- a release-only wheel matrix targeting Linux x86_64/aarch64, macOS x86_64/arm64, and Windows x86_64;
- one sdist;
- version, artifact, wheel, ABI-range, and sdist workflow stages;
- preservation of one routine Ubuntu/Python smoke job;
- continued manual crates.io publication.

The matrix design remains appropriate, but the workflow is not yet approved for production use. Platform conditions, version parsing, hard artifact gates, and operational smoke behavior require correction and a successful TestPyPI proof run.

## Post-implementation findings

### 1. Cargo feature propagation is incomplete

Disabling default features on `eggress-cli` or `eggress-embed` does not automatically disable `eggress-runtime`'s `default = ["full"]`. Internal dependency edges must use `default-features = false`, and selecting crates must forward the intended runtime feature explicitly.

`eggress-udp` also retains an unconditional Shadowsocks dependency. The corrective pass must either gate that dependency narrowly or document why it cannot be removed without disproportionate complexity.

### 2. HTTP early-response behavior is unsafe

The forward proxy copies the complete request body before reading the upstream response. This can deadlock on `Expect: 100-continue` and can hang against an upstream that emits a final response without consuming the body. Informational response handling also does not reliably proceed to the final response.

The corrective policy is deliberately small: reject unsupported expectations, bound body upload, loop over a bounded number of informational responses, reject `101`, and close failed exchanges rather than implementing a general bidirectional HTTP pump.

### 3. Python release verification is incomplete

The release-only workflow must correct:

- QEMU activation on native macOS arm64;
- regex-based version parsing;
- non-fatal or incomplete artifact checks;
- smoke tests that instantiate but do not start and stop a proxy;
- lack of top-level `pproxy` import verification in release jobs;
- lack of a completed TestPyPI proof run.

## Registered execution plans

| Order | Plan | Status | Purpose |
|---|---|---|---|
| 1 | [`LEAN_RUNTIME_PHASE_1_FEATURE_BOUNDARIES.md`](LEAN_RUNTIME_PHASE_1_FEATURE_BOUNDARIES.md) | Implemented, measurements provisional | Original bounded feature and size pass. |
| 2 | [`LEAN_RUNTIME_PHASE_2_FOCUSED_RELIABILITY.md`](LEAN_RUNTIME_PHASE_2_FOCUSED_RELIABILITY.md) | Implemented with HTTP follow-up required | Original focused reliability pass. |
| 3 | [`LEAN_RUNTIME_PHASE_3_PYTHON_DELIVERY.md`](LEAN_RUNTIME_PHASE_3_PYTHON_DELIVERY.md) | Implemented but release proof incomplete | Original Python ABI and release matrix pass. |
| 4 | [`LEAN_RUNTIME_CORRECTIVE_PASS.md`](LEAN_RUNTIME_CORRECTIVE_PASS.md) | Implemented; TestPyPI gate blocked | Correct feature propagation, safe HTTP early-response policy, and release workflow validation. |

The corrective pass is the only additional implementation plan authorized for this line of work. Do not split it into per-crate, per-test, or per-platform plans.

## In scope for closure

- disabling internal dependency defaults where feature forwarding must control selection;
- explicitly forwarding `common` and other existing groups;
- narrowly gating the UDP Shadowsocks dependency if practical;
- rerunning full/common dependency-tree and artifact measurements;
- explicitly rejecting unsupported `Expect` semantics;
- bounded informational-response processing and `101` rejection;
- bounding request-body upload and closing failed exchanges;
- correcting release matrix platform conditions;
- structural TOML version parsing;
- hard enforcement of the exact five-wheel/one-sdist stable-ABI contract;
- installed-wheel and installed-sdist startup/shutdown smoke using top-level `pproxy`;
- one successful manual TestPyPI run before production use;
- concise updates to active documentation and these plan files.

## Out of scope

- new proxy protocols or transport roles;
- broader `pproxy` parity expansion;
- SOCKS BIND, SSH, QUIC/HTTP/3, SSR, TLS interception, daemonization, or connection pooling;
- crate merging or a workspace-wide architecture rewrite;
- a generic plugin, backend, dependency-injection, or feature registry;
- a full-duplex HTTP proxy rewrite or upgrade tunneling;
- per-protocol public micro-features beyond the existing bounded groups;
- automatic crates.io publication;
- GitHub Releases, native binary archives, maintained containers, signatures, checksums, SBOMs, or provenance systems;
- routine OS, architecture, or Python-version matrices;
- code coverage, benchmark, cargo-bloat, audit, fuzz, soak, or external-oracle gates;
- generated parity reports or completion/evidence documents.

## Corrective measurements

The earlier lean measurements are superseded only after these commands run against corrected dependency defaults:

```bash
cargo tree -p eggress-cli --features full -e features \
  > /tmp/eggress-full-tree-corrective.txt

cargo tree -p eggress-cli --no-default-features --features common -e features \
  > /tmp/eggress-common-tree-corrective.txt

CARGO_TARGET_DIR=target/corrective-full \
  cargo build -p eggress-cli --release --features full

CARGO_TARGET_DIR=target/corrective-common \
  cargo build -p eggress-cli --release \
  --no-default-features --features common
```

Record exact byte sizes and dependency counts in the corrective plan and replace the provisional figures in this roadmap.

The feature boundary may be retained only if it produces a truthful graph and at least one material benefit:

- at least 5% artifact reduction;
- removal of one or more substantial optional dependency families;
- material compile/platform simplification.

If it does not, revert non-beneficial `cfg` complexity while retaining explicit Tokio and release-profile improvements.

## Verification policy

During implementation, run the narrowest affected tests. Final local closure requires:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked

cargo check -p eggress-cli --no-default-features --features common
cargo check -p eggress-embed --no-default-features --features common
cargo check -p eggress-python
```

Python-facing changes require a clean wheel installation and the focused installed-artifact smoke. Production release approval additionally requires the complete manual TestPyPI workflow run specified in the corrective plan.

External interoperability, certification, benchmark, fuzz, soak, and audit commands remain opt-in unless a changed parser or compatibility claim directly requires them.

## Roadmap closure criteria

This roadmap may return to `IMPLEMENTED` only when all are true:

1. Internal Cargo dependency defaults no longer reactivate `eggress-runtime/full` in a common-only build.
2. Common and full feature graphs are proven and documented accurately.
3. Corrected artifact measurements replace the provisional Phase 1 figures.
4. Default/full behavior, both CLI binaries, embedding, and Python full builds remain intact.
5. `Expect: 100-continue` cannot deadlock the proxy.
6. Informational responses are bounded and followed through to a final response; `101` is explicitly unsupported.
7. Request-body upload cannot hang indefinitely against a non-reading or early-responding upstream.
8. Failed HTTP exchanges close and cannot desynchronize a later request.
9. The release matrix has correct platform-specific setup.
10. Exact versions are read structurally from the intended TOML fields.
11. Exactly five stable-ABI wheels and one sdist are required before publication.
12. Installed wheel and sdist smoke tests import top-level `pproxy`, start a port-0 service, verify readiness/bound addresses, and shut down cleanly.
13. A complete manual TestPyPI workflow run succeeds and is recorded in the corrective plan. This is the only remaining release gate; the current run reached publication but was rejected by missing external trusted-publisher configuration.
14. Routine CI remains one Rust smoke workflow and one path-scoped Python smoke workflow.
15. Crates.io remains manual.
16. No additional protocol, generalized framework, routine matrix, or completion document is introduced.

## Closure record

At completion:

- update [`LEAN_RUNTIME_CORRECTIVE_PASS.md`](LEAN_RUNTIME_CORRECTIVE_PASS.md) in place with commit range, measurements, tests, artifact names, and TestPyPI run URL;
- replace `CORRECTIVE PASS PLANNED` here with `IMPLEMENTED` only after the external TestPyPI gate succeeds;
- summarize the final corrected feature graph, HTTP policy, and release proof;
- record the deliberately retained reverse/admin dependency and why.

Do not create another roadmap or separate completion document.
