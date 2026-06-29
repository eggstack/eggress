# Phase 18 Plan: pproxy Oracle and Evidence Harness

## Purpose

Phase 18 establishes the compatibility evidence foundation for all future real pproxy parity work. The immediate objective is to stop treating compatibility as a documentation claim or synthetic-test claim and instead make real pproxy behavior the oracle for every compatible feature.

This phase must produce a repeatable harness that launches Python `pproxy`, launches Eggress in equivalent compatibility modes, drives both with the same clients and fixture servers, captures observable behavior, and emits a machine-readable parity report.

## Non-goals

This phase should not implement new protocol functionality except small test hooks or diagnostic plumbing needed to run the harness. Do not repair Shadowsocks, UDP, HTTP persistence, SSR, or Python API parity in this phase. The output should expose those gaps precisely and make later phases safer.

## Target pproxy version

Pin the initial parity target to `pproxy==2.7.9`. The harness should make the target version explicit in all generated reports and should fail loudly if a different pproxy version is installed unless an override is provided.

Add a single source of truth, for example:

```text
tests/compat/pproxy_target.toml
```

Suggested contents:

```toml
[target]
package = "pproxy"
version = "2.7.9"
python = "python3"

[oracle]
startup_timeout_ms = 5000
shutdown_timeout_ms = 5000
connect_timeout_ms = 3000
io_timeout_ms = 3000
```

## Work items

### 18.1 Compatibility manifest

Create a manifest that enumerates all known pproxy features, even unimplemented ones. This must become the authoritative evidence index consumed by docs and tests.

Suggested path:

```text
tests/compat/pproxy_manifest.toml
```

Each entry should include:

- stable feature id;
- category: `protocol`, `cli`, `uri`, `python-api`, `routing`, `udp`, `transport`, `platform`, `security`, `packaging`;
- pproxy version where behavior was observed;
- Eggress implementation status;
- evidence level;
- test names that support the claim;
- known divergence notes;
- required external dependency, if any.

Suggested evidence levels:

```text
unimplemented
implemented_synthetic
implemented_differential
implemented_interop
compatible
intentional_non_parity
```

Rules:

- `compatible` requires real pproxy differential evidence or external protocol interop evidence.
- `implemented_synthetic` is not enough for compatibility.
- `intentional_non_parity` requires a rationale and user-visible diagnostic.
- Documentation generation should fail if README or parity docs mark a feature compatible without a matching manifest entry.

### 18.2 Oracle process runner

Build a Rust test-support crate or module that can start and supervise real pproxy processes.

Suggested location:

```text
crates/eggress-testkit/src/pproxy_oracle.rs
```

Capabilities:

- create isolated temporary work directories;
- allocate unused local TCP and UDP ports;
- launch `python -m pproxy` or the equivalent pproxy entry point;
- detect readiness by probing bound ports rather than sleeping;
- capture stdout and stderr concurrently;
- terminate gracefully, then kill on timeout;
- redact credentials from captured logs;
- preserve logs on test failure;
- expose process metadata to the parity report.

The runner must avoid brittle timing. Prefer readiness probes and bounded retries. Every spawned process must be tied to a guard that cleans up on panic or test failure.

### 18.3 Eggress compatibility runner

Create a parallel runner for Eggress that starts the binary or library in equivalent configurations.

Capabilities:

- launch `eggress pproxy run -- ...` for CLI compatibility cases;
- optionally launch `eggress` with generated TOML for lower-level runtime cases;
- expose bound addresses;
- capture logs;
- apply the same timeout and cleanup rules as the pproxy oracle;
- support dynamic fixtures such as chained upstreams and local echo servers.

The harness should prefer the public CLI compatibility path for CLI parity claims. Direct library startup should be used only when testing Eggress-native internals or where the pproxy equivalent is also library-level.

### 18.4 Fixture servers

Add reusable fixtures for deterministic local network behavior.

Required fixtures:

- TCP echo server with byte-exact echo and controlled close behavior;
- UDP echo server;
- HTTP origin server for ordinary forward-proxy tests;
- HTTP CONNECT upstream test server;
- SOCKS4 upstream test server;
- SOCKS5 upstream test server;
- TLS echo/origin fixture with local test certificates;
- malformed handshake clients for negative cases.

Each fixture should expose:

