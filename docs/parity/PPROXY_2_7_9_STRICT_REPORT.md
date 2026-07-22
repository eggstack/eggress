# pproxy 2.7.9 Strict Compatibility Report

> **CORRECTIVE PASS NOTICE:** This report was regenerated as part of the
> Milestones A–C Corrective Pass (`plans/MILESTONES_A_C_CORRECTIVE_PASS.md`).
> Records using `module_existence` comparators with `drop_in` status have namespace
> evidence only and require behavioral validation before true drop_in status can be
> claimed. See the corrective pass plan for details.

**Oracle version:** pproxy==2.7.9
**Manifest schema:** strict_1
**Policy:** docs/parity/PPROXY_COMPATIBILITY_POLICY.md
**Oracle ref:** compat/pproxy-2.7.9/provenance.toml
**Commit SHA:** `3402c7eae77f1e830d41b7a5addf09dd4017b7b3`
**Manifest hash:** `fb3a53ad7f89929bb78e5db30be21b390af84ad350220efbbe0820d86ce8fba6`
**Generated:** 2026-07-22T00:00:00Z

## Summary

| Metric | Count |
|--------|-------|
| Total records | 194 |
| Terminal (resolved) | 192 |
| Gap (unresolved) | 2 |
| Needs behavioral evidence | 81 |
| Certification readiness | 58% |

### By Status

| Status | Count | Notes |
|--------|-------|-------|
| drop_in | 108 |  |
| not_applicable | 49 | Internal details, daemon, Rule |
| structural | 28 |  |
| platform_constraint | 4 | Transparent, Pf, Redir, reuse |
| intentional_non_parity | 3 | SSH, SSR |
| gap | 2 | Gap — cli.get, process.reload.routing |

### By Evidence Level

| Evidence Level | Count | Notes |
|----------------|-------|-------|
| protocol_wire / failure_class / composition_validity | 67 | True behavioral evidence |
| module_existence / constant_value | 66 | **Namespace evidence only — needs behavioral validation** |
| cli_flag_parse / cli_flag_rejection | 23 | CLI parsing evidence |
| other_structural | 15 |  |
| cipher_kat / cipher_roundtrip | 13 | Cipher behavioral evidence |
| process_lifecycle | 10 | Process lifecycle evidence |

### By Category

| Category | Count |
|----------|-------|
| python_namespace | 79 |
| protocol | 34 |
| cli_option | 22 |
| composition | 20 |
| cipher | 15 |
| failure | 12 |
| process | 12 |

### By Owner

| Owner | Count |
|-------|-------|
| track-a | 101 |
| track-b | 69 |
| track-c | 24 |

### By Milestone

| Milestone | Count |
|-----------|-------|
| B | 194 |

## Gap Records

Records with unresolved `gap` status (2 total):

| ID | Status | Category | Owner | Milestone |
|----|--------|----------|-------|----------|
| cli.get | gap | cli_option | track-a | B |
| process.reload.routing | gap | process | track-c | B |

## Records Needing Behavioral Evidence

The following 81 records use structural comparators (module_existence, method_signature,
constant_value, property_existence, class_hierarchy) and have namespace-level evidence
only. These require paired oracle/candidate behavioral validation (protocol_wire,
cipher_kat, cipher_roundtrip, etc.) before their status can be upgraded to `drop_in`.

### cipher (2 records)

| ID | Comparator | Notes |
|----|-----------|-------|
| cipher.aead.nonce_property | property_existence | Milestone B: validated by existing cipher behavioral tests. |
| cipher.aead.setup_iv | method_signature | Milestone B: validated by existing cipher behavioral tests. |

### python_namespace (79 records)

