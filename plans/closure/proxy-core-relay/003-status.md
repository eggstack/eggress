# Proxy Core and Relay Milestone 003 — Closure Status

Status: conditionally closed

Source implementation plan:

- [`plans/implementation/proxy-core-relay/003-optional-relay-idle-timeout.md`](../../implementation/proxy-core-relay/003-optional-relay-idle-timeout.md)

Source subsystem roadmap:

- [`plans/subsystems/proxy-core-relay-roadmap.md`](../../subsystems/proxy-core-relay-roadmap.md)

Repository baseline reviewed: `51b37a0933861796af262b533be230419edc0b31` (also current HEAD when reviewed)

Implementation commits or pull requests:

- None. Changes remain uncommitted in the working tree; no release was published.

## 1. Executive finding

The source-level M003 capability is implemented: the generic relay has an additive opt-in connection-wide idle timeout, distinct timeout reporting with directional byte totals, unchanged existing API types/signatures, and no changes to Eggress relay defaults. Focused relay, core, and public API checks pass. The milestone is conditionally closed because the full workspace verification gate did not complete cleanly and release/downstream handoff evidence is intentionally deferred until a separately authorized publication.

## 2. Requirement-to-evidence matrix

| Requirement | Evidence | Result | Notes |
|---|---|---|---|
| Additive API and separate outcome; preserve published literals and exhaustive matches | `relay_with_idle_timeout`, `RelayIdleTimeoutOutcome`; `eggress-embed/tests/public_api.rs` | pass | Existing `RelayOptions` struct literal and `RelayTermination` exhaustive match compile. |
| Disabled behavior, first-poll start, and zero-duration expiry before stream polling | `idle_timeout_none_keeps_silent_relay_open`, `idle_deadline_starts_on_first_poll_not_future_construction`, `zero_idle_timeout_expires_before_polling_streams` | pass | Paused-time tests. |
| Connection-wide resets only after successful writes in either direction | Upstream/downstream reset tests; buffered-but-unwritten test | pass | Reads and buffered input do not reset the deadline. |
| Half-close drain, independent deadlines, exact tie, and exact byte counts | Half-close expiry/completion and deadline precedence tests | pass | Drain timeout remains distinct and wins its exact tie with idle expiry. |
| I/O errors and cancellation/resource release | Opted-in I/O error/count test; stream drop test | pass | Errors remain `RelayFailure`; both streams are released. |
| Workspace-wide verification | Workspace Clippy, format, and locked tests | partial | Filtered locked workspace tests pass with one stalled connector test excluded; unfiltered tests timed out in that test. Clippy fails on unexpected `ssh` cfg values in `benches/tls_setup.rs`; full formatting reports a pre-existing CLI-test formatting difference. |
| Release and downstream adoption handoff | No release or downstream issues created | deferred | Follow the implementation plan after a separately authorized compatible release. |

## 3. Production implementation evidence

- Added `RelayIdleTimeoutOutcome::{Completed, IdleTimedOut}` and `relay_with_idle_timeout(client, server, options, idle_timeout)` in `crates/eggress-relay/src/lib.rs`.
- Integrated idle and drain deadlines in the existing shared relay future. Successful non-empty writes reset the connection-wide deadline; both streams remain owned by the caller's future.
- Retained the existing `relay()` and `relay_with_options()` signatures/results and preserved their legacy drain-check ordering. Existing Eggress/core defaults remain unchanged.
- Added Tokio `test-util` as a dev-only feature, deterministic timeout/half-close/error/drop tests, public API compatibility coverage, and crate/architecture/API documentation.
- No release version, protocol behavior, compatibility claim, downstream consumer source, or GitHub issue was changed.

## 4. Verification executed

### Commands run

