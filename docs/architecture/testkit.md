# eggress-testkit

`crates/eggress-testkit/`

Test utilities, oracle harnesses, and compatibility validation for the workspace.

## Test Servers

| Function | Description |
|---|---|
| `start_echo_server()` | TCP echo server (echoes back received data) |
| `start_half_close_server()` | Server that half-closes after receiving data |
| `start_http_origin_server()` | HTTP origin server for forward proxy testing |

## Utilities

| Function/Type | Description |
|---|---|
| `get_free_port()` | Allocate a free TCP port |
| `SlowReader` | Wraps a stream with artificial read delays |
| `SlowWriter` | Wraps a stream with artificial write delays |
| `FragmentedStream` | Wraps a stream with artificial fragmentation |

## Oracle and Differential Testing

| Module | Description |
|---|---|
| `oracle` | Test oracle for comparing eggress behavior against pproxy |
| `differential` | Differential testing harness |
| `pproxy_oracle` | pproxy-specific oracle integration |
| `strict_comparators` | Strict behavioral comparison utilities |
| `strict_observations` | Observation recording for strict tests |

## Manifest Validation

| Module | Description |
|---|---|
| `manifest` | Canonical parity manifest validation |
| `canonical_manifest` | Canonical manifest data |
| `strict_manifest` | Strict behavioral manifest |

Validates the active parity contract encoded in
`docs/parity/pproxy_capability_manifest.toml`. The older
`docs/parity/pproxy_2_7_9_strict_manifest.toml` remains available to run
historical strict comparators, but is not an active compatibility claim.

## Composition and Corpus

| Module | Description |
|---|---|
| `composition` | Composition test models |
| `case_model` | Test case models |
| `corpus` | Test corpus data |

## Reporting

| Module | Description |
|---|---|
| `report` | Optional diagnostic result reporting; not a routine compatibility claim |

## Dependencies

None — test-only crate with minimal dependencies.

See [overview.md](overview.md) for context.
