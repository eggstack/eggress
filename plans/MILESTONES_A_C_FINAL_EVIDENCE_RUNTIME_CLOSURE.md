# Milestones A–C Final Evidence and Runtime Closure Pass

## Status

Ready for implementation.

## Parent plans and audit context

- `plans/PPROXY_FULL_DROP_IN_ROADMAP.md`
- `plans/MILESTONE_A_HONEST_CONTRACT.md`
- `plans/MILESTONE_B_PYTHON_SOURCE_COMPATIBILITY.md`
- `plans/MILESTONE_C_FUNCTIONAL_INTERNAL_API.md`
- `plans/MILESTONES_A_C_CORRECTIVE_PASS.md`
- `docs/PHASE_54_MILESTONES_A_C_CORRECTIVE_PASS_COMPLETION.md`

## Purpose

Close the remaining evidence and runtime defects discovered after the first Milestones A–C corrective implementation pass.

The previous pass added substantial and reusable infrastructure:

- a strict manifest and generated report;
- report freshness validation;
- isolated oracle and candidate environments;
- Python API probes;
- protocol, cipher, plugin, process, failure, and cleanup probes;
- proxy object and server compatibility classes;
- nested `.jump` topology;
- direct TCP and UDP operation;
- server handler and protocol parsing implementations;
- test taxonomy and strict workflow definitions.

That work must be preserved. This pass is not a rewrite and must not expand into Milestones D–F.

The remaining problem is that the repository currently claims local closure while the strict report, live implementation, Python tests, and evidence runners still disagree. This pass must make those four sources describe the same state and must prevent a closure document from becoming authoritative when the machine-verifiable gates do not support it.

## Required final outcome

At completion:

1. Milestones A, B, and C have one unambiguous status derived from generated evidence.
2. Every record marked `drop_in` is backed by a comparator that exercises the capability represented by that record.
3. Structural namespace records remain `structural`; they are not counted as unresolved behavioral gaps, but they also do not contribute to behavioral certification readiness.
4. Common supported `pproxy.Connection` objects actually traverse the configured upstream protocol instead of connecting directly to the destination.
5. Common supported protocol client methods perform the pproxy wire handshake.
6. `AuthTable` reproduces pproxy's shared per-client authentication state.
7. `ProxySimple.start_server()` has one implementation contract and the tests exercise that exact contract.
8. Differential Python tests execute separately in the oracle and candidate environments and compare observations.
9. The closure audit runs all mandatory A–C gates and fails if any required gate is skipped, absent, or stale.
10. Hosted CI is configured correctly and is capable of running once the external billing blocker is removed.
11. No implementation or evidence gap is hidden behind a documentation statement, expected `NotImplementedError`, test skip, import-only assertion, or registry count.

## Scope

### In scope

- status and completion-document correction;
- strict manifest semantics and readiness accounting;
- exact behavioral evidence mapping;
- shared `AuthTable` behavior;
- common upstream proxy client operation;
- HTTP CONNECT and forward-proxy client setup;
- SOCKS4/4a client setup;
- SOCKS5 client setup and supported UDP setup;
- supported Shadowsocks and Trojan client preparation where the Rust runtime already implements them;
- `ProxySimple.open_connection`, `tcp_connect`, `prepare_connection`, and UDP routing;
- server lifecycle contract reconciliation;
- paired Python test execution;
- closure audit completeness;
- strict workflow dependency and artifact corrections;
- exact local and hosted closure criteria.

### Out of scope

Do not implement or claim parity for:

- SSH;
- QUIC;
- HTTP/3;
- SSR;
- intentionally unsupported legacy ciphers;
- full reverse-proxy parity;
- transparent/PF/Redir parity outside supported platform evidence;
- every possible multi-hop UDP composition;
- final CLI/process/package parity assigned to Milestones D–F;
- performance optimization unrelated to a demonstrated A–C correctness defect.

Unsupported public symbols must remain visible with the correct structural or intentional-non-parity classification. Unsupported behavior must not be counted as A–C closure.

---

# Closure principles

## P1 — The manifest is descriptive, not aspirational

A record reflects current evidence. It must not be upgraded because implementation code exists or because a test file has a promising name.

## P2 — Evidence must exercise the represented capability

Examples:

- module import proves module existence only;
- method signature proves signature only;
- object construction proves construction only;
- registry membership proves registry membership only;
- local round-trip proves local symmetry, not external cipher compatibility;
- successful listener start proves lifecycle start, not application relay;
- a direct connection test does not prove SOCKS or HTTP upstream routing;
- a deliberate `NotImplementedError` does not prove a callable protocol method.

## P3 — Oracle and candidate execution must be isolated

A paired test must use separate interpreters or subprocesses whose import roots are verified before test execution. Tests using only `sys.executable` are candidate tests unless the test itself launches both environments.

