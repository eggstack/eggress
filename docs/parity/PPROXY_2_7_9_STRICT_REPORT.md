# pproxy 2.7.9 Strict Compatibility Report

**Oracle version:** pproxy==2.7.9
**Manifest schema:** strict_1
**Policy:** docs/parity/PPROXY_COMPATIBILITY_POLICY.md
**Oracle ref:** compat/pproxy-2.7.9/provenance.toml

## Summary

| Metric | Count |
|--------|-------|
| Total records | 194 |
| Terminal (resolved) | 111 |
| Gap (unresolved) | 83 |
| Certification readiness | 57% |

### By Status

| Status | Count |
|--------|-------|
| drop_in | 102 |
| gap | 83 |
| platform_constraint | 4 |
| not_applicable | 3 |
| intentional_non_parity | 2 |

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

Records with non-terminal status requiring resolution:

| ID | Status | Category | Owner | Milestone |
|----|--------|----------|-------|----------|
| python.pproxy.Connection | gap | python_namespace | track-a | B |
| python.pproxy.proto.BaseProtocol | gap | python_namespace | track-a | B |
| python.pproxy.proto.Socks5 | gap | python_namespace | track-a | B |
| python.pproxy.proto.HTTP | gap | python_namespace | track-a | B |
| python.pproxy.proto.Socks4 | gap | python_namespace | track-a | B |
| python.pproxy.proto.SS | gap | python_namespace | track-a | B |
| python.pproxy.proto.Trojan | gap | python_namespace | track-a | B |
| python.pproxy.proto.Direct | gap | python_namespace | track-a | B |
| python.pproxy.proto.WS | gap | python_namespace | track-a | B |
| python.pproxy.proto.H2 | gap | python_namespace | track-a | B |
| python.pproxy.proto.H3 | gap | python_namespace | track-a | B |
| python.pproxy.proto.Tunnel | gap | python_namespace | track-a | B |
| python.pproxy.proto.Echo | gap | python_namespace | track-a | B |
| python.pproxy.proto.HTTPOnly | gap | python_namespace | track-a | B |
| python.pproxy.proto.Socks5.accept | gap | python_namespace | track-a | B |
| python.pproxy.proto.Socks5.channel | gap | python_namespace | track-a | B |
| python.pproxy.proto.Socks5.connect | gap | python_namespace | track-a | B |
| python.pproxy.proto.Socks5.udp_accept | gap | python_namespace | track-a | B |
| python.pproxy.proto.HTTP.accept | gap | python_namespace | track-a | B |
| python.pproxy.proto.HTTP.http_accept | gap | python_namespace | track-a | B |
| python.pproxy.proto.SS.accept | gap | python_namespace | track-a | B |
| python.pproxy.proto.SS.guess | gap | python_namespace | track-a | B |
| python.pproxy.proto.Trojan.accept | gap | python_namespace | track-a | B |
| python.pproxy.proto.Trojan.guess | gap | python_namespace | track-a | B |
| python.pproxy.proto.WS.patch_ws_stream | gap | python_namespace | track-a | B |
| python.pproxy.proto.accept_func | gap | python_namespace | track-a | B |
| python.pproxy.proto.get_protos | gap | python_namespace | track-a | B |
| python.pproxy.proto.udp_accept_func | gap | python_namespace | track-a | B |
| python.pproxy.proto.socks_address | gap | python_namespace | track-a | B |
| python.pproxy.proto.netloc_split | gap | python_namespace | track-a | B |
| python.pproxy.proto.sslwrap | gap | python_namespace | track-a | B |
| python.pproxy.proto.packstr | gap | python_namespace | track-a | B |
| python.pproxy.proto.MAPPINGS | gap | python_namespace | track-a | B |
| python.pproxy.proto.HTTP_LINE | gap | python_namespace | track-a | B |
| python.pproxy.proto.SO_ORIGINAL_DST | gap | python_namespace | track-a | B |
| python.pproxy.proto.SOL_IPV6 | gap | python_namespace | track-a | B |
| python.pproxy.cipher.BaseCipher | gap | python_namespace | track-a | B |
| python.pproxy.cipher.AEADCipher | gap | python_namespace | track-a | B |
| python.pproxy.cipher.AES_256_GCM_Cipher | gap | python_namespace | track-a | B |
| python.pproxy.cipher.AES_192_GCM_Cipher | gap | python_namespace | track-a | B |
| python.pproxy.cipher.AES_128_GCM_Cipher | gap | python_namespace | track-a | B |
| python.pproxy.cipher.ChaCha20_IETF_POLY1305_Cipher | gap | python_namespace | track-a | B |
| python.pproxy.cipher.PacketCipher | gap | python_namespace | track-a | B |
| python.pproxy.cipher.AES_256_CFB_Cipher | gap | python_namespace | track-a | B |
| python.pproxy.cipher.ChaCha20_IETF_Cipher | gap | python_namespace | track-a | B |
| python.pproxy.cipher.ChaCha20_Cipher | gap | python_namespace | track-a | B |
| python.pproxy.cipher.MAP | gap | python_namespace | track-a | B |
| python.pproxy.cipher.get_cipher | gap | python_namespace | track-a | B |
| python.pproxy.server.AuthTable | gap | python_namespace | track-a | B |
| python.pproxy.server.ProxySimple | gap | python_namespace | track-a | B |
| python.pproxy.server.ProxyBackward | gap | python_namespace | track-a | B |
| python.pproxy.server.ProxyDirect | gap | python_namespace | track-a | B |
| python.pproxy.server.ProxyH2 | gap | python_namespace | track-a | B |
| python.pproxy.server.ProxyH3 | gap | python_namespace | track-a | B |
| python.pproxy.server.ProxySSH | gap | python_namespace | track-a | B |
| python.pproxy.server.ProxyQUIC | gap | python_namespace | track-a | B |
| python.pproxy.server.main | gap | python_namespace | track-a | B |
| python.pproxy.server.compile_rule | gap | python_namespace | track-a | B |
| python.pproxy.server.check_server_alive | gap | python_namespace | track-a | B |
| python.pproxy.server.prepare_ciphers | gap | python_namespace | track-a | B |
| python.pproxy.server.proxies_by_uri | gap | python_namespace | track-a | B |
| python.pproxy.server.proxy_by_uri | gap | python_namespace | track-a | B |
| python.pproxy.server.SOCKET_TIMEOUT | gap | python_namespace | track-a | B |
| python.pproxy.server.UDP_LIMIT | gap | python_namespace | track-a | B |
| python.pproxy.server.DIRECT_const | gap | python_namespace | track-a | B |
| python.pproxy.server.DUMMY | gap | python_namespace | track-a | B |
| cli.get | gap | cli_option | track-a | B |
| cipher.aes_256_gcm.kat | gap | cipher | track-b | B |
| cipher.aes_192_gcm.kat | gap | cipher | track-b | B |
| cipher.aes_128_gcm.kat | gap | cipher | track-b | B |
| cipher.chacha20_ietf_poly1305.kat | gap | cipher | track-b | B |
| cipher.aes_256_gcm.roundtrip | gap | cipher | track-b | B |
| cipher.aes_192_gcm.roundtrip | gap | cipher | track-b | B |
| cipher.aes_128_gcm.roundtrip | gap | cipher | track-b | B |
| cipher.chacha20_ietf_poly1305.roundtrip | gap | cipher | track-b | B |
| cipher.aead.encrypt_and_digest | gap | cipher | track-b | B |
| cipher.aead.decrypt_and_verify | gap | cipher | track-b | B |
| cipher.aead.nonce_property | gap | cipher | track-b | B |
| cipher.aead.setup_iv | gap | cipher | track-b | B |
| cipher.stream.cfb_roundtrip | gap | cipher | track-b | B |
| cipher.stream.ctr_roundtrip | gap | cipher | track-b | B |
| cipher.stream.ofb_roundtrip | gap | cipher | track-b | B |
| process.reload.routing | gap | process | track-c | B |

