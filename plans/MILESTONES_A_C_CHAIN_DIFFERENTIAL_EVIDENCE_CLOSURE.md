# Milestones A–C Chain, Differential, and Evidence Closure Pass

## Status

Ready for implementation.

## Baseline

- Repository: `eggstack/eggress`
- Baseline branch: `main`
- Baseline commit: `042b6e86d832b0538669b5dc0653b081988f74ca`
- Oracle: `pproxy==2.7.9`
- Active parent plan: `plans/MILESTONES_A_C_POST_IMPLEMENTATION_CORRECTIVE_CLOSURE.md`

This plan is a narrow follow-up to the post-implementation corrective closure plan. It does not replace the roadmap or earlier plans. It defines the remaining work after the implementation through baseline commit `042b6e86d832b0538669b5dc0653b081988f74ca`.

Milestones A, B, and C remain reopened until the completion rules in this document are satisfied.

---

# 1. Purpose

Close the remaining runtime and certification defects preventing an honest Milestones A–C completion claim.

The latest pass materially improved the repository:

- a single-hop `ProxySimple.tcp_connect()` now opens the configured proxy endpoint rather than directly opening the final destination;
- route-through test fixtures were added;
- `http_channel()` gained an initial absolute-form rewrite;
- manifest closure metadata was expanded;
- formerly optional interop gates were moved into the required closure script;
- the checked-in report was regenerated;
- strict-test import packaging was repaired.

The line of work is still blocked because:

1. two-hop chain ordering has not been frozen against the oracle and the current recursive `prepare_connection()` flow may execute the object graph in the wrong order;
2. the authoritative closure audit reports one failing required gate: strict Python differential observations are incomplete;
3. `socks_address` remains skipped because the public contract still does not match the oracle;
4. the live SOCKS5 listener path remains unproven and the earlier supported-path skip has not been removed;
5. the paired API comparator still accepts both-missing and identical-error observations and still contains a broad variadic-signature compatibility heuristic;
6. specialized behavioral records are not produced and aggregated consistently, especially cipher, protocol-wire, process, and interop records;
7. `inventory_only` was applied broadly without a record-by-record proof that no behavioral certification is required;
8. the closure audit creates incomplete evidence binding and still contains stale-environment and system-Python ambiguity;
9. the checked-in report is not evidence for the current head and the runtime evidence bundle is not yet authoritative;
10. no hosted workflow result or artifact bundle exists for the current head.

This pass must correct those defects without expanding into Milestones D–F.

---

# 2. Final required outcome

At completion, every statement below must be true.

1. The exact object topology and execution order produced by `pproxy.Connection()` and `pproxy.server.proxies_by_uri()` for a two-hop URI is captured from `pproxy==2.7.9`.
2. Candidate single-hop HTTP, SOCKS4/4a, and SOCKS5 connections reach the configured proxy endpoint and never fall back to direct destination access.
3. Candidate two-hop execution produces the same ordered proxy events as the oracle.
4. Proxy failure, handshake rejection, timeout, cancellation, or intermediate-hop failure closes all opened resources and never attempts the final destination directly.
5. `socks_address` matches the oracle in signature, call kind, reader contract, return shape, trailing-byte behavior, and exception behavior.
6. A real compatibility SOCKS5 listener accepts a client, negotiates a method, parses CONNECT, returns a reply, and relays payload.
7. Supported protocol parsers handle fragmented input and preserve post-handshake payload.
8. `http_channel()` implements all oracle-required request transformations covered by A–C.
9. The paired comparator fails closed for missing, errored, timed-out, malformed, skipped, or stale evidence.
10. Every closure-required manifest record has either direct evidence or a validated delegated evidence artifact.
11. The strict differential suite receives complete observations for every test it executes.
12. The authoritative closure audit passes with zero required failures, zero required skips, zero missing artifacts, and zero stale artifacts.
13. Evidence is bound to the exact candidate commit, manifest hash, wheel hashes, oracle artifact hash, runner hashes, interpreter, and platform.
14. A hosted workflow executes the same authoritative audit and retains the evidence bundle.
15. Only after local and hosted evidence pass may Milestones A–C be marked complete.

---

# 3. Scope

## In scope

- oracle probing of proxy-chain topology and execution order;
- `ProxySimple.tcp_connect()`, `prepare_connection()`, `destination()`, and `.jump` semantics;
- deterministic single-hop and two-hop route-through tests;
- direct-fallback sentinel tests;
- exact `socks_address` compatibility;
- real HTTP and SOCKS5 listener callback invocation;
- HTTP, SOCKS4/4a, and SOCKS5 fragmentation, replies, rollback, and payload preservation;
- `BaseProtocol.channel()` and `http_channel()` behavior;
- strict paired API comparison rules;
- specialized observation producers and delegated evidence aggregation;
- manifest `behavior_record` and `inventory_only` review;
- authoritative closure audit behavior;
- evidence index and hash binding;
- GitHub Actions execution and artifact retention;
- regression-injection proof for the defects in this plan.

## Out of scope

Do not implement or claim parity for:

- SSH;
- QUIC;
- HTTP/3;
- SSR;
- unsupported legacy Shadowsocks ciphers;
- new transparent-proxy platform support;
- later CLI/process/package parity assigned to D–F;
- unrelated performance refactors;
- broad Rust architecture rewrites not required for these defects.

Later-milestone gaps remain visible and must not be converted into A–C requirements.

---

# 4. Non-negotiable rules

