# Protocol, Cipher, Wrapper, and Plugin Behavioral Audit

**Workstream 7 — Corrective Verification Pass**

Date: 2026-07-15

## Summary

This audit inventories every public method on the Python protocol, cipher, wrapper,
and plugin objects and classifies each as:

| Classification | Meaning |
|---|---|
| **fully functional** | Method works as documented and returns correct results |
| **delegated to Rust** | Method exists but actual work is done by the Rust runtime |
| **construction-only** | Constructor works but operational methods are stubs/raise errors |
| **unsupported with warning** | Intentionally unsupported; raises `UnsupportedFeatureError` |
| **metadata-only** | Method returns metadata/configuration but does not alter runtime behavior |

### Key Findings

1. **`encrypt_and_digest` / `decrypt_and_verify` on all AEAD cipher objects raise
   `UnsupportedFeatureError`.** They are stubs. AEAD encryption is handled entirely
   by the Rust backend (`eggress-protocol-shadowsocks`). No Python cipher object
   can perform actual encryption.

2. **Plugin callbacks are NOT in the live stream/datagram path.** `PluginBridge`,
   `PluginRegistry`, and `CallbackWrapper` are standalone callback utilities. No Rust
   code references `PluginBridge` or invokes Python callbacks during proxy operation.
   The built-in hook names (`on_protocol_detect`, `on_cipher_select`, `on_connect`,
   `on_data`) are string constants only — they are not called by any Rust code.

3. **Wrapper objects (`TLS`, `Plugin`, `Chain`) do NOT alter runtime configuration.**
   They are metadata-only containers that delegate property access to inner protocols.
   The Rust runtime reads TOML config directly; Python wrapper objects are not consumed
   by the Rust layer.

4. **Unsupported protocol/cipher objects (SSR, H3, SSH, legacy ciphers) inflate
   structural counts but are correctly classified as non-functional.** SSR, H3, and
   SSH raise `UnsupportedFeatureError` at construction time and cannot be instantiated.
   All 20 legacy stream ciphers (RC4, AES-CFB, etc.) raise `UnsupportedFeatureError`
   on `encrypt`/`decrypt`.

---

## Protocol Object Inventory (`python/eggress/protocol.py`)

### BaseProtocol

| Method | Classification | Notes |
|---|---|---|
| `__init__(param, target, dest, source)` | fully functional | Stores all attributes |
| `name` (property) | fully functional | Returns `class.__name__.lower()` |
| `reuse()` | fully functional | Returns `False` |
| `udp_accept(data, **kw)` | construction-only | Raises `NotImplementedError` (base stub) |
| `udp_connect(rauth, host, port, data, **kw)` | construction-only | Raises `NotImplementedError` (base stub) |
| `udp_unpack(data)` | fully functional | Identity (returns data) |
| `udp_pack(host, port, data)` | fully functional | Identity (returns data) |
| `connect(reader, writer, rauth, host, port, **kw)` | construction-only | Raises `NotImplementedError` (base stub) |
| `guess(reader, **kw)` | construction-only | Raises `NotImplementedError` (base stub) |
| `accept(reader, user, **kw)` | construction-only | Raises `NotImplementedError` (base stub) |
| `__eq__`, `__hash__` | fully functional | Compares type, param, target, dest, source |
| `__repr__`, `__str__` | fully functional | Redacts secrets in repr |
| `__reduce__`, `__copy__`, `__deepcopy__` | fully functional | Pickle/copy support |

### Direct

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | fully functional | Sets `target = param or None` |

All inherited methods from `BaseProtocol` work as expected.

### HTTP

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | fully functional | Sets target, initializes `httpget = {}` |

### HTTPOnly(HTTP)

Empty subclass. Fully functional.

### Socks4

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | fully functional | Sets target from param |

### Socks5

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | fully functional | Sets target from param |
| `_TRAFFIC_KINDS` | metadata-only | `("tcp", "udp")` — used by composition matrix |

### SS (Shadowsocks)

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | fully functional | Bypasses SSR init, parses `cipher:password` |
| `cipher` attribute | fully functional | Extracted cipher name string |

