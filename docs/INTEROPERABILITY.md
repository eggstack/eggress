# Interoperability tests

> Authoritative inventory: `docs/TESTING.md` (suite list), `docs/DIFFERENTIAL_TESTING.md`
> (gates and prerequisites), `docs/CI_STATUS.md` (policy). This file is a
> pointer only.

These tests verify Egress compatibility with external implementations. The
integration tests live with the CLI crate in `crates/eggress-cli/tests/`
(`interoperability_curl`, `interoperability_pproxy`,
`interoperability_shadowsocks`, `interoperability_trojan`,
`advanced_transport_interop`, plus gated `differential_pproxy`,
`pproxy_differential`, and `oracle`).

## Dependencies

- **curl**: required for curl-based tests.
- **Python pproxy** (optional): cross-implementation tests pin `pproxy==2.7.9`.

## Running

```bash
cargo test -p eggress-cli --test interoperability_curl
cargo test -p eggress-cli --test interoperability_pproxy
```

Gated suites require their `EGRESS_REQUIRE_*` variables (oracle resolved from
`$EGRESS_ORACLE_PYTHON`, then `$EGRESS_PYTHON_BIN`, then discovery):

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
```

Unavailable external tools are skipped unless the relevant environment gate
is enabled. See `docs/TESTING.md` and `docs/DIFFERENTIAL_TESTING.md` for the
opt-in interoperability and oracle commands.
