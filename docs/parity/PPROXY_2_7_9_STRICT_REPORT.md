# pproxy 2.7.9 Strict Compatibility Report

> **CORRECTIVE PASS NOTICE:** This report was regenerated as part of the
> Milestones A–C Corrective Pass (`plans/MILESTONES_A_C_CORRECTIVE_PASS.md`).
> The previous report was stale (showed 83 gaps when the manifest had been updated).
> Records using `module_existence` comparators with `drop_in` status have namespace
> evidence only and require behavioral validation before true drop_in status can be
> claimed. See the corrective pass plan for details.

**Oracle version:** pproxy==2.7.9
**Manifest schema:** strict_1
**Policy:** docs/parity/PPROXY_COMPATIBILITY_POLICY.md
**Oracle ref:** compat/pproxy-2.7.9/provenance.toml

## Summary

| Metric | Count |
|--------|-------|
| Total records | 194 |
| Terminal (resolved) | 192 |
| Gap (unresolved) | 2 |
| Needs behavioral evidence | 90 |
| Certification readiness | 57% |

### By Status

| Status | Count | Notes |
|--------|-------|-------|
| drop_in | 102 | 90 need behavioral evidence (module_existence only) |
| gap | 2 | cli.get, process.reload.routing |
| platform_constraint | 4 | Transparent, Pf, Redir, cli.reuse |
| not_applicable | 3 | BaseProtocol-like internals, daemon, Rule |
| intentional_non_parity | 2 | SSH, SSR |

### By Evidence Level

| Evidence Level | Count | Notes |
|----------------|-------|-------|
| protocol_wire / failure_class / composition_validity | 57 | True behavioral evidence |
| cipher_kat / cipher_roundtrip | 15 | Cipher behavioral evidence |
| cli_flag_parse / cli_flag_rejection | 19 | CLI parsing evidence |
| process_lifecycle | 11 | Process lifecycle evidence |
| module_existence / constant_value | 90 | **Namespace evidence only — needs behavioral validation** |

### By Category

| Category | Count |
|----------|-------|
| python_namespace | 79 |
| protocol | 34 |
| cli_option | 22 |
| composition | 20 |
| cipher | 15 |
| process | 12 |
| failure | 12 |

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

The following 90 records are marked `drop_in` in the manifest but use only
`module_existence` or `constant_value` as their comparator. These have **namespace
evidence only** and require paired oracle/candidate behavioral validation before
true `drop_in` status can be claimed under the corrective pass.

### Python Namespace (57 records)

