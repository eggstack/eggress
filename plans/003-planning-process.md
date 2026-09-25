# Eggress Planning and Agent-Handoff Process

Status: normative planning governance

This document defines how Eggress's long-term architecture is translated into actionable work without allowing short-lived implementation details to destabilize the canonical specification, terminology, or master roadmap.

The keywords MUST, MUST NOT, REQUIRED, SHOULD, SHOULD NOT, and MAY are normative.

## 1. Purpose

Eggress requires two distinct planning horizons:

1. **Long-term planning** defines product identity, architectural boundaries, invariants, capability dependencies, non-goals, and end-state acceptance criteria.
2. **Interim planning** defines bounded implementation work against a particular repository baseline and is intended for handoff to coding agents.

These horizons MUST remain separate. Interim plans may discover evidence that warrants a long-term change, but they MUST NOT silently edit long-term direction to match the easiest implementation.

## 2. Document classes

### 2.1 Canonical long-term documents

Canonical long-term documents are:

- `plans/000-long-term-specification.md`;
- `plans/001-terminology-and-domain-model.md`;
- `plans/002-long-term-roadmap.md`;
- this planning-governance document.

The first three MUST remain stable during ordinary feature implementation. They MAY be amended only when:

- product direction has intentionally changed;
- a contradiction or material omission has been identified;
- an accepted ADR requires the canonical end state to change;
- the user explicitly directs a long-term architecture revision.

A corrective implementation pass is not, by itself, justification for changing a long-term requirement. A compat-tier change additionally requires the manifest plus oracle/differential/interop evidence, per the specification.

### 2.2 Architecture decision records

ADRs capture one architectural decision that affects several milestones, subsystems, or durable public contracts.

An ADR MUST state:

- context and forces;
- considered alternatives;
- the selected decision;
- consequences and tradeoffs;
- affected long-term sections and subsystems;
- migration or compatibility implications;
- status: proposed, accepted, rejected, deprecated, or superseded.

Accepted ADRs MUST NOT be rewritten to conceal history. A later decision supersedes the prior ADR and links to it.

### 2.3 Subsystem roadmaps

A subsystem roadmap translates relevant long-term requirements into one coherent workstream. It is longer-lived than an implementation plan but more adaptable than the canonical roadmap.

A subsystem roadmap MUST define:

- subsystem purpose and ownership boundary;
- relevant specification and terminology references;
- invariants and non-goals;
- current-state summary;
- dependency graph;
- ordered milestones;
- user-visible exit conditions;
- cross-cutting security, migration, protocol, storage, and observability concerns;
- known risks and deferred work.

A subsystem roadmap SHOULD avoid commit-specific file lists, exact current line numbers, and mechanical implementation sequences.

### 2.4 Milestone implementation plans

A milestone implementation plan is the primary handoff artifact for a coding agent.

It MUST be independently executable, bounded, and tied to a repository baseline. It MUST include:

- source subsystem roadmap and milestone;
- relevant ADRs and long-term requirements;
- objective and explicit non-goals;
- current implementation evidence;
- invariants that cannot regress;
- expected production-code changes;
- config, protocol, migration, and compatibility effects;
- ordered work packages;
- focused and broad verification commands;
- static guards and documentation updates;
- acceptance and stop conditions;
- closure evidence required.

An implementation plan MAY change as repository reality changes. Material deviations MUST be recorded rather than hidden.

### 2.5 Closure records

A closure record determines whether a milestone is actually complete.

It MUST include:

- implementation commits or pull requests;
- requirement-to-evidence matrix;
- tests and guards run, with outcomes;
- migration and compatibility evidence;
- security and contention evidence where applicable;
- documentation and operational evidence;
- known limitations;
- unresolved findings classified by severity;
- recommendation: closed, conditionally closed, corrective pass required, or blocked.

A code commit message saying a plan is closed is not sufficient closure evidence.

### 2.6 Archive records

Completed, superseded, or abandoned interim plans SHOULD move under `plans/archive/` once they are no longer active. Archive moves MUST preserve traceability and SHOULD retain original filenames and subsystem grouping.

Canonical long-term documents and accepted ADRs MUST NOT be archived merely because their initial implementation completed.

## 3. Work classification

Every planned item MUST be assigned one primary class.

### Invariant

A property that must remain true across releases and implementation strategies. Examples: boxed-stream boundaries, fail-closed composition, credential redaction, snapshot reload with fixed topology, shutdown order, socket-truthful metadata, policy-scoped transport reuse, manifest-governed compat claims. Invariant work normally requires static guards, property tests, or architecture-level evidence.

### Capability

User-, developer-, operator-, or integration-visible behavior. Examples: SOCKS5 UDP ASSOCIATE relay, `--ssl` TLS listener generation, `eggress upstream test`, `OutboundConnector` chain execution, wheel-matrix delivery. Capability completion requires end-to-end acceptance evidence, not merely internal types.

### Infrastructure

Internal machinery used by one or more capabilities. Examples: chain executor, health state machine, snapshot compiler, differential harness, publish helper. Infrastructure SHOULD expose clear contracts and tests but MUST NOT be represented as completed capability until a consumer path exists.

### Polish

