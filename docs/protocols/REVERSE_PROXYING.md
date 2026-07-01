# Reverse/Backward Proxying in pproxy

> Status: Captured from pproxy 2.7.9 source analysis. Not yet implemented in Eggress.

## 1. Overview

Reverse proxying in pproxy inverts the normal proxy connection model. In forward
proxying, a client connects to the proxy and the proxy opens an outbound
connection to the target. In reverse/backward proxying, a **control client**
establishes an outbound connection to a **reverse acceptor**, and the acceptor
dispatches externally-accepted connections back through the control channel for
the control client to handle.

This enables a proxy server behind NAT to serve remote clients without port
forwarding. The control client initiates an outbound connection (which works
through NAT), and the acceptor routes inbound connections through that tunnel.

### Roles

| Role | Description | Initiates TCP? | Binds listener? |
|------|-------------|---------------|-----------------|
| **Reverse acceptor** | Exposes a public listener; accepts connections; relays them over the control channel | No (listens) | Yes |
| **Reverse control client** | Dials out to the acceptor; receives stream requests; connects to targets | Yes (dials out) | No |

### Key Differences from Forward Proxying

| Aspect | Forward Proxy | Reverse Proxy |
|--------|--------------|---------------|
| Connection initiator | End client | Control client (dials out) |
| Target resolution | Client specifies target | Control client connects to target on behalf of acceptor's clients |
| Listener location | On the proxy | On the acceptor |
| NAT traversal | Not needed | Outbound dial avoids NAT |
| Multiplexing | None (one connection per client) | Multiple streams over one control connection |
| Lifecycle | Per-connection | Long-lived control channel with reconnection |

## 2. pproxy URI Forms

### Reverse/Backward URI Schemes

pproxy uses the `+in` modifier on any protocol scheme to activate reverse
/backward mode:

```
scheme+in://[auth@]host:port
```

The `+in` suffix tells pproxy that the remote server (`-r` argument) should
connect outward to the specified endpoint, and that endpoint will send
connections back through the control channel.

Multiple `+in` modifiers stack to create parallel control connections:

```
scheme+in+in://host:port    # 2 parallel backward connections
scheme+in+in+in://host:port # 3 parallel backward connections
```

The count of `+in` tokens determines `backward_num` — the number of concurrent
outbound control connections maintained.

### URI Examples

```bash
# SOCKS5 backward: dial out to acceptor, serve SOCKS5 streams
pproxy -l http://:8080 -r socks5+in://acceptor:1080

# HTTP backward with auth
pproxy -l socks5://:1080 -r http+in://user:pass@acceptor:8080

# Two parallel backward connections
pproxy -l http://:8080 -r socks5+in+in://acceptor:1080

# Backward with jump chain (backward client + upstream)
pproxy -l http://:8080 -r socks5+in://acceptor:1080__http://upstream:8080

# Backward with Shadowsocks encryption
pproxy -l http://:8080 -r ss+in://aes-256-gcm:pass@acceptor:8388
```

### Jump Chains

Jump chains use the `__` separator to compose multiple hops. The chain is
parsed right-to-left: the rightmost URI is the final destination, and each
leftward hop wraps the rightward hop.

```
socks5://hop1:1080__http://hop2:8080__direct://
```

When combined with `+in`, the backward client dials the acceptor, and accepted
streams are forwarded through the remaining jump hops:

```
socks5+in://acceptor:1080__http://upstream:8080__direct://
```

This means: dial out to `acceptor:1080`, receive streams, forward them through
`upstream:8080` via HTTP, then connect directly to the target.

## 3. Control Channel Protocol

pproxy's reverse control protocol is minimal. It is not a framed multiplexing
protocol like SSH or HTTP/2. Instead, it uses a simple byte-exchange handshake
followed by raw TCP relay.

### Connection Establishment

```
Control Client                    Acceptor
     |                               |
     |  TCP connect to acceptor      |
     |------------------------------>|
     |                               |
     |  Send auth credentials        |
     |  (raw bytes, no framing)      |
     |------------------------------>|
     |                               |
     |  Acceptor reads auth          |
     |  Sends 1-byte response        |
     |  (0x00 = reject, else = ok)   |
     |<------------------------------|
     |                               |
     |  If accepted:                 |
     |  Raw TCP bidirectional relay  |
     |<----------------------------->|
     |  (stream carries proxy proto) |
```

