# Phase 12: Scheduler, Chain, and Failure Semantics Parity — Completion Record

## Summary

Phase 12 aligns Eggress behavior with Python `pproxy` for chain selection,
scheduler behavior, retry/fallback behavior, multi-hop TCP chaining, and
client-visible failure semantics.

## Scheduler Decisions

| Scheduler | Parity status | Notes |
|-----------|--------------|-------|
| Round-robin | Compatible | Global atomic cursor; pproxy resets on reload (intentional non-parity) |
| First-available | Compatible | Both return first eligible upstream |
| Random | Supported | Eggress-specific; not in pproxy |
| Least-connections | Supported | Eggress-specific; not in pproxy |

### Fallback behavior

| Mode | pproxy | Eggress | Notes |
|------|--------|---------|-------|
| Direct fallback | `-F` flag | `fallback = "direct"` | Compatible |
| Reject on fail | default | `fallback = "reject"` | Compatible |
| Use-unhealthy | N/A | `fallback = "use-unhealthy"` | Eggress-specific |

## Multi-Hop TCP Chain Matrix

| Chain | Tested | Notes |
|-------|--------|-------|
| SOCKS5 → HTTP | Yes | End-to-end echo verified |
| HTTP → SOCKS5 | Yes | End-to-end echo verified |
| SOCKS4 → SOCKS5 | Yes | End-to-end echo verified |
| SOCKS5 → SOCKS4 | Yes | End-to-end echo verified |
| SOCKS5 → Trojan | No | TLS cert trust issue; needs insecure upstream flag |
| SOCKS5 → Shadowsocks | Yes | Tested in shadowsocks_tcp.rs |
| 3-hop (SOCKS5 → HTTP → SOCKS5) | Yes | End-to-end echo verified |

## Failure Mapping

| Condition | SOCKS5 | HTTP | SOCKS4 |
|-----------|--------|------|--------|
| Timeout | 0x06 | 504 | 91 |
| Refused | 0x05 | 502 | 91 |
| Policy denied | 0x02 | 403 | 91 |
| Network unreachable | 0x03 | 502 | 91 |
| Host unreachable/DNS | 0x04 | 502 | 91 |
| Auth failed | 0x01 | 502 | 91 |
| General failure | 0x01 | 502 | 91 |

## Tests Added

| File | Tests | Purpose |
|------|-------|---------|
| `crates/eggress-routing/tests/scheduler_parity.rs` | 10 | Scheduler behavior at routing crate level |
| `crates/eggress-runtime/tests/scheduler_runtime.rs` | 6 | Scheduler behavior through full runtime |
| `crates/eggress-runtime/tests/multihop_tcp.rs` | 7 | Multi-hop TCP chain end-to-end |
| `crates/eggress-runtime/tests/retry_fallback.rs` | 10 | Retry and fallback behavior |
| `crates/eggress-runtime/tests/observability.rs` | +6 | Extended observability tests |
| `crates/eggress-cli/tests/differential_pproxy.rs` | +6 | Extended differential tests |

## Documentation Added

| Document | Purpose |
|----------|---------|
| `docs/FAILURE_SEMANTICS.md` | Client-visible failure reply mapping |
| `docs/PHASE_12_SCHEDULER_CHAIN_FAILURE_PARITY_COMPLETION.md` | This completion record |

## Documentation Updated

| Document | Changes |
|----------|---------|
| `docs/PPROXY_PARITY_SPEC.md` | Scheduler audit table, Section 18 (Scheduler Semantics) |
| `docs/PARITY_MATRIX.md` | Expanded scheduler behavior table |
| `docs/METRICS.md` | Upstream failure reason labels |
| `docs/OPERATIONS.md` | Scheduler behavior section |
| `README.md` | Phase 12 status, capability items, doc links |
| `AGENTS.md` | Test commands, architecture facts |

## Differential Coverage

| Scenario | Status | Notes |
|----------|--------|-------|
| Round-robin distribution | Probe test | pproxy behavior documented |
| First healthy upstream | Probe test | Behavior confirmed |
| Multi-hop TCP echo | Probe test | 2-hop chain tested |
| Refused target failure | Differential test | Both fail coarsely |
| Auth failure class | Differential test | Both reject |
| Unsupported route | Probe test | Behavior documented |

## Intentional Non-Parity

1. **Scheduler state persistence**: Eggress preserves round-robin cursor across reloads; pproxy resets
2. **Retry within group**: Eggress makes single attempt; pproxy retry behavior undocumented
3. **Least-connections**: Eggress-specific; pproxy does not support
4. **Use-unhealthy fallback**: Eggress-specific; pproxy does not support
5. **SOCKS5 timeout code**: Eggress sends 0x06 (TTL expired); pproxy resets connection

## Blockers for Phase 13

- Trojan multi-hop chain test needs insecure upstream TLS flag
- Persistent HTTP forwarding not yet implemented
- No Python library/embedding API
