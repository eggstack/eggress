# Parity Compat M003 — Auth Reuse Semantics and Compatibility Redaction

Status: implemented

Repository baseline: `feat/auto-20261116-652bb64d` at `ba64e16` (workspace 1.0.12)

Source roadmaps:

- `plans/subsystems/parity-compat-roadmap.md` (new milestone M003)
- `plans/subsystems/security-hardening-roadmap.md` (S6)

Long-term requirements:

- `plans/000-long-term-specification.md` §4 (invariant 3, 9), §5 (compat contract)
- `plans/001-terminology-and-domain-model.md` §3–§4 (tier vs status, manifest authority)
- `plans/002-long-term-roadmap.md` (Phases 7–8, 36–42)

Applicable ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

Primary class: invariant / polish

## 1. Objective

Make the pproxy-compatible source-IP authentication reuse contract truthful and
visible, and close compatibility-surface credential leaks:

1. Correct the security/migration documentation: credentials come from the URI
   fragment; `--auth` is the re-authentication interval, not the credential flag.
2. Make the 30-day default and the process-global, IP-keyed cache explicit at
   startup and in the docs.
3. Redact raw UDP remote URIs in compatibility errors and raw argv in
   `pproxy translate --annotate`.
4. Redact compatibility IR `auth_password` in `Debug`.

No compatibility tier changes are intended. If a change alters a manifest claim,
the manifest plus oracle/differential/interop gate applies.

## 2. Why this milestone is ready

- The pinned pproxy 2.7.9 oracle uses a class-level `AuthTable._auth`/`_user`
  keyed only by source IP and an `authtime` default of `86400*30`
  (`pproxy/server.py`). Eggress reproduces that with one
  `CompatibilityRuntimeHooks::auth_reuse` cache threaded into every listener
  and a 30-day `effective_auth_timeout()`.
- `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md:9,23,67` currently says
  authentication is absent unless `--auth` is used and that translation does
  not add `[listeners.auth]` without `--auth`. That is false: credentials come
  from the URI fragment, and `translate/intermediates.rs:264-265` adds listener
  auth from that fragment. `--auth` only changes the reuse interval.
- `translate/intermediates.rs:854-862` interpolates the raw `remote_str` into
  `CompatError::InvalidArgs`; the canonical redactor exists at
  `crates/eggress-pproxy-compat/src/uri.rs:227-231`.
- `crates/eggress-cli/src/commands/pproxy.rs:53-55` prints raw `args.args` for
  `--annotate`.
- `crates/eggress-pproxy-compat/src/translate/model.rs:159-181` derives `Debug`
  for `ReverseServerToml`/`ReverseClientToml` with `auth_password` plaintext.

## 3. Current implementation evidence

### Auth reuse

- `PproxyArgs::effective_auth_timeout()` returns `Duration::from_secs(86_400 * 30)`
  when `--auth` is omitted (`crates/eggress-pproxy-compat/src/args.rs:550-553`).
- `pproxy_exec` passes that value unconditionally to `from_facade`
  (`crates/eggress-cli/src/pproxy_exec.rs:254-257`).
- `from_facade` creates one cache (`crates/eggress-runtime/src/supervisor.rs:138-150`);
  `prepare_listener_set` passes the same `Arc` to every listener
  (`supervisor.rs:482-487`), and `cached_identity` looks up only by IP
  (`crates/eggress-server/src/accept/mod.rs:212-218`).
- The pproxy oracle has the same global IP cache and 30-day default, so this is
  compatibility behavior, not a divergence. The security problem is that the
  docs understate it, and an operator can believe `--auth` is what enables
  authentication.

### Redaction

- `redact_uri_for_error` strips userinfo and bounds input
  (`uri.rs:227-231`) but the UDP remote error path does not use it.
- `PproxyUri` has a redacting `Debug` and `redacted_display`
  (`uri.rs:59-130,703-707`); the raw `remote_str` path bypasses both.
- `--annotate` uses `args.args.join(" ")`; a `-l socks5://user:pass@...` command
  line is reproduced verbatim.
- `ReverseServerToml`/`ReverseClientToml` are internal serialization structs;
  no current production log path prints them, but `Debug` is a latent leak.

## 4. Invariants that must not regress

- pproxy 2.7.9 behavior remains the default; no tier change without manifest +
  oracle/differential/interop evidence.
- `PproxyUri::redacted_display`, `RedactedUri`, and `CredentialSpec` redaction
  remain the canonical output paths.
