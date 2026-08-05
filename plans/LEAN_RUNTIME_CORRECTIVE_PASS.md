# Lean Runtime and Delivery — Corrective Pass

## Status

**PLANNED**

## Parent roadmap

[`LEAN_RUNTIME_AND_DELIVERY_ROADMAP.md`](LEAN_RUNTIME_AND_DELIVERY_ROADMAP.md)

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Reviewed head: `d54b74446a7f238720583511f9fc22d150b6ca10`
- Prior phases remain historically implemented, but the parent roadmap is reopened until this pass closes.

## Objective

Correct the narrow defects found after implementation of the lean runtime and Python delivery roadmap:

1. make `--no-default-features --features common` actually suppress transitive `full` defaults and prove the resulting dependency boundary;
2. eliminate unsafe or hanging HTTP behavior around `Expect: 100-continue`, informational responses, and early upstream responses using the smallest safe policy;
3. make the release-only Python workflow internally correct and operationally verified before the next production PyPI tag.

This pass is corrective, not additive. It must not introduce new protocols, a generalized feature framework, a full-duplex HTTP architecture rewrite, a custom release platform, routine CI matrices, or additional completion/evidence documents.

## Confirmed defects

### Cargo feature propagation

`eggress-cli` and `eggress-embed` expose bounded features, but their `eggress-runtime.workspace = true` dependency still receives the runtime crate's `default = ["full"]`. Disabling defaults on the top-level package therefore does not disable the dependency's defaults. The advertised `common` build can continue compiling runtime `extended`, `operations`, and `reverse` features.

Additionally, `eggress-udp` unconditionally depends on `eggress-protocol-shadowsocks` and compiles Shadowsocks flow types and branches. Even after runtime propagation is fixed, this may keep an advanced protocol family in a common build.

### HTTP request/response coordination

The forward-proxy path currently copies the complete request body before reading the upstream response. This can deadlock on `Expect: 100-continue` and can wait indefinitely when an upstream produces a final response without consuming the complete request body. The response path reads only one response head, so informational `1xx` handling is incomplete.

### Python release workflow

The release workflow currently has these correctness gaps:

- the QEMU step is conditioned only on `target == aarch64`, so it also runs on native macOS arm64;
- version extraction uses `sed` and reads the first root `version`, not the exact `[workspace.package].version` field;
- artifact collection does not hard-require exactly the approved five wheels and one sdist;
- absence of `abi3` is a warning instead of a release failure;
- wheel and sdist smoke jobs instantiate a compatibility server but do not start a port-0 listener, verify readiness/bound addresses, or shut it down;
- release jobs test `from eggress import pproxy`, but do not prove the separately installed top-level `import pproxy` namespace;
- no successful manual TestPyPI run has been recorded as a prerequisite to production use.

## Governing constraints

1. The default/full Rust build must retain all current supported behavior and both CLI binaries.
2. The public feature vocabulary remains `common`, `extended`, `operations`, `reverse`, `pproxy-compat`, and `full`. Do not add per-protocol public micro-features except one internal UDP Shadowsocks gate if required to make the existing groups truthful.
3. Admin and metrics remain required runtime dependencies. This pass does not reopen their optionalization.
4. The Python extension continues to request `eggress-embed/full` explicitly.
5. HTTP safety takes precedence over preserving connection reuse. Explicit rejection and connection close are preferable to a partial full-duplex implementation.
6. Routine `ci.yml` and `python-test.yml` remain single-job smoke workflows.
7. The platform matrix remains release-only and bounded to Linux x86_64/aarch64, macOS x86_64/arm64, Windows x86_64, and one sdist.
8. Crates.io remains manual.
9. Do not add GitHub Releases, native archives, containers maintained by the repository, signatures, checksums, SBOMs, provenance, or release branches.
10. Do not create another roadmap, phase hierarchy, completion report, evidence bundle, or verification framework. Update this plan and the parent roadmap in place at closure.

# Workstream 1 — Correct Cargo feature propagation

## Desired topology

The package-level defaults remain convenient for external users:

```toml
[features]
default = ["full"]
```

Internal workspace dependency edges that participate in feature forwarding must disable dependency defaults. The selecting package must then forward the intended feature explicitly.

Representative shape:

```toml
# Root [workspace.dependencies]
eggress-runtime = {
    path = "crates/eggress-runtime",
    default-features = false,
}
eggress-embed = {
    path = "crates/eggress-embed",
    default-features = false,
}

# crates/eggress-cli/Cargo.toml
common = ["eggress-runtime/common"]
extended = ["eggress-runtime/extended"]
operations = [
    "dep:eggress-system-proxy",
    "eggress-runtime/operations",
]
reverse = ["eggress-runtime/reverse"]

# crates/eggress-python/Cargo.toml
eggress-embed = { workspace = true, features = ["full"] }
```

