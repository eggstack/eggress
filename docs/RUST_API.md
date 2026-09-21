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
nightly rustdoc, or a generated API database to routine CI.

Required feature slices are listed in `AGENTS.md` and cover outbound base,
TOML, pproxy, SSH, and UDP combinations plus embed SSH/pproxy combinations.
Representative contract tests exercise the embed config handoff, outbound
constructors, and the compatibility re-export; lower-level crates retain
their existing unit and integration tests as executable qualification.