### SSR (ShadowsocksR)

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |
| `_SUPPORTED_IN_EGRESS` | metadata-only | `False` |

**Cannot be instantiated.** Intentionally unsupported legacy protocol.

### Trojan

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | fully functional | Parses target from `password@host:port` |

### WS (WebSocket)

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | fully functional | Parses target, strips path component |

### H2 (HTTP/2)

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | fully functional | Inherits from HTTP |

### H3 (HTTP/3)

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |
| `_SUPPORTED_IN_EGRESS` | metadata-only | `False` |

**Cannot be instantiated.** Intentionally unsupported protocol.

### SSH

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |
| `_SUPPORTED_IN_EGRESS` | metadata-only | `False` |

**Cannot be instantiated.** Intentionally unsupported protocol.

### Transparent (base class)

| Method | Classification | Notes |
|---|---|---|
| `guess(reader, sock, **kw)` | construction-only | Calls `query_remote` which raises `NotImplementedError` |
| `accept(reader, user, sock, **kw)` | construction-only | Calls `query_remote` which raises `NotImplementedError` |
| `udp_accept(data, sock, **kw)` | construction-only | Calls `query_remote` which raises `NotImplementedError` |
| `query_remote(sock)` | construction-only | Raises `NotImplementedError` (subclass must override) |

### Redir, Pf

Empty subclasses of `Transparent`. `query_remote` is **not implemented** — these
are construction-only. Actual transparent proxy functionality is in the Rust layer
(`eggress-server/src/listener/transparent.rs`).

### Tunnel

| Method | Classification | Notes |
|---|---|---|
| `__init__(param)` | fully functional | Sets `dest` and `destination` |
| `query_remote(sock)` | fully functional | Parses target from param or sock |
| `connect(...)` | fully functional | No-op (correct for fixed-target) |
| `udp_connect(...)` | fully functional | Returns data as-is |

### Echo

| Method | Classification | Notes |
|---|---|---|
| `query_remote(sock)` | fully functional | Returns `("echo", 0)` |

### Module-level functions

| Function | Classification | Notes |
|---|---|---|
| `get_protos(rawprotos)` | fully functional | Parses `"name{param}"` strings to instances |
| `accept(protos, reader, **kw)` | fully functional | Iterates protocols, calls guess/accept |
| `udp_accept(protos, data, **kw)` | fully functional | Iterates protocols, calls udp_accept |

### MAPPINGS registry

24 entries total:
- 20 class mappings (Direct, HTTP, HTTPOnly, Socks4, Socks4a, Socks5, Socks, SS, SSR,
  Trojan, WS, H2, H3, SSH, Redir, Pf, Tunnel, Echo + aliases)
- 4 string mappings (`ssl=""`, `secure=""`, `quic=""`, `in=""` — TLS/in markers)

---

## Cipher Object Inventory (`python/eggress/cipher.py`)

### BaseCipher

| Method | Classification | Notes |
|---|---|---|
| `__init__(key, ota, setup_key)` | fully functional | Stores key, derives IV |
| `key` (property) | fully functional | Returns raw key bytes |
| `iv` (property) | fully functional | Returns IV bytes |
| `setup_iv(iv)` | fully functional | Sets or generates IV |
| `name()` (classmethod) | fully functional | Derives name from class name |
| `encrypt(s)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |
| `decrypt(s)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |
| `__eq__`, `__hash__` | fully functional | Compares name + key |
| `__repr__`, `__str__` | fully functional | Does not expose key |
| `__reduce__` | fully functional | Raises `TypeError` (key material) |
| `__copy__`, `__deepcopy__` | fully functional | Creates new instance with same key |
| `__del__` | fully functional | Best-effort key zeroing |

### AEADCipher(BaseCipher)