- bound address;
- observed request/handshake bytes when needed;
- connection counters;
- deterministic shutdown;
- failure injection options for timeout/reset tests.

### 18.5 Differential case model

Define a common model for one parity case.

Suggested Rust shape:

```rust
struct PproxyCase {
    id: &'static str,
    feature_id: &'static str,
    pproxy_args: Vec<String>,
    eggress_args: Vec<String>,
    client_script: ClientScript,
    expected: ExpectedComparison,
    timeout: Duration,
}
```

Comparison categories:

- payload equality;
- status-code equality;
- close behavior equality;
- error class equality;
- exit code equality;
- stdout/stderr pattern equality;
- negative-case equivalence;
- explicit allowed divergence.

The harness should support structured allowed divergence only when documented in the manifest.

### 18.6 Machine-readable parity report

Emit a report after compatibility test runs.

Suggested path:

```text
target/compat/pproxy-parity-report.json
```

Report fields:

- Eggress commit SHA if available;
- pproxy version;
- OS/platform;
- Rust version;
- Python version;
- enabled feature gates;
- each manifest feature id;
- evidence level before run;
- tests executed;
- pass/fail/skip status;
- skip reason;
- observed divergence;
- updated suggested evidence level.

Add a human-readable markdown summary as optional generated output:

```text
target/compat/pproxy-parity-report.md
```

### 18.7 CI integration

Add a dedicated compatibility workflow.

Suggested workflow:

```text
.github/workflows/pproxy-compat.yml
```

CI steps:

1. install Rust stable;
2. install Python;
3. install `pproxy==2.7.9` in an isolated venv;
4. build Eggress;
5. run oracle/differential tests;
6. upload parity report artifacts.

The job may start as non-blocking if runtime is high, but it must be visible. Once stable, make it required for compatibility-labeled changes.

### 18.8 Documentation synchronization

Update:

- `docs/PARITY_MATRIX.md` to reference the manifest and report discipline;
- `docs/PPROXY_PARITY_SPEC.md` to state that pproxy 2.7.9 is the pinned oracle;
- `docs/REAL_PPROXY_PARITY_ROADMAP.md` if phase details need alignment;
- README wording so current claims distinguish `implemented`, `supported`, and `compatible`.

Add an explicit rule: no new README compatibility checkbox may be checked without a manifest entry and evidence.

## Initial cases to migrate into the harness

Start with current claimed compatibility cases:

- HTTP CONNECT to direct TCP echo;
- SOCKS5 CONNECT to direct TCP echo;
- SOCKS5 through HTTP upstream;
- SOCKS5 through SOCKS5 upstream;
- HTTP auth rejection;
- SOCKS5 auth rejection;
- `-l` and `-r` CLI parsing;
- pproxy compat `translate`, `check`, and `run` paths;
- unsupported flag diagnostics for known rejected flags.

Do not expand to new protocol gaps until this baseline is stable.

## Validation commands

At minimum:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p eggress-testkit pproxy_oracle
cargo test --test differential_pproxy -- --nocapture
python -m pip install 'pproxy==2.7.9'
python -m pproxy --help
```

If a new workflow is added, document the local equivalent command in `docs/TESTING.md`.

## Acceptance criteria

Phase 18 is complete when:

- `pproxy==2.7.9` is pinned as the compatibility oracle.
- A compatibility manifest exists and covers all current parity-matrix features.
- Real pproxy and Eggress can be launched, probed, compared, and cleaned up by tests.
- Baseline HTTP/SOCKS differential cases run locally without manual setup beyond installing pproxy.
- CI has a visible compatibility workflow or job.
- A machine-readable parity report is generated.
- Documentation no longer allows unsupported or synthetic-only features to masquerade as compatible.

## Risks

The main risk is flaky process orchestration. Mitigate by using readiness probes, bounded retries, process guards, and per-test temp directories.

A second risk is overfitting to pproxy quirks that are bugs. The harness should record observed behavior first. Later phases can decide whether to match, safely diverge, or gate behavior.

A third risk is CI instability from external package installation. Pin pproxy and Python versions where practical, cache dependencies, and preserve logs as artifacts.

## Handoff notes

This phase should be implemented before additional protocol parity work. Later phases should extend the manifest and harness rather than inventing one-off compatibility tests.