### Post-Handshake Behavior

Once the 1-byte handshake succeeds, the control channel becomes a raw TCP
tunnel. The acceptor runs the standard `stream_handler` for each accepted
connection, meaning the control client receives connections that look like
normal inbound proxy requests.

The control client does not parse the proxy protocol on the relayed stream — it
passes the raw reader/writer pair to its handler, which performs protocol
detection (SOCKS5, HTTP, etc.) just as if the connection arrived on a local
listener.

### Wire Format Summary

| Phase | Direction | Content | Notes |
|-------|-----------|---------|-------|
| Auth | Client → Server | Raw auth bytes | No length prefix, no type byte |
| Handshake | Server → Client | 1 byte: 0x00 = reject, else = accept | If 0x00, connection closes |
| Relay | Bidirectional | Raw TCP bytes | No framing, no multiplexing |

**There is no stream multiplexing.** Each backward connection carries exactly
one proxy session. If you need concurrent sessions, use multiple `+in` tokens
to create parallel control connections.

## 4. Authentication

Authentication is optional and specified in the URI fragment:

```
socks5+in://user:pass@acceptor:1080
```

The auth bytes are the raw `user:pass` string (URL-encoded by the URI parser).
The acceptor compares these bytes against its configured credentials.

### Auth Flow

```
Control Client                         Acceptor
     |                                    |
     |  Connect to acceptor               |
     |----------------------------------->|
     |                                    |
     |  Send: b"user:pass" (raw bytes)    |
     |----------------------------------->|
     |                                    |
     |  Acceptor reads len(auth) bytes    |
     |  Compares with configured auth     |
     |  If match: send b'\x01'            |
     |  If no match: send b'\x00'         |
     |<-----------------------------------|
     |                                    |
     |  If b'\x00': connection closed     |
     |  If b'\x01': relay begins          |
```

### Auth Characteristics

| Property | Behavior |
|----------|----------|
| Required? | Optional (no auth = send empty bytes) |
| Challenge-response? | No — simple comparison |
| Encrypted? | No (plaintext unless `+ssl` is used) |
| Per-connection? | Yes — each backward connection authenticates independently |
| Re-auth on reconnect? | Yes — auth is sent on every new control connection |

## 5. Lifecycle

### Normal Operation

```
Control Client                          Acceptor
     |                                      |
     |  1. Connect to acceptor              |
     |------------------------------------->|
     |                                      |
     |  2. Send auth credentials            |
     |------------------------------------->|
     |                                      |
     |  3. Receive handshake (0x01 = ok)    |
     |<-------------------------------------|
     |                                      |
     |  4. Control channel established      |
     |     (raw TCP relay active)           |
     |                                      |
     |  5. Acceptor accepts external client |
     |     <--- external client connects    |
     |                                      |
     |  6. Acceptor relays client stream    |
     |     through control channel          |
     |<------------------------------------>|
     |                                      |
     |  7. Control client performs proxy     |
     |     operation (SOCKS5 CONNECT, etc.) |
     |     on behalf of external client     |
     |                                      |
     |  8. When external client disconnects,|
     |     control channel connection closes|
     |                                      |
     |  9. Control client reconnects        |
     |     (step 1)                         |
     |------------------------------------->|
```

### State Machine

```
  Disconnected
       |
       | open_connection()
       v
  Connecting ----timeout/error----> Disconnected
       |
       | TCP connected
       v
  Authenticating ----auth fail----> Disconnected
       |
       | auth ok (0x01)
       v
  Ready --------relay active------> Ready
       |                               |
       | connection closes             | connection closes
       v                               v
  Disconnected                    Disconnected
       |                               |
       | reconnect (backoff)           | reconnect (backoff)
       v                               v
  Connecting                    Connecting
```

### Stream-per-Connection Model

pproxy does **not** multiplex streams over a single control connection. Each
backward connection carries exactly one proxy session. When that session ends,
the control connection closes, and the backward client must reconnect.

To handle concurrent clients, the backward client maintains N parallel control
connections (determined by the count of `+in` tokens in the URI).

```
External Client A  ──┐
                      ├──> Acceptor ──> Backward Client (connection 1)
External Client B  ──┤
                      ├──> Acceptor ──> Backward Client (connection 2)
External Client C  ──┘
```