- Compatibility diagnostics remain structured and machine-readable.
- `translate` still emits usable TOML (it contains the real credentials needed
  to run); only annotations/errors are redacted.
- `--annotate` remains useful for reproduction, with credentials replaced by a
  clear placeholder.
- No new auth flag or credential store is introduced.

## 5. Scope

### In scope

- `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md`,
  `docs/PPROXY_MIGRATION.md`, `docs/cli/PPROXY_CLI_INVENTORY.md`, and related
  security wording.
- A startup warning in the pproxy compatibility path describing the 30-day
  global IP reuse and `--auth 0`/native-auth alternatives.
- `intermediates.rs` error redaction.
- `pproxy translate --annotate` argv redaction.
- Compat IR `Debug` redaction.
- Focused Rust/Python redaction tests and manifest validation.

### Explicitly out of scope

- Changing the pproxy auth-reuse default or cache scope; that would be a
  compatibility change.
- Adding a new `--no-auth-reuse` flag or native auth-cache config.
- Manifest tier changes unless the evidence shows one is required.
- SOCKS4 credential enforcement (Protocols Edge M003; coordinate the shared-cache
  test).
- Updater/CI changes (Delivery M004).

## 6. Required production changes

### A. Correct the `--auth` contract in docs

Rewrite the security-differences section to state:

- credentials are carried by the listener URI fragment (`#user:pass`), not by
  `--auth`;
- `--auth N` sets the source-IP re-authentication interval; the pproxy default
  is `2592000` seconds (30 days), including when the flag is omitted;
- the cache is process-local, bounded, monotonic-expiry, and keyed only by
  source IP, so it is shared across compatibility listeners with credentials;
- `--auth 0` disables practical reuse; use native `[listeners.auth]` (no reuse
  cache) or a trusted single-user environment for stricter behavior.

Correct `PPROXY_MIGRATION.md` and `CLI_INVENTORY` to match. Do not claim that
translation omits `[listeners.auth]` without `--auth`.

### B. Add a startup warning

When the compatibility facade has listener credentials and an auth-reuse cache
(which is the normal pproxy-compatible path), emit one warning that names the
30-day default, the source-IP-only scope, and the cross-listener cache. Redact
credentials and listener URIs.

Do not warn when there are no credentialed listeners, and do not emit it once
per connection.

### C. Redact compatibility errors

In `translate/intermediates.rs`, replace raw `remote_str` interpolation with
the canonical `redact_uri_for_error(remote_str)` (or
`egress_uri::redact_proxy_uri`) for both “invalid UDP remote URI” paths. Ensure
the error still preserves the reason and enough endpoint context to debug.

Add tests with:

- `socks5://user:pass@127.0.0.1:1080` malformed / no-hop cases;
- passwords containing `@` and brackets;
- a `file://` or non-URI input that still gets bounded/redacted.

### D. Redact `translate --annotate`

Add a small argv redaction helper:

- iterate arguments, preserving flags and non-URI values;
- for URI-shaped tokens, call `redact_proxy_uri`/`PproxyUri::redacted_display`;
- replace `userinfo` with `****:****@` (or `****@` when only a username is
  present);
- preserve the chain separator and query string.

Use it for the `# pproxy arguments:` annotation. Keep the generated TOML
unchanged (it must still be runnable). Add a CLI test with `--annotate` and a
credentialed `-l`/`-r` pair.

### E. Redact compat IR Debug

Replace the derived `Debug` for `ReverseServerToml`/`ReverseClientToml` with
custom implementations that print `auth_password: ****` (or `Some(****)`) and
keep other fields. Add a unit test asserting the password string does not appear
in `format!("{:?}", ...)`.

### F. Manifest/tier check

Check whether `cli.auth` or any SOCKS4/auth claim changes. Documentation and
redaction-only changes should not. If a warning/behavior change is judged to
alter the claim:

- update the manifest and matrix;
- run the oracle/differential suite;
- record the new tier and evidence in closure.

Do not edit generated reports as the source of truth.

## 7. Ordered work packages

### Work package A — Documentation correction

Intent: stop misdescribing authentication vs reuse.

Acceptance evidence: docs diff; no `--auth`-as-authentication claim remains.

### Work package B — Startup warning

Intent: make the compatibility cache visible without changing behavior.

Acceptance evidence: warning test; redaction; no per-connection spam.

### Work package C — Error/annotate redaction

Intent: remove raw URI/argv credential leaks.

