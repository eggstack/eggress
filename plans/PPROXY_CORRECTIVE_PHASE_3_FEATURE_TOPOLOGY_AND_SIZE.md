# pproxy Corrective Phase 3 — Feature Topology and Binary Size

## Status

**PLANNED**

## Parent roadmap

[`PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`](PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md)

## Objective

Make the documented lean build correspond to a genuinely smaller linked capability set, then perform a measured low-risk size pass on the standalone CLI artifacts without changing the default/full supported feature surface.

This phase is not a workspace rewrite. It must retain a small number of understandable feature groups, avoid per-protocol micro-features, and remove proposed complexity when measurements do not justify it.

## Baseline facts

The previous lean-runtime phase established:

- `default = ["full"]` behavior;
- broad groups named `common`, `extended`, `operations`, `reverse`, and `pproxy-compat`;
- explicit Tokio features instead of Tokio `full`;
- thin LTO, one codegen unit, and symbol stripping for release;
- a size-oriented custom profile;
- a reported full `eggress` artifact around 9.3 MiB and lean artifact around 8.8 MiB at that implementation point.

The same phase deliberately retained admin and metrics in the common runtime because of runtime snapshot coupling. Current manifests still contain broad unconditional edges among runtime, server, admin, metrics, UDP, TLS, reverse, embed, and Python components.

A roughly 5% artifact reduction is useful but does not establish a fully truthful optional topology. The main goal is to remove meaningful optional dependency families from the lean graph while preserving the default full build.

## Governing rules

1. The default/full build must expose the same supported CLI, Rust API, Python API, protocols, admin endpoints, metrics, reverse mode, and pproxy compatibility binary as before.
2. Do not remove user-visible features from the default build to improve a size number.
3. Retain a bounded feature vocabulary. The target groups remain:
   - `common`
   - `extended`
   - `operations`
   - `reverse`
   - `pproxy-compat`
   - `full`
4. Do not add a public feature for every protocol parser, scheduler, metric, admin route, cipher, or transport wrapper.
5. Do not create a new crate solely to hold a few shared structs. Prefer an existing low-level crate when ownership is semantically appropriate.
6. Do not merge crates as a binary-size optimization. Crate count primarily affects build/maintenance topology, not final linked code.
7. Do not add unsafe socket or allocator code.
8. Do not change the Python wheel's default full capability set in this phase.
9. Do not make artifact size a CI gate.
10. Retain a change only when it removes a meaningful dependency/capability family, measurably reduces an artifact, or simplifies the feature graph.

## Required baseline capture

Before edits, record in the implementation commit or pull-request summary:

```bash
cargo tree -p eggress-cli -e features > /tmp/eggress-full-tree.txt
cargo tree -p eggress-cli --no-default-features --features common -e features > /tmp/eggress-common-tree.txt
cargo tree -p eggress-python -e features > /tmp/eggress-python-tree.txt

CARGO_TARGET_DIR=target/size-baseline/full \
  cargo build -p eggress-cli --release
CARGO_TARGET_DIR=target/size-baseline/common \
  cargo build -p eggress-cli --release --no-default-features --features common

ls -l target/size-baseline/full/release/eggress \
      target/size-baseline/full/release/pproxy \
      target/size-baseline/common/release/eggress
```

Also record:

```bash
cargo tree -p eggress-cli -d
cargo tree -p eggress-cli --no-default-features --features common -d
```

Use clean or isolated target directories. Do not compare stale artifacts from different feature sets.

`cargo bloat` may be used interactively when installed:

```bash
cargo bloat -p eggress-cli --release --bin eggress -n 40
cargo bloat -p eggress-cli --release --bin pproxy -n 40
```

It must not be added to repository dependencies or workflows.

## Workstream A — Define truthful group semantics

### `common`

`common` should contain the ordinary local proxy path:

- HTTP forward and CONNECT behavior currently considered common;
- SOCKS4/4a and SOCKS5;
- direct routing and the basic scheduler/rule path needed by those listeners;
- TLS where required by common supported URIs;
- UDP capabilities already part of common HTTP/SOCKS deployments;
- raw fixed-target behavior already classified as common;
- core configuration, runtime lifecycle, shutdown, and reload.

`common` should not automatically include:

- admin HTTP server;
- Prometheus/metrics export implementation;
- system-proxy integration;
- reverse/backward proxy;
- Shadowsocks;
- Trojan;
- WebSocket/WSS;
- the pproxy compatibility translator or `pproxy` binary.

### `extended`

`extended` should enable only the already-supported higher-cost protocol adapters, currently Shadowsocks, Trojan, and WebSocket/WSS, plus their narrowly required metrics/instrumentation hooks when operations are also enabled.

Do not add SSH, QUIC/H3, SSR, legacy cipher, or plugins.

### `operations`

`operations` should own:

- admin server;
- metrics exporter and operational endpoints;
- system-proxy integration;
- operational formatting or serialization dependencies not needed for the data plane.

The data plane may still emit lightweight internal counters or events without the operations feature, but it must not link the full admin/metrics server implementation merely to preserve a shared snapshot type.

### `reverse`

`reverse` owns the existing reverse/backward control-channel capability. It must not be pulled by `common`.

### `pproxy-compat`

`pproxy-compat` owns:

- the Rust compatibility parser and translator;
- the `pproxy` binary;
- compatibility-specific help/reporting;
- any compatibility-only baseline parser support.

Default/full installation must continue to include the `pproxy` binary. A no-default/common build may omit it through `required-features`.

### `full`

`full` remains the union of all currently supported groups and remains the default.

## Workstream B — Uncouple admin and metrics without alternate runtimes

### First inspect the coupling

Trace why `eggress-runtime` requires `eggress-admin` and `eggress-metrics`, and why `eggress-server`/`eggress-udp` refer to metrics types.

Identify which edges are:

- real operational implementation dependencies;
- shared DTO/type dependencies;
- initialization convenience;
- unconditional fields whose values are unused without operations;
- instrumentation calls that can compile to a no-op.

Do not begin by adding feature annotations to every file.

### Preferred bounded techniques

Use the least complex technique that removes the dependency family:

1. Move a genuinely shared lightweight data type to an existing lower-level crate such as `eggress-core` when that crate is its natural owner.
2. Feature-gate an operational field and its initialization when the field has no data-plane purpose.
3. Use a small internal no-op implementation for metrics events when operations are disabled.
4. Use `Option` for an operational handle only when absence is a normal supported state and does not create duplicate snapshot types.
5. Keep one compiled runtime snapshot structure. Do not fork the runtime into `OperationalSnapshot` and `LeanSnapshot` trees.

A small metrics sink trait is acceptable only if direct feature gating would spread across many crates. If used:

- define it in an existing low-level crate;
- keep methods limited to events already emitted;
- provide a zero-sized/no-op implementation;
- avoid async trait machinery, dynamic registration, plugin discovery, or generic observability backends.

### Stop conditions

Do not make admin/metrics optional if doing so requires any of:

- a second runtime/snapshot architecture;
- a new dependency-injection framework;
- pervasive generic parameters through protocol crates;
- duplicate server implementations;
- more than a small bounded set of `cfg` sites that are difficult to test;
- loss of operational behavior in the default/full build.

If a clean boundary is not available, document the coupling in the implementation summary and retain it. Continue with lower-risk artifact work rather than forcing abstraction.

## Workstream C — Correct manifest forwarding

Inspect and correct feature forwarding in:

- root `Cargo.toml`;
- `crates/eggress-cli/Cargo.toml`;
- `crates/eggress-runtime/Cargo.toml`;
- `crates/eggress-server/Cargo.toml`;
- `crates/eggress-admin/Cargo.toml`;
- `crates/eggress-metrics/Cargo.toml`;
- `crates/eggress-udp/Cargo.toml`;
- `crates/eggress-embed/Cargo.toml`;
- `crates/eggress-python/Cargo.toml`;
- protocol and transport crate manifests only where an unconditional edge is confirmed.