## P4 — Both-fail is not a match

If the oracle probe fails, is missing, times out, or produces malformed evidence, the record is `oracle_error`, not passing. If both sides lack a symbol or fail, the comparison fails unless a pinned known-upstream-defect record explicitly permits the observed oracle failure.

## P5 — Closure scripts must fail closed

Required dependencies, tests, artifacts, examples, and probes may not silently skip in the closure job. Optional fast-development jobs may skip; closure jobs may not.

## P6 — One contract per public path

The implementation, stubs, tests, examples, and documentation must agree on return types and lifecycle operations. Compatibility tests must not accept a native Eggress contract when the pproxy contract is different.

---

# Workstream FC0 — Reopen and Normalize Status

## Objective

Correct the repository status before additional implementation changes.

## Required changes

Update:

- `plans/MILESTONE_A_HONEST_CONTRACT.md`
- `plans/MILESTONE_B_PYTHON_SOURCE_COMPATIBILITY.md`
- `plans/MILESTONE_C_FUNCTIONAL_INTERNAL_API.md`
- `plans/MILESTONES_A_C_CORRECTIVE_PASS.md`
- `docs/PHASE_54_MILESTONES_A_C_CORRECTIVE_PASS_COMPLETION.md`
- `README.md`

Use the status:

> REOPENED — final evidence and runtime closure in progress.

The Phase 54 completion record must be preserved as a historical record but amended at its top with a superseding notice that names this plan and states that post-completion review found additional behavioral and evidence defects.

Do not delete the earlier claimed test counts. Mark them as historical observations whose coverage did not prove full A–C closure.

## Acceptance criteria

- No active document says that hosted CI is the only remaining criterion.
- No active document says 30/31 criteria are met.
- README describes the compatibility layer as experimental and A–C as reopened.
- `scripts/check_release_docs.py` rejects stale 30/31 or locally-closed wording while this plan is open.

---

# Workstream FC1 — Repair Strict Manifest Semantics and Readiness Accounting

## Objective

Make the strict report communicate three distinct dimensions:

1. implementation status;
2. evidence level;
3. behavioral certification readiness.

## Required changes

### 1. Preserve structural records honestly

Module, class, property, signature, constant, and hierarchy records may remain `structural` when their comparator is structural. They must not be listed as behavioral gaps unless the record itself represents required runtime behavior.

### 2. Do not call structural records behaviorally resolved

The report must separately display:

- structural inventory records;
- behaviorally certified records;
- intentional non-parity records;
- platform-constrained records;
- actual unresolved behavior gaps;
- records whose evidence is missing or stale.

### 3. Introduce explicit certification fields

Extend the strict schema where necessary with fields equivalent to:

```toml
certification_scope = "structural" | "behavioral" | "interop" | "process" | "platform"
closure_required = true | false
behavior_record = "<record-id>" # optional link from structural symbol to behavior record
```

A structural symbol can be considered complete for structural inventory without being misrepresented as behaviorally certified.

### 4. Correct milestone ownership

Records must not all report Milestone B. Assign:

- oracle, manifest, comparator, and evidence infrastructure to A;
- top-level object model, stream shape, source-compatible common operation, and server lifecycle to B;
- server internals, protocol internals, cipher internals, and plugin internals to C;
- deferred transports and executable/package work to later milestones where applicable.

### 5. Add behavior records for current closure claims

Add or confirm explicit behavior records for:

- `AuthTable` shared state and expiry;
- `ProxySimple` HTTP upstream TCP routing;
- `ProxySimple` SOCKS4 upstream TCP routing;
- `ProxySimple` SOCKS5 upstream TCP routing;
- supported SOCKS5 UDP routing;
- nested chain execution, not only topology;
- `ProxySimple.start_server` return/lifecycle contract;
- HTTP `connect` wire behavior;
- SOCKS4 `connect` wire behavior;
- SOCKS5 `connect` wire behavior;
- handler relay and cleanup;
- plugin transformed traffic;
- supported cipher external interoperability.

## Validator rules

Add validator rules that reject:

- `drop_in` + structural comparator for a record with `certification_scope = "behavioral"`;
- `closure_required = true` with absent evidence refs;
- missing behavior linkage for a public structural record whose milestone declares operational behavior;
- all records assigned to one milestone when the manifest meta declares multiple milestones;
- a completion document claiming zero behavioral gaps while closure-required records are not passing;
- an evidence ref that does not exist;
- an evidence artifact whose embedded commit or manifest hash does not match the current closure commit.

## Acceptance criteria

- The report no longer presents “two gaps” as the sole measure of A–C completeness.
- Structural inventory and behavioral readiness are reported separately.
- Milestone ownership is correct.
- Every A–C behavioral acceptance criterion maps to at least one closure-required behavior record.
- `strict-report --check` fails on any mismatch between the generated report and manifest.