1. **Probe before deciding chain order.** Do not infer two-hop semantics from comments, object names, or the previous plan. Capture the oracle’s object topology and wire event sequence first.
2. **No direct fallback.** A configured proxy failure is a connection failure.
3. **No routing through string conversion.** `str(proxy)`, `repr(proxy)`, and `_build_remote_uri()` are not runtime execution formats.
4. **No closure by skip or xfail.** Supported A–C tests must execute.
5. **No closure by mutual absence.** Both-missing and both-error are failures unless a record-specific pinned known-upstream-defect policy applies.
6. **No closure by importability.** Structural evidence does not certify runtime behavior.
7. **No broad signature heuristics.** Generic `*args, **kwargs` does not automatically match an explicit signature.
8. **No stale virtual environments.** Authoritative runs recreate all environments.
9. **No system-Python ambiguity.** Every Python gate names the interpreter it uses.
10. **No unverified `inventory_only`.** Public callable or stateful surfaces require a documented justification or a behavior link.
11. **No incomplete evidence success.** Missing delegated artifacts fail closure.
12. **No status promotion before evidence.** Documentation follows machine-readable evidence, not the reverse.

---

# 5. Current defects that must become regression tests

| Defect | Current condition | Required regression |
|---|---|---|
| Two-hop ordering uncertain | `tcp_connect()` opens `self` directly, then `prepare_connection()` calls `self._jump.destination()` and recursively invokes `self._jump.prepare_connection()` | oracle and candidate event traces must match exactly |
| `_build_remote_uri()` remains string-oriented | method can still stringify nested objects | static and runtime tests prove it is never used for routing |
| Address helper skipped | `TestAddressRoundTrip` is class-level skipped | remove skip and pass exact oracle cases |
| Listener behavior shallow | lifecycle tests primarily prove bind/close | complete SOCKS5 negotiation, CONNECT, and payload relay |
| Both-missing passes | paired comparator returns `all_match = true` for clean mutual absence | injected mutual absence must fail closure |
| Identical errors pass | paired comparator treats equal error strings as compatible | equal errors fail unless record-specific pinned policy applies |
| Variadic signature passes | one variadic side and one explicit side returns compatible | injected variadic wrapper mismatch must fail |
| Gate 14 incomplete | per-class observations expected by strict tests are not produced | every strict node receives an observation or validated delegated artifact |
| Broad inventory-only classification | many structural records were marked inventory-only | each record is justified or linked to behavior |
| Evidence binding incomplete | gate writes some hashes but does not validate a complete evidence index | stale or wrong-commit artifact fails closure |
| Hosted evidence absent | no workflow status or retained bundle for head | successful hosted authoritative audit is required |

---

# Workstream CE0 — Freeze Truthful Status and Defect Inventory

## Objective

Keep status and handoff context accurate while implementation proceeds.

## Target files

- `README.md`
- `AGENTS.md`
- `plans/MILESTONE_A_HONEST_CONTRACT.md`
- `plans/MILESTONE_B_PYTHON_SOURCE_COMPATIBILITY.md`
- `plans/MILESTONE_C_FUNCTIONAL_INTERNAL_API.md`
- `plans/MILESTONES_A_C_POST_IMPLEMENTATION_CORRECTIVE_CLOSURE.md`
- `docs/parity/PPROXY_A_C_SKIP_INVENTORY.toml`
- any current A–C completion record

## Required changes

1. Keep status as:

   `REOPENED — chain, differential, and evidence closure in progress`

2. Record baseline commit `042b6e86d832b0538669b5dc0653b081988f74ca`.
3. Record the acknowledged local audit result as 22/23, not complete.
4. List every supported A–C skip by full pytest node ID.
5. List the strict differential observation mismatch as a blocking harness defect.
6. Do not edit historical completion records into correctness; mark them superseded.

## Acceptance criteria

- No document claims A–C local closure.
- The address-helper and live-listener supported-path skips are blocking entries.
- The strict differential failure is documented as a required-gate failure.
- Documentation consistency checks reject contradictory completion claims.

---

# Workstream CE1 — Freeze Oracle Chain Topology and Execution Order

## Objective

Determine the exact pproxy chain model before modifying candidate chain logic.

## Target files

- new `scripts/probe_pproxy_chain_topology.py`
- new `python/tests/scenarios/chain_topology_scenario.py`
- new `compat/pproxy-2.7.9/observations/chain_topology.json`
- `python/eggress/_pproxy_proxy.py`
- `python-pproxy-compat/pproxy/server.py`

## Required oracle observations

For each URI below, record the object type, endpoint, `.jump` type, `.jump` endpoint, protocol list, `destination()` output, and method invocation order.

```text
http://127.0.0.1:18080
socks5://127.0.0.1:11080
socks5://127.0.0.1:11080__http://127.0.0.1:18080
http://127.0.0.1:18080__socks5://127.0.0.1:11080
```

The probe must not use `repr()` as its only observation. Inspect explicit attributes and instrument method calls.

## Wire event probe

Use two scripted proxy servers and an unreachable symbolic destination. Record events such as:

```json
[
  {
    "sequence": 1,
    "proxy_id": "http-b",
    "event": "connect_request",
    "host": "127.0.0.1",
    "port": 11080
  },
  {
    "sequence": 2,
    "proxy_id": "socks5-a",
    "event": "connect_request",
    "host": "chain-target.invalid",
    "port": 443
  }
]
```

This example is illustrative. The oracle decides the required order.

## Probe requirements

