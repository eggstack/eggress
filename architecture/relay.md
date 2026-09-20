# eggress-relay — Generic Bidirectional Stream Relay

Leaf crate with no eggress dependencies. Owns the protocol-neutral byte-relay
engine: copy between two Tokio-compatible duplex streams with an explicit
post-half-close policy, directional byte counts, and directional I/O errors.
`eggress-core::relay` is a thin compatibility facade over this engine that
preserves historical Eggress behavior; `eggress-server` still calls the
facade, never this crate directly.

## Module map

| File | Role |
|---|---|
| `crates/eggress-relay/src/lib.rs` | Entire crate: `RelayOptions` (`new`/`try_new`/`bounded`), `InvalidRelayOptions`, `HalfClosePolicy`, `RelayReport`/`RelayTermination`/`RelaySide`, `RelayFailure`/`RelayDirection`, `relay()`/`relay_with_options()`, inline unit tests |
| `crates/eggress-relay/README.md` | Direct-consumer usage and the `eggress-embed` distinction |
| `crates/eggress-core/src/relay.rs` | Legacy facade: unchanged `RelayResult`/`TerminationReason`/`relay(BoxStream, BoxStream)`, explicit 64 KiB + one-second options, rich-to-legacy mapping (not part of this crate) |

## Public API surface

```rust
pub const DEFAULT_BUFFER_SIZE: usize = 64 * 1024; // 64 KiB

pub enum HalfClosePolicy { Drain, DrainFor(Duration) }

pub struct RelayOptions {
    pub buffer_size: NonZeroUsize,   // non-zero by construction
    pub half_close: HalfClosePolicy,
}
// Default: 64 KiB + Drain. `RelayOptions::new(size, policy)`,
// `try_new(usize, ..) -> Result<_, InvalidRelayOptions>` (rejects zero),
// `bounded(size, drain_timeout)` constructors; `InvalidRelayOptions`
// (Display+Error) on zero size.

pub enum RelaySide { Client, Server }

pub enum RelayTermination {
    ClientClosed,
    ServerClosed,
    DrainTimedOut { first_closed: RelaySide },
}

pub struct RelayReport {
    pub bytes_upstream: u64,    // client -> server
    pub bytes_downstream: u64,  // server -> client
    pub termination: RelayTermination,
}

pub enum RelayDirection { Upstream, Downstream }

pub struct RelayFailure {
    pub direction: RelayDirection,
    pub source: std::io::Error,
    pub bytes_upstream: u64,
    pub bytes_downstream: u64,
}
// Display + std::error::Error (source() -> &io::Error).

pub async fn relay<C, S>(client: C, server: S)
    -> Result<RelayReport, RelayFailure>
where C: AsyncRead + AsyncWrite + Unpin,
      S: AsyncRead + AsyncWrite + Unpin;

pub async fn relay_with_options<C, S>(client: C, server: S, options: RelayOptions)
    -> Result<RelayReport, RelayFailure>
where C: AsyncRead + AsyncWrite + Unpin,
      S: AsyncRead + AsyncWrite + Unpin;
```

Deliberately absent bounds: no `BoxStream`, no `Send`, no `'static`. The
engine runs inline in the caller's task (no `tokio::spawn`), so generic
streams — `TcpStream`, `UnixStream`, TLS streams, joined halves, in-memory
`duplex` streams, application wrappers — work without boxing. Inside Eggress,
protocol/transport boundaries still use `BoxStream`; the core facade passes
its boxes straight into this generic surface.

## How it works (control flow)

1. Own both complete streams in one future and allocate one heap `Vec<u8>` per
   direction sized by `buffer_size` (tuning control, not a framing boundary —
   any non-zero size is semantically transparent). No generic
   `tokio::io::split` or split-lock synchronization is used.
2. Poll two directional copy states fairly from that single future. Each state
   retains its read/write cursor, byte count, EOF, and shutdown progress, so
   the complete streams remain available to both directions without nested
   futures or detached tasks.
3. Each direction: read into its buffer, `write_all`, bump its counter only
   after successful writes; on read EOF, `shutdown()` the opposite writer,
   tolerating `BrokenPipe`/`ConnectionReset` (half-close compatibility), then
   return `Ok`.
4. First direction to fail wins: drop the survivor and return `RelayFailure`
   with the failing direction, the underlying `io::Error`, and both counters.
5. First direction to close cleanly records its side (upstream EOF =
   `ClientClosed`, downstream EOF = `ServerClosed`); the survivor drains per
   policy: `Drain` awaits it normally, `DrainFor(d)` awaits it under
   `tokio::time::timeout(d, ..)`.
