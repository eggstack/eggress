# pproxy Corrective Phase 1 — CLI Semantics and Fail-Closed Execution

## Status

**PLANNED**

## Parent roadmap

[`PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`](PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md)

## Objective

Correct the known command-line semantic mismatches against `pproxy==2.7.9`, make every parsed compatibility option reach an explicit translation decision, and prevent the `pproxy` compatibility binary from starting Eggress when requested behavior is unsupported or ignored.

The phase is intentionally narrow. It owns the pproxy parser, translation diagnostics, startup gating, and the minimum runtime/config plumbing needed for accurate `--reuse` behavior. It does not authorize daemonization, a new system-proxy subsystem, connection pooling, or broad CLI redesign.

## Confirmed defects

1. `-d` is currently aliased to `--daemon`. Upstream uses `-d` for debug tracebacks and reserves `--daemon` for daemon mode.
2. `--reuse` is currently described as connection pooling. Upstream uses it for listener `SO_REUSEPORT` behavior on supported systems.
3. `--auth <seconds>` is parsed into `auth=<value>` but no translator branch consumes it.
4. `--sys` currently performs read-only inspection and continues startup. Upstream applies system proxy settings, so the current action is not an equivalent implementation.
5. Unknown and unsupported options produce warnings but do not stop service startup.
6. Help text, parser tests, diagnostics, capability manifests, and `compat/pproxy-2.7.9/cli-baseline.json` disagree.

## Required source inspection before edits

Read these files before changing code:

- `compat/pproxy-2.7.9/cli-baseline.json`
- `crates/eggress-pproxy-compat/src/args.rs`
- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-pproxy-compat/src/diagnostics.rs`
- `crates/eggress-pproxy-compat/src/warnings.rs`
- `crates/eggress-pproxy-compat/src/error.rs`
- `crates/eggress-cli/src/pproxy_main.rs`
- `crates/eggress-cli/tests/pproxy_binary.rs`
- any CLI exit-code tests under `crates/eggress-cli/tests/`
- `crates/eggress-config/src/` listener configuration types
- `crates/eggress-server/src/` TCP listener binding path
- `crates/eggress-system-proxy/src/apply.rs`
- `crates/eggress-system-proxy/src/capability.rs`
- `crates/eggress-system-proxy/src/command_runner.rs`

Do not assume the historical parity documents are correct when they disagree with the pinned baseline or source.

## Design rules

### Typed option representation

Stop using an undifferentiated `Vec<String>` as the only semantic representation for known flags. The smallest acceptable change is to retain raw arguments for diagnostics while introducing typed fields for options whose values affect behavior.

A suitable bounded representation is:

```rust
pub struct PproxyArgs {
    pub local: Vec<String>,
    pub remotes: Vec<String>,
    pub verbose_level: u8,
    pub debug: bool,
    pub daemon: bool,
    pub reuse_port: bool,
    pub auth_timeout: Option<Duration>,
    pub system_proxy: bool,
    pub known_options: Vec<KnownOption>,
    pub unknown_flags: Vec<String>,
}
```

Exact field names may follow current conventions. Do not build a generic dynamic option registry. Known options should have one parser branch and one explicit translation/execution branch.

### Exhaustive decisions

Every recognized option must end in exactly one classification:

- behaviorally implemented;
- implemented as a documented native equivalent;
- accepted with a precise compatibility warning where semantics remain safe;
- rejected as unsupported before runtime startup.

There must be no state where an option is listed in `KNOWN_RAW_FLAG_KEYS` but has no downstream handling.

### Fail-closed compatibility execution

The compatibility binary must not start a service when translation reports unsupported, unknown, ignored, or materially non-equivalent behavior.

`eggress pproxy check -- ...` may continue to return a report containing all classifications. The `pproxy` execution path must return a stable non-zero exit code before temporary config creation or supervisor startup.

Do not add a warning-only escape hatch in this phase. A future explicit override would be a product decision; silent or implicit partial execution is not acceptable.

## Workstream A — Correct parser aliases and option arity

### `-d` and `--daemon`

Implement separate parser branches:

- `-d` sets the compatibility debug flag.
- `--daemon` sets the daemon request.
- `-d` must never populate daemon state.
- `--daemon` remains intentionally unsupported unless the repository already has a complete foreground-detach lifecycle, PID ownership, log routing, signal handling, and shutdown contract. This phase must not create those facilities.

For the debug native equivalent:

- preserve normal log verbosity semantics; do not silently turn `-d` into `-v`;
- enable additional compatibility error detail where available;
- set or recommend `RUST_BACKTRACE=1` for panic diagnostics without treating panics as normal errors;
- ensure normal parser/config/runtime errors still use stable user-facing messages and exit codes.

Tests must prove `-d` and `--daemon` are independent.

### Value-taking options

Retain strict required-value behavior for:

- `-l` / `--listen`
- `-r` / `--remote`
- `-ul` / `--udp-listen`
- `-ur` / `--udp-remote`
- `--rulefile`
- `--log`
- `-s`
- `-a`
- `--ssl`
- `-b`
- `--pac`
- `--test`
- `--get`
- `--auth`

Add a table-driven parser test sourced from the checked-in baseline so future arity drift is caught in one place. Do not create a generated parser framework; a static test table is sufficient.

### Unknown flags

Unknown flags must remain distinct from known-but-unsupported flags. Do not report `--daemon` and an invented `--foo` as the same category.

Required categories:

- `unknown-option`
- `unsupported-option`
- `invalid-option-value`
- `platform-unsupported`
- `non-equivalent-option`

Use the existing diagnostic types where possible. Add types only when the current warning-only model cannot express a fatal classification cleanly.

## Workstream B — Implement accurate `--reuse`

### Required semantics

Treat `--reuse` as listener socket reuse, not upstream connection reuse or connection pooling.

The implementation target is the upstream `SO_REUSEPORT` behavior for TCP listeners on the platform set where Eggress can support it safely and predictably. Do not implement a reusable upstream connection pool.

### Configuration plumbing

Add one listener-level boolean, using the existing config model and compiled runtime snapshot:

```toml
[[listeners]]
reuse_port = true
```

The compatibility translator sets it for listeners created from the pproxy command. Native Eggress configuration may expose the same field if doing so is the simplest truthful design.

Touch only the minimum path:

- compatibility typed arguments;
- translated listener config;
- config validation/schema;
- server listener bind helper;
- focused tests.

Do not thread the flag through unrelated routing, upstream, metrics, or protocol objects.

### Socket implementation

Prefer a safe maintained socket API. `socket2` is acceptable if a direct dependency is needed and its linked cost is negligible because Tokio already uses the same family. Do not add project-local unsafe socket option code.

Required behavior:

- supported Unix target: create/configure the socket, set reuse-port before bind, bind, listen, make nonblocking, and convert to Tokio listener;
- unsupported target: compatibility execution fails before startup with `platform-unsupported`;
- native configuration validation reports the same platform boundary;
- a false/default value follows the current binding path unchanged.

Match the pinned upstream platform claim rather than broadening support speculatively. If the baseline states Linux-only, do not claim Windows or macOS parity without a focused verified probe.

### Tests

At minimum:

- parser sets `reuse_port=true` for `--reuse`;
- translator emits the listener field;
- help and diagnostics call it `SO_REUSEPORT`/listener reuse;
- two listeners can bind the same address under the supported platform test, gated with `cfg`;
- unsupported-platform validation is deterministic and does not attempt service startup;
- no code or documentation describes `--reuse` as connection pooling.

## Workstream C — Resolve `--auth`

### Upstream contract

Treat `--auth <seconds>` as the client authentication reuse interval for the same source IP, not as a general credential timeout and not as an upstream health interval.

Validate that the value is a non-negative bounded integer duration. Reject malformed, negative, overflow, NaN-like, or empty values before translation.

### Decision gate

Inspect the native listener authentication path. Choose one of two bounded outcomes:

#### Outcome 1 — Local implementation is available

Implement only when all are true:

- listener authentication already has access to the peer IP;
- successful authentication has one clear hook;
- credential policy can be represented without duplicating protocol logic;
- a small bounded TTL map can be owned by the listener/runtime snapshot;
- expiration and maximum entry count can be bounded without a cleanup task framework.

Use a keyed cache with:

- peer IP as the key;
- monotonic expiration;
- bounded maximum entries;
- lazy expiration on access;
- no credential or secret logging;
- no unbounded background task.

Do not share authentication state across unrelated listeners unless upstream behavior and current architecture require it.

#### Outcome 2 — Local implementation would require a new subsystem

Mark `--auth` unsupported and fail before startup with a message such as:

> `--auth` requests pproxy's per-client authentication reuse interval; Eggress currently authenticates per connection and cannot apply this option safely.

Do not accept and ignore the value. Do not add a large cache subsystem solely to turn the matrix green.

### Tests

For either outcome:

- valid values parse into a typed duration;
- invalid values fail with `invalid-option-value`;
- execution never silently ignores `--auth`;
- the compatibility report records the final tier accurately.

If implemented, add focused same-IP reuse, expiration, different-IP isolation, and bounded-entry tests. Avoid wall-clock sleeps; use an injectable clock or direct expiry values if the existing architecture supports it.

## Workstream D — Resolve `--sys`

### Current capability

The system-proxy crate already contains capability inspection, apply planning, command execution, and rollback types. The compatibility binary currently invokes inspection only. Determine whether the existing apply API can produce the exact bounded upstream behavior without adding a new platform backend.

### Required decision

Choose one of these outcomes:

#### Outcome 1 — Existing apply path is sufficient

Use the listener selected by pproxy compatibility mode to build an apply plan through the existing system-proxy crate. Required safeguards:

- capability check before any command;
- explicit platform diagnostics;
- apply only after config translation succeeds;
- rollback on startup failure;
- rollback during normal shutdown and handled termination where the existing runtime permits it;
- no credential-bearing values in command logs;
- no shell command construction outside the existing command-runner abstraction.

Do not redesign the system-proxy crate.

#### Outcome 2 — Lifecycle-safe apply/rollback is incomplete

Classify `--sys` as unsupported for compatibility execution and fail before startup. Preserve the separate native inspection command, but do not present inspection as equivalent to pproxy `--sys`.

The default should be Outcome 2 unless the existing apply/rollback lifecycle is already complete and locally callable. A partial system setting that survives a failed process is worse than explicit non-parity.

### Tests

- `--sys` never triggers inspection-only startup behavior;
- the compatibility report states either implemented apply/rollback semantics or explicit non-parity;
- mock command-runner tests cover apply and rollback if implemented;
- no real system proxy mutation occurs in unit or hosted CI tests.

## Workstream E — Fatal startup gating

Update `crates/eggress-cli/src/pproxy_main.rs` so execution order is:

1. parse arguments;
2. translate and classify every option;
3. print fatal diagnostics in stable order;
4. return the compatibility/config exit code when any fatal classification exists;
5. only then create temporary config, apply optional system integration, initialize runtime logging, or start the supervisor.

Unknown flags are fatal. Unsupported options are fatal. Non-equivalent options are fatal unless the phase implements a safe native equivalent. Warnings are reserved for benign differences that cannot alter requested routing, security, listening, authentication, or system state.

Do not write a temp config before fatal validation completes.

Preserve the report/check path so users can inspect all issues at once.

## Workstream F — Help, diagnostics, and baseline synchronization

Update in the same implementation change:

- `crates/eggress-cli/src/pproxy_main.rs` help text;
- `compat/pproxy-2.7.9/cli-baseline.json` only where the checked-in baseline is wrong, not to match Eggress behavior;
- parser and binary tests;
- active compatibility manifest entries for the corrected flags;
- practical compatibility matrix rows for final implementation decisions.

Phase 4 will consolidate broader documentation. Phase 1 must still prevent newly corrected behavior from being documented incorrectly.

Required help facts:

- `-d`: debug diagnostics/tracebacks native equivalent, not daemon;
- `--daemon`: unsupported, foreground service manager recommended;
- `--reuse`: listener `SO_REUSEPORT`, with platform boundary;
- `--auth`: exact implemented or unsupported status;
- `--sys`: exact apply/rollback or unsupported status;
- unsupported options stop startup.

## Focused verification

Run during implementation:

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test pproxy_binary
cargo test -p eggress-cli --test cli_exit_codes
```

