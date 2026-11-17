# Parity Compat Milestone 003 — Closure Status

Status: closed

Source implementation plan:

- `plans/implementation/parity-compat/003-auth-reuse-and-compat-redaction.md`

Source subsystem roadmaps:

- `plans/subsystems/parity-compat-roadmap.md` §7 Milestone 3, §12 milestone status
- `plans/subsystems/security-hardening-roadmap.md` §7 Milestone S6

Repository baseline reviewed: `ba64e16` (branch `feat/auto-20261116-652bb64d`,
workspace `1.0.12`) plus this pass's uncommitted working-tree changes

Implementation commits or pull requests:

- None. This pass is uncommitted working-tree work in the shared corrective
  wave; no commit, tag, or publication was created by this milestone.

## 1. Executive finding

The pproxy-compatible authentication contract is now documented truthfully and
the compatibility surface no longer leaks credentials through the paths this
milestone owns.

- `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md`,
  `docs/PPROXY_MIGRATION.md`, and `docs/cli/PPROXY_CLI_INVENTORY.md` now state
  that listener credentials come from the listener URI (pproxy's `#user:pass`
  fragment, or `user:pass@` userinfo), that the translator emits
  `[listeners.auth]` from those credentials with no `--auth` involvement, and
  that `--auth N` is only the source-IP **re-authentication reuse interval**
  (`2592000` s / 30 days by default, including when omitted; `0` disables
  practical reuse).
- Compatibility startup emits exactly one warning for the process-global,
  source-IP-keyed, cross-listener reuse cache. It fires after translation and
  `--test` early-return, before daemonization and serving, only when a
  translated listener actually carries `[listeners.auth]`. The text is static
  and contains no URI or credential.
- Invalid UDP remote (`-ur`) errors use the canonical
  `redact_uri_for_error`, which is now hop-aware and authority-bounded, so
  credentials never reach the diagnostic while the reason and endpoint survive.
- `eggress pproxy translate --annotate` renders its reproduction line through
  `redact_argv_for_annotation`: flags, chain separators, and query strings are
  preserved; userinfo becomes `****:****@` / `****@`; fragment auth becomes
  `#****`. Generated TOML is unchanged and still contains the real credentials
  needed to run.
- `eggress pproxy check --json` keeps its recognized display paths redacted
  (`parsed_uris.listeners`/`remotes`, diagnostics). Its documented "raw"
  fields (`raw_args`, `chain_info[].raw`) remain raw; see §10 for why that was
  left to a contract decision rather than changed inside this milestone.
- `ReverseServerToml` and `ReverseClientToml` use hand-written `Debug` impls
  that print `Some(****)` for `auth_password` (and `server_uri`), while
  serialization keeps the real values.

No compatibility tier or manifest claim changed, and no pproxy default, cache
scope, or cache expiry was touched. Closure is conditional on one item only:
the coordinator's workspace-wide `fmt`/`clippy`/`test` gate, which runs after
every corrective wave has landed.

## 2. Requirement-to-evidence matrix

