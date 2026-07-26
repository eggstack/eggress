# Milestones A–C Post-Implementation Corrective Closure

## Status

LOCAL GATES PASS — HOSTED EVIDENCE PENDING

Workstreams completed:
- PC0: Freeze the Honest Baseline
- PC1: Correct the Proxy Object Execution Model
- PC2: Build Deterministic Route-Through Fixtures
- PC3: Correct `socks_address` and Address Helpers
- PC4: Repair `start_server()` and Handler Invocation
- PC5: Complete Common Protocol Semantics (`http_channel()` URI rewrite, Proxy-* header stripping)
- PC6: Restore Fail-Closed Paired Evidence (both-missing fails, identical errors fail, `--closure-required`)
- PC7: Enforce Manifest-to-Behavior Evidence Links (Rule 10c, `inventory_only`, `closure_through = "C"`, `CURRENT_MILESTONE = "C"`)
- PC8: Make the Closure Audit Authoritative (all gates mandatory)
- PC9: Correct Hosted Workflow Execution (`if-no-files-found: error` for paired observations)
- PC10: Regression Injection Proof (12 injection cases, all detected)

Remaining:
- PC11: Final Status and Completion Evidence (pending hosted CI run)

## Parent plans and audit baseline

- `plans/PPROXY_FULL_DROP_IN_ROADMAP.md`
- `plans/MILESTONE_A_HONEST_CONTRACT.md`
- `plans/MILESTONE_B_PYTHON_SOURCE_COMPATIBILITY.md`
- `plans/MILESTONE_C_FUNCTIONAL_INTERNAL_API.md`
- `plans/MILESTONES_A_C_CORRECTIVE_PASS.md`
- `plans/MILESTONES_A_C_FINAL_EVIDENCE_RUNTIME_CLOSURE.md`

Audit baseline reviewed for this plan:

- repository: `eggstack/eggress`
- baseline head: `80fa9d0a0d214c501c5f449ddca3df802bbc0338`
- oracle: `pproxy==2.7.9`

This plan supersedes the implementation guidance in the prior final closure plan only where the current repository has demonstrated that the prior guidance was insufficiently precise. The earlier plans remain valuable design history and must not be deleted.

---

# 1. Purpose

Close the remaining Milestones A–C defects found after the latest implementation pass.

The repository is now substantially better than the original compatibility preview. Shared `AuthTable` state, common HTTP/SOCKS client methods, an installed-wheel Python test environment, a strict manifest schema, and a larger closure script have all landed. Those changes must be preserved.

The remaining blockers are narrower but release-critical:

1. `ProxySimple.tcp_connect()` still appears able to bypass the configured proxy and connect directly to the requested destination.
2. nested `.jump` chains are still reconstructed or interpreted through URI/string logic instead of executing the object graph hop by hop;
3. `socks_address` has the wrong callable contract and its tests are skipped;
4. the live SOCKS5 server test is skipped because the server callback does not satisfy the `asyncio.start_server()` invocation contract;
5. common protocol server-side behavior remains incomplete for fragmented input, replies, rollback, and HTTP channel transformations;
6. the paired evidence runner accepts both-missing and identical-error observations as success in paths where closure requires proof of behavior;
7. the paired runner skips `protocol_wire` records without proving that another required runner produced evidence for them;
8. the authoritative closure script treats paired API, strict differential, external TCP, and external UDP gates as optional;
9. the strict differential tests can skip when observation directories are absent and still allow the audit to pass;
10. manifest behavior links are declared in the schema but are not fully enforced;
11. the checked-in report is manifest-derived but is being treated as though it proves behavior for the current code commit;
12. hosted evidence is still absent and the workflow remains capable of warning or skipping where closure must fail.

This is a corrective closure pass. It must not expand into Milestones D–F.

---

# 2. Required final outcome

At completion, all of the following must be true:

1. A single-hop `pproxy.Connection("http://…")`, `socks4://…`, or `socks5://…` connection demonstrably traverses that proxy endpoint.
2. A two-hop `__` chain demonstrably traverses both configured proxy endpoints in the oracle-compatible order.
3. Disabling or rejecting the configured proxy causes the operation to fail. It must never fall back to a direct destination connection.
4. The runtime executes the nested proxy object graph. It does not derive routing from `repr()`, `str(proxy)`, or a partially reconstructed jump URI.
5. `socks_address` matches the oracle’s signature, call kind, input model, output model, and failure behavior.
6. `ProxySimple.start_server()` installs a client callback that can actually be invoked by `asyncio.start_server(reader, writer)` and supplies all required pproxy handler arguments.
7. Supported HTTP, SOCKS4/4a, and SOCKS5 client and server paths work under fragmented reads and malformed-input cases.
8. `http_channel()` implements the oracle-required HTTP transformations rather than delegating unconditionally to raw relay.
9. No supported A–C behavior is hidden behind `pytest.skip`, `xfail`, expected `NotImplementedError`, or an optional audit gate.
10. The paired comparator fails on both-missing, both-error, oracle error, candidate error, timeout, malformed JSON, missing evidence, and skipped closure-required records unless the record is explicitly classified as a pinned known upstream defect.
11. Every closure-required A–C record is mapped to a concrete evidence artifact produced for the exact candidate commit.
12. The authoritative local closure script exits nonzero if any mandatory A–C gate fails or is skipped.
13. The hosted closure job runs the same authoritative gate and retains mandatory evidence with `if-no-files-found: error`.
14. Milestones A–C remain reopened until both local and hosted closure evidence exists.

---

# 3. Scope

## In scope

- `ProxySimple` single-hop and chained upstream execution;
- direct-fallback prevention;
- common HTTP, SOCKS4/4a, and SOCKS5 route-through behavior;
- supported SOCKS5 UDP behavior already within A–C scope;
- exact `socks_address` compatibility;
- `stream_handler` callback adaptation and server lifecycle behavior;
- fragmented protocol parsing and rollback;
- `BaseProtocol.channel()` and `http_channel()` semantics;
- strict paired-runner failure behavior;
- behavior-record and evidence-reference enforcement;
- authoritative closure audit behavior;
- strict GitHub Actions execution and artifact retention;
- removal of closure-invalid skips;
- generated, commit-bound local and hosted closure evidence.

## Out of scope

Do not add or claim parity for:

- SSH;
- QUIC;
- HTTP/3;
- SSR;
- transparent proxy support on unsupported platforms;
- new legacy cipher implementations beyond correcting evidence classification;
- executable/package-level parity assigned to Milestones D–F;
- unrelated CLI expansion;
- performance refactors not required to correct the identified behavior.

Unsupported later-milestone features must remain explicit `intentional_non_parity`, `platform_constraint`, or later-milestone `gap` records. Do not hide them and do not make this pass larger by implementing them.

---

# 4. Non-negotiable implementation rules

