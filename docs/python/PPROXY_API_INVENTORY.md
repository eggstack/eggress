# pproxy 2.7.9 — Static API Inventory

Comprehensive inventory of the pproxy Python package API surface with tier
classification against eggress (Rust).

> Source: `/Library/Frameworks/Python.framework/Versions/3.14/lib/python3.14/site-packages/pproxy/`
> Snapshot: `tests/compat/fixtures/pproxy_api_snapshot.json`

---

## Tier Legend

| Tier | Name | Definition |
|------|------|------------|
| **A** | Exact | Same API name, same parameters, same return type, same behavior |
| **B** | Functional | Different API shape but achieves the same outcome |
| **C** | Partial | Some aspects covered, others missing |
| **D** | Deferred | Intentionally not yet covered (future work) |
| **N/A** | Not applicable | Design does not apply to eggress (different architecture) |

---

## 1. Module-Level Exports (`pproxy.__init__`)

| Symbol | Type | pproxy Signature | Description | Tier | Rationale |
|--------|------|------------------|-------------|------|-----------|
| `Connection` | alias | `pproxy.Connection(uris)` | Builds proxy chain from `__`-separated URIs → `ProxyDirect`/`ProxySimple`/etc. | **B** | `EggressConfig::from_uri()` achieves same via TOML; different API shape |
| `Server` | alias | `pproxy.Server(uris)` | Alias for `proxies_by_uri` | **B** | Same as `Connection`; routed through `EggressService::start()` |
| `Rule` | function | `pproxy.Rule(filename_or_pattern)` | Compiles a regex block/inline rule → `re.match` callable | **B** | eggress rule engine compiles TOML matchers to AST; different shape, same outcome |
| `DIRECT` | constant | `pproxy.DIRECT` → `ProxyDirect` | Sentinel representing a direct (no-proxy) connection | **A** | eggress `direct` action maps to the same semantics |

---

## 2. Protocol Classes (`pproxy.proto`)

### 2.1 BaseProtocol (abstract)

| Method | pproxy Signature | Description | Tier | Rationale |
|--------|------------------|-------------|------|-----------|
| `__init__(param)` | `(param=None)` | Construct with optional parameter string | N/A | eggress uses config structs, not runtime class constructors |
| `.name` | `property → str` | Lowercase class name | **B** | eggress uses `Protocol::name()` enum method |
| `.reuse()` | `() → bool` | Whether connections can be multiplexed | **D** | Connection pooling/ multiplexing deferred |
| `.channel(reader, writer, stat_bytes, stat_conn)` | `async` | Bidirectional relay loop | **B** | eggress `relay::bidirectional()` equivalent; different call shape |
| `.connect(reader, writer, rauth, host, port)` | `async` | Send protocol-specific upstream handshake | **B** | eggress per-protocol `UpstreamHandshake` |
| `.udp_accept(data)` | `() → (user, host, port, payload)` | Parse inbound UDP datagram | **B** | eggress codec-based UDP parsing |
| `.udp_connect(rauth, host, port, data)` | `() → bytes` | Build outbound UDP datagram | **B** | eggress codec-based UDP framing |
| `.udp_pack(host, port, data)` | `() → bytes` | Wrap payload with address header | **B** | eggress datagram codec |
| `.udp_unpack(data)` | `() → bytes` | Strip address header, return payload | **B** | eggress datagram codec |

### 2.2 Direct

| Method | Tier | Rationale |
|--------|------|-----------|
| All inherited from `BaseProtocol` | **A** | eggress `direct` action; same semantics |

**eggress support:** Inbound: **Yes** | Upstream: **Yes**

### 2.3 HTTP (scheme: `http`)

| Method | pproxy Signature | Tier | Rationale |
|--------|------------------|------|-----------|
| `guess(reader)` | `async → bool` | **A** | eggress `detect_protocol()` checks HTTP verb prefix |
| `accept(reader, user, writer)` | `async → (user, host, port, connected_cb)` | **B** | eggress HTTP listener returns parsed request; callback shape differs |
| `http_accept(user, method, path, authority, ver, lines, host, pauth, reply)` | `async` | **B** | Consolidated into `HttpInbound::accept()` |
| `connect(reader, writer, rauth, host, port)` | `async` | **A** | eggress sends identical `CONNECT host:port HTTP/1.1` |
| `http_channel(reader, writer, stat_bytes, stat_conn)` | `async` | **D** | HTTP tunnel channel (non-CONNECT forwarding) deferred |

