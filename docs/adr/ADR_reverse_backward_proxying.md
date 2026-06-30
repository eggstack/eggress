# ADR: Reverse/Backward Proxying Protocol Model

| Field | Value |
|-------|-------|
| Status | Accepted |
| Date | Phase 27 |
| Decision makers | Eggress maintainers |
| Related | `docs/PPROXY_PARITY_SPEC.md`, `docs/PARITY_MATRIX.md` |

## Context

pproxy supports reverse proxying via `bind`, `listen`, and backward URI forms.
In a reverse proxy topology, the control client dials out to an acceptor, and
the acceptor dispatches incoming connections back to the control client for
handling. This inverts the usual forward-proxy relationship: the acceptor
publishes a listener endpoint, and remote clients connect to it, with traffic
tunneled back to the control client.

eggress must decide:

1. Whether to use a custom protocol or pproxy's wire-compatible reverse proxy
   format.
2. Whether to support this only in pproxy-compat mode or as an Eggress-native
   feature.
3. How to handle authentication, TLS, multiplexing, and reconnect semantics.
4. How to restrict what remote listeners can bind to (security boundary).

The reverse proxy model is common in scenarios where a machine behind NAT or a
firewall needs to expose services: the machine initiates an outbound control
connection, and the public-facing acceptor routes inbound connections back
through the control channel.

## Decision

### 1. pproxy Wire-Compatible Protocol

Use a simple length-prefixed frame protocol compatible with pproxy's reverse
proxy wire format. This ensures interoperability with existing pproxy
deployments where an eggress reverse client can connect to a pproxy acceptor
(or vice versa).

The wire format is:

```
Frame:
  [4 bytes: stream_id (big-endian u32)]
  [4 bytes: payload_length (big-endian u32)]
  [payload_length bytes: payload]
```

Stream ID `0` is reserved for control frames (listener registration, heartbeat,
authentication). Stream IDs `1..=N` carry proxied data streams.

### 2. Dual Mode

Support two operational modes:

- **pproxy-compat mode**: Wire-compatible with pproxy's reverse proxy. No TLS
  by default, optional password authentication. Intended for interop with
  existing pproxy deployments.
- **Eggress-native mode**: Stronger security defaults — TLS required,
  HMAC-based authentication, private network target restrictions. Intended for
  new deployments that want a more secure reverse proxy.

### 3. Authentication

- **Eggress-native mode**: Authentication is required by default. Uses
  HMAC-SHA256 over a shared secret, exchanged in the control handshake. The
  challenge-response prevents replay attacks.
- **pproxy-compat mode**: Password comparison is optional. When a password is
  configured, it is sent in the initial control frame in plaintext (matching
  pproxy behavior). Operators are warned in logs when using unauthenticated or
  plaintext-authenticated reverse connections.

### 4. TLS

- **Eggress-native mode**: TLS is enabled by default for control channels and
  data streams. Uses `eggress-transport-tls` (rustls, no OpenSSL).
- **pproxy-compat mode**: No TLS by default, matching pproxy behavior. TLS
  can be enabled explicitly via config.

### 5. Multiplexing

Length-prefixed frames carry a 32-bit stream ID, enabling multiplexed data
streams over a single control connection:

- **Max concurrent streams**: Configurable, default 256.
- **Max frame size**: Configurable, default 64KB.
- **Flow control**: Not implemented in v1. Backpressure is applied by refusing
  new streams when the limit is reached. A full implementation is deferred to
  a future phase.

### 6. Reconnect Strategy

When the control connection drops, the reverse client reconnects with:

- **Exponential backoff**: Initial delay 1 second, max delay 30 seconds.
- **Jitter**: ±25% randomization on each backoff interval.
- **Re-register**: On successful reconnect, the client re-registers all
  listeners with the acceptor.
- **Configurable**: Backoff parameters are exposed in TOML config.

### 7. Remote Listener Authorization

By default, the acceptor restricts what bind addresses remote clients can
request:

- **Default**: Only loopback (`127.0.0.1`, `::1`) listeners are allowed.
- **Allowlist**: Operators explicitly configure allowed bind addresses/port
  ranges in TOML.
- **Reject**: Requests for bind addresses not in the allowlist are rejected
  with a structured diagnostic.
- **Prevents arbitrary bind**: A rogue or misconfigured reverse client cannot
  bind to `0.0.0.0` or arbitrary external addresses.

### 8. Config Model

New TOML configuration sections, separate from forward proxy config:

```toml
# Reverse listener (acceptor side)
[[reverse_listeners]]
bind = "0.0.0.0:8443"
protocol = "http"              # or "socks5", "socks4"
allow_bind = ["127.0.0.1", "::1"]
password = "secret"            # optional, pproxy-compat

# Reverse client (initiator side)
[[reverse_clients]]
remote = "example.com:8443"
password = "secret"
tls = true                     # Eggress-native default
backoff_initial_ms = 1000
backoff_max_ms = 30000
```

### 9. Lifecycle State Machine

The control channel follows a defined state machine:

```
Disconnected → Connecting → Authenticating → Registering → Ready
                                                         ↓
                                                   Draining → Closed
```

Reconnect from any failure state:

```
* → Reconnecting → Connecting → ...
```

State transitions are logged and exposed via metrics. The `Draining` state is
entered during graceful shutdown — existing streams complete, no new streams
are accepted.

### 10. Security Defaults

- **Eggress-native mode**: Denies reverse connections that target private
  network addresses (RFC 1918, RFC 4193, loopback) unless explicitly allowed
  in config. Logged and rejected with a structured diagnostic.
- **pproxy-compat mode**: Logs warnings when private network targets are
  requested but does not block by default (matching pproxy permissive
  behavior).

## Consequences

### Positive

- **pproxy interoperability**: eggress reverse clients can connect to pproxy
  acceptors and vice versa, enabling incremental migration.
- **Clean security model**: Eggress-native mode provides strong defaults
  (TLS, HMAC auth, private-net restrictions) without requiring operator
  expertise.
- **Testable**: The wire format is simple enough for property-based testing
  and differential testing against pproxy.
- **Incremental adoption**: Dual mode lets operators start with pproxy-compat
  and migrate to Eggress-native at their own pace.

### Negative

- **Two modes add complexity**: The codebase must handle both pproxy-compat
  and Eggress-native behaviors, including different auth, TLS, and security
  enforcement paths.
- **Wire protocol is not standard**: The length-prefixed frame format is
  compatible with pproxy but is not an RFC or widely adopted standard.
  Interoperability is limited to pproxy-compatible implementations.
- **No flow control in v1**: The initial multiplexing implementation lacks
  per-stream flow control. Backpressure is coarse (reject new streams).
  This is acceptable for v1 but limits high-throughput scenarios.

### Mitigations

- **Mode-specific tests**: Each mode has dedicated unit and integration tests.
  pproxy-compat mode is tested against pproxy via differential tests.
- **Clear documentation**: Mode differences are documented in config reference
  and operations guide.
- **Deprecation path**: pproxy-compat mode may be deprecated in a future
  phase if pproxy interop is no longer needed. The state machine and wire
  format abstraction make this a contained change.

## Alternatives Considered

### 1. Eggress-Native Only (No pproxy Compat)

Design a new reverse proxy protocol from scratch with no pproxy wire
compatibility.

**Rejected because**: This would prevent interop with existing pproxy
deployments, which is a core goal of the pproxy parity effort. Operators
with mixed eggress/pproxy fleets need a migration path.

### 2. pproxy-Compat Only (No Native Mode)

Only implement pproxy-compatible reverse proxy behavior, with no enhanced
security defaults.

**Rejected because**: pproxy's reverse proxy has weak security defaults
(no TLS, optional auth, no private-net restrictions). Eggress should provide
a more secure option for new deployments while maintaining compat for
migration.

### 3. Standard Protocol (SOCKS5 Bind / CONNECT-UDP)

Use an established protocol like SOCKS5 BIND or RFC 9298 CONNECT-UDP for
reverse proxying.

**Rejected because**: Neither SOCKS5 BIND nor CONNECT-UDP is implemented by
pproxy for reverse proxying. This would not achieve pproxy interop. These
protocols could be added as additional transport options in a future phase.

### 4. Control Channel Over WebSocket

Transport the reverse proxy control channel over WebSocket for NAT traversal
benefits.

**Rejected because**: pproxy does not use WebSocket for its reverse proxy
control channel. This would not be wire-compatible. WebSocket transport is
already supported as a general transport layer and could be composed with
reverse proxying externally if needed.

## References

- `docs/PPROXY_PARITY_SPEC.md` — Tier taxonomy and parity decisions
- `docs/PARITY_MATRIX.md` — Feature parity tracking
- `docs/protocols/` — Protocol specifications
- `crates/eggress-transport-tls/` — TLS transport layer (rustls-based)
- [pproxy reverse proxy documentation](https://github.com/windprophet/pproxy)