1. **No direct fallback.** A configured proxy failure is an operation failure, not permission to connect directly.
2. **Object graph is authoritative.** `.jump` objects define the next hop. String representations are diagnostic only.
3. **Oracle first.** Where a signature or edge behavior is uncertain, add a pinned oracle probe before editing candidate behavior.
4. **No closure by skip.** A skipped supported-path test is a failing closure gate.
5. **No closure by mutual absence.** Both sides missing or both sides erroring does not prove compatibility.
6. **No closure by structure alone.** Importability and signatures do not prove routing or protocol behavior.
7. **No stale environments.** Closure runners must recreate oracle and candidate environments from scratch.
8. **No system-Python ambiguity.** Python gates must use the explicit environment containing the tested wheel.
9. **No manual checkboxes as authority.** Completion status must be generated from machine-readable evidence.
10. **No Milestone D work.** Correct A–C only.

---

# 5. Current defects to preserve as regression cases

The implementation agent must treat these as failing regression tests before declaring closure:

| Defect | Current symptom | Required correction |
|---|---|---|
| Single-hop direct bypass | `ProxySimple._build_remote_uri()` returns `None` for `jump is DIRECT`; `tcp_connect()` calls the direct connector to the final target | connect to the current proxy endpoint through `.jump`, then perform the current proxy protocol handshake to the final target |
| String-derived chains | nested jump objects can be converted through `str()` or inner URI fields | recursively execute proxy objects; never reconstruct execution from display strings |
| `socks_address` mismatch | compatibility implementation encodes while pproxy callers/tests expect the oracle’s reader-based helper | probe and implement exact oracle contract; keep encoding in a separately named internal helper |
| SOCKS5 listener skip | `stream_handler` requires arguments not supplied by `asyncio.start_server()` | install a two-argument callback wrapper that supplies normalized handler state |
| Fragmentation weakness | `guess()` and `accept()` assume large reads contain a complete handshake | use exact-length/read-until helpers and rollback compatible with oracle behavior |
| `http_channel()` pass-through | method delegates directly to raw `channel()` | implement required proxy-header and request-target transformations |
| Both-missing accepted | paired runner can mark mutual absence as a match | closure-required records fail on both-missing |
| Identical errors accepted | same error text on oracle and candidate can pass | fail unless the record ID is explicitly allowlisted as a known upstream defect |
| Signature superset heuristic | `(*args, **kwargs)` can be treated as compatible with explicit signatures | require exact parameter kinds/names/defaults unless a specific record documents an approved wrapper and exposes exact `__signature__` |
| Protocol wire skipped | API runner skips `protocol_wire` records | evidence aggregator must require a corresponding wire artifact from the correct runner |
| Optional closure gates | paired and interop gates use `run_gate_optional` | convert A–C required evidence to mandatory gates |
| Skip-capable strict tests | missing observation directories trigger `pytest.skip` | closure invocation must provide directories and fail if absent |
| Unenforced behavior links | schema declares `behavior_record`, but validation does not reject missing links | apply the validator rule and verify target record existence/scope |
| Report confused with evidence | checked-in report contains an old commit and commit lines are normalized | separate manifest-derived report freshness from exact runtime evidence binding |

---

# Workstream PC0 — Freeze the Honest Baseline

## Objective

Record the exact defects and keep the repository status truthful while implementation proceeds.

## Target files

- `plans/MILESTONES_A_C_POST_IMPLEMENTATION_CORRECTIVE_CLOSURE.md`
- `plans/MILESTONE_A_HONEST_CONTRACT.md`
- `plans/MILESTONE_B_PYTHON_SOURCE_COMPATIBILITY.md`
- `plans/MILESTONE_C_FUNCTIONAL_INTERNAL_API.md`
- `README.md`
- any current A–C completion record

## Required changes

1. Keep A, B, and C as:

   `REOPENED — post-implementation corrective closure in progress`

2. Add a superseding note to any document that claims local A–C closure.
3. Record the baseline commit and the exact known skipped tests.
4. Add a machine-readable skip inventory, for example:

   `docs/parity/PPROXY_A_C_SKIP_INVENTORY.toml`

   Each entry must include:

   - test node ID;
   - reason;
   - milestone;
   - supported/deferred classification;
   - planned workstream;
   - whether closure is blocked.

5. Supported A–C skips must be classified as closure blockers.
6. Later-milestone and platform skips may remain, but must not be counted in the A–C closure suite.

## Acceptance criteria

- A, B, and C remain reopened.
- The two current supported-path skips are listed as blocking defects.
- No completion record says all local gates pass.
- Documentation consistency checks fail if a reopened milestone is described as complete elsewhere.

---

# Workstream PC1 — Correct the Proxy Object Execution Model

## Objective

Make each `ProxySimple` object represent one concrete proxy endpoint and execute `.jump` recursively as the route to that endpoint.

## Target files

- `python/eggress/_pproxy_proxy.py`
- `python/eggress/outbound.py`
- `python/eggress/pproxy_connection.py`
- `python-pproxy-compat/pproxy/server.py`
- `python/eggress/protocol.py`
- PyO3 outbound connector bindings only if required
- proxy factory and chain tests

## Required object model

For a parsed chain:

```python
proxy = pproxy.Connection(
    "socks5://proxy-a.example:1080__http://proxy-b.example:8080"
)
```

The expected topology is conceptually:

```text
ProxySimple(SOCKS5, endpoint=proxy-a:1080)
    .jump -> ProxySimple(HTTP, endpoint=proxy-b:8080)
                 .jump -> DIRECT
```

The first object is the protocol applied closest to the final destination. Its `.jump` describes how to reach that object’s endpoint.

## Required connection algorithm

Implement the equivalent of this reference flow. Exact method names and parameter order must follow the oracle:

```python
async def tcp_connect(self, target_host, target_port, *, local_addr=None, lbind=None):
    if self.direct:
        return await open_direct_socket(target_host, target_port, local_addr, lbind)

    proxy_host = self.host_name
    proxy_port = self.port
    if not proxy_host or not proxy_port:
        raise ValueError("configured proxy endpoint is incomplete")

    # Reach this proxy endpoint through the next hop.
    reader, writer = await self.jump.tcp_connect(
        proxy_host,
        proxy_port,
        local_addr=local_addr,
        lbind=lbind,
    )

    try:
        # Apply this proxy's TLS/cipher/plugin/protocol setup so that the
        # established stream reaches target_host:target_port.
        reader, writer = await self.prepare_connection(
            reader,
            writer,
            target_host,
            target_port,
        )
        return reader, writer
    except BaseException:
        writer.close()
        if hasattr(writer, "wait_closed"):
            await writer.wait_closed()
        raise
```

This is pseudocode. The implementation agent must first inspect the pinned pproxy source and oracle observations to determine the exact `prepare_connection()` signature and wrapper order.

## Critical semantic distinction

For a single-hop object:

```text
ProxySimple(SOCKS5 endpoint=127.0.0.1:1080, jump=DIRECT)
```

`jump=DIRECT` means:

> Open a direct TCP socket to `127.0.0.1:1080`, then issue a SOCKS5 request for the final target.

It does **not** mean:

> Ignore the SOCKS5 object and open a direct TCP socket to the final target.

## Chain execution example

For:

```text
SOCKS5 A -> HTTP B -> DIRECT
```

and final target `target.invalid:443`, the event sequence must be:

```text
1. DIRECT opens TCP to HTTP proxy B.
2. HTTP B receives CONNECT for SOCKS5 proxy A.
3. The resulting tunnel reaches SOCKS5 proxy A.
4. SOCKS5 A receives CONNECT for target.invalid:443.
5. The returned stream carries final target payload.
```

