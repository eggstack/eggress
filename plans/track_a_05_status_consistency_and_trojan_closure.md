# Track A.05: Status Consistency and Trojan Closure

## Objective

Resolve immediate status contradictions and close the most visible compatibility-reporting gaps before public parity claims are made. The highest-priority conflict is Trojan server support, but this pass also covers protocol-crate-only features that appear in user-facing support lists, system-proxy status, and CLI flag status drift.

## Problem Statement

The repo currently contains status surfaces that can disagree. For example, one document can say a feature is not implemented while another manifest says it is supported. Some Python feature introspection can list protocols that are parseable or implemented in protocol crates, even when runtime/config/CLI refuse those schemes.

This is dangerous for a compatibility project because users will trust the support matrix. Track A must make every visible support claim layer-aware.

## Layer Model

Every feature should be reported across layers:

- parser: URI/flag/API symbol can be parsed;
- translator: pproxy compatibility translator can represent it;
- config: egress config compiler can accept it;
- runtime: service supervisor can run it;
- cli: command-line path can invoke it;
- python: Python compatibility API can invoke it;
- docs: documented accurately;
- tests: evidence exists.

A feature that is parser-complete but runtime-refused is not supported for users. It should be described as parseable with diagnostics, not supported.

## Immediate Conflicts To Audit

### Trojan

Audit current Trojan implementation across:

- `crates/eggress-protocol-trojan`;
- config model;
- config compiler;
- runtime supervisor;
- listener accept path;
- upstream client path;
- TLS requirements;
- pproxy translator;
- CLI run path;
- Python supported features;
- README status;
- manifests and evidence docs.

Possible outcomes:

1. Trojan client and server are both runtime-wired and tested. Update README/docs/manifest to say so and add missing tests.
2. Trojan client is runtime-wired but server is protocol-crate-only or incomplete. Mark server unsupported or partial and add a Track C implementation item.
3. Trojan parser/translator accepts server URIs but runtime cannot start them. Change translator/check output to warn/refuse before runtime.

Do not leave contradictory claims.

### WebSocket, WSS, raw, tunnel, H2

These appear to have protocol crate implementations but are documented as refused by config/runtime/CLI. Ensure Python `supported_features()` and CLI check output do not call them supported unless the layer is explicit.

Suggested reporting:

- `parser = complete`
- `protocol_crate = complete`
- `runtime = refused`
- `tier = intentional_non_parity` or `unsupported` depending on project decision
- `caveat_class = protocol_crate_only`

If Track C will wire them, use `unsupported` rather than permanent `intentional_non_parity`.

### System proxy

Distinguish:

- inspect;
- dry-run apply;
- actual apply;
- rollback state save;
- restore/revert;
- pproxy `--sys` automatic mutation.

A safe native inspect command is not the same as pproxy's mutating `--sys`. Mark compatibility precisely.

### CLI flags

Reconcile status for:

- `--ssl`
- `-b` / `--block`
- `--rulefile`
- `--log`
- `--pac`
- `--test`
- `--get`
- `--daemon`
- `--reuse`

If translator support exists but runtime behavior is native-equivalent rather than exact, mark as `native_equivalent` or `compatible_with_warning`, not `drop_in`.

## Python Feature Introspection Fix

Replace a flat `supported_features()` list with a layer-aware report. Suggested API:

```python
features = eggress.pproxy.supported_features()
```

Return structured entries rather than strings:

```python
{
    "id": "uri.scheme_ws",
    "surface": "ws://",
    "parser": "complete",
    "runtime": "refused",
    "python": "unsupported",
    "tier": "unsupported",
    "notes": "protocol crate exists but runtime wiring is deferred"
}
```

If backwards compatibility requires keeping a string list, rename it or make it include only runtime-supported compatibility features.

## CLI Check Output Fix

`eggress pproxy check --json` should include:

- aggregate tier;
- per-capability tier;
- per-layer status;
- unsupported features;
- warnings;
- whether `run` is possible;
- whether behavior is drop-in, native-equivalent, or blocked.

Do not allow aggregate status to hide a blocked layer.

## Documentation Cleanup

Update README language to avoid broad final-certification claims until the manifest/certification workflow supports them. Suggested wording:

"Strong drop-in coverage for common HTTP/SOCKS/TCP pproxy workflows; long-tail pproxy features are tracked in the parity manifest with explicit diagnostics."

Move detailed status tables into generated parity docs rather than maintaining large manual checklists in README.

## Tests

Add tests that assert:

- Trojan status is the same in README-generated report, manifest, CLI check output, and Python feature introspection.
- WebSocket/raw/H2 are not reported as runtime-supported if runtime refuses them.
- `supported_features()` does not list parser-only features as supported.
- CLI check emits blocked/run-possible status correctly.
- manifest status changes require test updates.

## Acceptance Criteria

- Trojan client/server status is resolved and documented consistently.
- Parser-only or protocol-crate-only features are not reported as user-supported runtime features.
- Python feature reporting is layer-aware or limited to runtime-supported features.
- CLI check output is layer-aware.
- README avoids overbroad parity claims.
- Every corrected status has a manifest entry and test coverage.

## Non-goals

This task does not implement WebSocket/raw/H2 runtime wiring unless it is trivial and already nearly complete. It focuses on truthfulness and closure of conflicting claims. Long-tail implementation belongs to Track C.

## Handoff Notes

Start with Trojan because it is the cleanest concrete contradiction. Then fix the broader reporting model so similar contradictions cannot recur. Prefer generated docs over manual edits wherever feasible.
