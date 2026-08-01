# Historical strict compatibility policy

This file records the vocabulary and evidence rules used by the historical
strict-manifest work. It is retained for provenance, but it is not the current
public compatibility claim and it does not describe a separately published
`eggress-pproxy-compat` distribution.

For current policy and supported boundaries, use:

- [`README.md`](README.md) for the active compatibility labels and document index;
- [`PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`](PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md)
  for the maintained observable matrix;
- [`../PPROXY_PARITY_SPEC.md`](../PPROXY_PARITY_SPEC.md) for compatibility
  vocabulary and qualified claims.

The strict manifest and optional oracle harness remain useful when a specific
behavior requires historical investigation. A skipped external oracle run is
incomplete evidence, not a passing result. Routine CI intentionally does not
run the external oracle suite.