The following is incorrect and must be prohibited:

```python
uri = str(self.jump)
connector = OutboundConnector.from_pproxy_uri(uri)
```

`repr()` and `str()` are not execution serialization formats.

## Native connector guidance

The preferred architecture remains Rust-owned networking, but correctness takes priority:

1. Use a Rust direct socket primitive for the terminal `DIRECT` connection where available.
2. Apply lightweight Python compatibility handshakes on the returned stream when this most faithfully matches pproxy.
3. A native multi-hop connector may be used only if it consumes a structured chain representation and tests prove identical hop ordering.
4. Do not pass an incomplete jump URI to the native connector and assume it represents the current proxy object.

## Required methods to audit

- `proxy_by_uri()`;
- `proxies_by_uri()`;
- `destination()`;
- `open_connection()`;
- `tcp_connect()`;
- `prepare_connection()`;
- `udp_prepare_connection()`;
- `udp_open_connection()`;
- `udp_sendto()`;
- `_build_remote_uri()`;
- chain equality and display methods;
- connection accounting and cleanup.

`_build_remote_uri()` must not participate in runtime routing. It may be removed or retained for diagnostics only.

## Failure behavior

- invalid proxy endpoint: fail before contacting the destination;
- proxy refused connection: fail;
- proxy handshake rejection: fail;
- proxy timeout: fail;
- protocol parser failure: fail;
- intermediate chain failure: close every opened stream;
- no condition may trigger direct fallback.

## Acceptance criteria

- A single-hop HTTP connection reaches the HTTP proxy before any final target activity.
- A single-hop SOCKS4/4a connection reaches the SOCKS4 proxy.
- A single-hop SOCKS5 connection reaches the SOCKS5 proxy.
- Disabling each proxy causes failure rather than direct success.
- A two-hop chain produces the exact ordered hop event sequence.
- No runtime method uses `str(proxy)` or `repr(proxy)` to determine routing.
- Connection counters return to their initial values after success, failure, timeout, and cancellation.
- All intermediate writers are closed on failure.

---

# Workstream PC2 — Build Deterministic Route-Through Fixtures

## Objective

Add tests that cannot accidentally pass when the implementation connects directly.

## Target files

- new `python/tests/fixtures/scripted_proxy.py` or equivalent
- new `python/tests/test_pproxy_route_through.py`
- new strict paired scenarios under `python/tests/strict/`
- `scripts/strict_protocol_wire_probe.py`
- external interoperability scripts

## Fixture design

Create small scripted proxy servers that:

- listen on loopback;
- record every received byte and logical event;
- support fragmented responses;
- can return success without contacting a real target;
- can reject a handshake intentionally;
- can proxy to another local scripted server for two-hop tests;
- expose deterministic completion events to the test.

Suggested event model:

```python
@dataclass
class ProxyEvent:
    proxy_id: str
    kind: str
    host: str | None = None
    port: int | None = None
    raw: bytes = b""
```

Example expected events:

```python
assert events == [
    ProxyEvent("http-b", "connect", "127.0.0.1", socks5_a_port),
    ProxyEvent("socks5-a", "connect", "route-through.invalid", 443),
]
```

## Direct-bypass sentinel cases

Use destinations that cannot succeed directly but can be accepted symbolically by the scripted proxy.

### HTTP and SOCKS5 domain example

```text
target host: route-through.invalid
port: 443
```

The scripted proxy records the requested domain, returns handshake success, and then echoes application bytes. A direct implementation will fail DNS resolution; the correct proxy path succeeds.

### SOCKS4 IPv4 example

Use a documentation-only address that has no local listener, for example:

```text
192.0.2.123:65000
```

The scripted SOCKS4 server returns success and echoes tunneled bytes. A direct connection must fail.

## Required scenarios

1. direct connection baseline;
2. HTTP CONNECT through one proxy;
3. HTTP CONNECT rejection;
4. SOCKS4 IPv4 through one proxy;
5. SOCKS4a domain through one proxy;
6. SOCKS5 IPv4 through one proxy;
7. SOCKS5 domain through one proxy;
8. SOCKS5 username/password auth success;
9. SOCKS5 auth failure;
10. configured proxy unavailable;
11. two-hop HTTP-to-SOCKS5 or SOCKS5-to-HTTP chain;
12. cancellation during the first hop;
13. cancellation during the second-hop handshake;
14. timeout during fragmented response;
15. no direct fallback under every failure case.

## Paired oracle strategy

Each scenario must be runnable unchanged in:

- an oracle environment containing `pproxy==2.7.9`;
- a candidate environment containing the built Eggress and compatibility wheels.

Normalize nondeterministic fields such as ephemeral ports and timestamps before comparison. Preserve protocol request host, port, method, authentication method, event order, exception class, and cleanup outcome.

## Acceptance criteria

- Every supported single-hop proxy scenario passes against oracle and candidate.
- The direct-bypass sentinel fails against the pre-fix code and passes after the fix.
- The two-hop test proves both proxy processes received the expected handshakes in order.
- No test merely asserts `.jump` topology without executing traffic.
- Failure cases assert zero destination-side direct connections.
- The route-through suite has zero skips and zero xfails.

---

# Workstream PC3 — Correct `socks_address` and Address Helpers

## Objective

Match the exact public/internal pproxy address helper contract and remove the skipped address tests.

## Target files

- `python/eggress/protocol.py`
- `python/eggress/protocol.pyi`
- `python-pproxy-compat/pproxy/proto.py`
- `python/tests/test_milestone_c_properties.py`
- strict protocol probes

## Required first step: oracle probe

Before implementation, record:

- exact signature;
- synchronous versus asynchronous call kind;
- required reader methods;
- address type input values;
- IPv4 output form;
- IPv6 normalization;
- domain decoding and IDNA behavior;
- port byte order;
- truncated input exception class;
- unsupported address type exception class;
- whether bytes after the address remain unread.

Do not assume the currently skipped test’s `io.BytesIO` model is correct until compared to the oracle. Correct the tests if their model was wrong; correct the candidate if the candidate was wrong.

## Required design

Separate decoding and encoding responsibilities.

Example naming:

```python
# Public oracle-compatible helper.
async def socks_address(reader, atyp):
    ...

# Internal encoder used to construct requests.
def encode_socks_address(host, port):
    ...
```

The exact call kind of `socks_address` must follow the oracle. The example above is illustrative only.

Do not overload one function with incompatible argument shapes such as:

```python
socks_address(host, port)       # encoder
socks_address(reader, atyp)     # decoder
```

unless the oracle itself supports both forms.

## Required cases

- IPv4;
- IPv6;
- domain;
- IDNA/punycode domain;
- port 0;
- port 65535;
- empty or truncated stream;
- invalid type;
- domain length 0;
- domain length 255;
- fragmented reader data;
- trailing bytes preserved.

## Acceptance criteria

- Remove the class-level skip from `TestAddressRoundTrip`.
- The complete address suite passes in a clean installed-wheel environment.
- Oracle and candidate observations match for signature, call kind, return values, remaining buffered bytes, and exception class.
- Internal protocol request encoding uses the separately named encoder.
- Type stubs match runtime behavior.

