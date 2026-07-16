# Eggress pproxy compatibility distribution

This package intentionally provides the top-level `pproxy` import namespace
for the certified subset of pproxy 2.7.9 behavior.

```bash
pip install eggress-pproxy-compat
```

It installs `eggress==0.1.0` and the tested `cryptography` range as declared
dependencies. The package does not modify `sys.modules`, `sys.path`, or an
already-installed upstream package at runtime. Installing it into an
environment that already contains upstream `pproxy` is not supported; remove
the conflicting distribution first and verify `pproxy.__eggress_compat__`.

This is a certified-subset compatibility package, not strict full pproxy
parity. Legacy ciphers, SSR, SSH, QUIC/H3, and plugin live paths remain
explicitly outside the certified scope.
