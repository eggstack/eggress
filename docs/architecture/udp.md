# eggress-udp

`crates/eggress-udp/`

UDP association management, relay, and upstream forwarding.

## Entry Modes

| Mode | Description |
|---|---|
| `Socks5UdpAssociate` | SOCKS5 UDP ASSOCIATE from inbound SOCKS5 clients |
| `StandalonePproxyUdp` | Standalone pproxy-compatible UDP relay |
| `ShadowsocksUdp` | Shadowsocks UDP packet relay |
| `Echo` | Explicit UDP echo listener |
| `FixedTarget` | Explicit fixed-target UDP listener |

## Key Types

| Type | Description |
|---|---|
| `UdpAssociation` | Association state machine owned by TCP control connection |
| `UdpAssociationRegistry` | Bounded association tracking with global/per-listener limits |
| `UdpTargetFlow` | Connected UDP socket per target for reliable response demux |
| `UdpLimits` | Configurable limits: associations, datagrams, idle timeouts |
| `UdpMetrics` | Prometheus counters and gauges |
| `UdpCapability` | Classifies chains as UDP-supported or unsupported |
| `UdpMode` | Enum of UDP entry modes |

## Target Flow Model

Each unique target address gets its own `UdpTargetFlow`:
- Connected UDP socket (one per target)
- Simplified response demultiplexing
- Client address pinning
- Bounded by `max_targets_per_association`
- Idle target flows reaped by `target_idle_timeout`

## Upstream Relay

UDP upstreams are built from a closed set of composable hop codecs:

1. Encode the innermost destination hop first.
2. Wrap each preceding hop around that datagram.
3. Send through the first hop's UDP transport.
4. Decode the response in outer-to-inner order.

The supported runtime hop set is SOCKS5 UDP and standard Shadowsocks UDP. Each
SOCKS5 hop owns a bounded TCP UDP-ASSOCIATE control session; Shadowsocks hops
use their configured AEAD packet codec. The protocol crate also exposes a
feature-gated `legacy-crypto` PacketCipher codec for pproxy compatibility
vectors and standalone integration; it is not selected by native routing.
Domain, IPv4, and IPv6 destination
metadata remains inside the innermost frame.

For a one-hop SOCKS5 upstream, the pipeline is:
1. Establish TCP control connection to upstream
2. SOCKS5 handshake + UDP ASSOCIATE
3. Per-target UDP association with upstream
4. Encode/decode SOCKS5 UDP datagrams

HTTP, SOCKS4, Trojan, H2, WebSocket, and mixed chains containing any of those
protocols are rejected for UDP (no silent fallback). QUIC-specific UDP
transport is also rejected because UDP-over-QUIC stream mapping is not in the
supported composition matrix. A composed chain is accepted only
when every hop has a real codec and the runtime can establish its UDP
transport.

## Bounded compatibility listener modes

The compatibility translator can configure `Echo` and `FixedTarget` as
standalone listener modes. A fixed-target listener always sends datagrams to
the configured target; it does not accept a destination from the client and it
does not add general multi-hop UDP support. These modes are configured
independently from the TCP listener field, so an explicit UDP listener cannot
erase or replace a TCP fixed target.

## Security

- Client address pinning enabled by default
- Multicast, broadcast, unspecified, port zero rejected
- Datagram size bounded
- Association and target-flow counts bounded
- Loopback bind by default

## Metrics

| Metric | Description |
|---|---|
| `eggress_udp_associations_active` | Active associations |
| `eggress_udp_packets_up_total` | Packets sent upstream |
| `eggress_udp_packets_down_total` | Packets received upstream |
| `eggress_udp_upstream_failures_total` | Upstream handshake failures |

## Dependencies

- `eggress-protocol-shadowsocks` — Shadowsocks UDP
- `eggress-routing` — per-datagram routing

See [overview.md](overview.md) for context.