---

# Workstream FC2 — Make the Paired Runner Strictly Paired

## Objective

Ensure every claimed paired record actually executes in both isolated environments.

## Target files

- `scripts/run_strict_pproxy_api.py`
- `scripts/run_strict_pproxy_api.sh`
- `scripts/compare_observations.py`
- `scripts/strict_api_probe.py`
- `scripts/strict_class_probe.py`
- `scripts/strict_signature_probe.py`
- strict protocol/cipher/process/runtime probe scripts
- `python/tests/strict/`

## Required changes

### Environment verification

Before probing, record and validate:

- interpreter path;
- `sys.prefix`;
- `pproxy.__file__`;
- installed distribution name and version;
- candidate `eggress` distribution version;
- candidate commit SHA where available;
- oracle package hash/provenance hash.

Fail if the oracle imports the compatibility package or the candidate imports upstream pproxy.

### Comparator correctness

Change comparator behavior so:

- oracle failure always fails the record unless explicitly listed as a known upstream defect;
- both-missing fails;
- both-error fails;
- timeout fails;
- malformed JSON fails;
- skipped records make a closure run fail when `closure_required = true`;
- signatures compare parameter kind, names, defaults, positional-only markers, keyword-only markers, varargs, and coroutine status—not names alone;
- return and exception probes compare normalized categories and meaningful payload shape;
- object probes compare observable fields and lifecycle behavior, not only MRO and method names.

### Test architecture

Refactor Python files named `test_*_differential.py` so they either:

1. receive immutable paired observation artifacts generated in the same run; or
2. launch both the oracle and candidate interpreters themselves.

A test using only the current interpreter must be moved to candidate contract or unit coverage.

## Acceptance criteria

- A deliberate oracle import failure causes the paired suite to fail.
- A deliberate candidate import contamination causes the paired suite to fail.
- A both-missing symbol causes failure.
- A single altered default argument causes signature comparison failure.
- The paired summary reports pass, mismatch, oracle error, candidate error, skipped, and untestable counts separately.
- Closure mode exits nonzero unless every closure-required record produces valid paired evidence.

---

# Workstream FC3 — Correct `AuthTable` Shared State

## Objective

Reproduce pproxy 2.7.9 authentication state semantics, including sharing across instances for the same remote client.

## Target files

- `python/eggress/_pproxy_proxy.py`
- `python-pproxy-compat/pproxy/server.py` if the class is defined or wrapped there
- `scripts/strict_server_internals_probe.py`
- Python server-internal tests

## Required behavior

Determine exact oracle behavior through a pinned probe, then implement:

- shared state keyed by remote IP;
- shared timestamp/expiry state;
- authentication visibility across two `AuthTable` objects for one IP;
- isolation between different IPs;
- exact expiry clock and boundary behavior;
- overwrite behavior;
- missing-key behavior;
- cleanup behavior;
- class-level state reset hook for tests without changing public behavior.

Avoid process-global unbounded growth. Match oracle semantics first, then add bounded cleanup only if it is behaviorally invisible and documented.

## Required paired cases

1. Instance A sets a user; instance B with same IP observes it.
2. Instance C with another IP does not observe it.
3. Expiry through one instance is reflected in another.
4. Reauthentication replaces prior user exactly as oracle does.
5. Zero and negative authentication windows match oracle behavior.
6. Missing IP or special local values match oracle behavior.

## Acceptance criteria

- Shared-state tests fail against the pre-fix implementation and pass after the fix.
- The behavior record is `drop_in` only after paired execution.
- Candidate-only basic authentication tests are not cited as shared-state evidence.

---

# Workstream FC4 — Implement Real `ProxySimple` Upstream Connections

## Objective

Ensure a compatibility connection created from a proxy URI routes through that proxy.

## Current defect to eliminate

`ProxySimple` inherits `ProxyDirect.tcp_connect()` and therefore may open a direct connection to the final destination instead of connecting to and handshaking with the configured upstream proxy.

## Target files

- `python/eggress/_pproxy_proxy.py`
- `python/eggress/outbound.py`
- `python/eggress/pproxy_connection.py`
- PyO3 outbound connector bindings if a thin native path is required
- compatibility factory code
- proxy-object tests and interop tests

## Required architecture

### Preferred path

Delegate supported upstream connection establishment to the Rust `OutboundConnector` or an equivalent native primitive. Python should adapt the returned native stream into pproxy-compatible asyncio streams.

### Required method behavior

Implement or override on `ProxySimple`:

- `destination()`;
- `open_connection()`;
- `tcp_connect()`;
- `prepare_connection()`;
- `udp_prepare_connection()`;
- `udp_open_connection()`;
- `udp_sendto()` where supported;
- connection accounting;
- timeout behavior;
- local bind behavior;
- TLS/plugin/cipher preparation ordering.

