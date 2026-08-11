# pproxy Final Phase 1 — Python Semantic Closure

## Status

**PLANNED**

## Parent roadmap

[`PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md`](PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md)

## Objective

Remove the remaining silent-success and exact public-contract mismatches in the bundled top-level `pproxy` Python namespace. Preserve trivial upstream-compatible behavior where it is cheap and local; fail explicitly with the existing stable compatibility exception where implementing the upstream behavior would require intentionally excluded plugin/runtime machinery.

This phase is not authorization to rebuild pproxy's Python networking engine, plugin system, SSH/QUIC stack, or legacy cipher pipeline.

## Confirmed baseline defects

At planning baseline `5a724be68de7080cc6fff21aeb5774491a307dfa`:

1. `python/pproxy/server.py::prepare_ciphers(cipher, reader, writer, bind=None, server_side=True)` returns `(reader, writer)` unchanged whenever `cipher is not None`. Upstream pproxy 2.7.9 uses this helper to initialize cipher plugins and produce cipher-wrapped stream behavior. The current Eggress compatibility helper can therefore appear to succeed while omitting the requested operation.
2. `_proxy_by_uri()` parses comma-delimited plugin metadata from the URI path into `_plugins`, but the value is discarded. A URI requesting an unsupported plugin can therefore be accepted without the plugin being applied.
3. `DUMMY` is exposed as `object()`. In pproxy 2.7.9 it is a callable identity helper (`lambda s: s`). Client code that calls `pproxy.server.DUMMY(value)` fails under Eggress.
4. `UDP_LIMIT` is `64`; pproxy 2.7.9 exposes `30`.
5. Existing strict server-helper tests heavily emphasize symbol presence/signatures and do not by themselves prove these observable behaviors.

## Governing rules

1. Prefer exact behavior for trivial constants/helpers.
2. Unsupported operational behavior must raise `UnsupportedPProxyFeature` before performing partial work or side effects.
3. Reuse the existing `PProxyCompatibilityError` / `UnsupportedPProxyFeature` hierarchy. Do not create another exception taxonomy.
4. Do not silently drop plugin names/options anywhere in the top-level `pproxy` namespace.
5. Do not implement plugin execution in this phase.
6. Do not make `prepare_ciphers()` a fake pass-through wrapper to preserve signatures.
7. Preserve exact upstream sentinel/no-op behavior only when a focused oracle/source test demonstrates that upstream itself behaves that way.
8. Do not change the native `eggress` Python API unless a small shared helper is clearly required for the compatibility fix.
9. Avoid broad API inventory rewrites; update only active documentation/classification entries affected by these changes. Phase 2 owns general contract/documentation reconciliation.

## Likely files

Implementation should inspect and modify only what the changed behavior requires, likely including:

- `python/pproxy/server.py`
- `python/eggress/_pproxy_proxy.py` only if a shared unsupported-operation helper is required
- `python/eggress/pproxy.py` only if exception messaging/exports require a narrow correction
- `python/tests/strict/test_server_helpers_differential.py`
- `python/tests/test_pproxy_public_namespace.py`
- focused tests under `python/tests/` or `tests/compat/`
- `python/compat/classification.py` / `python/compat/classification.json` only if they are still generated/maintained inputs for affected symbols
- `docs/parity/pproxy_capability_manifest.toml` only for Python entries whose classification materially changes
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` only if the public Python/plugin wording changes

Do not touch unrelated protocol crates.

## Workstream A — Restore exact trivial public behavior

### `DUMMY`

Change the compatibility symbol to match pproxy 2.7.9 observable behavior:

```python
def DUMMY(value):
    return value