Exact placement may vary, but the dependency graph must have these semantics.

## Files to inspect

```text
Cargo.toml
crates/eggress-cli/Cargo.toml
crates/eggress-runtime/Cargo.toml
crates/eggress-server/Cargo.toml
crates/eggress-metrics/Cargo.toml
crates/eggress-udp/Cargo.toml
crates/eggress-udp/src/flow.rs
crates/eggress-udp/src/relay.rs
crates/eggress-udp/src/standalone.rs
crates/eggress-udp/src/standalone_shadowsocks.rs
crates/eggress-embed/Cargo.toml
crates/eggress-python/Cargo.toml
```

## Step 1.1 — Fix dependency-default propagation

At minimum:

- set `default-features = false` on the root workspace dependencies for `eggress-runtime` and `eggress-embed` where internal forwarding must control selection;
- verify `eggress-server` and `eggress-metrics` remain default-free on internal edges;
- make `eggress-cli/common` explicitly enable `eggress-runtime/common`;
- ensure every `full` feature is the union of the intended subordinate groups;
- keep `eggress-python` explicitly enabling `eggress-embed/full`;
- verify the default CLI build still enables `eggress-runtime/full` through `eggress-cli/full` and still builds `pproxy`.

Do not remove package defaults from the crates themselves merely to make internal workspace builds pass. External `cargo add`/path users should retain the full default unless they opt out.

## Step 1.2 — Audit the actual common dependency tree

Run:

```bash
cargo tree -p eggress-cli --no-default-features --features common -e features \
  > /tmp/eggress-common-tree.txt

cargo tree -p eggress-cli --features full -e features \
  > /tmp/eggress-full-tree.txt
```

The common tree must not include these crates unless a documented unavoidable core dependency remains:

```text
eggress-protocol-shadowsocks
eggress-protocol-trojan
eggress-protocol-websocket
eggress-protocol-reverse
eggress-system-proxy
eggress-pproxy-compat
```

`eggress-admin` and `eggress-metrics` are allowed because their optionalization was explicitly rejected.

## Step 1.3 — Gate Shadowsocks UDP only if necessary

If `eggress-protocol-shadowsocks` remains in the common tree solely through `eggress-udp`, add one internal feature to `eggress-udp`, preferably named `shadowsocks` or `extended`, and make the dependency optional.

Concentrate `cfg` boundaries around:

- `UdpFlowKey::ShadowsocksUpstream`;
- `UdpFlowKind::ShadowsocksUpstream`;
- `ShadowsocksUdpTargetFlow`;
- Shadowsocks encode/decode imports;
- route capability branches that instantiate Shadowsocks UDP flows;
- `standalone_shadowsocks` module exposure.

Do not redesign `UdpFlowKind`, create a generic UDP plugin system, or split the UDP crate. A disabled Shadowsocks chain must return the existing unsupported-capability path rather than silently falling back to direct UDP.

If gating this dependency requires pervasive changes beyond the listed composition/type boundaries, stop and document the retained dependency. In that case, remove any misleading claim that common excludes the Shadowsocks dependency and reassess whether the lean feature complexity still satisfies the roadmap retention threshold.

## Step 1.4 — Strengthen feature-boundary tests

Existing tests that only verify missing CLI subcommands are insufficient. Add narrowly scoped checks proving:

1. default/full builds include both `eggress` and `pproxy`;
2. common-only builds produce `eggress` but not `pproxy`;
3. a common-only runtime/config rejects Shadowsocks, Trojan, WebSocket, reverse, and system-proxy capabilities explicitly;
4. the Python crate still compiles with the full feature set;
5. the common dependency tree does not contain excluded crate families.

Do not add a permanent Cargo-tree CI workflow. The dependency-tree assertions may be a small local script used during the corrective pass, or explicit implementation evidence recorded in this plan at closure. Runtime/CLI negative tests remain the executable regression guard.

## Step 1.5 — Repeat measurements

Use isolated targets:

```bash
CARGO_TARGET_DIR=target/corrective-full \
  cargo build -p eggress-cli --release --features full

CARGO_TARGET_DIR=target/corrective-common \
  cargo build -p eggress-cli --release \
  --no-default-features --features common

ls -lh \
  target/corrective-full/release/eggress \
  target/corrective-full/release/pproxy \
  target/corrective-common/release/eggress
```

