# Migrating from pproxy to Eggress (Final)

This is the comprehensive migration guide for users moving from Python
`pproxy` 2.7.9 to the eggress parity release candidate (v0.1.0). It
covers direct CLI migration, TOML translation, protocol-by-protocol
notes, platform caveats, and verification steps.

**Source of truth:** `tests/compat/pproxy_manifest.toml`
**Frozen targets:** [PARITY_TARGET_FREEZE.md](PARITY_TARGET_FREEZE.md)
**Platform matrix:** [PLATFORM_SUPPORT_MATRIX.md](PLATFORM_SUPPORT_MATRIX.md)

## 1. Introduction

Eggress is a Rust-native, embeddable, multi-protocol proxy framework
targeting practical and behavioral parity with Python `pproxy` 2.7.9. It
provides:

- A CLI binary (`eggress`) with pproxy-compatible subcommands
- Native TOML configuration
- Python bindings via PyO3 (`pip install eggress`)
- Differential tests against pproxy 2.7.9 for behavioral verification

**Who this is for:** Operators and developers currently running pproxy
who want a Rust-native alternative with the same CLI workflow, protocol
support, and URI-driven configuration.

**This is a release candidate (v0.1.0), not general availability.** Some
differential tests have environmental constraints (Python 3.11 required
for pproxy 2.7.9 interop). Review the [release notes](RELEASE_NOTES_PARITY_RC.md)
before migrating production traffic.

## 2. Direct Migration (1:1 CLI Commands)

The following pproxy invocations translate directly to eggress with no
changes beyond the binary name:

| pproxy command | eggress equivalent |
|---|---|
| `pproxy -l socks5://:1080 -r direct` | `eggress pproxy run -- -l socks5://:1080 -r direct` |
| `pproxy -l http://:8080 -r socks5://proxy:1080` | `eggress pproxy run -- -l http://:8080 -r socks5://proxy:1080` |
| `pproxy -l socks4://:1080 -r http://upstream:8080` | `eggress pproxy run -- -l socks4://:1080 -r http://upstream:8080` |
| `pproxy -l socks5://:1080 -r http://a:8080 -r socks5://b:1080 -s rr` | `eggress pproxy run -- -l socks5://:1080 -r http://a:8080 -r socks5://b:1080 -s rr` |
| `pproxy -l http://:8080 -ul :1081 -ur socks5://proxy:1080` | `eggress pproxy run -- -l http://:8080 -ul :1081 -ur socks5://proxy:1080` |

**Flags that map directly:** `-l`, `-r`, `-ul`, `-ur`, `-s` (scheduler),
positional URIs.

