# ADR: Transparent Proxy `unsafe` Boundary

Status: Accepted (Phase 25-28 hardening).

## Context

The Linux transparent proxy listener in
`crates/eggress-server/src/listener/transparent.rs` retrieves the original
destination of a redirected TCP connection via the `SO_ORIGINAL_DST` socket
option. This requires:

1. Calling the `libc::getsockopt` FFI function on a raw file descriptor.
2. Interpreting the returned `sockaddr_storage` buffer as a `sockaddr_in` or
   `sockaddr_in6` based on the address family.

Both operations require `unsafe`. The workspace enforces
`unsafe_code = "forbid"` in every crate's `[lints]` table. This ADR records
the single, narrow exception for the transparent proxy module and the
invariants the unsafe code must maintain.

## Decision

The transparent proxy module contains four `unsafe` operations, all gated by
`#[cfg(target_os = "linux")]`:

| Location | Operation | Invariant |
|----------|-----------|-----------|
| `query_original_dst` | `std::mem::zeroed::<libc::sockaddr_storage>()` | `sockaddr_storage` is a POD type; zero-initialization is valid for any POD. The kernel only writes a `sockaddr_in` or `sockaddr_in6` (both smaller) into the buffer via `getsockopt`. |
| `query_original_dst` | `libc::getsockopt(fd, ...)` | `fd` is borrowed from a live `TcpStream` and is valid for the duration of the call. The pointer is properly aligned and points to `size_of::<sockaddr_storage>()` writable bytes. `getsockopt` writes only through the provided pointer and updates `len`. |
| `parse_sockaddr` (IPv4) | reinterpret as `sockaddr_in` | We verified `ss_family == AF_INET` and `len >= size_of::<sockaddr_in>()` before the cast. |
| `parse_sockaddr` (IPv6) | reinterpret as `sockaddr_in6` | We verified `ss_family == AF_INET6` and `len >= size_of::<sockaddr_in6>()` before the cast. |

We use `std::ptr::read_unaligned` for the reinterprets rather than pointer
casts; this avoids alignment assumptions and copies the struct out before
parsing individual fields.

Every `unsafe` block carries a `// SAFETY:` comment describing the invariant.
Direct unit tests cover:

- IPv4 and IPv6 round-trip parsing
- Truncated lengths rejecting
- Unknown address family rejecting

## IPv6 caveat

Linux's IPv6 `SO_ORIGINAL_DST` support is only present when nf_conntrack
IPv6 is loaded. When the path is unreachable we correctly return
`TransparentError::NoOriginalDestination`. The public manifest and parity
matrix continue to classify transparent proxy as `Supported` (synthetic) and
not `Compatible` because pproxy differential evidence at the IPv6 payload
level has not been collected.

## Validation

- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test -p eggress-server` covers `parse_sockaddr` directly with
  synthetic `sockaddr_in` and `sockaddr_in6` byte patterns.

## Alternative considered

Wrap the FFI in a dedicated `eggress-ffi` crate with a `#![allow(unsafe_code)]`
module. Not adopted because the call surface is small (one `getsockopt` call)
and the existing module-level `#[cfg(target_os = "linux")]` gating makes the
unsafe footprint easy to audit in place.

## Consequences

- The workspace `unsafe_code = "forbid"` lint remains in force everywhere
  except this single Linux-only module.
- Any future contributor adding unsafe code in this module must include a
  `// SAFETY:` comment and add a unit test for the new path.
- Reviewers should treat additions to `transparent.rs` as security-sensitive.