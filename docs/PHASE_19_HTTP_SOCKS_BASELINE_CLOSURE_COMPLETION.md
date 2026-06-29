# Phase 19 Completion: HTTP/SOCKS Baseline Closure

## Summary

Phase 19 closes the high-value conventional proxy surface with persistent HTTP forwarding, expanded differential evidence for HTTP CONNECT, SOCKS4/4a, and SOCKS5, and mixed-protocol listener robustness.

## Changes

### 19.1: Persistent HTTP Forward-Proxy Session Model
- Implemented per-connection request loop in `execute_http_forward`
- Supports HTTP/1.1 keep-alive semantics
- `Connection: close` properly handled on both client and upstream sides
- `ForwardRequest.connection_close` field added
- `ForwardResponse` now tracks version and connection state
- `forward_response` returns `ForwardResult` with upstream alive status
- `forward_request_stream` added for persistent session loops

### 19.2: HTTP CONNECT Differential Closure
- 6 new differential test cases added
- Covers auth success/failure, IPv4/IPv6/domain targets, refused targets

### 19.3: HTTP Forward-Proxy Differential Tests
- 6 new differential test cases added
- Covers GET, POST, HEAD, Connection: close, persistent connections, chunked body

### 19.4: SOCKS4/4a Differential Closure
- 2 new differential test cases added
- Covers SOCKS4 CONNECT and SOCKS4a domain resolution

### 19.5: SOCKS5 Differential Closure
- 3 new differential test cases added
- Covers IPv6, domain, and refused targets

### 19.6: SOCKS BIND Decision Point
- BIND deferred: returns `REP_COMMAND_NOT_SUPPORTED` (0x07)
- Documented in parity matrix and manifest

### 19.7: Mixed-Protocol Listener Robustness
- 8 new unit tests for protocol detection edge cases
- Covers fragmented bytes, garbage, slow clients, mixed-protocol listeners

### 19.8: Smoke Tests
- curl and Python urllib smoke test scripts available

### 19.9: Documentation Updates
- Parity matrix updated with Phase 19 evidence
- Manifest updated with 17 new feature entries
- README checkboxes updated
- Migration guide updated

## Validation

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p eggress-protocol-http
cargo test -p eggress-protocol-socks
cargo test -p eggress-server
```

## Acceptance Criteria Met

- [x] Ordinary HTTP forward proxying supports persistent sessions
- [x] HTTP CONNECT has expanded differential evidence
- [x] Ordinary HTTP forward proxying has differential evidence
- [x] SOCKS4/SOCKS4a have differential evidence
- [x] SOCKS5 has expanded differential evidence
- [x] SOCKS BIND has explicit deferral decision
- [x] Mixed-protocol listener behavior is robust
- [x] Parity docs and manifest are synchronized
