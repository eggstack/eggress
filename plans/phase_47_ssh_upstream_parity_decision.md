# Phase 47: SSH upstream parity decision and implementation path

## Goal

Resolve SSH upstream parity. SSH is one of the largest remaining differences between eggress and pproxy. This phase must either implement SSH upstream transport with a secure, tested design or classify it as intentional non-parity with stable diagnostics and documentation.

Do not leave SSH as a vague unsupported item. The decision must be explicit and reflected in manifest, CLI inventory, Python reports, and generated parity docs.

## Current baseline

- `ssh://` URIs are recognized enough to emit diagnostics.
- SSH listener/upstream transport is not implemented.
- pproxy supports SSH upstreams in some form.
- Eggress currently rejects SSH upstreams and SSH listeners.
- Manifest/report list SSH as unsupported.

## Decision criteria

Implement SSH only if the project is willing to own:

- SSH client transport dependencies;
- host-key verification and known-hosts policy;
- authentication methods;
- direct-tcpip channel lifecycle;
- keepalive and reconnect semantics;
- flow control and backpressure;
- security documentation;
- interop tests;
- Python packaging impact.

If any of these are unacceptable, classify SSH as intentional non-parity rather than unsupported.

## Workstream A: pproxy SSH behavior inventory

Inspect pproxy 2.7.9 behavior for:

- URI grammar: `ssh://user:pass@host:port`, key files, query params, default ports;
- authentication methods: password, key, agent;
- host-key verification defaults;
- command/channel type used for proxying;
- timeout and reconnect behavior;
- chaining behavior with SSH hops;
- error output and exit behavior.

Record findings in `docs/adr/ADR_ssh_upstream_parity.md`.

## Workstream B: implementation design if accepted

### Dependency selection

Evaluate Rust SSH client libraries:

- async support;
- maintenance status;
- host-key verification hooks;
- agent/key support;
- license compatibility;
- cross-platform behavior;
- packaging impact for PyPI wheels.

Prefer pure Rust if practical. Avoid libssh/OpenSSL build complexity unless explicitly accepted.

### Security model

Define secure defaults:

- host-key verification must be on by default;
- insecure host-key acceptance must require explicit opt-in;
- password/key material must be redacted and zeroized where practical;
- known-hosts path should be configurable;
- agent forwarding should be off unless deliberately supported;
- no shell command execution path should be exposed for proxying.

### Runtime integration

Implement SSH upstream as a transport connector:

- connect to SSH server;
- authenticate;
- open `direct-tcpip` channel to target;
- expose it as AsyncRead/AsyncWrite for existing relay;
- enforce per-hop timeout;
- integrate with upstream health checks;
- handle channel close/half-close semantics.

### Config/URI translation

Support native TOML and pproxy URI translation:

- `ssh://user:pass@host:22`
- optional key path if pproxy syntax supports it;
- optional known-hosts path in native config;
- `ssh+in` should remain unsupported unless reverse semantics are designed;
- SSH in multi-hop `__` chains only after single-hop works.

## Workstream C: intentional non-parity path if rejected

If SSH is not implemented:

- change manifest tier from `unsupported` to `intentional_non_parity` if the rationale is permanent;
- add ADR explaining why;
- keep parser-level recognition for good diagnostics;
- update Python/CLI diagnostics to recommend using OpenSSH dynamic forwarding or an external SOCKS/HTTP proxy;
- update generated report with caveat class `missing_protocol_transport` or `intentional_non_parity`.

## Tests if implemented

- Unit tests for URI parsing and redaction.
- Config validation tests for missing auth/host-key policy.
- Integration test against a local test SSH server if feasible.
- Interop test using OpenSSH server container or fixture.
- Chain tests: HTTP/SOCKS -> SSH -> target.
- Failure tests: bad password, bad host key, unreachable SSH server, target refused.
- Python check tests for SSH URI support/error behavior.

## Acceptance criteria

One of these must be true:

### Implemented path

- SSH upstream works as a single-hop upstream with secure host-key policy.
- SSH URI translation compiles to valid config.
- SSH in chains works or is explicitly refused with diagnostics.
- Interop test exists against a real SSH server.
- Manifest entries are promoted only to the evidence level achieved.

### Non-parity path

- ADR documents the decision.
- Manifest classifies SSH as intentional non-parity or explicitly unsupported with rationale.
- CLI/Python diagnostics are stable and actionable.
- README/PARITY_MATRIX/generated report agree.

## Verification commands

```bash
cargo fmt --all -- --check
cargo test -p eggress-pproxy-compat ssh
cargo test -p eggress-cli --test pproxy_cli ssh
cargo test --workspace
python -m pytest python/tests/test_pproxy_dropin.py -v
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

If implemented, add gated SSH interop command.

## Non-goals

- Do not support shell command execution.
- Do not disable host-key verification by default.
- Do not add agent forwarding without a separate security design.
- Do not support SSH listener mode unless pproxy and eggress architecture both justify it.
