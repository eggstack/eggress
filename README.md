# eggress

A Rust-native, embeddable, multi-protocol proxy framework and CLI targeting practical and behavioral parity with Python `pproxy`.

> **Status:** The Rust-native CLI and runtime are production-ready. The Python compatibility surface is a bounded drop-in subset for `pproxy==2.7.9`; strict full parity is not claimed. Eggress ships one Python distribution, `eggress`, which also installs a real top-level `pproxy` compatibility package. Common HTTP/SOCKS and modern encrypted-proxy workflows are supported, while SSH, QUIC/HTTP/3, SSR families outside the bounded TCP compatibility path, daemonization, and cross-session reuse remain intentional exclusions. See `docs/PPROXY_PARITY_SPEC.md` and `docs/parity/pproxy_capability_manifest.toml`.

eggress preserves the compact URI-driven workflow of `pproxy` while using explicit Rust abstractions for listeners, application proxy protocols, transport wrappers, routing, proxy chains, UDP associations, and platform integration.

## Design goals

- Nearly identical common CLI usage to `pproxy`
- Mixed-protocol listeners
- Arbitrary compatible multi-hop proxy chains
- TCP and UDP
- Secure defaults with explicit legacy compatibility
- Embeddable Rust library
- Resource-bounded hostile network input; trusted operator configuration may use compatibility features with documented computational cost
- Pure Rust dependencies wherever practical
- Differential interoperability tests against Python `pproxy`
- Linux, macOS, and Windows support where the underlying capability exists

## Installation

### CLI

```bash
cargo install --path crates/eggress-cli
```

This installs both the `eggress` and `pproxy` binaries.

### Lean local build

For a smaller binary without the extended protocol families (Shadowsocks, Trojan,
WebSocket), reverse runtime, system-proxy integration, or compatibility layers:
admin and metrics remain compiled because they share the runtime snapshot.

```bash
# Lean HTTP/SOCKS local proxy
cargo build -p eggress-cli --release --no-default-features --features common

# Optional smallest optimization profile
cargo build -p eggress-cli --profile release-small --no-default-features --features common
```

### Rust library

Add `eggress-embed` to your `Cargo.toml`:

```toml
[dependencies]
eggress-embed = { path = "crates/eggress-embed" }
```

### Python package

```bash
pip install eggress

# For AEAD cipher support:
pip install "eggress[cipher-api]"
```

Programs using the bounded public `pproxy` API can keep `import pproxy` after
installing Eggress. The distribution is still named `eggress`; uninstall the
upstream `pproxy` distribution first because both wheels provide the same import
namespace. The wheel includes the complete Phase 0 module namespace, and both
`python -m pproxy` and the installed `pproxy` console script use the same
Rust-backed compatibility entry point. The explicit `from eggress import pproxy`
translation helpers remain available for migration-oriented code.

Supported Python versions: 3.9, 3.10, 3.11, 3.12, 3.13.

Prebuilt wheels are available for:
- Linux x86_64 and aarch64 (manylinux2014 / glibc 2.17 floor)
- macOS x86_64 and arm64
- Windows x86_64

Other platforms (e.g. ARM, musllinux, FreeBSD) can build from the source distribution. The wheel uses the Python stable ABI (abi3-py39) so one wheel per platform supports all declared Python versions.

## CLI usage

```text
eggress
eggress -l http://:8080
eggress -l socks4://:1080
eggress -l socks5://:1080
eggress -l http+socks4+socks5://:8080
eggress -l http+socks5://user:pass@:8080
eggress -r http://proxy.example:8080
eggress -r socks5://proxy.example:1080
eggress -r socks5://hop1:1080__http://hop2:8080
```

For pproxy users, a drop-in `pproxy` binary is also available:

```bash
pproxy -l http://:8080 -r socks5://proxy:1080
pproxy translate -- -l http://:8080 -r socks5://proxy:1080
pproxy check -- -l socks5://:1080 -r http://proxy:8080
```