| ID | Comparator | Notes |
|----|-----------|-------|
| python.pproxy | module_existence | Top-level pproxy package module. Paired oracle comparison... |
| python.pproxy.proto | module_existence | Protocol definitions module. |
| python.pproxy.server | module_existence | Server implementation module. |
| python.pproxy.cipher | module_existence | Cipher implementations module. |
| python.pproxy.Connection | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.Server | module_existence | pproxy.Server wraps the full server lifecycle. |
| python.pproxy.DIRECT | constant_value | Direct connection constant. |
| python.pproxy.Rule | module_existence | Milestone B: now an alias for compile_rule (matches oracle). |
| python.pproxy.proto_reexport | module_existence | Protocol module re-export. |
| python.pproxy.proto.BaseProtocol | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Socks5 | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.HTTP | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Socks4 | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.SS | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Trojan | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Direct | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Transparent | module_existence | Transparent proxy base class. Platform-gated to Linux (SO... |
| python.pproxy.proto.SSH | module_existence | SSH protocol handler. Intentionally not implemented. eggr... |
| python.pproxy.proto.SSR | module_existence | SSR/legacy Shadowsocks protocol handler. Intentionally re... |
| python.pproxy.proto.WS | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.H2 | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.H3 | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Tunnel | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Echo | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Pf | module_existence | macOS PF transparent proxy. Platform-gated to macOS only;... |
| python.pproxy.proto.Redir | module_existence | Linux transparent proxy via SO_ORIGINAL_DST. Platform-gat... |
| python.pproxy.proto.HTTPOnly | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Socks5.accept | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Socks5.channel | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Socks5.connect | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Socks5.udp_accept | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.HTTP.accept | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.HTTP.http_accept | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.SS.accept | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.SS.guess | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Trojan.accept | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.Trojan.guess | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.WS.patch_ws_stream | method_signature | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.accept_func | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.get_protos | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.udp_accept_func | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.socks_address | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.netloc_split | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.sslwrap | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.packstr | module_existence | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.MAPPINGS | constant_value | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.HTTP_LINE | constant_value | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.SO_ORIGINAL_DST | enum_membership | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.proto.SOL_IPV6 | enum_membership | Internal pproxy implementation detail; not part of the pu... |
| python.pproxy.cipher.BaseCipher | module_existence | Base class for all ciphers. encrypt/decrypt in the base r... |
| python.pproxy.cipher.AEADCipher | module_existence | Functional AEAD base class with encrypt, decrypt, encrypt... |
| python.pproxy.cipher.AES_256_GCM_Cipher | module_existence | Functional AES-256-GCM AEAD cipher with encrypt/decrypt r... |
| python.pproxy.cipher.AES_192_GCM_Cipher | module_existence | Functional AES-192-GCM AEAD cipher with encrypt/decrypt r... |
| python.pproxy.cipher.AES_128_GCM_Cipher | module_existence | Functional AES-128-GCM AEAD cipher with encrypt/decrypt r... |
| python.pproxy.cipher.ChaCha20_IETF_POLY1305_Cipher | module_existence | Functional ChaCha20-Poly1305 AEAD cipher with encrypt/dec... |
| python.pproxy.cipher.PacketCipher | module_existence | Functional UDP packet cipher wrapping AEAD ciphers with n... |
| python.pproxy.cipher.AES_256_CFB_Cipher | module_existence | Functional AES-256-CFB stream cipher with encrypt/decrypt... |
| python.pproxy.cipher.ChaCha20_IETF_Cipher | module_existence | Functional ChaCha20-IETF stream cipher with 12-byte nonce... |
| python.pproxy.cipher.ChaCha20_Cipher | module_existence | Functional ChaCha20 stream cipher with 8-byte IV via cryp... |
| python.pproxy.cipher.MAP | constant_value | Functional cipher registry mapping names to classes. Incl... |
| python.pproxy.cipher.get_cipher | module_existence | Functional cipher factory: parses name:password[!ota], re... |
| python.pproxy.server.AuthTable | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.ProxySimple | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.ProxyBackward | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.ProxyDirect | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.ProxyH2 | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.ProxyH3 | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.ProxySSH | module_existence | SSH is intentionally not supported by eggress. |
| python.pproxy.server.ProxyQUIC | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.main | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.compile_rule | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.check_server_alive | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.prepare_ciphers | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.proxies_by_uri | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.proxy_by_uri | module_existence | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.SOCKET_TIMEOUT | constant_value | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.UDP_LIMIT | constant_value | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.DIRECT_const | constant_value | Milestone B: properly implemented with matching signature... |
| python.pproxy.server.DUMMY | constant_value | Milestone B: properly implemented with matching signature... |

## Terminal Records

### 192 records with terminal status

### cipher (15 records)

| ID | Status | Notes |
|----|--------|-------|
| cipher.aes_256_gcm.kat | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.aes_192_gcm.kat | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.aes_128_gcm.kat | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.chacha20_ietf_poly1305.kat | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.aes_256_gcm.roundtrip | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.aes_192_gcm.roundtrip | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.aes_128_gcm.roundtrip | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.chacha20_ietf_poly1305.roundtrip | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.aead.encrypt_and_digest | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.aead.decrypt_and_verify | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.aead.nonce_property | structural | Milestone B: validated by existing cipher behavioral tests. |
| cipher.aead.setup_iv | structural | Milestone B: validated by existing cipher behavioral tests. |
| cipher.stream.cfb_roundtrip | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.stream.ctr_roundtrip | drop_in | Milestone B: validated by existing cipher behavioral tests. |
| cipher.stream.ofb_roundtrip | drop_in | Milestone B: validated by existing cipher behavioral tests. |

### cli_option (21 records)

| ID | Status | Notes |
|----|--------|-------|
| cli.listen | drop_in | Bind one or more TCP listener URIs. |
| cli.remote | drop_in | Specify upstream proxy URIs with chaining via __ separator. |
| cli.udp_listen | drop_in | Bind a standalone UDP relay socket. |
| cli.udp_remote | drop_in | Specify upstream for UDP traffic relayed via -ul. |
| cli.scheduler | drop_in | Set load-balancing algorithm (rr, fa, rc, lc). |
| cli.alive | drop_in | Set alive check interval in seconds. |
| cli.ssl | drop_in | Enable TLS on listener (certfile,keyfile). |
| cli.block | drop_in | Block connections matching regex patterns. |
| cli.rulefile | drop_in | Load routing rules from a line-based file. |
| cli.daemon | not_applicable | Fork into background. Not applicable; eggress uses systemd/launchd/supervisor... |
| cli.verbose | drop_in | Enable verbose/debug logging. Parsed and diagnosed. |
| cli.log | drop_in | Write log output to a file. Parsed and diagnosed. |
| cli.reuse | platform_constraint | Connection reuse/pooling. Platform-gated to Linux; recognized but behavior di... |
| cli.pac | drop_in | Serve a PAC file for browser auto-configuration. |
| cli.sys | drop_in | Auto-configure system proxy settings. Parsed and diagnosed. |
| cli.test | drop_in | Test all remote proxies and exit. Parsed and diagnosed. |
| cli.config | drop_in | Load configuration from TOML file. |
| cli.ipv6 | drop_in | Enable IPv6 mode. |
| cli.bind | drop_in | Bind address for outgoing connections. |
| cli.version | drop_in | Print version information and exit. |
| cli.help | drop_in | Print usage information and exit. |

### composition (20 records)

| ID | Status | Notes |
|----|--------|-------|
| composition.http_listener_tcp | drop_in | HTTP as TCP listener is valid. |
| composition.socks5_listener_tcp | drop_in | SOCKS5 as TCP listener is valid. |
| composition.socks4_listener_tcp | drop_in | SOCKS4 as TCP listener is valid. |
| composition.shadowsocks_listener_tcp | drop_in | Shadowsocks as TCP listener is valid. |
| composition.trojan_listener_tcp | drop_in | Trojan as TCP listener is valid. |
| composition.http_upstream_tcp | drop_in | HTTP as TCP upstream is valid. |
| composition.socks5_upstream_tcp | drop_in | SOCKS5 as TCP upstream is valid. |
| composition.shadowsocks_upstream_tcp | drop_in | Shadowsocks as TCP upstream is valid. |
| composition.trojan_upstream_tcp | drop_in | Trojan as TCP upstream is valid. |
| composition.socks5_listener_udp | drop_in | SOCKS5 UDP associate listener is valid. |
| composition.direct_upstream | drop_in | Direct upstream (no proxy) is valid. |
| composition.ws_upstream_tcp | drop_in | WebSocket as TCP upstream is valid. |
| composition.h2_upstream_tcp | drop_in | H2 CONNECT as TCP upstream is valid. |
| composition.raw_upstream_tcp | drop_in | Raw/tunnel as TCP upstream is valid. |
| composition.ssh_upstream_rejected | drop_in | SSH as upstream is rejected with structured diagnostic. |
| composition.ssr_upstream_rejected | drop_in | SSR as upstream is rejected with structured diagnostic. |
| composition.chain_two_hop_valid | drop_in | socks5__http two-hop chain is valid. |
| composition.chain_three_hop_valid | drop_in | socks5__http__socks5 three-hop chain is valid. |
| composition.chain_socks5_to_ws | drop_in | socks5__ws chain is valid. |
| composition.chain_socks5_to_h2 | drop_in | socks5__h2 chain is valid. |

### failure (12 records)

| ID | Status | Notes |
|----|--------|-------|
| failure.dns.resolve_error | drop_in | DNS resolution failure produces equivalent error class. |
| failure.dns.nxdomain | drop_in | DNS NXDOMAIN produces equivalent error. |
| failure.connection.refused | drop_in | Connection refused produces equivalent error class. |
| failure.connection.timeout | drop_in | Connection timeout produces equivalent error class. |
| failure.auth.invalid_credentials | drop_in | Invalid auth credentials produce equivalent error class. |
| failure.auth.missing_credentials | drop_in | Missing auth credentials produce equivalent error class. |
| failure.malformed.http_request | drop_in | Malformed HTTP request produces equivalent error. |
| failure.malformed.socks4_version | drop_in | SOCKS4 with wrong version byte produces equivalent error. |
| failure.malformed.socks4_truncated | drop_in | Truncated SOCKS4 request produces equivalent error. |
| failure.malformed.socks5_version | drop_in | SOCKS5 with wrong version byte produces equivalent error. |
| failure.upstream.connection_loss | drop_in | Upstream connection loss during relay produces equivalent error. |
| failure.policy.deny | drop_in | Policy deny (blocked by rule) produces equivalent error. |

### process (11 records)

| ID | Status | Notes |
|----|--------|-------|
| process.startup.successful | drop_in | Process starts and binds listener successfully. |
| process.startup.bind_conflict | drop_in | Startup fails with address-in-use error. |
| process.shutdown.graceful | drop_in | Process shuts down gracefully on SIGTERM/SIGINT. |
| process.shutdown.connection_drain | drop_in | Active connections are drained during shutdown grace period. |
| process.signal.sigterm | drop_in | SIGTERM triggers graceful shutdown. |
| process.signal.sigint | drop_in | SIGINT (Ctrl-C) triggers graceful shutdown. |
| process.signal.sighup | drop_in | SIGHUP triggers config reload. |
| process.daemon.unsupported | not_applicable | --daemon flag is recognized but rejected. Not applicable; eggress uses system... |
| process.multiple_listeners | drop_in | Multiple listeners bound on different ports/protocols. |
| process.upstream_health_check | drop_in | Health checks run against configured upstreams. |
| process.admin_endpoint | drop_in | Admin HTTP server serves PAC and snapshot. |

### protocol (34 records)

| ID | Status | Notes |
|----|--------|-------|
| protocol.http_connect.listener_tcp_ipv4 | drop_in | Accept HTTP CONNECT and relay to IPv4 target. |
| protocol.http_connect.listener_tcp_ipv6 | drop_in | Accept HTTP CONNECT and relay to IPv6 target. |
| protocol.http_connect.listener_tcp_domain | drop_in | Accept HTTP CONNECT and relay to domain target. |
| protocol.http_connect.auth_success | drop_in | Accept HTTP CONNECT with valid proxy credentials. |
| protocol.http_connect.auth_failure | drop_in | Reject HTTP CONNECT with invalid proxy credentials. |
| protocol.http_connect.refused | drop_in | HTTP CONNECT to refused target produces equivalent error. |
| protocol.http_connect.half_close | drop_in | Half-close handling during CONNECT tunnel. |
| protocol.http_connect.fragmented | drop_in | Fragmented payload relay in CONNECT tunnel. |
| protocol.http_connect.timeout | drop_in | Upstream connect timeout produces equivalent failure class. |
| protocol.http_forward.get | drop_in | Forward HTTP GET requests to target. |
| protocol.http_forward.post_content_length | drop_in | Forward HTTP POST with Content-Length. |
| protocol.http_forward.head | drop_in | Forward HTTP HEAD requests. |
| protocol.http_forward.chunked | drop_in | Forward HTTP requests with chunked transfer encoding. |
| protocol.http_forward.persistent | drop_in | Persistent (keep-alive) forward proxy connections. |
| protocol.http_forward.connection_close | drop_in | Connection: close header handling. |
| protocol.http_forward.auth_success | drop_in | Forward proxy with valid credentials succeeds. |
| protocol.http_forward.malformed | drop_in | Malformed HTTP request produces equivalent error. |
| protocol.http_forward.upstream_close | drop_in | Upstream connection close during forward proxy. |
| protocol.socks4.connect_ipv4 | drop_in | SOCKS4 CONNECT to IPv4 target. |
| protocol.socks4.user_id | drop_in | User ID field forwarded in CONNECT request. |
| protocol.socks4.refused | drop_in | SOCKS4 CONNECT to refused target. |
| protocol.socks4.malformed | drop_in | Malformed SOCKS4 request handling. |
| protocol.socks4a.connect_domain | drop_in | SOCKS4a CONNECT with domain resolution. |
| protocol.socks5.connect_ipv4 | drop_in | SOCKS5 CONNECT to IPv4 target. |
| protocol.socks5.connect_ipv6 | drop_in | SOCKS5 CONNECT to IPv6 target. |
| protocol.socks5.connect_domain | drop_in | SOCKS5 CONNECT with domain target. |
| protocol.socks5.connect_refused | drop_in | SOCKS5 CONNECT to refused target. |
| protocol.socks5.auth_success | drop_in | SOCKS5 with valid credentials succeeds. |
| protocol.socks5.auth_failure | drop_in | SOCKS5 with invalid credentials rejected. |
| protocol.socks5.udp_associate | drop_in | SOCKS5 UDP associate relay. |
| protocol.shadowsocks.tcp_upstream | drop_in | Standard SIP003 AEAD TCP framing; wire-compatible. |
| protocol.shadowsocks.udp_relay | drop_in | Standard AEAD UDP relay; interoperable with standard implementations. |
| protocol.trojan.upstream | drop_in | Trojan upstream with TLS and SHA224 password auth. |
| protocol.trojan.auth_failure | drop_in | Trojan auth failure produces equivalent error. |

### python_namespace (79 records)

| ID | Status | Notes |
|----|--------|-------|
| python.pproxy | structural | Top-level pproxy package module. Paired oracle comparison: symbol not found i... |
| python.pproxy.proto | structural | Protocol definitions module. |
| python.pproxy.server | structural | Server implementation module. |
| python.pproxy.cipher | structural | Cipher implementations module. |
| python.pproxy.Connection | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.Server | structural | pproxy.Server wraps the full server lifecycle. |
| python.pproxy.DIRECT | structural | Direct connection constant. |
| python.pproxy.Rule | structural | Milestone B: now an alias for compile_rule (matches oracle). |
| python.pproxy.proto_reexport | structural | Protocol module re-export. |
| python.pproxy.proto.BaseProtocol | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Socks5 | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.HTTP | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Socks4 | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.SS | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Trojan | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Direct | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Transparent | platform_constraint | Transparent proxy base class. Platform-gated to Linux (SO_ORIGINAL_DST). |
| python.pproxy.proto.SSH | intentional_non_parity | SSH protocol handler. Intentionally not implemented. eggress recommends OpenS... |
| python.pproxy.proto.SSR | intentional_non_parity | SSR/legacy Shadowsocks protocol handler. Intentionally rejected. SSR stream c... |
| python.pproxy.proto.WS | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.H2 | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.H3 | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Tunnel | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Echo | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Pf | platform_constraint | macOS PF transparent proxy. Platform-gated to macOS only; requires PF firewall. |
| python.pproxy.proto.Redir | platform_constraint | Linux transparent proxy via SO_ORIGINAL_DST. Platform-gated to Linux only. |
| python.pproxy.proto.HTTPOnly | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Socks5.accept | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Socks5.channel | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Socks5.connect | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Socks5.udp_accept | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.HTTP.accept | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.HTTP.http_accept | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.SS.accept | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.SS.guess | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Trojan.accept | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.Trojan.guess | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.WS.patch_ws_stream | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.accept_func | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.get_protos | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.udp_accept_func | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.socks_address | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.netloc_split | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.sslwrap | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.packstr | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.MAPPINGS | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.HTTP_LINE | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.SO_ORIGINAL_DST | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.proto.SOL_IPV6 | not_applicable | Internal pproxy implementation detail; not part of the public source-compatib... |
| python.pproxy.cipher.BaseCipher | not_applicable | Base class for all ciphers. encrypt/decrypt in the base raises UnsupportedFea... |
| python.pproxy.cipher.AEADCipher | not_applicable | Functional AEAD base class with encrypt, decrypt, encrypt_and_digest, decrypt... |
| python.pproxy.cipher.AES_256_GCM_Cipher | not_applicable | Functional AES-256-GCM AEAD cipher with encrypt/decrypt round-trip and NIST K... |
| python.pproxy.cipher.AES_192_GCM_Cipher | not_applicable | Functional AES-192-GCM AEAD cipher with encrypt/decrypt round-trip validation. |
| python.pproxy.cipher.AES_128_GCM_Cipher | not_applicable | Functional AES-128-GCM AEAD cipher with encrypt/decrypt round-trip validation. |
| python.pproxy.cipher.ChaCha20_IETF_POLY1305_Cipher | not_applicable | Functional ChaCha20-Poly1305 AEAD cipher with encrypt/decrypt round-trip and ... |
| python.pproxy.cipher.PacketCipher | not_applicable | Functional UDP packet cipher wrapping AEAD ciphers with nonce/tag framing. |
| python.pproxy.cipher.AES_256_CFB_Cipher | not_applicable | Functional AES-256-CFB stream cipher with encrypt/decrypt round-trip via cryp... |
| python.pproxy.cipher.ChaCha20_IETF_Cipher | not_applicable | Functional ChaCha20-IETF stream cipher with 12-byte nonce via cryptography ba... |
| python.pproxy.cipher.ChaCha20_Cipher | not_applicable | Functional ChaCha20 stream cipher with 8-byte IV via cryptography backend. |
| python.pproxy.cipher.MAP | not_applicable | Functional cipher registry mapping names to classes. Includes -py aliases for... |
| python.pproxy.cipher.get_cipher | not_applicable | Functional cipher factory: parses name:password[!ota], returns (error, ApplyC... |
| python.pproxy.server.AuthTable | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.server.ProxySimple | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.server.ProxyBackward | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.server.ProxyDirect | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.server.ProxyH2 | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.server.ProxyH3 | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.server.ProxySSH | intentional_non_parity | SSH is intentionally not supported by eggress. |
| python.pproxy.server.ProxyQUIC | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.server.main | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.server.compile_rule | structural | Milestone B: properly implemented with matching signature and behavior. |
| python.pproxy.server.check_server_alive | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.server.prepare_ciphers | structural | Milestone B: properly implemented with matching signature and behavior. |
| python.pproxy.server.proxies_by_uri | structural | Milestone B: properly implemented with matching signature and behavior. |
| python.pproxy.server.proxy_by_uri | structural | Milestone B: properly implemented with matching signature and behavior. Paire... |
| python.pproxy.server.SOCKET_TIMEOUT | structural | Milestone B: properly implemented with matching signature and behavior. |
| python.pproxy.server.UDP_LIMIT | structural | Milestone B: properly implemented with matching signature and behavior. |
| python.pproxy.server.DIRECT_const | structural | Milestone B: properly implemented with matching signature and behavior. |
| python.pproxy.server.DUMMY | structural | Milestone B: properly implemented with matching signature and behavior. |

