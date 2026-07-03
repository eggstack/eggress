# pproxy Embedded Usage Patterns

Comparison of pproxy's Python API patterns and their eggress equivalents.

## 1. pproxy Usage Patterns

### Pattern 1: Create Proxy from URI

**pproxy code:**

```python
import pproxy

# Using Connection (alias for proxies_by_uri)
proxy = pproxy.Connection('socks5://:1080')

# Using Server (same function, different name)
proxy = pproxy.Server('socks5://:1080')
```

**What it does:** Parses a pproxy-style URI and returns a proxy object. The URI
encodes the protocol, bind address, optional cipher, and optional chaining. Both
`pproxy.Connection` and `pproxy.Server` are aliases for
`pproxy.server.proxies_by_uri`. The returned object is a `ProxySimple` (or
subclass) that can start a server or open outbound connections.

**pproxy `__init__.py` (lines 1-6):**

```python
from . import server

Connection = server.proxies_by_uri
Server = server.proxies_by_uri
Rule = server.compile_rule
DIRECT = server.DIRECT
```

**eggress equivalent:**

```python
from eggress import EggressConfig, EggressService

# Construct TOML with equivalent listener and upstream config
toml = """
version = 1

[[listeners]]
name = "proxy"
bind = "127.0.0.1:1080"
protocols = ["socks5"]
"""

config = EggressConfig.from_toml(toml)
```

**Status:** Partial — eggress uses TOML configuration instead of URI strings.
There is no single-line URI-to-proxy-object equivalent. The URI syntax
(`socks5://:1080`) is supported only through the pproxy translation layer
(`translate_pproxy_uri`), not as a first-class object constructor.

---

### Pattern 2: Chain Proxies

**pproxy code:**

```python
import pproxy

# Chain socks5 -> http with double-underscore separator
proxy = pproxy.Server('socks5://host:1080__http://proxy:8080')
```

**What it does:** The `__` separator defines a proxy chain. The left URI is the
inbound listener (socks5 on `host:1080`), and the right URI is the upstream
(http proxy on `proxy:8080`). The `proxies_by_uri` function splits on `__`,
reverses the segments, and builds a chain of `ProxySimple` objects linked via the
`jump` field. Each proxy's `open_connection` connects through its `jump` target.

**eggress equivalent:**

```python
from eggress import start_pproxy

# Chains are expressed via -l and -r flags
with start_pproxy([
    "-l", "socks5://host:1080",
    "-r", "http://proxy:8080",
]) as handle:
    pass
```

**Status:** Supported — eggress handles chains through the `-l`/`-r` separation.
The `__` URI separator is translated by `translate_pproxy_args`. Multi-hop chains
(SOCKS5→SOCKS5→HTTP) are supported via upstream routing rules in TOML.

---

### Pattern 3: Compile Rules

**pproxy code:**

```python
import pproxy

# From a file (one regex pattern per line)
rule = pproxy.Rule('rules.txt')

# Inline regex
rule = pproxy.Server('socks5://:1080?q:{.*example.*}')
```

**What it does:** `compile_rule` (aliased as `pproxy.Rule`) reads a file of
regex patterns or accepts an inline `{regex}` string. It compiles them into a
single combined regex with `$` anchor matching. The compiled function is used by
`ProxySimple.match_rule` to filter which destinations a proxy handles. If the
rule matches, the proxy is used; otherwise it falls through to the next
option or DIRECT.

**eggress equivalent:**

```toml
# In TOML config, routing rules use structured matchers
[[routing.rules]]
name = "block-internal"
match = { any_of = [
    { domain_suffix = ".internal" },
    { ip_cidr = "10.0.0.0/8" },
] }
action = "reject"
```

```python
handle.reload_toml(new_toml)
```

**Status:** Partial — eggress uses structured TOML matchers (`domain_suffix`,
`ip_cidr`, `port`, `all`, `any_of`, `not`) instead of freeform regex. This is
more type-safe and self-documenting but less flexible for arbitrary regex
patterns. Inline regex rules from pproxy URIs are translated via
`translate_pproxy_args` with best-effort conversion.

---

### Pattern 4: Use DIRECT

**pproxy code:**

