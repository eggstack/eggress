# Phase 41: pproxy differential parity harness

## Goal

Build a reusable differential test harness that compares eggress behavior against Python pproxy for capabilities claimed as `drop_in` or `compatible_with_warning` in the parity manifest.

The purpose is to move parity evidence from unit-level and synthetic tests toward observable behavior. Parser tests and config-compile tests are necessary, but they are not enough for pproxy replacement claims.

## Current context

Eggress already has substantial internal tests and pproxy translation tests. The remaining gap is systematic side-by-side execution against a pinned pproxy version. The harness should launch pproxy and eggress with equivalent inputs, send identical client traffic, and compare behavior at the boundaries users observe:

- stdout/stderr and exit code for CLI operations
- listener startup and bind behavior
- HTTP/SOCKS protocol replies
- proxied payload bytes
- authentication success/failure
- UDP association behavior
- route/rule effects
- TLS wrapping behavior
- shutdown behavior

## Primary deliverables

- Differential test harness under an appropriate location, such as `tests/differential/`, `crates/eggress-testkit/`, or a new workspace test crate if preferable.
- Pinned pproxy version declaration.
- Test fixtures for CLI args, expected tiers, and network scenarios.
- Side-by-side process management utilities.
- Minimal target echo/origin servers for TCP, HTTP, and UDP.
- CI gating strategy that can skip when Python/pproxy is unavailable but must run in at least one configured job before release.
- Parity manifest evidence updates from `integration` or `synthetic` to `differential` where appropriate.

## Harness design

### Process model

The harness should be able to start:

1. A target server, such as TCP echo, HTTP origin, or UDP echo.
2. One or more upstream proxy hops if the scenario requires a chain.
3. Python pproxy with a pproxy command line.
4. Eggress with equivalent pproxy command line through `eggress pproxy run -- ...` or generated TOML.
5. A client that sends identical traffic to each proxy.

Use port 0/dynamic ports wherever possible. Capture assigned addresses and avoid hardcoded ports.

### Pinned pproxy

Declare the exact pproxy version used as the oracle. Options:

- install via `python -m pip install pproxy==<version>` in CI;
- use a known local Python environment;
- vendor only metadata/fixtures, not pproxy source, unless licensing and maintenance are deliberately handled.

The test harness should print the pproxy version at startup.

### Test skipping

Differential tests may need Python and pproxy. They should be explicitly gated:

- normal unit tests should not silently depend on internet access;
- local runs can skip if pproxy is not installed;
- CI release/parity job must install pproxy and run the differential suite.

Use a clear environment variable such as `EGGRESS_RUN_PPROXY_DIFFERENTIAL=1` to opt in locally.

## Scenarios to implement first

### Scenario 1: HTTP CONNECT direct

pproxy command:

```bash
pproxy -l http://127.0.0.1:0
```

eggress command:

```bash
eggress pproxy run -- -l http://127.0.0.1:0
```

Client behavior:

- send CONNECT to target TCP echo server;
- verify 200-style connect success;
- send bytes through tunnel;
- verify echoed bytes match.

Compare:

- success/failure;
- response class;
- payload integrity;
- half-close behavior if feasible.

### Scenario 2: ordinary HTTP forward proxy

Client behavior:

- send absolute-form HTTP request through proxy to local HTTP origin;
- verify origin sees origin-form path if pproxy does;
- verify response body and headers as appropriate.

Compare:

- status code;
- body;
- essential forwarding headers;
- hop-by-hop header filtering where observable.

### Scenario 3: SOCKS4 and SOCKS4a CONNECT

Client behavior:

- connect through SOCKS4 to IPv4 target;
- connect through SOCKS4a to domain target.

Compare:

- reply code;
- payload tunnel behavior;
- target address observed by server where practical.

### Scenario 4: SOCKS5 CONNECT no-auth

Client behavior:

- negotiate no-auth;
- connect to IPv4, IPv6 if available, and domain target;
- send payload.

Compare:

- method selection;
- reply code;
- payload tunnel behavior.

### Scenario 5: SOCKS5 username/password auth

Client behavior:

- correct credentials succeed;
- wrong password fails;
- unsupported method behavior is comparable or documented.

Compare:

- method selection;
- auth status;
- failure close behavior.

### Scenario 6: SOCKS5 UDP ASSOCIATE

Client behavior:

- establish TCP UDP ASSOCIATE;
- send UDP datagram to target UDP echo server through relay;
- receive reply.

Compare:

- UDP relay address behavior;
- datagram payload;
- domain/IPv4 target handling;
- idle cleanup only if testable without flake.