**eggress support:** Inbound: **Yes** | Upstream: **Yes**

### 2.4 HTTPOnly (scheme: `httponly`)

| Method | Tier | Rationale |
|--------|------|-----------|
| `connect(reader, writer, rauth, host, port, myhost)` | **D** | HTTP-only proxy mode (no CONNECT tunnel); not in eggress scope |

**eggress support:** Inbound: **No** | Upstream: **No**

### 2.5 Socks4 (scheme: `socks4`)

| Method | pproxy Signature | Tier | Rationale |
|--------|------------------|------|-----------|
| `guess(reader)` | `async → bool` | **A** | eggress checks `0x04` version byte |
| `accept(reader, user, writer, users, authtable)` | `async → (user, host, port)` | **B** | eggress parses SOCKS4 request struct; user/auth handled via config |
| `connect(reader, writer, rauth, host, port)` | `async` | **A** | Identical SOCKS4 connect request (`0x04 0x01`) |

**eggress support:** Inbound: **Yes** | Upstream: **No**

### 2.6 Socks5 (scheme: `socks5`)

| Method | pproxy Signature | Tier | Rationale |
|--------|------------------|------|-----------|
| `guess(reader)` | `async → bool` | **A** | eggress checks `0x05` version byte |
| `accept(reader, user, writer, users, authtable)` | `async → (user, host, port)` | **B** | eggress parses SOCKS5 handshake + auth negotiation |
| `connect(reader, writer, rauth, host, port)` | `async` | **A** | Identical SOCKS5 connect flow |
| `udp_accept(data)` | `() → (user, host, port, payload)` | **B** | eggress SOCKS5 UDP relay parses same `\x00\x00\x00` header |
| `udp_connect(rauth, host, port, data)` | `() → bytes` | **B** | eggress builds same `\x00\x00\x00\x03` datagram |

**eggress support:** Inbound: **Yes** | Upstream: **Yes**

### 2.7 SS (Shadowsocks, scheme: `ss`)

| Method | pproxy Signature | Tier | Rationale |
|--------|------------------|------|-----------|
| `guess(reader)` | `async → bool` | **B** | Inherited from SSR; eggress detects SS via AEAD salt |
| `accept(reader, user, reader_cipher)` | `async → (user, host, port)` | **B** | eggress reads AEAD-encrypted address header |
| `connect(reader, writer, rauth, host, port, writer_cipher_r)` | `async` | **B** | eggress writes AEAD-encrypted address header |
| `patch_ota_reader / patch_ota_writer` | OTA chunk authentication | **N/A** | OTA is legacy; eggress uses AEAD only |
| `udp_accept(data, users)` | `() → (user, host, port, payload)` | **B** | eggress UDP uses same AEAD frame format |
| `udp_connect(rauth, host, port, data)` | `() → bytes` | **B** | eggress builds same UDP datagram |
| `udp_pack / udp_unpack` | Address header wrap/unwrap | **B** | eggress datagram codec |

**eggress support:** Inbound: **Yes** | Upstream: **Yes**

### 2.8 SSR (ShadowsocksR, scheme: `ssr`)

| Method | Tier | Rationale |
|--------|------|-----------|
| All methods | **N/A** | SSR is intentionally unsupported; eggress rejects SSR URIs with diagnostic |

**eggress support:** Inbound: **No** | Upstream: **No**

### 2.9 Trojan (scheme: `trojan`)

| Method | pproxy Signature | Tier | Rationale |
|--------|------------------|------|-----------|
| `guess(reader)` | `async → user\|True` | **A** | eggress checks SHA-224 hash of password |
| `accept(reader, user)` | `async → (user, host, port)` | **A** | Identical `\r\n` + `0x01` + address + `\r\n` framing |
| `connect(reader, writer, rauth, host, port)` | `async` | **A** | Identical Trojan connect header |

**eggress support:** Inbound: **Yes** | Upstream: **Yes**