Record exact byte sizes and crate counts in this plan at closure. Replace the earlier 9.3M/8.8M measurements in the parent roadmap; do not preserve measurements obtained from a feature graph that still enabled runtime defaults.

## Workstream 1 acceptance criteria

- `cargo tree` proves that top-level `--no-default-features` no longer leaves `eggress-runtime/full` enabled;
- common explicitly forwards `eggress-runtime/common`;
- default/full behavior and the `pproxy` binary remain unchanged;
- excluded runtime capabilities fail explicitly;
- the Python extension compiles with full features;
- new measurements are based on a truthful feature graph;
- feature conditionals remain concentrated and the public feature vocabulary does not grow.

# Workstream 2 — Make HTTP early-response behavior safe

## Policy decision

Use the smallest safe policy. This pass does **not** implement a general bidirectional request/response pump.

Required policy:

1. reject unsupported `Expect` semantics, including `Expect: 100-continue`, before forwarding a request body;
2. return an explicit `417 Expectation Failed` or the project's equivalent structured HTTP rejection and close the client connection;
3. correctly consume and forward zero or more informational responses that arrive outside the rejected Expect path, then process one final response;
4. reject `101 Switching Protocols` explicitly because ordinary forward-proxy upgrade tunneling is not implemented;
5. bound request-body forwarding so an upstream that responds early or stops reading cannot hold a session indefinitely;
6. on body-forward timeout/write failure, close both sides and return a protocol/relay failure without reusing the client connection.

It is acceptable that an early final response produced during body upload is not forwarded to the client in this narrow implementation, provided the session terminates within a deterministic bound and no unread body bytes are reused as another request. Document this limitation accurately.

A full implementation that concurrently uploads the body and reads upstream responses is out of scope unless the above safe policy cannot be expressed without introducing another correctness defect.

## Files to inspect

```text
crates/eggress-protocol-http/src/forward/server.rs
crates/eggress-protocol-http/src/error.rs
crates/eggress-server/src/execute.rs
crates/eggress-server/src/reply.rs
crates/eggress-protocol-http/src/forward/tests.rs
crates/eggress-server/tests/
docs/protocols/http.md
README.md
```

## Step 2.1 — Detect and reject `Expect`

During request-head parsing:

- collect `Expect` header values case-insensitively;
- reject any non-empty expectation because the current proxy does not implement expectation negotiation;
- do not forward the request head or body to the origin after rejection;
- send `417 Expectation Failed` with `Connection: close`;
- close the session and mark the outcome as a client protocol/unsupported expectation failure;
- do not silently remove `Expect` and continue.

Add raw-TCP regression coverage proving a client that sends headers with `Expect: 100-continue` receives a bounded 417 response without sending a body and without hanging.

## Step 2.2 — Loop over informational responses

Refactor response forwarding narrowly so it:

- reads a response head;
- when status is `100..=199` other than `101`, forwards the informational head with no response body and reads the next response head;
- limits the number of informational responses to a small constant such as 8 to prevent an infinite informational stream;
- processes body framing only for the final response;
- returns the final status in `ForwardResult`;
- treats `101` as explicitly unsupported, closes the session, and never enters an accidental raw relay.

Do not add HTTP/2, WebSocket upgrade support, or a general response state machine framework.

## Step 2.3 — Bound request-body upload

Wrap request-body forwarding and the final upstream flush in a deterministic timeout derived from an existing runtime timeout where possible. Prefer reusing an existing configured handshake/request/connect deadline over adding a broad new configuration surface.

Required behavior:

- timeout cancels body forwarding;
- the upstream stream is dropped;
- the client connection is closed and never reused;
- the session reports a bounded failure;
- no task is detached;
- no buffered client bytes can become the next request.

Do not drain an arbitrarily large rejected body merely to preserve keep-alive.

## Step 2.4 — Add focused tests

At minimum:

1. `Expect: 100-continue` receives 417 without a client body and completes within a short deadline;
2. multiple informational responses followed by a final response are forwarded in order;
3. `101 Switching Protocols` is rejected/closed explicitly;
4. more than the informational-response limit fails safely;
5. an upstream that does not consume the body cannot hang beyond the body-forwarding timeout;
6. after any expectation/body-forward failure, pipelined bytes are not parsed as a second request;
7. ordinary bodyless, content-length, and chunked forwarding continue to pass.

Use loopback and `tokio::io::duplex`; do not create an external HTTP conformance harness.

## Workstream 2 acceptance criteria

