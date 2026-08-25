# eggress-protocol-socks -- SOCKS4/4a and SOCKS5

Full server + client implementations of SOCKS4/4a and SOCKS5, including the
SOCKS5 UDP datagram codec used by the UDP subsystem. Both protocol families
share a common `Socks5Error` taxonomy and are detected by single-byte
version sniffers.

## Module map

| File | Role | Key lines |
|---|---|---|
| `detector.rs` | `Socks4Detector`: version byte 0x04, confidence 100 | `Socks4Detector` (:10) |
| `lib.rs` | `Socks5Detector`: version byte 0x05, confidence 100; re-exports | `Socks5Detector` (:21) |
| `socks4/server.rs` | `read_socks4_request` / `write_socks4_reply`; 4a domain when IP=`0.0.0.x`(x!=0); USERID<=255; BIND rejected | `MAX_USER_ID_LEN` (:8), `read_socks4_request` (:54), `write_socks4_reply` (:145) |
| `socks4/client.rs` | `socks4_connect`: IP->SOCKS4, domain->SOCKS4a (IP=0.0.0.1); IPv6 rejected | `socks4_connect` (:18) |
| `socks5/server.rs` | Full handshake: method neg -> RFC 1929 auth -> CONNECT; `SocksAddr`; REP=0x07 for unsupported cmds; sync parse fns for fuzzing | `parse_method_negotiation` (:118), `parse_connect_request` (:139), `parse_socks5_request` (:199), `read_auth_request` (:309), `handle_socks5_handshake` (:488) |
| `socks5/client.rs` | `socks5_connect`: greeting + auth + CONNECT + reply | `socks5_connect` (:31) |
| `socks5/udp_codec.rs` | `decode_socks5_udp_datagram` / `encode_socks5_udp_datagram`: RSV=0x0000, FRAG=0x00, ATYP valid, domain<=255, payload<=65535 | `decode_socks5_udp_datagram` (:30), `MAX_UDP_DATAGRAM_SIZE` (:3) |
| `error.rs` | Shared `Socks5Error` with `display_hex` for diagnostics | `Socks5Error` (:3), `display_hex` (:61) |

## Public API surface

From `lib.rs` (:10-13): `read_socks4_request`, `write_socks4_reply`,
`socks4_connect`, `Socks4Error`, `Socks4Request`, `Socks4Status`,
`Socks4Detector`, `SOCKS4_PROTOCOL_ID`, `Socks5Detector`, `Socks5Error`.
`socks5::server`, `socks5::client`, and `socks5::udp_codec` are public.

## Wire format

### SOCKS4/4a request (`:server.rs:47-50`)

```
+----+----+----+----+----+----+----+----+----+----+....+----+
| VN | CD | DSTPORT |      DSTIP        | USERID       |0x00|
+----+----+----+----+----+----+----+----+----+----+....+----+
  1    1      2            4              variable       1
```

VN=0x04, CD must be 0x01 (BIND rejected), USERID max 255 bytes NUL-terminated.
SOCKS4a: DSTIP=`0.0.0.x`(x!=0) triggers NUL-terminated domain after USERID.

### SOCKS4/4a reply

VN=0x00, CD: 90 Granted, 91 Failed, 92 FailedNoIdent, 93 FailedDifferentUser.

### SOCKS5 method negotiation

```
Client: [VN=0x05][NMETHODS][METHODS...]     Server: [VN=0x05][METHOD]
```

Methods: 0x00 no-auth, 0x02 user/pass, 0xFF no-acceptable.
`parse_method_negotiation` (:118) is sync for fuzzing.

### SOCKS5 username/password auth (RFC 1929)

```
Client: [VER=0x01][ULEN][UNAME][PLEN][PASSWD]   Server: [VER=0x01][STATUS]
```

Credential lengths capped at 255 (`MAX_CRED_LEN`, :49). Password compared
via `subtle::ConstantTimeEq` (:333-337). On failure, status=0x01 sent
then connection closed.

### SOCKS5 CONNECT request

```
+----+-----+-------+------+----------+----------+
| VER| CMD |  RSV  | ATYP | DST.ADDR | DST.PORT |
+----+-----+-------+------+----------+----------+
  1    1     1       1     variable      2
```

VER=0x05, CMD=0x01, RSV must be 0x00. ATYP: 0x01=IPv4(4B), 0x03=domain(1B
len+bytes), 0x04=IPv6(16B). Sync parsers `parse_connect_request` (:139) and
`parse_socks5_request` (:199) for fuzzing.

### SOCKS5 CONNECT reply

REP=0x00 success, REP=0x07 command not supported. `handle_socks5_handshake`
(:508-518) catches `UnsupportedCommand` and sends REP=0x07 before closing.

### SOCKS5 UDP datagram (`:udp_codec.rs`)

RSV(2B) must be 0x0000, FRAG(1B) must be 0x00, then ATYP/addr/port/payload.
Max 65535 bytes (`MAX_UDP_DATAGRAM_SIZE`, :3). Domain length validated;
zero rejected.

## How it works

### SOCKS4 accept flow

1. `read_socks4_request` (:54) reads 8-byte header, validates version/command.
2. USERID read byte-by-byte bounded at 255 (:77-78).
3. If DSTIP=`0.0.0.x`(x!=0), reads domain (:96-125).
4. Returns `Socks4Request` with `addr` and optional `domain`.

### SOCKS4 client flow

1. `socks4_connect` (:18) validates USERID length.
2. IPv4: standard SOCKS4. Domain: SOCKS4a IP=0.0.0.1, domain appended after
   USERID. IPv6: `UnsupportedAddressType`.
