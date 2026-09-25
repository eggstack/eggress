# Lean Runtime Phase 1 — Feature Boundaries and Binary Size

## Status

**IMPLEMENTED**

## Implementation

Commits: Single combined commit implementing all phase 1 changes.

### Retained feature groups

| Group | Scope | Contents |
|-------|-------|----------|
| `common` | runtime, cli, embed | HTTP/SOCKS core, TLS transport, UDP, raw |
| `extended` | runtime, server, metrics, cli, embed | Shadowsocks, Trojan, WebSocket (server-level feature gates) |
| `operations` | runtime, cli | System proxy (admin and metrics kept as required for snapshot invariant) |
| `reverse` | runtime, cli | Reverse/backward proxy control-channel |
| `pproxy-compat` | cli, embed | Rust compatibility translator and binary |
| `full` | all | Union of all features; `default = ["full"]` preserved |

### Full/lean artifact sizes

| Artifact | Full | Lean | Reduction |
|----------|------|------|-----------|
| `eggress` binary | 9.3M | 8.8M | ~5.4% |
| Dependency count | 492 | 478 | ~2.8% |
| `pproxy` binary | 8.2M | not built | N/A |

### Feature gate locations

- **Server crate**: `extended` feature gates Shadowsocks/Trojan/WebSocket accept, chain executor handlers, and `shadowsocks_metrics` field type
- **Runtime crate**: `extended` gates shadowsocks metrics initialization and UDP relay; `reverse` gates reverse server/client spawning; `operations` gates system-proxy dep
- **CLI crate**: `pproxy-compat` gates pproxy binary and translate/check/run subcommands; `operations` gates system-proxy subcommand
- **Metrics crate**: `extended` gates shadowsocks metrics bridging

### Tokio features

Reduced from `features = ["full"]` to:
```
rt, rt-multi-thread, macros, net, io-util, sync, time, signal, fs
```

### Release profiles

```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"

[profile.release-small]
inherits = "release"
opt-level = "z"
lto = true
```

### Commands run

```bash
cargo fmt --all -- --check          # pass
cargo clippy --workspace --all-targets -- -D warnings  # pass
cargo test --workspace              # 2390 passed, 146 ignored
cargo check -p eggress-cli --no-default-features --features common  # pass
CARGO_TARGET_DIR=target/full cargo build -p eggress-cli --release  # 9.3M
CARGO_TARGET_DIR=target/lean cargo build -p eggress-cli --release --no-default-features --features common  # 8.8M
```

### Feature gate deliberately rejected

Admin and metrics remain as required runtime dependencies because they are tightly coupled to the runtime snapshot invariant. Making them optional would require duplicating runtime state or creating alternate snapshot types, which the plan's stop conditions prohibit.

## Parent roadmap

[`LEAN_RUNTIME_AND_DELIVERY_ROADMAP.md`](LEAN_RUNTIME_AND_DELIVERY_ROADMAP.md)

## Objective

Create one supported lean local build without changing the current default/full product. Reduce unnecessary linked dependencies by introducing a small number of Cargo feature groups, replacing Tokio's workspace `full` feature set with the explicit required set, and adding measured release profiles.

This plan is successful only if the resulting build topology is simpler to understand and the lean configuration produces a material dependency or artifact reduction. Source-level modularity alone is not sufficient.

## Non-goals

Do not use this phase to:

- add, remove, or redesign proxy features;
- change protocol behavior or compatibility tiers;
- merge crates;
- redesign the runtime around generic factories, trait-object registries, or plugins;
- create a feature for every protocol sub-capability;
- make the lean build the default;
- remove the `pproxy` binary from default installation;
- add binary-size CI gates or install new required tooling;
- optimize hot paths, allocator behavior, or memory layout unless a compile boundary exposes an actual defect;
- apply `panic = "abort"` globally to the Python extension or embeddable library.

## Baseline observations

At the roadmap baseline:

- root `Cargo.toml` defines `tokio = { version = "1", features = ["full"] }`;
- `eggress-runtime` unconditionally depends on admin, metrics, UDP, TLS, Shadowsocks, and reverse-proxy crates;
- `eggress-cli` unconditionally depends on metrics, compatibility, and system-proxy crates;
- `eggress-cli` emits both `eggress` and `pproxy` binaries without `required-features`;
- the root package contains benchmarks and Criterion but no explicit production release profile;
- the Python extension depends on the full embed/compatibility stack and must continue to build the full supported product unless separately proven safe.