## Terminal Records

| ID | Status | Category | Notes |
|----|--------|----------|-------|
| python.pproxy | drop_in | python_namespace | Top-level pproxy package module. |
| python.pproxy.proto | drop_in | python_namespace | Protocol definitions module. |
| python.pproxy.server | drop_in | python_namespace | Server implementation module. |
| python.pproxy.cipher | drop_in | python_namespace | Cipher implementations module. |
| python.pproxy.Server | drop_in | python_namespace | pproxy.Server wraps the full server lifecycle. |
| python.pproxy.DIRECT | drop_in | python_namespace | Direct connection constant. |
| python.pproxy.Rule | not_applicable | python_namespace | pproxy.Rule is compile_rule function. Not applicable; eggress uses TOML rule ... |
| python.pproxy.proto_reexport | drop_in | python_namespace | Protocol module re-export. |
| python.pproxy.proto.Transparent | platform_constraint | python_namespace | Transparent proxy base class. Platform-gated to Linux (SO_ORIGINAL_DST). |
| python.pproxy.proto.SSH | intentional_non_parity | python_namespace | SSH protocol handler. Intentionally not implemented. eggress recommends OpenS... |
| python.pproxy.proto.SSR | intentional_non_parity | python_namespace | SSR/legacy Shadowsocks protocol handler. Intentionally rejected. SSR stream c... |
| python.pproxy.proto.Pf | platform_constraint | python_namespace | macOS PF transparent proxy. Platform-gated to macOS only; requires PF firewall. |
| python.pproxy.proto.Redir | platform_constraint | python_namespace | Linux transparent proxy via SO_ORIGINAL_DST. Platform-gated to Linux only. |
| cli.listen | drop_in | cli_option | Bind one or more TCP listener URIs. |
| cli.remote | drop_in | cli_option | Specify upstream proxy URIs with chaining via __ separator. |
| cli.udp_listen | drop_in | cli_option | Bind a standalone UDP relay socket. |
| cli.udp_remote | drop_in | cli_option | Specify upstream for UDP traffic relayed via -ul. |
| cli.scheduler | drop_in | cli_option | Set load-balancing algorithm (rr, fa, rc, lc). |
| cli.alive | drop_in | cli_option | Set alive check interval in seconds. |
| cli.ssl | drop_in | cli_option | Enable TLS on listener (certfile,keyfile). |
| cli.block | drop_in | cli_option | Block connections matching regex patterns. |
| cli.rulefile | drop_in | cli_option | Load routing rules from a line-based file. |
| cli.daemon | not_applicable | cli_option | Fork into background. Not applicable; eggress uses systemd/launchd/supervisor... |
| cli.verbose | drop_in | cli_option | Enable verbose/debug logging. Parsed and diagnosed. |
| cli.log | drop_in | cli_option | Write log output to a file. Parsed and diagnosed. |
| cli.reuse | platform_constraint | cli_option | Connection reuse/pooling. Platform-gated to Linux; recognized but behavior di... |
| cli.pac | drop_in | cli_option | Serve a PAC file for browser auto-configuration. |
| cli.sys | drop_in | cli_option | Auto-configure system proxy settings. Parsed and diagnosed. |
| cli.test | drop_in | cli_option | Test all remote proxies and exit. Parsed and diagnosed. |
| cli.config | drop_in | cli_option | Load configuration from TOML file. |
| cli.ipv6 | drop_in | cli_option | Enable IPv6 mode. |
| cli.bind | drop_in | cli_option | Bind address for outgoing connections. |
| cli.version | drop_in | cli_option | Print version information and exit. |
| cli.help | drop_in | cli_option | Print usage information and exit. |
| protocol.http_connect.listener_tcp_ipv4 | drop_in | protocol | Accept HTTP CONNECT and relay to IPv4 target. |
| protocol.http_connect.listener_tcp_ipv6 | drop_in | protocol | Accept HTTP CONNECT and relay to IPv6 target. |
| protocol.http_connect.listener_tcp_domain | drop_in | protocol | Accept HTTP CONNECT and relay to domain target. |
| protocol.http_connect.auth_success | drop_in | protocol | Accept HTTP CONNECT with valid proxy credentials. |
| protocol.http_connect.auth_failure | drop_in | protocol | Reject HTTP CONNECT with invalid proxy credentials. |
| protocol.http_connect.refused | drop_in | protocol | HTTP CONNECT to refused target produces equivalent error. |
| protocol.http_connect.half_close | drop_in | protocol | Half-close handling during CONNECT tunnel. |
| protocol.http_connect.fragmented | drop_in | protocol | Fragmented payload relay in CONNECT tunnel. |
| protocol.http_connect.timeout | drop_in | protocol | Upstream connect timeout produces equivalent failure class. |
| protocol.http_forward.get | drop_in | protocol | Forward HTTP GET requests to target. |
| protocol.http_forward.post_content_length | drop_in | protocol | Forward HTTP POST with Content-Length. |
| protocol.http_forward.head | drop_in | protocol | Forward HTTP HEAD requests. |
| protocol.http_forward.chunked | drop_in | protocol | Forward HTTP requests with chunked transfer encoding. |
| protocol.http_forward.persistent | drop_in | protocol | Persistent (keep-alive) forward proxy connections. |
| protocol.http_forward.connection_close | drop_in | protocol | Connection: close header handling. |
| protocol.http_forward.auth_success | drop_in | protocol | Forward proxy with valid credentials succeeds. |
| protocol.http_forward.malformed | drop_in | protocol | Malformed HTTP request produces equivalent error. |
| protocol.http_forward.upstream_close | drop_in | protocol | Upstream connection close during forward proxy. |
| protocol.socks4.connect_ipv4 | drop_in | protocol | SOCKS4 CONNECT to IPv4 target. |
| protocol.socks4.user_id | drop_in | protocol | User ID field forwarded in CONNECT request. |
| protocol.socks4.refused | drop_in | protocol | SOCKS4 CONNECT to refused target. |
| protocol.socks4.malformed | drop_in | protocol | Malformed SOCKS4 request handling. |
| protocol.socks4a.connect_domain | drop_in | protocol | SOCKS4a CONNECT with domain resolution. |
| protocol.socks5.connect_ipv4 | drop_in | protocol | SOCKS5 CONNECT to IPv4 target. |
| protocol.socks5.connect_ipv6 | drop_in | protocol | SOCKS5 CONNECT to IPv6 target. |
| protocol.socks5.connect_domain | drop_in | protocol | SOCKS5 CONNECT with domain target. |
| protocol.socks5.connect_refused | drop_in | protocol | SOCKS5 CONNECT to refused target. |
| protocol.socks5.auth_success | drop_in | protocol | SOCKS5 with valid credentials succeeds. |
| protocol.socks5.auth_failure | drop_in | protocol | SOCKS5 with invalid credentials rejected. |
| protocol.socks5.udp_associate | drop_in | protocol | SOCKS5 UDP associate relay. |
| protocol.shadowsocks.tcp_upstream | drop_in | protocol | Standard SIP003 AEAD TCP framing; wire-compatible. |
| protocol.shadowsocks.udp_relay | drop_in | protocol | Standard AEAD UDP relay; interoperable with standard implementations. |
| protocol.trojan.upstream | drop_in | protocol | Trojan upstream with TLS and SHA224 password auth. |
| protocol.trojan.auth_failure | drop_in | protocol | Trojan auth failure produces equivalent error. |
| composition.http_listener_tcp | drop_in | composition | HTTP as TCP listener is valid. |
| composition.socks5_listener_tcp | drop_in | composition | SOCKS5 as TCP listener is valid. |
| composition.socks4_listener_tcp | drop_in | composition | SOCKS4 as TCP listener is valid. |
| composition.shadowsocks_listener_tcp | drop_in | composition | Shadowsocks as TCP listener is valid. |
| composition.trojan_listener_tcp | drop_in | composition | Trojan as TCP listener is valid. |
| composition.http_upstream_tcp | drop_in | composition | HTTP as TCP upstream is valid. |
| composition.socks5_upstream_tcp | drop_in | composition | SOCKS5 as TCP upstream is valid. |
| composition.shadowsocks_upstream_tcp | drop_in | composition | Shadowsocks as TCP upstream is valid. |
| composition.trojan_upstream_tcp | drop_in | composition | Trojan as TCP upstream is valid. |
| composition.socks5_listener_udp | drop_in | composition | SOCKS5 UDP associate listener is valid. |
| composition.direct_upstream | drop_in | composition | Direct upstream (no proxy) is valid. |
| composition.ws_upstream_tcp | drop_in | composition | WebSocket as TCP upstream is valid. |
| composition.h2_upstream_tcp | drop_in | composition | H2 CONNECT as TCP upstream is valid. |
| composition.raw_upstream_tcp | drop_in | composition | Raw/tunnel as TCP upstream is valid. |
| composition.ssh_upstream_rejected | drop_in | composition | SSH as upstream is rejected with structured diagnostic. |
| composition.ssr_upstream_rejected | drop_in | composition | SSR as upstream is rejected with structured diagnostic. |
| composition.chain_two_hop_valid | drop_in | composition | socks5__http two-hop chain is valid. |
| composition.chain_three_hop_valid | drop_in | composition | socks5__http__socks5 three-hop chain is valid. |
| composition.chain_socks5_to_ws | drop_in | composition | socks5__ws chain is valid. |
| composition.chain_socks5_to_h2 | drop_in | composition | socks5__h2 chain is valid. |
| process.startup.successful | drop_in | process | Process starts and binds listener successfully. |
| process.startup.bind_conflict | drop_in | process | Startup fails with address-in-use error. |
| process.shutdown.graceful | drop_in | process | Process shuts down gracefully on SIGTERM/SIGINT. |
| process.shutdown.connection_drain | drop_in | process | Active connections are drained during shutdown grace period. |
| process.signal.sigterm | drop_in | process | SIGTERM triggers graceful shutdown. |
| process.signal.sigint | drop_in | process | SIGINT (Ctrl-C) triggers graceful shutdown. |
| process.signal.sighup | drop_in | process | SIGHUP triggers config reload. |
| process.daemon.unsupported | not_applicable | process | --daemon flag is recognized but rejected. Not applicable; eggress uses system... |
| process.multiple_listeners | drop_in | process | Multiple listeners bound on different ports/protocols. |
| process.upstream_health_check | drop_in | process | Health checks run against configured upstreams. |
| process.admin_endpoint | drop_in | process | Admin HTTP server serves PAC and snapshot. |
| failure.dns.resolve_error | drop_in | failure | DNS resolution failure produces equivalent error class. |
| failure.dns.nxdomain | drop_in | failure | DNS NXDOMAIN produces equivalent error. |
| failure.connection.refused | drop_in | failure | Connection refused produces equivalent error class. |
| failure.connection.timeout | drop_in | failure | Connection timeout produces equivalent error class. |
| failure.auth.invalid_credentials | drop_in | failure | Invalid auth credentials produce equivalent error class. |
| failure.auth.missing_credentials | drop_in | failure | Missing auth credentials produce equivalent error class. |
| failure.malformed.http_request | drop_in | failure | Malformed HTTP request produces equivalent error. |
| failure.malformed.socks4_version | drop_in | failure | SOCKS4 with wrong version byte produces equivalent error. |
| failure.malformed.socks4_truncated | drop_in | failure | Truncated SOCKS4 request produces equivalent error. |
| failure.malformed.socks5_version | drop_in | failure | SOCKS5 with wrong version byte produces equivalent error. |
| failure.upstream.connection_loss | drop_in | failure | Upstream connection loss during relay produces equivalent error. |
| failure.policy.deny | drop_in | failure | Policy deny (blocked by rule) produces equivalent error. |