The compatibility binary follows pproxy's no-argument default:
`http+socks4+socks5://:8080` with direct routing. Compatibility URIs preserve
combined protocols, fragment authentication, local outbound binding, plugins,
fixed targets, and raw rule suffixes for translation diagnostics. `--pac
<path>`, `--get <path,file>`, and `--test <target>` each consume their value;
they are not positional proxy URIs. PAC and valid GET values lower through the
admin server, while TEST delegates the exact target to native upstream testing.
PAC serving and verbosity are supported with compatibility warnings: PAC is
served through the mapped admin route, and `-v/-vv/-vvv` select Rust tracing
defaults while explicit `RUST_LOG` remains authoritative.
Canonical fixed-target listeners use
`tunnel{host:port}://:listen-port`; the legacy `raw://{host:port}` extension
remains accepted. UDP fixed-target and echo listeners require an explicit
`-ul` URI, keeping TCP and UDP listener roles independent.

## Rust library

Use `eggress-embed` to embed the proxy in another Rust application.

### Blocking usage

```rust
use eggress_embed::{EggressService, EggressConfig};

let config = EggressConfig::from_toml_str(r#"
    version = 1

    [[listeners]]
    name = "socks"
    bind = "127.0.0.1:0"
    protocols = ["socks5"]
"#)?;

let handle = EggressService::new(config).start_blocking()?;

// Discover bound addresses (supports port-0)
let addrs = handle.bound_addresses();
let socks_addr = addrs.listener("socks").unwrap();
println!("SOCKS5 listening on {socks_addr}");

// Check status
let status = handle.status();
println!("generation: {}, readiness: {}", status.generation, status.readiness);

// Get Prometheus metrics
let metrics = handle.metrics_text()?;
assert!(metrics.contains("eggress_connections_total"));

// Shutdown
handle.shutdown_blocking()?;
```

### Async usage

```rust
use eggress_embed::{EggressService, EggressConfig};

let config = EggressConfig::from_toml_str(r#"
    version = 1

    [[listeners]]
    name = "http"
    bind = "127.0.0.1:0"
    protocols = ["http"]
"#)?;

let handle = EggressService::new(config).start().await?;

let status = handle.status();
println!("generation: {}", status.generation);

handle.shutdown().await?;
```

### Hot-reload

Reload routing and upstreams without restarting:

```rust
let new_config = r#"
    version = 1

    [[listeners]]
    name = "http"
    bind = "127.0.0.1:0"
    protocols = ["http"]
"#;

match handle.reload_toml_str(new_config) {
    Ok(eggress_embed::ReloadOutcome::Applied { generation, upstreams }) => {
        println!("reloaded: generation={generation}, upstreams={upstreams}");
    }
    Err(e) => eprintln!("reload failed: {e}"),
}
```

### Redacted config output

Display config without leaking credentials:

```rust
let config = EggressConfig::from_toml_str(r#"
    version = 1

    [[listeners]]
    name = "socks"
    bind = "127.0.0.1:0"
    protocols = ["socks5"]

    [listeners.auth]
    type = "password"
    username = "admin"
    password = "super_secret_123"
"#)?;

let redacted = config.to_redacted_toml()?;
assert!(!redacted.contains("super_secret_123"));
assert!(redacted.contains("****"));
```

See `docs/EMBED_API.md` for the full embed API reference.

## Python library

Use the `eggress` Python package to embed the proxy or use pproxy-compatible APIs.

### Context manager (recommended)

```python
from eggress import EggressService

toml = """
version = 1

[[listeners]]
name = "proxy"
bind = "127.0.0.1:1080"
protocols = ["socks5"]
"""

with EggressService.from_toml(toml).start() as handle:
    print("Listening on", handle.bound_addresses)
    # ... use the proxy ...
# service is shut down automatically
```

### Async context manager

```python
import asyncio
from eggress import EggressService

TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""

async def main():
    async with await EggressService.from_toml(TOML).astart() as handle:
        print("Listening on", await handle.bound_addresses())

asyncio.run(main())
```

### Starting from pproxy arguments

```python
from eggress import start_pproxy

with start_pproxy(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
    print(handle.bound_addresses)
```

### Connection object

`eggress.Connection` provides a pproxy-compatible low-level connection object backed by Rust networking:

