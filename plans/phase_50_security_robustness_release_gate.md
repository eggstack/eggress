# Phase 50: security and robustness release gate

## Goal

Close the release-blocking security and robustness gaps before final parity certification. Eggress is a proxy framework with multiple parsers, authentication modes, TLS wrapping, routing rules, UDP relays, Python bindings, and optional system integration. The release gate must prove that common failure and abuse cases are bounded, observable, and safe by default.

## Current known gaps to audit

Prior reviews identified these areas as incomplete or needing release-grade proof:

- per-source connection and authentication failure limits;
- auth-failure rate limiting;
- proxy-loop detection;
- private-network egress policy;
- DNS policy and DNS rebinding-aware routing;
- secret handling and zeroization review;
- unsafe-code audit confirmation;
- fuzz corpus expansion;
- soak/resource-exhaustion tests;
- security disclosure process;
- Python shutdown/GIL/lifecycle safety;
- UDP amplification controls and edge limits.

## Workstream A: abuse limits

Implement or verify:

- per-source connection limits;
- per-listener and global active connection limits;
- authentication failure counters and lockout/backoff;
- handshake timeout enforcement for all protocols;
- max header/handshake sizes;
- UDP per-client and global association limits;
- datagram size and amplification controls.

Add metrics for every dropped/limited condition.

## Workstream B: egress safety policy

Add or verify policy controls for:

- deny private/reserved IP ranges by default or explicit configuration;
- allowlist/denylist CIDR rules;
- DNS resolution policy;
- DNS rebinding-aware routing decisions;
- loop detection when upstream routes back into local listeners;
- transparent proxy special cases.

If default-deny private egress is too disruptive for pproxy compatibility, keep default-compatible behavior but expose secure presets and document the tradeoff.

## Workstream C: parser and protocol fuzzing

Expand fuzz coverage for:

- pproxy URI parser;
- chain parser;
- HTTP CONNECT parser;
- SOCKS4/4a parser;
- SOCKS5 negotiation and UDP datagram parser;
- Shadowsocks frame parser;
- Trojan parser if server support exists;
- TOML config compiler;
- rulefile parser;
- Python-facing redaction helpers if feasible.

Add seed corpora from real pproxy fixtures and generated edge cases.

## Workstream D: secret handling

Audit all credential surfaces:

- URI parsing structs;
- generated TOML;
- diagnostics;
- Python reprs and exceptions;
- logs/tracing;
- metrics labels;
- admin API output;
- parity reports and test fixtures.

Add tests that credentials do not appear in:

- `Debug`/`Display` outputs;
- Python `repr()`;
- CLI `pproxy check --json`;
- validation errors;
- route explanations;
- generated docs/artifacts.

Use zeroization for long-lived secrets where practical and document where not practical.

## Workstream E: unsafe and dependency audit

Confirm:

- workspace lint still forbids unsafe where intended;
- any exceptions are documented;
- `cargo audit` or equivalent runs;
- license compatibility is checked;
- dependency bans still apply;
- Python packaging dependencies are audited.

## Workstream F: soak and resource tests

Add gated tests for:

- many short TCP connections;
- slowloris-style handshakes;
- auth failure bursts;
- UDP association churn;
- route reload under load;
- graceful shutdown under active connections;
- Python start/shutdown loop;
- memory ceiling smoke on low-power targets if feasible.

These can be ignored/gated but must be documented and runnable.

## Workstream G: security docs

Create/update:

- `SECURITY.md` with disclosure process;
- `docs/security/THREAT_MODEL.md`;
- `docs/security/SECURE_CONFIGURATION.md`;
- `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md`;
- release notes security section.

Docs must call out differences between pproxy compatibility mode and hardened eggress native mode.

## Acceptance criteria

- Auth failure and connection abuse cases are rate-limited or explicitly documented.
- Credentials are redacted across CLI, Python, logs, metrics, diagnostics, and reports.
- Fuzz targets exist for high-risk parsers.
- Soak/resource tests exist and are documented.
- Security disclosure process exists.
- Manifest/report/docs do not overstate security properties.
- Python lifecycle tests cover repeated start/shutdown and shutdown-on-context-exit.

## Verification commands

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo audit
cargo deny check
cargo fuzz run uri_parse -- -max_total_time=60
cargo fuzz run socks5_udp_datagram -- -max_total_time=60
python -m pytest python/tests/test_pproxy_redaction.py -v
python -m pytest python/tests/test_pproxy_concurrency.py -v
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
```

Adjust fuzz commands to actual target names.

## Non-goals

- Do not make hardened security presets silently break pproxy compatibility mode.
- Do not claim formal verification.
- Do not block release on expensive soak tests if they are documented as scheduled/gated, unless a failure reveals a real bug.
