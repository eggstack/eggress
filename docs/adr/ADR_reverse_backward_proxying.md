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

eggress must decide how to implement this feature to achieve pproxy parity while
keeping the implementation honest and simple.

The reverse proxy model is common in scenarios where a machine behind NAT or a
firewall needs to expose services: the machine initiates an outbound control
connection, and the public-facing acceptor routes inbound connections back
through the control channel.

## Decision

### 1. pproxy Wire-Compatible Protocol

Use pproxy's actual wire protocol: raw auth bytes, a 1-byte handshake, then
raw TCP relay. No length-prefixed framing, no stream multiplexing, no HMAC
authentication.

The wire format is:

| Phase | Direction | Content | Notes |
|-------|-----------|---------|-------|
| Auth | Client → Server | Raw `user:pass` bytes | No length prefix, no type byte |
| Handshake | Server → Client | 1 byte: `0x00` = reject, else = accept | If `0x00`, connection closes |
| Relay | Bidirectional | Raw TCP bytes | No framing, no multiplexing |

**There is no stream multiplexing.** Each backward connection carries exactly
one proxy session. If you need concurrent sessions, use multiple control
channels (matching pproxy's `+in` count).

### 2. Simplified Model (pproxy-Only, No Native Mode)

The implementation matches pproxy's behavior exactly. There is no
"Eggress-native mode" with enhanced security defaults. This decision was made
because:

- The pproxy wire format is already simple and well-defined.
- A separate native mode would double the code surface with no clear user benefit.
- Operators who need TLS or stronger auth can wrap the control channel with
  stunnel or use `+ssl` on the pproxy side.

### 3. Authentication

Authentication is optional and uses raw `user:pass` bytes sent as a single
write with no framing:

- **Server side**: If configured, the server reads auth bytes, parses
  `user:pass`, compares against configured credentials, and sends `0x01`
  (accept) or `0x00` (reject). If no auth is configured, the server sends
  `0x01` immediately.
- **Client side**: If auth is configured, the client sends `user:pass` raw
  bytes before reading the handshake response. If no auth, the client reads
  the 1-byte handshake directly.
- **No challenge-response**: Same auth bytes accepted on every reconnect.
  Plaintext by default.

### 4. TLS

No built-in TLS. The control channel is plaintext TCP by default, matching
pproxy behavior. Operators can wrap with stunnel, nginx, or use `+ssl` on the
pproxy side for encryption.

### 5. Connection Model

Each control channel carries exactly one proxy session:

1. Control client connects to acceptor and authenticates.
2. Acceptor accepts an external client connection.
3. Acceptor relays the external client's stream through the control channel
   (bidirectional raw TCP relay).
4. Control client performs proxy operation (SOCKS5 CONNECT, HTTP, etc.) on
   behalf of the external client.
5. When the external client disconnects, the control channel connection closes.
6. Control client reconnects (with backoff on error, immediate on normal close).

For concurrent sessions, the client opens N parallel control connections
(matching pproxy's `+in` token count). Each connection carries one session.

### 6. Reconnect Strategy

When the control connection drops, the client reconnects with exponential
backoff:

- **Initial delay**: Configurable (default 1 second).
- **Growth**: Doubles each attempt, capped at configurable max (default 30
  seconds).
- **Reset on success**: Backoff resets to initial delay after a successful
  session.
- **Immediate reconnect on normal close**: When the session ends normally
  (external client disconnected), the client reconnects immediately without
  backoff.

### 7. Security Posture

The implementation has known limitations:

| Property | Default | Notes |
|----------|---------|-------|
| Encryption | None | Control channel is plaintext TCP |
| Authentication | Optional | Raw `user:pass` bytes, compared as bytes |
| Auth transport | Plaintext | No challenge-response, no hashing |
| TLS | Not built-in | Use stunnel or `+ssl` wrapper |
| Private network | Not restricted | No ACL on target addresses |

**Recommended operator hardening**:
- Always configure authentication.
- Wrap control channel with TLS (stunnel, nginx, etc.).
- Use firewall rules to limit which clients can reach the acceptor.
- Restrict listener bind addresses to known interfaces.

### 8. Alternatives Considered

**Eggress-Native Only (No pproxy Compat)**
Rejected: prevents interop with existing pproxy deployments, which is a core
goal of the pproxy parity effort.

**pproxy-Compat Only with Enhanced Security Defaults**
Rejected: adds code complexity without clear benefit. Operators who need
stronger security can wrap the channel externally.

**Standard Protocol (SOCKS5 Bind / CONNECT-UDP)**
Rejected: pproxy does not use these for reverse proxying. Would not achieve
pproxy interop. Could be added as a separate feature in a future phase.

**Control Channel Over WebSocket**
Rejected: pproxy does not use WebSocket for reverse proxy control. Would not
be wire-compatible. WebSocket transport is already available as a general
transport layer.

**Length-Prefixed Frame Protocol with Multiplexing**
Rejected: does not match pproxy's actual wire format. Would break
interoperability. pproxy uses raw TCP relay after handshake, not framed
multiplexing.

## Consequences

### Positive

- **pproxy interoperability**: eggress reverse clients can connect to pproxy
  acceptors and vice versa.
- **Simplicity**: The wire format is minimal — raw bytes and a 1-byte
  handshake. Easy to test and reason about.
- **Low code surface**: No frame parser, no multiplexer, no HMAC. Fewer
  places for bugs.

### Negative

- **No encryption by default**: Control channel is plaintext. Operators must
  add TLS externally.
- **No multiplexing**: Each control channel carries one session. High
  concurrency requires multiple connections.
- **Weak auth**: Raw `user:pass` with no challenge-response or hashing.

### Mitigations

- **Documentation**: Security limitations and hardening recommendations are
  documented.
- **External TLS**: stunnel or nginx can wrap the control channel.
- **Multiple connections**: pproxy's `+in` token count provides a model for
  concurrent sessions.

## References

- `docs/protocols/REVERSE_PROXYING.md` — Wire format and behavior details
- `docs/PPROXY_PARITY_SPEC.md` — Tier taxonomy and parity decisions
- `docs/PARITY_MATRIX.md` — Feature parity tracking
- `crates/eggress-protocol-reverse/` — Implementation
- [pproxy reverse proxy documentation](https://github.com/windprophet/pproxy)