```

A lambda is acceptable if it preserves the same call behavior, but a named function is preferable if it improves traceback/introspection stability without changing compatibility expectations.

Tests must prove:

- `callable(DUMMY)` is true;
- a representative immutable value is returned unchanged;
- an object instance is returned by identity (`is`), not copied/coerced;
- the behavior matches a pinned upstream observation/source expectation.

Do not add unnecessary generic typing or wrapper classes.

### `UDP_LIMIT`

Set the compatibility constant to the pproxy 2.7.9 value (`30`) unless a fresh pinned-oracle check contradicts the known source baseline.

Tests must assert the exact value. If the constant does not actually control native Eggress UDP state, document it as a compatibility constant rather than wiring it into the Rust runtime merely to make the number operational.

Do **not** reduce native Eggress UDP limits to 30 unless existing runtime semantics intentionally depend on this compatibility constant. The top-level Python symbol and the Rust runtime are separate surfaces.

## Workstream B — Make `prepare_ciphers()` behavior honest

First establish the exact bounded contract with a focused pproxy 2.7.9 source/oracle check:

- `cipher is None` should retain upstream-compatible behavior (`(None, None)` at the helper boundary if that is the pinned behavior).
- non-`None` cipher behavior relies on pproxy's Python cipher/plugin stream adaptation and is not currently provided by this helper.

Preferred implementation:

1. Preserve the upstream-compatible `cipher is None` return.
2. For a non-`None` cipher, raise `UnsupportedPProxyFeature` immediately with:
   - stable feature identifier such as `prepare_ciphers` or a more specific already-established identifier;
   - concise explanation that pproxy's internal Python stream-cipher/plugin wrapper is not replicated;
   - actionable alternative pointing to Eggress's supported native Shadowsocks/AEAD or managed runtime APIs where appropriate.
3. Do not mutate the cipher object, reader, writer, plugins, or connection state before raising.

Only implement real non-`None` behavior instead of raising if the implementer can map the operation to an already-existing native Eggress implementation with a small direct adapter and no new plugin/runtime architecture. This is a strict stop condition: if the mapping requires reimplementing pproxy plugin hooks or Python stream wrappers, fail explicitly.

Required tests:

- `prepare_ciphers(None, reader, writer, ...)` matches the pinned upstream result.
- non-`None` cipher raises `UnsupportedPProxyFeature`.
- the exception exposes the stable feature identifier and an alternative/help string.
- a fake cipher object with observable mutation hooks proves no mutation/plugin method occurs before the exception.
- behavior is identical for `server_side=True` and `False` unless the pinned upstream contract makes the unsupported classification direction-specific.

## Workstream C — Reject ignored plugin metadata

`_proxy_by_uri()` currently extracts plugin text from the URI and ignores it. Correct this at construction time.

### Required behavior

When the parsed URI requests one or more plugin components that Eggress does not execute:

- raise `UnsupportedPProxyFeature` (or convert a Rust compatibility diagnostic into that exact stable Python exception if a shared parser is used);
- include the plugin name(s) in a safe diagnostic where doing so cannot expose credentials;
- do not return a `ProxySimple`/other proxy object that suggests the plugin is active;
- do not silently strip plugin options;
- do not install, import, discover, or execute third-party plugins.

No-plugin URIs must retain current behavior.

### Parsing boundary

Before coding, verify the pproxy URI form used by the checked-in fixtures/examples. Preserve URI syntax acceptance for plugin-free inputs. Do not reinterpret commas used by a different supported field as plugins without a regression test.

Required tests should include:

- one simple plugin request;
- plugin with options if pproxy syntax permits it;
- multiple plugin tokens if accepted by the parser;
- a normal URI without plugins still produces the expected proxy object;
- credentials in the same URI are not leaked by the exception string/repr;
- no plugin metadata is silently discarded.

If Rust `PproxyUri.plugins` already reports unsupported plugin metadata during translation, align the Python factory's behavior/message with that existing compatibility contract rather than creating a second policy.

## Workstream D — Audit directly adjacent silent-success methods only

Perform a **bounded** inspection of `python/pproxy/server.py` and `python/eggress/_pproxy_proxy.py` for methods adjacent to the changed helpers that:

- return `None`, `(reader, writer)`, or another plausible success value while intentionally omitting requested behavior;
- parse a field and then discard it;
- catch an unsupported operation and continue silently.

Do not inventory the entire Python package again. Existing corrective work already classified most structural methods.

For each newly discovered adjacent case, choose exactly one:

1. prove with a focused upstream test that the sentinel is correct and add a regression test; or
2. delegate to an existing working Eggress implementation; or
3. raise `UnsupportedPProxyFeature` before side effects.

If more than a small handful of new cases appear, stop and record them in the implementation summary rather than expanding this phase into another broad parity audit. The roadmap should then be revisited explicitly.

## Workstream E — Focused compatibility tests

Add high-value behavioral tests, not another observation framework.

Minimum assertions:

```text
DUMMY is callable and identity-preserving
UDP_LIMIT == pinned pproxy 2.7.9 value
prepare_ciphers(None, ...) matches upstream-compatible sentinel
prepare_ciphers(non_none, ...) raises UnsupportedPProxyFeature before mutation
plugin-bearing proxy URI fails explicitly
plugin-free proxy URI remains functional
credentials are redacted from plugin/unsupported diagnostics
```

Where practical, add one paired oracle test for the trivial public values/behavior. Do not require the external oracle for normal Python smoke CI; use checked-in observations or focused opt-in differential tests.

## Workstream F — Minimal documentation/classification update

Update only affected active claims so they do not imply plugin/cipher-helper functionality that is absent.

At minimum inspect:

- the Python-package/plugin row in `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`;
- relevant Python capability entries in `docs/parity/pproxy_capability_manifest.toml`;
- `docs/architecture/python.md` and `docs/architecture/pproxy-compat.md` if either claims these helpers are operationally backed.

Do not rewrite historical API inventory documents in this phase; Phase 2 owns their demotion/correction strategy.

## Verification

Focused Python tests first, for example:

```bash
.venv/bin/python -m pytest \
  python/tests/test_pproxy_public_namespace.py \
  python/tests/strict/test_server_helpers_differential.py -q