- run in a clean oracle venv containing only `pproxy==2.7.9` and test dependencies;
- record `pproxy.__file__`, distribution version, interpreter, and oracle artifact hash;
- normalize ephemeral ports separately from logical endpoint identities;
- fail if either proxy fixture was not contacted;
- fail if the final destination was contacted directly;
- retain raw bytes and normalized logical events.

## Acceptance criteria

- Oracle topology observations exist for both chain orientations.
- Oracle wire events prove the actual hop order.
- Candidate implementation decisions cite the frozen observation artifact.
- The observation artifact is hash-pinned and immutable unless the oracle version changes.

---

# Workstream CE2 — Correct Candidate Chain Execution

## Objective

Make candidate execution match CE1 exactly.

## Target files

- `python/eggress/_pproxy_proxy.py`
- `python/eggress/outbound.py`
- `python/eggress/pproxy_connection.py`
- `python-pproxy-compat/pproxy/server.py`
- PyO3 connector bindings only if required
- route-through tests

## Implementation decision rule

Do not begin this workstream until CE1 identifies which object is reached first and which protocol is applied first.

Implement one structured recursive algorithm. The algorithm must use proxy objects and explicit endpoint fields, not serialized display strings.

A valid implementation should resemble one of these structures, selected by oracle evidence.

### Model A: use `.jump` to reach the current endpoint

```python
async def tcp_connect(self, final_host, final_port, *, local_addr=None, lbind=None):
    reader, writer = await self.jump.tcp_connect(
        self.host_name,
        self.port,
        local_addr=local_addr,
        lbind=lbind,
    )
    return await self.apply_current_proxy(
        reader,
        writer,
        final_host,
        final_port,
    )
```

### Model B: reach the current endpoint, then apply the nested protocol sequence

```python
async def tcp_connect(self, final_host, final_port, *, local_addr=None, lbind=None):
    reader, writer = await open_direct_or_structured_endpoint(
        self.host_name,
        self.port,
        local_addr=local_addr,
        lbind=lbind,
    )
    return await self.apply_observed_protocol_sequence(
        reader,
        writer,
        final_host,
        final_port,
        self.jump,
    )
```

The oracle event trace determines which is correct. Do not combine both models heuristically.

## Required cleanup

- `_build_remote_uri()` must be removed from runtime routing or clearly renamed as diagnostic-only;
- `tcp_connect()` must not call `str(self._jump)` or `repr(self._jump)`;
- `prepare_connection()` must not recursively apply protocols in an order not proven by CE1;
- all intermediate writers must close on failure;
- timeout and cancellation must unwind the entire chain;
- connection counters must return to baseline.

## Required tests

- single-hop HTTP;
- single-hop SOCKS4 IPv4;
- single-hop SOCKS4a domain;
- single-hop SOCKS5 IPv4;
- single-hop SOCKS5 domain;
- SOCKS5 username/password;
- both two-hop orientations;
- first-hop unavailable;
- second-hop unavailable;
- first handshake rejection;
- second handshake rejection;
- timeout at each stage;
- cancellation at each stage;
- no direct fallback in every failure case.

## Acceptance criteria

- Candidate normalized event traces match oracle traces for both chain orientations.
- Every expected proxy receives exactly one expected handshake.
- The final destination sentinel records zero direct attempts.
- No runtime routing uses string conversion.
- All intermediate resources close on success, failure, timeout, and cancellation.

---

# Workstream CE3 — Strengthen Deterministic Route-Through Fixtures

## Objective

Make route-through tests impossible to pass by direct connection or by object-construction assertions.

## Target files

- `python/tests/test_pproxy_route_through.py`
- new or existing scripted proxy fixture helpers
- `python/tests/strict/test_protocol_wire_differential.py`
- `scripts/strict_protocol_wire_probe.py`

## Fixture requirements

Each scripted proxy must:

- bind only to loopback;
- record raw request bytes;
- record normalized logical events;
- optionally fragment every response byte;
- support intentional rejection and timeout;
- optionally tunnel to the next scripted proxy;
- expose a deterministic `await events_complete()` method;
- expose all accepted sockets for cleanup assertions.

## Direct-bypass sentinels

Use targets that cannot succeed directly but can be accepted symbolically by the proxy.

```text
chain-target.invalid:443
route-through.invalid:8443
192.0.2.123:65000
```

The proxy may return success and echo bytes without contacting the target. A direct implementation must fail.

## Assertion pattern

```python
assert normalized_events == oracle_events
assert destination_sentinel.connection_count == 0
assert fixture.open_connection_count == 0
assert fixture.unhandled_exceptions == []
```

Do not accept only:

```python
assert proxy.jump is not None
```

## Acceptance criteria

- All route-through tests execute installed wheels, not repository imports.
- Both chain orientations are covered.
- Fragmented responses are covered.
- Every failure case asserts zero direct destination attempts.
- The suite has zero skips and zero xfails.

---

# Workstream CE4 — Correct `socks_address` Exactly

## Objective

Remove the supported-path address-helper skip by matching the oracle contract.

## Target files

- `python/eggress/protocol.py`
- `python/eggress/protocol.pyi`
- `python-pproxy-compat/pproxy/proto.py`
- `python/tests/test_milestone_c_properties.py`
- strict API and behavior probes

## Required first step

Probe the oracle for:

- `inspect.signature(socks_address)`;
- synchronous versus asynchronous call kind;
- reader method requirements;
- IPv4, IPv6, and domain behavior;
- IDNA behavior;
- port byte order;
- invalid address type;
- truncated input;
- trailing bytes;
- exception class and message category.

Do not assume `io.BytesIO` is the correct reader until the oracle probe confirms it.