---

# Workstream PC4 — Repair `start_server()` and Handler Invocation

## Objective

Make compatibility servers accept real client connections using the pproxy handler contract.

## Current defect

`asyncio.start_server()` invokes the client callback as:

```python
client_connected_cb(reader, writer)
```

The current pproxy-facing `stream_handler()` requires additional state such as `unix`, `lbind`, `protos`, `rserver`, `cipher`, and `sslserver`. A partial without a complete normalized state can bind successfully but fail when a client connects.

## Target files

- `python/eggress/_pproxy_proxy.py`
- `python-pproxy-compat/pproxy/server.py`
- `python/tests/test_server_lifecycle_pproxy.py`
- `python/tests/test_milestone_c_functional.py`
- strict server-internal probes

## Required design

Probe the oracle’s `start_server()` setup and normalize every handler argument.

A safe candidate pattern is:

```python
async def _client_connected(reader, writer):
    await stream_handler(
        reader,
        writer,
        unix=self.unix,
        lbind=self.lbind,
        protos=self.protos,
        rserver=normalized_rserver,
        cipher=self.cipher,
        sslserver=self.sslserver,
        **normalized_optional_args,
    )
```

Then:

```python
return await asyncio.start_server(
    _client_connected,
    host=listen_host,
    port=listen_port,
    reuse_port=reuse_port,
)
```

The agent must determine whether the oracle schedules the coroutine callback directly or wraps it in a task. Match exception and cancellation behavior.

## Argument normalization

Explicitly determine defaults for:

- `rserver`;
- `urserver` where applicable;
- `block`;
- `salgorithm`;
- `verbose`;
- `debug`;
- `auth`/users;
- `authtime`;
- `sslserver`;
- connection statistics callbacks;
- scheduling callbacks.

Do not rely on a missing required argument to be supplied later.

## Required live tests

- HTTP listener accepts a connection and completes a supported request;
- SOCKS5 listener receives greeting and CONNECT request;
- SOCKS5 listener relays data to a local echo target;
- custom `stream_handler` is invoked exactly once with the expected reader/writer;
- bind failure raises the oracle-compatible exception;
- close and repeated close work;
- cancellation cleans up client tasks;
- handler exception closes the client writer;
- no unhandled task exception remains on the loop.

## Acceptance criteria

- Remove the skip from `test_start_server_socks5_listens`.
- The SOCKS5 listener test performs a complete handshake and payload relay, not only a bind check.
- `ProxySimple.start_server()` returns the oracle-compatible server handle.
- Server callback invocation no longer raises a missing-argument `TypeError`.
- All client tasks are complete or cancelled after server shutdown.
- Clean installed-wheel tests pass without importing repository source modules.

---

# Workstream PC5 — Complete Common Protocol Semantics

## Objective

Make supported protocol methods correct beyond construction and happy-path whole-buffer tests.

## Target files

- `python/eggress/protocol.py`
- `python/eggress/protocol.pyi`
- compatibility re-exports
- `python/tests/test_protocol_behavioral.py`
- `python/tests/test_channel_relay.py`
- strict protocol differential tests

## Required source cleanup

Remove or correct comments that describe HTTP, SOCKS4, or SOCKS5 as “construction-only” when functional behavior is claimed.

## Exact signatures

Probe and match the oracle signatures for:

- `BaseProtocol.connect`;
- `HTTP.connect`;
- `Socks4.connect`;
- `Socks5.connect`;
- `guess`;
- `accept`;
- `channel`;
- `http_channel`;
- `udp_pack`;
- `udp_unpack`;
- `udp_connect`;
- `udp_accept`.

Do not use a generic `*args, **kwargs` wrapper as a substitute for the oracle signature. If an adapter requires generic dispatch internally, expose an exact wrapper or exact `__signature__` and test actual argument binding.

## Fragmentation model

Protocol parsing must not assume one `read(1024)` contains the entire request.

Use compatible helpers equivalent to:

- exact byte count reads;
- delimiter reads;
- bounded header reads;
- rollback/pushback for bytes read during protocol detection;
- preservation of post-handshake payload.

### HTTP cases

- fragmented request line;
- fragmented headers;
- CONNECT success;
- CONNECT non-2xx;
- malformed status line;
- authenticated proxy header behavior;
- absolute-form request target;
- Host header parsing;
- bytes following the header retained;
- bounded maximum header size;
- early EOF.

### SOCKS4/4a cases

- IPv4 request;
- domain request;
- user ID;
- fragmented null-terminated fields;
- command rejection;
- server success reply;
- server failure reply;
- trailing payload preservation.

### SOCKS5 cases

- fragmented greeting;
- no-auth negotiation;
- username/password negotiation;
- unsupported method response;
- IPv4, IPv6, and domain requests;
- command rejection;
- CONNECT response encoding;
- client response parsing;
- auth failure;
- UDP ASSOCIATE where already in A–C scope;
- trailing payload preservation.

## `http_channel()` requirements

Determine the exact oracle behavior and implement dedicated transformations. Expected areas to probe include:

- absolute-form to origin-form request target rewriting;
- `Proxy-Connection` removal or conversion;
- `Proxy-Authorization` removal before origin forwarding;
- Host header preservation;
- CONNECT tunnel behavior versus plain forward proxy behavior;
- response bytes and half-close handling.

The following implementation is not sufficient:

```python
async def http_channel(...):
    return await self.channel(...)
```

## Relay semantics

Audit `BaseProtocol.channel()` for:

- exception propagation versus suppression;
- `stat_conn(+1/-1)` timing;
- `stat_bytes` direction and units;
- `drain()` behavior;
- half-close;
- `write_eof()`;
- `wait_closed()`;
- cancellation;
- no-data EOF;
- peer reset.

## Acceptance criteria

- HTTP, SOCKS4/4a, and SOCKS5 supported methods no longer rely on whole-buffer assumptions.
- Byte-level paired probes pass for fragmented and malformed cases.
- `http_channel()` has dedicated tests showing required transformations.
- SOCKS5 server-side handshake sends actual method and CONNECT replies.
- Protocol accept methods preserve post-handshake bytes.
- No supported protocol test expects `NotImplementedError`.
- Unsupported protocol classes remain explicitly unsupported and separately tested.

---

# Workstream PC6 — Restore Fail-Closed Paired Evidence

## Objective

Make the paired runner incapable of converting missing or failed evidence into a compatibility pass.

## Target files

- `scripts/run_strict_pproxy_api.py`
- `scripts/run_strict_pproxy_api.sh`
- `scripts/compare_observations.py`
- `python/tests/strict/conftest.py`
- all strict probe scripts
- strict runner unit tests

## Required comparator rules

For every `closure_required = true` record:

| Oracle | Candidate | Required result |
|---|---|---|
| exists | exists, matching | pass |
| exists | missing | fail |
| missing | exists | fail |
| missing | missing | fail |
| success | error | fail |
| error | success | fail |
| error | same error | fail unless exact record is a pinned `known_upstream_defect` |
| timeout | any | fail |
| malformed output | any | fail |
| skipped | any | fail |
| missing artifact | any | fail |

Mutual absence is an inventory fact, not behavioral proof.

