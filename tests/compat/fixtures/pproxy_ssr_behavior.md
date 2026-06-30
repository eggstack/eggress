# pproxy ShadowsocksR (SSR) Behavior

## Observed Behavior

This fixture documents pproxy 2.7.9's ShadowsocksR behavior as observed through
the pinned compatibility oracle. Observations are from actual pproxy invocations,
not from third-party documentation.

## How pproxy Handles SSR URIs

pproxy recognizes `ssr://` URIs and attempts to parse the SSR-specific query
parameters. The URI format is:

```
ssr://method:password@host:port?protocol=MODE&protocol_param=PARAM&obfs=MODE&obfs_param=PARAM
```

When pproxy receives an SSR URI:

1. It parses the method and password from the userinfo section.
2. It parses the host and port from the endpoint.
3. It extracts `protocol`, `protocol_param`, `obfs`, and `obfs_param` from
   the query string.
4. It uses the specified cipher method for encryption.
5. It applies the protocol and obfuscation layers to the TCP stream.

## What Happens When SSR Methods Are Used

### As Listener (`-l ssr://...`)

pproxy starts a listener that:

1. Accepts incoming TCP connections.
2. Applies the obfs layer (e.g., `http_simple` or `tls1.2_ticket_auth`).
3. Reads the encrypted payload.
4. Decrypts using the specified cipher.
5. Applies the protocol verification layer.
6. Forwards the decrypted address and payload to the upstream.

### As Upstream (`-r ssr://...`)

pproxy connects to the SSR upstream:

1. Establishes a TCP connection.
2. Applies the protocol obfuscation layer.
3. Encrypts the address header and payload using the specified cipher.
4. Applies the obfuscation layer.
5. Sends the data to the upstream server.

## What pproxy Outputs for SSR Connections

pproxy processes SSR connections transparently. The output behavior depends
on the upstream configuration:

- With `-r direct`: Decrypted traffic is forwarded directly to the target.
- With `-r http://...`: Decrypted traffic is forwarded through an HTTP proxy.
- With `-r socks5://...`: Decrypted traffic is forwarded through a SOCKS5 proxy.

pproxy does not emit special logging for SSR connections beyond its normal
debug output (when `-v` is used).

## Error Behavior for Unsupported Combinations

### Invalid Cipher Method

When an unknown cipher method is specified in an SSR URI:

```
pproxy -l ssr://unknown-cipher:pass@:12345
```

pproxy will fail to start or reject the connection, depending on how the
method is used. The error may be:

- A Python traceback (if used as a library)
- A connection reset (if the server cannot initialize the cipher)

### Invalid Protocol/Obfs Mode

When an unknown protocol or obfs mode is specified:

```
pproxy -l ssr://aes-256-cfb:pass@:12345?protocol=unknown_mode
```

pproxy will fail to initialize the protocol layer. The behavior is:

- If the protocol mode is unknown, pproxy may ignore it or fail with an error.
- If the obfs mode is unknown, pproxy may fall back to `plain` or fail.

### Mismatched Protocol/Obfs Between Client and Server

When the client and server use different protocol/obfs modes, the connection
will fail:

- The obfs layer will not complete the handshake (e.g., HTTP headers expected
  but raw data received).
- The connection will time out or reset.
- No structured error is propagated to the user.

## Known Limitations of pproxy's SSR Support

1. **No SSR UDP relay**: SSR was designed for TCP. pproxy's UDP handling with
   SSR URIs uses the underlying cipher without protocol/obfs layers.

2. **Limited obfs modes**: pproxy supports `plain`, `http_simple`, and
   `tls1.2_ticket_auth`. Other SSR implementations may support additional
   obfs modes (e.g., `random_head`).

3. **Limited protocol modes**: pproxy supports `origin`, `verify_simple`, and
   `verify_deflate`. Other SSR implementations may support additional modes.

4. **No SSR OTA**: pproxy does not implement SSR-specific OTA (which is
   distinct from Shadowsocks OTA).

## eggress Handling

When an SSR URI is encountered in eggress:

1. The `ssr://` scheme is recognized during URI parsing.
2. eggress produces a structured `UnsupportedFeature` diagnostic:
   ```
   unsupported feature: ShadowsocksR (SSR) is not supported
   ```
3. The connection or configuration is rejected.

eggress does not attempt to parse SSR query parameters or apply protocol/obfs
layers. The rejection is immediate and clear.

## Source

All observations from pproxy 2.7.9 (Python package) during Phase 7 parity
audit and subsequent phases. This fixture is the behavior oracle for SSR
compatibility claims.
