# pproxy Strict Phase 6 — CLI and Process Behavior Closure

## Objective

Make the `pproxy` compatibility executable match the observable command/process behavior of the exact 2.7.9 parser and run loop for the features Eggress has implemented.

This phase must not reintroduce false-gap flags discovered in Phase 0.

## Actual 2.7.9 option surface

Treat Phase 0 as authoritative. The expected parser set is:

- `-l`
- `-r`
- `-ul`
- `-ur`
- `-b`
- `-a`
- `-s`
- `-d`
- `-v`
- `--ssl`
- `--pac`
- `--get`
- `--auth`
- `--sys`
- `--reuse`
- `--daemon`
- `--test`
- `--version`

Do not add `--log`, `-f/--config`, or `--rulefile` as strict compatibility work unless Phase 0 overturns the source audit.

## Primary files

- `crates/eggress-pproxy-compat/src/args.rs`
- `crates/eggress-pproxy-compat/src/gate.rs`
- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-cli/src/*`
- standalone `pproxy` binary source
- tracing initialization in CLI/runtime
- Python `pproxy.server.main` / `__main__` bridge from Phase 4
- CLI integration tests

## Parser fidelity

Use one canonical compatibility parser/IR for both:

- standalone `pproxy` executable;
- `eggress pproxy ...` compatibility entry point;
- Python compatibility main path where feasible.

Verify:

- value-taking flags consume exactly one value;
- repeatable flags append in declaration order;
- `-v`, `-vv`, `-vvv` count behavior;
- `-d` count/boolean behavior matching argparse;
- integer parse behavior for `-a` and `--auth`;
- scheduler choices and argparse failure category;
- no-argument default listener;
- `--version` exits without runtime startup;
- `--test` exits after testing remotes;
- unsupported optional feature syntax fails before partial service startup.

## `-d` behavior

pproxy's debug mode is not just a logging filter: handler exceptions that would normally be swallowed/logged are re-raised, producing traceback-oriented diagnostics.

Reproduce the observable distinction without destabilizing the native runtime:

- compatibility handlers preserve ordinary graceful error handling when `-d` is absent;
- with `-d`, the root error is surfaced through the compatibility task/process error path;
- Rust backtraces are not a substitute for Python tracebacks, but error visibility, exit/failure category, and stderr behavior should match as closely as practical;
- do not enable global debug behavior for native Eggress.

Capture exact oracle outputs for representative malformed client input and startup failure.

## `-v/-vv` behavior

pproxy verbose mode includes connection logging and, at higher verbosity, traffic/stat reporting. Eggress already has metrics/tracing; adapt those rather than creating a duplicate counter system.

Required compatibility behavior:

- `-v`: connection/event text at the expected human-readable level;
- `-vv` and higher: expose representative traffic statistics at the cadence/interaction boundary established by the oracle;
- timestamps/terminal coloring need only match if Phase 0 classifies them as observable compatibility requirements;
- no secrets in logs.

If pproxy's interactive stdin statistics are retained as strict scope, implement a small compatibility frontend over existing metrics snapshots. Do not place interactive behavior in the core runtime.

## Startup output

Capture and normalize these cases:

- default mixed listener startup;
- TCP listener startup;
- UDP listener startup;
- backward listener startup;
- TLS listener startup;
- partial bind failure;
- unsupported optional feature;
- malformed URI/flag.

Match stdout vs stderr and exit code where doing so does not hide a serious error.

## `--test`

Ensure the compatibility path tests each configured remote in the same declaration order and exits without starting listeners. Preserve the existing native upstream-check implementation but match compatibility output/failure semantics.

Use local HTTP/HTTPS fixtures rather than public internet targets.

## `--reuse`

Verify SO_REUSEPORT is applied only where pproxy 2.7.9 applies it and the platform supports it. Unsupported platforms should match the closest pproxy-visible failure/behavior rather than pretending the option succeeded.

This flag is not connection pooling.

## Signals and shutdown

Verify:

- Ctrl-C;
- SIGTERM on Unix;
- listener close/wait behavior;
- UDP socket closure;
- reverse worker cancellation;
- system proxy rollback from Phase 2;
- no lingering tasks after run-loop exit.

Compatibility output such as `exit` should be matched only if the oracle consistently exposes it.

## `--daemon` boundary

Parsing and classification belong here, but actual daemonization is Phase 9 optional tail work. Until Phase 9 lands, the execution gate should produce a precise optional-feature diagnostic rather than silently ignoring the flag.

## Test strategy

Create a compact table-driven CLI oracle test for actual 2.7.9 flags. Each case should record only:

- argv;
- expected parse success/failure;
- exit code;
- stdout/stderr category/important substring;
- whether a listener was started.

Do not snapshot full nondeterministic output.

## Non-goals

- False-gap flags removed in Phase 0.
- Exact ANSI color bytes unless downstream compatibility demonstrably depends on them.
- Rebuilding Python's argparse internals.
- Mandatory oracle tests on every normal PR.
- Daemonization implementation; Phase 9.

## Acceptance criteria

1. All actual 2.7.9 flags have exact arity/default/repetition behavior in both compatibility entry points.
2. False-gap flags are not advertised as pproxy 2.7.9 options.
3. `-d` produces a materially distinct error-propagation mode rather than only changing tracing level.
4. `-v/-vv` expose oracle-aligned human connection/stat information without a second metrics backend.
5. `--test` performs remote checks and exits before listener startup.
6. `--reuse` is verified at the socket layer on supported systems.
7. Startup failures, unknown options, and unsupported optional features cannot launch a partial service.
8. SIGINT/SIGTERM cleanup closes listeners, UDP resources, reverse workers, and restores `--sys` state.
9. The Python `python -m pproxy` entry follows the same parser/process contract.
