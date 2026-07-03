# Python Lifecycle Parity: pproxy vs Eggress

This document maps pproxy's lifecycle model to the Eggress runtime model,
comparing each phase and identifying parity gaps.

## 1. pproxy Lifecycle Model

pproxy is a single-process asyncio proxy server with a monolithic lifecycle.

### Initialization

```
main(args)
  → argparse.parse_args(args)
  → for each -l/-r/-ul/-ur: proxies_by_uri(uri_jumps)
    → proxy_by_uri(uri, jump)  # parses scheme, creates ProxySimple/ProxyH2/etc.
  → for -b: compile_rule(filename)  # builds regex block rule
```

- Proxies are parsed from URI strings (`-l socks5://:1080`, `-r http://proxy:8080`)
- The `proxies_by_uri()` function chains multiple proxies via `__` separator
- Each URI is parsed by `proxy_by_uri()` which inspects scheme segments to
  select protocol, cipher, and transport classes
- Block rules (`-b`) compile to a regex matcher

### Bind

```python
for option in args.listen:
    server = loop.run_until_complete(option.start_server(vars(args)))
    servers.append(server)
```

- TCP listeners created via `asyncio.start_server()`
- UDP listeners created via `loop.create_datagram_endpoint()`
- Backward (inbound) clients start outbound connections to the upstream
- All listeners bind immediately during startup; no readiness gate

### Serve

```python
loop.run_forever()  # blocks until KeyboardInterrupt
```

- Single asyncio event loop (uvloop preferred if installed)
- Each accepted TCP connection spawns a coroutine via `stream_handler()`
- Each UDP datagram is processed by `datagram_handler()`
- Connections are relayed between local protocol handler and remote proxy option
- Scheduling algorithm (`-s`) selects among upstream proxies per-connection

### Health Check

```python
if args.alived > 0 and args.rserver:
    asyncio.ensure_future(check_server_alive(args.alived, args.rserver, verbose))
```

- Optional via `-a <interval>` flag (seconds, default: disabled)
- `check_server_alive()` loops: sleep interval, attempt TCP connect to each upstream
- Flips `remote.alive` boolean on success/failure
- Logs `OFFLINE`/`ONLINE` transitions
- No active TCP probes beyond a 3-second connect timeout
- No health state machine or hysteresis

### Shutdown

```python
try:
    loop.run_forever()
except KeyboardInterrupt:
    print('exit')

for task in asyncio.all_tasks(loop):
    task.cancel()
for server in servers:
    server.close()
for server in servers:
    if hasattr(server, 'wait_closed'):
        loop.run_until_complete(server.wait_closed())
loop.run_until_complete(loop.shutdown_asyncgens())
loop.close()
```

- Triggered by `KeyboardInterrupt` (SIGINT) or `SIGTERM` (default Python handler)
- Cancels all asyncio tasks
- Closes all servers (TCP and UDP) and waits for them to finish
- Shuts down async generators
- Closes the event loop
- No configurable drain timeout
- No connection-count tracking during drain

### Reload

Not supported. Changing configuration requires restarting the process.

## 2. Eggress Lifecycle Model

Eggress separates configuration, service construction, startup, runtime control,
and teardown into distinct API surfaces.

### Initialization

**From TOML (native):**
```python
config = EggressConfig.from_toml(toml_string)
config = EggressConfig.from_file("config.toml")
```

**From pproxy args (compatibility):**
```python
service = EggressService.from_pproxy_args(
    ["-l", "socks5://:1080", "-r", "http://proxy:8080"]
)
```

- `EggressConfig` validates and compiles the TOML at construction time
- `from_pproxy_args()` translates pproxy CLI args to eggress TOML internally
  via `translate_pproxy_args()`
- Unsupported pproxy features raise `UnsupportedFeatureError` unless
  `allow_partial=True`

### Start

```python
# Synchronous
handle = service.start()

# Async (non-blocking)
handle = await service.astart()
```

- `start()` blocks until readiness (up to 30 seconds) or startup failure
- `astart()` runs the blocking start in `asyncio.to_thread()` to avoid
  blocking the caller's event loop
- Internally spawns a dedicated OS thread (`eggress-embed-rt`) that owns
  the Tokio runtime and `ServiceSupervisor::run()`
- Pre-binds all listeners before reporting readiness to avoid race conditions
- Returns an `EggressHandle` (or `AsyncEggressHandle`) for runtime control

### Serve

- Tokio multi-threaded runtime under the hood
- GIL is released on all blocking Rust calls
- Protocol detection, connection acceptance, and relay all run natively in Rust
- Async context managers (`with`/`async with`) ensure cleanup on scope exit

### Health Check

Built into the runtime supervisor:

- Per-upstream health state machine with hysteresis
- Active TCP probes with configurable intervals
- Health state transitions: `Healthy` → `Degraded` → `Unhealthy` → `Disabled`
- Excluded from scheduling when `Unhealthy` or `Disabled`
- No user-facing flag required — enabled automatically when upstreams are configured

### Reload

```python
result = handle.reload_toml(new_toml)
# or
result = handle.reload_toml_file("new_config.toml")
```

- Hot-reloads routing rules, upstreams, groups, and health configuration
- Atomic swap via `ArcSwap<Router>` — lock-free reads during transition
- Rejects changes to listener topology (count, names, bind addresses) —
  these require a full restart
- Returns `ReloadOutcome` with new generation number and upstream count
- Mutex prevents concurrent reloads

### Shutdown

