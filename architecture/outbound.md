# eggress-outbound — Listener-Free Outbound Execution

The direct listener-free Rust dependency: concrete proxy-hop composition,
chain-executor construction with TLS, typed failure classification, and the
`OutboundConnector` that executes chains in-process without listeners.
`eggress-server` consumes the hop registry, executor factory, and classifier
for its listener-bound sessions; `eggress-embed` re-exports this API as its
full-service facade (`eggress_embed::outbound::*` stays source-compatible).

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | Crate root; stable public facade (`OutboundConnector`, `OutboundInfo`, typed errors, `UdpAssociation`, `OUTBOUND_MAX_DATAGRAM_SIZE`, executor/helpers) |
| `src/hops.rs` | One `HopHandler` per upstream protocol + `target_to_socks_addr` (`#[doc(hidden)]` shared seam) |
| `src/executor.rs` | `OutboundExecutorOptions`, `build_chain_executor()` / `build_chain_executor_with_options()` |
| `src/classify.rs` | Single typed classifier (`ClassifiedKind`, `classify_io_kind` / `classify_connect_error` / `classify_handshake_source`, `#[doc(hidden)]`) |
| `src/connector.rs` | `OutboundConnector`, `OutboundInfo`, `OutboundRoute`, TOML/pproxy/native constructors, TCP execution (`connect_tcp*`), `associate_udp` orchestration; inline tests retained for locality |
| `src/connect_error.rs` | Typed TCP failure surface: `OutboundConnectError`/`Kind`/`Stage`, `ClassifiedFailure`, classifier adaptation, protocol-label normalization (redacted Display) |
| `src/udp.rs` | Listener-free UDP lifecycle: `UdpAssociation`, direct/SOCKS5 send/recv, resolution/conversion/accounting (`udp` feature; frozen capability, no pooling) |
| `src/compat.rs` | pproxy boundary: constructor adaptation, credential-term extraction, redaction/scrubbing (over-redaction, bracket-aware), compat error mapping (`pproxy-compat` feature) |
| `src/error.rs` | `OutboundError` (Config/Runtime/UnsupportedFeature/Internal, redacted) |

## Public API surface

### Executor (`src/executor.rs`)

```rust
// Signatures are cfg-gated per feature; see src/executor.rs:112-119 verbatim:
pub struct OutboundExecutorOptions {
    pub tls_override: Option<Arc<rustls::ClientConfig>>,
    #[cfg(feature = "extended")]
    pub shadowsocks_metrics: Option<Arc<ShadowsocksMetrics>>,
    #[cfg(feature = "ssh")]
    pub ssh_sessions: Option<Arc<SshSessionCache>>,
}

pub fn build_chain_executor(
    tls_override: Option<&Arc<rustls::ClientConfig>>,
    shadowsocks_metrics: Option<Arc<ShadowsocksMetrics>>, // Option<()> without `extended`
    ssh_sessions: Option<Arc<SshSessionCache>>,           // gated on `ssh`
) -> ChainExecutor;
pub fn build_chain_executor_with_options(options: OutboundExecutorOptions) -> ChainExecutor;
```

Handlers in fixed order: Http, HttpOnly, Socks5, Socks4,
[Shadowsocks, Trojan, WebSocket] (extended), [ShadowsocksR]
(pproxy-legacy), Raw, Unix, [Ssh] (ssh, only when a session cache is
supplied), H2, [Quic, H3] (quic). The executor also installs a TLS wrapper
using system root CAs or the `tls_client_config` override, with
ALPN-specific cached configs and per-hop insecure behavior under the
`insecure-tls` gate.

### OutboundConnector (`src/connector.rs`)

| Method | Description |
|---|---|
| `from_chain(chain)` | General native constructor from a compiled `ProxyChainSpec`; no TOML, no pproxy, no server/runtime types; rejects empty chains |
| `direct()` | Explicit direct connector with no proxy hops |
| `from_toml(config_toml)` (feature `toml`) | Delegate parse/version/validate/compile to `eggress-config`, then extract the first upstream's chain, record the upstream count, and drop the service config |
| `from_pproxy_uri(uri)` (feature `pproxy-compat`) | Full pproxy `__` chain → `compile_chain_to_native` (no TOML) → stored chain (fail-closed, redacted errors) |
| `connect_tcp(host, port)` | Compatibility surface: failures stay `OutboundError::Runtime` |
| `connect_tcp_detailed(host, port)` | Opt-in typed surface returning `OutboundConnectError` (`kind`/`stage`/`hop_index`/`protocol`) |
| `connect_tcp_timeout(host, port, timeout)` | Compatibility timeout: outer deadline stays `Runtime("connection timed out")` |
| `connect_tcp_timeout_detailed(host, port, timeout)` | Typed timeout: outer deadline is `Timeout`/`Deadline` |
| `associate_udp(host, port)` (feature `udp`) | Listener-free fixed-target UDP: direct or single-hop SOCKS5 |
| `associate_udp_timeout(host, port, timeout)` (feature `udp`) | Same with establishment timeout |
| `active_udp_associations()` | Live listener-free UDP count |
| `upstream_count()` / `hop_count()` | Configured upstream count / chain hop count (0 for direct) |
| `validate_outbound_config(toml)` (feature `toml`, `#[cfg(feature = "toml")]`) | Static validation, returns hop count |