```python
import pproxy

# The global DIRECT object
target = pproxy.DIRECT

# Used as the final jump in a chain
proxy = pproxy.Server('socks5://:1080__DIRECT')
```

**What it does:** `DIRECT` is a singleton `ProxyDirect` instance. It represents
a direct outbound connection with no upstream proxy. When `schedule()` selects
DIRECT, `open_connection` calls `asyncio.open_connection` directly to the target
host/port. All chains terminate in DIRECT by default if no further jump is
specified.

**eggress equivalent:**

```toml
# A listener with no upstream defaults to direct connection
[[listeners]]
name = "proxy"
bind = "127.0.0.1:1080"
protocols = ["socks5"]
# No [[upstreams]] → direct connection
```

**Status:** Supported — the absence of upstream configuration in eggress
produces direct connections, equivalent to `pproxy.DIRECT`.

---

### Pattern 5: Start Server Programmatically

**pproxy code:**

```python
import asyncio
import pproxy

proxy = pproxy.Server('socks5://:1080')

async def main():
    args = {'v': 0, 'block': None, 'salgorithm': 'fa', 'ruport': False}
    server = await proxy.start_server(args)
    print(f'Serving on {proxy.bind}')
    await asyncio.get_event_loop().create_future()  # run forever

asyncio.run(main())
```

**What it does:** `ProxySimple.start_server` creates an `asyncio.Server` (or
`asyncio.UnixServer` for unix paths) using `asyncio.start_server`. It binds the
`stream_handler` function with all proxy parameters via `functools.partial`. The
returned server object can be closed for shutdown. The `args` dict provides
runtime configuration (block rules, scheduling algorithm, verbose, etc.).

**eggress equivalent:**

```python
from eggress import EggressService

svc = EggressService.from_toml(toml)

# Blocking
handle = svc.start()

# Async
handle = await svc.astart()
```

**Status:** Supported — `EggressService.start()` and `.astart()` provide the
equivalent programmatic server startup. The `args`-style configuration is
replaced by TOML config, which is parsed at construction time.

---

### Pattern 6: Open Outbound Connection

**pproxy code:**

```python
import asyncio
import pproxy

proxy = pproxy.Server('socks5://proxy:1080__http://upstream:8080')

async def main():
    reader, writer = await proxy.open_connection('example.com', 443)
    reader, writer = await proxy.prepare_connection(reader, writer, 'example.com', 443)
    writer.write(b'GET / HTTP/1.1\r\nHost: example.com\r\n\r\n')
    data = await reader.read(4096)
    print(data)

asyncio.run(main())
```

**What it does:** `open_connection(host, port)` opens a raw TCP connection to the
proxy's bind address (or directly if DIRECT). `prepare_connection` then performs
the protocol-specific handshake (SOCKS5 negotiation, HTTP CONNECT, etc.) through
the chain. This two-step pattern allows using the proxy as an outbound
connection provider without running a server.

**eggress equivalent:** N/A — not yet supported.

**Status:** Not Supported — eggress is a server-side proxy. It does not expose a
client-side `open_connection` API. Users must use standard Python libraries
(`httpx`, `aiohttp`, etc.) configured to use the running eggress proxy as an
HTTP/SOCKS5 proxy.

---

### Pattern 7: UDP Relay

**pproxy code:**

```python
import asyncio
import pproxy

proxy = pproxy.Server('udp+socks5://:1080')

async def relay_cb(data):
    print(f'Received {len(data)} bytes')

async def main():
    args = {'v': 0, 'block': None, 'salgorithm': 'fa'}
    server, protocol = await proxy.udp_start_server(args)
    # proxy.udp_open_connection(host, port, data, addr, reply)
    await asyncio.get_event_loop().create_future()
```

**What it does:** `udp_start_server` creates a `DatagramProtocol` endpoint via
`create_datagram_endpoint`. Incoming UDP packets are decrypted (if cipher is
set), protocol-decoded, and relayed to the destination. The `udp_open_connection`
method on `ProxyDirect` creates per-source-address `DatagramProtocol` instances
for outbound UDP relay. `ProxySimple.udp_prepare_connection` wraps data in the
upstream protocol format.

