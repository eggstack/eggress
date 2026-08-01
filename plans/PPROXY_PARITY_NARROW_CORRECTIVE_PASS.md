# pproxy Practical Parity — Narrow Corrective Pass

## Status

Proposed for execution. Required before Phase 6 may return to `Completed`.

## Parent roadmap

`plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`

## Baseline

This plan was written against repository head:

```text
b956b29090d7bfcb25ee8ac0d45182bf472d3383
```

The Phase 0–6 implementation sequence is substantially present, but a post-closure source audit found five bounded defects where compatibility syntax, translation, runtime behavior, or Python orchestration do not connect correctly.

## Objective

Repair only the following confirmed defects:

1. canonical pproxy fixed-target URI syntax is not accepted;
2. `echo://` runtime support is unreachable through the compatibility parser;
3. fixed-target TCP listener translation discards the target while configuring UDP;
4. outbound local binding is supported by the native chain executor but rejected by the compatibility translator;
5. the Python compatibility server path can perform an upstream handshake twice;
6. `httponly` is accepted as a listener even though the supported compatibility role is upstream/client-side request forwarding.

Items 1 and 2 share the parser boundary, so the pass contains six corrections across five implementation workstreams.

The desired result is a small wiring correction, not another parity roadmap, protocol expansion, Python-runtime rewrite, or certification project.

## Scope guardrails

### In scope

- exact pproxy 2.7.9 syntax probes for the affected URI forms;
- compatibility parser corrections;
- translator-to-native configuration wiring;
- preservation of existing native fixed-target and local-bind runtime paths;
- one correction to the public Python server connection flow;
- focused parser, translation, runtime, and Python regression tests;
- updates to the practical compatibility matrix and phase status after verified closure.

### Out of scope

- SSH;
- QUIC or HTTP/3;
- ShadowsocksR;
- legacy Shadowsocks stream ciphers or OTA;
- plugin execution;
- daemonization;
- connection pooling or `--reuse`;
- general multi-hop UDP;
- macOS PF destination recovery;
- backward TLS or expanded reverse composition;
- implementation of private `pproxy.server` helpers such as `stream_handler`, `datagram_handler`, or `test_url` merely because they exist;
- replacement of the current Python compatibility object model with a new service architecture;
- new hosted-CI jobs, evidence archives, test matrices, or release certification scripts;
- unrelated refactoring of `eggress-uri`, `eggress-core`, or `eggress-server`.

## Governing implementation rule

Every correction must be verified across the narrowest complete path:

```text
pproxy input
  -> compatibility parser
  -> compatibility translator or Python factory
  -> native typed configuration/runtime path
  -> loopback observable behavior
```

A parser-only or TOML-string-only test is insufficient for a capability whose defect occurs after parsing.

## Confirmed defect inventory

### Defect A — Canonical fixed-target syntax is rejected

The compatibility parser currently expects a fixed target around the endpoint after `://`, for example:

```text
raw://{127.0.0.1:9000}
```

The pproxy 2.7.9 syntax places the brace target in the protocol expression, for example:

```text
tunnel{127.0.0.1:9000}://:1080
```

and may compose it with another protocol and modifiers, for example:

```text
trojan+tunnel{127.0.0.1:9000}+ssl://password@proxy.example:443
```

The current token validator sees `tunnel{...}` as an unknown scheme.

### Defect B — `echo://` is rejected before runtime

The runtime contains explicit TCP and UDP echo behavior and the translator contains an `echo` branch, but `echo` is missing from the compatibility parser's known protocol set.

### Defect C — Fixed-target TCP is cleared during listener translation

The translator initially places the target in the listener's `fixed_target` field, then installs UDP fixed-target configuration and clears the TCP field. The raw TCP accept path requires `fixed_target`, so the generated listener can reach UDP while the TCP path fails before relay.

The translator also currently infers UDP configuration merely from a fixed-target raw/tunnel listener. That behavior must be checked against pproxy rather than assumed.

### Defect D — Local binding is rejected despite native support

`PproxyUri.local_bind` is parsed, `ProxyHopSpec.local_bind` exists, and `ChainExecutor` applies it through `ConnectOptions`. The compatibility translator nevertheless emits an unsupported diagnostic and drops the upstream whenever `local_bind` is present.

### Defect E — Python server upstream handshake can execute twice

The Python compatibility server flow calls `roption.open_connection()` and then calls `roption.prepare_connection()`.

For `ProxySimple`, inherited `open_connection()` dispatches to the overridden `tcp_connect()`, which already connects to the proxy and invokes `prepare_connection()`. The server flow then invokes the handshake a second time on the established stream.

