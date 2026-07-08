# Track A.01: pproxy Oracle Harness

## Objective

Expand the differential test harness from a useful protocol smoke suite into a durable pproxy oracle system. The goal is to make compatibility claims evidence-driven and reproducible across CLI behavior, TCP/UDP runtime behavior, routing behavior, and eventually Python API behavior.

Track A should focus on non-privileged, common-path oracle coverage. Privileged transparent proxying, SSH, SSR, QUIC/H3, PF, and other long-tail surfaces can be gated for later tracks, but their scenario metadata should be supported now.

## Current Problem

The existing evidence is strongest for HTTP/SOCKS TCP baseline behavior. Other areas are synthetic, partial, or interop-only. The repo needs a scenario-driven oracle runner that can compare egress and real pproxy under equivalent conditions, capture divergence precisely, and feed the canonical manifest.

## Scope

Implement or extend a reusable oracle framework that can:

- start egress and pproxy with equivalent CLI/config inputs;
- wait for readiness without brittle sleeps;
- allocate ephemeral ports deterministically;
- run TCP clients, UDP clients, HTTP clients, SOCKS clients, and route probes;
- capture stdout, stderr, exit code, startup logs, connection logs, and normalized results;
- classify scenarios as passed, failed, skipped, or unsupported;
- emit machine-readable scenario output for manifest/report generation.

## Scenario Metadata

Every scenario should include:

- `id`
- `capability_ids`
- `description`
- `pproxy_args`
- `egress_args` or translation path
- `client_action`
- `expected_equivalence`
- `normalization_rules`
- `required_platforms`
- `requires_root`
- `requires_ipv6`
- `requires_python_package`
- `requires_pycryptodome`
- `requires_asyncssh`
- `requires_legacy_feature`
- `requires_network`
- `timeout_budget`

Do not bury skip logic inside test functions. Make it visible in scenario metadata and report output.

## Harness Architecture

Use the existing `eggress-testkit::differential` structure if viable. Add a scenario registry and runner around it rather than writing ad hoc tests.

Suggested modules:

- `eggress-testkit::oracle::scenario`
- `eggress-testkit::oracle::process`
- `eggress-testkit::oracle::ports`
- `eggress-testkit::oracle::clients`
- `eggress-testkit::oracle::normalize`
- `eggress-testkit::oracle::report`

The process runner should have explicit lifecycle states: created, spawned, ready, exercised, terminated, killed, cleaned.

## Required Track A Scenarios

### CLI/defaults

- no-argument pproxy default listener behavior;
- `-l http+socks4+socks5://:PORT` mixed listener behavior;
- `-l http://:PORT` HTTP-only behavior;
- `-l socks5://:PORT` SOCKS5-only behavior;
- `-r direct://` explicit direct upstream;
- `-r http://HOST:PORT` HTTP upstream;
- `-r socks5://HOST:PORT` SOCKS5 upstream;
- `--version`;
- `-h` / `--help` smoke.

### HTTP/SOCKS TCP

- HTTP CONNECT echo;
- HTTP absolute-form GET;
- HTTP POST body with Content-Length;
- HTTP HEAD;
- HTTP persistent connection with sequential requests;
- SOCKS4 IPv4 connect;
- SOCKS4a domain connect;
- SOCKS5 IPv4/domain/IPv6 connect;
- HTTP auth success/failure;
- SOCKS5 auth success/failure.

### Chains

- SOCKS5 listener through HTTP upstream;
- SOCKS5 listener through SOCKS5 upstream;
- HTTP listener through SOCKS5 upstream;
- two-hop TCP chain with pproxy `__` separator;
- three-hop TCP chain synthetic/oracle where pproxy can be made reliable.

### Rules

- one remote with `?rules` file and unmatched direct fallback;
- multiple remotes with distinct rule files;
- `-b` block regex;
- invalid regex diagnostics;
- fancy-regex-only construct in compatibility mode.

### UDP

- standalone `-ul` direct echo;
- standalone `-ul` plus `-ur socks5://...` if pproxy supports the scenario reliably;
- SOCKS5 UDP ASSOCIATE direct echo;
- malformed UDP datagram behavior.

## Output Artifacts

Add an optional JSON report writer. The report should include:

- pproxy version;
- egress binary version/git SHA if available;
- scenario list;
- per-scenario status;
- normalized output comparison;
- observed divergences;
- skipped requirements;
- elapsed time.

The canonical manifest should be able to reference these scenario IDs.

## Version Pinning

Pin the oracle baseline to `pproxy==2.7.9` initially because existing manifests already cite it. Add an environment variable for future drift checks:

- `EGRESS_PPROXY_VERSION=2.7.9`
- `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`
- `EGRESS_ORACLE_REPORT=target/pproxy-oracle.json`

The default non-gated test suite should not require internet access or pproxy installation. Gated oracle tests should skip cleanly unless explicitly required.

## Readiness Detection

Avoid arbitrary sleeps. Prefer:

- read bound port from egress if available;
- attempt TCP connection until ready or timeout;
- parse pproxy startup lines only as a fallback;
- use a per-scenario timeout budget.

## Normalization Rules

Do not over-normalize. Normalize only known irrelevant differences:

- ephemeral ports;
- process IDs;
- timestamps;
- log prefixes;
- absolute temp paths;
- minor whitespace in startup banners where not semantically relevant.

Do not normalize protocol payload, HTTP status, SOCKS reply code, auth result, or routing decision unless the manifest classifies the feature as `compatible_with_warning` rather than `drop_in`.

## Acceptance Criteria

- A single gated command runs all non-privileged pproxy oracle scenarios.
- The oracle report can be generated locally.
- Scenario IDs are stable and can be referenced from the canonical manifest.
- The test runner distinguishes failure from skip.
- Common HTTP/SOCKS scenarios have differential evidence.
- Rulefile and regex compatibility scenarios exist, including `fancy_regex` coverage.

## Non-goals

This pass does not require SSR, SSH, QUIC/H3, macOS PF, or privileged transparent proxy oracle scenarios to pass. It only needs the metadata and skip infrastructure to represent them cleanly later.

## Handoff Notes

Start by wrapping the current differential tests into a scenario registry, then add new scenarios incrementally. Do not rewrite all protocol clients if existing test helpers are adequate. The main value is stable scenario identity and reportability.