## Required feature model

Keep the public feature model bounded to the following groups. Exact dependency lists may change after inventory, but new public groups require a documented reason.

### `common`

The ordinary local proxy surface:

- core listener, connector, relay, routing, URI, config, server, and runtime infrastructure;
- HTTP/1 forward proxy and CONNECT;
- SOCKS4/4a and SOCKS5;
- direct TCP and UDP;
- TLS transport required by common supported URIs;
- raw fixed-target support when it carries no substantial additional dependency family.

### `extended`

Already-supported protocol adapters that are not required for a basic HTTP/SOCKS local proxy:

- Shadowsocks;
- Trojan;
- WebSocket/WSS;
- H2 support if its dependency can be omitted cleanly from `common` without duplicating HTTP architecture.

Do not force H2 out of `common` if doing so requires invasive conditional types. The goal is meaningful simplification, not theoretical minimality.

### `operations`

Operational integrations that a basic local relay does not require:

- admin HTTP surface;
- Prometheus metrics;
- system-proxy modification.

If admin and metrics are tightly coupled to the runtime snapshot, preserve the shared snapshot invariant. Make construction optional; do not duplicate runtime state or create alternate snapshot types.

### `reverse`

The existing reverse/backward proxy capability and its control-channel implementation.

### `pproxy-compat`

The Rust compatibility translator and compatibility binary.

### `full`

The union of all current supported functionality. `default = ["full"]` must preserve current default installation and runtime behavior.

## Implementation strategy

### Step 1 — Freeze dependency and artifact baselines

Before modifying manifests, record:

```bash
rustc --version
cargo --version
cargo tree -p eggress-cli -e features > /tmp/eggress-full-tree-before.txt
cargo build -p eggress-cli --release
cp target/release/eggress /tmp/eggress-full-before
test -f target/release/pproxy && cp target/release/pproxy /tmp/pproxy-full-before
ls -l /tmp/eggress-full-before /tmp/pproxy-full-before
```

Also inspect reverse dependencies for the largest optional families:

```bash
cargo tree -p eggress-cli -i rustls
cargo tree -p eggress-cli -i prometheus-client
cargo tree -p eggress-cli -i hyper
cargo tree -p eggress-cli -i h2
cargo tree -p eggress-cli -i tokio-tungstenite
cargo tree -p eggress-cli -i eggress-protocol-shadowsocks
cargo tree -p eggress-cli -i eggress-protocol-reverse
cargo tree -p eggress-cli -i eggress-system-proxy
```

The baseline record belongs in the implementation commit or PR summary, not a new permanent report file.

### Step 2 — Inventory compile-time ownership

Inspect at minimum:

```text
Cargo.toml
crates/eggress-cli/Cargo.toml
crates/eggress-cli/src/main.rs
crates/eggress-cli/src/pproxy_main.rs
crates/eggress-runtime/Cargo.toml
crates/eggress-runtime/src/lib.rs
crates/eggress-runtime/src/snapshot.rs
crates/eggress-runtime/src/supervisor.rs
crates/eggress-server/Cargo.toml
crates/eggress-embed/Cargo.toml
crates/eggress-embed/src/lib.rs
crates/eggress-python/Cargo.toml
crates/eggress-admin/Cargo.toml
crates/eggress-metrics/Cargo.toml
crates/eggress-system-proxy/Cargo.toml
crates/eggress-protocol-reverse/Cargo.toml
crates/eggress-protocol-shadowsocks/Cargo.toml
crates/eggress-protocol-trojan/Cargo.toml
crates/eggress-protocol-websocket/Cargo.toml
crates/eggress-transport-tls/Cargo.toml
```

For every candidate optional crate, answer:

1. Is it referenced by a public type in an always-compiled API?
2. Is it instantiated only from runtime/CLI composition code?
3. Does excluding it require a small `cfg` branch or a second parallel architecture?
4. Does it pull in a substantial unique dependency family?
5. Is it required by the Python compatibility surface?

Only dependencies satisfying questions 2 and 4, without violating question 3, should be optionalized in this phase.