Ergonomics, diagnostics, performance improvements, cleanup, documentation, or maintainability work that does not establish the principal capability boundary. Polish SHOULD normally follow functional and correctness closure unless it removes an immediate safety or usability blocker.

## 4. Dependency model

The master roadmap provides macro-level ordering. Subsystem roadmaps refine dependencies into milestones.

Each milestone MUST declare dependencies as one of:

- **hard** — implementation cannot correctly begin before the dependency closes;
- **interface** — work may proceed against an agreed contract or test double;
- **soft** — parallel work is possible, but integration depends on the other milestone;
- **operational** — implementation can land, but deployment or release depends on external evidence.

A milestone is dependency-ready only when every hard dependency is closed and every interface dependency has a stable written contract.

The active planning registry MUST identify blocked milestones and their blockers.

## 5. Milestone sizing

A handoff milestone SHOULD be small enough that one implementation agent can understand the ownership boundary, implement the production changes, add focused tests, run the required verification, update documentation, and report residual risks in one coherent pass without redesigning unrelated subsystems.

A milestone is too large when it combines several independently releasable capability boundaries, requires unrelated config migrations, or contains several unresolved architecture decisions. A milestone is too small when it only renames one symbol or adds isolated test coverage without meaningful closure evidence, unless it is a corrective action required to unblock another milestone. Prefer vertical slices that establish one complete contract over broad horizontal refactors with no consumer.

## 6. Agent handoff contract

An implementation agent receives one primary milestone plan. The plan MUST tell the agent which documents are authoritative and which may be edited.

The default authority order is:

1. canonical long-term specification and terminology;
2. accepted ADRs;
3. subsystem roadmap;
4. milestone implementation plan;
5. current repository evidence (including `architecture/` deep dives, `docs/ROADMAP.md`, the parity manifest/matrix, `docs/CI_STATUS.md`, `docs/TESTING.md`).

When repository evidence conflicts with the plan, the agent SHOULD preserve long-term invariants, record the discrepancy, and make the smallest coherent adjustment necessary. The agent MUST NOT invent a new architecture merely to finish the checklist.

The agent MUST:

- inspect current code before editing;
- preserve unrelated user changes;
- box streams at protocol/transport boundaries;
- fail closed on unsupported composition with structured diagnostics;
- redact credentials in all output;
- update tests and architecture docs with code;
- produce a closure-oriented status report;
- identify anything not completed.

## 7. Corrective passes

A corrective pass is a new implementation plan, not an amendment pretending the original milestone succeeded.

Corrective plans MUST:

- reference the original milestone and closure record;
- list each unclosed requirement or discovered defect;
- explain why original verification did not catch it;
- include regression tests or guards preventing recurrence;
- avoid reopening already closed scope without evidence.

Repeated corrective passes indicate that the subsystem roadmap or milestone sizing should be revised. Compat-claim corrections additionally require manifest updates plus oracle/differential/interop runs.

## 8. Updating subsystem roadmaps

Subsystem roadmaps MAY evolve as implementation reveals new dependencies or better decomposition. Updates MUST preserve links to canonical long-term requirements, completed milestone history, reasons for reordering or splitting work, and explicit status of removed or deferred items. A roadmap MUST NOT mark a capability complete solely because its infrastructure milestone landed.

## 9. Registry requirements

`plans/registry.md` is the active planning control surface. It MUST remain compact and SHOULD contain only: active subsystem roadmaps, dependency-ready implementation plans, active or recently completed implementation plans, required closure passes, blocked work and blockers, latest status or closure record. The registry MUST link to source documents rather than duplicate their detailed content. It MUST agree with `docs/ROADMAP.md` on active work.

## 10. Required planning review

Before an implementation plan is handed off, review it for: correct long-term references; unresolved architecture decisions; dependency readiness; bounded scope and non-goals; explicit ownership and invariants; migration and compatibility effects; concurrency, cancellation, restart, and failure semantics; security and authorization effects (including redaction); required test and static-guard evidence; unambiguous closure criteria. If these are not answerable, the work is not ready for implementation handoff.

## 11. Planning anti-patterns

Prohibited or strongly discouraged: adding transient TODO checklists to the canonical long-term specification; creating one roadmap that mixes all subsystems at implementation-file granularity; handing an agent a broad product goal without a bounded milestone contract; equating compilation with closure; changing terminology independently in each subsystem; allowing implementation plans to override accepted architecture silently; retaining stale active plans after the underlying repository has materially changed; repeating the same requirements in several files without one authoritative source; recording only successful evidence while omitting blocked or unrun verification; creating polish phases before the capability's correctness boundary is closed; changing a compat claim by editing a generated report instead of the manifest.

## 12. Eggress subsystem decomposition

The long-term roadmap is expected to produce subsystem roadmaps approximately along these boundaries (aligned with `architecture/`):

- proxy core and relay;
- server, runtime lifecycle, and configuration;
- routing, health, metrics, and admin;
- edge protocols (HTTP/SOCKS/Shadowsocks/Trojan/tunnels/reverse);
- transports (TLS/SSH/QUIC/H3);
- outbound chains and connectors;
- parity contract and compatibility translation;
- delivery (CLI, system-proxy, embed, Python, release).

This is an initial decomposition, not a fixed release list. A subsystem boundary MAY be split when ownership or dependency analysis warrants it.