### Chain traversal

For nested `.jump` chains:

1. connect to the terminal upstream through its own jump;
2. prepare each protocol hop in the same order as pproxy;
3. preserve host and port semantics at each hop;
4. apply TLS, cipher, and plugin wrappers at the correct layer;
5. return the final pproxy-compatible `(reader, writer)` pair;
6. unwind and close all opened resources on failure.

Do not reconstruct a chain from string representations after the object graph has been built. Use the nested object graph as the source of truth.

## Required live tests

For each supported upstream client protocol:

- destination server records the connection source;
- a proxy server records that it received the handshake;
- the destination receives payload only through the proxy;
- the test fails if the candidate directly connects to the destination;
- oracle and candidate perform the same high-level outcome.

Required minimum:

- HTTP CONNECT;
- SOCKS4;
- SOCKS4a;
- SOCKS5 IPv4;
- SOCKS5 domain;
- direct terminal;
- two-hop chain containing two common supported proxies.

Include current Shadowsocks AEAD and Trojan when the existing Rust runtime exposes a stable outbound primitive suitable for the compatibility path.

## Acceptance criteria

- `Connection("socks5://proxy:port").tcp_connect(target, port)` produces a SOCKS5 handshake at the proxy.
- The same is true for HTTP and SOCKS4.
- A test that blocks direct destination access still succeeds through the proxy.
- A test that disables the proxy fails rather than silently connecting directly.
- Nested chain execution is demonstrated, not just `.jump` topology.
- Failure closes all intermediate streams and restores connection counts.

---

# Workstream FC5 — Complete Common Protocol Client Methods

## Objective

Make the common `pproxy.proto` client methods functional and oracle-compatible.

## Target protocols

- `Direct` where the oracle exposes callable behavior;
- `HTTP`;
- `HTTPOnly` where applicable;
- `Socks4`;
- `Socks5`;
- supported `SS`;
- supported `Trojan`;
- common raw/tunnel/WebSocket helpers already supported by the runtime.

## Required methods

Audit and implement, as applicable:

- `connect()`;
- `guess()`;
- `accept()`;
- `channel()`;
- `http_channel()`;
- `udp_accept()`;
- `udp_connect()`;
- `udp_pack()`;
- `udp_unpack()`;
- response callback behavior;
- authentication exchange;
- rollback and fragmented-read behavior.

## HTTP requirements

- emit exact CONNECT request shape;
- support authenticated proxy headers where the oracle does;
- parse fragmented success and failure responses;
- preserve post-header bytes;
- implement plain HTTP forwarding behavior and header rewriting where upstream does;
- make `http_channel()` semantically different from raw `channel()` when required;
- compare failure categories for malformed status lines and non-2xx responses.

## SOCKS4/4a requirements

- exact request bytes;
- user ID semantics;
- IPv4 versus domain/SOCKS4a selection;
- response parsing and status mapping;
- fragmented response reads;
- unsupported command behavior.

## SOCKS5 requirements

- method negotiation;
- no-auth and username/password auth;
- IPv4, IPv6, and domain address encoding;
- CONNECT response parsing;
- exact failure-category mapping;
- UDP ASSOCIATE framing and association lifecycle for currently supported use;
- pproxy-specific UDP differences captured by oracle probes.

## Test correction

Remove tests that treat expected `NotImplementedError` from a supported protocol subclass as completion evidence. Retain them only for explicitly unsupported base or deferred classes.

## Acceptance criteria

- Supported HTTP, SOCKS4, and SOCKS5 `connect()` methods no longer inherit or raise the base `NotImplementedError`.
- Byte-level paired probes pass for success, fragmented input, malformed input, auth failure, and remote rejection.
- Candidate tests and manifest records point to the same protocol behavior suites.
- No supported protocol remains described as “construction-only.”

---

# Workstream FC6 — Reconcile Server Lifecycle Contracts

## Objective

Make implementation, tests, type stubs, examples, and documentation agree on the pproxy-facing server contract.

## Current contradiction to remove

The current implementation path and committed tests disagree about whether `ProxySimple.start_server()` returns an `asyncio.Server` or an Eggress-native server object with `.addresses`, `.status()`, and `.aclose()`.

## Required process

### 1. Probe the oracle

Record:

- exact return type category;
- relevant public attributes;
- `close()` availability and call kind;
- `wait_closed()` availability and call kind;
- sockets exposure;
- repeated-close behavior;
- cancellation behavior;
- bind-failure behavior;
- whether the supplied `stream_handler` is used;
- how `args` are consumed.

### 2. Implement the pproxy-facing contract

The compatibility layer may wrap an Eggress-native server internally, but the object returned to pproxy callers must reproduce the oracle's observable contract.

