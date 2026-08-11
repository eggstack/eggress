# Practical pproxy 2.7.9 compatibility matrix

This maintained matrix describes observable Eggress compatibility with
`pproxy==2.7.9`; it makes no aggregate parity claim. Status labels are defined
in [`README.md`](README.md). A skipped oracle run is not evidence of a match.

| Surface | Capability | CLI | Python | Runtime | Evidence | Status | Notes |
|---|---|---:|---:|---:|---|---|---|
| URI grammar | HTTP/HTTPS, SOCKS4/4a, SOCKS5, direct | yes | yes | yes | compat tests; differential harness | `matched` | Common listener/upstream forms. |
| URI grammar | Combined protocols and `__` chains | yes | yes | yes | URI/translation tests | `matched` | Native chain validation still applies. |
| URI grammar | `+tls`, `+ssl`, repeated `+in` | yes | yes | yes | URI regression tests | `matched` | Modifiers are preserved before lowering. |
| URI grammar | Fragment auth, local bind, canonical fixed target, plugin/rule metadata | yes | yes | yes | parser/translation/config tests | `supported_difference` | `tunnel{host:port}://listener` is canonical; legacy `raw://{host:port}` remains an extension. Unusable metadata is diagnosed. |
| URI grammar | H2, WS/WSS, raw, tunnel upstreams | yes | yes | yes | advanced transport tests | `supported_difference` | H2/WS/WSS are upstream-only; raw/tunnel forms are bounded. |
| URI grammar | Unix-domain TCP upstream | yes | yes | platform-dependent | URI/config tests | `platform_limited` | Unix only; Windows reports a platform diagnostic. |
| CLI | No-argument mixed-listener default | yes | n/a | yes | `pproxy_binary`, CLI tests | `matched` | Uses the documented HTTP/SOCKS default. |
| CLI | `-l`, `-r`, `-ul`, `-ur` | yes | yes | yes | CLI translation tests | `matched` | Values remain associated with their flags. |
| CLI | `-f/--config` flag ownership | yes | yes | yes | `cli.config` manifest entry | `compatible_with_warning` | Flag recognized; schema differs so pproxy config files cannot be used unchanged. |
| CLI | `--pac <path>` ownership and PAC mapping | yes | yes | yes | `test_pac_flag_emits_warning`, parser tests | `supported_difference` | Consumes the path and maps it to the Eggress admin PAC route. |
| CLI | `--get <path,file>` static content | yes | bounded | yes | `test_valid_get_static_content_is_supported` | `supported_difference` | Valid values are served through native admin static content; malformed or unreadable values fail closed. |
| CLI | `--test <target>` ownership and delegation | yes | yes | yes | `test_target_preserves_value_taking_option`, CLI command tests | `supported_difference` | Consumes the target and both compatibility entry points pass it to native upstream testing. |
| CLI | `-s`, `-a`, `-b`, `--rulefile`, `--ssl` | yes | yes | yes | translation/config tests | `supported_difference` | Lowered to native routing, health, TLS, and rules. |
| CLI | `--sys` | yes | n/a | no | unsupported diagnostic tests | `unsupported` | `--sys` is unsupported in pproxy compatibility mode; fails before startup. Use native `eggress system-proxy inspect` for read-only inspection. |
| CLI | `-d` debug/traceback diagnostics | yes | n/a | yes | `test_debug_flag_emits_compatible_warning`, CLI diagnostic tests | `compatible_with_warning` | Emits `debug-mode`; selects a debug-level default tracing filter, but does not reproduce Python traceback semantics. Independent of `-v` and `--daemon`; explicit `RUST_LOG` remains authoritative. |
| CLI | `--daemon` | yes | n/a | no | unsupported fatal tests | `unsupported` | Fatal (exit code 5) before startup. Use a process manager. |
| CLI | Unknown flags | yes | n/a | no | fatal diagnostic tests | `unsupported` | Fatal with exit code 2 in both the standalone binary and `eggress pproxy run`. Report-class identifier `unknown-flag`. |
| CLI | `--reuse` (SO_REUSEPORT) | yes | n/a | yes | listener socket tests | `supported_difference` | Configures SO_REUSEPORT on listener sockets; not connection pooling. |
| CLI | `--auth` | yes | n/a | no | unsupported fatal tests | `unsupported` | Validated and classified as unsupported (per-client auth reuse). |
| CLI | `--log` log file | yes | n/a | no | `test_log_flag_emits_warning` | `compatible_with_warning` | Flag recognized and parsed; no file is written. Logs go to stderr via tracing-subscriber. |
| Inbound | HTTP/HTTPS CONNECT and forward proxy | yes | yes | yes | runtime/differential tests | `matched` | HTTPS is TLS-wrapped HTTP tunneling. |
| Inbound | SOCKS4/4a | yes | yes | yes | runtime/differential tests | `matched` | SOCKS4a domain targets are covered. |
| Inbound | SOCKS5 CONNECT and username/password auth | yes | yes | yes | runtime/differential tests | `matched` | BIND is unsupported on both pproxy and Eggress. |
| Inbound | SOCKS5 UDP ASSOCIATE | yes | bounded | yes | UDP runtime/differential tests | `supported_difference` | Public API and framing boundaries are documented. |
| Inbound | Shadowsocks AEAD and Trojan | yes | bounded | yes | protocol/runtime tests | `supported_difference` | Native implementations; no strict private-API claim. |
| Inbound | TCP/UDP echo and fixed-target raw/tunnel | yes | bounded | yes | corrective parser/config/runtime tests | `supported_difference` | TCP is retained independently; UDP requires explicit `-ul` configuration. |
| Inbound | Unix socket | yes | bounded | yes | Unix listener tests | `platform_limited` | Unix filesystem listener. |
| Inbound | transparent `redir` | yes | bounded | platform-dependent | transparent tests | `platform_limited` | Linux original-destination facilities required. |
| Upstream | direct, HTTP/HTTPS, SOCKS5 | yes | yes | yes | runtime/differential tests | `matched` | Core TCP workflows. |
| Upstream | SOCKS4/4a | yes | yes | yes | upstream protocol tests | `supported_difference` | Supported with narrower exercised coverage. |
| Upstream | Shadowsocks AEAD and Trojan | yes | bounded | yes | protocol tests | `supported_difference` | Native wire implementations. |
| Upstream | H2 and WS/WSS | yes | bounded | yes | advanced transport tests | `supported_difference` | Upstream-only compatibility bridge. |
| Upstream | raw/tunnel fixed target and local source bind | yes | bounded | yes | raw/tunnel/local-bind tests | `supported_difference` | Bind applies to the first physical outbound socket; arbitrary multi-hop tunnel semantics are excluded. |
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
| Python package | `pproxy.Connection` TCP echo | n/a | yes | yes | public namespace/connection tests | `supported_difference` | Top-level `pproxy.Connection` is a pproxy-shaped URI factory; the underlying stream adapter is the Eggress implementation. |
| Python package | Public UDP callback | n/a | yes | yes | public namespace tests | `supported_difference` | Not all private pproxy internals. |
| Python package | `pproxy.Server` start/close and HTTP/SOCKS upstream routing | n/a | yes | yes | lifecycle/handshake-counting tests | `supported_difference` | Top-level `pproxy.Server` is a pproxy-shaped URI factory alias; the compatibility server path is a Python adapter using `asyncio.start_server` and `_eggress_stream_handler`. It is NOT the native `eggress.pproxy.Server` lifecycle. |
| Python package | `Rule` and `DIRECT` | n/a | yes | yes | public namespace tests | `matched` | Public factory and rule behavior. |
| Python package | `proto`, `cipher`, plugin facades | n/a | bounded | bounded | protocol/cipher/plugin tests | `supported_difference` | Importability does not imply wire parity. |
| Ciphers | AES-GCM and ChaCha20-Poly1305 AEAD | yes | yes | yes | KAT/protocol tests | `matched` | Modern methods are supported. |
| Ciphers | Legacy stream ciphers and OTA | diagnosed | diagnosed | no | legacy rejection tests | `intentional_non_parity` | Use an AEAD method. |

## Stable intentional exclusions

SSH, QUIC/HTTP/3, SSR, legacy Shadowsocks ciphers/OTA, unsupported plugins,
daemonization (`--daemon`), per-client auth reuse (`--auth`), system proxy apply
via pproxy compat (`--sys`), unsupported reverse compositions, general multi-hop
UDP, SOCKS4/SOCKS5 BIND, and unavailable platform transparent facilities remain
explicit exclusions. Unknown flags and unsupported options are fatal (exit code 2
or 5) rather than warnings. Known syntax is diagnosed with an alternative,
boundary, or platform reason rather than silently selecting another protocol.