```python
from eggress import Connection

conn = Connection("socks5://:1080", "http://proxy:8080")
# Use conn for proxy operations
conn.close()
```

### pproxy compatibility API

```python
from eggress.pproxy import PPProxyService, Server

# Create from pproxy-style arguments
svc = PPProxyService.from_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"])
with svc.start() as handle:
    print(handle.status())

# Or use the Server class with observability
server = Server(listen="socks5://:1080", remote="http://proxy:8080")
server.start()
print(server.status())
print(server.sessions)
server.close()
```

### Translation helpers

```python
from eggress.pproxy import translate_pproxy_args, check_pproxy_args

# Translate pproxy CLI args to eggress TOML
result = translate_pproxy_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"])
print(result.toml)

# Check compatibility without starting
report = check_pproxy_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"])
print(report.tier)  # "drop_in", "compatible", "supported", etc.
```

See `docs/PYTHON_BINDINGS.md` for the full Python API reference.

## pproxy compatibility

eggress maintains a behavior-oriented compatibility contract against the
`pproxy==2.7.9` tag at commit
`09d4752f17ed6787e1a073c93980eec019887ee3`. The Rust crate
`eggress-pproxy-compat` is an internal translation library used by the CLI and
Python bindings; it is not a separate Python distribution. The bundled
`eggress.pproxy` module provides:

- URI-mode command translation from `pproxy` to `eggress` syntax (including `socks4a`, `https`, `direct`, `ss` scheme aliases)
- CLI flag translation with structured warnings for unsupported features
- Phase 1 URI grammar and CLI ownership coverage, including mixed listeners,
  redaction, fixed-target metadata, and value-taking options
- Compatibility routing: first-available declaration order, explicit
  `fa`/`rr`/`rc`/`lc` scheduler mappings, per-remote regex predicates, the
  `-b` block bridge, and direct unmatched fallback. Eggress's optional rule
  file bridge is an extension; pproxy 2.7.9 has no `--rulefile` flag.
- Structured diagnostics for unsupported protocols (SSH and deferred transports)
- Phase 5 and corrective-pass runtime coverage for `httponly` upstreams,
  TCP/UDP `echo`, canonical fixed-target
  TCP/UDP tunnels, Unix-domain TCP upstreams on Unix, and per-connection local
  source binding
- Differential tests verifying behavioral parity with Python `pproxy` (HTTP, SOCKS4/4a, SOCKS5, standalone UDP)
- A bounded top-level `pproxy` package (`Connection`, `Server`, `Rule`, `DIRECT`, `proto`, `cipher`, and `server`) backed by Eggress adapters
- Python migration helpers (`PPProxyService`, `start_pproxy`, translation and diagnostics APIs) under `eggress.pproxy`
- Structural protocol, cipher, and plugin facades where documented; construction or importability does not imply wire compatibility
- Functional bounded pproxy SSR compatibility: the exact six built-in plugin names (`plain`, `origin`, `http_simple`, `tls1.2_ticket_auth`, `verify_simple`, `verify_deflate`) are supported in compatibility mode; this is obfuscation compatibility, not TLS security
- Native outbound API (`OutboundConnector.connect_tcp()`) with GIL-releasing sync and asyncio wrappers
- `.pyi` type stubs for all public modules

### What is not supported

- **SSH** upstream transport — rejected with a recommendation to use OpenSSH dynamic forwarding (`ssh -D`)
- **QUIC/HTTP/3** — deferred by ADR; URI schemes `quic://` and `h3://` are rejected
- **Other SSR/auth families and legacy stream ciphers** — outside the bounded pproxy 2.7.9 SSR surface; legacy ciphers remain rejected with clear diagnostics
- **SOCKS4/SOCKS5 BIND** — pproxy 2.7.9 also requires CONNECT (`0x01`); the
  matching refusal is not an Eggress-specific strict gap
