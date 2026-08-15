# Shadowsocks TCP AEAD Framing Audit

The earlier version of this audit described Eggress's old clear-length,
single-AEAD framing and recommended leaving it experimental. That description
is superseded by strict phase 1.

The current implementation uses pproxy/SIP003-compatible framing:

- method-sized directional salts (16/24/32/32 bytes);
- encrypted two-byte length blocks followed by separately encrypted payload
  blocks;
- independent little-endian nonce counters for each direction;
- a 16,383-byte plaintext packet limit matching pproxy;
- partial-read-safe TCP state machines and bounded length checks.

The implementation and regression tests are in
`crates/eggress-protocol-shadowsocks/src/{method,aead,tcp,tcp_stream}.rs`.
The authoritative current wire description is
[SHADOWSOCKS_PARITY.md](SHADOWSOCKS_PARITY.md). The phase-1 plan records why
the correction was required:
`plans/PPROXY_STRICT_PHASE_1_SHADOWSOCKS_AEAD_CORRECTION.md`.

External evidence is intentionally gated because it requires local proxy
executables. Run both the pproxy oracle and the maintained Shadowsocks suite
before changing the parity matrix.