### 2.10 SSH (scheme: `ssh`)

| Method | Tier | Rationale |
|--------|------|-----------|
| `connect(reader, writer, rauth, host, port, myhost)` | **D** | SSH tunnel mode requires `asyncssh`; deferred |

**eggress support:** Inbound: **No** | Upstream: **No**

### 2.11 Transparent (base for Redir/Pf/Tunnel/Echo)

| Method | pproxy Signature | Tier | Rationale |
|--------|------------------|------|-----------|
| `guess(reader, sock)` | `async → bool` | **B** | eggress uses `SO_ORIGINAL_DST` / PF ioctl directly |
| `accept(reader, user, sock)` | `async → (user, host, port)` | **B** | eggress transparent listener reads original destination from socket option |
| `udp_accept(data, sock)` | `() → (user, host, port, payload)` | **B** | eggress transparent UDP same approach |

**eggress support:** Inbound: **Yes** (Linux: Redir, macOS: PF via platform capability model) | Upstream: **N/A**

### 2.12 Redir (scheme: `redir`)

| Method | Tier | Rationale |
|--------|------|-----------|
| `query_remote(sock)` | Linux `SO_ORIGINAL_DST` ioctl | **A** | eggress `transparent::query_original_dest()` uses identical syscall |

**eggress support:** Inbound: **Yes** (Linux only) | Upstream: **N/A**

### 2.13 Pf (scheme: `pf`)

| Method | Tier | Rationale |
|--------|------|-----------|
| `query_remote(sock)` | macOS PF `/dev/pf` ioctl | **B** | eggress PF support deferred; platform capability reports `KernelUnsupported` on macOS |

**eggress support:** Inbound: **D** (macOS PF deferred) | Upstream: **N/A**

### 2.14 Tunnel (scheme: `tunnel`)

| Method | Tier | Rationale |
|--------|------|-----------|
| `query_remote(sock)` | Fixed target from param | **A** | eggress `raw` tunnel has same semantics |
| `connect()` | No-op | **A** | Same — raw tunnel does not negotiate |
| `udp_connect()` | Returns data unwrapped | **A** | Same |

**eggress support:** Inbound: **No** (protocol-crate only) | Upstream: **No** (protocol-crate only)

### 2.15 WS (WebSocket, scheme: `ws`)

| Method | Tier | Rationale |
|--------|------|-----------|
| All methods | **D** | WebSocket tunnel is protocol-crate only; not integrated as inbound/upstream |

**eggress support:** Inbound: **No** (protocol-crate only) | Upstream: **No** (protocol-crate only)

### 2.16 H2 (HTTP/2 CONNECT, scheme: `h2`)

| Method | Tier | Rationale |
|--------|------|-----------|
| All methods | **D** | H2 CONNECT is protocol-crate only; not integrated as inbound/upstream |

**eggress support:** Inbound: **No** (protocol-crate only) | Upstream: **No** (protocol-crate only)

### 2.17 H3 (HTTP/3, scheme: `h3`)

| Method | Tier | Rationale |
|--------|------|-----------|
| All methods | **N/A** | QUIC/HTTP3 deferred by ADR |

**eggress support:** Inbound: **No** | Upstream: **No**

### 2.18 Echo (scheme: `echo`)

| Method | Tier | Rationale |
|--------|------|-----------|
| `query_remote(sock)` | Returns `('echo', 0)` | **N/A** | Test/debug utility; not needed in production proxy |

**eggress support:** Inbound: **N/A** | Upstream: **N/A**

### Module-Level Functions

| Function | pproxy Signature | Description | Tier | Rationale |
|----------|------------------|-------------|------|-----------|
| `accept(protos, reader, **kw)` | `async → (proto, user, host, port, ...)` | Iterate protocol detectors, return matched protocol + parsed request | **B** | eggress `ProtocolDetector` chain does same; different call shape |
| `udp_accept(protos, data, **kw)` | `→ (proto, user, host, port, data)` | Iterate UDP protocol detectors | **B** | eggress UDP codec dispatch |
| `get_protos(rawprotos)` | `→ (err, [proto instances])` | Parse protocol strings, instantiate protocol objects | **B** | eggress config compiler resolves protocol names to `ProtocolHandler` |
| `sslwrap(reader, writer, sslcontext, ...)` | `async → (reader, writer)` | TLS wrapper using `asyncio.sslproto` | **B** | eggress uses `tokio-rustls`; different crate, same outcome |
| `netloc_split(loc, default_host, default_port)` | `→ (host, port)` | Parse `host:port` with IPv6 support | **A** | eggress URI parser does identical splitting |
| `MAPPINGS` | `dict[str, type]` | Scheme → protocol class mapping | **B** | eggress `compile_protocol()` uses match statement |
| `HTTP_LINE` | `re.Pattern` | Regex for HTTP request line | **N/A** | Internal parsing detail |

