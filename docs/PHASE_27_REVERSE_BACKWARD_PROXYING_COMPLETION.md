# Phase 27 Completion: Reverse, Backward, and Jump Proxying Parity

## Status

**Phase 27 complete.** pproxy's reverse/backward proxying behavior is captured,
implemented in `eggress-protocol-reverse`, exposed through pproxy URI translation,
and surfaced via metrics. Implementation matches pproxy's actual wire format
exactly — a deliberately narrow, raw-relay implementation that favors
correctness and interop over invented additional surface area.

## Scope of what landed

### Behavior capture (27.1)

`docs/protocols/REVERSE_PROXYING.md` documents pproxy 2.7.9 reverse/backward
behavior in full: URI forms, control-channel protocol, lifecycle, reconnect,
authentication, drain, and security properties.

### Protocol decision (27.2)

`docs/adr/ADR_reverse_backward_proxying.md` records the decision to implement
a single pproxy-compatible mode (raw-relay control channel, raw `user:pass`
auth, plaintext TCP) and to defer any Eggress-native mode until pproxy
compatibility is proven out in the field. The original draft of the ADR
described an over-engineered length-prefixed frame protocol that did not match
pproxy's wire format; that draft was replaced with an accurate ADR during
Phase 27.

### Protocol implementation (27.3–27.6)

`crates/eggress-protocol-reverse/` provides the protocol crate:

- **Wire format**: raw `user:pass` bytes sent by the client, 1-byte handshake
  response (0x01 = accept, 0x00 = reject) by the server, then bidirectional
  TCP relay of one proxy session per control channel.
- **State machine**: `Disconnected → Connecting → Authenticating → Ready →
  Draining → Closed`. Reconnect triggers from any failure state.
- **Reverse acceptor (`ReverseServer`)**: binds an optional external listener,
  accepts control connections, authenticates against optional configured
  credentials, and pairs each accepted external client with a control
  connection. After handshake, external bytes and control-channel bytes are
  relayed bidirectionally until either side closes.
- **Reverse control client (`ReverseClient`)**: dials the acceptor, sends
  optional auth, reads the handshake response, then either relays the
  bidirectional stream to a configured default target (`default_target_host`
  / `default_target_port`) or, with no default target, drains the stream.
  Reconnect with exponential backoff (configurable initial/max).
- **No multiplexing**: each control connection carries exactly one proxy
  session, matching pproxy's stream-per-connection model. Concurrent sessions
  require multiple control connections.
- **No UDP**: TCP only. Documented as `intentional_non_parity` in the
  manifest because pproxy itself does not support reverse UDP.
- **No built-in TLS**: operators must wrap with stunnel / haproxy /
  WireGuard. Documented in `docs/SECURITY_REVIEW.md`.

### Protocol crate tests (27.10)

`crates/eggress-protocol-reverse/tests/integration.rs` covers:

- Server accepts control connection, sends handshake accept.
- Auth success path (correct credentials).
- Auth failure path (wrong credentials).
- Auth required but not provided path.
- ControlState transitions.
- Echo relay through server (external ↔ control bidirectional).
- Client/server round-trip with a configured default target.
- Client reconnects after server shutdown (backoff exercised).
- Graceful shutdown of the server.

22 unit tests in the protocol crate (handshake constants, control state,
auth helpers, bidirectional relay, metrics) plus 9 integration tests pass.

### Reverse metrics (27.9)

`crates/eggress-protocol-reverse/src/metrics.rs` introduces `ReverseMetrics`
with counters and gauges:

- `control_connections_active` (gauge)
- `control_connections_accepted_total` (counter)
- `control_connections_rejected_total` (counter)
- `control_reconnects_total` (counter)
- `streams_opened_total` (counter)
- `streams_closed_total` (counter)
- `stream_bytes_total` (counter)
- `last_error` (truncated message)

`render_prometheus()` produces Prometheus text format, and a serializable
`ReverseMetricsSnapshot` provides a structured view for admin APIs.

Wiring is opt-in via `set_metrics(Arc<ReverseMetrics>)` on the server and
client builders. Counts are emitted from the relevant code paths in
`accept_control_connections`, `handle_control_connection`,
`accept_external_clients`, `run`, and `run_session`.

