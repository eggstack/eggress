# Milestones A–C Corrective Pass

## Status

**In progress — 25/31 acceptance criteria met.** Remaining items:
- URI factory signatures aligned (18 protocol constructors now match oracle `(self, param)`)
- Plugin lifecycle tests added (15/15 oracle/candidate matched)
- Runtime/failure/cleanup dimensions captured (11/11 matched)
- Protocol accept/guess/connect methods aligned to oracle signatures
- Hosted CI blocked on infrastructure (known non-functional)

## Parent roadmap and plans

- `plans/PPROXY_FULL_DROP_IN_ROADMAP.md`
- `plans/MILESTONE_A_HONEST_CONTRACT.md`
- `plans/MILESTONE_B_PYTHON_SOURCE_COMPATIBILITY.md`
- `plans/MILESTONE_C_FUNCTIONAL_INTERNAL_API.md`

## Purpose

Reopen and correctly close Milestones A, B, and C after the first implementation pass added substantial compatibility scaffolding but marked capabilities and milestones complete without satisfying the roadmap's strict behavioral evidence requirements.

This is a corrective integration pass, not a redesign. Preserve and reuse the current:

- strict manifest parser and observation data model;
- comparator infrastructure;
- frozen `pproxy==2.7.9` package provenance;
- separate `eggress-pproxy-compat` distribution;
- compatibility proxy object hierarchy;
- asyncio stream adapter;
- protocol parsing helpers;
- cipher implementations and registries;
- plugin bridge;
- Rust outbound connector and server/runtime primitives.

The pass must correct the evidence model, reconnect compatibility objects to live runtime behavior, and replace structural or candidate-only completion claims with paired oracle-backed evidence.

## Why this pass is required

The current branch contains useful implementation work, but the following contradictions prevent honest closure:

1. Milestone A remains marked `Ready for implementation`, while later documents treat its evidence system as complete.
2. The generated strict report still reports 83 gaps and 57% readiness, while Milestone B and README text report only two remaining gaps.
3. Several strict-manifest records were changed from `gap` to `not_applicable` even though Milestone C explicitly targets those publicly importable internal APIs.
4. Multiple `drop_in` records use `module_existence` as their comparator and therefore do not validate signature, coroutine shape, return type, attributes, exceptions, lifecycle, or behavior.
5. The supposed upstream example fixtures are synthetic and largely commented out rather than immutable executable upstream examples.
6. `pproxy.Connection` and `pproxy.Server` now have the correct top-level alias shape, but the returned proxy objects still raise `NotImplementedError` for common `tcp_connect`, UDP, and server lifecycle paths.
7. `__` chains are returned as independent lists instead of a nested `.jump` chain matching pproxy.
8. The asyncio adapter exists but is not wired into the compatibility objects and has known semantic mismatches.
9. `pproxy.server` constants, signatures, shared authentication behavior, scheduler behavior, handlers, and connection lifecycle differ materially from the oracle.
10. Current stream and datagram handler tests validate echo/pass-through behavior rather than proxy behavior.
11. Protocol `connect`, UDP, and channel methods remain unimplemented for several classes while Milestone C is marked complete.
12. Cipher completeness is partly measured by map membership even where the selected class always raises.
13. External differential and interoperability tests are scaffolding/import tests rather than executed oracle/candidate comparisons.
14. No visible hosted CI status or retained release evidence currently supports the new completion claims.

## Corrective outcome

At completion:

- Milestones A, B, and C have truthful status based on machine-generated evidence;
- the strict manifest and generated report are synchronized and cannot drift;
- all publicly observable Python API records remain in scope;
- every `drop_in` record has paired oracle/candidate evidence appropriate to its behavior;
- canonical upstream examples and selected tests execute unchanged against both environments;
- common `Connection`, `Server`, TCP, UDP, chain, and internal helper paths are functional;
- unsupported transport families remain explicit gaps assigned to later milestones;
- no acceptance criterion is marked met by importability, registry membership, scaffolding, or a deliberate `NotImplementedError` alone.

## Scope

### In scope