---

## 3. Cipher Classes (`pproxy.cipher` + `pproxy.cipherpy`)

### 3.1 Base Classes

| Class | Description | Tier | Rationale |
|-------|-------------|------|-----------|
| `BaseCipher(key, ota, setup_key)` | Base with MD5 key derivation, `setup_iv()`, `encrypt()`/`decrypt()` | **N/A** | eggress uses `ring`/`chacha20poly1305` crates; different architecture |
| `AEADCipher` | AEAD base with chunked encrypt/decrypt (length-prefixed + tags) | **B** | Same wire format (SIP003 AEAD), different internal implementation |
| `PacketCipher(cipher, key, name)` | Per-packet cipher wrapper for UDP | **B** | Same concept, different shape |

### 3.2 AEAD Ciphers (pycryptodome)

| Class | Key Length | IV Length | Nonce | Tag | Tier | Rationale |
|-------|-----------|-----------|-------|-----|------|-----------|
| `AES_128_GCM_Cipher` | 16 | 16 | 12 | 16 | **A** | Identical SIP003 AEAD; same HKDF-SHA1 subkey derivation |
| `AES_192_GCM_Cipher` | 24 | 24 | 12 | 16 | **C** | pproxy supports; eggress does not (AES-128-GCM + AES-256-GCM only) |
| `AES_256_GCM_Cipher` | 32 | 32 | 12 | 16 | **A** | Identical SIP003 AEAD |
| `ChaCha20_IETF_POLY1305_Cipher` | 32 | 32 | 12 | 16 | **A** | Identical SIP003 AEAD |

### 3.3 Stream/Block Ciphers (pycryptodome)

| Class | Key Length | IV Length | Tier | Rationale |
|-------|-----------|-----------|------|-----------|
| `RC4_Cipher` | 16 | 0 | **N/A** | Legacy stream cipher; unsupported |
| `RC4_MD5_Cipher` | 16 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `ChaCha20_Cipher` | 32 | 8 | **N/A** | Legacy stream cipher; unsupported |
| `ChaCha20_IETF_Cipher` | 32 | 12 | **N/A** | Legacy stream cipher; unsupported |
| `Salsa20_Cipher` | 32 | 8 | **N/A** | Legacy stream cipher; unsupported |
| `AES_128_CFB_Cipher` | 16 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_192_CFB_Cipher` | 24 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_256_CFB_Cipher` | 32 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_128_CFB8_Cipher` | 16 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_192_CFB8_Cipher` | 24 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_256_CFB8_Cipher` | 32 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_128_OFB_Cipher` | 16 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_192_OFB_Cipher` | 24 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_256_OFB_Cipher` | 32 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_128_CTR_Cipher` | 16 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_192_CTR_Cipher` | 24 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `AES_256_CTR_Cipher` | 32 | 16 | **N/A** | Legacy stream cipher; unsupported |
| `BF_CFB_Cipher` | 16 | 8 | **N/A** | Legacy stream cipher; unsupported |
| `CAST5_CFB_Cipher` | 16 | 8 | **N/A** | Legacy stream cipher; unsupported |
| `DES_CFB_Cipher` | 8 | 8 | **N/A** | Legacy stream cipher; unsupported |

### 3.4 Pure Python Ciphers (`cipherpy.py`)

