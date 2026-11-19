# ADR-0003: Optional no-progress timeout for the generic relay

Status: accepted

Date: 2026-11-18

Decision owners: project maintainers

Related specification sections:

- plans/000-long-term-specification.md §4.11 (published-surface stability)
- plans/001-terminology-and-domain-model.md §3 (Relay)

Affected subsystem roadmaps:

- plans/subsystems/proxy-core-relay-roadmap.md

## Context

Issue #10 requests an opt-in no-progress/idle timeout at the existing eggress-relay seam. It must be disabled when absent, preserve byte delivery, cancellation, and half-close behavior, and state whether activity is measured per direction or per connection.

The current relay owns both streams in one future and tracks directional byte counts. RelayOptions is a public struct with public fields; RelayTermination is a public exhaustive enum. Extending either can break source compatibility for direct consumers. The crates.io reverse-dependency page currently lists eggress-core, eggress-embed, eggchaos-server, and eggtunnel. Eggchaos is the identified feature consumer; Eggtunnel exact-pins an older relay release.

## Decision drivers

- Keep the existing relay entry points and result contract source-compatible.
- Keep the new behavior explicitly opt-in and disabled on existing calls.
- Measure useful connection activity without restarting or duplicating the copy loop.
- Preserve exact directional byte counts and distinguish idle expiry from half-close drain expiry.
- Keep listener and product configuration policy outside the leaf relay crate.

## Considered options

### Option A — Add a field to RelayOptions and a variant to RelayTermination

This is the smallest implementation diff and is easy to wire into known Eggstack consumers. However, adding a public struct field breaks downstream struct literals, and adding an enum variant breaks exhaustive matches. The published reverse-dependency list is evidence of current consumers, not proof that no other source consumers exist.

### Option B — Add a separate opt-in API and outcome type (selected)

Keep RelayOptions, RelayReport, RelayTermination, relay(), and relay_with_options() unchanged. Add an opt-in entry point that accepts the existing RelayOptions plus an optional idle duration, and return a new outcome type that distinguishes a normal RelayReport from IdleTimedOut with both byte counts. Share the same internal RelayFuture/state machine.

This adds a small new public surface but avoids source-breaking existing contracts and lets old calls remain disabled without migration.

### Option C — Wrap the existing relay future in a caller-side timeout

This closes the streams when the wrapper is dropped, but discards the relay's final byte counts and does not provide a relay-specific terminal outcome. It also leaves each consumer to reimplement timeout/report handling. Reject.

## Decision

Accept Option B: add a separate, opt-in API and outcome type. Keep RelayOptions, RelayReport, RelayTermination, relay(), and relay_with_options() unchanged. The new API contract is:

```rust
pub async fn relay_with_idle_timeout<C, S>(
    client: C,
    server: S,
    options: RelayOptions,
    idle_timeout: Option<Duration>,
) -> Result<RelayIdleTimeoutOutcome, RelayFailure>
```

RelayIdleTimeoutOutcome has Completed(RelayReport) and IdleTimedOut { bytes_upstream, bytes_downstream } cases. The exact public type definitions must follow the existing crate conventions while preserving this shape and behavior.

The timeout is connection-wide: a successful write of one or more bytes in either direction resets the same idle deadline. Read readiness, buffered-but-unwritten input, EOF, shutdown, and timer wakeups do not count as forwarded-byte progress. The timer starts on the first poll of the opted-in relay future, not while an unpolled future is merely being constructed, and remains active after a half-close while the opposite direction drains. The existing DrainFor timer remains independent and continues to mean post-half-close drain grace.

None disables the idle timer. Some(Duration::ZERO) expires immediately on the future's first poll, before either stream is polled and before any bytes are forwarded. For a positive duration, at any poll where the monotonic current time is at or beyond the idle deadline, IdleTimedOut takes precedence over stream I/O or progress observed on that poll; a write at or after the deadline cannot reset or rescue the expired timer. If the idle and DrainFor deadlines are both due on the same poll, the existing DrainTimedOut result takes precedence, preserving the established drain classification. If the opted-in API receives None, it completes with the existing RelayReport behavior. The normal relay APIs continue to use no idle timeout and preserve current behavior.

Idle timeout is represented by the new API's outcome type rather than by adding a RelayTermination variant. I/O errors remain RelayFailure when observed before a deadline is due. The implementation owns both streams and returns from the same future; it must not spawn a detached timer task.

## Consequences

### Positive

- Existing downstream calls, struct literals, and exhaustive matches remain source-compatible.
- Timeout outcome and transferred-byte totals remain observable.
- Eggchaos can opt in through the supported relay seam without duplicating the bidirectional copy loop.
- Eggtunnel and the Eggress compatibility facade need not change merely to remain compatible.

### Negative

- The new API and outcome type are slightly more surface than a field and enum variant.
- Consumers that want the feature still need a released Eggress version and their own configuration/adoption work.

### Neutral or deferred

- The Eggress server/core compatibility facade remains on its current behavior; no global runtime default changes.
- No pproxy compatibility claim changes.
- Downstream config and dependency changes are separate repo work tracked by follow-up issues after publication.

## Compatibility and migration

This is additive. Existing public types and functions remain unchanged. The new API is opt-in; no existing caller starts timing out. Eggchaos should expose its own optional runtime setting and choose the new API only when configured. Eggtunnel's existing relay_drain remains a half-close grace, not an idle timeout. Any later change to this accepted public contract requires a superseding ADR and renewed compatibility review.

## Security and reliability implications

An enabled idle deadline can close a legitimate but quiet application stream; callers choose the duration, and the default remains disabled. Reset only after successful writes so pending writes cannot keep a stalled relay alive. On expiry, preserve byte counters, drop both streams through normal future completion, and keep cancellation behavior equivalent to the current engine. Do not log payloads or secret-bearing metadata.

## Verification

- Deterministic relay tests prove disabled behavior, connection-wide reset by either direction, expiry with exact byte counts, expiry during post-half-close drain, and immediate zero-duration expiry before stream polling.
- Exact-deadline tests prove idle expiry wins over same-poll I/O progress at or after its deadline, while DrainTimedOut wins a tie when both idle and drain deadlines are due.
- Existing transfer, half-close, drain-timeout, I/O failure, and future-drop tests continue to pass unchanged.
- Compile coverage proves existing RelayOptions literals and exhaustive RelayTermination matching remain valid.
- Eggchaos downstream issue records config propagation and outcome handling; Eggtunnel downstream issue records exact-pin adoption and whether it opts in.

## Supersession

None.
