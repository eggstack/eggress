# Final pproxy closure scenarios

Phase 10 consolidates the existing focused tests into a compact final closure
surface. It is not a hosted-CI gate and retains no traces or evidence bundles.
The active manifest and this matrix are the current claim; older strict
manifests and phase reports are provenance only.

## Final claim

Eggress provides broad pproxy 2.7.9 compatibility for documented HTTP/SOCKS,
modern encrypted-proxy, routing, CLI, UDP, reverse, optional SSH/QUIC, and
Python workflows, subject to the documented feature and platform boundaries.
The deliberate exclusions are macOS PF original-destination recovery and the
four unavailable legacy cipher names `cast5-cfb`, `idea-cfb`, `rc2-cfb`, and
`seed-cfb`.

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
- composed SOCKS5/Shadowsocks UDP chains with local echo fixtures;
- raw backward channels, repeated +in, reconnect, and one HTTP/SOCKS5 jump;
- fixed-target/raw and promoted advanced transport paths where fixtures apply.

Modern Shadowsocks closure requires both directions against pproxy for all four
AEAD methods (`aes-128-gcm`, `aes-192-gcm`, `aes-256-gcm`, and
`chacha20-ietf-poly1305`), plus maintained Shadowsocks interoperability. Run:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
  cargo test -p eggress-cli --test interoperability_pproxy -- --ignored --test-threads=1 shadowsocks
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 \
  cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
```

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

- `test_wheel_import_smoke.py`: installed `eggress` plus the opt-in
  `eggress-pproxy-compat` distribution and top-level `pproxy` imports, package
  metadata, and known unsupported diagnostics;
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

The complete installed-wheel contract is run from the built wheel, not the
source checkout:

```bash
python -m pytest --import-mode=importlib \
  python/tests/test_pproxy_phase4_contract.py \
  python/tests/test_wheel_import_smoke.py \
  python/tests/test_proxy_connection.py \
  python/tests/test_server_lifecycle.py -q
python -m pproxy --version
```

Optional tail evidence is feature-specific: `ssh` uses the local OpenSSH
fixture, `quic` uses the QUIC/H3 interop tests, `legacy-crypto` uses its KAT,
fragmentation, OTA, and PacketCipher tests, and `pproxy-daemon` uses the Linux
feature tests. macOS PF remains an explicit intentional non-parity decision;
its capability probe and ADR are the evidence, not a skipped pass.

External tests that cannot run because their oracle or maintained
implementation is unavailable are recorded as unavailable and do not promote
a status to `matched`.
