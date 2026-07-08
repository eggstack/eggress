# Track A.03: URI, Rulefile, and Regex Compatibility

## Objective

Close the pproxy URI/rule compatibility gap for common routing workflows, including Python-regex-like rule files. The compatibility path should use `fancy_regex` for expanded regex support, while documenting the remaining mismatch against Python `re` semantics as an upstream/runtime gap rather than silently diverging.

This task is central to pproxy parity because pproxy uses URI-attached rule files and regex matching to decide whether a remote applies or traffic falls back to direct.

## Current Problem

The current egress native rule model is stronger and more typed than pproxy's rule model, but the pproxy translator appears to treat some rulefile cases as a limited subset. pproxy users expect `?rules_file` on remote URIs and `-b BLOCK` regex flags to behave like pproxy. They do not expect to rewrite rules into egress TOML before migration.

Additionally, Rust `regex` is not a Python `re` replacement. It rejects look-around, backreferences, and other constructs that may appear in pproxy rule files. `fancy_regex` should be added for compatibility-mode matching.

## Design Decision: Dual Regex Backend

Implement two regex paths:

1. Native egress fast path: keep `regex` for native structured rules where grammar compatibility is not a pproxy goal.
2. pproxy compatibility path: use `fancy_regex` for pproxy rule files and `-b` block regexes.

The compatibility evaluator should record which backend was used and expose diagnostics for unsupported or ambiguous patterns.

## Why `fancy_regex`

`fancy_regex` supports a larger Perl/Python-like feature set than Rust `regex`, including look-around and backtracking-oriented matching. It still is not a byte-for-byte Python `re` engine, so documentation must be precise. The goal is practical compatibility improvement, not a false claim of complete Python regex equivalence.

## Cargo and Crate Changes

Add `fancy-regex` to workspace dependencies, preferably optional at first if binary size or dependency policy requires it. Suggested names:

- Cargo dependency: `fancy-regex`
- crate feature: `pproxy-compat-regex` or enabled by default if pproxy compatibility is a core feature.

Add a small abstraction:

```rust
enum CompatRegex {
    Fast(regex::Regex),
    Fancy(fancy_regex::Regex),
}
```

or a trait wrapper that can report:

- pattern;
- backend;
- compile error;
- match error;
- whether matching can be considered deterministic.

## URI Grammar Work

Ensure the pproxy URI parser preserves, validates, and translates:

- compound schemes: `http+socks4+socks5://`;
- TLS modifiers: `+ssl`, `+tls`, `+secure` where pproxy accepts them;
- remote chain separator `__`;
- local bind suffixes: `/@in`, `/@127.0.0.1`, `/@::1`;
- rulefile suffixes: `?rules_file`;
- auth fragments: `#user:pass`;
- Unix socket netlocs such as `/tmp/pproxy_socket`;
- base64 Shadowsocks cipher strings, even if legacy cipher execution is deferred;
- OTA marker `!`, even if execution is deferred;
- plugin suffixes for SSR, even if SSR execution is deferred;
- raw target variants such as `tunnel{ip}://` and `ws{dst_ip}://`, even if runtime wiring is Track C.

Parser support is not the same as runtime support. The parser should preserve valid pproxy syntax and the translator/runtime should classify unsupported execution paths through manifest-backed diagnostics.

## pproxy Rule Semantics

Implement pproxy-style remote rule behavior:

- a remote URI with `?rules_file` applies only when the target host matches a regex in that file;
- unmatched traffic falls through to the next matching remote or direct fallback, matching pproxy behavior;
- multiple `-r` entries with different rule files are evaluated in pproxy order;
- direct fallback should be explicit in generated egress config when pproxy would go direct;
- `-b BLOCK` rules should reject before upstream selection where pproxy would block;
- comments and blank lines in rule files should be ignored;
- invalid regexes should produce early diagnostics, not runtime panics.

Confirm exact pproxy ordering with oracle tests before freezing behavior.

## Rulefile Loader

Create a compatibility loader that produces structured rules:

```rust
struct PproxyRuleFile {
    path: PathBuf,
    entries: Vec<PproxyRuleEntry>,
    diagnostics: Vec<RuleDiagnostic>,
}

struct PproxyRuleEntry {
    line_number: usize,
    raw: String,
    regex: CompatRegex,
}
```

Track line numbers for precise error reporting.

## Diagnostics

Add stable diagnostic codes:

- `rulefile-read-error`
- `rulefile-invalid-regex`
- `rulefile-fancy-regex`
- `rulefile-python-regex-gap`
- `rulefile-empty`
- `block-invalid-regex`
- `localbind-partial`
- `uri-preserved-unsupported-component`

For `fancy_regex`-compiled patterns, optionally report a low-severity compatibility note in debug/check output, not in normal run output unless a semantic gap is detected.

## Documentation

Add a section to the compatibility docs:

- native egress rules use Rust `regex`/typed matchers;
- pproxy compatibility rule files use `fancy_regex`;
- this improves support for Python-like regex constructs;
- remaining gaps vs Python `re` are documented and surfaced as diagnostics;
- users who need exact Python `re` behavior should test with the oracle suite or migrate rules to native egress TOML.

Do not overstate exact Python regex parity.

## Tests

Add unit tests for:

- simple hostname regex;
- comment and blank line handling;
- invalid regex diagnostics;
- lookahead/lookbehind pattern that fails under Rust `regex` but works under `fancy_regex`;
- backtracking-heavy pattern with timeout/resource guard if applicable;
- `-b` block regex compilation;
- multiple remote rulefiles and fallback order;
- URI `?rules_file` preservation;
- local bind suffix parse preservation;
- auth fragment parsing.

Add oracle tests for:

- pproxy README rule example;
- one remote with google-domain rulefile and unmatched direct fallback;
- two remotes with distinct rule files;
- block rule precedence;
- invalid rulefile behavior, compared against pproxy where feasible.

## Resource Safety

Backtracking regex support can be abused. Add safeguards:

- compile-time pattern length limit;
- maximum rule entries per file;
- optional per-match timeout if supported or external guard around rule evaluation;
- warning for compatibility mode using expensive regex constructs;
- tests for hostile pattern behavior.

If `fancy_regex` cannot enforce a per-match timeout directly, document and bound rule count/pattern length while considering a future isolated evaluator.

## Manifest Updates

Update capability entries for:

- URI rule suffixes;
- `-b` block regex;
- `--rulefile` / pproxy rule file loading;
- native egress structured rules;
- regex backend compatibility;
- unsupported Python regex constructs.

Classify pproxy rulefile support as `compatible_with_warning` until oracle tests demonstrate enough equivalence for `drop_in`.

## Acceptance Criteria

- `fancy_regex` is integrated into the pproxy compatibility rule path.
- Native egress rules remain unaffected unless intentionally configured.
- pproxy README-style rule files work through compatibility mode.
- Rule matching fallback semantics are covered by tests.
- Unsupported regex constructs produce precise diagnostics.
- Manifest and docs state the regex compatibility boundary honestly.

## Non-goals

This task does not require SSR plugin execution, SSH, QUIC/H3, or exact Python `re` byte-level semantics. It improves compatibility materially and makes residual gaps explicit.