- compatibility status and documentation correction;
- strict manifest schema and validator hardening;
- report regeneration and stale-report enforcement;
- executable oracle/candidate runners;
- immutable executable upstream fixtures;
- Python namespace, signature, coroutine, return, attribute, exception, and lifecycle comparison;
- compatibility proxy object runtime integration;
- nested chain construction;
- asyncio reader/writer semantic correction;
- common server lifecycle integration;
- `pproxy.server` behavioral correction;
- functional common protocol methods;
- cipher registry truthfulness and common cipher interoperability;
- real differential and bidirectional interop tests;
- CI and retained evidence for A–C.

### Out of scope

Do not pull Milestone D–F protocol expansion into this corrective pass except where needed to classify an honest gap.

Remain deferred:

- SSH transport implementation;
- QUIC and HTTP/3 implementation;
- SSR implementation;
- full reverse and transparent composition expansion;
- complete multi-hop UDP through all transports;
- final CLI/process parity beyond correcting manifest/report claims;
- release certification for full pproxy parity.

The existence of deferred transports must not prevent A–C from being closed for the supported subset, but their public records must remain `gap` rather than `not_applicable`, `drop_in`, or import-only success.

---

# Workstream AC0 — Freeze and Reopen

## Objective

Stop further compatibility inflation before implementing fixes.

## Required changes

Update the three milestone plans:

- `plans/MILESTONE_A_HONEST_CONTRACT.md`
- `plans/MILESTONE_B_PYTHON_SOURCE_COMPATIBILITY.md`
- `plans/MILESTONE_C_FUNCTIONAL_INTERNAL_API.md`

Set status to `REOPENED — corrective pass in progress` until the acceptance gates in this document pass.

Update README wording so it distinguishes:

- current certified modern CLI/runtime subset;
- experimental Python source-compatibility surface;
- strict drop-in roadmap status.

Remove or qualify statements that proxy classes are fully implemented, that only two strict gaps remain, or that Milestone C is complete.

Do not delete historical completion summaries. Move them under a clearly labeled `Initial implementation pass` section so the repository retains provenance without treating those claims as current truth.

## Acceptance criteria

- No A–C plan is marked complete before this corrective pass closes.
- README does not claim full implementation of methods that raise `NotImplementedError`.
- Documentation consistently states that strict full drop-in parity is not yet achieved.
- A repository-wide documentation check rejects known contradictory phrases.

## Suggested tests

Add or extend `scripts/check_release_docs.py` to assert:

- plan statuses match generated manifest/report state;
- README gap counts match the generated strict report;
- no `COMPLETE` A–C status exists while required records remain unresolved;
- no `fully implemented` claim is present for classes containing certified-path `NotImplementedError` methods.

---

# Workstream AC1 — Restore Strict Manifest Integrity

## Objective

Make the strict manifest a truthful behavioral ledger rather than a mutable progress counter.

## Required changes

Audit every record in:

- `docs/parity/pproxy_2_7_9_strict_manifest.toml`

Correct the following classification errors:

1. Publicly importable and callable `pproxy.proto`, `pproxy.server`, `pproxy.cipher`, `pproxy.cipherpy`, and `pproxy.plugin` symbols remain in scope.
2. An implementation detail may be `not_applicable` only if it is not present or observable in the pinned oracle's installed package.
3. Unsupported SSH, QUIC/H3, SSR, and legacy cipher behavior remains `gap` until implemented or the full roadmap explicitly changes scope.
4. A class listed in a registry but raising for its normal operation is not complete.
5. A symbol existing under the correct name is namespace evidence only, not behavioral evidence.

Expand manifest fields where needed:

```toml
required_dimensions = [
  "module",
  "qualified_name",
  "signature",
  "callable_kind",
  "coroutine_shape",
  "return_shape",
  "attributes",
  "exceptions",
  "behavior"
]

evidence_level = "paired_oracle"
implementation_state = "functional" # or structural, partial, absent
```

Allowed evidence levels:

- `paired_oracle`
- `bidirectional_interop`
- `oracle_only_baseline`
- `candidate_only`
- `structural_only`
- `none`

Only `paired_oracle` or `bidirectional_interop` may support `drop_in` for behavioral records. `structural_only` may support only an explicit structural status, never `drop_in`.

Remove blanket milestone reassignment. Each record must have the milestone that actually owns its closure.

## Validator rules

Extend `crates/eggress-testkit/src/strict_manifest.rs` so validation fails when:

- `status = "drop_in"` and evidence level is not sufficient;
- a behavioral record uses only `module_existence`;
- an evidence reference does not exist;
- a test reference does not exist;
- a referenced test is skipped unconditionally;
- a class or method known to raise `NotImplementedError` is marked `drop_in` without an oracle proving the oracle also raises at that operation;
- manifest milestone counts do not match plan ownership;
- terminal status counts in the report differ from the manifest.

## Acceptance criteria

- Every record has a precise owner, milestone, implementation state, evidence level, and applicable dimensions.
- No public pproxy symbol is hidden through `not_applicable` reclassification.
- Every `drop_in` record has paired evidence matching all required dimensions.
- Unsupported methods and transport families are visible as gaps.
- The validator rejects each prior inflation pattern through regression tests.

---

# Workstream AC2 — Make the Report Generated and Non-Stale

## Objective

Eliminate hand-maintained or stale readiness claims.

## Required changes

Use one canonical generator for:

- `docs/parity/PPROXY_2_7_9_STRICT_REPORT.md`
- optional machine-readable JSON report under `target/compat/` or retained CI artifacts.

The report must include:

- current commit SHA;
- manifest hash;
- oracle package hash;
- generation timestamp;
- total records;
- counts by status, evidence level, milestone, category, platform, and implementation state;
- unresolved gap table;
- records whose evidence is stale;
- records blocked on unavailable optional dependencies;
- records with only candidate-side tests;
- records with known upstream defects.

Add a check mode:

```bash
cargo run -p eggress-testkit --bin strict-report -- --check
```

or an equivalent repository script.

`--check` must fail when checked-in report content differs from regeneration.

## Acceptance criteria

- README and milestone plans obtain counts from the generated report or avoid embedding counts.
- CI fails on stale reports.
- Report generation is deterministic apart from explicitly normalized timestamps.
- The report cannot say two gaps while the manifest says 83.

---

# Workstream AC3 — Replace Synthetic Fixtures with Executable Oracle Corpus

## Objective

Use unchanged executable pproxy material as the compatibility oracle.

## Required changes

Populate `compat/pproxy-2.7.9/` with immutable, provenance-recorded fixtures from the exact installed wheel or source repository corresponding to the pinned release.

Required fixture classes:

1. Top-level API examples:
   - `Connection(uri)` TCP example;
   - `Connection(uri)` UDP example;
   - `Server(uri).start_server(args)` example;
   - direct, HTTP, SOCKS5, Shadowsocks examples;
   - chain example using `__`.
2. Selected upstream tests covering:
   - URI factory behavior;
   - stream monkey patches;
   - rule compilation;
   - scheduling;
   - protocol address encoding;
   - cipher known-answer or round-trip behavior;
   - common TCP and UDP flows.
3. CLI fixtures:
   - `--help`;
   - `--version`;
   - parser defaults;
   - invalid URI and option combinations.

Do not represent executable compatibility evidence as commented examples. If an upstream fixture cannot run without network setup, parameterize only the environment endpoints while preserving the upstream code path and call structure.

Record per-file:

- upstream path;
- upstream commit or wheel hash;
- file hash;
- license;
- allowed normalization or adaptation;
- reason for any modification.

## Acceptance criteria

- At least one unchanged client TCP example executes successfully against the oracle and candidate.
- At least one unchanged server example starts, accepts a request, and shuts down against both.
- At least one unchanged UDP example executes against both for a supported path.
- Fixture hashes are validated before execution.
- Comment-only examples do not count as evidence.

---

# Workstream AC4 — Implement Real Oracle and Candidate Runners

## Objective

Turn the current observation schemas and comparators into a complete paired execution system.

## Required changes

Implement subprocess-isolated runners under `eggress-testkit` or dedicated scripts:

```text
oracle runner    -> clean venv with pproxy==2.7.9
candidate runner -> clean venv with eggress wheel + eggress-pproxy-compat wheel
```

Each scenario must execute the same probe code and emit normalized JSON.

Capture:

- module/import results;
- `inspect.signature`;
- `inspect.iscoroutinefunction`;
- type, `__module__`, and `__qualname__`;
- public/common attributes;
- return shape;
- awaitability;
- exception class, stage, and normalized category;
- warnings;
- stdout/stderr;
- exit code;
- child processes;
- open sockets and descriptors where supported;
- pending asyncio tasks;
- protocol transcript and payload hash;
- cleanup state.