Create a dedicated compatibility server-handle adapter if necessary. Do not expose native-only `.addresses`, `.status()`, or `.aclose()` as the required compatibility API unless the oracle exposes equivalent behavior.

### 3. Correct tests

Tests must import the separately installed compatibility package and exercise:

- start;
- real accept and relay;
- socket discovery;
- close;
- wait closed;
- repeated close;
- bind failure;
- custom handler behavior;
- cancellation cleanup.

Do not branch around the compatibility package by importing Eggress classes directly when real pproxy happens to be installed. Oracle and candidate test processes must be explicit.

## Acceptance criteria

- The return object and lifecycle methods match the oracle.
- Type stubs match runtime.
- Examples use only pproxy-compatible methods.
- The entire final Python test suite passes in a clean candidate environment.
- A test designed for the prior contradictory contract is removed or rewritten.

---

# Workstream FC7 — Repair Handler, Protocol, and Stream Semantics Where Evidence Is Too Weak

## Objective

Close correctness gaps hidden by permissive tests.

## Required reviews

### `BaseProtocol.channel`

Verify against oracle:

- whether `stat_bytes is None` should discard or still relay data;
- connection counter timing;
- drain behavior;
- close versus wait-closed behavior;
- exception propagation;
- half-close behavior;
- cancellation behavior.

A relay must not silently drop all data merely because no byte-stat callback is provided.

### `http_channel`

Implement and test upstream-required HTTP transformations rather than delegating unconditionally to raw channel.

### Protocol detection

Verify `guess()` and `accept()` with:

- fragmented handshake bytes;
- rollback semantics;
- multiple protocols in the detection list;
- early EOF;
- oversized headers;
- bytes after the parsed handshake.

### Stream adapter

Re-run the adapter contract against actual `asyncio.StreamReader` and `StreamWriter` behavior for:

- `read(-1)`;
- `read(0)`;
- async iteration;
- `write_eof()` call kind;
- `drain()`;
- `is_closing()`;
- `wait_closed()`;
- `read_w`, `read_n`, `read_until`, and `rollback`;
- cancellation and EOF races.

## Acceptance criteria

- Raw channels relay bytes when stats are absent.
- HTTP channel behavior has dedicated oracle evidence.
- Fragmented detection and rollback tests pass.
- No lifecycle method differs in coroutine/synchronous shape from the oracle.

---

# Workstream FC8 — Replace Derived-Fixture Claims with Reproducible Fixture Provenance

## Objective

Make the oracle corpus reproducible and accurately described.

## Required changes

The existing examples are derived canonical usage fixtures, not verbatim upstream examples. Rename documentation and evidence classifications accordingly.

Maintain two fixture classes:

### A. Pinned upstream artifacts

Where licensing and packaging permit, retain exact files extracted from the pinned `pproxy==2.7.9` source distribution or wheel, with:

- source archive hash;
- member path;
- extracted file hash;
- upstream license;
- no local edits.

### B. Project-authored behavioral scenarios

Keep derived scenarios, but identify them as Eggress-authored tests based on upstream public behavior. Each scenario must run unchanged in both environments.

The closure criteria must not say “unchanged upstream example” when the file is project-authored.

## Acceptance criteria

- Fixture manifest distinguishes exact upstream artifacts from authored scenarios.
- Hash validation runs in Tier 0.
- Every authored scenario states what oracle behavior it measures.
- Documentation uses accurate provenance language.

---

# Workstream FC9 — Make the Closure Audit Comprehensive and Fail-Closed

## Objective

Turn `run_strict_pproxy_closure_audit.sh` into the authoritative A–C closure gate.

## Required audit steps

The closure audit must execute and retain results for:

1. `cargo fmt --all -- --check`;
2. `cargo check --workspace --all-targets`;
3. `cargo clippy --workspace --all-targets -- -D warnings`;
4. `cargo test --workspace`;
5. `cargo deny check`;
6. `cargo audit`;
7. strict manifest validator tests;
8. strict report freshness;
9. release/document consistency checks;
10. clean canonical wheel build;
11. clean compatibility wheel build;
12. clean candidate Python test suite;
13. paired API runner in isolated environments;
14. all closure-required strict Python differential tests;
15. required runtime examples/scenarios;
16. external TCP interoperability;
17. external UDP interoperability for supported paths;
18. cipher KAT and interop probes;
19. plugin transformed-traffic probe;
20. process lifecycle probe;
21. runtime/failure/cleanup probe;
22. resource-leak and process-cleanup checks;
23. report and evidence hash binding.

## Fail-closed requirements