## Required design

Separate public decoding from internal request encoding.

```python
# Public oracle-compatible helper.
def socks_address(reader, atyp):
    ...

# Internal request encoder.
def encode_socks_address(host, port):
    ...
```

The public helper’s actual call kind must follow the oracle.

## Required cases

- IPv4;
- IPv6;
- ASCII domain;
- IDNA domain;
- port 0;
- port 65535;
- domain length 0;
- domain length 255;
- invalid type;
- truncated stream at every field boundary;
- fragmented reader;
- trailing bytes preserved.

## Acceptance criteria

- Remove the class-level skip from `TestAddressRoundTrip`.
- Installed-wheel candidate tests pass.
- Oracle and candidate match in signature, return values, trailing bytes, and exception classes.
- Internal request construction uses `encode_socks_address()` rather than overloading the public helper.
- Type stubs match runtime behavior.

---

# Workstream CE5 — Close Real Server Listener Behavior

## Objective

Prove that compatibility listeners handle real clients, not only bind and close.

## Target files

- `python/eggress/_pproxy_proxy.py`
- `python-pproxy-compat/pproxy/server.py`
- `python/tests/test_server_lifecycle_pproxy.py`
- `python/tests/test_milestone_c_functional.py`
- new `python/tests/test_pproxy_listener_behavior.py`

## Required callback adapter

`asyncio.start_server()` invokes:

```python
client_connected_cb(reader, writer)
```

The adapter must supply every additional pproxy handler parameter explicitly.

```python
async def client_connected(reader, writer):
    try:
        await stream_handler(
            reader,
            writer,
            unix=normalized_unix,
            lbind=normalized_lbind,
            protos=normalized_protos,
            rserver=normalized_rserver,
            cipher=normalized_cipher,
            sslserver=normalized_sslserver,
            **normalized_runtime_args,
        )
    except BaseException:
        writer.close()
        if hasattr(writer, "wait_closed"):
            await writer.wait_closed()
        raise
```

Probe whether the oracle schedules this coroutine as a task or returns it directly from the callback.

## Required live scenarios

- HTTP CONNECT listener to a loopback echo server;
- SOCKS5 no-auth greeting, CONNECT, and payload relay;
- SOCKS5 username/password success;
- SOCKS5 auth failure;
- fragmented greeting and request;
- malformed command;
- handler exception;
- client cancellation;
- repeated server close;
- shutdown with active clients;
- no unhandled task exceptions.

## Acceptance criteria

- Remove every supported-path listener skip.
- A live SOCKS5 client receives a valid method selection and CONNECT reply.
- Payload crosses the listener to the echo target and back.
- Handler argument binding produces no `TypeError`.
- Server shutdown leaves no client task or socket open.
- Installed-wheel tests pass in a clean venv.

---

# Workstream CE6 — Finish Common Protocol Semantics

## Objective

Close the remaining HTTP/SOCKS behavioral gaps under fragmented and malformed input.

## Target files

- `python/eggress/protocol.py`
- `python/eggress/protocol.pyi`
- protocol tests and strict wire probes

## Required parser behavior

Use exact-length, delimiter, and bounded-header readers. Do not rely on one large `read()` call.

### HTTP

- fragmented request line;
- fragmented headers;
- CONNECT success and rejection;
- absolute-form to origin-form rewriting;
- Host preservation;
- `Proxy-Authorization` removal before origin forwarding;
- `Proxy-Connection` handling;
- bytes following headers preserved;
- maximum header bound;
- early EOF and malformed request/status line.

### SOCKS4/4a

- IPv4 and domain requests;
- user ID parsing;
- fragmented null-terminated fields;
- success and failure replies;
- command rejection;
- trailing payload preservation.

### SOCKS5

- fragmented greeting;
- no-auth and username/password negotiation;
- unsupported method;
- IPv4, IPv6, and domain requests;
- CONNECT success and failure reply;
- malformed address and command;
- trailing payload preservation;
- UDP ASSOCIATE where already declared supported in A–C.

## `http_channel()` requirements

It must have dedicated behavior. The following is prohibited:

```python
async def http_channel(...):
    return await self.channel(...)
```

Test each transformation independently and in an end-to-end listener scenario.

## Acceptance criteria

- Byte-level oracle and candidate observations match for required cases.
- Fragmented input works for all supported protocols.
- Post-handshake bytes are preserved.
- HTTP proxy-only headers are not forwarded to the origin.
- No supported test expects `NotImplementedError`.
- Unsupported protocol classes remain explicit and separately classified.

---

# Workstream CE7 — Rewrite the Paired Comparator to Fail Closed

## Objective

Make the authoritative paired runner incapable of reporting compatibility without affirmative evidence.

## Target files

- `scripts/run_strict_pproxy_api.py`
- `scripts/run_strict_pproxy_api.sh`
- strict runner unit tests
- `python/tests/strict/conftest.py`

## Required parser correction

Use `tomllib` or the project’s canonical TOML parser. Do not parse the manifest with line-oriented regular expressions. Boolean fields such as `closure_required` and arrays must retain their actual types.

## Required result states

Each record must end in exactly one state:

```text
pass
fail
delegated
known_upstream_defect
harness_error
```

`skipped` is not permitted for a closure-required A–C record.

## Fail-closed comparison table

| Oracle | Candidate | Result |
|---|---|---|
| exists/succeeds | matches | pass |
| exists | missing | fail |
| missing | exists | fail |
| missing | missing | fail |
| succeeds | error | fail |
| error | succeeds | fail |
| error | same error | fail unless record-specific pinned known defect |
| timeout | any | harness_error/fail |
| malformed output | any | harness_error/fail |
| missing artifact | any | harness_error/fail |