- no `Expect: 100-continue` deadlock exists;
- informational responses cannot consume or hide the final response;
- `101` cannot accidentally promote to a tunnel;
- body upload has a deterministic upper bound;
- failed body upload closes rather than reuses the client connection;
- existing ordinary HTTP forwarding remains functional;
- no generalized full-duplex or upgrade architecture is introduced.

# Workstream 3 — Correct the Python release-only workflow

## Files to inspect

```text
.github/workflows/publish-python.yml
crates/eggress-python/Cargo.toml
crates/eggress-python/pyproject.toml
python/tests/test_wheel_import_smoke.py
docs/release/RELEASE_PROCESS.md
README.md
python/README.md
```

## Step 3.1 — Restrict QEMU correctly

Change the QEMU setup condition so it runs only for Linux aarch64 cross-builds, for example:

```yaml
if: runner.os == 'Linux' && matrix.target == 'aarch64'
```

Do not install Docker/QEMU tooling on native macOS arm64.

## Step 3.2 — Parse versions structurally

Replace `sed`/`head` extraction with a short Python 3.12 `tomllib` check that reads exact fields:

- root `Cargo.toml`: `workspace.package.version`;
- `crates/eggress-python/Cargo.toml`: `package.version`;
- `crates/eggress-python/pyproject.toml`: `project.version`;
- normalized `v*` tag when tag-triggered.

Reject malformed/non-semver three-component production tags rather than normalizing arbitrary text through `awk`.

Keep this as an inline script or one very small checked-in helper. Do not build a version-management framework or mutate versions automatically.

## Step 3.3 — Make artifact validation a hard gate

The collector must require exactly:

- five wheels;
- one sdist;
- one Linux x86_64 wheel;
- one Linux aarch64 wheel;
- one macOS x86_64 wheel;
- one macOS arm64 wheel;
- one Windows x86_64 wheel;
- every wheel tagged `cp39-abi3` or an equivalent valid Python 3.9 stable-ABI tag;
- no duplicate platform target;
- no debug or unrelated artifact.

Missing `abi3` is an error, not a warning.

Use wheel filename parsing sufficient for this fixed artifact contract. Do not introduce a package-index or artifact-manifest subsystem.

Choose and document an explicit manylinux floor instead of relying on `auto`, unless the maintained Maturin action documents a deterministic result appropriate to the supported Ubuntu baseline. Prefer broad compatibility over the newest possible glibc.

## Step 3.4 — Use one real artifact smoke

Create or reuse one short smoke program that is executed against each installed native wheel and the sdist installation. It must:

```python
import eggress
import pproxy
from eggress import EggressService
```

Then it must:

1. confirm `eggress` and top-level `pproxy` resolve from the installed environment, not the repository source tree;
2. create an `EggressService` from a minimal TOML listener on `127.0.0.1:0`;
3. start the service;
4. verify readiness and a non-empty bound address;
5. exit the context/shut down cleanly;
6. verify the handle is no longer ready where the API exposes that state.

Prefer one small checked-in script or focused pytest target reused by wheel, compatibility-range, and sdist jobs. Do not duplicate multiline Python in every YAML job.

For Linux aarch64, native execution under maintained action/QEMU support is optional if it is unreliable. Build/tag validation remains mandatory, while the same source is operationally tested on Linux x86_64, macOS arm64, and Windows x86_64.

## Step 3.5 — Preserve bounded release structure

The corrected release workflow may retain:

- validation job;
- five-target wheel matrix;
- one sdist build;
- collection/validation;
- native wheel smoke jobs;
- Python 3.9 and 3.13 ABI smoke on Linux x86_64;
- one sdist clean-install smoke;
- one TestPyPI or PyPI publish job.

Do not add the matrix to routine `python-test.yml`. Do not add more supported targets.

If practical, reduce repetition in the current workflow while making these corrections, but do not create reusable-workflow indirection for a single repository.

## Step 3.6 — Require TestPyPI proof before production

Before this pass is marked implemented:

1. manually dispatch `publish-python.yml` with `publish_target=testpypi` using a unique non-production version or other repository-approved TestPyPI-safe versioning approach;
2. confirm every build, collection, wheel smoke, ABI-range smoke, sdist smoke, and TestPyPI publish job succeeds;
3. install one artifact from TestPyPI in a clean environment and run the same smoke program;
4. record the workflow run URL, artifact filenames, and outcome in this plan's closure section;
5. do not create or push a production `v*` tag solely to test the workflow.

Production PyPI publication remains blocked until this proof exists.

## Workstream 3 acceptance criteria

