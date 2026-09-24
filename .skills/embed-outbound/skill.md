# Embed and Listener-Free Outbound

## When to use
Use when embedding eggress in another Rust process (`eggress-embed`) or when
executing proxy chains without listeners (`OutboundConnector`). For listener
sessions, see `architecture/server.md`; for protocol/transport internals, see
`.skills/rust-proxy-dev/skill.md`; for Python embedding, see
`.skills/python-bindings/skill.md`.

## Ownership rule
`eggress-outbound` is the single implementation authority (concrete
`HopHandler`s, `build_chain_executor()` factory, shared classifier).
`eggress-embed::outbound` is a source-compatible `pub use` facade
(`crates/eggress-embed/src/outbound.rs`). Fix outbound behavior in
`eggress-outbound`; never duplicate the registry, factory, or classifier in
the server or embed crates. `eggress-server` consumes the factory via
`execute/mod.rs`; `eggress-config::validate_and_compile_toml` remains the
canonical TOML parse/validate/compile authority.

## TLS composition invariants
- ALPN adaptation of a caller-supplied `Arc<rustls::ClientConfig>` uses
  `eggress_transport_tls::client_config_with_alpn`, which clones the
  underlying `rustls::ClientConfig` via `ClientConfig::clone()` and only
  mutates `alpn_protocols`. Trust roots, custom CA stores, mTLS client
  identity, custom verifier, and every other `rustls::ClientConfig` field
  are preserved. The outbound TLS wrapper must never rebuild a fresh
  system-roots `ClientConfig` as a fallback when an override is already
  present.
- A caller-supplied `tls_override` combined with a per-hop `insecure=true`
  request is rejected explicitly (no silent substitution of Eggress's
  default insecure verifier). Callers that intentionally own an insecure
  `ClientConfig` must pass that config as `tls_override` without also
  requesting the Eggress per-hop insecure mode; otherwise remove
  `tls_override` and use Eggress's feature-gated insecure policy. There
  is no separate insecure-override field.

## Full-service embed (`eggress-embed`)
- `EggressConfig::from_toml_str()` / `from_toml_file()` — parse and validate;
  stores compiled `RuntimeConfig` + source TOML. `to_redacted_toml()` masks
  secrets and URI userinfo.
- `EggressService::new(config).start()` (async) / `start_blocking()` —
  in-memory start with no temp file; returns `EggressHandle`.
- `handle.bound_addresses()` — port-0 friendly discovery; `status()` —
  generation/readiness/uptime/connections; `metrics_text()` — Prometheus
  without HTTP.
- `handle.reload_toml_str()` / `reload_toml_file()` / `reload_compiled()` —
  all share the canonical `apply_compiled_config` transaction; only
  routing/upstreams/groups/health swap atomically (listener topology is
  restart-only).
- `handle.shutdown()` / `shutdown_blocking()` (consuming) plus idempotent
  `cancel()` / `cancel_and_cleanup()` and best-effort `Drop` join.
- Preferred compat entry is `start_blocking_with_compatibility_hooks()`;
  `start_blocking_with_compatibility_options()` is the deprecated facade.

## Listener-free chains (`OutboundConnector`)
- `from_chain(ProxyChainSpec)` — native compiled chain, no TOML/pproxy;
  rejects empty chains. `direct()` — explicit no-hop alternative.
- `from_toml(toml)` (feature `toml`) — canonical config compile, then first
  upstream chain extraction.
- `from_pproxy_uri(uri)` (feature `pproxy-compat`) — full `__` multi-hop
  expression via `compile_chain_to_native()`, no TOML string, no listener,
  fail-closed with redacted errors.
- `connect_tcp()` / `connect_tcp_timeout()` — compatibility surfaces
  (`OutboundError::Runtime`); `connect_tcp_detailed()` /
  `connect_tcp_timeout_detailed()` — typed `OutboundConnectError`
  (`kind()`/`stage()`/`hop_index()`/`protocol()`; `HopConnect` vs
  `HopHandshake`; outer deadline is `Timeout`/`Deadline`). Classify with
  `eggress-outbound::classify`, never message strings.
- `OutboundInfo` TCP addresses describe the socket actually established:
  direct routes describe the target, while chains describe hop 0. Capture
  metadata before boxing and carry it alongside `BoxStream`; do not resolve a
  host again just to populate `peer_addr`. `Some(addr)` must identify the
  physical transport carrying the returned stream. Hop-zero pooled SSH/H2 may
  return `None`; nested SSH/H2 is unpooled and keeps hop-zero metadata. Address
  metadata is observational, and Unix or other non-TCP first hops may return
  `None`.
- Reuse SSH/H2 physical connections only at hop 0. Nested SSH/H2 must consume
  the supplied chain stream until route-prefix-scoped pool identity exists.
- Hop-zero H2 pools are shared only among executors that share the same TLS
  client-config object; this preserves pooling across the server's per-route
  executor construction while separating policy objects. Explicit `local_bind` disables SSH and H2 cache reuse;
  explicit insecure H2 also disables pooling. Do not use the public global H2
  registry from Eggress chain handlers.
- `associate_udp()` (feature `udp`) — fixed-target direct + single-hop SOCKS5
  only (IPv4/IPv6, family-corrected bind); composed/Shadowsocks in this
  surface fail with `UnsupportedFeature`.
- SSH session cache is owned by the connector for its lifetime:
  native/`from_chain`/`from_toml` use verified `SshSessionCache::new()`;
  `from_pproxy_uri()` uses `new_compatibility()` only with both `ssh` and
  `pproxy-compat`.

## Feature boundaries
- Base profile covers ordinary HTTP/SOCKS TCP (`from_chain`/`direct`).
- Opt-in: `toml`, `pproxy-compat`, `udp`, `ssh` (+`ssh,pproxy-compat` for
  pproxy-style SSH), `quic`, `extended`, `pproxy-legacy`.
- `eggress-embed` default `full` includes `pproxy-legacy`; `eggress-cli`
  default `full` does not. Never use `--all-features` (drags in test-only
  `insecure-quic` / `insecure-tls`).

## Testing
```bash
cargo test -p eggress-embed
cargo test -p eggress-outbound --no-default-features --features udp,pproxy-compat,toml
EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked \
  --no-default-features --features ssh,pproxy-compat --test ssh -- --nocapture
```

Embed integration lives in `crates/eggress-embed/tests/` (`start_stop`,
`proxy_traffic`, `reload`, `reload_convergence`, `metrics_status`,
`error_redaction`, `public_api`, `outbound_detailed`, `ssh`).

## References
- `architecture/embed.md`, `architecture/outbound.md`
- `docs/EMBED_API.md`, `docs/RUST_API.md`
