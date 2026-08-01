# Tools & Scripts

`scripts/`

Interoperability tests, certification probes, smoke clients, and development utilities.

## Interoperability Tests

| Script | Description |
|---|---|
| `compat_shadowsocks.sh` | Shadowsocks wire-format interoperability with pproxy |
| `compat_udp_pproxy.sh` | UDP interoperability with pproxy |
| `install_shadowsocks_interop.sh` | Install Shadowsocks for interop testing |

## Certification and Parity

| Script | Description |
|---|---|
| `run_pproxy_certification.sh` | Historical/full oracle helper; optional and not a routine CI gate |
| `run_strict_pproxy_api.sh` | Run strict API comparison |
| `run_strict_pproxy_interop.sh` | Run strict interoperability tests |
| `validate_pproxy_parity_manifest.py` | Validate parity manifest |

## Strict Probes

| Script | Description |
|---|---|
| `strict_api_probe.py` | Strict API surface probe |
| `strict_cipher_interop_probe.py` | Cipher interoperability probe |
| `strict_cipher_kat_probe.py` | Known Answer Test (KAT) probe |
| `strict_cipher_roundtrip_probe.py` | Cipher roundtrip probe |
| `strict_class_probe.py` | Python class probe |
| `strict_handler_relay_probe.py` | Handler relay probe |
| `strict_plugin_lifecycle_probe.py` | Plugin lifecycle probe |
| `strict_process_lifecycle_probe.py` | Process lifecycle probe |
| `strict_protocol_wire_probe.py` | Protocol wire format probe |
| `strict_runtime_failure_cleanup_probe.py` | Failure cleanup probe |
| `strict_server_internals_probe.py` | Server internals probe |
| `strict_signature_probe.py` | Signature probe |
| `strict_stream_adapter_probe.py` | Stream adapter probe |

## Analysis Tools

| Script | Description |
|---|---|
| `compare_observations.py` | Compare test observations |
| `snapshot_pproxy_api.py` | Snapshot pproxy API surface |
| `probe_pproxy_chain_topology.py` | Probe pproxy chain topology |
| `demonstrate_regression_injections.py` | Demonstrate regression injection patterns |

## Smoke Testing

| Script | Description |
|---|---|
| `smoke_clients.py` | Smoke test client connections |
| `test_wheel.sh` | Test Python wheel installation |

## Evidence Generation

| Script | Description |
|---|---|
| `build_strict_evidence_index.py` | Build strict evidence index |
| `run_strict_api_comparison.sh` | Run strict API comparison |
| `run_strict_pproxy_api.py` | Run strict pproxy API tests |

## Performance

| Directory | Description |
|---|---|
| `scripts/perf/` | Performance testing scripts |

## Fuzz Targets

The `fuzz/` directory contains a standalone fuzz workspace with targets for URI parsing and protocol detection.

## Benchmarks

The `benches/` directory contains Criterion benchmarks:
- `tcp_relay` — TCP relay throughput
- `udp_relay` — UDP relay throughput
- `route_match` — Rule matching performance
- `http_connect_upstream` — HTTP CONNECT upstream performance

See [overview.md](overview.md) for context.