| Requirement | Evidence | Result | Notes |
|---|---|---|---|
| Docs describe credentials as URI-borne (fragment / userinfo), not `--auth`-gated | `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md`, `docs/PPROXY_MIGRATION.md`, `docs/cli/PPROXY_CLI_INVENTORY.md` diffs | pass | No remaining "`--auth` enables authentication" or "no `[listeners.auth]` unless `--auth`" claim |
| Docs state `--auth` default `2592000` (30 days), including when omitted | same diffs; `PproxyArgs::effective_auth_timeout()` unchanged at `86_400 * 30` (`crates/eggress-pproxy-compat/src/args.rs`) | pass | Default existed; only documentation was wrong |
| Docs state cache is process-local, bounded, monotonic, IP-only, shared across credentialed listeners | same diffs; `AuthReuseCache` (`crates/eggress-server/src/accept/mod.rs:41-49`) untouched | pass | 4096-entry bound and lazy eviction documented |
| Docs offer `--auth 0` / native `[listeners.auth]` escape hatches | same diffs | pass | |
| One startup warning names the 30-day default, IP-only key, and cross-listener cache | `PproxyArgs::auth_reuse_advisory()` (`args.rs`), `auth_reuse_advisory_for()` + `execute()` (`crates/eggress-cli/src/pproxy_exec.rs:197-207,248-251`) | pass | Verbatim text in §3 below |
| Warning fires once at startup, never per connection | single call site in `execute`, after the `--test` early return, before daemonize/serve | pass | Manual run showed exactly one occurrence |
| Warning only with credentialed listeners | unit test `pproxy_exec::tests::auth_reuse_advisory_fires_for_credentialed_listener_only` (gated on compiled `[listeners.auth]`); manual unauthenticated run produced zero advisories | pass | Also gated off translation-only subcommands |
| Warning never echoes credentials or listener URIs | static text; unit tests assert no `user`/`pass`/host/port; manual `grep -c secret` = 0 on stderr and stdout | pass | |
| Invalid UDP remote errors redact raw URI | `crates/eggress-pproxy-compat/src/translate/intermediates.rs:855-865`; tests `translate::tests::udp_remote_errors_redact_credentials`, `translate::tests::udp_remote_error_redacts_percent_and_bracket_passwords` | pass | Both "invalid URI" and "no proxy hop" paths |
| Passwords with `@`, `:`, brackets, percent escapes never leak | `uri::tests::test_redact_uri_for_error_hides_credentials` (last-`@`, `p[a]ss`, unmatched `[` fallback, `%40`/`%3A`, username-only, chain, fragment, 256-bound) | pass | |
| `--annotate` redacts argv but preserves flags, chains, queries | `uri::redact_argv_for_annotation` + `uri::tests::test_redact_argv_for_annotation_preserves_flags_and_chains`, `..._username_only_and_fragments` | pass | `-l`/`-r`/`--auth` preserved |
| `--annotate` integration with credentialed `-l`/`-r` pair | `crates/eggress-cli/tests/pproxy_cli.rs::translate_annotate_redacts_credentials_but_keeps_toml_runnable` | pass | Asserts `****:****@` and absence of both secrets in the annotation line |
| `pproxy check --json` recognized displays redacted | `crates/eggress-cli/tests/pproxy_cli.rs::check_json_redacts_parsed_uris_and_diagnostics` | pass | `parsed_uris.listeners`/`remotes` and diagnostics redacted; `raw_args`/`chain_info[].raw` intentionally raw (§10) |
| Generated TOML unchanged and runnable (real credentials kept) | same CLI test asserts `password = "supersecret"` and the credentialed upstream URI remain in the TOML; translation code untouched | pass | |
| Compat IR `Debug` never prints `auth_password` | custom `Debug` in `crates/eggress-pproxy-compat/src/translate/model.rs`; test `translate::tests::compat_ir_debug_redacts_reverse_auth_password` | pass | Prints `Some(****)`; JSON serialization still contains the real value |
| No new flag, no cache scope/expiry change | diff review: `AuthReuseCache`, `from_facade`, supervisor wiring untouched; only advisory text added | pass | |
| Manifest claim unchanged | `python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml` → PASS 162 capabilities; generated-report roundtrip consistent; `docs/parity/` unmodified | pass | See §4 |
| Focused Rust verification green | commands in §4 | pass | |
| Coordinator workspace-wide gate | not run in this wave by instruction | partial | Sole closure condition |

## 3. Production implementation evidence

Verbatim warning emitted by both compatibility entry points (prefix
`warning: ` for `eggress pproxy run`, `pproxy: warning: ` for the standalone
`pproxy` binary):

```text
pproxy-compatible authentication reuse is active for listener credentials: the reuse cache is process-global, keyed only by source IP, and shared across credentialed compatibility listeners (bounded, monotonic expiry). Reuse interval: 2592000s (30 days) when --auth is omitted; effective for this process: 2592000s. Set --auth 0 to disable practical reuse, or use native [listeners.auth] for per-connection checks with no reuse cache.
```

Placement: `eggress_cli::pproxy_exec::execute`, after the `--test` diagnostic
early return and before `maybe_daemonize`/supervisor start, gated on
`rt_config.listeners.iter().any(|l| l.auth.is_some())`. The standalone `pproxy`
binary and `eggress pproxy run` both delegate to `execute`, so the warning is
emitted once per process, never per connection.

Ownership changes landed:

- `crates/eggress-pproxy-compat/src/uri.rs` — `redact_uri_for_error` is now
  `pub(crate)`, hop-aware (redacts every `scheme://` hop in a `__` chain), and
  authority-bounded (an `@` in a query/path is no longer treated as userinfo);
  new `redact_uri_token` and public `redact_argv_for_annotation`; last-`@`
  fallback for unmatched brackets keeps the redactor total.
