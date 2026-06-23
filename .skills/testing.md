# Testing Conventions

## When to use
Use when writing tests, debugging test failures, or understanding test infrastructure.

## Test layers

### Unit tests
In each crate's `src/` files. Test individual functions and types.

### Integration tests
In `crates/eggress-runtime/tests/`:
- `startup.rs` — listener bind, readiness, negative paths
- `routing.rs` — rule matching, fallback, direct routes
- `health.rs` — health state machine, probe reconciliation
- `admin.rs` — admin endpoints, route explanation
- `reload.rs` — config reload behavior
- `shutdown.rs` — graceful drain, force-cancel, admin-during-drain
- `pac_static.rs` — PAC generation, static content, reload freshness
- `udp.rs` — association lifecycle, echo, bind conflict
- `udp_upstream.rs` — SOCKS5 upstream relay, shutdown, metrics

### UDP-specific tests
- `crates/eggress-udp/tests/socks5_upstream.rs` — upstream relay scenarios
- `crates/eggress-runtime/tests/udp_upstream.rs` — runtime UDP upstream

### Interoperability tests
- `crates/eggress-cli/tests/interoperability_curl.rs` — curl-based
- `crates/eggress-cli/tests/interoperability_pproxy.rs` — pproxy-based

### CLI tests
- `crates/eggress-cli/tests/cli_tests.rs` — argument parsing
- `crates/eggress-cli/tests/reply_order.rs` — deferred success reply ordering

## Test utilities (`eggress-testkit`)
- Echo server, half-close server
- Temporary port allocator
- UDP echo server and SOCKS5 UDP test server (`testkit` module in `eggress-udp`)

## Running tests
```bash
# Full suite
cargo test --workspace

# Specific subsystem
cargo test -p eggress-runtime udp
cargo test -p eggress-udp socks5_upstream

# With output
cargo test --workspace -- --nocapture
```

## Writing new tests
- Use `#[tokio::test]` for async tests
- Use the testkit for server/client fixtures
- Use `tempfile` for config files
- Prefer integration tests over unit tests for behavioral coverage
- Test both success and failure paths
- Test negative paths (bind conflict, invalid config, oversized identity)