```python
# Synchronous
handle.shutdown()

# Async
await async_handle.shutdown()

# Context manager (automatic)
with EggressHandle(...) as h:
    pass  # shutdown() called on __exit__
```

- Cancels the shutdown token, triggering orderly teardown
- Readiness set to `false` → stop listeners → drain connections
  (force-cancel after grace period) → stop admin
- Drop behavior is a fallback: cancels token and best-effort joins the
  supervisor thread (5-second timeout on async path)
- Explicit `shutdown()` or `shutdown_blocking()` is preferred

## 3. Parity Comparison Table

| Phase | pproxy | Eggress | Parity |
|---|---|---|---|
| Init from URI | `proxies_by_uri()` | `EggressConfig` + TOML | **B** — different model; pproxy URIs map to TOML |
| Init from CLI args | `main(args)` | `from_pproxy_args(args)` | **B** — translation layer bridges the gap |
| Start serving | `main()` event loop | `EggressService.start()` | **A** — equivalent functional outcome |
| Config reload | Not supported | `reload_toml()` | **Eggress-native** — no pproxy equivalent |
| Graceful shutdown | SIGTERM handler | `shutdown()` | **B** — Eggress has explicit API + timeout; pproxy uses signal handler |
| Health checks | `-a` flag (optional) | Built-in per upstream | **B** — Eggress has richer health model |
| Context manager | Not supported | `with`/`async with` | **Eggress-native** — no pproxy equivalent |
| Async context mgr | Not supported | `async with` | **Eggress-native** — no pproxy equivalent |
| Status/metrics | Not available | `status()`, `metrics_text()` | **Eggress-native** — no pproxy equivalent |

**Parity Legend:**
- **A** — Feature present in both with same API shape and semantics
- **B** — Feature present in both but with architectural differences
- **Eggress-native** — Feature exists only in eggress; not a pproxy parity claim

## 4. Thread Model Comparison

| Aspect | pproxy | Eggress |
|---|---|---|
| Event loop | Single asyncio loop (uvloop preferred) | Tokio multi-threaded runtime |
| Python GIL | Held during all Python code; no GIL release | Released on all blocking Rust calls |
| Connection handling | One coroutine per connection | Native Rust tasks (no Python overhead) |
| UDP handling | `DatagramProtocol` per association | Native Rust UDP relay |
| Blocking operations | Must use `await asyncio.sleep()` etc. | Dedicated OS thread (`eggress-embed-rt`) + inner run thread |
| Thread ownership | Single thread | Two dedicated threads (blocking path) or blocking-pool + OS thread (async path) |

Key implications for embedded usage:

- **pproxy** inside an asyncio application shares the caller's event loop. Long-running
  pproxy operations can starve other coroutines.
- **Eggress** runs its own Tokio runtime on dedicated threads. The Python caller interacts
  via `asyncio.to_thread()` wrappers, keeping the GIL released and the caller's event loop
  responsive.

## 5. Recommendations for Migrating from pproxy Embedded Usage

### 5.1 Replace `loop.run_forever()` patterns

pproxy's `main()` owns the event loop. When embedding, users typically call
`main(args)` which blocks forever. With eggress:

```python
# Before (pproxy)
import pproxy
pproxy.server.main(["-l", "socks5://:1080", "-r", "http://proxy:8080"])

# After (eggress — async)
async with EggressService.from_pproxy_args(
    ["-l", "socks5://:1080", "-r", "http://proxy:8080"]
).astart() as handle:
    print(await handle.status())
    # service runs until scope exits
```

### 5.2 Use context managers for cleanup

pproxy requires manual signal handling and task cancellation. Eggress context
managers handle shutdown automatically:

```python
# Before (pproxy)
loop = asyncio.get_event_loop()
server = loop.run_until_complete(option.start_server(vars(args)))
try:
    loop.run_forever()
finally:
    server.close()
    loop.run_until_complete(server.wait_closed())

# After (eggress)
async with EggressService.from_pproxy_args(args).astart() as handle:
    await asyncio.Event().wait()  # block until cancelled
```

### 5.3 Leverage hot-reload

pproxy requires a restart for config changes. Eggress supports hot-reload:

```python
async with EggressService.from_toml(original_toml).astart() as handle:
    # Later, without downtime:
    result = await handle.reload_toml(new_toml)
    print(f"Generation {result['generation']}")
```

### 5.4 Monitor with built-in metrics

pproxy exposes no programmatic status. Eggress provides Prometheus metrics
and structured status:

```python
handle = service.start()
print(handle.status())          # generation, connections, uptime
print(handle.metrics_text())    # Prometheus text format
```

### 5.5 Be aware of health model differences

pproxy's health check is a simple alive/dead boolean per upstream with an
optional interval flag. Eggress has a state machine with hysteresis. If
your application relied on pproxy's `-a` flag to control health probing,
note that eggress enables health probing automatically for all configured
upstreams.

### 5.6 Configuration model shift

pproxy uses URI strings directly (`-l socks5://:1080`). Eggress uses TOML
configuration with an explicit schema. The `from_pproxy_args()` translation
layer bridges this gap, but native TOML is recommended for new integrations
as it supports:

- Named listeners with per-listener protocol selection
- Structured upstream groups with health configuration
- Routing rules with matchers
- Admin server and metrics endpoints

### 5.7 Thread isolation

pproxy's single-threaded asyncio model means proxy operations compete with
application code for CPU time. Eggress runs its proxy on dedicated OS threads,
keeping the Python application's event loop responsive. The GIL is released
on all blocking Rust calls, allowing true concurrency between Python code and
the proxy runtime.