- **TLS interception** — HTTPS uses CONNECT tunneling, not MITM
- **Certificate reload** — requires restart
- **Private pproxy internals** — only the documented bounded public surface is supported; private implementation details and unsupported protocol families fail explicitly
- **Advanced compatibility transports** — H2 listeners accept independent CONNECT streams; WS/WSS listeners use a fixed target (`ws{host:port}://listener`), with WSS over configured TLS; bounded raw/tunnel fixed-target TCP/UDP and `echo` listener forms remain supported

### Compatibility manifests

The pproxy-compatible binary supports `--auth <seconds>` for bounded,
source-IP authentication reuse when listener credentials are configured. The
cache is process-local and expires on a monotonic clock. `--sys` applies the
actual bound local SOCKS5 listener when available, otherwise HTTP, through the
existing platform backend and restores prior settings on shutdown or failed
startup.

- [`docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`](docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md) — maintained observable compatibility matrix and exclusions
- [`docs/parity/PPROXY_CLOSURE_SCENARIOS.md`](docs/parity/PPROXY_CLOSURE_SCENARIOS.md) — optional representative oracle and public smoke scenarios
- [`docs/parity/pproxy_capability_manifest.toml`](docs/parity/pproxy_capability_manifest.toml) — active phase-0 inventory with frozen-source evidence and strict-phase status
- [`docs/parity/pproxy_2_7_9_strict_manifest.toml`](docs/parity/pproxy_2_7_9_strict_manifest.toml) — 194 behavioral capability records
- [`docs/parity/PPROXY_COMPATIBILITY_POLICY.md`](docs/parity/PPROXY_COMPATIBILITY_POLICY.md) — historical strict-manifest policy and provenance

## Capability status

A capability is checked only when implementation, tests, documentation, and applicable interoperability tests are complete.

Legend: `[x]` complete, `[ ]` not complete.

### Core

- [x] Rust workspace and CI
- [x] Embeddable library API (`eggress-embed`)
- [x] Python bindings (PyO3)
- [x] PyPI package and wheels
- [x] `pproxy`-compatible CLI shell
- [x] Typed URI parser
- [x] Multi-hop chain parser
- [x] Redacted configuration display
- [x] TCP listener
- [x] Unix-domain listener
- [x] Direct TCP connector
- [x] Native `OutboundConnector` — `from_toml()`, `from_pproxy_uri()`, `connect_tcp()`, `connect_tcp_timeout()`; Python sync/async native stream wrappers
- [x] Replayable protocol sniff buffer
- [x] Mixed inbound protocol autodetection
- [x] Half-close-aware bidirectional relay
- [x] Graceful shutdown (drain-first, cancel-after-deadline)
- [x] Connection limits
- [x] Handshake limits and timeouts

### HTTP/1

- [x] HTTP CONNECT server and client
- [x] Single-exchange ordinary HTTP forward-proxy server
- [x] Absolute-form to origin-form rewriting
- [x] HTTP proxy Basic authentication
- [x] Persistent HTTP forwarding
- [x] Hop-by-hop request-header filtering
- [x] HTTP upstream chaining
- [x] Content-Length and chunked request bodies
- [x] Deferred CONNECT success reply

### SOCKS4

- [x] SOCKS4 CONNECT server and client
- [x] SOCKS4 user ID
- [x] SOCKS4a domain targets
- [x] SOCKS4 BIND refusal (pproxy 2.7.9 does not implement BIND)

### SOCKS5

- [x] SOCKS5 CONNECT server and client
- [x] SOCKS5 no-auth and username/password authentication
- [x] SOCKS5 IPv4, IPv6, and domain targets
- [x] SOCKS5 BIND refusal (pproxy 2.7.9 does not implement BIND)
- [x] SOCKS5 UDP ASSOCIATE server and client

### Routing and scheduling

- [x] Direct routes and ordered upstream routes
- [x] Regex, exact-host, domain-suffix, CIDR, port, and reject rules
- [x] First-available, round-robin, random, and least-connections scheduling
- [x] Active health checking with hysteresis
- [x] Direct fallback
- [x] Route explanation command

### Proxy chaining

- [x] HTTP, SOCKS4a, SOCKS5, Shadowsocks → destination
- [x] Cross-protocol chains (HTTP↔SOCKS5, HTTP→HTTP, SOCKS5→SOCKS5)
- [x] Three-or-more-hop TCP chains
- [x] Per-hop timeout and diagnostics
- [x] Chain capability validation

