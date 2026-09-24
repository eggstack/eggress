# Rust API Qualification

This is the maintained classification of the published Rust workspace. It is
documentation for downstream users and reviewers, not a visibility policy:
the campaign does not remove, move, rename, deprecate, or signature-change
existing public items.

## Preferred entry points

| Use case | Preferred crate/path |
|---|---|
| Full in-process service lifecycle | `eggress-embed::{EggressConfig, EggressService, EggressHandle}` |
| Listener-free outbound chains | `eggress-outbound::OutboundConnector` |
| Existing full-service imports | `eggress_embed::outbound::*` compatibility re-export |
| Generic relay between connected streams | `eggress-relay::{relay, RelayOptions, RelayReport}` |
| CLI and installed binaries | `eggress-cli` (`eggress` and `pproxy` targets) |

`eggress-config::RuntimeConfig` crossing `eggress-embed::EggressConfig` is an
established supported coupling. `eggress-server` and `eggress-runtime` public
items are likewise published library contracts; their visibility does not
authorize duplicating their implementation in another crate.

## Published crate classification

| Crate | Classification | Boundary note |
|---|---|---|
| `eggress-embed` | primary supported surface | Full lifecycle facade; re-exports outbound API |
| `eggress-outbound` | primary supported surface | Listener-free chain execution authority |
| `eggress-relay` | primary supported surface | Protocol-neutral byte relay |
| `eggress-cli` | primary supported surface | Native/compat CLI and binary targets |
| `eggress-core` | supporting library surface | Boxed streams, destinations, connectors, chain traits |
| `eggress-uri` | supporting library surface | Native URI grammar and redacted display |
| `eggress-config` | supporting library surface | TOML model, validation, compilation authority |
| `eggress-routing` | supporting library surface | Routing model, router, schedulers, health interfaces |
| `eggress-metrics` | supporting library surface | Metrics registry and runtime observability |
| `eggress-server` | supporting library surface | Listener-bound session orchestration |
| `eggress-runtime` | supporting library surface | Supervisor, snapshots, reload, lifecycle traits |
| `eggress-admin` | supporting library surface | Admin HTTP and snapshot-facing operations |
| `eggress-udp` | supporting library surface | UDP association, codecs, and relay primitives |
| `eggress-system-proxy` | supporting library surface | Platform system-proxy integration |
| `eggress-protocol-http` | supporting library surface | HTTP/CONNECT and H2 handlers |
| `eggress-protocol-socks` | supporting library surface | SOCKS handlers and UDP codec |
| `eggress-protocol-shadowsocks` | supporting library surface | AEAD and legacy compatibility handlers |
| `eggress-protocol-trojan` | supporting library surface | Trojan wire handlers |
| `eggress-protocol-websocket` | supporting library surface | WebSocket tunnel handler |
| `eggress-protocol-raw` | supporting library surface | Raw/tunnel stream handler |
| `eggress-protocol-reverse` | supporting library surface | Reverse control/data-plane handlers |
| `eggress-protocol-h3` | supporting library surface | HTTP/3 integration behind `quic` |
| `eggress-transport-tls` | supporting library surface | TLS stream composition |
| `eggress-transport-ssh` | supporting library surface | Optional verified/compat SSH upstream transport |
| `eggress-transport-quic` | supporting library surface | Optional QUIC transport |
| `eggress-pproxy-compat` | compatibility surface | pproxy parser, translation, diagnostics, contracts |
| `eggress-python` | compatibility facade surface | PyO3 module; Python API is the supported contract |
| `eggress-testkit` | implementation-facing public seam | Fixtures and oracle support, not a runtime facade |

Some supporting crates expose composition seams because published consumers
already use them. Those items remain semver-relevant until a separately
approved major-version strategy exists.

## Qualification gate

The low-maintenance gate is ordinary Rust compilation and focused construction
tests. It intentionally does not add `cargo-semver-checks`, public-API JSON,
nightly rustdoc, or a generated API database to routine CI. A manual
`cargo-semver-checks` invocation was evaluated and rejected for ordinary CI:
it would require nightly/API-baseline maintenance without catching the
representative paths already covered below.

Required feature slices are listed in `AGENTS.md` and enforced in
`.github/workflows/ci.yml`:

- `eggress-outbound`: base, `toml`, `pproxy-compat`, `ssh`, `ssh+pproxy-compat`, `udp`;
- `eggress-embed`: `ssh`, `pproxy-compat`, `ssh+pproxy-compat`.

Representative (not exhaustive) downstream-shaped compile contracts:

- `eggress-embed/tests/public_api.rs`: embed config handoff
  (`EggressConfig::from_toml_str`/`from_compiled`), outbound authority
  (`OutboundConnector::direct`/`from_chain`) plus `eggress_embed::outbound`
  compatibility re-export, relay/routing/core paths, plus Phase 4
  `supporting_config_runtime_paths_compile` (`eggress-config` TOML compile,
  `eggress-runtime` supervisor/state/classify signatures) and
  `protocol_representative_paths_compile` (HTTP/SOCKS via URI + `HttpDetector`/
  `ConnectRequest`);
- `eggress-core::connector::ConnectionMetadata` and the additive
  `DirectConnector::connect_with_options_and_metadata()` /
  `ChainExecutor::execute_with_metadata()` methods are supporting composition
  seams. They preserve truthful first-hop TCP socket addresses without
  changing existing connection signatures or the `BoxStream` boundary;
  hop-zero pooled SSH/H2 may report `None` when the candidate socket is not
  the transport carrying the returned stream. Eggress H2 pools are scoped to
  the TLS client-config identity; the endpoint/SNI/auth `H2PoolKey` does not
  encode trust policy. Explicit hop-zero `local_bind` disables SSH/H2 reuse,
  explicit insecure H2 is unpooled, and nested SSH/H2 always consumes the
  selected prior-hop stream without pooling.
- `eggress_transport_tls::client_config_with_alpn` is the single internal
  authority for ALPN adaptation of an existing `Arc<rustls::ClientConfig>`.
  It clones the underlying `rustls::ClientConfig` via `ClientConfig::clone()`
  and only mutates `alpn_protocols`; trust roots, custom CA stores, mTLS
  client identity, custom verifier, and every other `ClientConfig` field are
  preserved by `ClientConfig::clone()` and survive the adaptation. The
  outbound TLS wrapper uses this helper when an H2 hop adds the H2 ALPN list
  to a configured override, so custom TLS policies survive H2 ALPN
  adaptation. Callers must never rebuild a fresh system-roots configuration
  as a fallback when an override is already present; a caller-supplied
  `tls_override` combined with per-hop `insecure=true` is rejected
  explicitly.
- `eggress-server/tests/public_api.rs`: `NoopMetrics`, `UdpAssociationHandle`,
  `SessionReport`, `ConnectionConfig`, `ConnectionContext`, `AuthReuseCache`;
- lower-level crates retain their existing unit/integration tests as semantic
  qualification; ordinary crate tests own behavior, contracts own import paths.

Compatibility re-exports (`eggress_embed::outbound::*`, server re-exports)
remain covered; private implementation movement (Phases 1–2) does not change
public paths. No public item or feature changes visibility, name, location,
default, or signature in this phase.
