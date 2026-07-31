# Phase 2 — pproxy Routing, Rule, and Scheduler Parity

## Status

Proposed.

## Parent roadmap

`plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`

## Depends on

Phase 1 URI and CLI AST.

## Objective

Make pproxy compatibility mode select remotes, apply rules, and fall through in the same order as pproxy 2.7.9 for common documented configurations.

Eggress already has a more expressive native rule engine. This phase must translate pproxy's simpler model into that engine without changing native routing semantics.

## Current defects to close

- Multiple remotes default to round-robin in the translator, while pproxy defaults to first-available.
- Per-remote rule suffixes are parsed incompletely or not at all.
- `--rulefile` content is currently treated mainly as global reject/block patterns rather than as routing predicates attached to remotes.
- The relationship between `-b`, per-remote rules, unmatched traffic, and direct fallback is inconsistent across documents and code.
- Remote order may be lost when groups are generated.
- Existing parity entries overstate support for rule-file translation.

## In scope

- default first-available scheduler behavior;
- explicit scheduler mappings for pproxy's documented scheduler values;
- stable preservation of remote declaration order;
- per-remote rules from URI suffixes;
- global rule-file behavior where pproxy exposes it;
- block expressions;
- unmatched-traffic fallthrough;
- direct routing when no remote matches;
- route explanation output that reflects translated pproxy ordering;
- TCP and standalone UDP routing where pproxy uses the same rule model;
- focused oracle comparisons for ambiguous rule precedence.

## Out of scope

- redesign of Eggress's native TOML rule language;
- dynamic policy engines;
- PAC JavaScript evaluation;
- external rule subscriptions;
- distributed routing state;
- retry orchestration beyond pproxy behavior;
- implementing unsupported transports so they can be selected by a rule;
- generalized regex sandboxing or process isolation.

## Required behavior model

Build one documented compatibility routing model from oracle probes. At minimum determine and encode:

1. The default scheduler with one remote.
2. The default scheduler with multiple remotes.
3. How explicit `-s` changes selection.
4. Whether rules belong to the immediately preceding remote or another scope.
5. First-match or first-eligible behavior.
6. What unmatched traffic does when some remotes carry rules.
7. Interaction between block rules and remote rules.
8. Interaction between rule-file read/parse failure and startup.
9. Rule matching input: hostname, textual host:port, resolved address, or another value.
10. Whether TCP and UDP use identical rule ordering.

Use a small local oracle fixture with two loopback proxies and distinguishable responses. Do not add a permanent complex test harness.

## Translation design

### Ordered routes

Translate pproxy remotes into an ordered compatibility route list before lowering to native Eggress config.

Suggested intermediate representation:

```rust
struct CompatRoute {
    declaration_index: usize,
    upstream_or_chain: CompatUpstream,
    predicate: Option<CompatRuleSet>,
}
```

The translator should then lower this into existing Eggress rules and upstream groups while preserving declaration order.

### Default scheduler

When no scheduler flag is supplied:

- one remote: first-available;
- multiple remotes: first-available in declaration order.

Do not infer round-robin from remote count.

When the user explicitly supplies a scheduler, map only documented values. Unknown values should fail or warn according to pproxy behavior, not silently change to another scheduler.

### Per-remote rules

A rule attached to a remote must route matching traffic to that remote or chain. It must not become a global reject rule.

Suggested lowering:

```text
remote A + rules A  -> rule A, action group-A
remote B + rules B  -> rule B, action group-B
unruled remote C    -> final catch-all group-C
no unruled remote   -> direct or reject according to oracle behavior
```

Each compatibility remote may use a one-member group if that keeps lowering simple. Avoid inventing a new runtime route action.

### Block expressions

`-b` remains a high-priority reject rule. It should precede route rules if that matches the oracle. Validate this with one focused case.

### Rule-file parsing

Reuse `CompatRegex` and existing file-size/entry limits. Correct semantic lowering rather than replacing regex infrastructure.

Support the pproxy formats actually accepted by 2.7.9. If pproxy rule files are simply regex lines, preserve line order and comments. If action syntax exists, implement only actions observed in the frozen oracle and document unsupported forms precisely.

Do not classify a line as a block merely because the translator cannot determine its route target.