### Upstream protocol support

| Upstream | TCP CONNECT | UDP relay |
|----------|------------|-----------|
| Direct | yes | yes |
| HTTP CONNECT | yes | no |
| SOCKS4/SOCKS4a | yes | no |
| SOCKS5 | yes | one-hop |
| Shadowsocks | yes (aes-128-gcm, aes-192-gcm, aes-256-gcm, chacha20-ietf-poly1305) | yes (standard AEAD upstream; pproxy PacketCipher standalone inbound) |
| Trojan | yes (rustls) | no |

### UDP

- [x] Direct UDP, UDP association table, per-client and global limits
- [x] Association idle timeout and target-flow idle cleanup
- [x] Target-aware reply demultiplexing
- [x] UDP routing with direct-fallback support
- [x] Packet-size and amplification limits
- [x] Per-listener TOML UDP configuration
- [x] SOCKS5 UDP ASSOCIATE server
- [x] UDP through one-hop SOCKS5 upstream
- [x] UDP through one-hop Shadowsocks upstream (standard AEAD)
- [x] Standalone UDP relay (`mode = "standalone_pproxy_udp"`)
- [ ] UDP through Trojan upstream
- [ ] UDP through multi-hop proxy chains
- [ ] UDP through HTTP/MASQUE/CONNECT-UDP

### TLS

- [x] rustls client and server transport
- [x] System root certificates and custom CA roots
- [x] SNI, ALPN (configurable)
- [x] Secure certificate verification default
- [x] Explicit insecure compatibility mode
- [x] HTTPS proxy server and client
- [x] TLS-wrapped SOCKS and custom protocols
- [ ] Certificate reload (deferred)

### Shadowsocks

- [x] TCP client and server (standard SIP003 AEAD framing)
- [x] UDP client and server (standard upstream AEAD plus pproxy-compatible standalone inbound format)
- [x] AEAD cipher support (aes-128-gcm, aes-192-gcm, aes-256-gcm, chacha20-ietf-poly1305)
- [x] Legacy stream cipher diagnostics (rejected with clear error)
- [x] Interoperability with `shadowsocks-rust`
- [x] pproxy 2.7.9 SSR address framing with IPv4, IPv6, domain, and optional auth prefix
- [x] Bounded pproxy SSR plugin codecs behind the `pproxy-legacy` feature
- [x] Ordered `plain`, `origin`, `http_simple`, `tls1.2_ticket_auth`, `verify_simple`, and `verify_deflate` plugin names

### Trojan

- [x] Trojan client, server, and authentication
- [x] Trojan TCP target framing
- [x] Domain length validation (1-255 bytes)
- [ ] Trojan fallback routing

### WebSocket

- [x] WebSocket tunnel client and server
- [x] WSS via rustls
- [x] Binary-message byte-stream adapter
- [x] Ping/pong handling, close and half-close mapping
- [x] Fixed-target WebSocket tunnel
- [x] WebSocket in proxy chains
- [x] Stream-native composition — WS handshake over prior-hop stream
- [x] Compatibility WS/WSS fixed-target listeners

### Raw forwarding

- [x] Fixed-target TCP forwarding
- [x] Raw tunnel client and server
- [x] Stream-native composition — raw passthrough over prior-hop stream
- [x] Bounded fixed-target UDP forwarding (explicit compatibility listener mode)

### HTTP/2

- [x] HTTP/2 CONNECT server and client
- [x] Stream adapter, flow-control integration, stream reset propagation
- [x] GOAWAY handling, upstream connection pooling
- [x] H2-over-TLS ALPN, H2 authentication
- [x] Stream-native composition — H2 handshake over prior-hop stream
- [x] Compatibility H2 CONNECT listener with per-stream routing

### Reverse and backward proxying