3. Reads 8-byte reply, maps status.

### SOCKS5 accept flow

1. `read_method_negotiation` (:257) reads version + methods.
2. `send_method_selection` (:279): password set + client offers 0x02 -> user/pass;
   no password + 0x00 -> no-auth; else 0xFF.
3. If auth: `read_auth_request` (:309) with constant-time compare (:333).
4. `read_connect_request` (:359) reads version/cmd/rsv/atyp/addr.
5. Unsupported cmd -> `handle_socks5_handshake` sends REP=0x07 (:508-518).
6. `send_connect_reply` (:467) writes success.

### UDP codec flow

`decode_socks5_udp_datagram` (:30) validates RSV/FRAG/ATYP/bounds, returns
zero-copy `Socks5UdpRequest`. `encode_socks5_udp_datagram` (:104) writes
RSV=0x0000, FRAG=0x00, encoded target, and payload.

## Error and failure model

**Socks4Error** (`socks4/error.rs`): `InvalidVersion`, `UnsupportedCommand`,
`UserIdTooLong`, `ConnectionFailed`(CD=91), `FailedNoIdent`(CD=92),
`FailedDifferentUser`(CD=93), `UnknownStatus`, `DomainTooLong`,
`UnsupportedAddressType`(IPv6), `MalformedRequest`.

**Socks5Error** (`error.rs`): `UnsupportedVersion`, `UnsupportedCommand`,
`UnsupportedAddressType`, `UnsupportedAuthMethod`, `AuthFailed`,
`CredentialsTooLong`, `MethodNegotiationFailed`, `InvalidReservedByte`,
`DomainTooLong`, `MalformedMessage`. `display_hex` (:61) formats
version/cmd/atyp in hex. `From<Socks5Error> for io::Error` (:50).

## Security notes

| Resource | Limit | Enforced at |
|---|---|---|
| SOCKS4 USERID | 255 B | `server.rs:77`, `client.rs:24` |
| SOCKS4a domain | 255 B | `server.rs:101` |
| SOCKS5 username/password | 255 B | `server.rs:319`/`326`, `client.rs:85` |
| SOCKS5 domain (all paths) | 255 B | `server.rs:389`, `:96`, `client.rs:131` |
| UDP datagram | 65535 B | `udp_codec.rs:37` |

**Constant-time compare**: SOCKS5 password via `subtle::ConstantTimeEq`
(:server.rs:333-337). SOCKS4 user_id is clear-text, not a credential.

**Validation**: RSV must be 0x00 in CONNECT and UDP. FRAG must be 0x00.
Empty SOCKS4a domain rejected (:server.rs:117-121). `send_method_selection`
never downgrades password-required to no-auth (:server.rs:286-288).

## Test coverage

- `detector.rs`: match/no-match/empty.
- `lib.rs`: SOCKS4 roundtrip, SOCKS4a, user_id, version/BIND rejection,
  user-id-too-long, fragmented read.
- `socks4/client.rs`: IPv4, SOCKS4a, rejected, no-ident, different-user,
  malformed, unknown status, slow timeout, user-id length, no-reply EOF.
- `socks5/server.rs`: method negotiation (no-auth, user/pass, no-acceptable,
  password-rejects-auth-none), auth success/failure, connect IPv4/domain/IPv6,
  reply encoding, unsupported version/cmd/atyp, boundary creds, full
  handshake, fragmented handshake, SocksAddr encode, RSV rejection (all),
  domain 255-byte boundary.
- `socks5/client.rs`: auth, CONNECT request, reply parsing.
- `socks5/udp_codec.rs`: encode/decode roundtrip, RSV/FRAG rejection,
  short/oversized/zero-domain, payload preservation.

**Fuzz targets**: `socks5_handshake` exercises `parse_method_negotiation`,
`parse_connect_request`, `parse_socks5_request`.
`socks5_udp_datagram` exercises `decode_socks5_udp_datagram`.

## Reviewer gotchas

1. **Two detector files**: `Socks4Detector` in `detector.rs` (:10);
   `Socks5Detector` in `lib.rs` (:21). Split because `Socks5Detector`
   needs re-exports.
2. **`read_connect_request` rejects non-CONNECT**: The async version
   (:server.rs:359) returns `UnsupportedCommand` for BIND/UDP_ASSOCIATE.
   But `read_socks5_request` (:server.rs:412) accepts all three -- used
   for UDP ASSOCIATE flow. Caller must choose correctly.
3. **REP=0x07 sent by `handle_socks5_handshake`**, not by
   `read_connect_request`. The latter returns the error; the former catches
   `UnsupportedCommand` and sends the RFC reply (:server.rs:508-518).
4. **SOCKS4a sentinel is `0.0.0.x`(x!=0)**: Domain extension triggers
   when last octet is non-zero (:server.rs:97). IP=0.0.0.0 is normal
   (invalid) IPv4.
5. **Domain encode checks byte length, not char length**: `encode_reply`
   (:server.rs:96) compares `domain.len()` (UTF-8 bytes) against 255.
   Multi-byte chars can cause a visually-short domain to exceed the limit.
6. **Sync vs async parse**: `parse_method_negotiation` and
   `parse_connect_request` are sync (`&[u8]` -> `Result`) for fuzzing.
   Async versions do same parsing over `AsyncRead`. Not duplicated logic.

## See also

[Overview](overview.md), [UDP](udp.md), [Routing](routing.md),
[Server](server.md).
