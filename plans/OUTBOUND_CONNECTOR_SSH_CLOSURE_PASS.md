# OutboundConnector SSH Closure Pass

## Status

**READY FOR IMPLEMENTATION**

## Parent work

- `plans/OUTBOUND_CONNECTOR_SSH_FEATURE_BOUNDARY_CORRECTIVE_PASS.md`
- implementation commit `4f6ec746adf4a76df7ed6e534053bd8e387da999` (`fix(embed): initialize SSH outbound connector state`)
- closure-evidence commit `476577919d6c52b74d1f8324df5e838bb5a2ab14` (`docs(plan): record outbound SSH closure evidence`)

## Baseline

Written against `main` at:

```text
476577919d6c52b74d1f8324df5e838bb5a2ab14
```

The substantive SSH facade correction is already implemented and locally qualified. This pass is intentionally narrow: convert the remaining local-only evidence into a durable repository/CI guarantee, correct stale plan state, and leave an unambiguous release handoff for downstream consumers.

---

# Executive summary

The previous corrective pass landed the right architecture:

- `eggress-embed/ssh` no longer force-enables `eggress-pproxy-compat`;
- native/TOML `OutboundConnector` construction owns a verified `SshSessionCache`;
- explicit pproxy compatibility construction owns the compatibility SSH cache;
- direct mode does not allocate SSH state;
- no Eggpool-specific public API, feature, type, executor hook, or compatibility layer was added;
- focused OpenSSH regression coverage exists at `crates/eggress-embed/tests/ssh.rs` and has been run successfully on a host with OpenSSH available.

There are three remaining closure concerns:

1. The parent plan still advertises `READY FOR IMPLEMENTATION` despite containing successful closure evidence.
2. GitHub CI compiles the relevant feature slices but does not persistently execute the embed-level OpenSSH regression. The defect being fixed was a runtime state defect that compile-only checks cannot detect.
3. The OpenSSH fixture currently represents almost any setup failure as `None`, which is convenient for optional local testing but can silently turn a broken fixture into a passing CI test if the test body simply returns. A CI runtime gate is only meaningful if fixture setup failures fail the job once OpenSSH is expected to exist.

The current public release remains `v1.0.6`, which predates the fix. Releasing the corrected facade is a separate release action after this closure pass; do not make Eggpool remove its fallback against `1.0.6`.

---

# Objective

Close the outbound SSH facade correction without widening scope or changing the design that already landed.

The desired end state is:

```text
source fix
  + feature-boundary compile checks
  + real SSH runtime regression in CI
  + non-silent fixture failures in required CI
  + accurate completed plan state
  + release-ready evidence
```

After this pass, a maintainer should be able to answer all of the following from the repository alone:

- Does native/TOML SSH use verified host-key policy? **Yes.**
- Does pproxy SSH retain the explicitly compatibility-oriented policy? **Yes.**
- Does `ssh` alone avoid pulling in `eggress-pproxy-compat`? **Yes.**
- Does a listener-free `OutboundConnector` actually move bytes through an SSH upstream? **Continuously tested in CI.**
- Does invalid SSH authentication fail closed without leaking the password? **Continuously tested in CI.**
- Can a fixture setup error be mistaken for an ordinary optional local skip? **Not in the required CI path.**
- Is the parent corrective plan actually complete? **Its status says so and records final evidence.**
- Can Eggpool consume this from the currently published `1.0.6`? **No; a post-fix Eggress release is required first.**

---

# Non-goals / scope guardrails

Do not turn this closure pass into another SSH architecture project.

Specifically, do **not**:

- redesign `OutboundConnector`;
- introduce a public executor/session-cache builder;
- add an Eggpool-specific API, feature, type, configuration mode, or compatibility shim;
- change native host-key verification policy;
- make pproxy compatibility policy global;
- add new SSH authentication mechanisms;
- alter SSH protocol semantics unrelated to the regression;
- add a full feature-power-set matrix;
- make CI depend on Docker or an external SSH service;
- add a second SSH implementation;
- split `eggress-embed` into another crate;
- refactor the OpenSSH fixture into `eggress-testkit` merely for aesthetic deduplication;
- change downstream linker/LTO/strip settings;
- perform unrelated binary-size work;
- publish/tag a release unless the release workflow is explicitly being performed as a separate maintainer action.