- QEMU runs only on Linux aarch64;
- exact TOML fields and tag version are compared;
- the collector fails unless the exact approved artifact set and stable ABI tags exist;
- release smoke imports top-level `pproxy` and actually starts/shuts down a port-0 service;
- the sdist undergoes the same operational smoke;
- the routine Python smoke workflow remains one Ubuntu/Python 3.12 job;
- a complete TestPyPI dispatch succeeds before production use;
- crates.io remains manual;
- no new release surface is added.

# Required verification

## Rust feature and HTTP verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked

cargo check -p eggress-cli
cargo check -p eggress-cli --no-default-features --features common
cargo check -p eggress-embed --no-default-features --features common
cargo check -p eggress-python

cargo tree -p eggress-cli --no-default-features --features common -e features
cargo tree -p eggress-cli --features full -e features
```

Run the focused HTTP test package independently while iterating:

```bash
cargo test -p eggress-protocol-http
cargo test -p eggress-server
```

## Python local verification

```bash
python3 -m venv .venv-corrective
.venv-corrective/bin/python -m pip install --upgrade pip
.venv-corrective/bin/python -m pip install \
  "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" \
  "cryptography>=42,<47"

(cd crates/eggress-python && \
  ../../.venv-corrective/bin/maturin build --release \
  --out ../../target/corrective-wheels)

.venv-corrective/bin/python -m pip install --force-reinstall \
  target/corrective-wheels/eggress-*.whl

EGRESS_EXPECT_INSTALLED_WHEEL=1 \
  .venv-corrective/bin/python -m pytest \
  python/tests/test_wheel_import_smoke.py -q
```

The final release-only matrix must be verified by the TestPyPI workflow run described above.

# Overall acceptance criteria

This corrective pass is complete only when all are true:

1. common-only Cargo builds no longer activate `eggress-runtime/full` through dependency defaults.
2. The common dependency tree truthfully excludes the approved advanced/compatibility crates, or any retained dependency is explicitly justified and the lean feature value is reassessed.
3. Full/default behavior, `pproxy`, Rust embedding, and Python full builds remain intact.
4. Full/common artifact measurements are repeated and replace the invalid earlier measurements.
5. `Expect: 100-continue` is explicitly rejected without deadlock.
6. Informational responses are bounded and followed through to one final response; `101` is rejected.
7. Request-body forwarding cannot hang indefinitely on an early/non-reading upstream.
8. Failed HTTP body/expectation exchanges close and cannot desynchronize a subsequent request.
9. The release workflow has correct platform conditions and structural version parsing.
10. The exact five-wheel/one-sdist stable-ABI artifact contract is a hard gate.
11. Wheel and sdist smoke tests perform real installed-package startup and shutdown and import top-level `pproxy`.
12. A complete manual TestPyPI run succeeds and is recorded.
13. Routine CI remains unchanged in shape.
14. No new protocol, generalized feature system, full-duplex HTTP redesign, release framework, routine matrix, or completion document is introduced.
15. This plan and the parent roadmap are updated in place to `IMPLEMENTED` only after all criteria pass.

# Stop conditions

Stop and narrow the implementation if:

- truthful feature propagation requires a workspace-wide API redesign;
- gating Shadowsocks out of UDP requires splitting the crate or duplicating flow architecture;
- safe HTTP behavior requires a generalized bidirectional relay rather than explicit rejection/time bounds;
- a platform wheel requires a custom maintained container or cross toolchain;
- workflow cleanup begins expanding into GitHub Releases, native archives, signing, or crates.io automation;
- a proposed test duplicates the full compatibility suite on every platform.

When a stop condition is reached, retain the smallest safe behavior, correct documentation and claims, and record the deferred limitation in this plan. Do not create another plan unless a concrete newly discovered defect is independently blocking release.

# Suggested commit sequence

Use a small number of commits:

1. `fix(build): propagate bounded runtime features correctly`
2. `test(build): prove common and full feature boundaries`
3. `fix(http): reject unsupported expectations and bound early responses`
4. `test(http): cover informational and body-upload failure paths`
5. `fix(ci): harden Python artifact build and smoke validation`
6. `docs: close lean runtime corrective pass`

Combining adjacent commits is acceptable. Do not commit per crate, per platform, or per test case.

# Closure record

At completion, update this file in place with:

- implementation commit range;
- final feature forwarding map;
- full/common exact byte sizes and dependency counts;
- excluded crates confirmed absent from common;
- HTTP policy and tests added;
- final artifact filenames and manylinux floor;
- successful TestPyPI workflow run URL;
- commands run;
- any stop condition invoked and the resulting documented limitation.

Then update the parent roadmap in place. Do not create a separate corrective-pass completion file.