- `crates/eggress-pproxy-compat/src/args.rs` — new
  `PproxyArgs::auth_reuse_advisory()`; `effective_auth_timeout()` unchanged.
- `crates/eggress-pproxy-compat/src/translate/intermediates.rs` — UDP remote
  errors redact the raw `-ur` value; the `auth-timeout` translation warning
  now says `--auth` sets the reuse interval and credentials come from the URI.
- `crates/eggress-pproxy-compat/src/translate/model.rs` — hand-written `Debug`
  for `ReverseServerToml`/`ReverseClientToml`; serialization unchanged.
- `crates/eggress-cli/src/commands/pproxy.rs` — `--annotate` uses
  `redact_argv_for_annotation`. `check --json` was reviewed but its documented
  "raw" fields were left unchanged (§10).
- `crates/eggress-cli/src/pproxy_exec.rs` — startup advisory helper and call.
- Tests: compat unit tests (uri/args/translate), `pproxy_exec` unit test,
  annotated-translate CLI test.

Not implemented (out of scope by plan §5): no new auth flag, no native
auth-cache configuration, no change to the pproxy reuse default or cache
scope, no SOCKS4 credential enforcement (Protocols Edge M003).

## 4. Verification executed

### Commands run

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test cli_exit_codes
cargo test -p eggress-cli --test pproxy_cli
cargo test -p eggress-cli --test pproxy_binary
cargo test -p eggress-cli --test pproxy_translation_golden
cargo test -p eggress-cli --lib auth_reuse_advisory
cargo test -p eggress-runtime --test security_invariants
cargo test -p eggress-embed --test error_redaction
cargo clippy -p eggress-pproxy-compat -p eggress-cli --all-targets -- -D warnings
cargo fmt -p eggress-pproxy-compat -p eggress-cli -p eggress-runtime
cargo fmt -p eggress-pproxy-compat -p eggress-cli -p eggress-runtime -- --check
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --write-report <tmp>/report.md docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report <tmp>/report.md docs/parity/pproxy_capability_manifest.toml
```

Manual process checks (real binaries, listener killed after ~2 s):

```bash
target/debug/eggress pproxy run -- -l 'socks5://user:secret@127.0.0.1:19899'
target/debug/pproxy -l 'socks5://u:p@127.0.0.1:19897'
target/debug/eggress pproxy run -- -l 'socks5://127.0.0.1:19898'
```

### Results

- `cargo test -p eggress-pproxy-compat` — pass: 373 lib tests, 6
  `native_equivalence`, 11 `uri_syntax_equivalence`, 0 doc tests; 0 failed.
- `cargo test -p eggress-cli --test cli_exit_codes` — pass: 5/5.
- `cargo test -p eggress-cli --test pproxy_cli` — pass: 14/14, including the
  new annotated-translate and `check --json` redaction tests.
- `cargo test -p eggress-cli --test pproxy_binary` — pass: 34/34 (extra
  confidence for the changed `--auth` warning text and startup banner).
- `cargo test -p eggress-cli --test pproxy_translation_golden` — pass: 9/9;
  no fixture change was needed (the goldens do not pin the `--auth` warning).
- `cargo test -p eggress-cli --lib auth_reuse_advisory` — pass: 1/1.
- `cargo test -p eggress-runtime --test security_invariants` — pass: 8/8.
- `cargo test -p eggress-embed --test error_redaction` — pass: 12/12.
- `cargo clippy -p eggress-pproxy-compat -p eggress-cli --all-targets -- -D warnings`
  — pass, no diagnostics.
- `cargo fmt -p eggress-pproxy-compat -p eggress-cli -p eggress-runtime` and
  its `-- --check` re-run — clean (no reformat of unrelated packages;
  `cargo fmt --all` was not used).
- Manifest `--strict` — PASS, 162 capabilities (`drop_in` 90,
  `compatible_with_warning` 54, `native_equivalent` 16,
  `intentional_non_parity` 2, `unsupported` 0).
- Report roundtrip — "Report is consistent with manifest", exit 0.
- Manual `eggress pproxy run` with credentials — exactly one advisory on
  stderr; `grep -c secret` on stdout and stderr = 0.
- Manual standalone `pproxy` with credentials — exactly one advisory with the
  `pproxy: ` prefix; no credential.
- Manual `eggress pproxy run` without credentials — zero advisories (only the
  pre-existing `[direct-mode]` warning).

Environment note: the assigned command
`python3 scripts/validate_pproxy_parity_manifest.py --check-report` is
incomplete — the script requires a report path argument. The equivalent
supported checks above (`--strict`, then `--write-report` followed by
`--check-report` on a temporary generated report) were run instead; the
committed tree has no report file to check against.

**Coordinator broad gate (2026-11-16, after all six corrective waves):** pass —
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace --locked`, the documented CLI/embed feature slices, the
embed OpenSSH regression (6/6), and `cargo deny --workspace check`
(advisories/bans/licenses/sources). `cargo audit --ignore RUSTSEC-2023-0071`
reports only the four tracked `zip 6.0.0` advisories. The condition recorded in
§11 is therefore satisfied.