## Known upstream defect handling

Allow an oracle error only when all are true:

1. the manifest record status is `known_upstream_defect`;
2. the record ID is present in a version-pinned allowlist;
3. the oracle error fingerprint matches the pinned expected fingerprint;
4. the candidate behavior is compared according to the record’s documented policy;
5. the report lists the exception separately from passing compatibility records.

Do not allow arbitrary matching error strings.

## Signature comparison

Compare:

- positional-only markers;
- positional-or-keyword parameters;
- keyword-only parameters;
- varargs;
- kwargs;
- parameter names;
- defaults, including `None`, empty values, and sentinels;
- coroutine versus synchronous call kind;
- callable versus non-callable;
- property versus method;
- return annotation only when the project intentionally certifies annotations.

Remove this broad rule:

```python
if one_side_is_variadic and the_other_is_explicit:
    return True
```

A wrapper may accept more arguments and still be source-incompatible under introspection or positional binding.

If a wrapper must remain variadic internally, expose the oracle signature explicitly:

```python
wrapper.__signature__ = inspect.signature(oracle_shape_function)
```

Prefer an exact Python function definition over synthetic signatures.

## Closure-required skip behavior

`run_paired_comparison()` must increment failures when a closure-required record:

- has no extractable probe;
- is skipped by comparator type;
- has missing observation files;
- is not handled by the API runner and lacks delegated evidence.

## Delegated evidence

`protocol_wire`, external interop, process, and cipher records may be executed by specialized runners. The API runner may mark them as `delegated`, not `passed` or generic `skipped`.

A delegated result must include:

```json
{
  "id": "behavior.connection.socks5.route_through",
  "status": "delegated",
  "required_artifact": "protocol_wire/socks5_route_through.json",
  "required_runner": "strict_protocol_wire"
}
```

The final evidence aggregator must fail if that artifact is absent, stale, malformed, or bound to another commit.

## Environment verification

Record and validate:

- interpreter path;
- `sys.prefix`;
- `pproxy.__file__`;
- `eggress.__file__` for candidate;
- installed distribution names and versions;
- oracle package hash;
- candidate wheel hash;
- candidate Git commit;
- manifest hash;
- probe script hash.

The candidate is expected to import the compatibility `pproxy` package. Verification must distinguish that package from the upstream distribution by path and installed distribution metadata.

## Required unit/regression tests

- both missing fails;
- identical errors fail;
- oracle error fails;
- candidate error fails;
- timeout fails;
- malformed JSON fails;
- missing output fails;
- altered default argument fails;
- positional-only difference fails;
- coroutine-kind difference fails;
- generic variadic wrapper versus explicit signature fails;
- delegated record without artifact fails;
- contaminated oracle imports candidate package and fails;
- contaminated candidate imports upstream distribution and fails.

## Acceptance criteria

- `--closure-required` is passed by every authoritative closure invocation.
- A paired summary reports `pass`, `fail`, `delegated`, `skipped`, `oracle_error`, `candidate_error`, and `harness_error` separately.
- Closure mode exits nonzero if `skipped > 0` for an A–C required record.
- Closure mode exits nonzero if any delegated artifact is missing.
- Both-missing and identical-error regression injections fail as intended.
- No broad variadic-signature compatibility heuristic remains.

---

# Workstream PC7 — Enforce Manifest-to-Behavior Evidence Links

## Objective

Make the manifest describe structural inventory and executable behavior without conflating them.

## Target files

- `docs/parity/pproxy_2_7_9_strict_manifest.toml`
- `crates/eggress-testkit/src/strict_manifest.rs`
- `crates/eggress-testkit/src/bin/strict_report.rs`
- strict manifest tests
- new evidence-index validator

## Record model

Structural public records may remain structural, but must link to the behavioral records that certify their operational use.

Example:

```toml
[[record]]
id = "python.pproxy.Connection"
category = "python_namespace"
kind = "function"
module = "pproxy"
name = "Connection"
comparator = "method_signature"
status = "structural"
certification_scope = "structural"
closure_required = true
behavior_record = "behavior.connection.supported_route_through"
evidence_level = "structural_only"
```

Behavior record:

```toml
[[record]]
id = "behavior.connection.supported_route_through"
category = "protocol"
kind = "behavior"
module = "pproxy"
name = "Connection route-through"
comparator = "protocol_wire"
status = "drop_in"
certification_scope = "behavioral"
closure_required = true
evidence_level = "paired_oracle"
evidence_refs = [
  "protocol_wire/http_route_through.json",
  "protocol_wire/socks4_route_through.json",
  "protocol_wire/socks5_route_through.json",
  "protocol_wire/two_hop_route_through.json",
]
```

Do not upgrade the structural record to `drop_in` merely because the symbol exists.

## Validator corrections

Implement and test all of these rules:

1. `behavior_record` target exists.
2. structural closure-required public records have a behavior link unless explicitly inventory-only.
3. behavior target has behavioral, interop, or process certification scope.
4. behavior target belongs to the same or an earlier applicable milestone.
5. no circular behavior links.
6. evidence reference paths exist in the closure evidence index.
7. every required evidence artifact records the current manifest hash.
8. every required evidence artifact records the tested candidate commit.
9. every A–C closure-required behavior record has a supported runner.
10. `drop_in` behavioral records cannot use structural comparators.
11. later-milestone records are excluded from A–C closure totals.
12. the validator checks through Milestone C, not only a hard-coded current Milestone A.

Replace or parameterize the hard-coded current milestone. Preferred options:

```toml
[meta]
closure_through = "C"
```

or:

```bash
strict-report --check --through C
```

## Report semantics

Report these separately:

- structural inventory coverage;
- behavioral records required for A–C;
- behavioral records passing;
- interop records required and passing;
- process records required and passing;
- platform constraints;
- intentional non-parity;
- later-milestone gaps;
- missing evidence;
- stale evidence;
- known upstream defects.

Do not compute readiness as `terminal_status / total_records`. A structural record is neither an unresolved behavioral gap nor proof of behavior.

Suggested A–C readiness formula:

```text
passing required A–C behavior/interop/process records
----------------------------------------------------
all required A–C behavior/interop/process records
```

Structural coverage should have its own percentage.

## Report and commit binding

The checked-in report is a deterministic view of the manifest. It may be bound to the manifest hash.

Runtime closure evidence must be separately bound to:

- exact candidate commit SHA;
- exact manifest hash;
- candidate wheel hash;
- oracle wheel/sdist hash;
- runner version/hash;
- execution timestamp;
- platform and interpreter.

Do not treat a normalized commit line in the checked-in report as proof that the current code was tested.

## Acceptance criteria

- The missing behavior-link validator rule is actually executed.
- A nonexistent behavior target fails validation.
- A circular behavior link fails validation.
- A structural record linked to another structural record fails validation.
- All A–C public structural records have valid behavior mappings or explicit inventory-only classification.
- A–C readiness is derived only from required behavioral/interoperability/process evidence.
- Later D/E gaps do not block A–C but remain visible.
- `strict-report --check` validates manifest-derived freshness.
- a separate evidence validator rejects artifacts bound to another commit.

---

# Workstream PC8 — Make the Closure Audit Authoritative

