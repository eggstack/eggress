# Upstream pproxy Examples

This directory contains representative examples from the pproxy 2.7.9
upstream distribution, used as test fixtures for the paired
oracle/candidate differential testing harness.

## Files

| File | Description |
|------|-------------|
| `server_config.py` | Canonical server configuration patterns |
| `client_connection.py` | Python client connection patterns |
| `rule_file.txt` | Rule file format examples |

## Provenance

These fixtures are derived from:
- pproxy CLI help text (`pproxy --help`)
- pproxy Python API contract (`pproxy.api_contract`)
- pproxy upstream README and documentation

They are NOT copied from the upstream repository verbatim. Instead, they
represent the canonical usage patterns that any pproxy user would employ.

## Usage in Differential Testing

The oracle runner executes these patterns against pproxy 2.7.9 and
records structured observations. The candidate runner executes the
same patterns against eggress and records observations. Differential
comparators compare the observations.

## License

pproxy is licensed under MIT. These derivative fixtures follow the
same license.