### Step 3 — Add feature groups at composition boundaries

Prefer feature definitions in the crates that own composition:

- `eggress-runtime` controls protocol and operational runtime components;
- `eggress-cli` controls binary-specific integrations and the compatibility binary;
- `eggress-embed` forwards the supported runtime feature groups;
- `eggress-python` explicitly requests the full feature set needed by the wheel.

Use optional dependencies with `dep:` feature references. Avoid exposing internal crate names as the documented user interface when a group feature is sufficient.

A representative shape is:

```toml
[features]
default = ["full"]
full = ["common", "extended", "operations", "reverse", "pproxy-compat"]
common = [
    "dep:eggress-protocol-http",
    "dep:eggress-protocol-socks",
    "dep:eggress-udp",
    "dep:eggress-transport-tls",
]
extended = [
    "dep:eggress-protocol-shadowsocks",
    "dep:eggress-protocol-trojan",
    "dep:eggress-protocol-websocket",
]
operations = [
    "dep:eggress-admin",
    "dep:eggress-metrics",
    "dep:eggress-system-proxy",
]
reverse = ["dep:eggress-protocol-reverse"]
pproxy-compat = ["dep:eggress-pproxy-compat"]
```

This example is not a mandate to duplicate features in every crate. Forward only the groups consumed by that crate.

### Step 4 — Gate construction, not core types

Use narrow `#[cfg(feature = "...")]` boundaries around:

- protocol adapter registration;
- runtime factory match arms;
- admin/metrics server startup;
- system-proxy command handling;
- reverse control-channel startup;
- compatibility-only CLI commands and binary target.

Do not scatter conditional compilation through protocol algorithms or core relay types. A disabled feature should fail during configuration/URI validation with an existing structured unsupported-capability diagnostic, not by compiling a stub protocol that fails later.

When a configuration names a capability absent from the build:

- return a deterministic error naming the disabled feature group;
- never silently fall back to direct routing or another protocol;
- preserve redaction of secret-bearing URIs;
- do not change the error semantics of full/default builds.

### Step 5 — Preserve default binary installation

In `crates/eggress-cli/Cargo.toml`:

- keep `default = ["full"]`;
- make the `pproxy` binary require `pproxy-compat`;
- verify that default installation still builds both binaries;
- verify that the documented lean build may produce only `eggress` unless `pproxy-compat` is explicitly selected.

Representative manifest shape:

```toml
[[bin]]
name = "pproxy"
path = "src/pproxy_main.rs"
required-features = ["pproxy-compat"]
```

Document this distinction. It is not a feature removal because default behavior remains unchanged.

### Step 6 — Reduce Tokio features

Replace workspace `tokio` `full` with the explicit union actually required by production and tests.

Start with likely candidates and remove any not used:

```text
rt
rt-multi-thread
macros
net
io-util
sync
time
signal
fs
process
```

Use compiler errors and feature-tree inspection rather than guessing. Test-only requirements may be enabled in dev-dependencies for the affected crate rather than globally if practical.

Do not spend time eliminating a Tokio feature that is required broadly and adds negligible unique code. The primary requirement is to remove the opaque `full` umbrella and make runtime requirements reviewable.

### Step 7 — Add release profiles

Add a normal production profile and a separate optional size profile at the workspace root.

Recommended starting point:

```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"

[profile.release-small]
inherits = "release"
opt-level = "z"
lto = true
```

Do not set workspace-wide `panic = "abort"` in this phase. The CLI may benefit, but the same workspace also produces an embeddable Rust library and Python extension. A package-specific panic decision requires evidence that it does not alter those surfaces and is not necessary to obtain the principal size reduction.

Do not add allocator replacements, `build-std`, nightly flags, UPX, or post-link binary compression.

### Step 8 — Document supported build commands

Update only active documentation, likely:

```text
README.md
docs/EMBED_API.md
docs/architecture/overview.md
docs/architecture/runtime.md
docs/architecture/cli.md
```

Add concise commands for:

```bash
# Current full/default installation
cargo install --path crates/eggress-cli

# Lean local HTTP/SOCKS build
cargo build -p eggress-cli --release --no-default-features --features common

# Optional smallest optimization profile
cargo build -p eggress-cli --profile release-small --no-default-features --features common
```