Direct tests do not reveal this because `DIRECT` has no proxy handshake.

### Defect F — `httponly` is accepted as a listener

The native `HttpOnlyHopHandler` correctly represents an upstream request adapter. The compatibility translator also accepts `httponly` in the listener protocol allowlist and lowers it to HTTP. The corrective pass must preserve the upstream role and reject the unsupported listener role with a specific diagnostic.

## Workstream 1 — Correct the affected URI grammar

### Goal

Represent canonical fixed-target protocol tokens and `echo` without weakening existing URI validation.

### Primary files

- `crates/eggress-pproxy-compat/src/uri.rs`
- `crates/eggress-pproxy-compat/src/tests.rs`
- `tests/compat/fixtures/pproxy_phase1_uri_cli.toml`

### Tasks

1. Add one small oracle/source probe for the exact pproxy 2.7.9 forms used by:
   - fixed-target TCP tunnel;
   - fixed-target UDP tunnel;
   - fixed target composed with a proxy protocol and TLS;
   - TCP and UDP echo listeners.
2. Record the observed examples in a focused test comment or the existing compatibility fixture. Do not create a new oracle framework.
3. Replace simple `scheme_part.split('+')` token handling with a bounded token parser that can recognize:
   - a plain protocol token such as `http` or `socks5`;
   - a protocol token carrying one brace target, such as `tunnel{host:port}`;
   - transport modifiers `tls`, `ssl`, `secure`, and repeated `in`;
   - protocol composition where only the target-capable token owns the brace target.
4. Store the brace contents in `PproxyUri.fixed_target` and store the base token (`tunnel` or the exact observed equivalent) in `protocol_chain`.
5. Reject:
   - unmatched braces;
   - nested braces;
   - more than one fixed-target token in a hop;
   - an empty target;
   - a target on a protocol for which pproxy does not accept it;
   - ambiguous conflicting target placement between the scheme token and endpoint extension.
6. Add `echo` to the recognized compatibility protocol set.
7. Preserve existing non-canonical `raw://{host:port}` parsing only as a documented compatibility extension if existing users/tests rely on it. Canonical pproxy syntax must be the preferred generated and documented form.
8. Keep credentials redacted when brace-target URIs appear in errors or diagnostics.
9. Ensure `parse_pproxy_chain()` handles canonical fixed-target syntax inside `__` chains without splitting inside braces.

### Acceptance criteria

- canonical fixed-target syntax parses into the correct base protocol and `fixed_target`;
- a composed fixed-target/TLS URI preserves protocol order, TLS state, endpoint, credentials, and target;
- `echo://` parses as a known protocol;
- malformed braces fail with `InvalidUri`, not `UnsupportedProtocol` or a panic;
- unsupported known target compositions fail with a precise compatibility diagnostic;
- existing HTTP/SOCKS, IPv6, fragment-auth, rule, plugin-metadata, and chain tests remain unchanged or pass without semantic regression;
- no credential or sensitive Unix path is exposed by new diagnostics.

### Focused tests

Add table-driven cases for at least:

```text
tunnel{127.0.0.1:9000}://:1080
tunnel{[::1]:9000}://:1080
trojan+tunnel{127.0.0.1:9000}+ssl://password@proxy.example:443
echo://127.0.0.1:0
raw://{127.0.0.1:9000}              # legacy extension, if retained
tunnel{}://:1080                     # reject
tunnel{a}:tunnel{b}://:1080          # reject/appropriate malformed equivalent
```

Use the exact syntax established by the oracle probe when it differs from these illustrative forms.

## Workstream 2 — Separate fixed-target TCP and UDP translation

### Goal

Keep the fixed target available to the TCP raw/tunnel listener and configure UDP only when pproxy input actually requests a UDP listener path.

### Primary files

- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-config/src/model.rs`
- `crates/eggress-config/src/compile.rs`
- `crates/eggress-server/src/accept.rs`
- `crates/eggress-runtime/src/supervisor.rs`
- `crates/eggress-udp/src/lib.rs`
- focused tests in the owning crates

The shared runtime files should change only if the current typed fields cannot express the correct split. Prefer a translator-only correction.

### Tasks

1. Establish from the pproxy 2.7.9 probe whether the canonical fixed-target listener form starts:
   - TCP only;
   - UDP only when paired with `-ul`/UDP configuration;
   - or both roles by default.
2. Encode that observed role explicitly. Do not infer UDP solely because `fixed_target` exists.
3. Preserve `listener_entry.fixed_target` for the TCP raw/tunnel path.
4. Remove the unconditional sequence that assigns `udp.mode = fixed_target` and then clears the listener TCP target.
5. When UDP fixed-target mode is requested, populate `listener_entry.udp.fixed_target` independently. Do not transfer ownership away from the TCP field unless the observed pproxy form is UDP-only.
6. Reject attempts by the client to override the configured target.
7. Ensure a canonical fixed-target URI translates through:
   - compatibility AST;
   - generated TOML;
   - `ConfigFile` deserialization;
   - config validation;
   - config compilation;
   - runtime listener setup.
8. Keep general multi-hop UDP excluded. A fixed-target datagram relay is not permission to broaden UDP chain support.
9. Preserve existing packet-size, private-egress, association, cancellation, and idle-time controls.
10. Update generated diagnostics so a fixed-target listener is not described as fully supported unless its requested TCP/UDP role actually starts.

### Acceptance criteria

- a canonical fixed-target TCP listener relays a loopback echo payload;
- the compiled `ConnectionConfig.fixed_target` is non-`None` for the TCP path;
- fixed-target UDP relays one loopback datagram only when the requested compatibility form includes the UDP role;
- TCP support does not disappear when UDP support is enabled;
- clients cannot select another destination;
- no general multi-hop UDP capability is introduced;
- malformed or unresolved targets fail at a deterministic stage with a useful error.

### Required regression tests

1. Parser-to-TOML assertion using canonical pproxy syntax.
2. TOML-to-compiled-config assertion that separately checks:
   - TCP `fixed_target`;
   - optional UDP `fixed_target`.
3. TCP loopback runtime relay.
4. UDP loopback runtime relay for the exact supported form.
5. Negative test proving client-supplied destination bytes cannot replace the configured target.

Do not retain a test that only checks for `raw://target:port` in a generated string while bypassing the listener execution path.

## Workstream 3 — Connect local binding through the translator

### Goal

Carry pproxy local/source binding from the compatibility URI into the already implemented native `ProxyHopSpec.local_bind` path.

### Primary files

- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-uri/src/lib.rs` only if formatting/parsing cannot round-trip the current typed field
- `crates/eggress-core/src/chain.rs` only for a confirmed execution defect
- focused compatibility and core tests

### Tasks

1. Remove the blanket `local-bind` unsupported diagnostic for supported TCP upstreams.
2. Preserve `PproxyUri.local_bind` when building each native upstream hop.
3. Prefer emitting the existing native trailing local-bind syntax accepted by `eggress-uri`; do not add a second configuration field or parser unless necessary.
4. Confirm correct placement relative to credentials and `?rule=`. Add a round-trip test through `eggress_uri::parse_proxy_chain()`.
5. Apply local binding to the first physical outbound connection in a chain. Do not claim that a source bind is independently applicable after every logical handshake unless the runtime truly creates a new socket at that point.
6. Define supported values narrowly:
   - IP address;
   - optionally IP plus port only if both pproxy and native `ConnectOptions` support it;
   - IPv4/IPv6 family-compatible binding.
7. Reject hostnames, malformed addresses, and family mismatches with a pre-connect error.
8. Preserve local bind on direct, HTTP, SOCKS4/4a, SOCKS5, H2, WS/WSS, raw/tunnel, Trojan, Shadowsocks, `httponly`, and Unix only where the underlying connection type can use an IP source bind.
9. For Unix-domain upstreams, either ignore no bind only when absent or return a role-specific error when a TCP/IP local bind is supplied. Do not silently accept an unusable option.
10. Ensure diagnostics redact credentials while retaining the operator-supplied bind value where useful.

### Acceptance criteria

- a supported pproxy upstream URI with local bind translates without an unsupported marker;
- the generated native chain parses with `ProxyHopSpec.local_bind` populated;
- a loopback TCP test observes the requested source address where the OS permits it;
- an unavailable or family-mismatched address fails before proxy protocol handshake;
- upstream behavior without local bind is unchanged;
- no global socket or process networking state is modified.

### Focused tests

- IPv4 loopback source bind through direct or SOCKS5 upstream;
- IPv6 case when loopback IPv6 is available, otherwise a small unit-level family validation test;
- malformed bind rejection;
- local bind with credentials and rule suffix;
- Unix upstream plus incompatible IP bind rejection.

## Workstream 4 — Correct role classification for `echo` and `httponly`

### Goal

Expose `echo` through its actual listener roles and keep `httponly` restricted to its upstream request-adapter role.

### Primary files

- `crates/eggress-pproxy-compat/src/uri.rs`
- `crates/eggress-pproxy-compat/src/translate.rs`
- relevant compatibility/runtime tests

### Tasks

1. Add `echo` to the parser and listener role table.
2. Confirm pproxy TCP/UDP echo construction and defaults with the same bounded oracle probe used by Workstream 1.
3. Map TCP echo to `ProtocolId::Echo` without adding a default route or outbound connection.
4. Map UDP echo only when the observed pproxy input requests it.
5. Confirm TCP echo preserves EOF and does not open a target connection.
6. Remove `httponly` from the listener allowlist.
7. For a `httponly` listener, emit a specific `unsupported-role` diagnostic stating that Eggress supports it as an upstream request adapter only.
8. Keep `httponly` in supported upstream protocol handling and retain the existing request-target rewrite path.
9. Add a test that an HTTP CONNECT upstream remains distinct from `httponly` forwarding.

### Acceptance criteria

- canonical TCP echo starts and returns bytes until EOF;
- canonical UDP echo returns a loopback datagram when requested;
- `echo` is not reported as unknown;
- `httponly` listener input fails before runtime with an accurate role diagnostic;
- `httponly` upstream GET and POST continue to work;
- ordinary HTTP listener and HTTP CONNECT upstream behavior is unchanged.

## Workstream 5 — Remove the Python double-handshake path

### Goal

Ensure a Python compatibility server routes through an HTTP or SOCKS upstream with exactly one proxy handshake.

### Primary files

- `python/eggress/_pproxy_proxy.py`
- `python/pproxy/server.py`
- `python/tests/test_pproxy_public_namespace.py`
- existing server lifecycle/route-through tests

### Scope boundary

This workstream does not redesign the Python package or implement private pproxy internals. It corrects one public orchestration defect in the existing object model.

### Tasks

1. Probe or inspect pproxy 2.7.9 to establish the intended contracts of:
   - `open_connection()`;
   - `tcp_connect()`;
   - `prepare_connection()`.
2. Choose one invariant and document it in code:
   - either `open_connection()` returns a raw transport and `prepare_connection()` performs the proxy handshake once;
   - or `tcp_connect()`/`open_connection()` returns a destination-ready stream and the caller must not prepare it again.
3. Apply that invariant consistently to `ProxyDirect`, `ProxySimple`, and `_eggress_stream_handler`.
4. Avoid a compatibility fix that changes direct `Connection.tcp_connect()` behavior or its reader/writer return contract.
5. Ensure nested `jump` handling still prepares each configured proxy hop exactly once and terminates at `DIRECT`.
6. On handshake failure:
   - close the writer once;
   - await `wait_closed()` where available;
   - do not attempt the next preparation step;
   - preserve the original exception category where practical.
7. Keep unsupported SSH/QUIC/H3 execution failures explicit.
8. Do not implement currently private `stream_handler`, `datagram_handler`, `sslwrap`, or H2/QUIC internals as part of this correction.

### Acceptance criteria

- direct `await pproxy.Connection("direct://").tcp_connect()` still relays an echo payload;
- a Python compatibility server configured with one HTTP upstream relays one request/echo path successfully;
- the upstream observes exactly one HTTP CONNECT handshake;
- the equivalent SOCKS5 path observes exactly one greeting/connect sequence;
- an authenticated upstream succeeds with the expected credentials once;
- a rejected handshake closes cleanly without a second handshake attempt;
- a two-hop supported chain prepares each hop once;
- no temporary listener is introduced for outbound `Connection.tcp_connect()`.

### Required tests

Add focused test fixtures that count handshakes rather than only testing final bytes:

1. HTTP upstream fixture increments a CONNECT counter and fails if it receives a second CONNECT on the established tunnel.
2. SOCKS5 upstream fixture counts greeting and CONNECT requests.
3. One failure-path fixture checks cleanup.
4. Retain the existing direct TCP and UDP public tests.

Keep these tests local and deterministic; do not add an external proxy service.

## Workstream 6 — Reconcile closure documentation after implementation

### Goal

Return Phase 6 to `Completed` only after the affected observable paths pass focused tests.

### Primary files

- `plans/PPROXY_PARITY_PHASE_6_CLOSURE.md`
- `plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/parity/PPROXY_CLOSURE_SCENARIOS.md` only when the scenario list changes
- current README/architecture documents only where they contain an affected claim

### Tasks

1. Keep Phase 6 marked reopened while this plan is active.
2. Update only matrix rows affected by this pass:
   - canonical fixed-target grammar;
   - echo;
   - fixed-target TCP/UDP;
   - local bind;
   - `httponly` role;
   - Python `Server` through HTTP/SOCKS upstream.
3. Use `matched` only when the narrowly defined behavior has an oracle/interoperability comparison or direct equivalent observation.
4. Use `supported_difference` where Eggress deliberately exposes a narrower role or different lifecycle.
5. Do not change stable intentional exclusions.
6. Remove or correct any example using the noncanonical fixed-target syntax as though it were pproxy syntax.
7. Record the focused commands actually run. Do not claim workspace-wide or external-oracle success when it was not executed.
8. Restore Phase 6 to `Completed` only when all final acceptance criteria below pass.

## Implementation order

Execute in this order to minimize rework:

1. Workstream 1: parser representation and oracle examples.
2. Workstream 4: echo and `httponly` role classification.
3. Workstream 2: fixed-target TCP/UDP translation and runtime tests.
4. Workstream 3: local-bind translator wiring.
5. Workstream 5: Python single-handshake invariant and tests.
6. Workstream 6: matrix and closure status reconciliation.

Do not begin with documentation edits that assert completion before the runtime tests exist.

## Commit structure for handoff

Use small independently reviewable commits where practical:

1. `fix(pproxy): parse canonical fixed-target and echo URIs`
2. `fix(pproxy): preserve fixed-target TCP and explicit UDP roles`
3. `fix(pproxy): carry upstream local bind into native chains`
4. `fix(pproxy): restrict httponly to upstream role`
5. `fix(python): prevent duplicate upstream proxy handshake`
6. `docs: close narrow pproxy corrective pass`

Combining Workstreams 1 and 4 is acceptable if parser and role tests remain clear. Do not combine unrelated cleanup.

## Focused verification

Run the smallest owning suites during implementation:

```bash
cargo fmt --check
cargo test -p eggress-pproxy-compat
cargo test -p eggress-config fixed_target
cargo test -p eggress-server fixed_target
cargo test -p eggress-runtime fixed_target
cargo test -p eggress-core local_bind
python -m pytest \
  python/tests/test_pproxy_public_namespace.py \
  python/tests/test_server_lifecycle.py \
  python/tests/test_pproxy_route_through.py -q
