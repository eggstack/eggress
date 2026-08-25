# eggress-protocol-reverse — Reverse / Backward Proxy (NAT Traversal)

Implements pproxy's backward model: a client behind NAT dials OUT to an
acceptor; external clients hit the acceptor and their sessions are tunneled
through control channels back to the NAT'd host. Also ships pproxy-wire
compatible adapters (raw and SOCKS5-framed backward channels).

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | Native auth handshake: client sends `user:pass\n`, server answers 1 byte (`0x01` accept / `0x00` reject). `redact_auth` for logs, `ControlState`, half-close-preserving relay, auth payload cap 4096 bytes, 100 ms delay on auth failure |
| `src/server.rs` | `ReverseServer` (acceptor): control listener pools authenticated channels in a queue; external connections pop a channel and relay. `ReverseServerConfig::validate()`: non-loopback external bind REQUIRES auth + non-empty `allow_bind` allowlist; bounds on control conns / pending external / streams |
| `src/client.rs` | `ReverseClient`: dial → auth → serve; reconnect loop with exponential backoff (1 s initial, doubling, 30 s cap); `TargetResolver` trait decouples it from routing |
| `src/compat_pproxy.rs` | `PproxyBackwardClient/Server`: raw byte-pipe framing and SOCKS5-framed channel variants matching upstream pproxy backward wires |
| `src/metrics.rs` | `ReverseMetrics`: control conns active/accepted/rejected, auth failures, reconnects, streams opened/closed |

## Runtime integration

`eggress-runtime/src/reverse.rs` implements `TargetResolver` by building a
synthetic `RouteRequest` (transport `ReverseTcp`, listener = reverse listener
name) against `SharedRoutingService::decide()` — policy gates each target;
Reject maps to `TargetResolution::Reject`. See [runtime.md](runtime.md).

## Explicit limitations

TCP only · one session per control connection (parallelism via N channels) ·
no built-in TLS (wrap externally) · no keepalive heartbeat yet.

## Review entry points

- Verify: `cargo test -p eggress-protocol-reverse`;
  `cargo test -p eggress-runtime reverse`