- [x] Reverse acceptor (control channel + external listener)
- [x] Reverse control client with auto-reconnect
- [x] Plaintext control-channel handshake
- [x] pproxy URI translation (`socks5+in://`, `bind://`, `listen://`, `backward://`, `rebind://`)
- [x] TOML `[reverse_servers]` / `[reverse_clients]` config model
- [x] Reverse listener access policy (allowlist)
- [x] Reverse admin endpoints
- [ ] Built-in TLS for control channel (use stunnel or external TLS)
- [ ] Reverse UDP (intentional — pproxy does not support UDP reverse)

### Transparent proxying

- [x] Linux `SO_ORIGINAL_DST` and REDIRECT workflow
- [x] Startup capability checks
- [ ] Linux IPv6 original destination
- [ ] Linux TPROXY workflow
- [ ] macOS PF original-destination recovery

### Administration and operations

- [x] TOML configuration with validation
- [x] Configuration reload (routing/upstreams/groups hot-swapped; listener topology requires restart)
- [x] Structured logs (human-readable and JSON)
- [x] Secret redaction for URIs, authentication, and runtime logs
- [x] Traffic counters, per-upstream metrics, Prometheus endpoint
- [x] Local admin API, PAC generation and serving, static HTTP endpoint
- [x] Upstream test command
- [x] System-proxy configuration on macOS and Windows

### Security and robustness

- [x] Bounded parsers and replay buffer
- [x] Connection semaphore
- [x] DNS rebinding-aware routing
- [x] Unsafe-code audit (`deny` level)
- [x] Dependency audit in CI (bans openssl-sys, native-tls, aws-lc-sys, cmake)
- [x] Property tests, fuzz harness smoke, security invariant tests
- [x] Soak tests, resource-exhaustion tests
- [x] Security disclosure process

### Packaging

- [x] Linux binaries (x86_64, aarch64), macOS binaries (x86_64, arm64), Windows binaries (x86_64)
- [x] Python package on PyPI (wheels for Linux/macOS/Windows)
- [ ] Crates.io packages (blocked: CLI depends on internal crates)
- [ ] Reproducible builds

## Project structure

```text
eggress/
├── crates/                 # Workspace crates (core, cli, server, runtime, protocols, transport, etc.)
├── compat/                 # Upstream oracle definition and fixtures
│   └── pproxy-2.7.9/      # Frozen pproxy 2.7.9 oracle: provenance, hashes, requirements
├── fuzz/                   # Fuzz harness smoke targets (libfuzzer-sys based)
├── benches/                # Criterion benchmarks
├── tests/                  # Cross-implementation tests (curl, pproxy)
├── scripts/                # Helper and validation scripts
├── docs/                   # Documentation, parity manifests, and release artifacts
└── plans/                  # Historical planning documents
```

## Dependency policy

eggress prefers pure Rust dependencies where mature implementations exist.

Preferred foundations:

- Tokio for asynchronous I/O
- rustls for TLS
- Hyper/H2 for HTTP transports
- RustCrypto primitives
- Reusable Rust crates from `shadowsocks-rust`
- Pure Rust parsers and codecs

Native dependencies and platform FFI are reserved for operating-system facilities such as transparent proxying and system-proxy configuration.

Dependency hygiene is enforced via `deny.toml` at the workspace root. CI runs `cargo deny check` to block banned crates and audit advisories.

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Embed API](docs/EMBED_API.md)
- [Python bindings](docs/PYTHON_BINDINGS.md)
- [pproxy parity spec](docs/PPROXY_PARITY_SPEC.md)
- [pproxy migration](docs/PPROXY_MIGRATION.md)
- [Config reference](docs/CONFIG_REFERENCE.md)
- [URI grammar](docs/URI_GRAMMAR.md)
- [Testing](docs/TESTING.md)
- [Metrics](docs/METRICS.md)
- [Operations](docs/OPERATIONS.md)
- [Failure semantics](docs/FAILURE_SEMANTICS.md)
- [Security review](docs/SECURITY_REVIEW.md)
- [Secure configuration](docs/security/SECURE_CONFIGURATION.md)
- [Threat model](docs/security/THREAT_MODEL.md)
- [Release process](docs/release/RELEASE_PROCESS.md)
- [Full roadmap](docs/ROADMAP.md)
