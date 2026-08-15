# Practical pproxy 2.7.9 compatibility matrix

This maintained matrix describes observable Eggress compatibility with the
frozen `pproxy==2.7.9` tag at commit
`09d4752f17ed6787e1a073c93980eec019887ee3`. It makes no aggregate parity
claim. Status vocabulary is defined in [`README.md`](README.md), and the
machine-readable inventory is the canonical source for detailed evidence.

## CLI surface declared by the 2.7.9 parser

The tagged parser is `pproxy/server.py:895-913`. Each real option appears once
below. `--log`, `-f/--config`, and `--rulefile` are deliberately absent: they
are not 2.7.9 parser options. Eggress may expose native or compatibility
extensions with similar names, but those do not become pproxy claims.

| Option | Surface | Eggress status | Evidence / notes |
|---|---|---|---|
| `-l` | TCP listener URI, repeatable | `matched` | `cli.listen`; translation and listener tests |
| `-r` | TCP remote URI, repeatable | `matched` | `cli.remote`; chain translation/runtime tests |
| `-ul` | UDP listener URI, repeatable | `matched` | `cli.udp_listen`; standalone UDP tests |
| `-ur` | UDP remote URI, repeatable | `matched` | `cli.udp_remote`; UDP upstream tests |
| `-b` | Block regex | `supported_difference` | `cli.block`; lowered to native reject rules |
| `-a` | Alive-check interval | `supported_difference` | `cli.alive`; native health state differs |
| `-s` | `fa`, `rr`, `rc`, or `lc` scheduler | `supported_difference` | `cli.scheduler`; native scheduler mapping |
| `-d` | Debug traceback mode | `supported_difference` | `cli.debug`; Rust tracing defaults, not Python traceback semantics |
| `-v` | Verbose output, repeatable | `supported_difference` | `cli.verbose`; `-v/-vv/-vvv` tracing defaults |
| `--ssl` | Listener certificate/key | `supported_difference` | `cli.ssl_listener`; native TLS configuration |
| `--pac` | PAC path | `supported_difference` | `cli.pac`; mapped to native admin serving |
| `--get` | Static path/file, repeatable | `supported_difference` | `cli.get`; native admin static content |
| `--auth` | Per-source-IP re-auth interval | `supported_difference` | `cli.auth`; bounded process-local IP cache with monotonic expiry |
| `--sys` | Apply system proxy settings | `supported_difference` | `cli.sys`; existing backend, post-bind apply, in-memory rollback; macOS/Windows strict targets |
| `--reuse` | `SO_REUSEPORT` | `matched` | `cli.reuse`; platform behavior is documented |
| `--daemon` | Daemonize process | `gap` | `cli.daemon`; Phase 9 process-model work |
| `--test` | Test supplied URL and exit | `supported_difference` | `cli.test`; in-process native upstream test |
| `--version` | Print version and exit | `matched` | `cli.version` |
| `-h / --help` | Argparse help | `matched` | `cli.help` |

Eggress-only CLI extensions such as `--config`, `--log`, `--rulefile`, and
`pproxy check --json` are documented as extensions, not counted as upstream
flags. The compatibility parser can retain them without changing this oracle
contract.

## Protocol, URI, and composition surface

| Surface | Status | Evidence / boundary |
|---|---|---|
| HTTP/HTTPS, SOCKS4/4a, SOCKS5 CONNECT | `matched` | URI, protocol, and paired compatibility tests |
| SOCKS5 username/password authentication | `matched` | Success and failure differential cases |
| SOCKS5 UDP ASSOCIATE | `supported_difference` | Public framing/relay boundary is narrower |
| Direct TCP/UDP and HTTP/SOCKS upstreams | `matched` | Runtime and differential tests |
| pproxy backward/reverse raw channels | `supported_difference` | Separate raw-auth adapter, reconnect and jump tests; QUIC marker deferred |
| UDP chains of SOCKS5/Shadowsocks hops | `supported_difference` | Composable codecs and runtime flow tests; unsupported protocols fail validation |
| TCP `__` chains and routing predicates | `supported_difference` | Native chain model preserves supported compositions |
| Shadowsocks AEAD TCP/UDP | `supported_difference` | Four modern methods with method-specific salts; standard UDP upstream plus exact pproxy PacketCipher standalone inbound path |
| Trojan client/server roles | `supported_difference` | Native implementation; no private pproxy API claim |
| H2 and WS/WSS | `supported_difference` | Upstream and compatibility listener roles; H2 multiplexes CONNECT streams, WS/WSS use fixed targets |
| SSR framing and built-in plugins | `gap` | Exact tagged behavior is Phase 3 |
| SSH upstream/jump/remote-forward | `gap` | Phase 7 optional transport |
| QUIC/H3 listener/client/UDP roles | `gap` | Phase 8 optional transport |
| Legacy stream ciphers and OTA | `gap` | Phase 9 legacy tail |
| SOCKS4/SOCKS5 BIND | `unsupported` by both | Tagged accept paths require CONNECT (`0x01`); not Eggress-specific strict work |
| Linux redir | `platform_limited` | Requires original-destination facilities and privileges |
| macOS PF | `gap` | Phase 9 platform tail |

## Python package inventory

The exact tagged package contains ten module files: `__init__`, `__doc__`,
`__main__`, `cipher`, `cipherpy`, `plugin`, `proto`, `server`, `sysproxy`, and
`verbose`. The Eggress wheel ships all ten modules. The adapters preserve the
tracked imports and callable shapes while keeping networking, cipher
execution, and system-proxy mutation in the existing native/runtime paths.
Legacy pure-Python cipher names remain import-compatible but fail explicitly
when constructed; they are not silently substituted.

Run the compact comparison probe with an isolated oracle interpreter and then
with the interpreter containing the Eggress wheel:

```bash
.venv-oracle/bin/python scripts/pproxy_surface_probe.py > /tmp/pproxy-2.7.9.json
.venv/bin/python scripts/pproxy_surface_probe.py > /tmp/eggress-pproxy.json
```

The focused source/wheel contract is covered by
`python/tests/test_pproxy_phase4_contract.py`, including `python -m pproxy
--version` and the native system-proxy bridge boundary.

See [`pproxy_capability_manifest.toml`](pproxy_capability_manifest.toml) for
the per-module, per-capability status, source evidence, implementation path,
strict phase, and focused test references.

## Stable exclusions and boundaries

SSH, QUIC/HTTP/3, SSR, exact plugin transforms, legacy Shadowsocks ciphers and
OTA, daemonization, UDP over non-UDP protocols, and unavailable platform transparent facilities remain
explicit strict gaps or exclusions according to their phase records. Unknown
flags and unsupported options fail with structured diagnostics; known Eggress
extensions do not alter the frozen upstream inventory.
