# Phase 35 Plan: Security Containment and Abuse Resistance

## Purpose

Phase 35 hardens Eggress as a network-facing proxy toolkit after the broad pproxy parity work. The project now exposes many surfaces: TCP/UDP listeners, chains, Shadowsocks, Trojan upstreams, transparent listeners, Unix sockets, reverse/backward proxying, Python embedding, admin endpoints, metrics, config reload, compatibility translation, and optional system proxy work.

The purpose of this phase is to review and contain the security risks created by those surfaces. The goal is not only to prevent vulnerabilities; it is to make dangerous configurations explicit, observable, testable, and hard to enable accidentally.

## Scope

This phase covers:

- Open-proxy prevention and bind-address safety.
- Credential redaction and secret handling.
- Admin/metrics exposure hardening.
- Reverse/backward proxy abuse controls.
- Transparent proxy privilege boundaries.
- Python embedding security posture.
- Config validation for dangerous combinations.
- Resource exhaustion limits.
- Dependency/supply-chain review.
- Fuzz/property/security regression tests.
- Documentation and manifest/evidence updates.

## Non-goals

Do not implement a full WAF, IDS, or authentication service.

Do not add complex policy engines unless needed to make existing proxy features safe.

Do not make local development painful by requiring auth for loopback-only toy configs.

Do not silently alter user traffic or system proxy settings.

## Work items

### 35.1 Threat model refresh

Create or update a threat model covering all runtime surfaces.

Output:

```text
docs/security/THREAT_MODEL.md
```

Cover attackers:

- unauthenticated external client using Eggress as an open proxy;
- malicious upstream proxy;
- malicious local user with config write access;
- malicious Python caller embedding Eggress;
- compromised reverse client/server;
- network observer of plaintext reverse control channel;
- attacker inducing resource exhaustion;
- attacker exploiting parser edge cases;
- attacker reading logs/metrics for secrets.

Acceptance:

- Threat model has assets, trust boundaries, entry points, and mitigations.

### 35.2 Open-proxy prevention policy

Audit all listener types for accidental external exposure.

Listener types:

- standard TCP listeners;
- TLS listeners;
- SOCKS/HTTP mixed listeners;
- Shadowsocks listeners;
- transparent listeners;
- Unix sockets;
- UDP listeners;
- reverse external listeners;
- Python-created listeners.

Tasks:

- Define default warning/error behavior for non-loopback bind addresses without auth.
- Add config validation warnings or hard errors depending on mode.
- Add `--allow-open-listener` or equivalent only if needed and explicit.
- Ensure Python `Server` examples default to `127.0.0.1`.
- Ensure CLI diagnostics warn about `0.0.0.0`/`::` binds.

Acceptance:

- Accidental open-proxy exposure is visible and test-covered.

### 35.3 Credential redaction audit

Audit all credential-bearing paths.

Sources:

- URI userinfo;
- Shadowsocks passwords;
- Trojan passwords;
- reverse auth;
- system proxy credentials;
- generated TOML;
- rollback files;
- Python exceptions;
- JSON diagnostics;
- logs/tracing spans;
- metrics labels;
- admin snapshots;
- test failure output.

Tasks:

- Add a central redaction helper if not already canonical.
- Replace ad hoc formatting with the canonical helper.
- Add tests for all credential-bearing schemes.
- Add snapshot tests for JSON diagnostics/admin output.
- Ensure no metrics label contains raw URI/userinfo/passwords.

Acceptance:

- Redaction is uniform and regression-tested.

### 35.4 Admin and metrics exposure hardening

Review admin server and metrics endpoint.

Tasks:

- Default admin bind should be loopback-only.
- Add warnings/errors for non-loopback admin binds without auth or explicit allow flag.
- Verify admin snapshots do not leak credentials.
- Add optional auth or document why admin must be protected externally.
- Ensure `/metrics` does not include high-cardinality secrets or unbounded labels.
- Add tests for admin redaction and bind warnings.

Acceptance:

- Admin exposure cannot be accidentally internet-facing without diagnostics.

### 35.5 Reverse/backward proxy containment

Reverse mode is a high-risk surface.

Tasks:

- Re-audit non-loopback `external_bind` validation.
- Enforce auth+allowlist for non-loopback external listeners.
- Review max control connections, max streams, max pending external clients.
- Add rate limiting/backoff for failed auth if practical.
- Ensure plaintext auth warning is emitted in docs and diagnostics.
- Ensure reverse metrics/admin output redacts auth and targets as appropriate.
- Add tests for malicious client churn and failed-auth resource cleanup.

Acceptance:

- Reverse mode has conservative defaults and abuse limits.

### 35.6 Transparent proxy privilege boundary

Transparent proxying requires platform privileges and kernel behavior.

Tasks:

- Review transparent listener startup behavior when capabilities are missing.
- Decide whether fallback to normal listener is safe or should be opt-in; a transparent config falling back to normal listener may be surprising.
- Add explicit diagnostics for fallback behavior.
- Ensure original destination lookup failures do not leak or proxy to unintended targets.
- Add tests for route rejection with recovered destinations.
- Document required iptables/nftables setup and teardown.

Acceptance:

- Transparent proxy failure modes are explicit and safe.

### 35.7 Config validation for dangerous combinations

Add config validation rules for combinations that are likely unsafe.

Examples:

- non-loopback unauthenticated listener;
- admin non-loopback without auth;
- reverse non-loopback without allowlist;
- system proxy apply using non-loopback target without warning;
- transparent listener with unsupported platform;
- raw tunnel listener without fixed target;
- UDP listener with unbounded flow table;
- extremely high connection limits without explicit override.

Acceptance:

- Dangerous combinations either fail validation or emit structured warnings.

### 35.8 Resource exhaustion controls

Audit limits and backpressure.

Resources:

- TCP connections;
- UDP flows;
- reverse control connections;
- reverse pending externals;
- tasks;
- buffers;
- H2/WebSocket/raw protocol crates if retained;
- admin request size;
- Python start/stop churn.

Tasks:

- Ensure defaults are bounded.
- Add tests for limits.
- Add metrics for limit rejections.
- Document operational tuning.

Acceptance:

- Every network-facing path has bounded resource controls or explicit rationale.

### 35.9 Parser/fuzz/property security tests

Extend fuzz/property testing for untrusted inputs.

Targets:

- pproxy URI parser;
- CLI args translator;
- SOCKS codecs;
- HTTP CONNECT parser;
- Shadowsocks parser/framing;
- Trojan handshake parser;
- reverse URI parser;
- config TOML parser;
- Python wrapper diagnostics if feasible.

Tasks:

- Add fuzz targets or property tests.
- Add corpus seeds from compatibility fixtures.
- Add CI/manual policy for fuzz runs.

Acceptance:

- Parser crash resistance is covered by at least property tests and optional fuzz targets.

### 35.10 Dependency and supply-chain review

Run and document dependency checks.

Tasks:

- `cargo deny check`;
- `cargo audit`;
- Python dependency audit if applicable;
- maturin/build dependency review;
- workflow permission review;
- release artifact secret scan;
- license compatibility check.

Acceptance:

- Supply-chain findings are documented and triaged.

### 35.11 Security documentation updates

Update:

```text
docs/SECURITY_REVIEW.md
docs/security/THREAT_MODEL.md
docs/security/HARDENING_GUIDE.md
docs/security/OPEN_PROXY_PREVENTION.md
docs/security/REVERSE_SECURITY.md
docs/security/REDACTION_POLICY.md
docs/OPERATIONS.md
README.md
```

Acceptance:

- Users can understand safe deployment defaults and dangerous options.

### 35.12 Manifest/evidence updates

Add manifest entries for security claims:

```text
security_open_proxy_warning
security_admin_bind_warning
security_secret_redaction
security_reverse_allowlist
security_transparent_capability_diagnostics
security_resource_limits
security_parser_property_tests
security_dependency_audit
```

Evidence levels should be synthetic unless external security review exists.

Acceptance:

- Security claims are tracked and test-backed.

## Validation commands

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p eggress-testkit manifest
cargo deny check
cargo audit
```

Optional fuzz/security:

```bash
cargo fuzz list
cargo fuzz run pproxy_uri -- -max_total_time=60
cargo fuzz run socks_codec -- -max_total_time=60
```

Python:

```bash
maturin develop
python -m pytest python/tests/test_pproxy_diagnostics.py -q
python -m pytest python/tests/test_security_examples.py -q
```

## Acceptance criteria

Phase 35 is complete when:

- Threat model is updated.
- Open-proxy prevention policy exists and is tested.
- Credential redaction is audited and test-covered.
- Admin/metrics exposure is hardened.
- Reverse mode containment is revalidated.
- Transparent privilege boundary is explicit.
- Dangerous config combinations are validated or warned.
- Resource exhaustion limits are documented and tested.
- Parser/property/fuzz security coverage exists.
- Dependency/supply-chain checks are run and triaged.

## Handoff notes

This phase should be conservative. Security hardening should prefer explicit opt-in, clear diagnostics, and bounded defaults over pproxy convenience parity where the two conflict.