## Objective

Ensure `scripts/run_strict_pproxy_closure_audit.sh` cannot pass without all required A–C evidence.

## Target files

- `scripts/run_strict_pproxy_closure_audit.sh`
- new `scripts/validate_closure_evidence.py`
- new `scripts/check_junit_no_required_skips.py`
- environment lock files if used
- test requirements lock

## Required shell behavior

Keep the result accumulator, but remove optional treatment from required gates.

Required form:

```bash
run_gate "13_paired_api_runner" \
  env ORACLE_VENV="$ORACLE_VENV" CANDIDATE_VENV="$CANDIDATE_VENV" \
  ./scripts/run_strict_pproxy_api.sh --closure-required
```

Not permitted:

```bash
run_gate_optional "13_paired_api_runner" ...
```

The same rule applies to strict differential, external TCP interop, and supported external UDP interop.

## Fresh environments

At audit start:

```bash
rm -rf target/closure-audit
rm -rf .venv-oracle-api .venv-candidate-api
mkdir -p target/closure-audit
```

Create environments deterministically and record dependency locks.

Do not reuse stale `.venv-*` directories.

## Dependency installation

Install required packages inside explicit venvs. Do not use:

```bash
pip install ... || true
```

A failed required dependency install must fail the gate.

The candidate test environment must explicitly include:

- canonical Eggress wheel;
- compatibility wheel;
- `pytest`;
- `pytest-asyncio`;
- `pytest-timeout` if timeout flags are used;
- `cryptography` where required;
- any fixture-specific dependency.

## Mandatory gate list

1. `cargo fmt --all -- --check`;
2. `cargo check --workspace --all-targets`;
3. `cargo clippy --workspace --all-targets -- -D warnings`;
4. `cargo test --workspace`;
5. `cargo deny check`;
6. `cargo audit`;
7. strict manifest tests;
8. strict report freshness;
9. release/document consistency;
10. exact oracle provenance/hash validation;
11. canonical wheel build;
12. compatibility wheel build;
13. clean installed-wheel Python suite;
14. paired API runner with `--closure-required`;
15. strict differential suite using fresh paired observation directories;
16. route-through protocol-wire suite;
17. external TCP interoperability for supported A–C paths;
18. external UDP interoperability for supported A–C paths;
19. cipher KAT and pproxy interop for supported ciphers;
20. plugin transformed-traffic probe;
21. server lifecycle and process probe;
22. runtime failure and cleanup probe;
23. resource-leak probe;
24. supported-path skip/xfail audit;
25. evidence index validation;
26. commit/manifest/wheel hash binding;
27. regression-injection demonstration.

## Strict differential invocation

The audit must pass explicit directories:

```bash
"$CANDIDATE_VENV/bin/python" -m pytest python/tests/strict -q \
  --oracle-observations-dir "$OBS_DIR/oracle" \
  --candidate-observations-dir "$OBS_DIR/candidate" \
  --strict-markers \
  --junitxml "$AUDIT_DIR/junit-strict.xml"
```

A missing directory must be a harness error, not a skip.

Change the fixture used in closure mode from:

```python
pytest.skip("observation directories required")
```

to a hard failure such as:

```python
pytest.fail("closure mode requires oracle and candidate observation directories")
```

Candidate-only developer runs may retain a separate non-closure fixture mode, but the authoritative command must be strict.

## Skip policy

The complete repository test suite may contain legitimate platform or later-milestone skips. The A–C closure subset must contain zero skips and zero xfails.

Generate JUnit XML and validate:

- no skipped A–C closure test;
- no xfailed A–C closure test;
- no deselected required node ID;
- every manifest `test_ref` appears in executed test evidence.

## Evidence output

Required directory structure:

```text
target/closure-audit/
  CLOSURE_AUDIT_REPORT.md
  acceptance_matrix.json
  environment/
  junit/
  paired_observations/
  protocol_wire/
  interop/
  process/
  cleanup/
  evidence/
    candidate_commit.sha
    manifest.sha256
    candidate_wheel.sha256
    compat_wheel.sha256
    oracle_artifact.sha256
    evidence_index.json
```

## Final exit behavior

The audit passes only when:

```text
failed == 0
skipped_required == 0
missing_required_artifacts == 0
stale_required_artifacts == 0
```

Do not print “AUDIT PASSED” when optional or skipped entries correspond to A–C requirements.

## Acceptance criteria

- No required A–C gate uses `run_gate_optional`.
- Missing pproxy oracle installation fails.
- Missing candidate wheel fails.
- Missing observation directories fail.
- One skipped closure test fails.
- One missing protocol-wire artifact fails.
- One stale commit hash fails.
- Paired runner and strict differential tests execute within the same authoritative audit or consume verified fresh artifacts from that exact audit invocation.
- The final report lists exact command, exit code, duration, log path, artifact path, commit, and manifest hash for every gate.

---

# Workstream PC9 — Correct Hosted Workflow Execution

## Objective

Run the authoritative closure gate in GitHub Actions and retain trustworthy evidence.

## Target files

- `.github/workflows/strict-differential.yml`
- related reusable workflows if introduced
- workflow lint configuration

## Preferred workflow architecture

Use one authoritative closure job that executes the complete local closure script from a clean checkout. Other jobs may provide faster feedback but must not substitute for the authoritative result.

Example:

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
        name: strict-closure-${{ github.sha }}
        path: target/closure-audit/
        if-no-files-found: error
        retention-days: 90
```

If jobs remain split, the closure job must download the paired artifacts and validate:

- artifact commit SHA equals `${{ github.sha }}`;
- manifest hash equals the checkout manifest;
- artifact runner version matches checkout scripts;
- no required artifact is missing.

## Required workflow corrections

1. Paired observations use `if-no-files-found: error`, not `warn`.
2. Strict tests receive explicit observation directories.
3. The paired runner uses `--closure-required`.
4. The closure job does not rerun weaker commands than the local authoritative audit.
5. All invoked Python executables have their dependencies explicitly installed.
6. Required artifacts upload even after failure using `if: always()`.
7. The artifact bundle contains logs for failed gates.
8. The workflow exposes a visible required commit status.

## Python/platform coverage

For A–C source compatibility:

- run candidate structural and unit tests on the declared supported Python versions;
- run the expensive paired and external interoperability closure on one canonical Python version, currently 3.11 unless project policy selects another;
- retain existing Windows/macOS checks for platform-specific regressions;
- do not claim a Python version as supported if no clean installed-wheel test runs there.

## Hosted blocker

If repository billing or Actions availability prevents execution:

- local status may become `LOCAL GATES PASS — HOSTED EVIDENCE PENDING`;
- Milestone A remains open;
- do not redefine hosted evidence as optional;
- the code and workflow should be ready to pass without another corrective code change once the external blocker is removed.

## Acceptance criteria

- Workflow syntax/lint passes.
- Every command invoked by the workflow has an explicitly installed executable.
- Missing paired observations fails artifact upload.
- The closure job fails if strict differential tests skip.
- The closure job fails if external supported-path TCP or UDP interop fails.
- A successful hosted run provides a visible status and a downloadable evidence bundle named with the exact commit SHA.

---

# Workstream PC10 — Regression Injection and Negative Proof

## Objective

Demonstrate that the new gates detect the exact defects that previously escaped.

## Target files

- `scripts/demonstrate_regression_injections.sh`
- helper patch fixtures under `tests/regression_injections/`
- closure audit

## Required injection cases

Each injection must modify a temporary worktree or temporary copied file, run the narrow relevant gate, assert that the gate fails, and restore the repository automatically.

1. Change `ProxySimple.tcp_connect()` to open the final target directly.
2. Make `_build_remote_uri()` stringify a nested jump and use it for execution.
3. Revert `socks_address` to encoder-only behavior.
4. Remove normalized `rserver` from the server callback.
5. Make `http_channel()` delegate to `channel()` without transformations.
6. Return success for both-missing observations.
7. Return success for identical oracle/candidate errors.
8. Restore the variadic-signature superset heuristic.
9. Delete one required paired observation.
10. Delete one required protocol-wire artifact.
11. mark one closure-required test skipped.
12. bind an artifact to a different commit SHA.
13. omit `--closure-required` from the paired invocation.
14. make a required interop gate optional.
15. point the oracle environment at the compatibility package.
16. point the candidate environment at upstream pproxy.

## Example injection pattern

```bash
run_injection "direct_bypass" \
  apply_patch tests/regression_injections/direct_bypass.patch \
  --expect-failure \
  "$CANDIDATE_PYTHON -m pytest python/tests/test_pproxy_route_through.py -q"