Requirements:

- internal workspace dependencies use `default-features = false` where the parent explicitly controls feature selection;
- each parent feature forwards only the child features it owns;
- optional dependencies use `dep:<name>` where appropriate;
- `required-features` prevents the `pproxy` binary from being built without compatibility support;
- `cargo check` succeeds for each supported group combination named below;
- no accidental default-feature re-enablement occurs through an internal dependency edge.

Do not attempt every possible feature combination. Support and test this bounded set:

```bash
cargo check -p eggress-cli
cargo check -p eggress-cli --no-default-features --features common
cargo check -p eggress-cli --no-default-features --features common,extended
cargo check -p eggress-cli --no-default-features --features common,operations
cargo check -p eggress-cli --no-default-features --features common,reverse
cargo check -p eggress-cli --no-default-features --features common,pproxy-compat
```

If `pproxy-compat` semantically requires another group, encode and document that dependency rather than allowing a broken combination.

## Workstream D — Preserve the Python distribution boundary

The published wheel should remain the full bounded product. Do not introduce multiple wheel flavors or optional binary distributions.

Requirements:

- `eggress-python` may continue to request the full embed/runtime feature set;
- feature rewiring must not accidentally omit protocols or operational functions from the release wheel;
- stable ABI and existing release artifact targets remain unchanged;
- Python tests run against the same default/full behavior as before;
- do not expose Cargo feature selection as a new Python user-facing API.

A local minimal Python extension build is out of scope unless it falls out naturally with no packaging/documentation burden. Do not add a second PyPI package.

## Workstream E — Measured full-binary reductions

After the feature graph is stable, inspect remaining full-binary contributors. Consider only low-risk items supported by measurements.

### Candidate 1 — Tracing subscriber features

Inspect `tracing-subscriber` features. If JSON formatting, registry layers, ANSI behavior, or other optional components are enabled but unused by CLI/runtime code, disable them explicitly.

Retain environment filtering and normal human-readable formatting currently required by the CLI.

### Candidate 2 — TLS logging/features

Inspect rustls and transport dependencies for optional logging or providers that are not used. Do not change cryptographic provider, trust behavior, protocol versions, or certificate validation merely for size.

Any TLS feature reduction must pass current TLS and protocol tests.

### Candidate 3 — Clap defaults

Inspect Clap default features. Disable only clearly unused help/color/suggestion components when doing so does not degrade current CLI help or diagnostics.

Do not replace Clap or write a custom parser for the native `eggress` CLI.

### Candidate 4 — Serialization formatting

Gate admin-only pretty-printing or schema/formatting support behind operations when it is currently linked into the data plane. Do not replace Serde/TOML across the project.

### Candidate 5 — Standalone size profile

Evaluate a custom standalone CLI profile such as:

```toml
[profile.release-cli-small]
inherits = "release-small"
panic = "abort"
```

Retain it only if:

- it is documented as an opt-in standalone binary profile;
- the default release and Python wheel profiles are unchanged;
- controlled runtime/config errors still return normally;
- panic behavior is not presented as graceful error handling;
- artifact reduction is measurable.

Do not set `panic = "abort"` globally for the default release or Python extension.

### Candidate 6 — `opt-level = "s"` versus `"z"`

Measure both for the standalone size profile. Keep whichever produces the smaller actual artifacts without material compile-time or runtime regression in the focused smoke tests. Do not assume `z` is always smaller.

### Rejected by default

Do not pursue without a separate user decision:

- custom allocators;
- UPX or post-link packers;
- stripping unwind information from default builds;
- replacing Tokio;
- replacing rustls;
- handwritten CLI parsing;
- whole-workspace crate merging;
- unsafe string/table compression;
- generated protocol code solely for size;
- runtime dynamic loading/plugins.