| ID | Comparator | Notes |
|----|-----------|-------|
| python.pproxy | module_existence | Top-level module |
| python.pproxy.proto | module_existence | Protocol module |
| python.pproxy.server | module_existence | Server module |
| python.pproxy.cipher | module_existence | Cipher module |
| python.pproxy.Connection | module_existence | Top-level class alias |
| python.pproxy.Server | module_existence | Top-level class alias |
| python.pproxy.DIRECT | constant_value | Direct constant |
| python.pproxy.Rule | module_existence | Rule function alias |
| python.pproxy.proto_reexport | module_existence | Proto re-export |
| python.pproxy.proto.BaseProtocol | module_existence | Internal base class |
| python.pproxy.proto.Socks5 | module_existence | SOCKS5 protocol class |
| python.pproxy.proto.HTTP | module_existence | HTTP protocol class |
| python.pproxy.proto.Socks4 | module_existence | SOCKS4 protocol class |
| python.pproxy.proto.SS | module_existence | Shadowsocks class |
| python.pproxy.proto.Trojan | module_existence | Trojan class |
| python.pproxy.proto.Direct | module_existence | Direct class |
| python.pproxy.proto.WS | module_existence | WebSocket class |
| python.pproxy.proto.H2 | module_existence | H2 class |
| python.pproxy.proto.H3 | module_existence | H3 class |
| python.pproxy.proto.Tunnel | module_existence | Tunnel class |
| python.pproxy.proto.Echo | module_existence | Echo class |
| python.pproxy.proto.HTTPOnly | module_existence | HTTPOnly class |
| python.pproxy.proto.Socks5.accept | module_existence | Accept method |
| python.pproxy.proto.Socks5.channel | module_existence | Channel method |
| python.pproxy.proto.Socks5.connect | module_existence | Connect method |
| python.pproxy.proto.Socks5.udp_accept | module_existence | UDP accept method |
| python.pproxy.proto.HTTP.accept | module_existence | HTTP accept method |
| python.pproxy.proto.HTTP.http_accept | module_existence | HTTP http_accept method |
| python.pproxy.proto.SS.accept | module_existence | SS accept method |
| python.pproxy.proto.SS.guess | module_existence | SS guess method |
| python.pproxy.proto.Trojan.accept | module_existence | Trojan accept method |
| python.pproxy.proto.Trojan.guess | module_existence | Trojan guess method |
| python.pproxy.proto.WS.patch_ws_stream | module_existence | WS stream patch |
| python.pproxy.proto.accept_func | module_existence | Accept function |
| python.pproxy.proto.get_protos | module_existence | Get protos function |
| python.pproxy.proto.udp_accept_func | module_existence | UDP accept function |
| python.pproxy.proto.socks_address | module_existence | SOCKS address helper |
| python.pproxy.proto.netloc_split | module_existence | Netloc split helper |
| python.pproxy.proto.sslwrap | module_existence | SSL wrap function |
| python.pproxy.proto.packstr | module_existence | Pack string function |
| python.pproxy.proto.MAPPINGS | constant_value | Protocol mappings dict |
| python.pproxy.proto.HTTP_LINE | constant_value | HTTP line constant |
| python.pproxy.proto.SO_ORIGINAL_DST | constant_value | Socket option |
| python.pproxy.proto.SOL_IPV6 | constant_value | Socket option |
| python.pproxy.cipher.BaseCipher | module_existence | Base cipher class |
| python.pproxy.cipher.AEADCipher | module_existence | AEAD cipher class |
| python.pproxy.cipher.AES_256_GCM_Cipher | module_existence | AES-256-GCM class |
| python.pproxy.cipher.AES_192_GCM_Cipher | module_existence | AES-192-GCM class |
| python.pproxy.cipher.AES_128_GCM_Cipher | module_existence | AES-128-GCM class |
| python.pproxy.cipher.ChaCha20_IETF_POLY1305_Cipher | module_existence | ChaCha20-Poly1305 class |
| python.pproxy.cipher.PacketCipher | module_existence | Packet cipher class |
| python.pproxy.cipher.AES_256_CFB_Cipher | module_existence | AES-256-CFB class |
| python.pproxy.cipher.ChaCha20_IETF_Cipher | module_existence | ChaCha20-IETF class |
| python.pproxy.cipher.ChaCha20_Cipher | module_existence | ChaCha20 class |
| python.pproxy.cipher.MAP | constant_value | Cipher map dict |
| python.pproxy.cipher.get_cipher | module_existence | Get cipher function |

### Server Module (15 records)

| ID | Comparator | Notes |
|----|-----------|-------|
| python.pproxy.server.AuthTable | module_existence | Auth table class |
| python.pproxy.server.ProxySimple | module_existence | Proxy simple class |
| python.pproxy.server.ProxyBackward | module_existence | Proxy backward class |
| python.pproxy.server.ProxyDirect | module_existence | Proxy direct class |
| python.pproxy.server.ProxyH2 | module_existence | Proxy H2 class |
| python.pproxy.server.ProxyH3 | module_existence | Proxy H3 class |
| python.pproxy.server.ProxySSH | module_existence | Proxy SSH class |
| python.pproxy.server.ProxyQUIC | module_existence | Proxy QUIC class |
| python.pproxy.server.main | module_existence | Main function |
| python.pproxy.server.compile_rule | module_existence | Compile rule function |
| python.pproxy.server.check_server_alive | module_existence | Check alive function |
| python.pproxy.server.prepare_ciphers | module_existence | Prepare ciphers function |
| python.pproxy.server.proxies_by_uri | module_existence | Proxies by URI function |
| python.pproxy.server.proxy_by_uri | module_existence | Proxy by URI function |
| python.pproxy.server.SOCKET_TIMEOUT | constant_value | Socket timeout constant |

### Server Constants (4 records)

| ID | Comparator | Notes |
|----|-----------|-------|
| python.pproxy.server.SOCKET_TIMEOUT | constant_value | Socket timeout |
| python.pproxy.server.UDP_LIMIT | constant_value | UDP limit constant |
| python.pproxy.server.DIRECT_const | constant_value | Direct constant |
| python.pproxy.server.DUMMY | constant_value | Dummy constant |

### CLI (1 record)

| ID | Comparator | Notes |
|----|-----------|-------|
| cli.get | cli_flag_rejection | Gap — not implemented |

## Terminal Records (102 drop_in with behavioral evidence)

### Protocol (34 records — all with protocol_wire, failure_class, or composition_validity comparators)