The code design in `4f6ec74` is not under reconsideration unless a closure test reveals an actual correctness defect.

---

# Confirmed current state

## Feature boundary

`crates/eggress-embed/Cargo.toml` currently has the intended feature topology:

```toml
pproxy-compat = ["dep:eggress-pproxy-compat"]
ssh = [
    "dep:eggress-transport-ssh",
    "eggress-runtime/ssh",
    "eggress-pproxy-compat?/ssh",
]
```

This is correct. Preserve it.

## Executor ownership

`crates/eggress-embed/src/outbound.rs` now centralizes executor construction through private `ExecutorMode` / `build_outbound_executor` logic:

- native mode -> `SshSessionCache::new()`;
- pproxy compatibility mode -> `SshSessionCache::new_compatibility()`;
- direct mode -> no SSH cache.

This is correct. Preserve the constructor-level policy distinction and keep the helper private.

## Runtime regression coverage

`crates/eggress-embed/tests/ssh.rs` already covers the right failure surface:

1. pproxy SSH traverses a real OpenSSH server and echoes sentinel bytes;
2. invalid credentials fail closed and do not expose the password;
3. native TOML SSH rejects an untrusted host key.

The tests are more valuable than a constructor-only regression because the original defect occurred after parsing/compilation, when the chain executor attempted to use SSH without the required session cache.

## CI gap

`.github/workflows/ci.yml` currently runs no-default compile slices for:

```text
ssh
pproxy-compat
ssh,pproxy-compat
```

Keep those checks. They protect the Cargo feature boundary.

However, the workflow does not install an SSH server and does not run the embed OpenSSH regression. `cargo test --workspace --locked` is not a substitute because normal/default `eggress-embed` features do not enable SSH, and optional OpenSSH fixtures may skip when host tools are absent.

---

# Workstream 1 — Make the OpenSSH fixture fail meaningfully

## Goal

Preserve convenient local skipping when OpenSSH tools are genuinely unavailable, while ensuring an available-but-broken fixture cannot silently convert a regression into a pass.

## Primary file

- `crates/eggress-embed/tests/ssh.rs`

## Current weakness

`OpenSsh::start()` currently returns `Option<Self>` and uses `.ok()?` for many operations. This means all of the following can collapse into the same `None` result:

- `sshd` is not installed;
- `ssh-keygen` is not installed;
- temporary-directory creation fails;
- key generation fails;
- file copy/write fails;
- obtaining the user fails;
- spawning `sshd` fails;
- the server never becomes reachable.

Each test then treats `None` as a skip by returning early.

That behavior is acceptable for a best-effort developer machine check, but it is too permissive for a required CI regression.

## Required behavior

Distinguish **environment unavailable** from **fixture broken**.

A suitable shape is one of the following:

```rust
async fn start() -> Result<Option<Self>, FixtureError>
```

where:

- `Ok(None)` means only that required external commands are absent and the test may be skipped in optional/local execution;
- `Err(...)` means fixture setup was attempted and failed, and the test must fail;
- `Ok(Some(fixture))` means the SSH server is ready.

Or use a small equivalent helper if that produces cleaner test code.

Do not over-engineer a public test framework. A compact private fixture error using `io::Error`, `String`, or an existing simple error pattern is sufficient.

## Specific failure handling

Once both `sshd` and `ssh-keygen` are discoverable, all subsequent failures must be hard test failures:

- tempfile creation;
- host/client key generation;
- copying `authorized_keys`;
- ephemeral port allocation;
- determining the local user;
- writing `sshd_config`;
- spawning `sshd`;
- waiting for the server to accept connections.

Do not preserve broad `.ok()?` conversion after tool availability has been established.

The error text must not expose SSH passwords or private-key contents. Paths to temporary fixture files are acceptable unless the repository has a stricter existing test-redaction convention.

## Optional explicit CI requirement flag

If the implementation benefits from making CI intent explicit, a narrowly scoped environment variable such as:

```text
EGRESS_REQUIRE_OPENSSH_TESTS=1
```

may be used so missing `sshd`/`ssh-keygen` becomes a hard error in CI while remaining an ordinary skip locally.

This flag is optional. If CI installs the tools in a preceding step and the fixture distinguishes unavailable tools from setup failure correctly, an extra variable is not required.