The runner must distinguish:

- oracle setup failure;
- candidate setup failure;
- harness defect;
- behavioral mismatch;
- known upstream defect;
- platform constraint.

An oracle failure is never a candidate pass.

## Required files

Suggested layout:

```text
crates/eggress-testkit/src/strict_runner.rs
crates/eggress-testkit/src/strict_scenarios.rs
crates/eggress-testkit/src/strict_normalization.rs
scripts/strict_oracle_probe.py
scripts/strict_candidate_probe.py
tests/strict_api/
tests/strict_runtime/
```

## Acceptance criteria

- One command creates both environments and runs the paired suite.
- Each manifest evidence reference points to an actual scenario result.
- A deliberately injected signature, return-type, exception, and wire mismatch is detected in tests.
- Environment metadata and package hashes are retained with evidence.
- No `drop_in` record depends solely on in-process candidate tests.

---

# Workstream AC5 — Correct Top-Level Factory and Proxy Object Semantics

## Objective

Make `pproxy.Connection`, `pproxy.Server`, `pproxy.Rule`, `pproxy.DIRECT`, `proxy_by_uri`, and `proxies_by_uri` operationally compatible for supported protocol paths.

## Required changes

Preserve the correct alias arrangement already implemented:

```python
Connection = proxies_by_uri
Server = proxies_by_uri
Rule = compile_rule
DIRECT = ProxyDirect()
```

Correct `ProxyDirect` to match oracle-observable state and behavior:

- `bind` equivalent to oracle direct sentinel text;
- `lbind` behavior;
- `unix` value;
- `alive` initial state;
- `connections` accounting;
- UDP association map;
- `destination(host, port)`;
- `logtext(host, port)`;
- `match_rule(host, port)`;
- functional `open_connection`;
- functional `prepare_connection`;
- functional `tcp_connect`;
- functional `udp_open_connection`;
- functional `udp_sendto`;
- compatible equality, hashing, and repr where observed.

Wire direct TCP and UDP through Rust-owned connectors where possible. The compatibility layer may adapt the result into asyncio objects but must not create a temporary local proxy listener.

Correct `ProxySimple` and subclasses so common supported paths use their configured protocol chain, cipher, auth, TLS, and jump object.

Implement nested chain construction exactly:

```python
jump = DIRECT
for uri in reversed(uri_jumps.split("__")):
    jump = proxy_by_uri(uri, jump)
return jump
```

Do not return a list for a single `__` chain. Lists remain only where upstream returns independent remote options.

Correct URI parsing for:

- combined protocol schemes;
- credentials and ciphers;
- plugins;
- query rule file;
- fragments/users;
- host and port defaults;
- Unix paths;
- `in` reverse modifiers;
- optional dependency errors.

## Acceptance criteria

- `pproxy.Connection(uri)` returns an oracle-compatible object topology.
- A two- and three-hop `__` chain has the same nested `.jump` graph as the oracle.
- `await pproxy.Connection("direct://").tcp_connect(host, port)` returns a working reader/writer pair.
- Supported HTTP and SOCKS upstream connection paths execute through Rust primitives.
- `udp_sendto` works for direct and currently supported SOCKS5 paths.
- Unsupported SSH/QUIC/H3/SSR construction fails at the same stage and with a compatible dependency/unsupported category, remaining manifest gaps.

---

# Workstream AC6 — Repair and Integrate the Asyncio Adapter

## Objective

Provide the exact stream behavior consumed by pproxy applications and internals.

## Required corrections

Audit `python/eggress/_asyncio_adapter.py` against `asyncio.StreamReader`, `asyncio.StreamWriter`, and pproxy's monkey-patched methods.

Correct at minimum:

- `read(-1)` must read until EOF rather than call a zero-threshold fill loop;
- `read(0)` returns immediately;
- `__aiter__` is synchronous and returns `self`;
- `__anext__` handles EOF without surfacing an unintended `IncompleteReadError`;
- `write_eof()` matches the synchronous stdlib method shape;
- close and `wait_closed()` ordering;
- drain cancellation and buffered-write preservation;
- half-close behavior;
- exception propagation instead of converting all read errors into EOF;
- `get_extra_info` behavior and defaults;
- loop affinity;
- destructor/resource warnings.