## Signature rules

Compare exactly:

- positional-only parameters;
- positional-or-keyword parameters;
- keyword-only parameters;
- varargs and kwargs;
- parameter names;
- defaults;
- coroutine kind;
- callable/property/class kind.

Delete this behavior:

```python
if a_is_variadic != b_is_variadic:
    return True
```

If a wrapper needs generic internal dispatch, expose an exact public function or exact `__signature__` and test real argument binding.

## Known upstream defects

An oracle error may be accepted only when:

1. the manifest record status is `known_upstream_defect`;
2. the record ID is in a version-pinned allowlist;
3. the observed fingerprint matches the pinned fingerprint;
4. the candidate is evaluated under the record’s explicit policy;
5. the report counts it separately from compatibility passes.

Equal free-form error strings are never enough.

## Environment verification

Fail before probing unless all are true:

- oracle interpreter exists and has the expected `sys.prefix`;
- oracle imports upstream `pproxy==2.7.9` from the oracle venv;
- oracle cannot import candidate `eggress` from repository paths;
- candidate imports the compatibility `pproxy` package and canonical `eggress` wheel;
- candidate does not import upstream pproxy distribution;
- candidate commit and wheel hashes are recorded.

Every subprocess install/build command must check its return code.

## Acceptance criteria

- Both-missing fails.
- Identical errors fail absent a pinned record-specific rule.
- Variadic-versus-explicit mismatch fails.
- Altered default, parameter kind, or coroutine kind fails.
- Missing probe output fails.
- Contaminated oracle and candidate environments fail.
- `--closure-required` produces nonzero exit for any unresolved required record.

---

# Workstream CE8 — Complete Observation Production and Delegated Evidence

## Objective

Produce every observation required by the strict differential suite and manifest.

## Target files

- strict API probe scripts;
- cipher KAT/round-trip probe scripts;
- protocol-wire probe scripts;
- process-lifecycle probe scripts;
- external interop scripts;
- new `scripts/build_strict_evidence_index.py`;
- strict test observation loaders.

## Observation layout

Use one canonical structure:

```text
target/closure-audit/observations/
  api/
    oracle/
    candidate/
  cipher/
    oracle/
    candidate/
  protocol_wire/
    oracle/
    candidate/
  process/
    oracle/
    candidate/
  interop/
    tcp/
    udp/
```

Do not mix oracle and candidate files in one flat directory unless each file has an unambiguous side field and the loader validates it.

## Delegated result schema

```json
{
  "record_id": "behavior.connection.socks5.route_through",
  "status": "delegated",
  "runner": "strict_protocol_wire",
  "artifact": "observations/protocol_wire/candidate/socks5_route_through.json",
  "candidate_commit": "<full sha>",
  "manifest_sha256": "<sha256>",
  "runner_sha256": "<sha256>"
}
```

The evidence aggregator resolves delegated records. The API runner must not call them passed.

## Gate 14 correction

Inventory every node ID in `python/tests/strict/` and map it to its required observation producer. In particular, produce the per-class cipher observations expected by `test_cipher_differential.py` or change that test to consume a documented canonical cipher artifact without weakening coverage.

Add a machine-readable mapping:

```toml
[[mapping]]
test_node = "python/tests/strict/test_cipher_differential.py::..."
record_id = "python.pproxy.cipher.AES_256_GCM_Cipher"
runner = "strict_cipher_kat"
artifact = "observations/cipher/oracle/...json"
```

## Acceptance criteria

- Every strict differential test has a producer mapping.
- Gate 14 runs with zero missing observation errors.
- Missing or stale delegated artifact fails the evidence aggregator.
- Artifact side, record ID, comparator, commit, and manifest hash are validated.
- No specialized behavior is silently skipped by the API runner.

---

# Workstream CE9 — Audit `inventory_only` and Behavior Links

## Objective

Prevent structural classifications from hiding required behavior.

## Target files

- `docs/parity/pproxy_2_7_9_strict_manifest.toml`
- `crates/eggress-testkit/src/strict_manifest.rs`
- report generator and validator tests

## Classification test

A record may be `inventory_only = true` only when all are true:

1. it is not callable or stateful in a compatibility-relevant way;
2. downstream behavior is fully certified through another explicitly linked public record, or no behavior exists;
3. the rationale is written in `notes`;
4. a validator-recognized reason code is present;
5. reviewer evidence confirms that omitting behavioral execution does not weaken the compatibility claim.

Suggested reason codes:

```text
namespace_marker
reexport_only
metadata_constant
abstract_inventory
covered_by_parent_behavior
```

Public factories, connection methods, protocol methods, listener methods, lifecycle methods, parsers, and stateful classes should normally have `behavior_record` links.

## Required validator rules

- behavior target exists;
- target has behavioral, interop, or process scope;
- no circular links;
- no structural-to-structural closure link;
- inventory-only has an allowed reason code and notes;
- callable/stateful kinds cannot use inventory-only without an explicit reviewed exception;
- every closure-required A–C behavior record has a runner and artifact mapping;
- closure is evaluated through `meta.closure_through = "C"`.

## Acceptance criteria

- All broad inventory-only assignments are reviewed individually.
- Public callable/stateful records have behavior links unless an explicit validated exception exists.
- Invalid reason codes fail validation.
- A nonexistent or circular behavior link fails validation.
- Readiness is based on required behavior/interop/process evidence, not terminal structural count.