`from_pproxy_uri()` preserves every `__` hop in source order. Only a single
`direct` hop takes the direct fast path; multi-hop `direct`, backward
(`+in`), or unsupported roles fail closed with redacted errors. The
connector owns the executor's SSH session state for its full lifetime:
native/TOML/`from_chain` construction uses the verified
`SshSessionCache::new()` policy, while `from_pproxy_uri()` uses
`new_compatibility()` only when both `ssh` and `pproxy-compat` are enabled.
Direct mode does not allocate SSH state.

### Typed outbound errors (`OutboundConnectError`)

Same contract as the former embed surface: kinds `Timeout`, `Dns`,
`ConnectionRefused`, `NetworkUnreachable`, `HostUnreachable`,
`Authentication`, `Tls`, `Protocol`, `Policy`, `Other`; stages
`DirectConnect`, `HopConnect`, `HopHandshake`, `Deadline`.
`HopConnect` vs `HopHandshake` distinguishes proxy transport failure from
proxy-reported destination failure without string parsing. Kind/stage/hop/
protocol are diagnostic facts, not retry recommendations; callers own
retry/backoff policy and no direct fallback occurs. `Display`/`Debug` carry
only kind/stage/hop/protocol facts, never credentials, URIs, or config
snippets.

### Outbound UDP (`associate_udp`, feature `udp`)

Fixed-target connected semantics over existing UDP primitives, no hidden
listener: direct uses a family-aware wildcard bind (`0.0.0.0:0` for IPv4,
`[::]:0` for IPv6) + `connect(resolved target)`; single-hop SOCKS5 uses
`open_socks5_udp_upstream()` with per-send/recv datagram encode/decode.
Target validation via `validate_standalone_target(allow_private_egress=true)`
plus `validate_datagram_size(65535)`. Unsupported chains (HTTP, multi-hop,
composed, Shadowsocks UDP in this surface) fail with `UnsupportedFeature`,
never silent direct fallback.

## Configuration / features

| Feature | Description |
|---|---|
| `toml` | TOML construction (`eggress-config` boundary) |
| `pproxy-compat` | pproxy URI construction (`from_pproxy_uri`) |
| `udp` | Listener-free UDP (`dep:eggress-udp` only; SS/composed fail `UnsupportedFeature` in this surface) |
| `extended` | Shadowsocks, Trojan, WebSocket hops |
| `pproxy-legacy` | ShadowsocksR (`extended` + SSR framing) |
| `legacy-crypto` | Legacy Shadowsocks ciphers |
| `ssh` | SSH upstream transport; does not activate `pproxy-compat` |
| `quic` | QUIC/H3 transport and protocol |
| `insecure-tls` | Per-hop `?insecure` verifier (test-gated; never default) |

`pproxy-compat` never implies `ssh` and `ssh` never implies
`pproxy-compat`. `udp` is independent of TCP; `toml` is independent of
pproxy syntax.

## Security notes

- Outbound error paths parse hops via `eggress_pproxy_compat` and fall back
  to a scheme-agnostic last-`@`-outside-brackets scrubber that additionally
  masks `#` auth fragments; over-redaction is preferred to leakage.
- `OutboundError`/`OutboundConnectError` Display/Debug are bounded and
  redacted; no public `source()` chain.
- Verified native SSH host-key policy; compatibility policy only through the
  explicit compatibility constructor with both features enabled.
- No proxy failure falls back to direct; private/reserved-target rules are
  enforced by the core connector; HTTP-only buffering (64 KiB) and UDP
  payloads (65535) stay bounded.

## Test coverage

| Area | Location |
|---|---|
| Classifier unit tests | `src/classify.rs` (`mod tests`) |
| HttpOnly rewrite + stream | `src/hops.rs` (`mod tests`, moved from server) |
| `from_chain` / `direct` / TOML / pproxy construction + redaction | `src/connector.rs` (`mod tests`) |
| UDP echo/relay/lifecycle | `src/connector.rs` (`mod tests`, features `udp,pproxy-compat`) |
| Typed `connect_tcp_detailed` matrix, legacy compat | `eggress-embed/tests/outbound_detailed.rs` (via re-export facade) |
| SSH session reuse/host-key/OpenSSH fixture | `eggress-embed/tests/ssh.rs` (via re-export facade) |

Run: `cargo test -p eggress-outbound`,
`cargo test -p eggress-outbound --no-default-features --features udp,pproxy-compat,toml`,
plus the required OpenSSH regression
(`EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked
--no-default-features --features ssh,pproxy-compat --test ssh`).

## Reviewer gotchas

- `OutboundConnector` holds an `OutboundRoute` (direct vs `Arc<ProxyChainSpec>`
  + upstream count), never a full `RuntimeConfig`.
- `eggress-server` consumes `build_chain_executor`, `classify`, and
  `target_to_socks_addr` from here; `eggress-embed::outbound` is a pure
  `pub use eggress_outbound::*` facade.
- `eggress-udp` is absent unless the `udp` feature is selected;
  a direct `eggress-config` edge exists only under `toml`
  (`pproxy-compat` pulls `eggress-config` transitively through
  `eggress-pproxy-compat` translation ownership, intentionally retained —
  the connector still stores an `OutboundRoute`, never a `RuntimeConfig`).

## See also

- [core.md](core.md) — generic `ChainExecutor`/`HopHandler`, BoxStream, direct connector
- [server.md](server.md) — listener-bound sessions consuming this crate
- [embed.md](embed.md) — full-service facade re-exporting this API
- [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) — SSH/QUIC/H3 transport features
- [udp.md](udp.md) — UDP association lifecycle