Implement pproxy-required helpers:

- `read_w`;
- `read_n`;
- `read_until`;
- `rollback`.

Decide and test whether compatibility mode must monkey-patch standard asyncio classes at import time, as upstream does, or whether returned adapter classes provide an observationally equivalent contract. If third-party code checks the patched standard classes, reproduce the import side effect in the compatibility distribution only.

## Acceptance criteria

- Paired stream behavior tests pass for all helper methods.
- Cancellation during read, drain, connect, and close leaks no tasks or descriptors.
- `tcp_connect()` actually returns this repaired adapter or a true stdlib-equivalent stream pair.
- Upstream examples execute unchanged.
- No adapter method has the wrong sync/async callable kind.

---

# Workstream AC7 — Implement Common Server Lifecycle

## Objective

Make `pproxy.Server(uri).start_server(args)` functional for protocol paths already supported by Eggress.

## Required changes

Implement compatibility lifecycle methods on the factory-returned common proxy object:

- `start_server(args, stream_handler=...)`;
- `udp_start_server(args)` where supported;
- returned handle shape;
- close and wait-closed behavior;
- error timing;
- repeated close behavior;
- signal/cancellation cleanup.

The implementation should delegate to Eggress Rust listener/runtime primitives but expose pproxy's Python-facing contract.

Required supported listener paths for this pass:

- direct/common TCP listener construction used by pproxy;
- HTTP;
- SOCKS4/4a;
- SOCKS5;
- current supported Shadowsocks AEAD;
- Trojan where already supported inbound;
- supported TLS wrapping;
- supported H2 path if the Rust runtime already provides it.

Do not fake SSH, QUIC/H3, or SSR server objects. Leave explicit gaps.

## Acceptance criteria

- The unchanged canonical server example starts and accepts traffic.
- Returned server handles expose expected close/lifecycle behavior.
- Supported listener protocols pass candidate-client/oracle-server and oracle-client/candidate-server scenarios.
- Failed bind, invalid credentials, cancellation, and close produce compatible failure categories.

---

# Workstream AC8 — Correct `pproxy.server` Internals

## Objective

Match the observable behavior of server helpers rather than merely exporting their names.

## Required corrections

### Constants and sentinels

Match oracle values and types for:

- `SOCKET_TIMEOUT`;
- `UDP_LIMIT`;
- `DUMMY`;
- `DIRECT`;
- `sslcontexts`.

### Stream patches

Implement `patch_StreamReader` and `patch_StreamWriter` with the same class-level side effects and helper semantics as the oracle. No-op wrappers are insufficient.

### `AuthTable`

Reproduce shared state keyed by remote IP, expiration clock semantics, missing-key behavior, and state visibility across instances.

### `prepare_ciphers`

Match the oracle signature and async behavior:

- inputs: cipher, reader, writer, bind, server-side flag;
- plugin initialization order;
- cipher callback setup;
- return shape.

Do not substitute a configuration dictionary for the oracle's stream-wrapping result.

### `schedule`

Match:

- signature;
- alive and rule filtering;
- `fa`, `rr`, `rc`, and `lc` algorithms;
- mutation of the server list in round-robin mode;
- empty-set behavior;
- error behavior for unknown algorithms.

### `compile_rule`

Match the oracle's brace-regex and file-regex callable return contract. Returning a custom dictionary is not compatible.

### `check_server_alive`

Match its coroutine/loop behavior, alive transitions, cancellation, verbose callbacks, direct-object skip, and writer cleanup.

### `stream_handler`

Implement the oracle sequence:

1. TLS wrapping;
2. peer/local address extraction;
3. cipher preparation;
4. listener protocol detection and accept;
5. echo/empty/block handling;
6. scheduler selection;
7. upstream open;
8. upstream protocol preparation;
9. client response callback;
10. bidirectional channel tasks;
11. accounting and cleanup;
12. debug-mode propagation.

### `datagram_handler`

Implement:

1. cipher decrypt;
2. protocol UDP accept/unpack;
3. echo/empty/block handling;
4. scheduler selection;
5. upstream UDP framing;
6. send/association creation;
7. reply framing and encryption;
8. logging and error behavior.