| Class | Key Length | IV Length | Tier | Rationale |
|-------|-----------|-----------|------|-----------|
| `Table_Cipher` | 0 | 0 | **N/A** | Obfuscation-only; no real security |
| `RC4_Cipher` (py) | 16 | 0 | **N/A** | Legacy; unsupported |
| `RC4_MD5_Cipher` (py) | 16 | 16 | **N/A** | Legacy; unsupported |
| `ChaCha20_Cipher` (py) | 32 | 8 | **N/A** | Legacy; unsupported |
| `ChaCha20_IETF_Cipher` (py) | 32 | 12 | **N/A** | Legacy; unsupported |
| `XChaCha20_Cipher` (py) | 32 | 24 | **N/A** | Legacy; unsupported |
| `XChaCha20_IETF_Cipher` (py) | 32 | 28 | **N/A** | Legacy; unsupported |
| `ChaCha20_IETF_POLY1305_Cipher` (py) | 32 | 32 | **A** | Pure-Python fallback for same AEAD cipher |
| `XChaCha20_IETF_POLY1305_Cipher` (py) | 32 | 44 | **N/A** | Not standard SIP003; unsupported |
| `Salsa20_Cipher` (py) | 32 | 8 | **N/A** | Legacy; unsupported |
| `AES_{128,192,256}_CFB_Cipher` (py) | varies | 16 | **N/A** | Legacy; unsupported |
| `AES_{128,192,256}_CFB8_Cipher` (py) | varies | 16 | **N/A** | Legacy; unsupported |
| `AES_{128,192,256}_CTR_Cipher` (py) | varies | 16 | **N/A** | Legacy; unsupported |
| `AES_{128,192,256}_OFB_Cipher` (py) | varies | 16 | **N/A** | Legacy; unsupported |
| `AES_{128,192,256}_GCM_Cipher` (py) | varies | varies | **A** | Pure-Python AEAD fallback |
| `BF_CFB_Cipher` (py) | 16 | 8 | **N/A** | Legacy; unsupported |
| `Camellia_{128,192,256}_CFB_Cipher` (py) | varies | 16 | **N/A** | Legacy; unsupported |
| `IDEA_CFB_Cipher` (py) | 16 | 8 | **N/A** | Legacy; unsupported |
| `SEED_CFB_Cipher` (py) | 16 | 16 | **N/A** | Legacy; unsupported |
| `RC2_CFB_Cipher` (py) | 16 | 8 | **N/A** | Legacy; unsupported |

### Cipher Summary

| Cipher | pproxy | eggress | Tier |
|--------|--------|---------|------|
| AES-128-GCM | Yes (pycryptodome + pure py) | Yes (ring crate) | **A** |
| AES-256-GCM | Yes (pycryptodome + pure py) | Yes (ring crate) | **A** |
| ChaCha20-IETF-Poly1305 | Yes (pycryptodome + pure py) | Yes (chacha20poly1305 crate) | **A** |
| AES-192-GCM | Yes | No | **C** |
| All other ciphers | Yes | No (rejected as legacy) | **N/A** |

---

## 4. Scheduling Algorithms

| Name | pproxy Key | Description | eggress Equivalent | Tier | Rationale |
|------|------------|-------------|--------------------|------|-----------|
| First Available | `fa` | Return first alive upstream matching rule | `FirstAvailableScheduler` | **A** | Same semantics |
| Round Robin | `rr` | Rotate through alive upstreams | `RoundRobinScheduler` | **A** | Same semantics (global atomic cursor) |
| Random Choice | `rc` | Random selection from alive upstreams | `RandomScheduler` | **A** | Same semantics |
| Least Connections | `lc` | Select upstream with fewest active connections | `LeastConnectionsScheduler` | **A** | Same semantics (active + in_flight) |

---

## 5. Plugins (`pproxy.plugin`)

| Name | Purpose | Tier | Rationale |
|------|---------|------|-----------|
| `Plain_Plugin` | No-op plugin | **N/A** | Not needed |
| `Origin_Plugin` | No-op plugin (alias) | **N/A** | Not needed |
| `Http_Simple_Plugin` | HTTP-obfuscated transport (hex-encoded payload in GET path) | **N/A** | Obfuscation plugin; not in eggress design |
| `Tls1__2_Ticket_Auth_Plugin` | TLS 1.2 ticket auth obfuscation | **N/A** | Obfuscation plugin; not in eggress design |
| `Verify_Simple_Plugin` | CRC32 frame verification | **N/A** | Obfuscation plugin; not in eggress design |
| `Verify_Deflate_Plugin` | zlib compression + CRC verification | **N/A** | Obfuscation plugin; not in eggress design |

