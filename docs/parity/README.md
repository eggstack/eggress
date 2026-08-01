# pproxy compatibility

Eggress targets practical, behavior-oriented compatibility with
`pproxy==2.7.9`. The compatibility surface is bounded: the native runtime and
the compatibility translator are separate surfaces, and a matching name or
successful import does not establish full pproxy parity.

The maintained user-facing summary is
[`PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`](PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md).
It uses these labels:

| Label | Meaning |
|---|---|
| `matched` | Representative oracle comparison or direct interoperability confirms the defined behavior. |
| `supported_difference` | Usable, with a documented observable difference or narrower boundary. |
| `intentional_non_parity` | Explicitly excluded from the practical compatibility target. |
| `native_extension` | Eggress-only functionality with no pproxy claim. |
| `platform_limited` | Available where the required operating-system facility exists. |

## Authoritative documents

- [`PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`](PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md) — maintained public matrix and exclusions.
- [`PPROXY_CLOSURE_SCENARIOS.md`](PPROXY_CLOSURE_SCENARIOS.md) — compact optional oracle and smoke scenario index.
- [`pproxy_capability_manifest.toml`](pproxy_capability_manifest.toml) — detailed machine-readable implementation inventory.
- [`composition_matrix.toml`](composition_matrix.toml) — protocol, role, and traffic-kind composition constraints.
- [`../PYTHON_BINDINGS.md`](../PYTHON_BINDINGS.md) — native and pproxy-compatible Python workflows.
- [`../PPROXY_PARITY_SPEC.md`](../PPROXY_PARITY_SPEC.md) — compatibility vocabulary and boundaries.

The strict manifest, evidence inventories, and phase completion records are
historical or diagnostic inputs. They are not a release claim and must not be
used to infer aggregate parity percentages. External oracle checks remain
optional and are not part of routine hosted CI.

The Python distribution is `eggress`. Its wheel contains the bounded top-level
`pproxy` package as well as the `eggress` namespace; there is no separately
published `eggress-pproxy-compat` distribution. Do not install upstream
`pproxy` beside Eggress because both distributions own the same import
namespace.

## Verification commands

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test cli_tests --test pproxy_binary
cargo test -p eggress-cli --test integration
python -m pytest python/tests/test_wheel_import_smoke.py \
  python/tests/test_proxy_connection.py \
  python/tests/test_server_lifecycle.py -q
```

Optional paired checks require a local `pproxy==2.7.9` oracle environment:

```bash
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 \
  cargo test -p eggress-cli --test pproxy_differential -- --ignored --test-threads=1
```
