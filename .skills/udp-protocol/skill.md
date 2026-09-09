# UDP Protocol Development

## When to use
Use when working with UDP associations, datagram relay, upstream SOCKS5 relay, or adding UDP transport support.

## Architecture overview
- TCP SOCKS5 control connection owns the UDP association lifetime (listener path)
- Each target gets its own `UdpTargetFlow` (connected UDP socket) for reliable response demux
- Client address pinning is enabled by default
- Listener UDP supports direct, one-hop SOCKS5/Shadowsocks (AEAD), and composed SOCKS5/Shadowsocks multi-hop chains via `open_composed_udp_upstream`; HTTP/MASQUE/Trojan UDP remain unsupported
- Listener-free outbound (`OutboundConnector::associate_udp`, fixed-target, no hidden listener) supports direct + single-hop SOCKS5 only; composed/Shadowsocks in that surface fail with `UnsupportedFeature`
- Shadowsocks upstream is supported one-hop using standard AEAD UDP packets;
  standalone inbound Shadowsocks uses the explicit pproxy PacketCipher format
  and must not be conflated with the standard upstream path.

## Key types (`eggress-udp`)
- `UdpAssociation` — listener association state machine (not the embed outbound type)
- `UdpAssociationRegistry` — bounded tracking with global/per-listener limits
- `UdpTargetFlow` — connected UDP socket per target
- `UdpFlowKind` — Direct, Socks5Upstream, ShadowsocksUpstream, Composed
- `UdpFlowKey` — typed flow key enum
- `UdpLimits` — configurable constraints
- `UdpMetrics` — Prometheus counters/gauges
- `UdpRelayCapability` — classifies chains as supported/unsupported
- `eggress-embed::outbound::UdpAssociation` — fixed-target listener-free outbound (`send/recv/close`, direct + single SOCKS5)

## Adding UDP support to a new upstream protocol

1. Add a `UdpFlowKind` variant
2. Implement the flow establishment (TCP control + UDP relay handshake)
3. Update `UdpRelayCapability` to classify your protocol
4. Add upstream metrics
5. Handle in `relay.rs` `handle_client_datagram()`

## Common pitfalls
- Never use bare `tokio::spawn` for relay tasks — use `TaskTracker`
- Every close path must call `registry.remove(id)` exactly once
- UDP bind address changes require restart (not hot-reloadable)
- Idle timeout changes only apply to new associations after reload
- The legacy `udp_enabled = true` flag is still supported for backward compat

## Testing
- `cargo test -p eggress-udp` — unit tests
- `cargo test -p eggress-runtime udp` — integration tests
- `cargo test -p eggress-udp socks5_upstream` — upstream relay tests
- `cargo test -p eggress-runtime udp_upstream` — runtime upstream tests
- `cargo test -p eggress-embed --lib udp` — listener-free outbound UDP (direct + SOCKS5, lifecycle)

## Config example
```toml
[[listeners]]
name = "socks-in"
bind = "127.0.0.1:1080"
protocols = ["socks5"]

[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
advertise = "127.0.0.1"
idle_timeout = "60s"
target_idle_timeout = "30s"
max_associations = 512
max_targets_per_association = 32
max_datagram_size = 65535
client_pin = true
```