| Method | Classification | Notes |
|---|---|---|
| `__init__(key, ota, setup_key)` | fully functional | Adds nonce setup |
| `nonce` (property) | fully functional | Returns nonce bytes |
| `setup_nonce(nonce)` | fully functional | Sets or generates nonce |
| `encrypt(s)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |
| `decrypt(s)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |
| `encrypt_chunk(chunk)` | **unsupported with warning** | Delegates to `encrypt` → raises |
| `decrypt_chunk(chunk)` | **unsupported with warning** | Delegates to `decrypt` → raises |
| `encrypt_and_digest(plaintext)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |
| `decrypt_and_verify(ciphertext, tag)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |

### Concrete AEAD Ciphers

All four AEAD cipher classes (`AES_256_GCM_Cipher`, `AES_192_GCM_Cipher`,
`AES_128_GCM_Cipher`, `ChaCha20_IETF_POLY1305_Cipher`) inherit from `AEADCipher`
and add only `KEY_LENGTH`, `IV_LENGTH`, `NONCE_LENGTH`, `TAG_LENGTH`, `PACKET_LIMIT`
constants. **All encrypt/decrypt methods raise `UnsupportedFeatureError`.**

These objects carry metadata (key lengths, algorithm identity) that the Rust layer
uses for AEAD framing, but no Python-side encryption occurs.

### Legacy/Stream Ciphers (20 classes)

All legacy cipher classes (`RC4_Cipher`, `RC4_MD5_Cipher`, `ChaCha20_Cipher`,
`ChaCha20_IETF_Cipher`, `Salsa20_Cipher`, `AES_256_CFB_Cipher`,
`AES_192_CFB_Cipher`, `AES_128_CFB_Cipher`, `AES_256_CFB8_Cipher`,
`AES_192_CFB8_Cipher`, `AES_128_CFB8_Cipher`, `AES_256_OFB_Cipher`,
`AES_192_OFB_Cipher`, `AES_128_OFB_Cipher`, `AES_256_CTR_Cipher`,
`AES_192_CTR_Cipher`, `AES_128_CTR_Cipher`, `BF_CFB_Cipher`,
`CAST5_CFB_Cipher`, `DES_CFB_Cipher`) share this behavior:

| Method | Classification | Notes |
|---|---|---|
| `__init__(key, ota, setup_key)` | fully functional | Stores key |
| `encrypt(s)` | **unsupported with warning** | Raises `UnsupportedFeatureError` (Track F) |
| `decrypt(s)` | **unsupported with warning** | Raises `UnsupportedFeatureError` (Track F) |

These are **construction-only**: they can be instantiated (for pproxy compatibility
in `get_cipher`), but cannot perform any encryption. They exist to satisfy pproxy's
cipher registry interface.

### PacketCipher

