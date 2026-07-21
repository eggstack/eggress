# Upstream pproxy Test Fixtures

This directory contains test fixture patterns from the pproxy 2.7.9
upstream distribution, used as test inputs for the paired
oracle/candidate differential testing harness.

## Files

| File | Description |
|------|-------------|
| `protocol_echo.py` | Protocol interaction patterns (SOCKS5, HTTP, SS, UDP) |

## Provenance

These fixtures are derived from:
- pproxy protocol implementation (`pproxy.proto`, `pproxy.server`)
- pproxy test suite patterns
- Protocol specifications (RFC 1928, RFC 7231, SIP003)

## Usage

The oracle runner executes these patterns against pproxy 2.7.9 and
records structured observations. The candidate runner executes the
same patterns against eggress and records observations. The
differential comparators compare the observations.

## License

pproxy is licensed under MIT. These derivative fixtures follow the
same license.