### `test_url`, `print_server_started`, and `main`

Match callable kind, signature, output, return shape, and failure behavior.

## Acceptance criteria

- Each helper has paired oracle tests.
- Function signatures and coroutine status match.
- No common-path helper is a no-op or pass-through substitute.
- Stream and datagram handlers relay through an actual upstream endpoint in tests.
- Existing echo/pass-through tests are retained only as unit tests and are not cited as parity evidence.

---

# Workstream AC9 — Complete Common Protocol Internal Behavior

## Objective

Make the common `pproxy.proto` surface functional for supported paths.

## Required changes

Audit every protocol class and method against the oracle:

- constructor and fields;
- `name`;
- `guess`;
- `accept`;
- `connect`;
- `channel`;
- `http_channel`;
- `udp_accept`;
- `udp_connect`;
- `udp_pack`;
- `udp_unpack`;
- stream patch helpers;
- address helpers;
- TLS wrapping.

Required functional classes for A–C closure:

- `Direct`;
- `HTTP` and `HTTPOnly`;
- `Socks4`;
- `Socks5`;
- supported current `SS` paths;
- supported `Trojan` paths;
- `WS`/raw/tunnel helpers already backed by runtime support;
- `Echo` and `Empty` behavior;
- supported H2 internals where available.

Methods may call thin Python adapters over Rust protocol/runtime primitives. They must not remain construction-only metadata when the oracle method is callable.

For unsupported classes:

- preserve namespace and constructor evidence;
- raise at the oracle-compatible stage;
- keep behavior records as gaps assigned to Milestone D;
- do not mark them `not_applicable`.

## Acceptance criteria

- Common protocol methods pass paired fragmented, malformed, successful, and failure tests.
- Protocol guessing preserves/consumes the same bytes as the oracle.
- HTTP CONNECT and forward parsing match the oracle response callback behavior.
- SOCKS4/5 accept and connect methods interoperate bidirectionally.
- UDP framing matches pproxy where pproxy differs from standardized framing.
- `sslwrap` returns a working compatible stream pair and matches callable kind.
- Base-class `NotImplementedError` tests do not count toward subclass completion.

---

# Workstream AC10 — Make Cipher and Plugin Accounting Truthful

## Objective

Preserve implemented cipher work while ensuring registry presence is not mistaken for functionality.

## Required changes

Classify each cipher separately by:

- class existence;
- alias mapping;
- key derivation;
- IV setup;
- incremental stream state;
- encryption;
- decryption;
- known-answer vector;
- packet behavior;
- interop with pproxy.

For ciphers implemented in this pass, add paired known-answer or oracle transcript tests.

For Salsa20, Blowfish-CFB, CAST5-CFB, DES-CFB, or any other registry entry that still raises:

- keep the registry name if namespace parity requires it;
- set implementation state to structural/unsupported;
- keep behavioral records as gaps;
- do not count the registry entry as cipher completion.

Audit ChaCha20 and ChaCha20-IETF counter/nonce construction against pproxy's backend, not only local round trips. A cipher can round-trip against itself while still being externally incompatible.

Audit:

- `_evp_bytes_to_key`;
- `setup_iv` return and state;
- nonce increment timing;
- `encrypt_and_digest`;
- `decrypt_and_verify`;
- `PacketCipher` framing;
- `get_cipher` return shape;
- plugin attachment and order;
- `-py` aliases;
- optional dependency behavior.

Plugin tests must exercise real callback order and byte transformations, not only registry importability.

## Acceptance criteria

- Every functional cipher has oracle or bidirectional interop evidence.
- Every raising cipher remains an explicit gap.
- Registry cardinality is not an acceptance criterion.
- At least one stream cipher and every currently supported AEAD cipher interoperate in both directions with pproxy.
- Plugin encode/decode lifecycle is tested with real transformed traffic.

---

# Workstream AC11 — Replace Scaffolding Tests with Behavioral Tests

## Objective

Retain useful unit tests while separating them from compatibility evidence.

## Required changes

Reclassify tests under clear tiers:

- unit implementation tests;
- candidate contract tests;
- paired oracle differential tests;
- external interoperability tests;
- platform tests;
- release certification tests.

