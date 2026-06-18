# Interoperability Tests

These tests verify eggress's compatibility with external implementations.

## Dependencies

- **curl**: Required for curl-based tests. Install via system package manager.
- **Python pproxy** (optional): For pproxy-based cross-implementation tests.

## Running

```bash
# All tests (skips unavailable tools)
cargo test --test interoperability_curl
cargo test --test interoperability_pproxy

# CI installs dependencies explicitly
```

## Test Matrix

| Test | eggress role | External tool | Protocol |
|------|-------------|---------------|----------|
| curl HTTP CONNECT | server | curl | HTTP |
| curl SOCKS5 | server | curl | SOCKS5 |
| curl SOCKS4a | server | curl | SOCKS4 |
| pproxy HTTP | client | pproxy | HTTP |
| pproxy SOCKS5 | client | pproxy | SOCKS5 |
| pproxy SOCKS5 (eggress server) | server | curl | SOCKS5 |
| pproxy HTTP (eggress server) | server | curl | HTTP |