Do not add a general test configuration subsystem for this.

## Acceptance criteria

- On a machine with no `sshd`/`ssh-keygen`, the focused test can still report a clear skip rather than fail unexpectedly, unless explicit required mode is enabled.
- If tools exist but key generation, config creation, daemon startup, or readiness fails, the test fails.
- Existing three behavioral assertions remain intact.
- No production API changes are introduced.

---

# Workstream 2 — Add a persistent embed SSH runtime CI gate

## Goal

Make GitHub CI reproduce the runtime behavior that was previously only qualified locally.

## Primary file

- `.github/workflows/ci.yml`

## Required CI setup

Use the existing Ubuntu Rust smoke job. Do not create a new operating-system matrix or separate workflow unless there is a demonstrated workflow-limit reason.

Install the minimal host dependency needed by the existing fixture before the focused SSH test. On `ubuntu-latest`, the intended shape is approximately:

```yaml
- name: Install OpenSSH test server
  run: |
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends openssh-server
    sudo mkdir -p /run/sshd
```

Exact packaging details may be adjusted if the runner image changes, but keep the dependency limited to the system OpenSSH server rather than introducing containers/services.

## Focused test command

Prefer the narrow feature slice instead of `--all-features`:

```bash
cargo test -p eggress-embed --locked --no-default-features \
  --features "ssh,pproxy-compat" \
  --test ssh -- --nocapture
```

Reasons:

- the test module requires both `ssh` and `pproxy-compat`;
- native TOML behavior is still exercised within that build;
- it proves native mode does not inherit pproxy compatibility policy even when both features coexist;
- it avoids unrelated optional features and the repository's documented warning about indiscriminate all-features CI gates.

If implementation discovers that another minimal feature is genuinely required by the test fixture/config parser, add only that feature and document why.

## Optional transport-level focused run

After OpenSSH is installed, it is reasonable to also execute:

```bash
cargo test -p eggress-transport-ssh --locked --test openssh -- --nocapture
```

Only keep this as a separate CI step if it is low-cost and materially improves coverage. The closure blocker is the **embed boundary** regression, not duplicating every transport test in another feature gate.

Do not let adding the lower-level run delay or complicate the required facade test.

## Ordering

Keep the existing feature compile checks. A sensible order is:

1. format;
2. clippy;
3. workspace tests;
4. optional CLI compile gate;
5. embed feature-boundary compile slices;
6. install OpenSSH server;
7. focused embed SSH runtime regression;
8. fuzz-target check.

Installing OpenSSH earlier is also acceptable if it makes the workflow simpler, but avoid changing unrelated test semantics accidentally unless desired.

## Runtime expectations

The focused CI test must actually execute all three tests rather than silently skip because `sshd` is unavailable.

Expected result:

```text
3 passed; 0 failed
```

If the test suite later gains additional SSH embed regressions, closure documentation should record the actual count rather than hard-code three forever.

## Acceptance criteria

- CI provisions a local OpenSSH server dependency on Ubuntu.
- The embed SSH test runs with the exact features needed for SSH + pproxy compatibility.
- Byte traversal passes through `OutboundConnector::connect_tcp()`.
- Invalid auth remains fail-closed and password-redacted.
- Native TOML rejects an untrusted fixture host key even though `pproxy-compat` is enabled in the same build.
- The job fails if the fixture is broken rather than treating arbitrary setup failures as an optional skip.
- Existing CI remains bounded; no feature matrix is introduced.

---

# Workstream 3 — Keep the feature-boundary compile checks

## Goal

Do not lose the compile-time dependency isolation while adding runtime coverage.

## Required commands

Retain at least:

```bash
cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features "ssh,pproxy-compat"
```

For closure evidence, also rerun dependency inspection locally:

```bash
cargo tree -p eggress-embed --locked --no-default-features --features ssh -e features
cargo tree -p eggress-embed --locked --no-default-features --features "ssh,pproxy-compat" -e features
```

Confirm:

- `ssh`-only contains `eggress-transport-ssh` and SSH runtime/server support;
- `ssh`-only does **not** activate `eggress-pproxy-compat`;
- combined mode activates both and forwards the compatibility crate's SSH feature.