---

# Workstream CE10 — Rebuild the Authoritative Closure Audit

## Objective

Make the local audit a complete, reproducible, fail-closed certification run.

## Target files

- `scripts/run_strict_pproxy_closure_audit.sh`
- new evidence and JUnit validators;
- dependency lock files;
- regression-injection runner.

## Fresh-run requirements

At start:

```bash
rm -rf target/closure-audit
rm -rf .venv-oracle-api .venv-candidate-api
mkdir -p target/closure-audit
```

Do not preserve or symlink stale observation directories from earlier invocations.

Create explicit oracle and candidate environments. Required dependency installation must not use `|| true`.

The current pattern is prohibited:

```bash
pip install pytest pytest-asyncio pytest-timeout || true
```

## Required gate responsibilities

The audit may have 27 named gates or fewer composed gates, but it must machine-verifiably execute all responsibilities below.

1. cargo fmt;
2. cargo check;
3. cargo clippy with warnings denied;
4. cargo test workspace;
5. cargo deny;
6. cargo audit;
7. strict manifest validation;
8. deterministic report freshness;
9. documentation consistency;
10. oracle artifact and provenance validation;
11. canonical wheel build;
12. compatibility wheel build;
13. installed-wheel Python suite;
14. paired API comparison in closure mode;
15. complete strict differential suite;
16. route-through protocol-wire suite;
17. external TCP interop;
18. supported external UDP interop;
19. supported cipher KAT and interop;
20. plugin transformed-traffic probe;
21. real listener/process lifecycle probe;
22. failure and cancellation cleanup;
23. resource-leak checks;
24. required skip/xfail audit;
25. evidence-index validation;
26. commit/manifest/wheel/oracle/runner hash binding;
27. regression-injection proof.

## JUnit policy

Generate JUnit XML for every Python closure subset and validate:

- zero required skips;
- zero required xfails;
- zero missing required node IDs;
- zero unexpected deselections;
- every manifest `test_ref` appears in executed evidence.

## Evidence index

Create:

```text
target/closure-audit/evidence/evidence_index.json
```

It must list every required record, producing gate, artifact path, hashes, and outcome.

## Final pass condition

```text
failed_required == 0
skipped_required == 0
missing_required_artifacts == 0
stale_required_artifacts == 0
unmapped_required_records == 0
```

## Acceptance criteria

- The audit starts from clean environments and empty evidence directories.
- No required installation failure is ignored.
- Gate 14 strict differential passes.
- One skipped required test fails the audit.
- One deleted or stale artifact fails the audit.
- The final report includes command, interpreter, exit code, duration, log, artifact, commit, and manifest hash for every responsibility.

---

# Workstream CE11 — Correct Hosted Workflow Closure

## Objective

Run the same authoritative audit in GitHub Actions and retain trustworthy evidence.

## Target files

- `.github/workflows/strict-differential.yml`
- reusable workflows only if needed

## Required architecture

Prefer one authoritative job:

```yaml
strict-closure-audit:
  runs-on: ubuntu-latest
  timeout-minutes: 90
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - uses: actions/setup-python@v5
      with:
        python-version: "3.11"
    - run: cargo install cargo-deny cargo-audit
    - run: python -m pip install maturin
    - run: ./scripts/run_strict_pproxy_closure_audit.sh
    - uses: actions/upload-artifact@v4
      if: always()
      with:
        name: strict-a-c-closure-${{ github.sha }}
        path: target/closure-audit/
        if-no-files-found: error
        retention-days: 90
```

Fast feedback jobs may remain, but they do not replace the authoritative job.

## Required workflow behavior

- explicit dependencies for every invoked command;
- no use of weaker duplicated closure commands;
- artifact upload uses `if-no-files-found: error`;
- failed-gate logs are retained with `if: always()`;
- visible commit status is produced;
- artifact commit and manifest hashes are validated against `${{ github.sha }}`;
- strict differential observations are generated in the same run or downloaded and hash-validated.

## Acceptance criteria

- Workflow syntax and lint pass.
- A missing paired or delegated artifact fails the job.
- A required skip fails the job.
- A successful run retains one complete evidence bundle named with the exact commit SHA.
- If Actions is externally unavailable, status remains `LOCAL GATES PASS — HOSTED EVIDENCE PENDING`; A remains open.

---

# Workstream CE12 — Regression Injection Proof

## Objective

Prove that the corrected gates detect every defect that escaped earlier passes.

## Target files

- `scripts/demonstrate_regression_injections.sh`
- `tests/regression_injections/`
- closure audit evidence

## Required injections

1. Directly connect to the final target in single-hop `tcp_connect()`.
2. Reverse the oracle-confirmed two-hop protocol order.
3. Use `str(self.jump)` for runtime routing.
4. Revert `socks_address` to encoder-only behavior.
5. Remove a required listener handler argument.
6. Make `http_channel()` delegate directly to `channel()`.
7. Return success for both-missing observations.
8. Return success for identical errors.
9. Restore variadic-signature automatic compatibility.
10. Delete a required cipher observation.
11. Delete a required protocol-wire artifact.
12. Bind one artifact to another commit.
13. Mark one supported closure test skipped.
14. Omit `--closure-required`.
15. Point oracle imports at the candidate package.
16. Point candidate imports at upstream pproxy.
17. Reclassify a callable public record as inventory-only without an approved reason.
18. Make a required interop responsibility optional.

Each injection must run in a temporary worktree or copied file tree and restore automatically.

## Acceptance criteria