- Do not append `FAIL` and continue to a potentially passing summary unless all failures are accumulated and the final exit remains nonzero.
- Do not use `|| true` for required report or evidence generation.
- Required artifacts must use `if-no-files-found: error` in hosted closure jobs.
- Missing venvs, dependencies, optional modules declared mandatory for the closure matrix, or test executables must fail.
- Skipped closure-required tests fail the audit.
- The summary must report exact commands, exit codes, elapsed time, and artifact locations.

## Required retained artifacts

- manifest and SHA-256;
- generated strict report;
- candidate commit SHA;
- oracle package hash and version;
- environment locks;
- paired observations;
- mismatch report;
- pytest JUnit XML;
- cargo test summary;
- interop transcripts;
- process and cleanup summary;
- fixture validation report;
- final acceptance matrix generated from manifest/evidence rather than manually checked boxes.

## Acceptance criteria

- Removing the candidate wheel causes audit failure.
- Breaking one Python compatibility test causes audit failure.
- Deleting paired observations causes audit failure.
- A skipped closure-required interop test causes audit failure.
- The audit cannot report PASS without running the paired runner and full Python suite.

---

# Workstream FC10 — Correct Hosted Workflow Execution

## Objective

Ensure the strict workflow is technically runnable when the external account blocker is removed.

## Target files

- `.github/workflows/strict-differential.yml`
- relevant Python and compatibility workflows
- `python/pyproject.toml`
- optional dedicated test requirements lock

## Required changes

### Install explicit test dependencies

The strict candidate environment must explicitly install:

- `pytest`;
- `pytest-timeout` if `--timeout` is used;
- any cryptography extras needed by required cipher tests;
- any package required by the authored fixture scenarios;
- the canonical Eggress wheel;
- the compatibility wheel.

Do not rely on development-environment or transitive dependencies.

### Use platform-correct venv paths

If the workflow is expanded beyond Linux, use platform-aware invocation or shell matrices. Do not hard-code `bin/python` for Windows.

### Run the authoritative audit

The final strict job should invoke the authoritative closure audit or the same commands through reusable scripts. Avoid maintaining a weaker hosted sequence than the local closure gate.

### Artifacts

Upload evidence with:

- `if: always()`;
- `if-no-files-found: error` for mandatory evidence;
- retention long enough for release review;
- names containing commit SHA and oracle version.

### Hosted blocker handling

Keep the billing blocker documented, but do not mark A complete until a hosted run executes and retains the required artifacts. A future successful run should be able to close the last infrastructure criterion without code changes.

## Acceptance criteria

- Workflow lint passes.
- A local container or equivalent clean Linux simulation executes the workflow command sequence.
- The workflow installs every executable it invokes.
- Mandatory artifact upload fails if evidence is absent.
- A successful hosted run provides a visible commit status and retained evidence.

---

# Workstream FC11 — Resolve the Remaining Declared Manifest Gaps Assigned to A–C

## Objective

Address only the two current explicit gap records if they are truly within A–C scope.

## `cli.get`

Determine whether this is part of Milestone B source/executable compatibility or a later CLI milestone.

- If A–C requires it, implement exact `--get URL` behavior and paired process/output evidence.
- If it belongs to Milestone E or later CLI parity, correct milestone ownership and leave it as a later gap without blocking A–C.

Do not replace pproxy's behavior with a structured Eggress-native diagnostic while claiming drop-in status.

## `process.reload.routing`

Determine whether exact observable reload behavior belongs to C or a later process milestone.

- If A–C requires it, add paired process lifecycle evidence proving routing change without restart.
- If assigned later, correct milestone ownership and leave the gap open for that milestone.

## Acceptance criteria

- Both records have correct milestone ownership.
- Neither is hidden or upgraded without paired evidence.
- A–C closure calculations include only records actually owned by A–C.

---

# Test and evidence matrix

## Milestone A

Required evidence:

- oracle source/wheel provenance and hashes;
- fixture class and hash validation;
- strict manifest validation;
- strict report generation and freshness;
- paired runner contamination tests;
- oracle-error and both-missing failure tests;
- evidence-to-record existence and hash checks;
- hosted artifact retention.

## Milestone B

Required behavior:

- exact top-level aliases;
- URI factory and error behavior;
- object attributes;
- nested chain topology;
- direct TCP/UDP;
- HTTP upstream TCP;
- SOCKS4/4a upstream TCP;
- SOCKS5 upstream TCP;
- supported SOCKS5 UDP;
- at least one executed two-hop chain;
- compatible asyncio stream behavior;
- exact server handle/lifecycle behavior;
- authored client and server scenarios in both environments.

## Milestone C

Required behavior:

- constants and sentinels;
- stream patches;
- shared `AuthTable` state;
- rules;
- scheduler algorithms;
- cipher preparation ordering and return shape;
- actual upstream stream relay;
- actual upstream datagram relay;
- common HTTP/SOCKS protocol accept and connect methods;
- channel and HTTP channel semantics;
- TLS wrapper;
- supported cipher KAT and interop;
- explicit unsupported cipher behavior;
- plugin transformed traffic;
- cleanup and resource behavior.