---

## 6. Server Functions (`pproxy.server`)

### 6.1 Core Classes

| Class | pproxy Signature | Description | Tier | Rationale |
|-------|------------------|-------------|------|-----------|
| `AuthTable(remote_ip, authtime)` | Time-based auth cache per IP | **B** | eggress uses per-connection auth; different lifecycle |
| `ProxyDirect` | Direct connection (no proxy) | **A** | eggress `direct` action |
| `ProxySimple` | Single-hop proxy with protocol + cipher | **B** | eggress upstream config; different shape |
| `ProxyH2` | HTTP/2 connection multiplexer proxy | **D** | H2 CONNECT is protocol-crate only |
| `ProxyQUIC` | QUIC transport proxy | **N/A** | QUIC/HTTP3 deferred by ADR |
| `ProxyH3` | HTTP/3 over QUIC proxy | **N/A** | QUIC/HTTP3 deferred by ADR |
| `ProxySSH` | SSH tunnel proxy | **D** | SSH tunnel deferred |
| `ProxyBackward` | Reverse/backward proxy (inbound connections) | **B** | eggress reverse protocol crate; different shape |

### 6.2 Functions

| Function | pproxy Signature | Description | Tier | Rationale |
|----------|------------------|-------------|------|-----------|
| `compile_rule(filename)` | `→ re.match callable` | Compile regex block/inline rule | **B** | eggress rule engine compiles to AST; different shape |
| `proxy_by_uri(uri, jump)` | `→ ProxyDirect\|ProxySimple\|...` | Parse single URI into proxy object | **B** | eggress `EggressConfig::from_uri()` |
| `proxies_by_uri(uri_jumps)` | `→ proxy` | Parse `__`-separated URI chain | **B** | eggress `EggressConfig::from_uri()` (chain support) |
| `prepare_ciphers(cipher, reader, writer, ...)` | `async → (reader_cipher, writer_cipher)` | Initialize cipher pair for connection | **B** | eggress cipher init is per-protocol, not global |
| `schedule(rserver, algorithm, host, port)` | `→ ProxyDirect\|ProxySimple` | Select upstream via scheduling algorithm | **A** | eggress `Scheduler::select()` |
| `stream_handler(reader, writer, ...)` | `async` | Main TCP connection handler | **B** | eggress `serve_connection()` + `execute_chain()` |
| `datagram_handler(writer, data, addr, ...)` | `async` | Main UDP datagram handler | **B** | eggress UDP association handler |
| `check_server_alive(interval, rserver, verbose)` | `async` loop | Periodic upstream health check | **B** | eggress health probe system with hysteresis |
| `test_url(url, rserver)` | `async` | Test URL through proxy chain | **N/A** | Debug utility; not needed in production |
| `print_server_started(option, server, print_fn)` | Print bound address | **N/A** | Internal logging detail |

---

## 7. CLI Arguments (`pproxy.server.main`)

