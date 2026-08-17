# eggress-transport-quic and eggress-protocol-h3

QUIC and HTTP/3 are optional. The `quic` feature adds Quinn, the QUIC stream
adapter, and the HTTP/3 CONNECT protocol without changing default or `common`
builds.

## Roles

| URI/protocol | Runtime role | ALPN | Requirements |
|---|---|---|---|
| `quic+http://host:port` | Raw QUIC stream carrying HTTP | none | Client or listener application protocol |
| `quic+socks5://host:port` | Raw QUIC stream carrying SOCKS5 | none | Client or listener application protocol |
| `h3://host:port` | HTTP/3 CONNECT | `h3` | TLS certificate/key on listeners |

`QuicClient` caches one established QUIC connection per configured endpoint
and opens independent bidirectional streams. A terminated connection is
discarded and the next stream request reconnects once. `QuicListener` accepts
connections and dispatches each bidirectional stream independently, so one
slow stream cannot serialize other streams.

`H3Client` and `serve_connection` map HTTP/3 CONNECT request streams to the
shared `BoxStream` boundary. The request uses an HTTPS URI with the destination
authority and optional `Proxy-Authorization: Basic ...`; successful CONNECT
responses are `200`. Non-CONNECT requests receive `405`, and failed proxy
authentication receives `407`.

## Security and validation

Native QUIC clients use platform certificate verification. The insecure
verifier exists only for compatibility adapters and must be explicit and
warning-bearing. Listener certificate and key PEM are validated before bind;
HTTP/3 listeners advertise ALPN `h3` and raw QUIC listeners advertise no
application ALPN. Unix, transparent, fixed-target HTTP/3, and listener UDP
association modes are rejected during config compilation.

UDP-over-QUIC stream mapping is intentionally outside the supported
composition matrix. It is reported as unsupported rather than falling back to
direct UDP.

## Boundaries and tests

Quinn and `h3-quinn` types stop at the optional transport/protocol crates;
the server, chain executor, and runtime only consume opaque connections and
`BoxStream`. Focused coverage is provided by:

```text
cargo test -p eggress-transport-quic
cargo test -p eggress-protocol-h3
cargo check -p eggress-runtime --features quic
```

WebTransport, MASQUE, HTTP/3 datagrams, and QUIC 0-RTT application data are
not implied by this implementation.

## Phase 8 build record

The selected stack is `quinn 0.11.11`, `h3 0.0.8`, and `h3-quinn 0.0.10`.
The workspace MSRV remains Rust 1.85; the local verification toolchain was
Rust 1.97.1. Release binary measurements on that toolchain were:

| Build | `eggress` | `pproxy` |
|---|---:|---:|
| default | 10,522,824 bytes | 9,458,560 bytes |
| `--features quic` | 12,102,376 bytes | 11,038,104 bytes |

The feature reuses the workspace rustls/ring backend; it does not add a
second TLS implementation. QUIC/H3 remains disabled by default.