## 5. Invariant review

| Source-plan invariant (§4) | Evidence | Status |
|---|---|---|
| pproxy 2.7.9 behavior remains the default; no tier change without manifest + oracle evidence | `AuthReuseCache`/`from_facade`/supervisor untouched; manifest `--strict` PASS; no `docs/parity/` change | holds |
| `PproxyUri::redacted_display`, `RedactedUri`, `CredentialSpec` redaction remain canonical | untouched; `uri_syntax_equivalence::redaction_hides_credentials_both_grammars` passes | holds |
| Compatibility diagnostics remain structured and machine-readable | no diagnostic enum/schema change; one plain startup warning line | holds |
| `translate` still emits usable TOML with real credentials | `translate_annotate_redacts_credentials_but_keeps_toml_runnable`; existing translate goldens pass | holds |
| `--annotate` remains useful for reproduction | flags/chain separators/query preserved; only userinfo/fragment replaced with placeholders | holds (fragment-suffix nuance in §10) |
| No new auth flag or credential store | diff review | holds |

## 6. Failure and recovery review

- Duplicate delivery / idempotency: the advisory is emitted from one call site
  per process start; repeated invocations each warn once. No idempotency state
  was added.
- Cancellation races: none introduced; the advisory is a synchronous
  `eprintln!` before bind/serve.
- Process or supervisor restart: the warning re-emits once on each new
  compatibility process, which is the intended operator-visible behavior.
- Partial persistence failure: not applicable (no persistence added).
- Stale generation or lease: not applicable.
- Contention and resource release: redaction allocates one bounded `String` per
  call (`<= 256` bytes for error output) and holds no locks.
- Malformed or unauthorized input: `redact_uri_for_error` is total — it
  tolerates non-URI tokens, unmatched brackets/braces, empty authorities, and
  multi-hop input, never panics, and never echoes the raw input; covered by
  unit tests.
- Bounded relay and association behavior: unchanged.

## 7. Migration and compatibility review

- Config migration: none required. Documentation now describes the existing
  behavior; operators who believed `--auth` was the authentication switch must
  put credentials in the listener URI and use `--auth` only for the reuse
  interval.
- Backward compatibility: no CLI, URI, translation, or runtime behavior change
  beyond one additional startup warning line and redacted diagnostics.
- Protocol negotiation / translator tiers: unchanged. No manifest or matrix
  edit; no oracle/differential/interop run was required (the claim did not
  change).
- Rollback: revert the three documentation files and the Rust source/test files
  listed in §3; no persistent state, schema, or cache change exists to roll
  back.

## 8. Security review

- Authorization enforcement: unchanged (`[listeners.auth]` from URI
  credentials, `AuthReuseCache` semantics identical).
- Secret handling: warning text is static with no identifiers; `--annotate`
  redacts userinfo and fragment auth; UDP remote errors use the canonical
  redactor; compat IR `Debug` hides `auth_password`/`server_uri` while
  serialization preserves them.
- Path validation / privilege boundaries: untouched.
- Denial-of-service bounds: redaction output is bounded to 256 bytes for error
  messages; the auth cache remains the pre-existing 4096-entry bounded cache
  with monotonic expiry.
- Redaction: unit tests cover `@`-in-password, `:`-in-password, balanced and
  unmatched brackets, percent escapes, username-only userinfo, IPv6 hosts,
  chains, query strings, and fragments.
- Residual exposures (see §10): `pproxy check --json` `raw_args` and
  `chain_info[].raw`; other compat IR `Debug` derives
  (`ListenerToml`/`AuthToml`/`UpstreamToml`); Python embedding entry point has
  no advisory.

## 9. Documentation and operations