| ID | Status | Notes |
|----|--------|-------|
| protocol.http_connect.listener_tcp_ipv4 | drop_in | HTTP CONNECT IPv4 |
| protocol.http_connect.listener_tcp_ipv6 | drop_in | HTTP CONNECT IPv6 |
| protocol.http_connect.listener_tcp_domain | drop_in | HTTP CONNECT domain |
| protocol.http_connect.auth_success | drop_in | HTTP CONNECT auth success |
| protocol.http_connect.auth_failure | drop_in | HTTP CONNECT auth failure |
| protocol.http_connect.refused | drop_in | HTTP CONNECT refused |
| protocol.http_connect.half_close | drop_in | HTTP CONNECT half-close |
| protocol.http_connect.fragmented | drop_in | HTTP CONNECT fragmented |
| protocol.http_connect.timeout | drop_in | HTTP CONNECT timeout |
| protocol.http_forward.get | drop_in | HTTP forward GET |
| protocol.http_forward.post_content_length | drop_in | HTTP forward POST |
| protocol.http_forward.head | drop_in | HTTP forward HEAD |
| protocol.http_forward.chunked | drop_in | HTTP forward chunked |
| protocol.http_forward.persistent | drop_in | HTTP forward persistent |
| protocol.http_forward.connection_close | drop_in | HTTP forward connection close |
| protocol.http_forward.auth_success | drop_in | HTTP forward auth success |
| protocol.http_forward.malformed | drop_in | HTTP forward malformed |
| protocol.http_forward.upstream_close | drop_in | HTTP forward upstream close |
| protocol.socks4.connect_ipv4 | drop_in | SOCKS4 CONNECT IPv4 |
| protocol.socks4.user_id | drop_in | SOCKS4 user ID |
| protocol.socks4.refused | drop_in | SOCKS4 refused |
| protocol.socks4.malformed | drop_in | SOCKS4 malformed |
| protocol.socks4a.connect_domain | drop_in | SOCKS4a CONNECT domain |
| protocol.socks5.connect_ipv4 | drop_in | SOCKS5 CONNECT IPv4 |
| protocol.socks5.connect_ipv6 | drop_in | SOCKS5 CONNECT IPv6 |
| protocol.socks5.connect_domain | drop_in | SOCKS5 CONNECT domain |
| protocol.socks5.connect_refused | drop_in | SOCKS5 CONNECT refused |
| protocol.socks5.auth_success | drop_in | SOCKS5 auth success |
| protocol.socks5.auth_failure | drop_in | SOCKS5 auth failure |
| protocol.socks5.udp_associate | drop_in | SOCKS5 UDP associate |
| protocol.shadowsocks.tcp_upstream | drop_in | Shadowsocks TCP |
| protocol.shadowsocks.udp_relay | drop_in | Shadowsocks UDP |
| protocol.trojan.upstream | drop_in | Trojan upstream |
| protocol.trojan.auth_failure | drop_in | Trojan auth failure |

### Cipher (15 records — all with cipher_kat, cipher_roundtrip, or property comparators)

| ID | Status | Notes |
|----|--------|-------|
| cipher.aes_256_gcm.kat | drop_in | AES-256-GCM KAT |
| cipher.aes_192_gcm.kat | drop_in | AES-192-GCM KAT |
| cipher.aes_128_gcm.kat | drop_in | AES-128-GCM KAT |
| cipher.chacha20_ietf_poly1305.kat | drop_in | ChaCha20-Poly1305 KAT |
| cipher.aes_256_gcm.roundtrip | drop_in | AES-256-GCM roundtrip |
| cipher.aes_192_gcm.roundtrip | drop_in | AES-192-GCM roundtrip |
| cipher.aes_128_gcm.roundtrip | drop_in | AES-128-GCM roundtrip |
| cipher.chacha20_ietf_poly1305.roundtrip | drop_in | ChaCha20-Poly1305 roundtrip |
| cipher.aead.encrypt_and_digest | drop_in | AEAD encrypt_and_digest |
| cipher.aead.decrypt_and_verify | drop_in | AEAD decrypt_and_verify |
| cipher.aead.nonce_property | drop_in | AEAD nonce property |
| cipher.aead.setup_iv | drop_in | AEAD setup_iv |
| cipher.stream.cfb_roundtrip | drop_in | Stream CFB roundtrip |
| cipher.stream.ctr_roundtrip | drop_in | Stream CTR roundtrip |
| cipher.stream.ofb_roundtrip | drop_in | Stream OFB roundtrip |

### Composition (20 records — all with composition_validity or composition_rejection)