If all N control connections are busy, additional external clients must wait
until a control connection becomes available.

## 6. Reconnect Behavior

When a control connection closes (either side), the backward client automatically
reconnects with exponential backoff.

### Backoff Algorithm

```python
errwait = 0
while not self.closed:
    try:
        reader, writer = await asyncio.wait_for(
            self.backward.open_connection(...),
            timeout=60
        )
        # ... handle connection ...
        errwait = 0  # reset on success
    except Exception:
        await asyncio.sleep(errwait)
        errwait = min(errwait * 1.3 + 0.1, 30)  # cap at 30 seconds
```

| Reconnect # | Delay | Cumulative |
|-------------|-------|------------|
| 1 | 0.0s | 0.0s |
| 2 | 0.1s | 0.1s |
| 3 | 0.23s | 0.33s |
| 4 | 0.4s | 0.73s |
| 5 | 0.62s | 1.35s |
| 6 | 0.9s | 2.25s |
| 7 | 1.27s | 3.52s |
| 8 | 1.75s | 5.27s |
| 9 | 2.38s | 7.65s |
| 10 | 3.19s | 10.84s |
| 11+ | ... | ... |
| Final | 30.0s (capped) | grows linearly |

### Reconnect Triggers

| Event | Behavior |
|-------|----------|
| Control channel EOF | Reconnect with backoff |
| Control channel reset | Reconnect with backoff |
| Acceptor restart | Reconnect with backoff |
| Auth failure | Connection closed; reconnect with backoff |
| Timeout (60s read) | Connection closed; reconnect with backoff |
| Normal close (external client done) | Reconnect immediately (errwait stays 0) |
| Acceptor sends 0x00 | Connection closed; reconnect with backoff |

### Re-registration

On reconnect, the backward client re-authenticates and the control channel is
re-established. There is no explicit listener re-registration — the acceptor
accepts the new control connection and resumes dispatching.

## 7. Close and Drain Behavior

### External Client Disconnects

When the external client closes its connection:

1. The acceptor detects the close on the client-facing socket
2. The acceptor closes the control channel connection
3. The backward client detects the close
4. The backward client reconnects (step 1 of lifecycle)

### Control Client Disconnects

When the control client loses connectivity or shuts down:

1. The acceptor detects the close on the control channel
2. Any in-flight data is dropped
3. The acceptor closes the listener (or the specific stream)
4. External clients that were being served receive a connection reset

### Acceptor Restart

When the acceptor restarts:

1. All control connections are severed
2. All bound listeners are closed
3. External clients receive connection refused
4. The backward client reconnects once the acceptor is back
5. On reconnect, the control channel is re-established
6. External clients can connect again

**Note:** There is no explicit listener persistence across acceptor restarts.
The backward client must reconnect and the acceptor must accept the new control
connection before external clients can connect again.

## 8. Security Considerations

| Property | Default | Notes |
|----------|---------|-------|
| Encryption | None | Control channel is plaintext TCP |
| Authentication | Optional | Raw `user:pass` in URI, compared as bytes |
| Auth transport | Plaintext | No challenge-response, no hashing |
| TLS | Available via `+ssl` | Not default |
| Listener bind | Unrestricted | Acceptor binds any address the OS allows |
| Private network | Not restricted | No ACL on target addresses |
| Stream limit | Unbounded | No per-control limit on concurrent sessions |
| Replay resistance | None | Same auth bytes accepted on every reconnect |

### Recommendations for Production

- Use `+ssl` to encrypt the control channel
- Always configure authentication
- Restrict listener bind addresses to known interfaces
- Use firewall rules to limit which clients can reach the acceptor
- Monitor control connection count for anomalies

## 9. UDP Support

pproxy does **not** support UDP reverse proxying. Only TCP streams are
multiplexed over the control channel.

The `+in` modifier only applies to TCP protocols. UDP listeners (`-ul`) operate
independently and cannot be served through a backward control channel.

## 10. Differences from Standard Reverse Proxies