```

The injection harness itself must fail if the injected defect is not detected.

## Acceptance criteria

- All required injections demonstrate a nonzero relevant gate.
- No injected change remains in the working tree.
- The injection report records the expected and actual failing gate.
- The authoritative closure audit retains the injection report.

---

# Workstream PC11 — Final Status and Completion Evidence

## Objective

Update milestone status only after machine-verifiable closure.

## Local closure status

After all local required gates pass, update status to:

`LOCAL GATES PASS — HOSTED EVIDENCE PENDING`

Do not mark A, B, or C complete yet.

## Hosted closure status

After a successful hosted run on the exact candidate commit:

1. retain the workflow run URL/identifier;
2. retain artifact identifiers;
3. record candidate commit SHA;
4. record manifest hash;
5. record oracle artifact hash;
6. record candidate and compatibility wheel hashes;
7. generate the acceptance matrix from evidence;
8. update milestone plans to complete only if every required criterion passes;
9. create a new completion record rather than editing historical completion claims into correctness.

## Completion document requirements

The final document must be generated or verified against machine-readable evidence and include:

- exact tested commit;
- exact oracle;
- exact workflow run;
- gate totals;
- zero required skips;
- structural coverage count;
- behavioral closure count;
- interop closure count;
- deferred later-milestone records;
- intentional non-parity records;
- known upstream defects;
- artifact hashes.

It must not use manually asserted test counts copied from console output without retained logs.

## Acceptance criteria

- Local completion cannot set A–C to complete.
- Hosted completion references the exact commit and artifacts.
- README, plans, manifest, report, workflow status, and completion record agree.
- No prior superseded completion record is reused as the final authority.

---

# 6. Required implementation sequence

Use the following order. A smaller implementation model should not work ahead of the current commit boundary.

## Commit 1 — Truth and regression inventory

- PC0 status normalization;
- supported skip inventory;
- superseding notice where required;
- no runtime changes.

## Commit 2 — Deterministic route-through fixtures

- scripted HTTP/SOCKS fixtures;
- direct-bypass sentinel scenarios;
- two-hop event harness;
- tests may be committed with the runtime fix if repository policy requires every commit to remain green.

## Commit 3 — Proxy object execution correction

- PC1 recursive object-graph execution;
- single-hop route-through;
- no direct fallback;
- cleanup and accounting;
- route-through tests green.

## Commit 4 — Address helper correction

- PC3 oracle probe;
- exact `socks_address` behavior;
- separate encoder;
- remove address-test skip.

## Commit 5 — Server callback correction

- PC4 normalized handler adapter;
- live HTTP and SOCKS5 listener behavior;
- remove SOCKS5 listener skip;
- lifecycle cleanup.

## Commit 6 — Protocol fragmentation and HTTP channel

- PC5 exact signatures;
- fragmented reads;
- rollback;
- server replies;
- `http_channel()` transformations;
- UDP corrections within current scope.

## Commit 7 — Fail-closed paired runner

- PC6 comparator rules;
- strict signature behavior;
- delegated artifact model;
- environment contamination checks;
- negative unit tests.

## Commit 8 — Manifest and report enforcement

- PC7 behavior links;
- validator rule execution;
- closure-through-C logic;
- readiness calculation;
- manifest-derived report correction.

## Commit 9 — Authoritative closure audit

- PC8 mandatory gates;
- fresh environments;
- strict observation directories;
- skip/JUnit checks;
- evidence index and binding.

## Commit 10 — Hosted workflow correction

- PC9 authoritative job;
- explicit dependencies;
- mandatory artifact retention;
- strict paired invocation.

## Commit 11 — Regression injection proof

- PC10 injection harness;
- retained injection report;
- all injections detected.

## Commit 12 — Local closure evidence

- execute the complete audit;
- retain local artifacts outside source control as appropriate;
- update status only to `LOCAL GATES PASS — HOSTED EVIDENCE PENDING` if every local requirement passes.

## Commit 13 — Hosted closure record

Only after a successful hosted run:

- generate completion record;
- bind it to commit and artifacts;
- update A–C status to complete;
- leave D–F unchanged.

Do not combine Commit 1 with implementation changes. Reviewers need an honest baseline.

---

# 7. Mandatory acceptance matrix

## Milestone A — Honest compatibility contract

- [ ] Oracle package/version is pinned and hash-validated.
- [ ] Oracle and candidate import roots are verified.
- [ ] Candidate wheel and compatibility wheel hashes are retained.
- [ ] Both-missing fails for closure-required records.
- [ ] Identical errors fail unless a pinned known-upstream-defect rule applies.
- [ ] Oracle error, candidate error, timeout, malformed output, and missing artifact fail.
- [ ] Variadic wrappers are not automatically accepted as exact signatures.
- [ ] Structural records link to executable behavior records.
- [ ] Behavior links are validated for existence and scope.
- [ ] Evidence artifacts are bound to the exact candidate commit and manifest hash.
- [ ] A–C readiness is computed from required behavioral/interop/process records.
- [ ] Required skips cause closure failure.
- [ ] Hosted CI retains mandatory evidence.

## Milestone B — Python source and object compatibility

- [ ] Top-level aliases and signatures match the oracle.
- [ ] `proxies_by_uri()` produces the correct nested object graph.
- [ ] `DIRECT` is used as the terminal route to a proxy endpoint, not as permission to bypass the proxy.
- [ ] direct TCP works.
- [ ] direct UDP works where currently supported.
- [ ] single-hop HTTP traverses the configured proxy.
- [ ] single-hop SOCKS4 traverses the configured proxy.
- [ ] single-hop SOCKS4a traverses the configured proxy.
- [ ] single-hop SOCKS5 traverses the configured proxy.
- [ ] supported SOCKS5 authentication works.
- [ ] supported SOCKS5 UDP works through the proxy.
- [ ] one two-hop chain executes both proxy handshakes in order.
- [ ] proxy failure never falls back to direct destination access.
- [ ] returned reader/writer objects satisfy required asyncio behavior.
- [ ] `start_server()` returns the oracle-compatible handle.
- [ ] clean installed-wheel Python suite passes.
- [ ] authored client/server scenarios run unchanged in oracle and candidate environments.

## Milestone C — Functional internal API

- [ ] shared `AuthTable` semantics remain correct.
- [ ] `socks_address` matches the oracle contract.
- [ ] internal address encoding is separated from public decoding behavior.
- [ ] `stream_handler` can be invoked by an actual asyncio server callback.
- [ ] `datagram_handler` performs supported real relay.
- [ ] HTTP client handshake works.
- [ ] HTTP server parsing and replies work.
- [ ] SOCKS4/4a client handshake works.
- [ ] SOCKS4/4a server parsing and replies work.
- [ ] SOCKS5 client handshake works.
- [ ] SOCKS5 server method negotiation, auth, request parsing, and replies work.
- [ ] fragmented handshakes are handled.
- [ ] rollback preserves bytes during protocol detection.
- [ ] post-handshake payload bytes are preserved.
- [ ] raw channel relays without a statistics callback.
- [ ] `http_channel()` performs required transformations.
- [ ] supported TLS wrapper behavior matches the oracle.
- [ ] supported cipher KAT and interop pass.
- [ ] unsupported ciphers remain explicitly classified and are not closed by registry presence.
- [ ] plugin lifecycle transforms real traffic.
- [ ] failure and cancellation cleanup closes all resources.
- [ ] no supported behavior is closed by expected `NotImplementedError`.
- [ ] no supported behavior test is skipped or xfailed.

## Cross-cutting closure

- [ ] paired API is mandatory.
- [ ] strict differential is mandatory.
- [ ] supported external TCP interop is mandatory.
- [ ] supported external UDP interop is mandatory.
- [ ] closure-required delegated evidence is mandatory.
- [ ] missing observation directories fail.
- [ ] JUnit evidence shows zero required skips.
- [ ] regression injections are detected.
- [ ] workflow artifacts use `if-no-files-found: error`.
- [ ] local status remains pending hosted evidence until Actions succeeds.

---

# 8. Required verification commands

The final scripts may wrap these commands, but equivalent gates must run.

```bash
# Rust quality gates
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo audit