| ID | Status | Notes |
|----|--------|-------|
| composition.http_listener_tcp | drop_in | HTTP TCP listener |
| composition.socks5_listener_tcp | drop_in | SOCKS5 TCP listener |
| composition.socks4_listener_tcp | drop_in | SOCKS4 TCP listener |
| composition.shadowsocks_listener_tcp | drop_in | SS TCP listener |
| composition.trojan_listener_tcp | drop_in | Trojan TCP listener |
| composition.http_upstream_tcp | drop_in | HTTP TCP upstream |
| composition.socks5_upstream_tcp | drop_in | SOCKS5 TCP upstream |
| composition.shadowsocks_upstream_tcp | drop_in | SS TCP upstream |
| composition.trojan_upstream_tcp | drop_in | Trojan TCP upstream |
| composition.socks5_listener_udp | drop_in | SOCKS5 UDP listener |
| composition.direct_upstream | drop_in | Direct upstream |
| composition.ws_upstream_tcp | drop_in | WS TCP upstream |
| composition.h2_upstream_tcp | drop_in | H2 TCP upstream |
| composition.raw_upstream_tcp | drop_in | Raw TCP upstream |
| composition.ssh_upstream_rejected | drop_in | SSH rejected |
| composition.ssr_upstream_rejected | drop_in | SSR rejected |
| composition.chain_two_hop_valid | drop_in | Two-hop chain |
| composition.chain_three_hop_valid | drop_in | Three-hop chain |
| composition.chain_socks5_to_ws | drop_in | SOCKS5→WS chain |
| composition.chain_socks5_to_h2 | drop_in | SOCKS5→H2 chain |

### Process (11 records — all with process_lifecycle comparators)

| ID | Status | Notes |
|----|--------|-------|
| process.startup.successful | drop_in | Startup success |
| process.startup.bind_conflict | drop_in | Bind conflict |
| process.shutdown.graceful | drop_in | Graceful shutdown |
| process.shutdown.connection_drain | drop_in | Connection drain |
| process.signal.sigterm | drop_in | SIGTERM handling |
| process.signal.sigint | drop_in | SIGINT handling |
| process.signal.sighup | drop_in | SIGHUP handling |
| process.multiple_listeners | drop_in | Multiple listeners |
| process.upstream_health_check | drop_in | Health check |
| process.admin_endpoint | drop_in | Admin endpoint |
| process.reload.routing | gap | **Gap — not implemented** |

### Failure (12 records — all with failure_class comparators)

| ID | Status | Notes |
|----|--------|-------|
| failure.dns.resolve_error | drop_in | DNS resolve error |
| failure.dns.nxdomain | drop_in | DNS NXDOMAIN |
| failure.connection.refused | drop_in | Connection refused |
| failure.connection.timeout | drop_in | Connection timeout |
| failure.auth.invalid_credentials | drop_in | Invalid credentials |
| failure.auth.missing_credentials | drop_in | Missing credentials |
| failure.malformed.http_request | drop_in | Malformed HTTP |
| failure.malformed.socks4_version | drop_in | Malformed SOCKS4 version |
| failure.malformed.socks4_truncated | drop_in | Truncated SOCKS4 |
| failure.malformed.socks5_version | drop_in | Malformed SOCKS5 version |
| failure.upstream.connection_loss | drop_in | Upstream connection loss |
| failure.policy.deny | drop_in | Policy deny |

### CLI (19 records — all with cli_flag_parse or cli_flag_rejection)

| ID | Status | Notes |
|----|--------|-------|
| cli.listen | drop_in | Listen flag |
| cli.remote | drop_in | Remote flag |
| cli.udp_listen | drop_in | UDP listen flag |
| cli.udp_remote | drop_in | UDP remote flag |
| cli.scheduler | drop_in | Scheduler flag |
| cli.alive | drop_in | Alive flag |
| cli.ssl | drop_in | SSL flag |
| cli.block | drop_in | Block flag |
| cli.rulefile | drop_in | Rulefile flag |
| cli.verbose | drop_in | Verbose flag |
| cli.log | drop_in | Log flag |
| cli.pac | drop_in | PAC flag |
| cli.sys | drop_in | Sys flag |
| cli.test | drop_in | Test flag |
| cli.config | drop_in | Config flag |
| cli.ipv6 | drop_in | IPv6 flag |
| cli.bind | drop_in | Bind flag |
| cli.version | drop_in | Version flag |
| cli.help | drop_in | Help flag |

### Non-terminal

| ID | Status | Category | Notes |
|----|--------|----------|-------|
| python.pproxy.proto.Transparent | platform_constraint | python_namespace | Linux SO_ORIGINAL_DST |
| python.pproxy.proto.Pf | platform_constraint | python_namespace | macOS PF |
| python.pproxy.proto.Redir | platform_constraint | python_namespace | Linux SO_ORIGINAL_DST |
| cli.reuse | platform_constraint | cli_option | Linux connection reuse |
| python.pproxy.proto.SSH | intentional_non_parity | python_namespace | SSH not implemented |
| python.pproxy.proto.SSR | intentional_non_parity | python_namespace | SSR rejected |
| python.pproxy.Rule | not_applicable | python_namespace | compile_rule alias |
| cli.daemon | not_applicable | cli_option | Uses systemd/launchd |
| process.daemon.unsupported | not_applicable | process | Uses systemd/launchd |