6. A drain failure returns `RelayFailure` for the surviving direction; a
   drain expiry drops the survivor and returns
   `RelayTermination::DrainTimedOut { first_closed }` with current counts.
7. Dropping the outer future drops both direction futures and all owned
   halves — no detached task can outlive the relay.

## Half-close semantics

- Generic default (`HalfClosePolicy::Drain`) is unbounded and safe for
  arbitrary request/response protocols: a client may half-close its request
  while the upstream takes longer than one second to respond, and the
  response is still delivered completely.
- `HalfClosePolicy::DrainFor(d)` bounds that wait for peers that never answer
  a FIN; expiry is reported as `DrainTimedOut`, never silently as an ordinary
  close.
- The Eggress facade explicitly requests `DrainFor(1s)` + 64 KiB, preserving
  the historical server cutoff. Changing Eggress product behavior is a
  separate decision, never bundled into this primitive.

## Error & failure model

- Clean completion → `Ok(RelayReport)` with exact directional counts and
  `ClientClosed`/`ServerClosed` (no `BothClosed` — the rich API does not claim
  a distinction it cannot define deterministically).
- Bounded-drain expiry → `Ok(RelayReport)` with `DrainTimedOut` (visible, not
  collapsed).
- Any directional I/O error → `Err(RelayFailure)` with direction, source
  `io::Error` (so `ErrorKind` survives), and transferred counts.
- Legacy mapping (in `eggress-core`, not here): rich close reasons pass
  through; both `DrainTimedOut` sides collapse to their first-closed side;
  every `RelayFailure` debug-logs source/direction and collapses to legacy
  `TerminationReason::Error` with the captured counts.

## Dependency boundary

Only `tokio` (workspace features). No `eggress-*` crate, no `thiserror`
(the failure type implements `Display`/`Error` directly), no metrics, no
buffer pool (application policy), no middleware/callback/inspection hooks —
consumers needing inspection wrap their streams above this layer.

## Test coverage map

All inline in `src/lib.rs` (`cargo test -p eggress-relay`):

| Test | Covers |
|---|---|
| `bidirectional_transfer_reports_exact_counts_client_first` | Round-trip content + exact counts, client-first EOF |
| `server_first_close_reports_server_closed` | Server-first EOF path |
| `unbounded_drain_delivers_slow_response_after_client_half_close` | 300 ms-delayed response succeeds under default `Drain` (the regression the one-second cutoff would break) |
| `bounded_drain_contrast_times_out_on_slow_response` | Same 300 ms delay under `DrainFor(50ms)` → `DrainTimedOut{Client}` |
| `bounded_drain_timeout_reports_first_closed_side_and_counts` | Silent peer → bounded drain expires with side + counts, never hangs |
| `upstream_failure_reports_direction_kind_and_counts` | Scripted upstream read error → direction, `ErrorKind`, pre-failure bytes |
| `downstream_failure_reports_direction_kind` | Scripted downstream read error → direction, `ErrorKind`, counts |
| `tiny_buffer_preserves_content_and_counts` | 7-byte buffer over 32 KiB payload: exact content + counts |
| `dropping_relay_releases_both_streams` | Drop outer future while pending → both halves released, no spawned task retains them |
| `zero_buffer_size_is_rejected` / `default_is_unbounded_sixty_four_kib` | Non-zero-by-construction options + documented default |

Legacy compatibility cases (echo, client/server half-close, hanging peer,
I/O-error→`Error`, cancellation) live in `eggress-core/src/relay.rs`
(`cargo test -p eggress-core relay`) and prove the facade preserves server
behavior.

## Reviewer gotchas

- One `RelayFuture` owns both complete streams and their directional states;
  there are no detached direction tasks or generic split locks.
- The relay state machine uses a bounded per-poll step budget. This avoids
  split-lock contention while preventing an always-ready direction from
  starving its peer.
- The first clean close selects the draining side. An optional `Sleep` deadline
  is stored on the relay future, and timeout returns the current directional
  byte counters without spawning or detaching work.
- `BrokenPipe`/`ConnectionReset` from post-EOF `shutdown()` are tolerated
  exactly like the historical implementation; any other shutdown error fails
  the direction.
- Do not re-export this rich API from `eggress-core`: the point is direct
  consumption without expanding core's permanent surface.

## See also

- [core.md](core.md) — legacy `relay()` facade, `BoxStream`, chain executor
- [server.md](server.md) — connection pipeline still on the core facade
- [overview.md](overview.md) — system map and component index
- [testing-and-tooling.md](testing-and-tooling.md) — `tcp_relay` benchmark
  (end-to-end through this engine + `copy_bidirectional` baseline)