There is no need to commit generated tree output. Record a concise result in closure evidence.

---

# Workstream 4 — Correct plan lifecycle state

## Goal

Make the planning records match reality.

## Files

- `plans/OUTBOUND_CONNECTOR_SSH_FEATURE_BOUNDARY_CORRECTIVE_PASS.md`
- this plan

## Parent plan update

After all required closure checks pass, change the parent plan status from:

```text
READY FOR IMPLEMENTATION
```

to a clear completed state, for example:

```text
COMPLETE — VERIFIED 2026-09-14
```

Use the actual completion date if implementation occurs later.

Do not erase the existing closure evidence. Append the final CI-backed evidence, including:

- implementation commit;
- closure-pass commit;
- CI run/result or head commit with passing CI;
- focused embed SSH test count;
- confirmation that fixture setup failures are no longer silently skipped when tools are present/required;
- feature-tree isolation result;
- confirmation that no public/downstream-specific API was added.

## This plan

When complete, update this plan's status to the same completed convention and append a short closure record.

The repository should not have one plan saying the work is complete in prose while its status still says implementation has not started.

---

# Workstream 5 — Fixture reuse decision

## Goal

Avoid unnecessary cleanup in a closure pass while preventing uncontrolled fixture duplication later.

There is already a substantial OpenSSH integration fixture under:

- `crates/eggress-transport-ssh/tests/openssh.rs`

The embed test currently carries a smaller fixture tailored to the public facade. That duplication is acceptable for this closure because extracting a shared harness would broaden the patch and could make a narrow regression harder to review.

Do **not** move the fixtures into `eggress-testkit` in this pass unless the CI-hardening work reveals that a small extraction is clearly simpler than maintaining two copies.

Use this trigger for future extraction:

> If a third crate/test suite needs to provision an OpenSSH daemon, or if the two existing fixtures begin receiving the same nontrivial maintenance fixes, extract the reusable daemon/key/config lifecycle into `eggress-testkit` while keeping behavior assertions in each consuming crate.

No new plan is required merely to document that future threshold.

---

# Workstream 6 — Release readiness and downstream handoff

## Goal

Separate code closure from release publication while making the downstream dependency requirement explicit.

## Current release state

The latest public release at the time this plan was written is `v1.0.6`, and the workspace still reports `1.0.6`. That release predates `4f6ec74`.

Therefore:

- do not tell downstreams that `1.0.6` contains the fixed embed SSH facade;
- do not remove Eggpool's native SSH fallback while it remains pinned to/reliant on `1.0.6` behavior;
- do not point Eggpool at an unreleased Git revision as the permanent architecture unless the maintainer explicitly wants a temporary git dependency.

## After this closure pass

Once CI is green and plan status is complete, the repository is **release-ready for this correction**.

Release publication should follow Eggress's existing version/tag/crates/binary workflow and is a separate maintainer action. Choose the next version according to the project's versioning policy rather than hard-coding a version number in this plan.

The release that carries the fix must include at minimum:

- commit `4f6ec74` or its equivalent ancestry;
- this CI closure pass;
- the corrected `eggress-embed` feature graph;
- the embed SSH regression.

After the fixed Eggress crate version is published, the downstream Eggpool handoff is:

1. update Eggpool to the fixed Eggress version;
2. use `eggress-embed` as the SSH outbound ownership boundary;
3. remove Eggpool's temporary `eggress-ssh-fallback` implementation path;
4. remove direct Eggress implementation-crate dependencies that existed solely to rebuild parser/config/compiler/executor/session-cache state;
5. retain any independent Eggpool test-only transport seam only if still needed for its own fixture semantics;
6. rerun Eggpool's full proxy matrix, SSH runtime checks, failure isolation tests, feature checks, and footprint comparison.

Do not make those Eggpool changes inside this Eggress closure pass.

---

# Required implementation order

Implement in this order unless a concrete repository constraint makes another order safer:

