# pproxy Oracle Fixtures

This directory contains Eggress-authored behavioral scenarios based on
the pproxy 2.7.9 public API, used as test fixtures for the paired
oracle/candidate differential testing harness.

## Files

| File | Description | Network |
|------|-------------|---------|
| `direct_tcp.py` | Direct connection construction and attributes | No |
| `chain_example.py` | Chain (__ separator) topology validation | No |
| `server_config.py` | Server construction, rule compilation, constants | No |
| `client_connection.py` | Client Connection object construction | No |
| `tcp_echo_through_proxy.py` | TCP data relay through proxy | Yes |
| `udp_echo_through_proxy.py` | UDP datagram relay through proxy | Yes |
| `server_start_lifecycle.py` | Server start/accept/shutdown lifecycle | Yes |
| `cli_test.py` | CLI --help, --version, error handling | No |
| `rule_file.txt` | Rule file format examples | No |

## Provenance

These fixtures are Eggress-authored behavioral scenarios based on the
pproxy 2.7.9 public API. They are derived from:
- pproxy CLI help text (`pproxy --help`)
- pproxy Python API contract
- pproxy upstream README and documentation

They are NOT copied from the upstream repository verbatim. Instead, they
represent the canonical usage patterns that any pproxy user would employ.

## Usage in Differential Testing

The oracle runner executes these patterns against pproxy 2.7.9 and
records structured observations. The candidate runner executes the
same patterns against eggress and records observations. Differential
comparators compare the observations.

## Running Examples

```bash
# Construction-only examples (no network)
python3.11 direct_tcp.py
python3.11 chain_example.py
python3.11 server_config.py
python3.11 client_connection.py

# Network examples (start echo servers internally)
PROXY_URI=direct:// python3.11 tcp_echo_through_proxy.py
PROXY_URI=direct:// python3.11 udp_echo_through_proxy.py
python3.11 server_start_lifecycle.py
python3.11 cli_test.py
```

## License

pproxy is licensed under MIT. These derivative fixtures follow the
same license.
