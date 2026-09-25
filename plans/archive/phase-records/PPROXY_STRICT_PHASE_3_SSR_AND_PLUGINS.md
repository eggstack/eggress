# pproxy Strict Phase 3 — Exact SSR Core and Built-in Plugins

## Objective

Implement the exact ShadowsocksR-style surface that `pproxy==2.7.9` exposes without turning Eggress into a general ShadowsocksR implementation.

The key scope rule is: reproduce pproxy's code, syntax, and observable framing only. Do not import unrelated SSR protocol/obfs families from other projects.

## Exact upstream target

The tagged pproxy implementation exposes:

- an `SSR` protocol with optional user/auth prefix plus SOCKS-style destination address framing;
- six built-in plugin names: `plain`, `origin`, `http_simple`, `tls1.2_ticket_auth`, `verify_simple`, and `verify_deflate`.

`SS`/Shadowsocks OTA is separate legacy work and belongs to Phase 9 unless a plugin strictly requires the common wrapper abstraction.

## Architecture

Prefer compatibility isolation inside `eggress-protocol-shadowsocks` rather than adding SSR behavior to unrelated runtime crates.

Suggested structure:

```text
crates/eggress-protocol-shadowsocks/src/
  compat/
    mod.rs
    ssr.rs
    plugin.rs
    plugin_http_simple.rs
    plugin_tls_ticket.rs
    plugin_verify_simple.rs
    plugin_verify_deflate.rs
```

A separate crate is justified only if feature-gating within the existing crate cannot keep the default dependency/binary impact small.

Add a feature such as `pproxy-legacy` if this compatibility code materially increases default size. Default native builds should not need deprecated obfuscation code.

## SSR core implementation

Implement only the pproxy 2.7.9 framing:

### Server/inbound

- optional configured user prefix matching;
- first address-type byte;
- IPv4/domain/IPv6 destination parsing using the same accepted address tags as pproxy;
- destination port parsing;
- remaining bytes preserved as application payload.

### Client/upstream

- write configured auth prefix;
- encode domain destination using the pproxy SSR form;
- preserve exact port byte order;
- relay remaining stream normally.

### UDP

Only implement SSR UDP if Phase 0 confirms pproxy 2.7.9 actually exposes a usable SSR UDP path. Do not infer support merely from shared base classes.

## Plugin model

Do not build arbitrary dynamically registered plugins. Use a closed enum:

```rust
enum PproxyPlugin {
    Plain,
    Origin,
    HttpSimple,
    Tls12TicketAuth,
    VerifySimple,
    VerifyDeflate,
}
```

Provide only the hooks required by the exact transforms:

- initial client preface/response handling;
- initial server preface/response handling;
- outbound stream transform;
- inbound stream transform.

Where possible wrap `AsyncRead/AsyncWrite` with small codec adapters instead of mutating shared buffers.

## Plugin requirements

### `plain` and `origin`

Identity transforms. They still need parser and name parity.

### `http_simple`

Reproduce pproxy's HTTP-looking handshake sufficiently for bidirectional pproxy interoperability. Match required request/response framing; do not attempt to become a general HTTP camouflage framework.

### `verify_simple`

Implement pproxy's framed chunks including:

- random padding length byte;
- length field;
- CRC calculation and endianness;
- maximum chunking behavior needed for interop;
- incremental decoder for fragmented input.

### `verify_deflate`

Implement pproxy's zlib framing, exact length field, and incremental buffering. Reuse a small maintained compression dependency already present transitively only if appropriate; otherwise add the narrowest dependency and feature-gate it.

### `tls1.2_ticket_auth`

Treat this as compatibility obfuscation, not real TLS.

Reproduce the pproxy handshake/HMAC/timestamp/cache framing needed for interop. Keep it completely separate from `rustls` TLS code so callers cannot confuse it with an authenticated TLS transport.

Security constraints:

- timestamp tolerance matches pproxy where observable;
- replay/cache behavior is bounded;
- malformed frames fail closed;
- no panic on adversarial input;
- no claim that the plugin provides TLS security.

## URI/parser integration

Update pproxy URI parsing so comma-separated plugin names after the path are retained in order and lowered into the closed plugin list.

Rules:

- unknown plugin -> pproxy-shaped parse error listing/identifying supported names;
- plugins without a cipher/protocol context that can execute them -> startup validation error;
- no silent ignore;
- preserve ordering because pproxy applies plugin transforms sequentially.

## Python compatibility

`pproxy.plugin.get_plugin()` and the six class/name surfaces must become functional if Phase 0 classifies them as required. They may wrap Rust-backed compatibility objects; they do not need to contain Python implementations.

## Testing

For each plugin:

- codec unit tests with fragmented reads;
- malformed/truncated frame rejection;
- pproxy client -> Eggress server;
- Eggress client -> pproxy server.

For SSR core:

- IPv4, IPv6, and domain target;
- configured auth prefix success/failure;
- TCP payload roundtrip;
- plugin ordering with at least a two-plugin documented combination.

Use local echo servers; no public network dependency.

## Non-goals

- Other SSR auth protocols or obfs families.
- SIP003 external plugins.
- Legacy stream cipher implementation beyond what is necessary to compile/test this phase; Phase 9 owns those methods.
- Security claims for obfuscation plugins.

## Acceptance criteria

1. `ssr://` URIs accepted by pproxy 2.7.9 and within the exact implemented surface execute in Eggress compatibility mode.
2. SSR TCP client and server interoperate bidirectionally with pproxy 2.7.9 for IPv4/domain/IPv6 targets.
3. All six exact plugin names parse and execute.
4. `plain`/`origin` are identity-compatible; the remaining four plugins pass bidirectional pproxy interop tests.
5. Plugin ordering is preserved.
6. Unknown plugin names fail before service startup.
7. Compatibility plugin code is isolated from native security-critical TLS/protocol paths.
8. Default binary impact is measured; if material, legacy/plugin support is behind a non-default feature.