| Property | Standard Reverse Proxy (nginx, etc.) | pproxy Backward |
|----------|--------------------------------------|-----------------|
| Protocol | HTTP/HTTPS | Any proxy protocol (SOCKS5, HTTP, Shadowsocks, etc.) |
| Connection model | Connection-per-request | Long-lived control channel |
| Multiplexing | HTTP/2 or separate connections | One session per control connection; parallel connections via `+in` count |
| Target specification | In request headers (Host, URI) | Control client resolves and connects |
| X-Forwarded-For | Added | Not added |
| Health checks | Active/passive probing | Reconnect with backoff |
| TLS termination | On the proxy | On the control channel (optional) |
| Load balancing | Multiple backends | Multiple `+in` connections to same backend |
| Session affinity | Not applicable | N/A — each session is independent |

## 11. Implementation Notes for Eggress

### Phase 27 Scope

This behavior capture informs Phase 27 implementation. Key design decisions:

1. **Wire compatibility**: Eggress should implement a compatible wire protocol
   to interoperate with existing pproxy backward clients/servers.

2. **Security defaults**: Eggress should require authentication and support
   TLS by default, even if pproxy does not.

3. **Lifecycle management**: The reconnect backoff and reconnection logic must
   handle acceptor restarts, network partitions, and auth failures.

4. **Stream limits**: Eggress should enforce maximum concurrent streams per
   control connection and maximum control connections per listener.

5. **Metrics**: Control channel state, reconnect count, stream count, and
   auth failures should be exposed.

### Manifest Entries

The following features are captured but not implemented:

| Feature ID | Category | Status | Notes |
|-----------|----------|--------|-------|
| `backward_tcp_control` | protocol | unimplemented | Backward control channel TCP relay |
| `backward_auth` | security | unimplemented | Simple auth on backward channel |
| `backward_reconnect` | platform | unimplemented | Exponential backoff reconnect |
| `backward_parallel` | protocol | unimplemented | Multiple `+in` connections |
| `backward_jump_chain` | protocol | unimplemented | Backward + jump chain composition |
| `backward_tls` | transport | unimplemented | TLS-wrapped backward channel |
| `backward_udp` | udp | intentional_non_parity | pproxy does not support UDP backward |

## 12. Example Configurations

### Minimal Backward Server

```bash
# Acceptor (public-facing):
pproxy -l http://:8080

# Control client (behind NAT):
pproxy -l http://:9090 -r http+in://acceptor.example.com:8080
```

### Authenticated Backward with Jump

```bash
# Acceptor:
pproxy -l http://:8080

# Control client with auth and upstream:
pproxy -l http://:9090 \
  -r http+in://secret@acceptor.example.com:8080__socks5://upstream:1080
```

### Multiple Parallel Connections

```bash
# 3 parallel backward connections for higher concurrency:
pproxy -l http://:9090 \
  -r http+in+in+in://acceptor.example.com:8080
```

### Shadowsocks-Encrypted Backward

```bash
# Encrypted control channel:
pproxy -l http://:9090 \
  -r ss+in://aes-256-gcm:secret@acceptor.example.com:8388
```

## References

- pproxy source: `server.py` — `ProxyBackward` class
- Phase 27 plan: `plans/PHASE_27_REVERSE_BACKWARD_AND_JUMP_PROXYING.md`
- Parity spec: `docs/PPROXY_PARITY_SPEC.md`
- Parity matrix: `docs/PARITY_MATRIX.md`

## 13. Heartbeat and Keepalive

pproxy does **not** implement an explicit application-layer heartbeat on the
backward control channel. Keepalive is delegated entirely to the TCP transport:

| Mechanism | Behavior |
|-----------|----------|
| TCP keepalive | Relies on OS defaults; no application-level ping |
| Read timeout | 60-second `asyncio.wait_for` on the initial control channel read |
| Idle timeout | None — connection stays open until either side closes |
| Application-level ping | Not implemented |
| Application-level pong | Not implemented |