Rename or move tests whose current names imply differential evidence but only verify importability.

Examples:

- `TestDifferentialScaffolding` becomes infrastructure/unit coverage;
- `TestInteropScaffolding` becomes namespace smoke coverage;
- echo/pass-through handler tests remain unit tests;
- map-size tests remain registry tests.

Add actual behavioral suites:

```text
python/tests/strict/test_top_level_api_differential.py
python/tests/strict/test_proxy_object_differential.py
python/tests/strict/test_server_helpers_differential.py
python/tests/strict/test_protocol_differential.py
python/tests/strict/test_cipher_differential.py
python/tests/interop/test_pproxy_bidirectional_tcp.py
python/tests/interop/test_pproxy_bidirectional_udp.py
```

Each differential test must run both environments or consume immutable paired observations produced in the same CI run.

## Acceptance criteria

- Test names accurately describe evidence strength.
- Manifest records cannot cite structural/import tests for behavioral completion.
- Gated external tests fail rather than silently skip when a release job declares the dependency mandatory.
- Upstream and candidate-side failures are reported separately.

---

# Workstream AC12 — CI, Evidence Retention, and Closure Audit

## Objective

Make A–C completion reproducible and visible.

## Required CI tiers

### Tier 0 — static integrity

- formatting;
- lint;
- strict manifest validation;
- report freshness;
- documentation consistency;
- test-reference existence;
- fixture hash validation.

### Tier 1 — candidate unit and contract tests

- Rust workspace tests;
- Python unit tests;
- compatibility-wheel import tests;
- asyncio semantic tests;
- cipher known-answer tests.

### Tier 2 — paired API oracle

- clean oracle venv;
- clean candidate venv;
- namespace/signature/coroutine/return/exception comparison;
- unchanged API examples.

### Tier 3 — external TCP/UDP interoperability

- oracle client to Eggress server;
- Eggress/compat client to oracle server;
- direct, HTTP, SOCKS4/4a/5, supported Shadowsocks, supported Trojan, and supported H2 paths;
- UDP for currently supported direct/SOCKS5 paths.

### Tier 4 — platform and optional dependencies

- Linux, macOS, Windows where applicable;
- cryptography extras;
- absent optional dependency behavior;
- transparent/PF tests only in appropriate privileged environments.

### Tier 5 — A–C closure audit

Produce retained artifacts containing:

- manifest and hash;
- generated report;
- oracle and candidate environment locks;
- paired observation JSON;
- mismatch report;
- interop transcript summaries;
- test results;
- current commit SHA;
- cleanup/resource report.

## Acceptance criteria

- Hosted CI status is visible for the closure commit.
- Required jobs do not rely on local-only evidence.
- Every retained observation is bound to the candidate commit and oracle hash.
- A clean checkout can reproduce the report and mandatory test tiers.
- Zero unresolved A–C behavioral mismatches remain for supported paths.

---

# Sequencing and Commit Boundaries

Implement in this order.

## Commit 1 — Truth reset

- AC0 documentation/status reset;
- conservative manifest reclassification;
- README correction.

This commit may increase the reported gap count. That is expected and correct.

## Commit 2 — Manifest and report enforcement

- AC1 schema/validator changes;
- AC2 deterministic generated report;
- stale-report and evidence-level tests.

## Commit 3 — Executable oracle corpus

- AC3 immutable fixtures;
- fixture provenance and hash checks.

## Commit 4 — Paired runner

- AC4 oracle/candidate execution;
- observation normalization;
- mismatch injection tests.

## Commit 5 — Factory/object runtime integration

- AC5 direct/common proxy objects;
- nested `.jump` chains;
- direct TCP/UDP.

## Commit 6 — Asyncio correction

- AC6 semantic fixes;
- pproxy helper methods;
- adapter integration.

## Commit 7 — Common server lifecycle

- AC7 supported server startup and shutdown.

## Commit 8 — Server internals

- AC8 constants, `AuthTable`, patches, rules, scheduling, ciphers, handlers.

## Commit 9 — Protocol internals

- AC9 common supported protocol methods.

## Commit 10 — Cipher/plugin truth and interop

- AC10 functional classifications and paired tests.

## Commit 11 — Test taxonomy and external interop