## Workstream 2.1 — Oracle precedence probes

Create a compact test script or Rust integration test that starts:

- two distinguishable local HTTP or SOCKS upstreams;
- one pproxy oracle process;
- requests to matching and non-matching hostnames.

Record normalized outcomes for the ten behavior questions above. Keep fixtures small and local-only.

## Workstream 2.2 — Correct scheduler defaults

1. Change translator default to first-available regardless of remote count.
2. Preserve remote declaration order in group members.
3. Verify explicit `rr`, `fa`, random-choice, and least-connection mappings only where pproxy documents them.
4. Ensure reload preserves or resets state only where observable compatibility requires it; do not refactor native scheduler state merely for an unobserved detail.
5. Update compatibility diagnostics and docs.

## Workstream 2.3 — Implement per-remote rule lowering

1. Consume `PproxyRuleRef` from the Phase 1 AST.
2. Load or compile rules before config emission.
3. Generate one ordered native rule per compatibility route predicate.
4. Generate a final catch-all route according to oracle behavior.
5. Keep rule IDs deterministic and useful in route explanation.
6. Preserve chain identity when a rule points to a multi-hop remote.
7. Return clear errors for missing or unreadable required rule files.

## Workstream 2.4 — Correct global `--rulefile` behavior

Determine whether the global flag applies to every remote, to route selection, or to another pproxy subsystem. Implement exactly that bounded behavior.

If the current parser accepts noncanonical historical forms, keep them as extensions only when unambiguous. Do not let extension behavior alter canonical pproxy configurations.

## Workstream 2.5 — TCP/UDP consistency

- Apply compatible route predicates to standalone UDP where pproxy does.
- Reject a rule-selected UDP route whose transport cannot relay UDP with a precise error.
- Preserve direct UDP fallback where the oracle does.
- Do not add multi-hop UDP in this phase.

## Workstream 2.6 — Route explanation and diagnostics

Make `route explain` show:

- compatibility route declaration index;
- source rule file or inline rule identifier;
- matched pattern;
- selected remote or chain;
- scheduler and eligible member order;
- final direct/reject fallback.

Do not create a separate compatibility explanation engine; annotate existing compiled rules sufficiently to expose the information.

## Acceptance criteria

Phase 2 is complete when:

- multiple remotes without `-s` use first-available in declaration order;
- explicit scheduler selection behaves as documented;
- per-remote rule suffixes route traffic to their associated remote rather than rejecting it;
- unmatched traffic follows the oracle's direct/fallback behavior;
- block expressions have the correct precedence;
- missing or malformed rule files fail or warn at the same stage as pproxy within reasonable message differences;
- route order remains stable after translation;
- chain remotes retain their full chain when selected by a rule;
- unsupported UDP route compositions are rejected before runtime;
- existing native Eggress TOML rules and schedulers are unchanged;
- parity documentation no longer marks routing complete where only native equivalents exist.

## Focused verification

```bash
cargo fmt --check
cargo test -p eggress-pproxy-compat rule
cargo test -p eggress-routing
cargo test -p eggress-cli --test cli_tests scheduler
cargo test -p eggress-runtime --test integration route
```

Run the local oracle precedence probe once before closure and retain only its concise fixture/output if useful. Do not add it to routine CI.

## Required regression cases

- two unruled remotes default to first-available;
- explicit round-robin alternates;
- rule A selects remote A;
- rule B selects remote B;
- unmatched request uses the expected fallback;
- `-b` rejects before a matching remote rule where appropriate;
- rule-selected multi-hop chain executes all hops;
- unreadable rule file produces a deterministic diagnostic;
- TCP rule matches do not accidentally capture UDP traffic when transport differs.

## Rollback and compatibility notes

Changing the default from round-robin to first-available may alter behavior for current Eggress users who relied on the incorrect compatibility translation. Restrict the change to pproxy translation mode. Native TOML groups must keep their configured scheduler.

## Handoff guidance

Commit in this order:

1. oracle precedence fixtures;
2. default scheduler correction;
3. per-remote rule lowering;
4. global rulefile correction;
5. UDP consistency and route explanation;
6. documentation cleanup.

Avoid touching protocol codecs or Python packaging in this phase.