```

Then the normal Python-facing gate after rebuilding the extension:

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

If Rust code or the manifest validator changes, also run affected Rust tests. Final substantial-change gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

External pproxy installation/oracle execution is required only for a semantic uncertainty that the checked-in 2.7.9 baseline does not already resolve.

## Acceptance criteria

Phase 1 is complete only when all are true:

- `pproxy.server.DUMMY` is callable and preserves its argument by identity/value in the same observable manner as pproxy 2.7.9;
- `pproxy.server.UDP_LIMIT` equals the pinned pproxy 2.7.9 public value, unless a newly verified oracle result requires a documented correction;
- native Eggress UDP resource limits are not accidentally changed merely to match the compatibility constant;
- `prepare_ciphers(None, ...)` has a test-backed upstream-compatible result;
- `prepare_ciphers(non_none_cipher, ...)` either performs a real existing Eggress-backed equivalent operation or raises `UnsupportedPProxyFeature` before any partial mutation/side effect;
- no pass-through `(reader, writer)` success remains for an unsupported cipher/plugin operation;
- a plugin-bearing URI cannot construct a proxy object while silently discarding the requested plugin;
- plugin diagnostics are stable, actionable, and credential-safe;
- plugin-free URI construction retains existing behavior;
- any directly adjacent silent-success case discovered by the bounded audit is either proven upstream-compatible, delegated to working behavior, or made explicitly unsupported;
- focused tests cover exact constant/helper behavior, unsupported cipher behavior, plugin rejection, and redaction;
- active compatibility documentation no longer implies these unsupported internals are operationally implemented;
- no plugin framework, legacy cipher implementation, SSH/QUIC transport, new exception taxonomy, or broad Python runtime reimplementation is introduced;
- the Python smoke suite passes after a fresh extension build;
- the broad Rust gate passes if Rust/shared contract code changed;
- this plan is updated in place to `IMPLEMENTED` with implementation commit(s), exact focused tests run, and any deliberately retained incompatibility.