Do not add a new build manual, feature matrix, generated table, or binary-size dashboard.

## Required tests

### Manifest/build topology

```bash
cargo check -p eggress-cli
cargo check -p eggress-cli --no-default-features --features common
cargo check -p eggress-cli --no-default-features --features full
cargo check -p eggress-embed --no-default-features --features common
cargo check -p eggress-python
```

If feature forwarding uses different exact syntax, adjust commands while preserving the tested configurations.

### Default behavior

```bash
cargo build -p eggress-cli --release
./target/release/eggress --help
./target/release/pproxy --help
```

Run existing CLI tests that verify no-argument `pproxy` behavior, URI translation, and structured diagnostics.

### Lean behavior

Build into a separate target directory:

```bash
CARGO_TARGET_DIR=target/lean \
  cargo build -p eggress-cli --release --no-default-features --features common

target/lean/release/eggress --help
```

Run existing local HTTP CONNECT and SOCKS5 smoke tests against the lean build or add one narrow CLI integration test that is feature-gated and reuses current testkit fixtures.

Add negative tests proving that a configuration naming an excluded capability fails clearly. At minimum:

- one extended protocol URI under `common`;
- admin or metrics configuration under `common`, if those fields remain parseable;
- reverse mode under `common`;
- compatibility binary not selected when `pproxy-compat` is absent.

### Artifact and dependency comparison

Use clean or isolated target directories:

```bash
CARGO_TARGET_DIR=target/full cargo build -p eggress-cli --release
CARGO_TARGET_DIR=target/lean cargo build -p eggress-cli --release \
  --no-default-features --features common

ls -lh target/full/release/eggress target/lean/release/eggress
cargo tree -p eggress-cli -e features > /tmp/eggress-full-tree-after.txt
cargo tree -p eggress-cli --no-default-features --features common -e features \
  > /tmp/eggress-lean-tree-after.txt
```

Confirm that excluded groups are absent from the lean dependency tree. Do not rely only on file size.

### Phase gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Acceptance criteria

Phase 1 is complete only when:

1. `default`/`full` preserves the existing supported runtime and both CLI binaries.
2. `common` builds a functional HTTP/SOCKS local proxy without extended, reverse, admin, metrics, system-proxy, or compatibility-only dependency families unless a documented unavoidable shared dependency remains.
3. Disabled capabilities fail explicitly and never silently degrade.
4. Feature conditionals remain concentrated at manifests and composition/registration boundaries.
5. No public API is redesigned merely to make a dependency optional.
6. Tokio no longer uses `features = ["full"]` in the workspace dependency.
7. The normal release profile uses symbol stripping and LTO suitable for a network proxy.
8. The optional size profile exists without becoming a CI gate.
9. Full and lean artifact sizes and dependency trees are recorded in the implementation summary.
10. The lean boundary satisfies at least one retention condition from the parent roadmap; otherwise non-beneficial gating is reverted.
11. Python extension compilation still requests the complete supported runtime and compatibility surface.
12. Existing workspace tests pass.
13. No new CI workflow, permanent size script, generated report, or completion document is added.

## Stop conditions and rollback rules

Stop and narrow the implementation when any of the following occurs:

- optionalization requires parallel runtime snapshot types;
- a public API must become generic or feature-dependent across common use;
- more than a small number of factory/registration modules require pervasive `cfg` branches;
- the lean build retains nearly all substantial dependency families and is less than 5% smaller;
- Python or embedding behavior requires divergent runtime implementations;
- a disabled feature can only be represented through silent fallback.

In these cases, keep the low-risk Tokio and release-profile improvements, preserve the default build, and document why the rejected gate was not worth its complexity in the roadmap closure record.

## Handoff sequence

Use a small number of commits:

1. `build: define bounded runtime feature groups`
2. `build: reduce tokio features and add release profiles`
3. `test: cover full and lean build boundaries`
4. `docs: document full and lean builds`

Combining adjacent commits is acceptable. Do not create one commit per crate or per feature flag.

## Closure update

When implemented, update this file in place with:

- implementation commit range;
- exact retained feature groups;
- full/lean artifact sizes;
- major dependency families removed from `common`;
- any feature gate deliberately rejected;
- commands run.

Do not create a separate Phase 1 completion report.