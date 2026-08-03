# Optional pproxy closure scenarios

Phase 6 consolidates existing focused tests into a small optional closure
surface. It is not a hosted-CI gate and retains no traces or evidence bundles.

## Paired oracle scenarios

The reusable Rust harness in
`crates/eggress-cli/tests/pproxy_differential.rs` covers representative
`pproxy==2.7.9` comparisons:

- HTTP CONNECT and HTTP forward GET/POST;
- SOCKS4 and SOCKS4a domain targets;
- SOCKS5 CONNECT, authentication, domain target, refusal, and UDP ASSOCIATE;
- HTTP and SOCKS5 upstream chains, including authentication;
- first-available route selection and per-remote rule predicates;
- standalone UDP direct echo and one-hop SOCKS5 UDP relay;
- fixed-target/raw and promoted advanced transport paths where fixtures apply.

Run it with:

```bash
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 \
  cargo test -p eggress-cli --test pproxy_differential -- --ignored --test-threads=1
```

The older scenario-driven oracle test remains available for deeper diagnosis,
but is not required for routine development.

## Local public smoke scenarios

The clean-wheel and public API coverage is intentionally split across focused
tests rather than repeated in the oracle harness:

- `test_wheel_import_smoke.py`: installed `eggress` and top-level `pproxy`
  imports, package metadata, and known unsupported diagnostics;
- `test_proxy_connection.py`: `Connection.tcp_connect()` echo and lifecycle;
- `test_pproxy_public_namespace.py`: `Rule`, `DIRECT`, protocol/cipher exports,
  and UDP callback behavior;
- `test_server_lifecycle.py`: `Server` start, addresses, context management,
  reload, and close;
- `test_pproxy_route_through.py` and `test_pproxy_listener_behavior.py`:
  compatibility routing and listener workflows.

The Python wheel intentionally installs namespaces and library adapters only;
the `eggress` and compatibility `pproxy` executables remain Rust binaries
installed through Cargo. Clean-wheel smoke checks that boundary and verifies
the Cargo binaries separately.

Suggested local command:

```bash
python -m pytest python/tests/test_wheel_import_smoke.py \
  python/tests/test_proxy_connection.py \
  python/tests/test_pproxy_public_namespace.py \
  python/tests/test_server_lifecycle.py -q
```