```

Use actual test names available after implementation rather than adding empty filters merely to satisfy this list.

Because shared core/server/runtime code may change, run once before closure:

```bash
cargo test --workspace --locked
```

Run the existing optional representative pproxy differential suite only for affected scenarios when the oracle environment is available:

```bash
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 \
  cargo test -p eggress-cli --test pproxy_differential -- --ignored --test-threads=1
```

A skipped external run is recorded as not executed, not as a pass. It does not justify expanding CI.

## Final acceptance criteria

This corrective pass is complete only when all of the following are true:

- canonical pproxy fixed-target syntax parses and preserves the target;
- malformed target syntax fails clearly;
- `echo://` reaches working TCP and bounded UDP behavior;
- fixed-target TCP retains its compiled target and relays loopback traffic;
- UDP fixed-target configuration is independent and only enabled for an observed/requested UDP role;
- compatibility local bind reaches `ProxyHopSpec.local_bind` and affects the outbound socket;
- `httponly` listener input produces a role-specific rejection while upstream forwarding remains functional;
- Python server routing through HTTP and SOCKS5 upstreams performs each handshake exactly once;
- direct Python `Connection` TCP/UDP behavior remains intact;
- no excluded transport or broad UDP feature is added;
- focused tests pass;
- workspace tests pass once if shared runtime code changed;
- the practical matrix is corrected to match observed behavior;
- Phase 6 status is restored only after the above evidence exists;
- no new required CI workflow, evidence bundle, dependency family, or compatibility subsystem is introduced.

## Stop conditions

Stop and retain an explicit `supported_difference` or `intentional_non_parity` classification when:

- exact pproxy behavior requires a new protocol engine;
- fixed-target UDP requires general multi-hop UDP redesign;
- local bind requires global process networking mutation;
- correcting Python server behavior requires replacing the entire compatibility package;
- an affected role is not documented or observable in pproxy 2.7.9;
- a platform-specific path cannot be tested locally without privileged infrastructure.

A stop condition must be documented with the exact blocked behavior. It must not be hidden behind a generic unknown-protocol error.

## Handoff notes

The highest-risk mistake is to add more parser or TOML tests without exercising the resulting listener or Python server. Each workstream therefore ends in a loopback behavior test.

The second highest-risk mistake is scope expansion. This plan does not authorize work on SSH, QUIC/H3, SSR, legacy crypto, plugins, daemonization, reuse, PF, reverse TLS, or general multi-hop UDP.

The implementation model should treat the existing runtime primitives as authoritative and repair the adapter boundaries around them.