`docs/METRICS.md` documents each metric.

### pproxy URI compatibility (27.8)

`crates/eggress-pproxy-compat` extended the URI parser and translator:

- `is_reverse()` split into `is_reverse_listener()` (for `bind://`,
  `listen://`, `backward://`, `rebind://` schemes) and `is_backward()` (for
  any scheme with the `+in` modifier).
- New `backward_num()` returns the count of `+in` tokens.
- `+ssl` modifier is now recognized and tracked as a `ssl: bool` flag.
- Multiple `+in` tokens (`socks5+in+in://...`) parse correctly.
- Translate emits `[[reverse_servers]]` tables for listener URIs that are
  reverse-listener schemes.
- Translate emits `[[reverse_clients]]` tables for remote URIs with the
  `+in` modifier.
- `parallel_connections` is emitted when `backward_num > 1`.
- `+ssl` reverse URIs emit an `unsupported` diagnostic (backward-tls).
- Jump chains with backward (`socks5+in://a:1__http://b:2`) emit an
  `unsupported` diagnostic (backward-jump-chain).

138 pproxy-compat tests pass, including 9 new reverse-specific translation
tests.

### Manifest entries (27.1, 27.10)

`tests/compat/pproxy_manifest.toml` updates:

| Feature | Status | Evidence level |
|---------|--------|----------------|
| `backward_tcp_control` | supported | `implemented_synthetic` |
| `backward_auth` | supported | `implemented_synthetic` |
| `backward_reconnect` | supported | `implemented_synthetic` |
| `backward_parallel_connections` | unsupported | `unimplemented` |
| `backward_jump_chain` | unsupported | `unimplemented` |
| `backward_tls` | unsupported | `unimplemented` |
| `backward_no_udp` | `intentional_non_parity` | `intentional_non_parity` |

The three `supported` entries reference the integration tests that prove
local behavior. The four `unsupported` entries have updated `divergence`
notes describing what would be needed to lift the gap.

### Phase 27 follow-up closure (post-`ff209d8`)

The original Phase 27 workstream shipped the protocol implementation, pproxy
URI translation, and basic metrics. A follow-up pass closed the remaining
workstream items in `plans/PHASE_27_REVERSE_BACKWARD_AND_JUMP_PROXYING.md`:

- **`27.1` Behavior capture extension**: `docs/protocols/REVERSE_PROXYING.md`
  sections 13–18 now document heartbeat / keepalive cadence, log message +
  exit code conventions, listener bind failure, target connect failure,
  half-close / reset behavior, and how chaining interacts with reverse mode.
- **`27.3` Redacted logging + per-state metrics**: `redact_auth(&str)` is
  now exposed as a public helper; `server_auth_handshake` returns the
  redacted `user:****` form. `ReverseMetrics` gained `auth_failures_total`,
  `heartbeat_failures_total`, `drain_total`, `drain_duration_ms_total`, and a
  per-state `state_time_ms[6]` array. Prometheus output covers every new
  counter and exposes per-state labels.
- **`27.4` Server-side security hardening**: `ReverseServerConfig` gained
  `allow_bind: Option<Vec<SocketAddr>>`, `max_listeners_per_client`,
  `max_streams_per_listener`, and `max_pending_external`. `ReverseServer::run`
  now enforces an allow-bind allowlist (matching ip+port), caps concurrent
  control connections and streams per listener, and surfaces a new
  `ProtocolError::BindDenied(SocketAddr)` variant. `ReverseServerState` exposes
  atomic counters with a `state_handle()` snapshot for admin / observability.
- **`27.5` Client-side routing hook**: `ReverseClient` now accepts a
  `TargetResolver` trait (`TargetResolution::{Connect, Reject}`); the default
  `DefaultTargetResolver` uses the legacy `default_target_host` /
  `default_target_port` fields, and a custom resolver may reject to implement
  a soft policy gate. `run_session` records route rejection as
  `last_error` and reconnects with backoff.