Acceptance evidence: Rust/Python redaction tests; `redact_uri_for_error` reuse.

### Work package D — Debug redaction and manifest check

Intent: close the latent IR leak and confirm no tier drift.

Acceptance evidence: Debug test; manifest validator; differential only if claim
changed.

## 8. Failure, cancellation, restart, and contention semantics

- The startup warning is emitted once at compatibility startup, before serving.
- The auth-reuse cache remains bounded and monotonic; no change to expiry or
  eviction.
- Redaction helpers must be total and allocation-bounded; a malformed URI must
  not panic or echo raw input.
- `translate --annotate` remains deterministic.
- No runtime locking or reload changes.

## 9. Compatibility and migration

- Default pproxy-compatible behavior is unchanged; only warnings/docs/redaction.
- Operators who relied on the incorrect docs must migrate to:
  - credentials in the listener fragment (existing behavior), and
  - `--auth N` for the reuse interval, or `--auth 0`/native auth for no reuse.
- `translate` TOML remains valid and contains real credentials; do not paste
  annotated output with credentials into public logs.
- If a manifest claim changes, follow the tier/evidence gate.

## 10. Required tests

### Focused unit tests

- `redact_uri_for_error` / `redact_proxy_uri` for credentialed UDP remotes.
- `--annotate` output contains `****` and not the password/username.
- `ReverseServerToml`/`ReverseClientToml` Debug contains `****`.
- Startup warning includes 30 days/global scope and no credentials.

### Integration tests

- `egress pproxy translate` with `--annotate` and credentialed remotes.
- `egress pproxy check --json` remains redacted.
- Python `translate_pproxy_*` / `redacted_toml` tests remain green.
- `scripts/validate_pproxy_parity_manifest.py --check-report` remains green.

### Security and negative tests

- Malformed UDP remote with `user:pass@` does not leak.
- Passwords with `@`, `:`, brackets, and percent escapes do not leak.
- No raw `args.args` path in `--annotate`.

### Migration and compatibility tests

- Existing `cli.auth` manifest claim unchanged unless evidence is updated.
- Oracle/differential only if the claim changed.

## 11. Required verification commands

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test cli_exit_codes
cargo test -p eggress-runtime --test security_invariants
cargo test -p eggress-embed --test error_redaction
python3 scripts/validate_pproxy_parity_manifest.py --check-report
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

If a claim changed:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli \
  --test differential_pproxy -- --ignored --test-threads=1
```

## 12. Documentation updates

- `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md` — full `--auth`
  correction and cache scope.
- `docs/PPROXY_MIGRATION.md` — auth/reuse migration guidance.
- `docs/cli/PPROXY_CLI_INVENTORY.md` — `--auth` default and meaning.
- `docs/SECURITY_REVIEW.md` — redaction evidence if relevant.
- `docs/parity/` only if a claim/evidence entry changes.

## 13. Acceptance criteria

- Security/migration docs correctly separate URI-fragment credentials from
  `--auth` reuse.
- Compatibility startup warns about the 30-day, IP-only, cross-listener cache.
- Malformed UDP remote errors and `--annotate` output never contain raw
  credentials.
- Compat IR Debug never prints `auth_password`.
- Manifest claim is unchanged or has a recorded evidence-backed update.
- Existing pproxy behavior and differential tests remain green.

## 14. Stop conditions

Stop and report if:

- the warning/redaction requires a compatibility tier change without oracle
  evidence;
- pproxy oracle behavior contradicts the documented 30-day/global cache;
- `--annotate` cannot be redacted without breaking its reproduction purpose
  (define a placeholder contract first);
- a public `Debug`/serialization contract requires raw values (add an explicit
  accessor rather than leaking by default);
- the change would alter credentials in generated TOML (it must not).

## 15. Closure evidence required

- Docs diff showing the corrected `--auth` contract.
- Warning test output with no credentials.
- Redaction test output for errors/annotate/Debug.
- Manifest validator output and either “claim unchanged” or differential
  evidence for the updated claim.
- Explicit no-release statement.

## 16. Handoff notes

- The pproxy oracle’s `AuthTable` is class-level (`_auth`/`_user`); do not
  “fix” it into a per-listener cache in compatibility mode without a tier
  decision. Document it and warn.
- `--auth` is not an authentication switch. Credentials are in the URI fragment.
- Keep generated TOML usable; redact only diagnostics/annotations.
- Coordinate the shared-cache test with Protocols Edge M003’s SOCKS4 cache
  seeding fix.
