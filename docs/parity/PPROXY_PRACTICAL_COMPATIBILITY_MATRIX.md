# Practical pproxy 2.7.9 compatibility matrix

This maintained matrix describes observable Eggress compatibility with
`pproxy==2.7.9`; it makes no aggregate parity claim. Status labels are defined
in [`README.md`](README.md). A skipped oracle run is not evidence of a match.

| Surface | Capability | CLI | Python | Runtime | Evidence | Status | Notes |
|---|---|---:|---:|---:|---|---|---|
| URI grammar | HTTP/HTTPS, SOCKS4/4a, SOCKS5, direct | yes | yes | yes | compat tests; differential harness | `matched` | Common listener/upstream forms. |
| URI grammar | Combined protocols and `__` chains | yes | yes | yes | URI/translation tests | `matched` | Native chain validation still applies. |
| URI grammar | `+tls`, `+ssl`, repeated `+in` | yes | yes | yes | URI regression tests | `matched` | Modifiers are preserved before lowering. |
| URI grammar | Fragment auth, local bind, fixed target, plugin/rule metadata | yes | yes | partial | parser/redaction tests | `supported_difference` | Parsed metadata is diagnosed when unusable. |
| URI grammar | H2, WS/WSS, raw, tunnel upstreams | yes | yes | yes | advanced transport tests | `supported_difference` | H2/WS/WSS are upstream-only; raw/tunnel forms are bounded. |
| URI grammar | Unix-domain TCP upstream | yes | yes | platform-dependent | URI/config tests | `platform_limited` | Unix only; Windows reports a platform diagnostic. |
| CLI | No-argument mixed-listener default | yes | n/a | yes | `pproxy_binary`, CLI tests | `matched` | Uses the documented HTTP/SOCKS default. |
| CLI | `-l`, `-r`, `-ul`, `-ur` | yes | yes | yes | CLI translation tests | `matched` | Values remain associated with their flags. |
| CLI | `--pac`, `--get`, `--test` ownership | yes | yes | yes | parser/CLI regression tests | `matched` | Values are not positional URIs. |
| CLI | `-s`, `-a`, `-b`, `--rulefile`, `--ssl` | yes | yes | yes | translation/config tests | `supported_difference` | Lowered to native routing, health, TLS, and rules. |
| CLI | `--sys`, `--log`, JSON checks | yes | yes | yes | CLI diagnostic tests | `native_extension` | Native inspection and structured diagnostics. |
| CLI | `--daemon` and `--reuse` | yes | yes | no | unsupported diagnostic tests | `intentional_non_parity` | Use a process manager; pooling is excluded. |
| Inbound | HTTP/HTTPS CONNECT and forward proxy | yes | yes | yes | runtime/differential tests | `matched` | HTTPS is TLS-wrapped HTTP tunneling. |
| Inbound | SOCKS4/4a | yes | yes | yes | runtime/differential tests | `matched` | SOCKS4a domain targets are covered. |
| Inbound | SOCKS5 CONNECT and username/password auth | yes | yes | yes | runtime/differential tests | `matched` | BIND is explicitly rejected. |
| Inbound | SOCKS5 UDP ASSOCIATE | yes | bounded | yes | UDP runtime/differential tests | `supported_difference` | Public API and framing boundaries are documented. |
| Inbound | Shadowsocks AEAD and Trojan | yes | bounded | yes | protocol/runtime tests | `supported_difference` | Native implementations; no strict private-API claim. |
| Inbound | echo and fixed-target raw/tunnel | yes | bounded | yes | Phase 5 runtime tests | `native_extension` | Bounded utility/fixed-target modes. |
| Inbound | Unix socket | yes | bounded | yes | Unix listener tests | `platform_limited` | Unix filesystem listener. |
| Inbound | transparent `redir` | yes | bounded | platform-dependent | transparent tests | `platform_limited` | Linux original-destination facilities required. |
| Upstream | direct, HTTP/HTTPS, SOCKS5 | yes | yes | yes | runtime/differential tests | `matched` | Core TCP workflows. |
| Upstream | SOCKS4/4a | yes | yes | yes | upstream protocol tests | `supported_difference` | Supported with narrower exercised coverage. |
| Upstream | Shadowsocks AEAD and Trojan | yes | bounded | yes | protocol tests | `supported_difference` | Native wire implementations. |
| Upstream | H2 and WS/WSS | yes | bounded | yes | advanced transport tests | `supported_difference` | Upstream-only compatibility bridge. |
| Upstream | raw/tunnel fixed target | yes | bounded | yes | raw/tunnel tests | `supported_difference` | Arbitrary multi-hop tunnel semantics are excluded. |
| Upstream | SSH, QUIC/H3, SSR | recognized | diagnosed | no | parser/diagnostic tests | `intentional_non_parity` | Known syntax receives a specific explanation. |
| TCP chains | One-hop HTTP/SOCKS | yes | yes | yes | differential/integration tests | `matched` | Representative echo and forward workflows. |
| TCP chains | Multi-hop TCP | yes | yes | yes | chain translation/runtime tests | `supported_difference` | Compatible chains, not every Cartesian combination. |
| TCP chains | First-available, round-robin, least-connections, random | yes | yes | yes | routing tests | `supported_difference` | Compatibility defaults to first-available declaration order. |
| TCP chains | Per-remote predicates and rule files | yes | yes | yes | routing/translation tests | `matched` | Regex lines and high-priority blocks are deterministic. |
| TCP chains | `httponly` request rewriting | yes | yes | yes | Phase 5 runtime tests | `supported_difference` | Upstream adapter, not a listener protocol. |
| UDP | Standalone direct echo | yes | bounded | yes | UDP runtime/differential tests | `matched` | Explicit standalone mode. |
| UDP | SOCKS5 association | yes | bounded | yes | UDP integration tests | `supported_difference` | Public callback API is bounded. |
| UDP | One-hop SOCKS5 upstream | yes | no | yes | UDP upstream tests | `supported_difference` | Multi-hop UDP remains excluded. |
| UDP | Shadowsocks AEAD | yes | bounded | yes | Shadowsocks UDP tests | `supported_difference` | Standard AEAD framing; no legacy methods. |
| UDP | HTTP, SOCKS4, Trojan upstreams | diagnosed | diagnosed | no | translator rejection tests | `intentional_non_parity` | No usable UDP relay path. |
| Routing | URI host/port predicates and direct fallback | yes | yes | yes | routing tests | `matched` | Unmatched compatibility traffic falls through directly. |
| Routing | pproxy regex-line rule files | yes | yes | yes | rule translation tests | `matched` | Missing/malformed files fail clearly. |
| Routing | Native health-aware scheduler state | yes | yes | yes | scheduler/runtime tests | `native_extension` | Broader than pproxy's compatibility surface. |
| Reverse | `+in`, bind/listen, backward channel | yes | bounded | yes | reverse runtime/interoperability tests | `supported_difference` | TCP control-channel compositions only. |
| Reverse | Unsupported reverse compositions/backward TLS | diagnosed | diagnosed | no | reverse validation tests | `intentional_non_parity` | Fails before execution. |
| Transparent | Linux original-destination recovery | yes | bounded | yes | transparent runtime tests | `platform_limited` | Requires firewall setup and privileges. |
| Transparent | macOS PF recovery | diagnosed | diagnosed | no | platform capability tests | `intentional_non_parity` | No disposable PF recovery implementation. |
| Python package | `eggress` wheel plus top-level `pproxy` | yes | yes | yes | clean-wheel smoke tests | `matched` | One distribution; no separate compat wheel. |
| Python package | `pproxy.Connection` TCP echo | n/a | yes | yes | public namespace/connection tests | `supported_difference` | Native Eggress stream adapter. |
| Python package | Public UDP callback | n/a | yes | yes | public namespace tests | `supported_difference` | Not all private pproxy internals. |
| Python package | `pproxy.Server` start/close | n/a | yes | yes | server lifecycle tests | `supported_difference` | Native-backed lifecycle/observability. |
| Python package | `Rule` and `DIRECT` | n/a | yes | yes | public namespace tests | `matched` | Public factory and rule behavior. |
| Python package | `proto`, `cipher`, plugin facades | n/a | bounded | bounded | protocol/cipher/plugin tests | `supported_difference` | Importability does not imply wire parity. |
| Ciphers | AES-GCM and ChaCha20-Poly1305 AEAD | yes | yes | yes | KAT/protocol tests | `matched` | Modern methods are supported. |
| Ciphers | Legacy stream ciphers and OTA | diagnosed | diagnosed | no | legacy rejection tests | `intentional_non_parity` | Use an AEAD method. |

## Stable intentional exclusions

SSH, QUIC/HTTP/3, SSR, legacy Shadowsocks ciphers/OTA, unsupported plugins,
daemonization, cross-session connection reuse, unsupported reverse compositions,
general multi-hop UDP, and unavailable platform transparent facilities remain
explicit exclusions. Known syntax is diagnosed with an alternative, boundary,
or platform reason rather than silently selecting another protocol.
