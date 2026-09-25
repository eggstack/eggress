# Phase 3 — Compatibility Bridge for Existing Native Transports

## Status

Complete. The compatibility bridge is implemented and verified through the
native config/compiler path, the workspace runtime tests, and the Python
compatibility smoke suite.

## Parent roadmap

`plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`

## Depends on

Phase 1 URI AST. Phase 2 may proceed in parallel.

## Objective

Make protocol and transport capabilities already implemented in the Eggress runtime reachable through pproxy-compatible URI, CLI, config translation, and Python helper paths.

This phase is adapter work. It must not create new protocol engines, replace working codecs, or broaden the transport scope beyond what current runtime crates already support.

## Current gap

The native runtime and README report support for:

- HTTP/2 CONNECT;
- WebSocket and WSS tunnels;
- raw/fixed-target TCP tunnels;
- tunnel aliases;
- stream-native chaining through these transports.

The compatibility URI parser recognizes some of these names, but translator allowlists and role mapping reject them or classify them as unknown. Capability documents therefore describe support that users cannot consistently reach through the `pproxy` command or compatibility Python helpers.

## In scope

- H2 upstream translation;
- H2 listener translation only if the native runtime currently has a stable listener role matching pproxy;
- WS and WSS upstream translation;
- WS/WSS listener translation only where current runtime support and pproxy role semantics align;
- raw and tunnel fixed-target upstream translation;
- any already-supported fixed-target listener role;
- chain composition through HTTP, SOCKS, H2, WS/WSS, raw, and tunnel where the runtime already supports the cell;
- auth, TLS, ALPN, path, and fixed-target metadata required by existing runtime configs;
- compatibility diagnostics for valid syntax that maps to an unsupported role;
- Python translation and outbound-connector helpers using the same generated config;
- correction of manifest and README status after wiring.

## Out of scope

- QUIC and HTTP/3;
- SSH;
- SSR or legacy Shadowsocks;
- new WebSocket plugin/obfuscation modes;
- MASQUE or CONNECT-UDP;
- new H2 server implementation if the native listener is incomplete;
- a second chain executor;
- transport-specific schedulers;
- adding composition cells that the runtime cannot presently execute.

## Required inventory before implementation

Create a concise table from current runtime code, not old manifests:

| Scheme | Listener role | Upstream role | TCP | UDP | TLS form | Fixed target | Chain over prior stream |
|---|---|---|---|---|---|---|---|
| `h2` | verify | verify | verify | no | ALPN/TLS as implemented | verify | verify |
| `ws` | verify | verify | yes | no | no | verify | verify |
| `wss` | verify | verify | yes | no | yes | verify | verify |
| `raw` | verify | verify | yes | no | wrapper-dependent | yes | verify |
| `tunnel` | alias | alias | yes | Phase 5 for UDP | wrapper-dependent | yes | verify |

Do not use a checkmark in the final table until the corresponding config path is demonstrated with a focused runtime test.

## Design constraints

### One translation path

All compatibility entry points must use the same lowering path:

```text
pproxy CLI / Python helper
        -> Phase 1 AST
        -> compatibility validation
        -> Eggress TOML/config objects
        -> existing runtime supervisor
```

Do not add Python-only or CLI-only transport construction.

### Role validation

Separate scheme recognition from role support.

Examples:

- a scheme may be valid as an upstream but invalid as a listener;
- a transport may be valid only with a fixed target;
- WSS requires TLS material and hostname/SNI data;
- H2 may require ALPN and a specific authority/path;
- raw/tunnel may not perform an application handshake and therefore require explicit destination metadata.

Return `unsupported_role` or `invalid_composition`, not `unknown_scheme`, for recognized but unavailable roles.

### Existing runtime config

Prefer emitting the runtime's existing canonical URI/config form. Do not add duplicate transport fields to the compatibility crate if the native config already represents them.

If the native config URI grammar cannot carry a pproxy field, add one narrowly scoped typed config field and use it for native and compatibility callers.

## Workstream 3.1 — Establish current runtime truth

The implementation inventory is:

| Scheme | Listener role | Upstream role | TCP | UDP | TLS form | Fixed target | Chain over prior stream |
|---|---|---|---|---|---|---|---|
| `h2` | unsupported role | supported | yes | no | implied `+tls`, H2 ALPN | no | yes |
| `ws` | unsupported role | supported | yes | no | optional `+tls` | no | yes |
| `wss` | unsupported role | supported | yes | no | implied `+tls` | no | yes |
| `raw` | unsupported role | supported | yes | no | wrapper-dependent | yes, endpoint form | yes |
| `tunnel` | unsupported role | supported alias | yes | no | wrapper-dependent | yes, endpoint form | yes |