---

# Required implementation sequence

## Commit 1 — Truth reset

- FC0 status changes;
- superseding notice on the Phase 54 completion record;
- README qualification;
- document consistency checks.

## Commit 2 — Manifest and report model

- FC1 schema fields;
- milestone ownership;
- behavior records;
- report sections;
- validator rules;
- generated report.

## Commit 3 — Paired-runner hardening

- FC2 environment validation;
- error semantics;
- strict signature comparison;
- contamination and mismatch tests.

## Commit 4 — `AuthTable`

- FC3 shared state implementation;
- paired shared-state tests;
- evidence mapping.

## Commit 5 — Common upstream client runtime

- FC4 `ProxySimple` connection overrides;
- Rust connector delegation;
- live HTTP/SOCKS route-through tests;
- failure cleanup.

## Commit 6 — Protocol client handshakes

- FC5 HTTP/SOCKS4/SOCKS5 client methods;
- fragmented and malformed tests;
- supported UDP behavior.

## Commit 7 — Server lifecycle reconciliation

- FC6 oracle probe;
- compatibility handle adapter if required;
- test/stub/example alignment.

## Commit 8 — Handler and stream semantic hardening

- FC7 channel, HTTP channel, rollback, fragmentation, cancellation, and half-close corrections.

## Commit 9 — Fixture provenance correction

- FC8 exact versus authored fixture distinction;
- hashes and validator integration.

## Commit 10 — Full closure audit

- FC9 fail-closed audit;
- JUnit/evidence retention;
- generated acceptance matrix.

## Commit 11 — Hosted workflow repair

- FC10 explicit dependencies;
- authoritative gate invocation;
- mandatory artifact behavior.

## Commit 12 — Gap ownership and final local closure

- FC11 ownership or implementation;
- regenerate manifest/report;
- execute complete closure audit;
- update status to `LOCAL GATES PASS — HOSTED EVIDENCE PENDING` only if every local gate passes.

## Commit 13 — Hosted evidence closure

After the external billing blocker is removed:

- execute strict workflow on the exact candidate commit;
- retain evidence;
- confirm visible commit status;
- update A–C plans to complete only if hosted results match local evidence;
- create a new completion record bound to the successful workflow run and artifact identifiers.

Do not combine Commit 1 with implementation work. Reviewers must see the honest reopened baseline.

---

# Mandatory acceptance checklist

## Milestone A closure

- [ ] Exact oracle artifact is pinned and hash-validated.
- [ ] Fixture provenance distinguishes upstream artifacts from authored scenarios.
- [ ] Every closure-required record has an executable comparator.
- [ ] Paired execution verifies import roots and distribution identities.
- [ ] Both-missing, both-error, oracle-error, timeout, and malformed output fail.
- [ ] Structural and behavioral readiness are separately reported.
- [ ] Milestone ownership is accurate.
- [ ] Report freshness is enforced.
- [ ] Evidence is bound to manifest hash and candidate commit.
- [ ] Hosted CI retains mandatory evidence.

## Milestone B closure

- [ ] Top-level aliases and signatures match.
- [ ] URI factories and exceptions match.
- [ ] Nested `.jump` topology matches.
- [ ] Direct TCP and UDP work.
- [ ] HTTP upstream TCP traverses the proxy.
- [ ] SOCKS4/4a upstream TCP traverses the proxy.
- [ ] SOCKS5 upstream TCP traverses the proxy.
- [ ] Supported SOCKS5 UDP works through the proxy.
- [ ] At least one two-hop chain executes end to end.
- [ ] `tcp_connect()` returns compatible asyncio streams.
- [ ] Proxy failure never falls back to a direct destination connection.
- [ ] Server start return shape and lifecycle match the oracle.
- [ ] Full clean candidate Python suite passes.
- [ ] Authored client and server scenarios pass in both environments.

## Milestone C closure

- [ ] Server constants and sentinels match.
- [ ] Stream monkey patches match.
- [ ] `AuthTable` state is shared across same-IP instances.
- [ ] Rule compilation matches.
- [ ] Scheduler behavior and mutation match.
- [ ] Cipher preparation order and return shape match.
- [ ] Stream handler performs real upstream relay.
- [ ] Datagram handler performs real upstream relay.
- [ ] HTTP client `connect()` works.
- [ ] SOCKS4/4a client `connect()` works.
- [ ] SOCKS5 client `connect()` works.
- [ ] Common protocol accept/guess behavior handles fragmentation and rollback.
- [ ] Raw channel relays without requiring a statistics callback.
- [ ] HTTP channel performs required transformations.
- [ ] TLS wrapper matches the oracle contract.
- [ ] Supported cipher KAT and interop pass.
- [ ] Unsupported ciphers remain explicit nonfunctional records.
- [ ] Plugin lifecycle transforms real traffic.
- [ ] Cleanup and connection accounting pass under failure and cancellation.
- [ ] No supported behavior is closed by expected `NotImplementedError`.