## Workstream F — Tests and behavior preservation

### Feature build tests

Add only enough compile/runtime tests to prove supported groups:

- default/full starts the same core listeners and operations;
- common starts a representative HTTP or SOCKS listener without admin/metrics/reverse/extended dependencies;
- common rejects excluded configuration with a clear feature-disabled error rather than silently ignoring it;
- `pproxy` binary is present in default/full and absent when required features are not selected;
- Python default/full behavior remains unchanged.

Do not add a routine feature-combination matrix workflow. Local commands and existing CI default build are sufficient.

### High-value regression preservation

Do not delete or weaken tests covering:

- HTTP framing and informational response bounds;
- UDP association lifecycle;
- reload and generation cleanup;
- shutdown/cancellation ordering;
- hostile/oversized parser input;
- TLS validation;
- compatibility translation failures.

Binary-size work must not reduce safety coverage.

## Measurement and retention policy

At the end, record:

- exact toolchain and target triple;
- full `eggress` size;
- full `pproxy` size;
- common `eggress` size;
- dependency counts or meaningful removed families;
- custom size-profile result if retained;
- changes rejected because benefit was negligible.

Retain a topology change when at least one is true:

- a meaningful optional implementation family is absent from the common `cargo tree`;
- common artifact size decreases materially from the audit baseline;
- default/full artifact size decreases measurably without behavior loss;
- manifest/feature wiring becomes simpler and more truthful even if byte reduction is small.

Revert a proposed abstraction when:

- artifact difference is noise;
- no substantial dependencies disappear;
- `cfg` complexity materially increases;
- default/full behavior becomes harder to reason about;
- Python packaging becomes more complex.

Do not set an arbitrary release-blocking byte threshold.

## Focused verification

During manifest iteration:

```bash
cargo check -p eggress-cli
cargo check -p eggress-cli --no-default-features --features common
cargo check -p eggress-cli --no-default-features --features common,extended
cargo check -p eggress-cli --no-default-features --features common,operations
cargo check -p eggress-cli --no-default-features --features common,reverse
cargo check -p eggress-cli --no-default-features --features common,pproxy-compat
```

Affected crates:

```bash
cargo test -p eggress-core
cargo test -p eggress-server
cargo test -p eggress-runtime
cargo test -p eggress-admin
cargo test -p eggress-metrics
cargo test -p eggress-cli
```

Python regression when embed/runtime manifests change:

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

Final gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Then rerun the isolated artifact measurements.

## Acceptance criteria

Phase 3 is complete only when all are true:

- default/full feature behavior and public APIs are unchanged;
- the bounded feature vocabulary remains understandable and no per-capability taxonomy is introduced;
- `common` omits extended, reverse, operations, and pproxy compatibility implementation families where the architecture permits a clean boundary;
- any retained admin/metrics coupling has an explicit measured stop-condition explanation rather than speculative abstraction;
- internal dependency default features do not accidentally re-enable omitted groups;
- the supported bounded feature combinations compile;
- excluded configuration fails clearly instead of being ignored;
- the default Python wheel remains full and passes its existing suites;
- full/common artifact sizes and dependency graphs are measured from isolated targets;
- each retained size optimization has a measurable or topology benefit;
- ineffective complexity is removed;
- no protocol feature, connection pool, new package, unsafe code, allocator, packer, crate merge, or CI size gate is added.

## Handoff notes for the implementer

- Start with `cargo tree`; do not start with `cfg` edits.
- Work one dependency family at a time and rerun the bounded feature checks after each change.
- Keep a temporary list of removed edges and artifact changes in the implementation summary, not a new repository evidence file.
- Prefer deleting one broad unconditional dependency over scattering many tiny feature flags.
- Verify default/full behavior after every manifest change because Cargo feature unification can hide mistakes.
- Update this plan in place with final measurements, retained/rejected changes, and implementation commits. Do not create a binary-size closure plan.