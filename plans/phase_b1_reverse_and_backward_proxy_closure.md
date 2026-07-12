# Phase B1 — Reverse and backward proxy closure

## Objective

Complete Eggress reverse/backward proxying as a real pproxy-compatible runtime surface. The end state must support valid configuration, pproxy URI translation, process supervision, external data-plane connections, reconnect and drain behavior, parallel control channels where pproxy supports them, chain composition, Python exposure, security policy, and differential evidence.

This phase begins only after A1 has repaired the immediate configuration regression and A2/A3 have established stable composition and oracle IDs.

## Current state and gap

The repository already contains reverse server/client primitives, a control channel, authentication, reconnect behavior, metrics, URI recognition, TOML sections, supervisor integration, and admin visibility. However, the recent compiler path exposed that `external_bind` was not represented in the model and was therefore always `None`. Documentation also lists several reverse capabilities as complete while parallel channels, chain composition, built-in TLS, and Python backward-mode support remain incomplete.

The goal is not to redesign reverse proxying into a new multiplexed protocol. Preserve pproxy's observable model unless a divergence is explicitly required for safety.

## Reference characterization

Before implementation, use the A3 oracle to characterize pinned pproxy behavior for:

- `+in`, repeated `+in`, and ordering of modifiers;
- `bind://`, `listen://`, `backward://`, and `rebind://` aliases;
- control connection count and whether repeated `+in` means parallel channels;
- authentication bytes and framing;
- target specification and defaulting;
- external listener startup timing;
- behavior when control connection is unavailable;
- reconnect timing and backoff;
- behavior when a control connection dies during a relayed stream;
- whether each user stream consumes one control connection;
- jump-chain composition before or after the reverse hop;
- TLS wrapper behavior;
- stdout/stderr and exit semantics for invalid reverse combinations.

Record transcripts and do not infer behavior solely from pproxy source structure.

## Workstream 1: configuration model and validation

1. Finalize typed reverse-server fields:
   - stable ID/name;
   - control bind/address;
   - external bind/address;
   - authentication username/password or secret source;
   - connection/session limits;
   - access policy/allowlist;
   - TLS wrapper settings if supported;
   - drain and handshake timeouts.
2. Finalize typed reverse-client fields:
   - server address;
   - authentication;
   - default target host/port where required;
   - parallel connection count;
   - reconnect initial/max delay;
   - heartbeat/read timeout if retained;
   - optional prior-hop chain;
   - TLS settings.
3. Define safe defaults only where they match pproxy or are clearly native-only. Require explicit external bind when ambiguity could expose a listener.
4. Validate partial credentials, partial targets, zero/invalid parallel count, invalid backoff ranges, wildcard/public binds without policy, unsupported TLS/chain combinations, duplicate IDs, address-family mismatches, and self-referential loops.
5. Ensure redacted TOML and diagnostics never expose secrets.

## Workstream 2: compiler and supervisor integration

1. Remove all placeholder `None` assignments for user-configurable fields.
2. Compile reverse services into the same normalized capability/composition model used by A2.
3. Start reverse servers and clients under `ServiceSupervisor` with clear ownership.
4. Ensure startup is atomic: if a required bind or control connection cannot be established under the configured startup policy, report a deterministic error and clean up all partial resources.
5. Track tasks with existing task-tracking/cancellation primitives.
6. Ensure reload behavior is explicitly defined. At minimum:
   - mutable credentials/routes/targets may reload only if safe;
   - listener topology and bind changes require restart unless deliberately implemented;
   - old configuration remains active if compilation of new config fails.
7. Expose bound addresses, control-channel state, active streams, reconnect count, rejected auth, and last error in status/admin APIs with bounded cardinality.

## Workstream 3: control-channel correctness

1. Preserve bounded newline-delimited authentication framing introduced by recent fixes.
2. Define the full control handshake as a typed codec with strict maximum lengths and timeouts.
3. Verify constant-time secret comparison where practical.
4. Prevent connection counter underflow, double decrement, and stale session reuse.
5. Define ownership of each control connection and each relayed stream.
6. Handle control loss during:
   - idle state;
   - external accept waiting for a channel;
   - target connection establishment;
   - active bidirectional relay;
   - shutdown/drain.
7. Bound pending external connections and reject or time them out predictably when no control channel is available.
8. Add backpressure so a slow target/client cannot create unbounded buffering.
9. Add protocol versioning only if necessary; remain compatible with pinned pproxy where direct interoperation is a goal.

## Workstream 4: parallel control channels

Implement `parallel_connections` and repeated pproxy inbound modifiers according to oracle behavior.

Requirements:

- independent authenticated channels;
- fair channel selection;
- no channel handed to two streams simultaneously;
- prompt removal of dead channels;
- bounded reconnect storms with jitter only if it does not break required timing semantics;
- metrics for desired, connected, available, busy, rejected, and reconnecting channels;
- deterministic shutdown of all channels;
- tests with one, several, and partially failing channels.

If pproxy uses one user stream per control connection, preserve that model. Do not add multiplexing and then claim exact parity unless compatibility is proven.

## Workstream 5: data-plane target and chain composition

1. Ensure the reverse client connects to the target supplied or implied by the reverse mode.
2. Support IPv4, IPv6, and domain targets where pproxy does.
3. Integrate existing route/upstream chain execution so a target may be reached through supported prior hops if pproxy permits this composition.
4. Determine whether chains apply on the external side, control-client side, target side, or multiple positions; model each cell explicitly.
5. Preserve deferred success semantics: do not tell the external client a stream is ready before required downstream setup succeeds when the protocol exposes a success reply.
6. Add loop detection for reverse services connecting back into themselves.
7. Define half-close propagation and ensure each direction can drain independently.
8. Track relay byte counts and terminal reason without leaking destination secrets.