**eggress equivalent:**

```python
from eggress import EggressService

toml = """
version = 1

[[listeners]]
name = "udp_proxy"
bind = "127.0.0.1:1080"
protocols = ["socks5"]
udp = true
"""

with EggressService.from_toml(toml).start() as handle:
    pass
```

**Status:** Supported — eggress supports UDP relay for SOCKS5, Shadowsocks, and
pproxy-compatible standalone UDP mode. The UDP association is managed via
`mode = "standalone_pproxy_udp"` or as part of a SOCKS5 listener with `udp = true`.
No programmatic `udp_open_connection` equivalent exists.

---

### Pattern 8: Full CLI via `main()`

**pproxy code:**

```python
import pproxy

# Run full pproxy CLI from Python
pproxy.main(['-l', 'http://:8080', '-r', 'socks5://proxy:1080'])
```

**What it does:** `pproxy.main()` parses CLI arguments via `argparse`, creates
proxy objects from `-l`/`-r`/`-ul`/`-ur` URIs, sets up SSL, PAC, scheduling,
verbose logging, health checks, and system proxy settings. It then calls
`start_server` for each listener and enters `loop.run_forever()`. This is the
full pproxy startup path, equivalent to running `pproxy` from the command line.

**eggress equivalent:**

```python
from eggress import start_pproxy

# Direct equivalent
start_pproxy(['-l', 'http://:8080', '-r', 'socks5://proxy:1080'])
```

Or:

```python
from eggress import EggressService

svc = EggressService.from_pproxy_args([
    "-l", "http://:8080",
    "-r", "socks5://proxy:1080",
])
with svc.start() as handle:
    pass
```

**Status:** Supported — `start_pproxy()` is the direct equivalent of
`pproxy.main()` for common use cases. `EggressService.from_pproxy_args()` provides
more control. Not all pproxy flags are supported (e.g., `--sys`, `--daemon`,
`--reuse`, `--pac`, `--get`). Unsupported flags are reported as diagnostics.

---

### Pattern 9: Access Protocol Details

**pproxy code:**

```python
import pproxy
from pproxy import proto

# Access protocol classes directly
socks5_proto = proto.Socks5({})
http_proto = proto.HTTP({})
ss_proto = proto.SS({})

# Protocol names
print(proto.Socks5.name)  # 'socks5'
print(proto.HTTP.name)    # 'http'

# Protocol detection
detected = await proto.Socks5({}).guess(reader)
```

**What it does:** The `pproxy.proto` module exposes individual protocol
implementations (`Socks5`, `HTTP`, `Socks4`, `SS`, `SSR`, `Trojan`, `Redir`,
`Pf`, `Tunnel`, `WS`, `H2`, `H3`). Each implements `guess` (detection),
`accept` (server-side handshake), and `connect` (client-side handshake). The
`MAPPINGS` dict maps scheme names to classes. Users can instantiate protocols
directly for custom integration.

**eggress equivalent:** N/A — not yet supported.

**Status:** Not Supported — eggress does not expose individual protocol
implementations as a public API. Protocol detection and handling are internal to
the Rust runtime. Users interact with protocols via TOML configuration only.

---

### Pattern 10: Custom Cipher

**pproxy code:**

```python
from pproxy.cipher import get_cipher

err, cipher = get_cipher('aes-256-gcm:password123')
if err:
    print(f'Error: {err}')
else:
    # cipher is a callable that creates cipher instances
    # Applied via URI: ss://aes-256-gcm:password123@host:port
    pass
```

**What it does:** `get_cipher` parses a `cipher_name:key` string, looks up the
cipher class in the `MAP` dictionary (supporting both C extension and pure
Python implementations), and returns a callable that creates cipher instances.
Ciphers include `aes-256-gcm`, `aes-128-gcm`, `chacha20-ietf-poly1305`,
`aes-256-cfb`, `aes-128-cfb`, `rc4`, `bf-cfb`, etc. The cipher is applied
transparently to the connection stream via reader/writer patching.

**eggress equivalent:** N/A — not yet supported.

**Status:** Not Supported — eggress handles encryption at the protocol level
(Shadowsocks AEAD, TLS for Trojan) internally. Users cannot specify custom
ciphers via the Python API. Cipher selection is determined by the upstream
protocol configuration in TOML.