| Flag | Dest | Default | Description | Tier | Rationale |
|------|------|---------|-------------|------|-----------|
| `-l` | `listen` | `http+socks4+socks5://:8080/` | TCP server URI(s) | **B** | eggress uses TOML `[[listeners]]` |
| `-r` | `rserver` | `direct` | TCP remote/upstream URI(s) | **B** | eggress uses TOML `[[upstreams]]` |
| `-ul` | `ulisten` | none | UDP server URI(s) | **B** | eggress uses TOML UDP listener config |
| `-ur` | `urserver` | `direct` | UDP remote/upstream URI(s) | **B** | eggress uses TOML UDP upstream config |
| `-b` | `block` | none | Block regex rule | **B** | eggress `[[rules]]` with `action = "reject"` |
| `-a` | `alived` | `0` | Health check interval (seconds) | **B** | eggress `health.check_interval_sec` |
| `-s` | `salgorithm` | `fa` | Scheduling algorithm | **A** | eggress `upstream.scheduler` (same keys) |
| `-d` | `debug` | `0` | Debug tracebacks | **B** | eggress `RUST_LOG=debug` |
| `-v` | `v` | `0` | Verbose output | **B** | eggress `RUST_LOG=info` / `trace` |
| `--ssl` | `sslfile` | none | SSL cert[,key] file | **B** | eggress TLS config in TOML |
| `--pac` | `pac` | none | PAC file path | **B** | eggress admin server serves PAC |
| `--get` | `gets` | `[]` | Custom HTTP GET endpoints | **N/A** | pproxy-specific static serving |
| `--auth` | `authtime` | `2592000` | Re-auth interval (seconds) | **B** | eggress auth is per-connection (no cache timeout) |
| `--sys` | `sys` | `false` | Set system proxy (macOS/Windows) | **N/A** | OS integration; not in eggress scope |
| `--reuse` | `ruport` | `false` | `SO_REUSEPORT` (Linux) | **N/A** | Linux-specific; not yet in eggress |
| `--daemon` | `daemon` | `false` | Run as daemon | **N/A** | Process management; not in eggress scope |
| `--test` | `test` | none | Test URL through proxy chain | **N/A** | Debug utility |
| `--version` | version | — | Print version | **A** | eggress `--version` |

---

## 8. URI Format

pproxy URIs follow the pattern:

```
scheme[+scheme...][!param]://[user:pass@]host:port[/path][?rule][#fragment][,plugin1,plugin2]
```

| Component | Example | Description | Tier | Rationale |
|-----------|---------|-------------|------|-----------|
| `scheme` | `http`, `socks5`, `ss`, `trojan` | Protocol selector | **B** | eggress uses protocol names in TOML config |
| `+scheme` | `http+socks5` | Multi-protocol listener | **B** | eggress supports mixed-protocol listeners via detection chain |
| `ssl` / `secure` | `ss+ssl://` | TLS transport wrapper | **B** | eggress TLS in `transport.tls` TOML section |
| `h2` | `http+h2://` | HTTP/2 transport | **D** | Protocol-crate only |
| `quic` / `h3` | `ss+quic://` | QUIC transport | **N/A** | Deferred by ADR |
| `ssh` | `ssh://` | SSH transport | **D** | Deferred |
| `in` | `socks5+in://` | Reverse (backward) proxy mode | **B** | eggress reverse protocol crate |
| `!param` | `socks5!timeout://` | Protocol parameter | **D** | eggress handles via config struct fields |
| `user:pass` | `ss:user:pass@host:port` | Credentials (also `cipher:key` for SS) | **B** | eggress TOML `credentials` or URI inline |
| `cipher:key` | `aes-256-gcm:secret@host:port` | Shadowsocks cipher config | **B** | eggress `cipher_method` + `password` fields |
| `/path` | `/path` | Bind path (unix socket) or HTTP path | **B** | eggress unix socket config |
| `?rule` | `?example.com` | Inline regex rule | **B** | eggress `[[rules]]` with `matcher` |
| `#fragment` | `#password` | Password (or `#filepath` for file) | **B** | eggress `password` or `password_file` |
| `,plugin` | `,tls1_2_ticket_auth` | Plugin chain | **N/A** | Plugins are obfuscation; not in eggress design |
| `__` separator | `ss://c:k@h:p__http://h:p` | Chain of proxies | **B** | eggress `chain` TOML config |

---

## Summary Statistics

| Category | Total Items | A (Exact) | B (Functional) | C (Partial) | D (Deferred) | N/A |
|----------|-------------|-----------|----------------|-------------|--------------|-----|
| Module exports | 4 | 1 | 3 | 0 | 0 | 0 |
| Protocol classes | 18 | 7 | 7 | 0 | 3 | 1 |
| Cipher classes | 43 | 3 | 1 | 1 | 0 | 38 |
| Scheduling algorithms | 4 | 4 | 0 | 0 | 0 | 0 |
| Plugins | 6 | 0 | 0 | 0 | 0 | 6 |
| Server functions | 11 | 2 | 6 | 0 | 0 | 3 |
| CLI arguments | 16 | 2 | 9 | 0 | 0 | 5 |
| URI components | 12 | 1 | 8 | 0 | 2 | 1 |
| **Total** | **114** | **20** | **34** | **1** | **5** | **54** |