| Method | Classification | Notes |
|---|---|---|
| `__init__(cipher, key, name)` | fully functional | Stores metadata |
| `cipher` (property) | fully functional | Returns wrapped cipher |
| `key` (property) | fully functional | Returns key bytes |
| `name` (property) | fully functional | Returns cipher name |
| `encrypt(data)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |
| `decrypt(data)` | **unsupported with warning** | Raises `UnsupportedFeatureError` |

### _ApplyCipher

| Method | Classification | Notes |
|---|---|---|
| `__init__(cipher, key, name, ota, plugins, datagram)` | fully functional | Metadata carrier |
| `key` (property) | fully functional | Returns key bytes |
| `name` (property) | fully functional | Returns cipher name |
| `ota` (property) | fully functional | Returns OTA flag |
| `plugins` (property) | fully functional | Returns plugin list (empty) |
| `datagram` (property) | fully functional | Returns `PacketCipher` or None |
| `__call__(data)` | **metadata-only** | **Returns data as-is** (identity function) |

`_ApplyCipher.__call__` is a no-op identity function. It carries metadata (key, name,
ota flag, datagram cipher) that the Rust layer reads from config, but the Python
callable itself performs no encryption.

### get_cipher(cipher_key)

| Classification | Notes |
|---|---|
| fully functional | Parses `"name:password[!ota]"`, derives key via EVP_BytesToKey, returns `_ApplyCipher` |

### MAP registry

24 entries: 4 AEAD + 20 legacy stream ciphers.

---

## Plugin Object Inventory (`python/eggress/plugin.py`)

**CRITICAL FINDING: Plugin callbacks are NOT in the live stream/datagram path.**

`PluginBridge`, `PluginRegistry`, and `CallbackWrapper` are standalone Python
callback utilities. No Rust code in the workspace references `PluginBridge`,
`PluginRegistry`, or invokes Python callbacks during proxy operation. The built-in
hook names are string constants only.

The Rust runtime handles protocol detection, data relay, and cipher operations
entirely in Rust. There is no FFI bridge that calls back into Python for
`on_data`, `on_connect`, `on_protocol_detect`, or `on_cipher_select` events.

### CallbackResult

| Field/Method | Classification | Notes |
|---|---|---|
| `hook_name` | fully functional | Frozen dataclass field |
| `value` | fully functional | Callback return value |
| `elapsed_ms` | fully functional | Execution time |
| `timed_out` | fully functional | Timeout flag |
| `rejected` | fully functional | Rejection flag |
| `error` | fully functional | Error message |
| `ok` (property) | fully functional | True if no error/timeout/rejection |

### CallbackMetrics

| Method | Classification | Notes |
|---|---|---|
| `total`, `succeeded`, `failed`, `timed_out`, `rejected` | fully functional | Counters |
| `total_elapsed_ms` | fully functional | Accumulated time |
| `avg_elapsed_ms` (property) | fully functional | Average computation |
| `record(result)` | fully functional | Records a CallbackResult |

### CallbackWrapper

| Method | Classification | Notes |
|---|---|---|
| `__init__(callback, timeout, hook_name)` | fully functional | Wraps callback with metadata |
| `hook_name` (property) | fully functional | |
| `timeout` (property) | fully functional | |
| `metrics` (property) | fully functional | |
| `execute(args, kwargs, timeout)` | fully functional | Runs callback with timeout enforcement |
| | | Sync callbacks dispatched to thread executor |
| | | Async callbacks awaited directly |

**NOT wired into Rust runtime.** This is a standalone callback execution wrapper.

### PluginRegistry

| Method | Classification | Notes |
|---|---|---|
| `__init__()` | fully functional | Creates empty registry with lock |
| `register(name, callback, timeout)` | fully functional | Registers callback, creates wrapper |
| `unregister(name)` | fully functional | Removes callback |
| `get(name)` | fully functional | Returns callback or None |
| `get_wrapper(name)` | fully functional | Returns CallbackWrapper or None |
| `has(name)` | fully functional | Checks existence |
| `list_hooks()` | fully functional | Returns hook name list |
| `clear()` | fully functional | Removes all callbacks |
| `__len__` | fully functional | |
| `__contains__` | fully functional | |
| `__repr__` | fully functional | |

**Thread-safe** via `threading.Lock`. **NOT wired into Rust runtime.**

### PluginBridge

| Method | Classification | Notes |
|---|---|---|
| `__init__(registry, max_queue, default_timeout)` | fully functional | Creates bridge with semaphore |
| `registry` (property) | fully functional | |
| `max_queue` (property) | fully functional | |
| `default_timeout` (property) | fully functional | |
| `active_count` (property) | fully functional | |
| `is_shutdown` (property) | fully functional | |
| `metrics()` | fully functional | Aggregated metrics per hook |
| `submit_async(hook_name, *args, timeout, **kwargs)` | fully functional | Async callback execution |
| `submit(hook_name, *args, timeout, **kwargs)` | fully functional | Sync blocking execution |
| `shutdown()` | fully functional | Prevents new submissions |
| `shutdown_async(cancel_active)` | fully functional | Async shutdown with optional cancel |
| `__repr__` | fully functional | |

**NOT wired into Rust runtime.** No Rust code creates or invokes a `PluginBridge`.
The bridge is a standalone Python utility for applications that want to add Python
callbacks around proxy operations, but the core proxy data path is entirely in Rust.

### Built-in Hook Names

| Constant | Value | Live-path? |
|---|---|---|
| `HOOK_ON_PROTOCOL_DETECT` | `"on_protocol_detect"` | **No** — constant only |
| `HOOK_ON_CIPHER_SELECT` | `"on_cipher_select"` | **No** — constant only |
| `HOOK_ON_CONNECT` | `"on_connect"` | **No** — constant only |
| `HOOK_ON_DATA` | `"on_data"` | **No** — constant only |

---

## Wrapper Object Inventory (`python/eggress/wrapper.py`)

**CRITICAL FINDING: Wrapper objects do NOT alter runtime configuration.**

The Rust runtime reads TOML config directly. Python wrapper objects are metadata-only
containers used for pproxy API compatibility. They are not consumed by the Rust layer.

### BaseWrapper (abstract)

| Method | Classification | Notes |
|---|---|---|
| `__init__(inner)` | fully functional | Stores inner protocol |
| `inner` (property) | fully functional | Returns wrapped protocol |
| `name` (property) | fully functional | Returns `_WRAP_TYPE` |
| `target` (property) | fully functional | Delegates to inner |
| `dest` (property) | fully functional | Delegates to inner |
| `source` (property) | fully functional | Delegates to inner |
| `_SUPPORTED_IN_EGRESS` (property) | fully functional | Delegates to inner |
| `_TRAFFIC_KINDS` (property) | fully functional | Delegates to inner |
| `_ROLE` (property) | fully functional | Delegates to inner |
| `__eq__`, `__hash__` | fully functional | Compares type + inner |
| `__repr__` | fully functional | |
| `__reduce__`, `__copy__`, `__deepcopy__` | fully functional | |

### TLS(BaseWrapper)

| Method | Classification | Notes |
|---|---|---|
| `__init__(inner, certfile, keyfile, sni)` | fully functional | Stores TLS parameters |
| `certfile` (property) | fully functional | |
| `keyfile` (property) | fully functional | |
| `sni` (property) | fully functional | |
| `name` (property) | fully functional | Returns `"tls"` |
| `__eq__`, `__hash__` | fully functional | Includes certfile, keyfile, sni |
| `__repr__` | fully functional | Redacts file paths to basename |
| `__reduce__`, `__copy__`, `__deepcopy__` | fully functional | |

**Does NOT configure TLS on the Rust runtime.** Stores metadata that pproxy
API consumers can read, but the Rust layer reads TLS config from TOML.

### Plugin(BaseWrapper)

| Method | Classification | Notes |
|---|---|---|
| `__init__(inner, handler)` | fully functional | Stores handler reference |
| `handler` (property) | fully functional | Returns handler |
| `name` (property) | fully functional | Returns `"plugin"` |
| `__eq__`, `__hash__` | fully functional | Includes handler |
| `__repr__` | fully functional | |
| `__reduce__`, `__copy__`, `__deepcopy__` | fully functional | |

**Does NOT wire handler into live data path.** The handler is a metadata attribute
only. The Rust runtime does not call it.

### Chain

| Method | Classification | Notes |
|---|---|---|
| `__init__(protocols)` | fully functional | Stores as tuple |
| `target` (property) | fully functional | First protocol's target |
| `dest` (property) | fully functional | Last protocol's dest |
| `source` (property) | fully functional | Last protocol's source |
| `name` (property) | fully functional | Returns `"chain"` |
| `__len__` | fully functional | |
| `__getitem__` | fully functional | |
| `__iter__` | fully functional | |
| `__contains__` | fully functional | |
| `__eq__`, `__hash__` | fully functional | |
| `__repr__` | fully functional | |
| `flat()` | fully functional | Unwraps wrappers to base protocols |
| `validate()` | fully functional | Checks `_SUPPORTED_IN_EGRESS` flags |
| `__reduce__`, `__copy__`, `__deepcopy__` | fully functional | |

**Does NOT execute chain.** This is a data structure only. Chain execution is
handled by the Rust chain executor.

### normalize_chain(protocols)

| Classification | Notes |
|---|---|
| fully functional | Orders: base → TLS → Plugin |

---

## Methods Incorrectly Claimed as Functional

The following methods exist on public objects but do NOT perform their apparent
function:

1. **`AEADCipher.encrypt_and_digest(plaintext)`** — Raises `UnsupportedFeatureError`.
   Does not encrypt. Listed in `.pyi` stub as returning `tuple[bytes, bytes]`.

2. **`AEADCipher.decrypt_and_verify(ciphertext, tag)`** — Raises `UnsupportedFeatureError`.
   Does not decrypt. Listed in `.pyi` stub as returning `bytes`.

3. **`AEADCipher.encrypt_chunk(chunk)`** — Delegates to `encrypt()`, which raises
   `UnsupportedFeatureError`.

4. **`AEADCipher.decrypt_chunk(chunk)`** — Delegates to `decrypt()`, which raises
   `UnsupportedFeatureError`.

5. **`BaseCipher.encrypt(s)`** — Raises `UnsupportedFeatureError`.

6. **`BaseCipher.decrypt(s)`** — Raises `UnsupportedFeatureError`.

7. **`PacketCipher.encrypt(data)`** — Raises `UnsupportedFeatureError`.

8. **`PacketCipher.decrypt(data)`** — Raises `UnsupportedFeatureError`.

9. **`_ApplyCipher.__call__(data)`** — Returns data as-is (identity function).
   Does not perform encryption despite being named "apply cipher".

10. **All 20 legacy stream cipher `encrypt`/`decrypt` methods** — Raise
    `UnsupportedFeatureError`.

---

## Plugins: Live-Path vs. Standalone

**Verdict: Plugins are standalone callback utilities, NOT live-path.**

Evidence:
- Zero references to `PluginBridge`, `PluginRegistry`, or `CallbackWrapper` in any
  Rust source file (`crates/**/*.rs`).
- Zero references to Python plugin objects in `eggress-python/src/lib.rs`.
- The built-in hook names (`on_protocol_detect`, `on_cipher_select`, `on_connect`,
  `on_data`) are string constants defined only in `python/eggress/plugin.py`.
- The Rust runtime performs protocol detection, cipher negotiation, connection setup,
  and data relay entirely in Rust code with no Python callback invocation.
- The `Plugin` wrapper class in `wrapper.py` stores a handler attribute but never
  executes it — it's metadata only.
- `__init__.py` exports `PluginRegistry` and `PluginBridge` for API completeness, but
  they are not used by the core proxy pipeline.

The plugin infrastructure is architecturally correct (bounded semaphore, timeout
enforcement, reentrancy detection, GIL-safe dispatch) but currently has no integration
point with the Rust runtime. It serves as a foundation for future Python-side extension
points.

---

## Drop-In Count Inflation Analysis

### Protocol objects that inflate structural counts

| Object | In MAPPINGS? | Instantiable? | Functional? |
|---|---|---|---|
| SSR | Yes (`ssr`) | **No** (raises) | No |
| H3 | Yes (`h3`) | **No** (raises) | No |
| SSH | Yes (`ssh`) | **No** (raises) | No |
| Redir | Yes (`redir`) | Yes | Partial (query_remote unimplemented) |
| Pf | Yes (`pf`) | Yes | Partial (query_remote unimplemented) |
| Echo | Yes (`echo`) | Yes | Yes (test utility) |

SSR, H3, and SSH inflate `MAPPINGS` to 24 entries (20 classes + 4 strings) but
cannot be instantiated. They are correctly flagged as `_SUPPORTED_IN_EGRESS = False`.

### Cipher objects that inflate structural counts

| Category | Count | Instantiable? | encrypt/decrypt? |
|---|---|---|---|
| AEAD ciphers | 4 | Yes | **No** (raises) |
| Legacy stream ciphers | 20 | Yes | **No** (raises) |

All 24 cipher classes in `MAP` can be instantiated (for pproxy compatibility) but
none can perform encryption. The AEAD ciphers carry metadata used by the Rust layer;
the legacy ciphers exist solely for registry completeness.

### Objects that are metadata-only

| Object | Count | Alters runtime? |
|---|---|---|
| TLS wrapper | 1 | No |
| Plugin wrapper | 1 | No |
| Chain | 1 | No (data structure) |
| _ApplyCipher | 1 (per cipher) | No (identity function) |