## Workstream 6: TLS and authentication policy

Characterize pproxy's TLS composition. If supported and in scope:

- allow TLS on control listener/client;
- use rustls and existing certificate/root abstractions;
- verify certificates by default;
- support custom CA and explicit insecure compatibility mode;
- handle SNI and ALPN as appropriate;
- add client and server interoperability tests.

If pproxy does not provide built-in TLS for the relevant reverse mode, retain plaintext parity and document native TLS wrapping separately. Do not classify external stunnel guidance as drop-in runtime parity.

Security requirements:

- no unauthenticated public reverse listener by default;
- explicit warnings for wildcard/public external bind;
- bounded auth failures and handshake reads;
- integration with future auth-throttling policy;
- secret redaction in errors, status, metrics labels, and debug representations.

## Workstream 7: pproxy CLI and URI compatibility

1. Normalize all supported reverse aliases and modifiers into one typed representation.
2. Preserve modifier ordering only where semantically relevant.
3. Translate repeated `+in` into the characterized parallel-channel behavior.
4. Support reverse forms inside `__` chains only for matrix cells explicitly marked supported.
5. Reject impossible compositions during `pproxy check` with stable diagnostic codes and a useful explanation.
6. Ensure the drop-in `pproxy` binary starts reverse services directly.
7. Add exact/normalized tests for help, invalid forms, exit codes, and startup messages where scripts may depend on them.

## Workstream 8: Python exposure

Expose reverse mode through the current service API and prepare it for later low-level Python parity.

Required surface:

- create from pproxy args/URI;
- create from native TOML/config objects;
- start/astart;
- bound external/control addresses;
- connected/available channel counts;
- active stream count;
- last reconnect/error state;
- reload where supported;
- close/wait_closed and async context manager behavior;
- stable exceptions for configuration, bind, auth, connection, and shutdown failures.

No Python call may hold the GIL while waiting on network I/O or shutdown.

## Workstream 9: testing and fault injection

Add unit tests for codecs, validation, URI normalization, redaction, counter transitions, backoff bounds, and channel selection.

Add end-to-end tests for:

- one reverse server/client and one external stream;
- multiple sequential streams;
- concurrent streams with parallel channels;
- no channel available;
- auth success/failure and oversized handshake;
- target refusal and timeout;
- external client disconnect before assignment;
- control disconnect before assignment and during relay;
- reverse client restart/reconnect;
- server restart;
- graceful drain and forced cancellation;
- IPv4, IPv6, and domain targets;
- supported chain composition;
- TLS modes if implemented;
- Python lifecycle;
- status/admin accuracy;
- bounded pending queues and resource exhaustion.

Add A3 differential scenarios for every supported pproxy reverse URI family. Add direct Eggress↔pproxy interoperability if their control protocol is intended to be wire-compatible; otherwise compare end-to-end observable behavior with separate same-implementation pairs.

Run repeated soak/fault tests to detect leaked tasks, sockets, counters, and reconnect storms.

## Diagnostics

Define stable codes for at least:

- missing external bind;
- missing target;
- partial credentials;
- unsafe public bind;
- unsupported reverse composition;
- unsupported reverse UDP;
- unsupported reverse chain position;
- control authentication failure;
- control handshake too large/timeout;
- no control channel available;
- target connection failure;
- reverse loop detected;
- platform-unavailable transport;
- reload requires restart.

## Documentation deliverables

Update:

- reverse configuration reference;
- pproxy URI/migration guide;
- security guidance;
- architecture and lifecycle description;
- admin/metrics reference;
- Python API documentation and stubs;
- capability manifest and generated matrix/report;
- examples for local, authenticated public, parallel, chained, TLS, and system-service deployment.

Examples must use safe loopback defaults unless the purpose is explicitly public exposure.

## Acceptance criteria

- Reverse server and client are fully expressible through TOML and pproxy-compatible URI/CLI forms.
- A real external client reaches a configured target through the reverse path.
- Repeated/parallel inbound behavior matches the pinned pproxy oracle.
- Parallel channels support concurrent streams without double assignment or counter corruption.
- Reconnect, target failure, control loss, half-close, drain, and cancellation are deterministic and tested.
- Supported chain compositions are explicit in the A2 matrix and pass end-to-end tests.
- Unsupported reverse UDP and other invalid cells fail before startup.
- Python service APIs expose lifecycle and status without blocking the GIL.
- Metrics/admin state remain correct under failures and reconnects.
- Secrets are redacted and public unauthenticated exposure produces a hard error or explicit policy-controlled warning.
- Every promoted reverse capability references unit/integration and A3 differential evidence.
- README and parity reports no longer contain caveats inconsistent with actual runtime behavior.

## Out of scope

- inventing a multiplexed reverse protocol when pproxy uses one stream per channel;
- reverse UDP unless oracle characterization proves it exists and a separate approved extension is added;
- implementing new upstream protocols solely for reverse chaining;
- full low-level Python `Connection`/`Server` compatibility, which belongs to Track C;
- runtime promotion of WS/raw/H2, which belongs to B3/B4.

## Handoff sequencing

Recommended commit order:

1. oracle characterization and design note;
2. config/model/compiler repair;
3. typed control codec and lifecycle fixes;
4. supervisor integration and observability;
5. parallel channels;
6. chain/TLS composition;
7. CLI/Python exposure;
8. differential, fault, soak, documentation, and manifest updates.

Do not promote manifest tiers until the corresponding end-to-end and differential gates pass.