- Every injection causes the intended narrow gate to fail.
- The injection harness fails if an injected defect is not detected.
- No mutation remains in the main worktree.
- The retained report records injection, expected gate, actual gate, and exit code.

---

# Workstream CE13 — Final Evidence and Status Promotion

## Objective

Close A–C only after evidence is complete.

## Local interim status

After all local responsibilities pass:

`LOCAL GATES PASS — HOSTED EVIDENCE PENDING`

Do not mark A, B, or C complete.

## Hosted completion requirements

A final completion record must include:

- full candidate commit SHA;
- oracle package/version and artifact hash;
- canonical and compatibility wheel hashes;
- manifest hash;
- runner hashes;
- workflow run ID;
- artifact ID and retention period;
- required record counts by structural, behavioral, interop, and process scope;
- zero required skips;
- zero missing or stale artifacts;
- known upstream defects listed separately;
- later-milestone gaps listed separately.

Generate or validate the completion record from `evidence_index.json`. Do not manually copy test counts from console output.

## Acceptance criteria

- Local completion cannot mark A–C complete.
- Hosted completion references the exact tested commit and retained artifact.
- README, milestone plans, manifest, report, workflow status, and completion record agree.
- Historical superseded records remain historical.
- D–F status is unchanged.

---

# 6. Ordered implementation sequence for a smaller model

Do not work ahead of this sequence.

## Commit 1 — Truth and blocking inventory

- CE0 only;
- update status and skip/harness inventory;
- no runtime changes.

### Commit acceptance

- docs remain truthful;
- consistency test catches a false completion statement.

## Commit 2 — Oracle chain probe

- CE1 probe scripts and frozen oracle artifacts;
- no candidate chain changes.

### Commit acceptance

- both chain orientations have topology and event traces;
- oracle provenance is recorded.

## Commit 3 — Route fixtures and failing chain regressions

- CE3 fixtures;
- candidate tests demonstrate the current mismatch where applicable;
- direct sentinels included.

### Commit acceptance

- fixtures themselves pass unit tests;
- chain event assertions are deterministic.

## Commit 4 — Candidate chain correction

- CE2 only;
- implement oracle-confirmed execution order;
- no evidence-runner changes.

### Commit acceptance

- single-hop and two-hop route-through suite passes;
- all failure-path cleanup tests pass.

## Commit 5 — Address helper closure

- CE4 oracle probe, runtime correction, type stubs, skip removal.

### Commit acceptance

- complete address suite passes in installed-wheel venv;
- no address-helper skip remains.

## Commit 6 — Real listener closure

- CE5 callback adapter and live listener tests;
- remove supported listener skip.

### Commit acceptance

- real SOCKS5 handshake and payload relay pass;
- shutdown leaves no tasks or sockets.

## Commit 7 — Protocol fragmentation and HTTP channel

- CE6 only.

### Commit acceptance

- byte-level fragmented and malformed cases pass;
- HTTP proxy-only headers are handled correctly.

## Commit 8 — Fail-closed paired comparator

- CE7 parser, comparison, environment, and unit tests.

### Commit acceptance

- all negative comparator injections fail;
- closure mode has no generic skip result.

## Commit 9 — Complete observation producers

- CE8 specialized observations and mapping;
- repair strict differential gate.

### Commit acceptance

- every strict node has an observation mapping;
- strict differential suite passes locally.

## Commit 10 — Manifest classification audit

- CE9 only.

### Commit acceptance

- inventory-only review complete;
- behavior links and reason codes validate.

## Commit 11 — Authoritative closure audit

- CE10 fresh environments, JUnit checks, evidence index, binding.

### Commit acceptance

- full local audit passes;
- deleted/stale artifact regression fails.

## Commit 12 — Hosted workflow

- CE11 only.

### Commit acceptance

- workflow lint passes;
- authoritative job invokes only the canonical closure script.

## Commit 13 — Regression injection completion

- CE12 all injections.

### Commit acceptance

- every required injection is detected;
- report retained.

## Commit 14 — Local evidence status

- execute full audit;
- set only local-pending-hosted status if successful.

## Commit 15 — Hosted completion record

Only after a successful hosted run on the exact head:

- CE13 completion record;
- mark A–C complete;
- leave D–F unchanged.

---

# 7. Mandatory final acceptance matrix

## Milestone A — Honest contract and evidence

- [ ] Oracle artifact is pinned and hash-validated.
- [ ] Oracle and candidate import roots are isolated.
- [ ] Manifest is parsed with a real TOML parser.
- [ ] Both-missing fails.
- [ ] Identical errors fail absent a pinned record-specific rule.
- [ ] Variadic-versus-explicit signatures do not automatically match.
- [ ] Timeouts, malformed output, missing output, and missing artifacts fail.
- [ ] Delegated records require valid delegated artifacts.
- [ ] Every strict test has an observation producer mapping.
- [ ] Inventory-only classifications are individually justified.
- [ ] Behavioral readiness uses required behavioral/interop/process evidence.
- [ ] Evidence is bound to exact commit, manifests, wheels, oracle, and runners.
- [ ] Local audit has zero required skips.
- [ ] Hosted audit and artifact bundle exist.

## Milestone B — Python source and object behavior