**Flags that are unsupported or have special handling** (see [section 6](#6-unsupported--intentional-non-parity-features)):
`--daemon`, `--reuse`, `--alive`. Other flags map to TOML config:
`--ssl` (native-equivalent), `-b` (drop_in), `--rulefile`
(compatible_with_warning), `--log` (native-equivalent), `--sys`
(native-equivalent), `--pac` (native-equivalent), `--get`
(unsupported), `--test` (native-equivalent).

## 3. TOML Translation

### How `eggress pproxy translate` works

The `translate` subcommand converts pproxy-style CLI arguments into
eggress TOML configuration. The output can be saved and used with
`eggress --config`.

```bash
eggress pproxy translate -- -l socks5://:1080 -r http://proxy:8080
```

Output:

```toml
version = 1

[[listeners]]
name = "listener-0"
bind = "127.0.0.1:1080"
protocols = ["socks5"]

[[upstreams]]
id = "upstream-0"
uri = "http://proxy:8080"

[[upstream_groups]]
id = "upstream-group-0"
members = ["upstream-0"]
scheduler = "first-available"

[[rules]]
id = "default"
any = true
upstream_group = "upstream-group-0"

[routing]
default = "direct"
```

### Translated flags

| pproxy flag | TOML equivalent |
|---|---|
| `-l socks5://:1080` | `[[listeners]]` with `protocols = ["socks5"]` |
| `-r http://proxy:8080` | `[[upstreams]]` with `uri = "http://proxy:8080"` |
| `-r a://x -r b://y` | `[[upstream_groups]]` with `members` list |
| `-s rr` | `scheduler = "round-robin"` |
| `-ul :1081` | `mode = "standalone_pproxy_udp"` in `[listeners.udp]` |
| `-ur socks5://proxy:1080` | UDP upstream group with transport rule |
| `socks5://user:pass@:1080` | `[listeners.auth]` with credentials |

### Annotated output

Use `--annotate` to add explanatory comments:

```bash
eggress pproxy translate --annotate -- -l socks5://:1080 -r http://proxy:8080
```

### Check compatibility without translating

```bash
eggress pproxy check --json -- -l socks5://:1080 -r http://proxy:8080
```

Returns a JSON object with `tier`, `diagnostics`, and `features` fields.

## 4. Configuration Schema Differences

When translating complex pproxy configurations, be aware of these
structural differences:

| Aspect | pproxy | eggress |
|---|---|---|
| Config format | JSON-style or CLI flags | TOML |
| Listener definition | URI-based (`-l scheme://host:port`) | Structured `[[listeners]]` with `name`, `bind`, `protocols` |
| Upstream definition | URI-based (`-r scheme://host:port`) | `[[upstreams]]` with `id` and `uri` |
| Load balancing | `-s rr` / `-s fa` / `-s lc` | `[[upstream_groups]]` with `scheduler` field |
| Auth | Embedded in URI | `[listeners.auth]` or URI-embedded |
| TLS | `--ssl cert,key` | `[listeners.tls]` with `cert`, `key`, `alpn` |
| Routing rules | `-b regex` or `--rulefile` | `[[rules]]` with matchers (`host_suffix`, `host_regex`, `destination_port`, etc.) |
| Hot reload | SIGHUP (full config) | SIGHUP (routing/upstreams/groups/health only; listener topology requires restart) |
| Admin/monitoring | None | `[admin]` with `/metrics`, `/-/status`, PAC serving |
| Reverse proxy | `bind://`, `backward://` URI forms | `[[reverse_servers]]` and `[[reverse_clients]]` TOML sections |

### Features requiring manual TOML rewrite

If your pproxy setup uses these features, you must write eggress TOML
configuration by hand:

1. **TLS listeners** -- Configure `[listeners.tls]` in TOML
2. **Block regex rules** -- Use `[[rules]]` with `host_regex` and
   `reject` action
3. **Rule files** -- Use `[[rules]]` with structured matchers
4. **Daemon mode** -- Use systemd, supervisord, or another process manager
5. **Connection pooling** -- Not implemented (one upstream per session)
6. **Log files** -- Use `RUST_LOG` env var and shell redirection
7. **System proxy** -- Use `eggress system-proxy inspect` (read-only)

## 5. Protocol-by-Protocol Migration Notes

### HTTP CONNECT / Forward Proxy

**Status:** Compatible (manifest: `http_connect_server`, `http_forward_proxy`)

Eggress provides full HTTP CONNECT and persistent HTTP forward proxy.
Differential tests verify byte-exact payload match with pproxy 2.7.9.

| Scenario | pproxy | eggress |
|---|---|---|
| HTTP CONNECT tunnel | Yes | Yes (compatible) |
| HTTP forward proxy (GET/POST/HEAD) | Yes | Yes (persistent session model) |
| HTTP Basic auth | Yes | Yes (URI-embedded credentials) |
| Connection: close handling | Yes | Yes (compatible) |
| IPv4/IPv6/domain targets | Yes | Yes (compatible) |

**No migration changes needed.** The same URI forms work:
`-l http://:8080 -r direct`

### SOCKS4 / SOCKS4a

**Status:** Compatible (manifest: `socks4_server`, `socks4a_server`)

Differential tests with pproxy 2.7.9 added in Phase 19.

| Scenario | pproxy | eggress |
|---|---|---|
| SOCKS4 CONNECT (IPv4) | Yes | Yes (compatible) |
| SOCKS4a CONNECT (domain) | Yes | Yes (compatible) |
| SOCKS4 BIND | Yes | Not implemented (returns 0x07) |

**Migration note:** SOCKS4 BIND is not supported. If your workflow
relies on BIND, you need an alternative approach.

### SOCKS5 (TCP and UDP ASSOCIATE)

**Status:** Compatible for CONNECT; Supported for UDP ASSOCIATE
(manifest: `socks5_connect_server`, `socks5_udp_associate_server`)

| Scenario | pproxy | eggress |
|---|---|---|
| SOCKS5 CONNECT (IPv4/IPv6/domain) | Yes | Yes (compatible) |
| SOCKS5 username/password auth | Yes | Yes (compatible) |
| SOCKS5 auth rejection | Yes | Yes (compatible) |
| SOCKS5 UDP ASSOCIATE | Yes (custom framing) | Yes (SOCKS5 UDP ASSOCIATE framing) |
| SOCKS5 BIND | Yes | Not implemented (returns 0x07) |

**Migration notes:**
- UDP ASSOCIATE framing differs (pproxy uses custom framing; eggress
  uses standard SOCKS5 UDP ASSOCIATE). Both relay UDP successfully.
- Standalone UDP mode is available via `-ul`/`-ur` flags or
  `mode = "standalone_pproxy_udp"` in TOML.
- SOCKS5 BIND is not supported.

### Shadowsocks TCP / UDP

**Status:** Supported (manifest: `shadowsocks_tcp_upstream`, `shadowsocks_udp`)

Eggress uses standard SIP003 AEAD framing. It is wire-compatible with
standard Shadowsocks implementations (ssserver, sslocal, shadowsocks-rust).

| Scenario | pproxy | eggress |
|---|---|---|
| Shadowsocks TCP (AEAD) | Yes | Yes (standard SIP003) |
| Shadowsocks UDP (AEAD) | Yes | Yes (standard AEAD format) |
| Shadowsocks inbound listener | Yes | Yes (explicit protocol mode) |
| Legacy stream ciphers | Yes | **Not supported** (security) |
| ShadowsocksR (SSR) | Yes | **Not supported** (non-standard) |

**Migration notes:**
- **Only AEAD methods** are supported: `aes-128-gcm`, `aes-256-gcm`,
  `chacha20-ietf-poly1305`. Legacy stream ciphers produce
  `LegacyMethodUnsupported` errors.
- SSR URIs (`ssr://`) are rejected with `SsrUnsupported` errors.
- Shadowsocks TCP framing uses standard SIP003 AEAD (two AEAD
  operations per chunk, encrypted length). This is a change from
  earlier eggress versions that used non-standard framing.

### Trojan (Client Only)

**Status:** Partial (manifest: `trojan_upstream`, `trojan_upstream_client`)

| Scenario | pproxy | eggress |
|---|---|---|
| Trojan upstream (client) | Yes | Yes (client only) |
| Trojan inbound listener | Yes | **Not supported** (upstream-only) |
| Trojan UDP | No | No |

**Migration note:** Trojan can only be used as an upstream protocol.
There is no Trojan server/listener. Use `trojan://password@host:443`
as an `-r` argument.

### Reverse / Backward Proxy

**Status:** Supported (manifest: `backward_tcp_control`, `backward_auth`)

| Scenario | pproxy | eggress |
|---|---|---|
| TCP reverse relay | Yes | Yes (raw-relay model) |
| Plaintext auth | Yes | Yes (raw user:pass bytes) |
| TLS on control channel | Yes | **Not supported** (use stunnel) |
| UDP reverse | No | No |
| Jump chains through reverse | Yes | **Not supported** |
| Multiple parallel control connections | Yes | **Not supported** (one per channel) |

**Migration notes:**
- The on-wire protocol matches pproxy's raw-relay format (1-byte
  handshake + raw auth bytes + bidirectional TCP relay).
- TLS must be added externally (stunnel, haproxy, WireGuard).
- Each control channel carries one session (no multiplexing).

## 6. Unsupported / Intentional Non-Parity Features

These pproxy features are deliberately not replicated in eggress:

| Feature | Rationale | eggress alternative |
|---|---|---|
| `--daemon` mode | Design choice | Use systemd or process manager |
| `--ssl` TLS listeners | Native-equivalent | Generates TLS listener TOML config via `eggress pproxy run --ssl` |
| `-b` block regex rules | Drop-in | Generates `[[rules]] reject` entries via `eggress pproxy run -b` |
| `--rulefile` | Compatible with warning | Translates pproxy rulefiles to `[[rules]]` with diagnostics for untranslatable patterns |
| `--reuse` (connection pooling) | Design choice | One upstream connection per session |
| `--log` file | Native-equivalent | Emits structured diagnostic |
| `--sys` (system proxy mutation) | Native-equivalent | Auto-invokes `eggress system-proxy inspect` before starting |
| `--alive` check interval | Design choice | Configure `[upstreams.health]` in TOML |
| SOCKS4/SOCKS5 BIND | Deferred | Returns `REP_COMMAND_NOT_SUPPORTED` (0x07) |
| Multi-hop UDP chains | Architecture | Single-hop only |
| SSH transport | Out of scope | SSH is not a proxy protocol |
| Legacy stream ciphers | Security | Use AEAD methods only |
| ShadowsocksR (SSR) | Non-standard | Use standard Shadowsocks |
| QUIC / HTTP/3 | Deferred | See ADR at `docs/adr/ADR_quic_h3_pproxy_parity.md` |
| Persistent connection pooling | Design choice | One upstream connection per session |
| macOS PF transparent proxy | Not implemented | Use pfctl with standard listener |

## 7. Python API Migration

### Package installation

```bash
# From PyPI
pip install eggress

# From wheel
pip install dist/eggress-*.whl
```

### API mapping

| pproxy Python | eggress Python |
|---|---|
| `pproxy.Server(uris)` | `eggress.Server(listen=[...], remote=[...])` |
| `pproxy.Connection(config)` | `eggress.EggressConfig.from_toml(toml)` |
| `server_forever()` | `EggressService(config).start()` or `Server.run()` |
| (no context manager) | `with EggressService(...).start() as handle:` |
| (no async CM) | `async with await EggressService(...).astart() as handle:` |
| Manual shutdown | `handle.shutdown()` (idempotent) |

### Translation helpers

```python
from eggress import translate_pproxy_args, check_pproxy_args

# Translate CLI args
result = translate_pproxy_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"])
print(result.toml)

# Start from pproxy args
from eggress import start_pproxy
with start_pproxy(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
    print(handle.bound_addresses)
```

### Key differences

| Aspect | pproxy | eggress |
|---|---|---|
| GIL handling | N/A (pure Python) | GIL released on all blocking Rust calls |
| Error types | Generic `Exception` | 7 typed exceptions (`ConfigError`, `StartupError`, etc.) |
| Context managers | Not available | Sync and async context managers |
| Hot reload | Not available | `handle.reload_toml(new_config)` |
| Metrics | Not available | `handle.metrics_text()` (Prometheus format) |
| Protocol classes | `pproxy.proto.Http`, etc. | Available as metadata objects via `eggress.protocol` (Phase C4) |
| Cipher classes | `pproxy.cipher.AEAD`, etc. | Available as metadata objects via `eggress.cipher` (Phase C4) |

### URI inspection

```python
from eggress import check_pproxy_uri, redact_pproxy_uri, diagnostics_for_uri

info = check_pproxy_uri("socks5://user:pass@example.com:1080+tls")
print(info.scheme)    # "socks5"
print(info.has_auth)  # True
print(info.tls)       # True

print(redact_pproxy_uri("socks5://secret:word@proxy:1080"))
# "socks5://****:****@proxy:1080"

diags = diagnostics_for_uri("ssh://proxy:22")
for d in diags:
    print(f"[{d.code}] {d.message}")
```

## 8. Platform Caveats

### Linux

- **Transparent TCP proxy** (`SO_ORIGINAL_DST`) requires `CAP_NET_ADMIN`
  or root, plus iptables/nftables REDIRECT rules.
- All features available.
- See [PLATFORM_SUPPORT_MATRIX.md](PLATFORM_SUPPORT_MATRIX.md) for
  per-feature details.

### macOS

- **No PF transparent proxy** (`intentional_non_parity`). Use `pfctl`
  with a standard listener instead.
- System proxy inspection uses `networksetup`.
- System proxy apply/revert requires explicit `--apply` flag.
- All other features available.

### Windows

- **No Unix domain socket listeners** (`intentional_non_parity`).
- **No transparent proxy** (`intentional_non_parity`).
- System proxy inspection reads the registry.
- All other features available.

### Python differential tests

Differential tests against `pproxy==2.7.9` require **Python 3.11**.
pproxy 2.7.9 uses `asyncio.get_event_loop()` which raises on
Python 3.14. The eggress Python package itself runs on Python 3.9+.

## 9. Security Warnings

1. **No admin authentication**: Admin endpoints (`/-/status`,
   `/-/metrics`) are bound to `127.0.0.1` by default. If you bind admin
   to `0.0.0.0`, it is exposed without auth.

2. **Reverse control channel is plaintext**: The backward/reverse control
   channel sends auth as raw `user:pass` bytes. Use TLS via stunnel or
   equivalent when traversing untrusted networks.

3. **Shadowsocks AEAD only**: Legacy stream ciphers are intentionally
   rejected. They have no authentication and are vulnerable to
   bit-flipping attacks.

4. **Raw tunnels have no auth or encryption**: Network-level access
   control is the operator's responsibility.

5. **Credentials in TOML**: Credentials are stored in plaintext in
   configuration files. Use `auth_password_env` for environment-based
   injection where possible.

6. **Non-loopback binds**: Binding listeners to `0.0.0.0` exposes the
   proxy to the network without built-in access control.

7. **No connection rate limiting**: No request rate limiting on any
   protocol or admin endpoint.

See [SECURITY_REVIEW.md](../../SECURITY_REVIEW.md) for the full threat
model and mitigations.

## 10. Performance Expectations

Eggress is a Rust-native proxy with Tokio async runtime. Expected
characteristics:

- **TCP relay latency**: Sub-millisecond for local relay
- **UDP relay**: Handles 100+ datagrams per second through standalone mode
- **Concurrent connections**: 50+ simultaneous SOCKS5 sessions under 5s
  (Tier 1 performance smoke)
- **Resource cleanup**: FD count returns to baseline after session drain;
  no task leaks

Differential performance comparison with pproxy is deferred to Tier 3
benchmarking. No regression from Phase 34 baselines is expected.

## 11. Known Limitations

1. **SOCKS4/SOCKS5 BIND** not implemented (returns 0x07).
2. **Multi-hop UDP chains** not supported (single-hop only).
3. **Connection pooling** not implemented (one upstream per session).
4. **Hot reload** is routing-only; listener topology changes require
   restart.
5. **Trojan** is upstream-only (no server/listener).
6. **macOS PF transparent proxy** not implemented.
7. **QUIC/HTTP/3** deferred by ADR.
8. **mypy false positives** (~20 expected errors from PyO3 native types).
9. **Hosted CI** is non-functional (billing issues); local verification
   is the source of truth.
10. **pproxy `--alive`** interval flag produces a warning but does not
    generate health probe config.

## 12. CLI Translation Examples

### Example 1: Simple SOCKS5 proxy

```bash
# pproxy
pproxy -l socks5://127.0.0.1:1080

# eggress (direct)
eggress pproxy run -- -l socks5://127.0.0.1:1080

# eggress (native TOML)
eggress --config config.toml
```

### Example 2: SOCKS5 through HTTP upstream

```bash
# pproxy
pproxy -l socks5://:1080 -r http://proxy:8080

# eggress
eggress pproxy run -- -l socks5://:1080 -r http://proxy:8080
```

### Example 3: Multiple upstreams with round-robin

```bash
# pproxy
pproxy -l socks5://:1080 -r http://a:8080 -r socks5://b:1080 -s rr

# eggress
eggress pproxy run -- -l socks5://:1080 -r http://a:8080 -r socks5://b:1080 -s rr
```

### Example 4: Standalone UDP relay

```bash
# pproxy
pproxy -l http://:8080 -ul :1081 -ur socks5://proxy:1080

# eggress
eggress pproxy run -- -l http://:8080 -ul :1081 -ur socks5://proxy:1080
```

### Example 5: Shadowsocks server

```bash
# pproxy
pproxy -l ss://aes-256-gcm:pass@:8388 -r direct

# eggress
eggress pproxy run -- -l ss://aes-256-gcm:pass@:8388 -r direct
```

### Example 6: Authenticated listener

```bash
# pproxy
pproxy -l socks5://admin:secret@:1080 -r direct

# eggress
eggress pproxy run -- -l socks5://admin:secret@:1080 -r direct
```

### Example 7: Chain (SOCKS5 through HTTP then SOCKS5)

```bash
# pproxy (using __ separator)
pproxy -l http://:8080 -r socks5://hop1:1080__http://hop2:8080

# eggress (using __ separator in -r)
eggress pproxy run -- -l http://:8080 -r socks5://hop1:1080__http://hop2:8080
```

### Example 8: Transparent proxy (Linux only)

```bash
# pproxy
pproxy -l redir://0.0.0.0:8080 -r direct

# eggress
eggress pproxy run -- -l redir://0.0.0.0:8080 -r direct
```

### Example 9: Reverse proxy (backward client)

```bash
# pproxy
pproxy -l socks5://:1080 -r socks5+in://acceptor:8080

# eggress
eggress pproxy run -- -l socks5://:1080 -r socks5+in://acceptor:8080
```

### Example 10: Translate and save to file

```bash
# Generate TOML
eggress pproxy translate -- -l socks5://:1080 -r http://proxy:8080 > config.toml

# Verify
eggress pproxy check --json -- -l socks5://:1080 -r http://proxy:8080

# Run from file
eggress --config config.toml
```

## 13. How to Verify Parity

### Differential tests (requires pproxy 2.7.9 + Python 3.11)

```bash
python -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
```

### Shadowsocks interop tests

```bash
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
```

### Reverse proxy interop tests

```bash
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1
```

### Python binding tests

```bash
python -m pytest python/tests/test_pproxy_compat.py -v
python -m pytest python/tests/test_pproxy_redaction.py -v
python -m pytest python/tests/test_pproxy_concurrency.py -v
```

### Full workspace validation

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo audit
```

### Manifest validation

```bash
cargo test -p eggress-testkit --lib manifest
cargo test -p eggress-testkit --lib corpus
```

## 14. Rollback Plan

If eggress does not meet your needs after migration:

1. **Stop eggress**: `kill <pid>` or `eggress` will exit on SIGINT/SIGTERM.
2. **Restore pproxy**: `pip install "pproxy==2.7.9"`
3. **Revert config**: pproxy uses its own config format; keep your
   original pproxy configuration files.
4. **No data migration**: eggress does not modify or store state beyond
   runtime. No data rollback is needed.
5. **DNS/firewall**: If you changed firewall rules for transparent proxy
   on Linux, revert iptables/nftables REDIRECT rules.

## 15. References

- [PARITY_TARGET_FREEZE.md](PARITY_TARGET_FREEZE.md) -- Frozen target versions
- [PLATFORM_SUPPORT_MATRIX.md](PLATFORM_SUPPORT_MATRIX.md) -- Per-platform feature availability
- [RELEASE_NOTES_PARITY_RC.md](RELEASE_NOTES_PARITY_RC.md) -- Release notes
- [CONFIG_REFERENCE.md](../../CONFIG_REFERENCE.md) -- Full TOML configuration reference
- [PPROXY_MIGRATION.md](../../PPROXY_MIGRATION.md) -- Original migration guide
- [PPROXY_PARITY_SPEC.md](../../PPROXY_PARITY_SPEC.md) -- Formal parity specification
- [PARITY_MATRIX.md](../../PARITY_MATRIX.md) -- Feature-by-feature comparison matrix
- [COMPATIBILITY_EVIDENCE.md](../../COMPATIBILITY_EVIDENCE.md) -- Evidence table with test commands
- [PYTHON_BINDINGS.md](../../PYTHON_BINDINGS.md) -- Python API reference
- [EMBED_API.md](../../EMBED_API.md) -- Rust embed API reference
- [SECURITY_REVIEW.md](../../SECURITY_REVIEW.md) -- Threat model and mitigations
- [EXIT_CODES.md](../cli/EXIT_CODES.md) -- CLI exit code reference
- [PPROXY_CLI_INVENTORY.md](../cli/PPROXY_CLI_INVENTORY.md) -- Full CLI flag inventory
