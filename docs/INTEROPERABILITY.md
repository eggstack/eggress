# Interoperability tests

These tests verify Egress compatibility with external implementations. The
integration tests live with the CLI crate in `crates/eggress-cli/tests/`.

## Dependencies

- **curl**: required for curl-based tests.
- **Python pproxy** (optional): cross-implementation tests pin `pproxy==2.7.9`.

## Running

```bash
cargo test -p eggress-cli --test interoperability_curl
cargo test -p eggress-cli --test interoperability_pproxy
```

Unavailable external tools are skipped unless the relevant environment gate
is enabled. See `docs/TESTING.md` and `docs/DIFFERENTIAL_TESTING.md` for the
opt-in interoperability and oracle commands.