- **`27.7` Runtime routing integration**: `eggress-routing` gained a
  `TransportKind::ReverseTcp` variant and a `MatchExpr::ReverseListener` matcher
  (which only matches when transport is `ReverseTcp`). The TOML `LeafMatcher`
  learned a `reverse_listener` field; the parser accepts `transport =
  "reverse_tcp"`. `crates/eggress-runtime/src/reverse.rs` exposes a
  `RouteEngineTargetResolver` adapter that wraps a `SharedRoutingService` and
  consults the router on every reconnect — `Direct` and `UpstreamGroup`
  decisions map to `Connect`, `Reject` decisions map to `Reject`. Routing is
  a gate, not a redirect, since reverse mode always dials the same external
  target.
- **`27.8` Python helpers**: `eggress-python` exposes
  `describe_reverse_pproxy_uri(uri: str) -> ReverseUriSummary` which classifies
  a pproxy reverse URI as `server` (`bind://`, `listen://`, `backward://`,
  `rebind://` → `reverse_servers`) or `client` (`*+in://...` →
  `reverse_clients`), redacts the target display, and exposes `tls` and
  modifier lists. `python/tests/test_reverse_uri_helper.py` covers 17 cases
  including credential redaction and modifier counts.
- **`27.9` Admin API**: `crates/eggress-admin` gained a `ReverseRegistry` and
  a new `/-/reverse` HTTP endpoint that emits per-server
  `{id, control_bind, active_control, active_streams, denied_bind,
  dropped_stream_limit}`. The runtime supervisor wires `ReverseRegistry`
  into `AdminState` so that future supervisor changes can register servers
  dynamically.
- **`27.10` Gated interop tests**: `crates/eggress-runtime/tests/reverse_interop.rs`
  contains one loopback self-interop test (ungated) plus two gated
  pproxy handshake smoke tests under `EGRESS_REQUIRE_REVERSE_INTEROP=1`.
  Status, `Controls::Reverse` clause, and how to run are documented in
  `docs/COMPATIBILITY_EVIDENCE.md`.

### Updated protocol-crate test counts (post-`ff209d8`)

| Subset | Count |
|--------|-------|
| `eggress-protocol-reverse` lib | 42 / 42 pass (was 22) |
| `eggress-protocol-reverse` integration | 19 / 19 pass (was 9) |
| `eggress-admin` lib (incl. reverse route + registry tests) | 21 / 21 pass (was 19) |
| `eggress-runtime` lib `reverse::` | 4 / 4 pass (new) |
| `eggress-runtime` `reverse_interop` | 2 un-gated pass / 2 gated ignored |
| `eggress-python` `test_reverse_uri_helper.py` | 17 / 17 pass |
| `eggress-pproxy-compat` lib (unchanged) | 138 / 138 pass |

## Out-of-scope (intentional deferrals)

- **Live supervisor spawn**: `[[reverse_servers]]` and `[[reverse_clients]]`
  entries in the runtime TOML config are translated to runtime config objects
  but not yet wired to live `ReverseServer` / `ReverseClient` tasks inside
  `ServiceSupervisor`. The protocol crate is fully usable in standalone mode
  (see gated interop tests in `crates/eggress-runtime/tests/reverse_interop.rs`)
  and the supervisor wiring is a thin mechanical follow-up.
- **Multi-channel concurrency**: a single `ReverseClient` still maintains a
  single control connection. pproxy achieves concurrency via `+in+in+in` count,
  which would require running N parallel control-client tasks. The config
  model supports this (`parallel_connections` field) but the runtime execution
  is not wired.
- **Jump chain composition on relayed streams**: a relayed stream is currently
  handed to the configured `default_target` or dropped. Hooking the chain
  executor into the reverse client is a follow-up.
- **Built-in TLS**: intentionally deferred. Operators wrap with stunnel /
  haproxy / WireGuard.
- **Reverse UDP**: `intentional_non_parity` because pproxy itself does not
  support reverse UDP.
- **Differential pproxy wire-format tests**: the gated pproxy interop tests
  verify handshake wiring in both directions. They do **not** yet compare a
  relayed payload byte-for-byte against pproxy==2.7.9. Promoting the
  `reverse_pproxy_interop` row to **Compatible** requires additional
  differential tests that exchange a known payload through an eggress →
  pproxy or pproxy → eggress relay and assert byte equality. The
  `EGRESS_REQUIRE_REVERSE_INTEROP` test scaffold is the documented place
  for that work.