The table reflects the current runtime handlers and config compiler, with
compatibility-specific role validation. Native listener topology for these
transport wrappers is intentionally not exposed by the pproxy translator.

1. Inspect transport crates, config validation, chain executor, and tests.
2. Populate the role/composition table.
3. Identify exact config forms that already pass tests.
4. Mark stale manifest entries that need temporary downgrade until wiring lands.
5. Confirm which transport cells require only translator changes versus a small config bridge.

No code should be written from the old parity matrix alone.

## Workstream 3.2 — Extend compatibility validation

1. Add H2, WS, WSS, raw, and tunnel to role-aware compatibility enums.
2. Consume fixed-target, path, TLS, auth, and modifier fields from the Phase 1 AST.
3. Validate required fields before TOML generation.
4. Reject UDP use except where current runtime supports it.
5. Reject listener roles not present in the runtime.
6. Preserve chain hop order and target ownership.

## Workstream 3.3 — Emit native config

For each supported cell:

1. Generate a minimal config fixture.
2. Parse it through the native config crate.
3. Start a local runtime with port `0` where applicable.
4. Route a TCP echo payload through the transport.
5. Verify shutdown and half-close behavior using existing stream adapters.

Keep generated config deterministic aside from ephemeral bind addresses.

## Workstream 3.4 — Chain wiring

Cover only representative cells that exercise distinct mechanisms:

- SOCKS5 -> WS -> target;
- HTTP -> WSS -> target;
- SOCKS5 -> H2 -> HTTP or target;
- HTTP -> raw/tunnel -> fixed target;
- one three-hop chain including one promoted transport.

Do not test every Cartesian combination. Existing chain validation should reject equivalent unsupported cells.

## Workstream 3.5 — Python compatibility helpers

Ensure these paths use the newly wired transports:

- `translate_pproxy_uri()`;
- `translate_pproxy_args()`;
- `check_pproxy_uri()` and diagnostics;
- `ProxyConnection` / outbound connector construction;
- Phase 4 top-level `pproxy.Connection` once available.

At this phase, the current `eggress` namespace helpers are sufficient for tests. Do not implement the top-level package here.

## Workstream 3.6 — Correct documentation and manifest entries

After tests pass:

- mark each role separately;
- distinguish native runtime support from compatibility syntax support;
- remove claims for listener roles that are not implemented;
- document required fixed-target/TLS syntax;
- retain QUIC/H3 as intentional non-parity.

## Acceptance criteria

Phase 3 is complete when:

- valid pproxy H2, WS/WSS, raw, and tunnel upstream URIs no longer fail as unknown schemes;
- each supported URI produces native config accepted by the config validator;
- each claimed transport completes one local TCP relay test;
- representative chains complete without a temporary local compatibility listener;
- TLS/SNI/ALPN requirements are correctly propagated for WSS and H2;
- fixed-target ownership is preserved for raw/tunnel;
- recognized but unsupported roles produce role-specific diagnostics;
- UDP use is rejected unless a runtime implementation exists;
- CLI and Python translation helpers produce the same config;
- no new protocol engine or chain executor is introduced;
- the parity manifest matches the resulting role table.

## Focused verification

Use the smallest applicable tests:

```bash
cargo fmt --check
cargo test -p eggress-pproxy-compat transport
cargo test -p eggress-runtime websocket
cargo test -p eggress-runtime h2
cargo test -p eggress-runtime chain
cargo test -p eggress-cli --test cli_tests pproxy_transport
python -m pytest python/tests/test_pproxy_compat.py -k "h2 or websocket or raw"
```

Run full workspace tests only if shared transport or chain code changes.

## Failure cases that require tests

- WSS without required TLS hostname/config;
- H2 with an invalid authority or missing required target;
- raw/tunnel without fixed destination where required;
- a promoted transport used as an unsupported listener;
- H2/WS/raw in `-ur` UDP position;
- unsupported transport inside a multi-hop chain;
- credentials appearing in diagnostics or generated explanation output.

## Rollback and compatibility notes

This phase should mainly convert previously rejected configurations into working ones. If a translation form conflicts with an existing Eggress extension, keep compatibility behavior scoped to pproxy entry points. Do not change native config interpretation to imitate pproxy syntax unless a shared typed field is clearly necessary.

## Handoff guidance

Implement one transport family at a time:

1. runtime truth table;
2. H2 translator wiring;
3. WS/WSS wiring;
4. raw/tunnel wiring;
5. representative chains;
6. Python helper and documentation alignment.

Do not start Phase 5 runtime feature work from this plan.