## Cross-cutting closure

- [ ] Closure audit executes all required Rust and Python gates.
- [ ] Closure audit executes paired API and interop suites.
- [ ] Closure audit fails on skipped mandatory tests.
- [ ] Strict hosted workflow installs every dependency it invokes.
- [ ] Mandatory evidence artifact upload fails when files are absent.
- [ ] README, plans, manifest, report, implementation, tests, and completion record agree.

---

# Required verification commands

Commands may be wrapped by repository scripts, but the final closure must execute equivalent gates.

```bash
# Rust static and unit gates
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo audit

# Strict integrity
cargo test -p eggress-testkit strict_manifest
cargo test -p eggress-testkit strict_report
cargo run -p eggress-testkit --bin strict-report -- --check
python3 scripts/check_release_docs.py

# Clean candidate environments
rm -rf .venv-oracle-api .venv-candidate-api
./scripts/run_strict_pproxy_api.sh

# Candidate Python suite
.venv-candidate-api/bin/python -m pytest python/tests -q --strict-markers

# Paired A-C API and behavior
.venv-candidate-api/bin/python scripts/run_strict_pproxy_api.py \
  --oracle-venv .venv-oracle-api \
  --candidate-venv .venv-candidate-api \
  --output-dir target/strict/paired_observations

EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 \
  .venv-candidate-api/bin/python -m pytest python/tests/strict -q --strict-markers

# External supported-path interop
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/run_strict_pproxy_interop.sh

# Authoritative closure gate
EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 \
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
./scripts/run_strict_pproxy_closure_audit.sh
```

The authoritative closure script must include the full Python suite and all required probes rather than relying on the operator to run them separately.

---

# Regression injections required before closure

The implementation agent must demonstrate that the gates detect at least these deliberate failures:

1. change `AuthTable` back to per-instance state;
2. make `ProxySimple.tcp_connect()` call `asyncio.open_connection(target)` directly;
3. remove HTTP `connect()`;
4. change a public default argument;
5. point candidate venv at upstream pproxy;
6. make both oracle and candidate probes return missing;
7. delete paired observation artifacts;
8. skip a mandatory interop test;
9. remove `pytest-timeout` while retaining `--timeout`;
10. make `BaseProtocol.channel()` drop bytes when stats callback is absent.

Each injection must cause the corresponding gate to fail. The injected changes must not be committed.

---

# Risks and controls

## Risk: Reimplementing protocol code twice

Prefer thin Python adapters over existing Rust runtime primitives. Add Python wire logic only where the compatibility API requires direct protocol-object behavior and no suitable native primitive exists.

## Risk: Overfitting to authored scenarios

Use multiple evidence forms: exact source inspection where permissible, paired API probes, malformed/fragmented cases, and bidirectional external interop.

## Risk: Treating implementation success as oracle parity

Require oracle and candidate observations for every closure-required record. Candidate unit tests remain necessary but insufficient.

## Risk: CI blocker remains unresolved

Local closure may be recorded as `LOCAL GATES PASS — HOSTED EVIDENCE PENDING`, but Milestone A must remain open until hosted evidence is retained. Do not redefine the acceptance criterion away.

## Risk: Expanding into Milestone D

When an unsupported transport is encountered, preserve its explicit later-milestone status. Do not implement SSH, QUIC/H3, or SSR in this pass.

---

# Handoff guidance

Start with FC0–FC2. Do not alter runtime code until the manifest and paired runner can accurately expose current failures.

Then implement FC3–FC7 in dependency order. The highest-priority runtime defect is the direct-bypass behavior of `ProxySimple`; closure of Milestone B is impossible until live tests prove that HTTP and SOCKS proxy URIs actually traverse their configured upstreams.

Run FC9 before updating any completion status. The completion record must be generated from the final audit artifacts and must not contain manually asserted checkboxes unsupported by evidence.

The final reviewer should be able to answer all of the following from retained artifacts alone:

- Which exact pproxy artifact was the oracle?
- Which exact Eggress commit was tested?
- Which interpreter imported which package?
- Which structural records exist?
- Which supported behaviors passed paired comparison?
- Which behaviors remain deferred or intentionally unsupported?
- Did HTTP/SOCKS client calls route through the configured proxies?
- Did the full Python compatibility suite pass?
- Did external TCP/UDP interop pass?
- Did resource cleanup pass?
- Did hosted CI retain the evidence?

If any answer is unavailable, A–C are not closed.