```bash
rustfmt --edition 2021 crates/eggress-relay/src/lib.rs crates/eggress-embed/tests/public_api.rs
cargo fmt --package eggress-relay --package eggress-embed -- --check
cargo test -p eggress-relay --locked
cargo test -p eggress-core --locked relay
cargo test -p eggress-embed --locked --test public_api
cargo clippy -p eggress-relay --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --locked -- --skip direct_connect_falls_back_to_next_resolved_address
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

### Results

- Targeted formatting passed.
- `cargo test -p eggress-relay --locked`: 23 unit tests and 2 doctests passed.
- `cargo test -p eggress-core --locked relay`: 8 relay tests passed.
- `cargo test -p eggress-embed --locked --test public_api`: 5 tests passed.
- Relay-crate Clippy passed with `-D warnings`.
- `cargo clippy --workspace --all-targets -- -D warnings` failed on `unexpected cfg condition value: ssh` in `benches/tls_setup.rs:92,96`, promoted by `-D warnings`.
- `cargo fmt --all -- --check` reports a pre-existing formatting difference in the pre-existing working-tree edit `crates/eggress-cli/tests/pproxy_binary.rs`; that file was not modified for M003.
- `cargo test --workspace --locked` ran many suites but did not complete within 300 seconds; it was still running `eggress-core::connector::tests::direct_connect_falls_back_to_next_resolved_address`. The filtered command `cargo test --workspace --locked -- --skip direct_connect_falls_back_to_next_resolved_address` passed across the remaining workspace tests and doctests with that one test filtered out. The unfiltered workspace suite remains incomplete.
- External interoperability suites were not run; no compatibility claim changed. No benchmark, audit, release, or downstream issue action was in scope.

## 5. Invariant review

- Existing public relay APIs, public struct literals, and exhaustive termination matches remain source-compatible: supported by the compile test.
- Existing calls and the core compatibility facade do not enable idle timing; quiet `None` behavior and core relay tests pass.
- `None` disables timing; zero expires before stream I/O; a positive timer begins on first poll.
- Successful writes in either direction reset the shared deadline; EOF, reads, buffered input, and shutdown do not.
- Idle timing remains active through half-close drain. `DrainFor` stays independent, with drain expiry retaining precedence on an exact tie.
- Directional byte accounting, I/O error direction/source, stream release, and cancellation ownership remain covered by focused tests.
- No detached task or timer service was introduced.

## 6. Failure and recovery review

The timeout is a normal terminal result and drops both streams through completion of the shared future. I/O failures remain errors with directional counts. Cancellation still drops the owning future and its streams. No persistent state, retries, or recovery protocol was added. The timeout remains opt-in because legitimate quiet streams may otherwise be closed.

## 7. Migration and compatibility review

This is an additive API. Existing callers require no migration; the relay core facade does not opt in. `DrainFor` retains its half-close meaning and is not reinterpreted. No compatibility manifest or pproxy claim changed.

## 8. Security review

No secrets, payload logging, authentication, or privilege behavior was added. The timeout outcome exposes only the existing directional byte totals.

## 9. Documentation and operations

Updated `crates/eggress-relay/README.md`, public rustdoc in `crates/eggress-relay/src/lib.rs`, `architecture/relay.md`, `docs/RUST_API.md`, the proxy-core subsystem roadmap, the implementation plan, `plans/registry.md`, and `docs/ROADMAP.md`. This closure record captures the test and operational boundaries.

## 10. Unresolved findings

| Severity | Finding | Impact | Required action |
|---|---|---|---|
| Medium | Workspace Clippy is blocked by the `ssh` cfg warnings in the TLS benchmark. | The full lint gate is not qualified. | Resolve under an appropriately scoped maintenance change or verify after its fix. |
| Medium | Workspace formatting includes a pre-existing difference in `crates/eggress-cli/tests/pproxy_binary.rs`. | Full formatting gate is not clean; that unrelated edit was preserved. | Have its owner format/review it, then rerun the full formatting check. |
| Medium | Workspace tests exceeded the 300-second command limit in a connector test. | The full locked workspace test result is unknown. | Rerun in an environment where the connector test completes; record the complete result. |
| Low | No compatible release or downstream adoption issues exist yet. | Consumers cannot use the API from a published version, and adoption is untracked. | After separately authorized publication, open the two linked issues or record a no-action disposition per the implementation plan. |

## 11. Roadmap disposition

Milestone conditionally closed with named verification and operational evidence outstanding. Source implementation and focused qualification are complete; do not treat this as a published API or downstream adoption. Complete the broad verification items and post-publication handoff before claiming fully closed operational status.

## 12. Registry updates

- Mark Proxy core and relay M003 conditionally closed in `plans/registry.md` and `plans/subsystems/proxy-core-relay-roadmap.md`.
- Keep release publication and downstream issue creation as post-publication follow-ups; neither was performed.
