# pproxy Strict Phase 10 — Final Differential Closure and Claim Reset

## Status

Complete — the active manifest and compatibility matrix now reconcile the
implemented optional tails and record the remaining macOS PF and unavailable
legacy-cipher exclusions explicitly.

## Objective

Produce one evidence-backed final answer to the question: which `pproxy==2.7.9` behaviors can Eggress now claim as strict-compatible, and which exclusions remain?

This phase is deliberately small. It is not authorization to build a new certification apparatus.

## Preconditions

Mandatory:

- Phase 0 complete;
- Phase 1 complete.

Recommended strict core before attempting an unqualified replacement claim:

- Phases 2-6 complete.

Optional tail:

- Phases 7-9 either complete or explicitly declined with retained exclusions.

Do not mark a declined optional phase as complete merely because the roadmap allows declining it.

## Sources of truth to reconcile

- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/parity/README.md`
- top-level `README.md`
- Python packaging/readme compatibility text
- completed strict-parity plans

Historical reports may remain for provenance but must point readers at the active final matrix.

## Final differential set

Keep the suite compact and representative, but every capability promoted to `matched`/strict-compatible needs direct evidence appropriate to its type.

### Core CLI/URI

- no-argument default;
- mixed listener;
- representative `-l/-r/-ul/-ur` combinations;
- scheduler/rules;
- TLS;
- `--pac` / `--get` / `--test`;
- `--auth`;
- `--sys` on available supported platform;
- `--reuse`;
- `-d`, `-v/-vv`;
- unknown/malformed options.

### Protocol/runtime

- HTTP forward + CONNECT;
- SOCKS4/4a;
- SOCKS5 TCP and UDP;
- modern Shadowsocks TCP/UDP all supported AEAD methods;
- Trojan;
- H2;
- WS/WSS;
- SSR/plugins if Phase 3 enabled;
- direct/raw/tunnel;
- Unix sockets;
- Linux redir;
- reverse/backward;
- supported multi-hop TCP and UDP compositions.

### Optional transports/tail

Only if enabled:

- SSH direct/jump/remote forward;
- QUIC/H3 TCP and applicable UDP;
- legacy cipher/OTA vectors;
- macOS PF;
- daemonization.

### Python

From a clean built wheel:

- every required module import;
- tracked signatures/async classification;
- top-level factories/constants;
- one real TCP listener/upstream flow;
- one UDP callback/flow where in scope;
- optional feature failure behavior;
- `python -m pproxy`.

## Evidence rules

Use three evidence classes only:

1. **source/oracle structural** — exact module/flag/signature contract;
2. **paired pproxy differential** — behavior directly compared with 2.7.9;
3. **external interoperability** — standards/ecosystem protocol validation.

A local Eggress-to-Eggress roundtrip is regression evidence, not strict compatibility evidence.

Skipped external tests are not passes. Record them as unavailable and retain the weaker status until executed.

## Status vocabulary

Use a minimal active vocabulary:

- `matched` — strict observable match with adequate evidence;
- `supported_difference` — usable but observably different;
- `platform_limited` — compatible only on identified platforms/capabilities;
- `intentional_non_parity` — deliberately not implemented;
- `gap` — intended strict target not yet implemented.

Remove stale statuses that imply certification levels the project no longer uses.

## Claim decision

### Unqualified claim allowed only if

All actual 2.7.9 capabilities selected as part of the final strict target are `matched` or legitimately `platform_limited`, and there are no remaining `gap` or `intentional_non_parity` records that would make normal `pproxy` programs/configurations fail unexpectedly.

Possible wording:

> Eggress is a pproxy 2.7.9-compatible replacement across the documented CLI, protocol, and Python surfaces, subject to listed platform requirements.

Do not say `100%` unless the project defines a quantitative metric, which this roadmap does not require.

### Qualified claim if optional tail remains excluded

If SSH, QUIC/H3, legacy crypto/OTA, PF, daemonization, or other actual upstream capabilities remain excluded, retain explicit wording such as:

> Eggress provides broad pproxy 2.7.9 compatibility for modern HTTP/SOCKS/encrypted-proxy, routing, CLI, UDP, reverse, and Python workflows, with explicit exclusions listed in the compatibility matrix.

The exact final wording must reflect the remaining matrix, not this example.

## Cleanup

After final classification:

- remove stale diagnostics for features now implemented;
- remove false-gap records identified in Phase 0;
- delete dead compatibility shims only when no active Python/CLI path uses them;
- update completed strict plans with a one-line status/result, not large retrospective rewrites;
- point older roadmap banners to the final active matrix.

Do not delete useful historical evidence/provenance files simply to reduce file count.

## Verification commands

Keep routine checks proportional. Typical closure run:

```bash
cargo fmt --check
cargo test --workspace
python -m pytest python/tests
```

Then run the dedicated local strict/oracle command(s) for enabled external features. Do not turn every external topology into a mandatory always-on CI job.

## Acceptance criteria

1. The active manifest contains exactly the real 2.7.9 target, with no false-gap flags or commands.
2. Every `matched` runtime protocol has paired pproxy or external interop evidence; self-roundtrip alone is insufficient.
3. Shadowsocks modern AEAD has bidirectional pproxy and standards evidence.
4. Every installed Python module/symbol claimed as strict-compatible is tested from a built wheel.
5. Every enabled optional feature has its phase-specific external evidence.
6. Every declined optional feature remains clearly `intentional_non_parity`; none is hidden by aggregate wording.
7. README/package claims match the final matrix exactly.
8. There is one active compatibility source of truth and historical files point to it.
9. No new broad CI/certification framework is added during closure.
10. The final result supports either a defensible unqualified compatibility statement or a precise qualified statement with explicit remaining exclusions.
