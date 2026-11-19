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
| `crates/eggress-relay/src/lib.rs` | Entire crate: `RelayOptions` (`new`/`try_new`/`bounded`), `InvalidRelayOptions`, `HalfClosePolicy`, `RelayReport`/`RelayTermination`/`RelaySide`, `RelayIdleTimeoutOutcome`, `RelayFailure`/`RelayDirection`, `relay()`/`relay_with_options()`/`relay_with_idle_timeout()`, inline unit tests |
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

pub enum RelayIdleTimeoutOutcome {
    Completed(RelayReport),
    IdleTimedOut { bytes_upstream: u64, bytes_downstream: u64 },
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

pub async fn relay_with_idle_timeout<C, S>(
    client: C, server: S, options: RelayOptions, idle_timeout: Option<Duration>
) -> Result<RelayIdleTimeoutOutcome, RelayFailure>
where C: AsyncRead + AsyncWrite + Unpin,
      S: AsyncRead + AsyncWrite + Unpin;
```

Deliberately absent bounds: no `BoxStream`, no `Send`, no `'static`. The
engine runs inline in the caller's task (no `tokio::spawn`), so generic
streams — `TcpStream`, `UnixStream`, TLS streams, joined halves, in-memory
`duplex` streams, application wrappers — work without boxing. Inside Eggress,
protocol/transport boundaries still use `BoxStream`; the core facade passes
its boxes straight into this generic surface. Existing APIs remain unchanged.
The opt-in API's `None` disables idle timing; its timeout outcome is separate
from `RelayTermination::DrainTimedOut` and carries both directional counters.

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
7. When enabled, a connection-wide idle deadline resets only after a
   successful non-empty write in either direction. It remains active during
   post-half-close draining; idle expiry returns `IdleTimedOut` with current
   counters, while `DrainFor` retains its distinct `DrainTimedOut` report.
8. Dropping the outer future drops both directional states, both streams, and
   all owned halves — no detached task can outlive the relay.

## Half-close semantics

- Generic default (`HalfClosePolicy::Drain`) is unbounded and safe for
  arbitrary request/response protocols: a client may half-close its request
  while the upstream takes longer than one second to respond, and the
  response is still delivered completely.
- `HalfClosePolicy::DrainFor(d)` bounds that wait for peers that never answer
  a FIN; expiry is reported as `DrainTimedOut`, never silently as an ordinary
  close. It is independent from the opt-in idle deadline, which remains active
  both before and after half-close.
- The Eggress facade explicitly requests `DrainFor(1s)` + 64 KiB, preserving
  the historical server cutoff. Changing Eggress product behavior is a
  separate decision, never bundled into this primitive.

## Error & failure model

- Clean completion → `Ok(RelayReport)` with exact directional counts and
  `ClientClosed`/`ServerClosed` (no `BothClosed` — the rich API does not claim
  a distinction it cannot define deterministically).
- Bounded-drain expiry → `Ok(RelayReport)` with `DrainTimedOut` (visible, not
  collapsed).
- Opt-in idle expiry → `Ok(RelayIdleTimeoutOutcome::IdleTimedOut)` with both
  transferred-byte counts; old entry points cannot produce this outcome.
- Any directional I/O error → `Err(RelayFailure)` with direction, source
  `io::Error` (so `ErrorKind` survives), and transferred counts.
- Legacy mapping (in `eggress-core`, not here): rich close reasons pass
  through; both `DrainTimedOut` sides collapse to their first-closed side;
  every `RelayFailure` debug-logs source/direction and collapses to legacy
  `TerminationReason::Error` with the captured counts.

## Dependency boundary

Runtime dependencies remain only `tokio` (workspace features). Tokio's
`test-util` feature is dev-only for deterministic paused-time tests. No
`eggress-*` crate, no `thiserror` (the failure type implements
`Display`/`Error` directly), no metrics, no buffer pool (application policy),
no middleware/callback/inspection hooks — consumers needing inspection wrap
their streams above this layer.

## Test coverage map

All inline in `src/lib.rs` (`cargo test -p eggress-relay`):

| Test | Covers |
|---|---|
| `bidirectional_transfer_reports_exact_counts_client_first` | Round-trip content + exact counts, client-first EOF |
| `server_first_close_reports_server_closed` | Server-first EOF path |
| `unbounded_drain_delivers_slow_response_after_client_half_close` | 300 ms-delayed response succeeds under default `Drain` (the regression the one-second cutoff would break) |
| `bounded_drain_contrast_times_out_on_slow_response` | Same 300 ms delay under `DrainFor(50ms)` → `DrainTimedOut{Client}` |
| `bounded_drain_timeout_reports_first_closed_side_and_counts` | Silent peer → bounded drain expires with side + counts, never hangs |
| `idle_timeout_none_keeps_silent_relay_open` / `zero_idle_timeout_expires_before_polling_streams` / `idle_timeout_expires_quiet_relay_with_counts_and_releases_streams` | Disabled default, immediate expiry, counter preservation, and endpoint release |
| `upstream_forwarded_bytes_reset_the_shared_idle_deadline` / `downstream_forwarded_bytes_reset_the_shared_idle_deadline` | Either direction resets the shared deadline |
| `buffered_but_unwritten_input_does_not_reset_idle_deadline` | Only successful writes count as activity |
| `idle_timeout_remains_active_during_half_close_drain` / `response_within_idle_interval_completes_after_half_close` | Idle expiry and slow half-close response behavior |
| `idle_deadline_wins_over_progress_ready_at_exact_deadline` / `drain_deadline_wins_exact_tie_with_idle_deadline` | Deterministic same-deadline precedence |
| `upstream_failure_reports_direction_kind_and_counts` / `opted_in_relay_preserves_io_failure_and_byte_counts` | Scripted I/O errors preserve direction, `ErrorKind`, and pre-failure bytes |
| `downstream_failure_reports_direction_kind` | Scripted downstream read error → direction, `ErrorKind`, counts |
| `tiny_buffer_preserves_content_and_counts` | 7-byte buffer over 32 KiB payload: exact content + counts |
| `dropping_relay_releases_both_streams` | Drop outer future while pending → both halves released, no spawned task retains them |
| `zero_buffer_size_is_rejected` / `default_is_unbounded_sixty_four_kib` | Non-zero-by-construction options + documented default |

Legacy compatibility cases (echo, client/server half-close, hanging peer,
I/O-error→`Error`, cancellation) live in `eggress-core/src/relay.rs`
(`cargo test -p eggress-core relay`) and prove the facade preserves server
behavior.

## Reviewer gotchas

- One `RelayFuture` owns both complete streams, directional states, and both
  optional deadlines; there are no detached direction tasks/timers or generic
  split locks.
- The relay state machine uses a bounded per-poll step budget. This avoids
  split-lock contention while preventing an always-ready direction from
  starving its peer.
- The first clean close selects the draining side. Independent idle and
  `DrainFor` `Sleep` deadlines stay on the relay future. Drain expiry wins an
  exact tie; idle expiry is otherwise a distinct outcome with current
  directional byte counters, without spawning or detaching work.
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