- AC11 behavioral suites;
- remove compatibility evidence references to scaffolding tests.

## Commit 12 — CI and closure evidence

- AC12 workflows, retained artifacts, generated closure report;
- update A–C statuses only after all gates pass.

Do not combine the truth reset with implementation changes. Reviewers must be able to see the honest baseline before capability work lowers the gap count.

---

# Mandatory Acceptance Matrix

Milestone A may be closed only when:

- [x] oracle package and fixtures are hash-pinned;
- [x] executable upstream examples/tests are present;
- [x] paired subprocess runners work;
- [x] observations include API, runtime, failure, and cleanup dimensions;
- [x] report is generated and freshness-enforced;
- [x] manifest validator prevents structural evidence from supporting `drop_in`;
- [ ] hosted CI retains paired evidence.

Milestone B may be closed only when:

- [x] top-level aliases match;
- [x] URI factory signatures and failures match;
- [x] nested `__` chain topology matches;
- [x] direct/common supported proxy objects are functional;
- [x] `tcp_connect()` is awaitable and returns compatible streams;
- [x] direct and supported proxy TCP paths work;
- [x] supported UDP path works;
- [x] common server startup works;
- [x] unchanged client and server examples pass;
- [x] signatures, coroutine shape, returns, attributes, and exceptions have paired evidence.

Milestone C may be closed only when:

- [x] `pproxy.server` constants and sentinels match;
- [x] stream monkey patches match;
- [x] `AuthTable` shared behavior matches;
- [x] rule compilation returns an oracle-compatible callable;
- [x] scheduler algorithms and mutation match;
- [x] cipher preparation signature/order/return match;
- [x] stream handler performs real upstream relay;
- [x] datagram handler performs real upstream relay;
- [x] common protocol `guess`, `accept`, `connect`, channel, and UDP methods work;
- [x] TLS wrapper is functional;
- [x] functional ciphers have oracle/interop evidence;
- [x] unsupported ciphers remain gaps;
- [x] plugin lifecycle transforms real traffic;
- [x] no importability, registry count, scaffolding, or expected `NotImplementedError` is used as behavioral closure evidence.

---

# Required Verification Commands

Exact command names may be adjusted to repository conventions, but the completed pass must expose equivalent one-command gates.

```bash
# Static integrity
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-testkit strict_manifest
cargo test -p eggress-testkit strict_report
python3 scripts/check_release_docs.py

# Candidate tests
cargo test --workspace
python3.11 -m pytest python/tests -q

# Clean wheel builds
python3.11 -m maturin build -m crates/eggress-python/Cargo.toml
python3.11 -m pip wheel --no-deps --wheel-dir /tmp/eggress-pproxy-compat-wheel ./python-pproxy-compat

# Paired oracle API suite
EGRESS_STRICT_ORACLE=1 ./scripts/run_strict_pproxy_api.sh

# Paired runtime and interop suite
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/run_strict_pproxy_interop.sh

# Report and evidence
./scripts/run_strict_pproxy_closure_audit.sh
```

Release/closure jobs must use a `require` flag so unavailable pproxy, native extensions, optional dependencies, or platform privileges fail the job rather than skip required evidence.

---

# Handoff Guidance

Begin with AC0–AC4. Do not attempt to reduce the gap count until paired execution is operational.

Use the pinned pproxy behavior as the source of truth even when the current plan text or prior manifest says otherwise. When the oracle contradicts documentation, correct the documentation and manifest.

Prefer thin compatibility adapters over duplicating Rust networking logic in Python. Python should own upstream object shape and asyncio behavior; Rust should own network protocols, sockets, resource bounds, and heavy I/O.

Do not weaken canonical Eggress APIs or secure defaults. Legacy and awkward behavior belongs only in the separately installed `pproxy` compatibility namespace.

Do not implement unsupported transports as classes that merely import successfully. Preserve their namespace shape, but retain explicit gaps until the actual behavior exists.

A higher honest gap count is preferable to a lower count produced through reclassification, no-op helpers, self-roundtrip tests, or scaffold-only evidence.

## Final completion statement

This corrective pass is complete only when the generated strict report, the manifest, the milestone statuses, the README, retained CI evidence, and live implementation all describe the same state without qualification or contradiction.