Use the actual exit-code test filename if it differs.

When listener/config code changes:

```bash
cargo test -p eggress-config
cargo test -p eggress-server
cargo test -p eggress-runtime
```

Final phase gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Optional focused oracle probe is permitted only for unresolved `-d`, `--reuse`, `--auth`, or `--sys` details. Do not run or add the complete certification suite as a phase gate.

## Acceptance criteria

Phase 1 is complete only when all are true:

- `-d` and `--daemon` parse independently;
- `-d` can never request daemon mode;
- `--daemon` cannot start a partially equivalent foreground service;
- `--reuse` is implemented as listener reuse on the supported platform or rejected before startup with the correct platform/feature reason;
- no source, help text, manifest, or diagnostic calls `--reuse` connection pooling;
- `--auth` has a typed validated value and either a bounded implementation or a fatal unsupported classification;
- `--sys` applies and rolls back settings safely through existing abstractions or fails before startup;
- every recognized flag has an explicit downstream decision;
- unknown, unsupported, ignored, malformed, or materially non-equivalent inputs produce a non-zero exit before temp config creation and runtime startup;
- the check/report command still returns a complete structured report;
- focused tests cover parser arity, aliases, classifications, exit behavior, and any new listener/system integration;
- the full workspace gate passes;
- no daemonization, connection pool, generic option registry, new CI workflow, or new compatibility framework is added.

## Handoff notes for the implementer

- Start with failing tests for the five confirmed defects.
- Correct the typed parser model before editing help text.
- Search for every use of `raw_flags`, `KNOWN_RAW_FLAG_KEYS`, `has_unsupported`, `reuse`, `auth=`, `sys`, and `daemon` before deciding the smallest edit set.
- Prefer explicit unsupported behavior over speculative half-implementation.
- Keep diagnostic messages stable and test category/meaning rather than full prose unless the existing CLI contract already asserts exact strings.
- Record the final `--auth` and `--sys` decision in the parent roadmap when closing this phase.
- Do not create a separate Phase 1 completion document; change this file's status and add the implementation commit range.