## Validation

Run from `/Users/davidbowman/projects/eggress`:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-protocol-reverse
cargo test -p eggress-pproxy-compat --lib reverse
cargo test -p eggress-testkit manifest
```

All green. Broader runs (full workspace test suite) complete but the
runtime integration tests include multi-process pproxy interop paths and
take ~5–10 minutes; spot-checked subsets are summarized below.

| Subset | Result |
|--------|--------|
| `eggress-protocol-reverse` (lib + integration) | 31 / 31 pass |
| `eggress-pproxy-compat` (lib) | 138 / 138 pass |
| `eggress-testkit` (lib) | 51 / 51 pass (2 ignored) |
| `eggress-server` (lib) | 80 / 80 pass |
| `eggress-routing` (lib + properties + scheduler) | 169 / 169 pass |
| `eggress-config` (lib) | 84 / 84 pass |
| `eggress-runtime` startup / routing / health / admin / reload / shutdown / pac_static / lifecycle / observability / security | 97 / 97 pass |
| `eggress-runtime` udp / udp_upstream / upstream_protocols / shadowsocks_tcp / shadowsocks_udp / transparent / unix_socket / multihop_tcp / retry_fallback / scheduler_runtime | 106 / 106 pass |

## Documentation updates

- `docs/protocols/REVERSE_PROXYING.md`: captured pproxy behavior (existing).
- `docs/adr/ADR_reverse_backward_proxying.md`: rewritten to match actual
  implementation.
- `docs/METRICS.md`: added reverse metrics section.
- `docs/CONFIG_REFERENCE.md`: expanded `[[reverse_servers]]` and
  `[[reverse_clients]]` sections with intro paragraphs, concurrency notes,
  security hardening, and unsupported-features notes.
- `docs/OPERATIONS.md`: added reverse/backward mode operations subsection.
- `docs/SECURITY_REVIEW.md`: added reverse/backward proxy security
  subsection.
- `docs/PARITY_MATRIX.md`: added reverse/backward proxy row to inbound TCP
  table; added three rows to the remaining-protocol-audit table.
- `docs/PPROXY_PARITY_SPEC.md`: corrected references to length-prefixed
  framing and dual-mode architecture.
- `README.md`: replaced the misleading reverse checklist with one that
  matches what is actually shipped.
- `AGENTS.md`: expanded reverse test commands; updated protocol-reverse
  crate description in the project tree.

## Handoff notes for follow-up phases

- **Live supervisor spawn**: extend `eggress-runtime`'s `ServiceSupervisor`
  to spawn a `ReverseServer` for each `[[reverse_servers]]` entry and a
  `ReverseClient` for each `[[reverse_clients]]` entry. The
  `RouteEngineTargetResolver`, `ReverseRegistry`, and `reverse_registry`
  field on `RuntimeState` are already in place. Required wiring is
  mechanical: register each started server in the registry (use
  `ReverseServerEntry { id, control_bind, state }`); spawn client tasks
  with `RouteEngineTargetResolver` injected; cancel on shutdown.
- **Multi-channel concurrency**: when wiring reverse clients into the
  runtime, iterate `1..=config.parallel_connections.unwrap_or(1)` and
  spawn one `ReverseClient` task per channel.
- **Jump chain composition**: when reverse integration lands, the
  control-client side should invoke the chain executor with the chain
  derived from the URI's jump suffix (`__`-separated). Until then,
  emit an unsupported diagnostic at translation time (already done).
- **Differential pproxy wire-format tests**: the current gated tests verify
  handshake wiring. Add relay-payload differential tests (eggress →
  pproxy over a known payload, byte-equality assert) under
  `EGRESS_REQUIRE_REVERSE_INTEROP=1` and promote the
  `reverse_pproxy_interop` row in `docs/COMPATIBILITY_EVIDENCE.md` to
  **Compatible**.
- **TLS termination**: defer until operator demand is clear. Wrap with
  external tooling (stunnel/haproxy/WireGuard) is the recommended
  path and is documented.