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

### Tier classification rules

`crates/eggress-pproxy-compat/src/tier.rs` is the **single executable
owner** for both per-diagnostic and aggregate compatibility tier
semantics. `manifest_tier_for_category()` maps diagnostic warning
categories to the five-tier vocabulary;
`classify_unsupported_feature_tier()` (in `diagnostics.rs`) maps
unsupported feature ids to tiers and is reused by
`manifest_tier_for_unsupported_feature()` so per-diagnostic and
aggregate classification never disagree.

`classify_aggregate_tier()` picks the worst tier from a set of
warnings and unsupported features, with the severity order
(worst first): `unsupported` > `intentional_non_parity` >
`compatible_with_warning` > `native_equivalent` > `drop_in`. Known
intentional exclusions (SSH listener/upstream, SSR listener/upstream,
legacy Shadowsocks ciphers) aggregate to `intentional_non_parity`
rather than generic `unsupported`; unknown warning categories and
unknown unsupported feature ids fail closed to `unsupported`.

Both the Rust CLI (`pproxy check`) and the Python `check_pproxy_args()`
reporter consume the same native aggregate result via the `tier`
property on `PyTranslationResult`. Python does not maintain an
independent tier table, severity order, or intentional-exclusion set.
A cross-check test in `eggress-testkit` validates that manifest
diagnostic tiers match the Rust reporter.

These rules govern how capabilities are classified:

- **Same flag + different config schema**: Not `drop_in`. A flag with the same
  purpose but incompatible file contents is a `supported_difference`, not
  unchanged-input compatibility.
- **Accepted flag + different output destination**: Not `native_equivalent`. If
  the flag is recognized but the requested outcome does not occur (e.g., log
  file is not written), classify as `supported_difference` and document the
  actual alternative.
- **Same unsupported operation on both implementations**: Not
  `intentional_non_parity`. When pproxy itself does not implement an operation
  (e.g., SOCKS4 BIND), a matching refusal by Eggress is `unsupported`, not
  Eggress-specific non-parity.

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