### Scenario 7: standalone UDP `-ul` / `-ur`

Client behavior:

- run pproxy/eggress standalone UDP relay;
- send datagrams through direct or one-hop supported upstream;
- verify reply.

Compare:

- datagram payload;
- source/target behavior where observable.

### Scenario 8: scheduler behavior

Setup:

- two upstream proxy targets;
- run with `-s rr`, `-s fa`, `-s rc`, `-s lc` where deterministic enough.

Compare:

- for round-robin and first-available, expected route distribution;
- for random, only validate accepted behavior/statistical sanity if non-flaky;
- for least-connections, use concurrent requests if practical.

### Scenario 9: block/rulefile behavior from Phase 38

Client behavior:

- blocked host must fail similarly;
- allowed host must pass;
- rule order should be observable.

Compare:

- failure status/reply class;
- not exact error text unless pproxy scripts are known to depend on it.

### Scenario 10: TLS listener via `--ssl`

Client behavior:

- connect using TLS to HTTP or SOCKS listener;
- perform normal proxy operation inside TLS.

Compare:

- TLS handshake success;
- proxy operation success;
- certificate handling differences documented if they differ.

## CLI output comparison

Add a smaller snapshot suite for non-networking CLI compatibility:

- `--help`
- `--version`
- invalid URI
- unsupported SSH URI
- unsupported SSR URI
- unsupported daemon/reuse flags
- malformed `__` chain
- `pproxy check --json`

Do not require byte-for-byte equality with pproxy unless the project explicitly wants it. Prefer normalized comparison:

- exit code class;
- contains stable diagnostic code;
- redacts secrets;
- does not panic;
- JSON schema validates.

## Fixture format

Use structured fixtures so new scenarios can be added easily. Example:

```toml
[[case]]
id = "http_connect_direct"
category = "http"
tier = "drop_in"
pproxy_args = ["-l", "http://127.0.0.1:0"]
client = "http_connect_echo"
expected = "payload_roundtrip"
manifest_capabilities = [
  "protocol.http.connect.server",
  "cli.listen",
]
```

If using Rust fixtures instead of TOML, preserve the same information in code.

## Integration with parity manifest

The harness should emit or record which manifest capabilities each differential test covers. It does not need to auto-edit the manifest, but it should be easy to map:

- test case ID;
- capability IDs;
- pproxy version;
- eggress version/commit;
- result;
- notes.

A future release report should be able to say which `drop_in` capabilities have differential evidence.

## Reliability constraints

Differential tests are prone to flakes. Apply these rules:

- use local loopback only;
- use dynamic ports;
- include startup readiness checks;
- avoid sleeps except bounded retry loops;
- set short but realistic timeouts;
- always kill child processes on failure;
- capture stderr/stdout for diagnostics;
- avoid depending on external network or DNS except local synthetic domains where controlled;
- skip IPv6 tests when loopback IPv6 is unavailable.

## Security and cleanup

- Never log plaintext credentials from fixtures.
- Use temporary directories for certs, logs, and generated configs.
- Ensure child processes are killed on panic/test failure.
- Avoid changing system proxy settings.
- Do not bind public interfaces in tests.

## Acceptance criteria

- A developer can run the differential suite locally with one documented command after installing pproxy or enabling the harness setup.
- At least HTTP CONNECT, ordinary HTTP forward proxying, SOCKS4/4a, SOCKS5 CONNECT, SOCKS5 auth, SOCKS5 UDP ASSOCIATE, and standalone UDP have initial differential cases or explicit TODO fixtures.
- The harness records pproxy version.
- Tests use dynamic ports and clean up child processes.
- The parity manifest marks at least a subset of previously integration-only evidence as differential.
- CI has a documented path for running the differential suite, even if not part of every push.

## Suggested commands

Local opt-in:

```bash
EGGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-testkit --test pproxy_differential -- --nocapture
```

or, if implemented as workspace integration tests:

```bash
EGGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test --test pproxy_differential -- --nocapture
```

The exact command should be documented after implementation.

## Non-goals

- Do not attempt to make every pproxy behavior byte-identical in the first harness pass.
- Do not depend on external proxy servers.
- Do not test SSH, QUIC, H3, SSR, or Trojan server until those features are implemented or deliberately classified.
- Do not make the entire workspace test suite require Python pproxy by default.

## Handoff notes

Prioritize harness reliability over breadth. A small suite that deterministically compares real behavior is more useful than a large flaky matrix. Once the harness is stable, later protocol phases can add their own fixtures and promote manifest evidence from integration to differential.
