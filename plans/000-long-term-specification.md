# Eggress Long-Term Architecture and Product Specification

Status: canonical long-term implementation directive

Companion documents:

- `plans/001-terminology-and-domain-model.md`
- `plans/002-long-term-roadmap.md`
- `plans/003-planning-process.md`

This document defines the intended end state for Eggress. It establishes product scope, architectural ownership, protocol expectations, compatibility properties, and acceptance criteria. The roadmap decomposes this specification into ordered execution phases. The terminology document is normative whenever older Eggress docs or code use overlapping terms such as route, upstream, group, chain, hop, session, or tier.

The keywords MUST, MUST NOT, REQUIRED, SHOULD, SHOULD NOT, and MAY are normative.

## 1. Product definition

Eggress is a Rust-native, embeddable multi-protocol proxy framework and CLI targeting practical behavioral compatibility with Python `pproxy==2.7.9` (oracle pin). Strict drop-in parity is never assumed from names or shapes alone.

The same architecture MUST support the following consumption forms without creating separate products:

```text
Standalone CLI
    egress / pproxy binaries -- TOML config or pproxy-translated args --> listeners --> upstream chains

Embedded Rust service
    egress-embed facade (full-service) / egress-outbound OutboundConnector (listener-free chains)

Embedded Python
    PyO3 native module + pproxy-shaped compat helpers
```

## 2. Primary product goals

Eggress MUST provide:

1. A core TCP proxy foundation (URI grammar, stream relay, SOCKS4/4a, SOCKS5, HTTP CONNECT, ordinary HTTP forwarding, chain executor, CLI integration).
2. Routing, health, and operations (rule engine, upstream groups/schedulers, health state machine, TOML config, metrics/logging, admin API, reload/graceful shutdown).
3. UDP relay (SOCKS5 UDP ASSOCIATE through upstream relay with flow management).
4. Upstream protocol coverage (HTTP/SOCKS/Shadowsocks/Trojan/WS/Raw/reverse/H3 roles plus TLS/SSH/QUIC transports as specified).
5. A bounded pproxy compatibility surface: `eggress-pproxy-compat` translation (CLI flags, URIs, rulefiles), tiered diagnostics, and a manifest-governed claim set.
6. Embeddable APIs (Rust `eggress-embed` + listener-free `OutboundConnector` chains; Python bindings with exception identity and packaging).
7. Release engineering (version lockstep, wheel matrix, crates.io publishing discipline, standalone installers, self-update).

## 3. Non-goals

Eggress is not:

- a strict byte-for-byte drop-in for every pproxy code path regardless of documented tier;
- a general VPN, enterprise identity provider, or hosting platform;
- a second HTTP/pooling/retry abstraction per migration (migrations consolidate onto one owner);
- an OpenSSL/C-dependent stack (no OpenSSL, C deps, or build scripts without explicit architectural reason).

## 4. Architectural invariants

The following MUST remain true across releases and implementation strategies:

1. **Boxed byte streams at boundaries.** Protocols, TLS, SSH, QUIC, and chain hops consume an `AsyncRead + AsyncWrite` stream and return an upgraded one. Generic stream types MUST NOT leak through the architecture; streams are boxed at protocol/transport boundaries.
2. **Fail-closed composition.** Protocol/transport composition is validated before execution. Unsupported transports/roles fail closed with structured diagnostics, never silent fallback.
3. **Credential redaction.** Credentials and secret-bearing URIs are redacted before logging, diagnostics, or evidence output.
4. **Snapshot reload, fixed topology.** Only routing/upstream/group/health state swaps atomically (arc-swap of the compiled snapshot). Listener topology is not hot-reloaded.
5. **Enforced shutdown order.** Readiness false → listener stop → UDP drain → connection drain/cancel → admin shutdown last.
6. **Socket-truthful metadata.** When listener-free TCP callers need socket metadata, it is captured from the established `TcpStream` before boxing and carried beside `BoxStream`. Chain metadata describes the actual first-hop TCP socket; hostnames are never re-resolved just to report metadata.
7. **Policy-scoped transport reuse.** Reusable SSH/H2 physical transports are policy-scoped: nested hops are unpooled; ordinary hop-zero reuse stays within its stable SSH cache or shared TLS-policy scope. Explicit `local_bind` disables hop-zero SSH/H2 reuse; explicit insecure H2 is unpooled. H2 registries are keyed by TLS client-config object identity, not a process-global registry.
8. **ALPN adaptation preserves trust.** Adaptation of a caller-supplied `Arc<rustls::ClientConfig>` clones via `ClientConfig::clone()` and only mutates `alpn_protocols`. Trust roots, CA stores, mTLS identity, custom verifiers, and all other fields survive. A `tls_override` combined with per-hop `insecure=true` is rejected explicitly.
9. **Compat claims follow the manifest.** Claims use the tier vocabulary (`matched` / `supported_difference` / `platform_limited` / `intentional_non_parity`); changing a claim means updating the manifest and running the oracle/differential/interop suite. Generated reports follow the manifest, never lead it.
10. **Bounded parsing, deny unsafe.** Protocol parsing is bounded; `unsafe_code = "deny"`; edition 2021; MSRV 1.89 (release contract); Tokio + `thiserror` + `tracing`.
11. **Published-surface stability.** `docs/RUST_API.md` qualifies the public surface; it is documentation/qualification only and does not authorize hiding, moving, renaming, or signature-changing already-published items.

## 5. Compatibility contract

Practical compatibility with `pproxy==2.7.9` is governed by `docs/parity/pproxy_capability_manifest.toml` and `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`. The manifest is the authoritative claim set; the matrix is the user-facing summary; generated reports are derived artifacts. The final strict contract pins the `2.7.9` tag at commit `09d4752f17ed6787e1a073c93980eec019887ee3` in `qwj/python-proxy`.

Known standing boundaries: legacy Shadowsocks stream ciphers are intentional non-parity (opt-in `legacy-crypto`); QUIC/HTTP/3 is feature-gated (`quic`); SSH upstream compat is feature-gated (`ssh`), SSH listeners unsupported; bounded SSR TCP framing/plugin path is feature-gated; macOS PF original-destination recovery and four unavailable legacy cipher names are intentional exclusions.

## 6. End-state acceptance

The end state is reached when every subsystem roadmap in `plans/subsystems/` is closed, every closure record's acceptance evidence is recorded, the compat manifest has no unresolved `gap` entries outside explicitly tiered boundaries, and `docs/ROADMAP.md` plus `plans/registry.md` agree that no active milestone remains.
