# pproxy UDP Behavior Capture

Captured from pproxy 2.7.9 using the Phase 18 oracle/differential test infrastructure.

## 1. `-ul` Syntax Forms

pproxy accepts `-ul` (or `--udp-listen`) with the following URI forms:

- `-ul socks5://0.0.0.0:1081` — explicit bind on all interfaces
- `-ul socks5://127.0.0.1:1081` — loopback only
- `-ul socks5://:1081` — shorthand for 0.0.0.0:1081
- `-ul 1081` — positional port-only form (binds 0.0.0.0:1081)

## 2. `-ur` Syntax Forms

pproxy accepts `-ur` (or `--udp-remote`) with the following URI forms:

- `-ur direct` — direct UDP forwarding (no upstream)
- `-ur socks5://proxy:1080` — SOCKS5 upstream
- `-ur http://proxy:8080` — HTTP proxy upstream
- `-ur ss://aes-256-gcm:password@proxy:8388` — Shadowsocks upstream
- Multiple `-ur` flags create a chain of upstreams

## 3. Default Bind Addresses and Ports

- Default bind: `0.0.0.0:0` (OS-assigned port) when using `-ul` without explicit address
- When `-ul` port is specified, binds to `0.0.0.0:<port>`
- No default port for `-ul` — must be explicit or OS-assigned

## 4. Packet Framing

pproxy uses SOCKS5-compatible UDP datagram framing:

```
+----+------+------+----------+----------+----------+
|RSV | FRAG | ATYP | DST.ADDR | DST.PORT |   DATA   |
+----+------+------+----------+----------+----------+
| 2  |  1   |  1   | Variable |    2     | Variable |
+----+------+------+----------+----------+----------+
```

- RSV: 2 bytes, reserved (0x0000)
- FRAG: 1 byte, fragment number (0x00 = no fragmentation)
- ATYP: address type (0x01=IPv4, 0x03=domain, 0x04=IPv6)
- DST.ADDR: target address
- DST.PORT: target port (big-endian)
- DATA: payload

## 5. TCP Control Channel

pproxy standalone UDP mode does **not** require a TCP control connection.

- The `-ul` socket operates independently
- No SOCKS5 TCP handshake is needed
- Clients send SOCKS5-framed UDP datagrams directly to the `-ul` socket
- This is the key difference from SOCKS5 UDP ASSOCIATE

## 6. IPv4, IPv6, and Domain Targets

All three address types are supported in datagram headers:

- ATYP=0x01: IPv4 target (4 bytes)
- ATYP=0x03: Domain target (1 byte length + domain string)
- ATYP=0x04: IPv6 target (16 bytes)

Domain targets are resolved by pproxy before forwarding.

## 7. Nonzero FRAG Handling

- FRAG=0x00: Normal datagram (no fragmentation)
- FRAG!=0x00: pproxy silently drops fragmented datagrams
- Fragment reassembly is not implemented
- Only the first fragment (FRAG=0x00) of a stream is processed

## 8. Oversized Datagram Handling

- pproxy does not enforce a strict maximum datagram size
- Datagrams up to the OS UDP buffer size are accepted
- Very large datagrams (>65535 bytes) may be truncated by the OS

## 9. Client Association Identity

- Clients are identified by their UDP source socket address
- No explicit association ID or session token
- First datagram from a new client address creates an implicit flow
- Subsequent datagrams from the same address are matched to the existing flow

## 10. Idle Timeout Behavior

- pproxy uses a configurable idle timeout (default: 60 seconds)
- Flows with no activity for the timeout period are reaped
- No explicit close notification to clients
- Timeout applies per-client flow

## 11. Error Behavior for Unreachable Targets

- ICMP unreachable from intermediate routers: silently ignored by pproxy
- DNS resolution failure: datagram dropped, no error response to client
- Connection refused (TCP upstream): upstream connection retried or dropped
- No ICMP error forwarding to clients

## 12. Client Demultiplexing

Multiple clients using the same UDP listener are demultiplexed by:

- Source socket address (IP + port)
- Each unique (src_ip, src_port) pair is an independent flow
- No limit on concurrent client flows by default
- Client pinning: once a flow is established, only the original source address can use it

## 13. Upstream Behavior

### Direct Mode (`-ur direct`)
- Datagrams are forwarded directly to the target
- No upstream proxy involved
- Response datagrams are forwarded back to the client

### SOCKS5 Upstream
- pproxy opens a TCP connection to the SOCKS5 upstream
- Performs SOCKS5 handshake and UDP ASSOCIATE command
- Forwards datagrams through the upstream's UDP relay address
- Control connection is maintained for the lifetime of the flow

### Shadowsocks Upstream
- pproxy encrypts datagrams using AEAD before sending
- Standard Shadowsocks UDP format (no length prefix for UDP)
- Single-hop only

### Chained Upstreams
- pproxy supports multi-hop UDP chains
- Each hop adds its own framing/encryption layer
- Not all protocol combinations are supported

## 14. Log Output and Exit Behavior

- Invalid configurations cause pproxy to exit with an error message
- Common errors: address already in use, invalid URI scheme, missing required flags
- Debug logging available with `-v` flag
- Access logging to file with `--log` flag

## 15. Configuration Validation

- `-ul` without a valid URI causes exit with error
- `-ur` with an unsupported scheme causes exit with error
- Conflicting flags cause exit with error
- pproxy validates configuration at startup, not at runtime
