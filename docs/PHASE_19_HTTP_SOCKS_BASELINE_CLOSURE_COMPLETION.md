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
- 11 differential test cases total (6 original + 5 gap-closure)
- Covers auth success/failure/malformed, IPv4/IPv6/domain targets, refused targets, timeout, client half-close, server half-close, fragmented client payload, fragmented upstream payload

### 19.3: HTTP Forward-Proxy Differential Tests
- 10 differential test cases total (6 original + 4 gap-closure)
- Covers GET, POST, HEAD, Connection: close, persistent connections, chunked body, upstream Connection: close, malformed request, unsupported transfer coding, forward proxy auth

### 19.4: SOCKS4/4a Differential Closure
- 8 differential test cases total (2 original + 6 gap-closure)
- Covers SOCKS4 CONNECT echo, SOCKS4a domain, user ID propagation, domain failure, refused target, malformed version, truncated request

### 19.5: SOCKS5 Differential Closure
- 8 differential test cases total (3 original + 5 gap-closure)
- Covers IPv6, domain, refused targets, malformed address type, unsupported UDP command, early client close (greeting), early client close (request), server half-close

### 19.6: SOCKS BIND Decision Point
- BIND deferred: returns `REP_COMMAND_NOT_SUPPORTED` (0x07)
- Documented in parity matrix and manifest

### 19.7: Mixed-Protocol Listener Robustness
- 9 new unit tests for protocol detection edge cases
- Covers fragmented bytes, garbage, slow clients, mixed-protocol listeners, auth-required mixed with no-auth

### 19.8: Smoke Tests
- curl smoke tests in `interoperability_curl.rs` (HTTP CONNECT, SOCKS5, SOCKS4a)
- Python urllib smoke test script at `scripts/smoke_clients.py`

### 19.9: Documentation Updates
- Parity matrix updated with Phase 19 evidence
- Manifest updated with 17 new feature entries
- PPROXY_PARITY_SPEC updated: HTTP forward proxy marked compatible, resolved probes
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
