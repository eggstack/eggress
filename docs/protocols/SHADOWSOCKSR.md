# ShadowsocksR (SSR) — pproxy Behavior

This document captures pproxy 2.7.9's behavior with ShadowsocksR (SSR). All
observations are from the pinned pproxy oracle. This is not derived from
third-party SSR documentation.

## Overview

ShadowsocksR (SSR) is a fork of Shadowsocks that adds protocol obfuscation
layers and additional cipher modes. pproxy supports SSR as both a listener and
an upstream protocol. eggress intentionally does not implement SSR.

**Classification**: Intentional non-parity for eggress.

## URI Scheme and Aliases

pproxy recognizes SSR URIs using the `ssr://` scheme. The URI format extends
the Shadowsocks URI with additional fields:

```
ssr://method:password@host:port?protocol=MODE&protocol_param=PARAM&obfs=MODE&obfs_param=PARAM
```

Aliases used by pproxy:

| Scheme | Notes |
|--------|-------|
| `ssr://` | Primary SSR URI scheme |

## URI Grammar

The full SSR URI grammar as parsed by pproxy:

```
ssr://BASE64(method:password@host:port)
ssr://method:password@host:port?protocol=MODE&protocol_param=PARAM&obfs=MODE&obfs_param=PARAM
```

Fields:

| Field | Description | Example |
|-------|-------------|---------|
| `method` | Cipher method name | `aes-256-cfb` |
| `password` | Authentication password | `mypassword` |
| `host` | Server hostname or IP | `10.0.0.1` |
| `port` | Server port | `12345` |
| `protocol` | Protocol obfuscation mode | `origin`, `verify_simple`, `verify_deflate` |
| `protocol_param` | Protocol-specific parameter | (varies by mode) |
| `obfs` | Obfuscation mode | `plain`, `http_simple`, `tls1.2_ticket_auth` |
| `obfs_param` | Obfuscation-specific parameter | (varies by mode) |

Note: pproxy may also accept base64-encoded SSR URIs where the entire payload
after `ssr://` is base64-encoded.

## Supported SSR Cipher Methods

pproxy supports the following ciphers for SSR connections:

| Cipher | Key Size | IV Size |
|--------|----------|---------|
| `aes-128-cfb` | 16 bytes | 16 bytes |
| `aes-192-cfb` | 24 bytes | 16 bytes |
| `aes-256-cfb` | 32 bytes | 16 bytes |
| `aes-128-ctr` | 16 bytes | 16 bytes |
| `aes-192-ctr` | 24 bytes | 16 bytes |
| `aes-256-ctr` | 32 bytes | 16 bytes |
| `rc4-md5` | 16 bytes | 16 bytes |
| `chacha20-ietf` | 32 bytes | 12 bytes |
| `salsa20` | 32 bytes | 8 bytes |
| `xchacha20` | 32 bytes | 24 bytes |
| `chacha20-ietf-poly1305` | 32 bytes | 12 bytes |

Note: SSR supports both stream ciphers and (in some implementations) AEAD
ciphers. The AEAD methods in SSR use a different wire format than standard
Shadowsocks AEAD.

## Supported SSR Protocol Modes

pproxy supports the following protocol obfuscation modes:

| Mode | Description |
|------|-------------|
| `origin` | No protocol obfuscation; raw data after encryption |
| `verify_simple` | Simple verification hash appended to initial payload |
| `verify_deflate` | Deflate-compressed verification in initial payload |

### Protocol Mode Behavior

- **`origin`**: The protocol layer is a no-op. Encrypted data is forwarded
  directly after the initial handshake.

- **`verify_simple`**: A verification hash is computed and appended to the
  initial payload. The remote side verifies this hash to detect protocol
  mismatches.

- **`verify_deflate`**: The initial payload is deflate-compressed before
  encryption. The remote side decompresses and verifies.

## Supported SSR Obfs Modes

pproxy supports the following obfuscation modes:

| Mode | Description |
|------|-------------|
| `plain` | No obfuscation; raw encrypted data |
| `http_simple` | HTTP-like request/response headers prepended to disguise traffic |
| `tls1.2_ticket_auth` | TLS 1.2 session ticket-like handshake to disguise traffic |

### Obfs Mode Behavior