# Strict manifest and report
cargo test -p eggress-testkit strict_manifest
cargo test -p eggress-testkit strict_report
cargo run -p eggress-testkit --bin strict-report -- --check
python3 scripts/check_release_docs.py

# Fresh environments
rm -rf .venv-oracle-api .venv-candidate-api target/closure-audit

# Paired API with mandatory closure semantics
./scripts/run_strict_pproxy_api.sh --closure-required

# Candidate installed-wheel suite
.venv-candidate-api/bin/python -m pytest python/tests -q \
  --strict-markers \
  --junitxml target/closure-audit/junit/candidate.xml

# Strict paired suite with explicit artifacts
.venv-candidate-api/bin/python -m pytest python/tests/strict -q \
  --oracle-observations-dir target/closure-audit/paired_observations/oracle \
  --candidate-observations-dir target/closure-audit/paired_observations/candidate \
  --strict-markers \
  --junitxml target/closure-audit/junit/strict.xml

# Route-through behavior
.venv-candidate-api/bin/python -m pytest \
  python/tests/test_pproxy_route_through.py \
  python/tests/test_server_lifecycle_pproxy.py \
  -q --strict-markers

# Supported external interop
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/run_strict_pproxy_interop.sh
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/compat_udp_pproxy.sh

# Evidence validation
python3 scripts/validate_closure_evidence.py \
  --through C \
  --audit-dir target/closure-audit \
  --require-zero-skips

# Regression injections
./scripts/demonstrate_regression_injections.sh

# Authoritative final gate
./scripts/run_strict_pproxy_closure_audit.sh
```

The authoritative script must run all required gates itself. Operators must not need to remember separate commands for closure.

---

# 9. Prohibited completion shortcuts

The implementation is not complete if any of the following is used:

- adding or retaining `pytest.mark.skip` for a supported A–C defect;
- converting a failure to `xfail`;
- treating matching exceptions as compatibility;
- treating both missing symbols as compatibility;
- treating `*args, **kwargs` as automatically source-compatible;
- marking protocol-wire records passed because their modules import;
- making route-through or interop gates optional;
- running strict tests without observation directories;
- using a stale candidate venv;
- importing repository source instead of the installed wheel in closure tests;
- using registry cardinality as cipher evidence;
- claiming chain support from `.jump` topology alone;
- using `str()` or `repr()` to execute a chain;
- falling back to direct access when an upstream fails;
- marking A–C complete before hosted evidence exists;
- manually editing a completion checklist without generated supporting artifacts.

---

# 10. Handoff guidance for a smaller implementation model

1. Start with PC0 and PC2. Make the current defects observable with deterministic tests.
2. Do not begin comparator or documentation work until the route-through tests prove the runtime path.
3. For PC1, reason in terms of three addresses:

   - final destination;
   - current proxy endpoint;
   - next-hop proxy endpoint.

   Confusing these is the source of the current direct-bypass defect.

4. For every `ProxySimple.tcp_connect(final_host, final_port)` call, ask:

   - Which object opens the physical socket?
   - Which endpoint does that socket reach?
   - Which protocol object writes the next handshake?
   - Which host/port appears inside that handshake?

5. Use the event-recording fixtures. Do not infer routing success from returned bytes alone.
6. Before implementing `socks_address`, run the oracle probe and freeze its observation.
7. Before modifying signatures, compare `inspect.signature()` and actual argument binding in both environments.
8. When a test uncovers a mismatch, fix the implementation or classify it as a later-milestone gap. Do not skip it.
9. Keep all A–C required evidence in one closure directory and validate hashes at the end.
10. Update status only after the authoritative audit succeeds.

The highest-priority defect is the single-hop direct bypass. Milestone B cannot close until a proxy URI demonstrably causes traffic to reach that proxy.

---

# 11. Reviewer checklist

A reviewer should be able to answer these questions using retained evidence alone:

1. Which exact pproxy artifact was used as the oracle?
2. Which exact Eggress commit and wheel were tested?
3. Did a single-hop HTTP connection reach the HTTP proxy?
4. Did a single-hop SOCKS4/4a connection reach the SOCKS proxy?
5. Did a single-hop SOCKS5 connection reach the SOCKS proxy?
6. Did a two-hop chain execute both handshakes in order?
7. Was direct destination access impossible in the route-through sentinel tests?
8. Does `socks_address` match the oracle?
9. Did a real SOCKS5 listener accept and relay a client connection?
10. Did strict comparison fail on both-missing and identical-error injections?
11. Were any required tests skipped or xfailed?
12. Did all delegated evidence artifacts exist and match the candidate commit?
13. Did external supported-path TCP and UDP interop run?
14. Did cleanup and resource checks pass?
15. Did hosted CI retain the evidence bundle?

If any answer is unavailable, Milestones A–C are not closed.