If the underlying TCP connection survives but no traffic flows (for example,
because the acceptor's `stream_handler` is idle), neither side times out. The
control channel remains open indefinitely. pproxy's 60-second timeout only
fires during the initial read of the auth bytes, not during the relay phase.

The Eggress implementation matches pproxy's behavior: no application-level
heartbeat, 60-second initial read timeout, and otherwise relies on TCP
keepalive. Operators requiring active liveness detection must use a separate
monitoring path.

## 14. Log Messages and Exit Codes

pproxy uses Python's `logging` module at `INFO` level. The relevant log
messages emitted during reverse proxy operation:

| Event | Log message |
|-------|-------------|
| Backward client started | `client connect to ssl://...` (when +ssl) or `client connect to ...` (plaintext) |
| Control connection established | `connected to <host>:<port>` |
| Auth failure | `client <host>:<port> login failed` |
| Listener bind failure | `bind on <addr> failed: <reason>` |
| External client connect | `connection from <peer>` (from `stream_handler`) |
| Reconnect | `reconnect after <n> second(s)` |
| Listener closed | `client closed` (from accept loop) |
| Fatal error | `proxy: <error>` then exit with code 1 |

pproxy exit codes:

| Code | Meaning |
|------|---------|
| 0 | Normal shutdown (SIGTERM/SIGINT) |
| 1 | Fatal error (bind failure, unexpected exception) |
| 130 | SIGINT (Ctrl-C) |
| 143 | SIGTERM |

Eggress uses `tracing` at `INFO`/`DEBUG`/`WARN` levels. The following
messages are emitted:

| Event | Tracing event |
|-------|---------------|
| Server started | `info! reverse server listening for control connections` |
| External listener bound | `info! reverse server listening for external clients` |
| Control connection accepted | `info! control connection accepted` |
| Auth failure | `warn! control connection auth failed` |
| External client relay started | `info! relaying external client through control connection` |
| Reconnect | `warn! session failed, reconnecting` |
| Shutdown | `info! reverse server shutting down` |

Credentials are never logged. Peer addresses are logged for diagnostics.

## 15. Listener Bind Failure

When the acceptor cannot bind the requested listener address, pproxy exits
with code 1 and the error `bind on <addr> failed: <reason>`. Common causes:

| Cause | Example pproxy log |
|-------|--------------------|
| Address already in use | `bind on 0.0.0.0:8080 failed: [Errno 98] Address already in use` |
| Permission denied | `bind on 0.0.0.0:80 failed: [Errno 13] Permission denied` |
| Hostname resolution failure | `bind on 0.0.0.0:8080 failed: [Errno -2] Name or service not known` |
| Address family mismatch | `bind on 0.0.0.0:8080 failed: [Errno 97] Address family not supported` |

The Eggress implementation surfaces the same `std::io::Error` from the
`TcpListener::bind` call and propagates it up through `ReverseServer::run`.
The error message is included in `last_error` of `ReverseMetrics` and the
server task exits with the error.

## 16. Target Connect Failure

When the control client (backward client) cannot connect to its configured
default target, pproxy closes the control channel. The acceptor detects the
close and discards the in-flight stream.

| Cause | pproxy behavior |
|-------|-----------------|
| Connection refused | Close control channel; external client receives EOF |
| DNS resolution failure | Close control channel; external client receives EOF |
| Timeout | Close control channel; external client receives EOF |
| Auth required by target | Close control channel; external client receives EOF |

The Eggress implementation records the error in `last_error` and
`control_reconnects_total`, then reconnects the control channel with
exponential backoff. External clients receive a connection reset on the
listener side.

## 17. Half-Close and Reset

| Event | pproxy behavior | Eggress behavior |
|-------|-----------------|-------------------|
| External client half-closes (shutdown write) | Acceptor half-closes control channel | `relay_bidirectional` exits when either side reads EOF |
| Control client half-closes | Acceptor half-closes listener side | `relay_bidirectional` exits when either side reads EOF |
| TCP RST | Connection terminates immediately | Connection terminates immediately |
| Idle (no traffic) | Connection remains open | Connection remains open until either side closes |

The Eggress implementation uses bidirectional copy and treats half-close and
full-close identically. When either side's read returns 0 bytes, the relay
function returns and both streams are dropped, which causes the other side to
see EOF. This matches pproxy's `asyncio.streams` behavior.

## 18. Chaining with Reverse Mode

pproxy supports jump chains (`__` separator) combined with the `+in` modifier:

```
socks5+in://acceptor:1080__http://upstream:8080__direct://
```

Parsing: the `+in` modifier is on the **leftmost** URI. The control client
dials `acceptor:1080`, and accepted streams are forwarded through the
remaining hops: `upstream:8080` via HTTP, then `direct://` to the target.

The Eggress implementation does not support jump chains through reverse
streams. When such a URI is translated via `pproxy translate`, the
configuration emits an `unsupported` diagnostic for `backward-jump-chain`.
Each URI in a chain must be a separate `-r` argument instead.