- **`plain`**: The obfs layer is a no-op. Encrypted data flows directly.

- **`http_simple`**: The connection begins with an HTTP-like request
  (`GET / HTTP/1.1\r\n...`) followed by fake HTTP response headers. Data is
  then sent as "HTTP body" after the headers.

- **`tls1.2_ticket_auth`**: The connection begins with a TLS 1.2-like
  ClientHello containing a session ticket. The server responds with a
  ServerHello-like message. Data is then sent after the handshake.

## OTA Behavior

SSR does not use Shadowsocks OTA (One-Time Authentication). SSR's protocol
and obfs layers provide a different form of obfuscation, not authentication.
The password is used for encryption key derivation only.

## TCP Behavior

SSR TCP connections consist of the following layers:

### Wire Format (SSR with protocol=origin, obfs=plain)

```
+--------+----------------------------------------------+
|  IV    |  Encrypted( address_header + payload )       |
+--------+----------------------------------------------+
  16 bytes            variable (continuous stream)
```

This is identical to legacy Shadowsocks stream ciphers when no protocol/obfs
layers are applied.

### Wire Format (SSR with obfs=http_simple)

```
+------------------------------------------------------+
|  HTTP-like request headers (cleartext)              |
+------------------------------------------------------+
|  HTTP-like response headers (cleartext)             |
+------------------------------------------------------+
|  IV + Encrypted( address_header + payload )          |
+------------------------------------------------------+
```

The HTTP headers disguise the traffic as HTTP. The encrypted payload is sent
as the "HTTP body" after the headers.

### Wire Format (SSR with obfs=tls1.2_ticket_auth)

```
+------------------------------------------------------+
|  TLS 1.2 ClientHello (cleartext, with session ticket)|
+------------------------------------------------------+
|  TLS 1.2 ServerHello (cleartext)                    |
+------------------------------------------------------+
|  IV + Encrypted( address_header + payload )          |
+------------------------------------------------------+
```

The TLS-like handshake disguises the traffic as TLS. The encrypted payload is
sent after the handshake.

### Key Differences from Standard Shadowsocks AEAD

| Aspect | SSR | Standard AEAD |
|--------|-----|---------------|
| Authentication | None (stream cipher) | 16-byte tag per chunk |
| Length framing | None (continuous stream) | 2-byte encrypted length |
| Obfuscation | protocol + obfs layers | None |
| Wire format | Non-standard | SIP003 standard |
| Interoperability | SSR-only | Cross-implementation |

## UDP Behavior

pproxy's SSR support for UDP is limited. SSR was designed primarily for TCP
traffic. UDP relay through SSR uses the underlying stream cipher without the
protocol/obfs layers (since UDP is connectionless).

## Authentication Failure Behavior

When an SSR connection fails due to incorrect password or method:

- The server will reject the connection after attempting decryption
- The client may receive a connection reset or timeout
- No structured error codes are defined for SSR authentication failures
- pproxy may log decryption failures but does not propagate structured errors

## Eggress Status

**Intentional non-parity**: SSR is not implemented in eggress.

When an `ssr://` URI is encountered, eggress:

1. Recognizes the `ssr` scheme during URI parsing
2. Rejects it with a structured `UnsupportedFeature` diagnostic
3. The diagnostic message includes: "ShadowsocksR (SSR) is not supported"

No feature gate is needed since nothing is implemented.

### Rationale

1. **Security**: SSR protocol and obfs layers provide obfuscation, not
   authentication. The underlying ciphers are stream ciphers with no
   authentication tags, making them vulnerable to bit-flipping attacks.

2. **No RFC**: SSR has no formal specification or RFC. The protocol is
   defined by implementation (a fork of a fork).

3. **Maintenance burden**: Implementing protocol + obfs layers adds significant
   code complexity with no security benefit over standard AEAD.

4. **No clear use case**: Modern proxy deployments should use AEAD ciphers
   with authenticated encryption. SSR's obfuscation layers are a historical
   artifact from a period when traffic obfuscation was needed to evade
   deep packet inspection.

## Source

All behavior captured from pproxy 2.7.9 (Python package) during Phase 7
parity audit and subsequent phases. pproxy 2.7.9 is pinned as the behavior
oracle for all compatibility claims.
