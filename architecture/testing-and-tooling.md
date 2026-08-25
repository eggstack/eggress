# Testing, Benchmarking, Fuzzing, and Oracle Tooling

Verification infrastructure spans five crates/directories plus CI policy.
Local fast tests are the default; external-oracle work is opt-in.

## eggress-testkit (`crates/eggress-testkit`)

Test-only library consumed as dev-dependency: echo/half-close servers,
temporary port allocation, oracle interpreter resolution
(`$EGRESS_ORACLE_PYTHON` → legacy var → discovery), parity manifests, corpora,
and differential harness plumbing.

## fuzz/ (standalone cargo workspace — NOT covered by workspace commands)

11 libfuzzer targets mapping to every bounded parser:
socks5_handshake · socks5_udp_datagram · http_connect_response ·
trojan_request · trojan_accept · route_match · uri_parse · shadowsocks_frame ·
toml_config · websocket_handshake · h2_connect_authority.
Check with: `cargo check --manifest-path fuzz/Cargo.toml --bins`

## benches/ (root package `eggress-bench`, Criterion)

route_match (decide latency scenarios) · tcp_relay (1K/64K throughput) ·
udp_relay (codec encode/decode) · http_connect_upstream (open/auth/407).

## scripts/

Grouped: strict pproxy probes (API surface, cipher KATs, wire formats, plugin/
process lifecycle), interop runners (shadowsocks, UDP, trojan, curl),
certification (`run_pproxy_certification.sh`), evidence builders/validators,
wheel/release smoke, perf (`perf/run_local_baseline.sh`, soak).

## Python-side tests

- `tests/compat/`: API-contract validation against extracted pproxy 2.7.9
  contract + fixtures (URI corpus, CLI cases, snapshots) + regression
  injection modules proving the differential harness catches mutations.
- `python/tests/`: six-tier taxonomy (unit/contract/differential/interop/
  platform/certification). `pytest.ini` forces `--import-mode=importlib` so
  the source tree cannot shadow the installed wheel's compiled extension.

## Frozen oracle assets

`compat/pproxy-2.7.9/`: pinned provenance (git tag, commit), hashes, known
defects, baselines, observations. Treat as immutable reference data.

## CI (three workflows, no more without project decision)

ci.yml (Rust fmt/clippy/test) · python-test.yml (path-scoped wheel smoke) ·
publish-python.yml (v* tag → 5-platform wheels → PyPI via protected env).
Policy docs: `docs/CI_STATUS.md`, `docs/TESTING.md`.

Also here: `Containerfile` (distroless runtime image, ports 8080/1080/9090).