- `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md` — replaced the false
  "No Authentication by Default" section with the URI-credential contract, a
  dedicated source-IP reuse section, corrected translate-time wording, and
  corrected comparison tables.
- `docs/PPROXY_MIGRATION.md` — new "Listener credentials and `--auth`"
  subsection; compatibility-boundary bullet corrected.
- `docs/cli/PPROXY_CLI_INVENTORY.md` — `--auth` row and a paragraph separating
  URI credentials from the reuse interval.
- Operator diagnostics: one startup warning (text in §3); the `--auth`
  translation warning now says it sets the reuse interval.
- Recovery instructions: none needed (documentation/warning/redaction only).
- `docs/SECURITY_REVIEW.md` was not edited because it is outside this
  milestone's owned file list; the coordinator should fold the redaction
  evidence into it if that document tracks it.

## 10. Unresolved findings

| Severity | Finding | Impact | Required action |
|---|---|---|---|
| medium | `eggress pproxy check --json` emits credentialed URIs in its documented "raw" fields: `raw_args` (original argv) and `parsed_uris.chain_info[].raw` (raw chain string). The `cli.check_json_chain_info` manifest entry explicitly describes `raw`, Python contract tests assert `raw_args` equality, and `docs/PPROXY_MIGRATION.md` documents `raw_args` as the original arguments. Redacting either field changes a documented, manifest-described output contract, so it was **not** done unilaterally in this milestone. | Machine-readable check output can contain credentials when piped to logs or committed as evidence. | Open a follow-up that updates the manifest `eggress_behavior` text, the migration doc, the Python contract/tests, and the CLI together; then redact (or add `redacted_args`/`redacted_chain` alongside). Tracked as the primary residual of M003. |
| low | Other compat IR structs still derive `Debug` (`ListenerToml.auth`, `AuthToml.password`, `ShadowsocksToml.password`, `TrojanToml.password`, `UpstreamToml.uri`, and `TranslationIntermediates`). No production log path prints them today. | Latent leak if a future diagnostic prints the IR. | Extend the hand-written `Debug` pattern in a follow-up; `ReverseServerToml`/`ReverseClientToml` are closed. |
| low | Annotated argv truncates at the first `#` when a token carries fragment auth (`#****`), so a fragment-bearing hop followed by `__` chain hops loses those trailing hops in the annotation line only. | Reproduction value for a rare `-l a#u:p__b` shape; no credential exposure. | Accept, or make fragment redaction hop-aware if the shape becomes common. |
| low | The Python embedding path (`EggressService.start_with_compatibility_options`) constructs the same reuse cache but does not print the advisory; Python sources are outside this milestone's owned files. | Python-embedded deployments do not see the startup warning. | Add the advisory in the Python bindings surface if parity of operator visibility is required. |
| low | No process-level regression test asserts the advisory in `pproxy_run_process.rs` (test ownership for this milestone was limited to the `--annotate` CLI test). | The call site is covered by unit tests plus manual process verification, not a process test. | Add a process assertion in a test-ownership-expanded follow-up. |
| low | Workspace-wide `fmt`/`clippy`/`test` gate not run in this wave (coordinator owns it). | Integration risk with concurrent waves. | Coordinator runs the broad gate once after all corrective waves. |

## 11. Roadmap disposition

Milestone closed. Production implementation, focused tests, documentation, and
manifest validation for M003 are complete, the coordinator's workspace-wide
`fmt`/`clippy`/`test` gate passed on 2026-11-16, and the manifest claim is
unchanged.

The `check --json` raw-field exposure in §10 is a pre-existing,
manifest-described output contract outside M003's stated redaction scope
(UDP errors, `--annotate`, compat IR `Debug`). It is tracked for a follow-up and
is **not** a closure condition for this milestone. No release, tag, or publish
action is part of this milestone.

## 12. Registry updates

Required in `plans/registry.md` (owned by the coordinator, not edited here):

- mark Parity Compat M003 as closed, pointing at
  `plans/closure/parity-compat/003-status.md`;
- mark Security Hardening S6 (same plan/evidence) as closed in
  `plans/subsystems/security-hardening-roadmap.md` §12;
- record that the coordinator workspace-wide gate passed on 2026-11-16.

Applied in the source subsystem roadmap by this milestone:
`plans/subsystems/parity-compat-roadmap.md` §7/§12 now report M003 as
closed with the closure record path. The manifest, matrix, and
generated reports are unchanged.