---

## 2. Eggress Current Patterns

### `start_socks5.py`

**What it does:** Starts a SOCKS5 proxy on `127.0.0.1:1080` using a TOML
configuration string. Uses `EggressService.from_toml()` to parse the config,
`.start()` to launch the service, and the context manager for lifecycle
management. Prints bound addresses and readiness status. Blocks on `while True`
until Ctrl+C.

**Pattern:** Blocking context manager startup with inline TOML config.

```python
with EggressService.from_toml(TOML).start() as handle:
    print("Listening on", handle.bound_addresses)
```

### `async_service.py`

**What it does:** Async equivalent of `start_socks5.py`. Uses
`EggressService.from_toml()` to create the service, `await svc.astart()` for
async startup, and `async with` for lifecycle. Demonstrates async
`bound_addresses`, `status()`, and `metrics_text()` access.

**Pattern:** Async context manager with await-based API.

```python
async with await svc.astart() as handle:
    print("Listening on", await handle.bound_addresses)
```

### `pproxy_translate.py`

**What it does:** Translates pproxy-style CLI arguments (`-l`, `-r`) to eggress
TOML configuration using `translate_pproxy_args()`. Prints the generated TOML,
warnings (partial-behavior notes), unsupported features, and overall success
status. Does not start a service — translation only.

**Pattern:** pproxy argument translation without service startup.

```python
result = translate_pproxy_args(["-l", "socks5://127.0.0.1:1080", "-r", "http://proxy:8080"])
print(result.toml)
```

### `pproxy_run.py`

**What it does:** Combines translation and startup — takes pproxy-style CLI
arguments and starts an eggress service directly using `start_pproxy()`. Prints
bound addresses and a metrics preview. This is the closest equivalent to
`pproxy.main()`.

**Pattern:** pproxy argument-to-service convenience function.

```python
with start_pproxy(["-l", "socks5://127.0.0.1:1080", "-r", "http://proxy:8080"]) as handle:
    print("Listening on", handle.bound_addresses)
```

### `reload_config.py`

**What it does:** Demonstrates hot-reload of configuration at runtime. Starts
with an initial TOML config, then calls `handle.reload_toml()` with a new config
string. Prints the generation number before and after reload, and the list of
upstreams after reload. Shows that reload is atomic and non-disruptive.

**Pattern:** Runtime configuration reload via TOML string.

```python
result = handle.reload_toml(RELOAD_TOML)
print("Reloaded, generation:", result["generation"])
```

---

## 3. Gap Analysis Summary

| Pattern | pproxy | Eggress | Status |
|---------|--------|---------|--------|
| Create proxy from URI | `pproxy.Server('socks5://:1080')` | `eggress.Server(listen=['socks5://:1080'])` | **Supported** — pproxy-shaped constructor with URI translation |
| Chain proxies | `pproxy.Server('a://host__b://proxy')` | `start_pproxy(["-l", "a://host", "-r", "b://proxy"])` | **Supported** — via `-l`/`-r` flags or TOML upstreams |
| Compile rules | `pproxy.Rule('rules.txt')` | TOML structured matchers | **Partial** — structured matchers replace freeform regex |
| Use DIRECT | `pproxy.DIRECT` | No upstream config | **Supported** — absence of upstreams = direct |
| Start server | `proxy.start_server(args)` | `EggressService.start()` | **Supported** — different API shape, same outcome |
| Open outbound connection | `proxy.open_connection(host, port)` | N/A | **Not Supported** — server-side only |
| UDP relay | `proxy.udp_start_server(args)` | `EggressService` with `udp = true` | **Supported** — no programmatic `udp_open_connection` |
| Full CLI via `main()` | `pproxy.main([...])` | `start_pproxy([...])` | **Supported** — common flags work; some pproxy flags unsupported |
| Access protocol details | `proto.Socks5`, `proto.HTTP` | N/A | **Not Supported** — protocol internals not exposed |
| Custom cipher | `get_cipher('aes-256-gcm:key')` | N/A | **Not Supported** — cipher handled by protocol layer |