- [ ] Top-level aliases and signatures match the oracle.
- [ ] Single-hop HTTP traverses the configured proxy.
- [ ] Single-hop SOCKS4 traverses the configured proxy.
- [ ] Single-hop SOCKS4a traverses the configured proxy.
- [ ] Single-hop SOCKS5 traverses the configured proxy.
- [ ] Supported SOCKS5 authentication works.
- [ ] Both two-hop orientations match oracle event order.
- [ ] No routing uses string serialization.
- [ ] Proxy failure never falls back directly.
- [ ] Cancellation and timeout unwind every hop.
- [ ] `socks_address` matches the oracle.
- [ ] `start_server()` returns the oracle-compatible handle.
- [ ] Real listener behavior works.
- [ ] Clean installed-wheel suite passes.

## Milestone C — Functional protocol API

- [ ] Shared `AuthTable` semantics remain correct.
- [ ] Public address decoding and internal encoding are separated.
- [ ] HTTP client and server handshakes work.
- [ ] SOCKS4/4a client and server handshakes work.
- [ ] SOCKS5 method negotiation, auth, CONNECT, and replies work.
- [ ] Fragmented protocol input works.
- [ ] Rollback and trailing payload preservation work.
- [ ] Raw channel works without statistics callback.
- [ ] `http_channel()` performs required transformations.
- [ ] Supported UDP behavior has required evidence.
- [ ] Supported cipher KAT and interop evidence exists.
- [ ] Plugin transformed-traffic evidence exists.
- [ ] Failure and cancellation cleanup closes resources.
- [ ] No supported behavior is closed by `NotImplementedError`, skip, or xfail.

## Cross-cutting closure

- [ ] Strict differential gate passes.
- [ ] External TCP interop passes.
- [ ] Supported external UDP interop passes.
- [ ] Evidence index has no missing, stale, or unmapped required records.
- [ ] Regression injections are detected.
- [ ] Workflow artifact upload uses `if-no-files-found: error`.
- [ ] Hosted status and retained evidence correspond to the exact completion commit.

---

# 8. Required verification commands

The authoritative script may wrap these commands, but equivalent responsibilities must execute.

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo audit

cargo test -p eggress-testkit strict_manifest
cargo test -p eggress-testkit strict_report
cargo run -p eggress-testkit --bin strict-report -- --check
python3 scripts/check_release_docs.py

rm -rf target/closure-audit .venv-oracle-api .venv-candidate-api

./scripts/run_strict_pproxy_api.sh --closure-required

.venv-candidate-api/bin/python -m pytest python/tests -q \
  --junitxml target/closure-audit/junit/candidate.xml

.venv-candidate-api/bin/python -m pytest python/tests/strict -q \
  --oracle-observations-dir target/closure-audit/observations \
  --candidate-observations-dir target/closure-audit/observations \
  --junitxml target/closure-audit/junit/strict.xml

.venv-candidate-api/bin/python -m pytest \
  python/tests/test_pproxy_route_through.py \
  python/tests/test_pproxy_listener_behavior.py \
  python/tests/test_milestone_c_properties.py -q

./scripts/run_strict_pproxy_interop.sh
./scripts/compat_udp_pproxy.sh
python3 scripts/build_strict_evidence_index.py --validate
python3 scripts/check_junit_no_required_skips.py target/closure-audit/junit/
./scripts/demonstrate_regression_injections.sh
./scripts/run_strict_pproxy_closure_audit.sh
```

Exact observation-directory CLI shape may be adjusted, but oracle and candidate side identity must remain explicit and validated.

---

# 9. Stop conditions

Stop implementation and leave A–C reopened if any of these occurs:

- oracle chain order remains ambiguous;
- two-hop candidate events do not match the oracle;
- either supported-path skip remains;
- strict differential observations are incomplete;
- paired comparator still accepts mutual absence or identical errors;
- any required record is unmapped;
- local audit has a failed or skipped required responsibility;
- evidence is bound to another commit;
- hosted workflow did not run or did not retain artifacts.

Do not compensate by changing a record to inventory-only, known-upstream-defect, not-applicable, or later-milestone without evidence and policy review.

---

# 10. Handoff guidance

1. Start with CE0 and CE1. Do not edit chain runtime until the oracle event trace exists.
2. Keep commits narrow. Do not combine comparator changes with runtime routing changes.
3. Use installed wheels for compatibility tests.
4. Treat the strict differential failure as a producer/aggregation defect, not a reason to remove tests.
5. Remove supported skips only by implementing and testing the behavior.
6. Review every inventory-only record rather than applying bulk defaults.
7. Regenerate reports only after manifest changes; do not confuse the report with runtime evidence.
8. Run the full audit before changing any milestone status.
9. Do not begin Milestone D work.

The highest-priority closure sequence is:

```text
oracle chain trace
→ candidate chain correction
→ address and listener skip removal
→ fail-closed comparator
→ complete observation production
→ authoritative audit
→ hosted evidence
```

---

# 11. Reviewer questions

A reviewer must be able to answer all of these from retained evidence alone:

1. What exact pproxy artifact was used?
2. What object topology did the oracle produce for both two-hop URI orientations?
3. What exact wire event order did the oracle produce?
4. Did candidate events match exactly?
5. Did every single-hop route reach the configured proxy?
6. Was direct destination access impossible in sentinel tests?
7. Does `socks_address` match the oracle?
8. Did a real SOCKS5 listener negotiate, CONNECT, and relay payload?
9. Did fragmented HTTP/SOCKS cases pass?
10. Did both-missing and identical-error injections fail?
11. Did every strict test receive its required observation?
12. Were any supported tests skipped or xfailed?
13. Were all inventory-only classifications justified?
14. Did local closure pass with no missing or stale artifact?
15. Did hosted closure run on the same commit and retain the evidence bundle?

If any answer is unavailable, Milestones A–C are not complete.