1. Harden `crates/eggress-embed/tests/ssh.rs` so only genuinely unavailable host tools can produce an optional skip; setup failures become test failures.
2. Run the focused embed SSH test locally with OpenSSH available and confirm all behavioral cases still pass.
3. Add the minimal OpenSSH installation/setup step to the existing Ubuntu CI job.
4. Add the focused `eggress-embed` SSH runtime test with `--no-default-features --features "ssh,pproxy-compat"`.
5. Keep and rerun the existing three feature-boundary compile slices.
6. Run dependency-tree inspection for `ssh` only and combined `ssh,pproxy-compat`.
7. Run full repository qualification.
8. Push implementation and confirm GitHub CI executes—not skips—the focused SSH regression successfully.
9. Update the parent plan status and closure evidence.
10. Update this plan status and closure evidence.
11. Leave release publication as the next explicit maintainer action.

---

# Verification checklist

## Focused runtime

```bash
cargo test -p eggress-embed --locked --no-default-features \
  --features "ssh,pproxy-compat" \
  --test ssh -- --nocapture
```

Expected: real OpenSSH fixture starts and all embed SSH cases pass.

If retained as an additional gate:

```bash
cargo test -p eggress-transport-ssh --locked --test openssh -- --nocapture
```

## Feature slices

```bash
cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features "ssh,pproxy-compat"
```

## Dependency isolation

```bash
cargo tree -p eggress-embed --locked --no-default-features --features ssh -e features
cargo tree -p eggress-embed --locked --no-default-features --features "ssh,pproxy-compat" -e features
```

## Repository qualification

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check -p eggress-cli --locked --no-default-features \
  --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon \
  --bins
cargo check --manifest-path fuzz/Cargo.toml --bins
```

If the repository's documented verification contract changes before implementation, follow the current authoritative `AGENTS.md` / CI documentation and record any deviation.

---

# Review checklist

A reviewer should explicitly verify:

- [ ] `OutboundConnector` production logic remains architecturally unchanged unless a test exposed a real defect.
- [ ] Native/TOML mode still uses verified SSH host-key policy.
- [ ] Pproxy compatibility mode still uses compatibility policy only at that constructor boundary.
- [ ] Direct mode still does not allocate SSH state.
- [ ] `ssh`-only still does not activate `eggress-pproxy-compat`.
- [ ] The OpenSSH fixture can skip only for genuinely missing optional tools in non-required mode.
- [ ] Once tools are present/required, fixture setup failures fail the test.
- [ ] GitHub CI installs/provisions OpenSSH and executes the embed runtime regression.
- [ ] CI proves byte traversal, auth failure/redaction, and native host-key rejection.
- [ ] No broad feature matrix or external network service was introduced.
- [ ] No Eggpool-specific API or dependency was added.
- [ ] Parent plan status is changed to complete only after CI-backed evidence exists.
- [ ] This plan is marked complete only after the same evidence exists.
- [ ] Release publication is not falsely claimed before a post-fix release/tag actually exists.

---

# Completion criteria

This closure pass is complete only when all of the following are true:

1. `crates/eggress-embed/tests/ssh.rs` no longer silently converts arbitrary fixture setup failures into skips.
2. GitHub CI provisions OpenSSH and actually executes the embed SSH runtime regression.
3. The pproxy SSH byte-traversal regression passes in CI.
4. Invalid SSH authentication remains fail-closed and credential-redacted in CI.
5. Native/TOML SSH still rejects an untrusted host key in the same combined-feature build.
6. The `ssh`, `pproxy-compat`, and combined compile slices pass.
7. Dependency-tree evidence still proves `ssh` alone does not activate `eggress-pproxy-compat`.
8. Full repository fmt/check/clippy/test and existing optional compile gates pass.
9. The parent corrective plan status is changed from `READY FOR IMPLEMENTATION` to a completed/verified state and contains final CI-backed closure evidence.
10. This plan is also marked complete with its implementation commit and CI result.
11. No Eggpool-specific public surface, unrelated SSH feature, or architecture expansion was introduced.
12. Documentation clearly states that downstream fallback removal requires a new Eggress release containing the fix; `v1.0.6` must not be treated as containing it.

---

# Handoff guidance

This should be a small closure patch, not another feature pass. Prefer the smallest change set that turns the already-correct implementation into a continuously enforced contract.

The critical distinction is between **compile-time feature correctness** and **runtime executor correctness**. The repository already has the former. This pass must make the latter persistent in CI, because the original missing-session-cache defect was invisible to parsing, construction, and compilation alone.

Do not chase additional cleanup unless it is required to make that runtime proof reliable.