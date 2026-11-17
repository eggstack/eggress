# Security Hardening Roadmap

Status: closed (2026-11-16) — corrective security program implemented and verified

Long-term references:

- `plans/000-long-term-specification.md` §4 (invariants 2, 3, 7, 8, 9, 10), §6 (end-state acceptance)
- `plans/001-terminology-and-domain-model.md` §2–§4 (listener roles, route/upstream identity, tier vs status)
- `plans/002-long-term-roadmap.md` (dependency-ordered work; no security phase was previously opened)
- `plans/003-planning-process.md` §7 (corrective passes) and §9–§10 (registry/review)

Related ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

## 1. Purpose and ownership boundary

This roadmap coordinates the corrective security work identified by the
2026-11-16 repository audit. It is a **program**, not a new runtime subsystem:
it owns sequencing, evidence requirements, and cross-subsystem consistency, but
each milestone delegates production changes to the owning subsystem roadmap and
implementation plan.

It MUST NOT:

- add new proxy capabilities or change compatibility tiers as a side effect;
- take ownership of protocol framing, routing, admin, transports, or delivery code;
- authorize a version bump, tag, or publication;
- silently relax a canonical invariant to make a finding disappear.

The owning boundaries remain:

- TLS/QUIC/SSH mechanics → **transports**;
- listener auth, SOCKS4, UDP target handling, protocol bounds → **protocols-edge**;
- routing matchers and admin HTTP → **routing-health-observability**;
- CLI/updater/release/CI and operator output → **delivery**;
- pproxy translation, manifest/tier claims, and compatibility diagnostics → **parity-compat**.

Architecture references: `architecture/overview.md`,
`architecture/transports-tls.md`, `architecture/transports-ssh-quic-h3.md`,
`architecture/protocols-socks.md`, `architecture/udp.md`,
`architecture/routing.md`, `architecture/admin.md`, `architecture/cli.md`.

## 2. Work classification

### Invariants

- Outbound TLS verifies the peer against a current, trusted root set; insecure
  verification is never the default and is visible when selected.
- Credentials, secret-bearing URIs, TLS private keys, and auth material are
  absent from logs, diagnostics, errors, JSON, and Debug output.
- Destination policy fails closed: a policy that says “no private egress” is
  enforced after name resolution on every locally resolving path, not only for
  literal targets.
- CIDR matching is canonical and family-correct; its contract is explicit about
  literal IPs versus names.
- Compatibility authentication follows the pinned pproxy 2.7.9 contract or is
  explicitly documented as a supported difference with evidence.
- Dependency advisories are visible in CI and release preflight; unreachable
  advisory paths require a narrow, justified, reviewable exception.

### Capabilities

No new user-facing capability is introduced. The program preserves existing
SOCKS/HTTP/TLS/UDP/admin/updater behavior while closing bypasses and exposure.

### Infrastructure

- A shared destination-policy/address-classification helper where duplication
  currently allows UDP and TCP to disagree.
- An advisory gate in CI/release preflight.
- Redacting Debug/serialization for compiled security-sensitive structures.
- In-process updater hashing and private staging mechanics.

### Polish

- Security documentation truth: root-store description, unsafe-code statement,
  handshake-timeout statement, `--auth` semantics, CIDR limitations, admin
  transport, insecure TLS reachability.
- Structured, redacted diagnostics for the corrected paths.

## 3. Non-goals

- New proxy protocols, transports, listeners, or routing actions.
- Changing pproxy compatibility tiers without a manifest update plus
  oracle/differential/interop evidence.
- Replacing rustls, ring, or the boxed-stream architecture.
- Implementing admin TLS, unified TCP/UDP egress policy, or DNS-aware routing
  matchers in this program; these are recorded as residual/deferred decisions.
- Publishing a release or pushing a tag.

## 4. Current state

The 2026-11-16 audit found the following corrective items. Severity is the
audit severity; the plans may refine exploitability conditions.

| ID | Severity | Finding | Owning plan |
|---|---|---|---|
| H1 | High | Bundled TLS roots include a distrusted emSign root and the webpki verifier accepts known-compromised intermediates (CVE-2026-111943) | `plans/implementation/transports/004-tls-trust-and-dependency-advisory-remediation.md` |
| H2 | High | SOCKS4 auth checks only the username; password is not enforced and success can seed the IP reuse cache | `plans/implementation/protocols-edge/003-socks4-auth-and-udp-private-egress.md` |
| M3 | Medium | `allow_private_egress = false` is bypassable for UDP domain targets; the promised post-DNS check is absent | same protocols-edge plan |
| M4 | Medium | CIDR route rules are bypassable with domain names and IPv4-mapped IPv6 literals; docs present them as a private-network ACL | `plans/implementation/routing-health-observability/003-cidr-canonicalization-and-admin-exposure.md` |
| M5 | Medium | pproxy-compatible auth reuse is always on with a 30-day default and a global IP-keyed cache; security docs misdescribe `--auth` | `plans/implementation/parity-compat/003-auth-reuse-and-compat-redaction.md` |
| M6 | Medium | Updater trusts `EGRESS_UPDATE_BASE_URL`/`file://`, PATH-resolved `curl`/`sha256sum`, and a predictable staging directory | `plans/implementation/delivery/004-updater-trust-and-advisory-gate.md` |
| M7 | Medium | 21 RustSec advisories in the lockfile; no CI/release advisory gate | delivery 004 plus transports 004/005 |
| M8 | Medium | Password auth can run over plaintext on non-loopback listeners; admin is HTTP-only and rejects `https://` | protocols-edge 003 (listener warning) and routing 003 (admin) |
| L9 | Low | Credential-redaction gaps in compat errors, `translate --annotate`, `system-proxy inspect --json`, and latent Debug | parity 003 and delivery 004 |
| L10 | Low | Private/reserved-IP classification misses CGNAT and IPv4-embedding IPv6 forms | protocols-edge 003 |
| L11 | Low | Insecure TLS is compiled into the default CLI and reachable via `?insecure`, contrary to “API-only” docs | transports 004 |
| L12 | Low | Admin HTTP lacks Host/Origin validation; DNS rebinding can read unauthenticated topology | routing 003 |

The audit did not find a pre-auth memory-safety RCE. The highest-impact items
are H1 and H2; the rest are policy/enforcement, supply-chain, or exposure
correctives.

## 5. Target architecture

The program restores the already-intended boundaries rather than adding a new
security layer:

1. **Verified transport default.** `eggress-transport-tls` builds outbound TLS
   from a current trust store and rejects known-distrusted paths; insecure
   verification is explicit, warned, and documented.
2. **One inbound-auth contract per protocol.** SOCKS5/HTTP/H2/WebSocket/reverse
   keep their constant-time credential checks; SOCKS4 is aligned with the
   pinned pproxy full-credential user-ID semantics instead of username-only
   acceptance.
3. **Resolved-target policy.** Locally resolving direct paths apply the same
   private/reserved-IP policy after resolution. Upstream chains remain
   explicitly trust-dependent because the upstream resolves names.
4. **Canonical routing inputs.** IPv4-mapped IPv6 is normalized before CIDR
   matching; CIDR rules remain literal-IP-only and are documented as such.
   Private-network protection for names comes from the egress guard, not CIDR.
5. **Explicit operator exposure.** Non-loopback auth without TLS and
   non-loopback admin over plaintext HTTP are warned about; admin Host
   validation blocks DNS-rebinding reads.
6. **Redacted by construction.** Secret-bearing Debug/serialization and
   compatibility diagnostics are redacted at the type or formatter boundary,
   not at individual call sites.
7. **Supply-chain visibility.** CI and release preflight run an advisory gate;
   optional-feature advisories are either fixed or explicitly accepted with a
   tracked upstream dependency.

## 6. Dependency graph

```text
S1 TLS trust/advisory remediation (transports 004)
    |
S2 russh migration (transports 005)          S5 advisory gate (delivery 004)
    |                                             ^
    +---------------------------------------------+
                                                  ^
S6 compat auth/redaction (parity 003) -----------+

S3 inbound auth/UDP policy (protocols-edge 003)   independent

S4 CIDR/admin exposure (routing 003)              independent
```

- **S1 → S5** is a soft dependency: the gate can land with a documented `zip`
  exception, but a clean gate needs S1 and S2.
- **S2 → S5** is soft/operational because SSH is an optional feature.
- **S6 → S5** is soft: the gate should not permanently ignore redaction tests.
- **S3/S4** are independent of the dependency upgrades and can proceed in parallel.

## 7. Milestones

### Milestone S1 — TLS trust and default-build advisory remediation

Class: invariant.

Plan: `plans/implementation/transports/004-tls-trust-and-dependency-advisory-remediation.md`

Objective: remove the distrusted-root and webpki verification exposure from the
default outbound TLS graph; upgrade resolvable QUIC/tokio advisories; document
and warn on the insecure TLS surface.

Dependencies: none hard. `rustls 0.23.46+`, `rustls-webpki 0.103.16+`,
`webpki-roots 1.0.10+` are available; `cargo update` dry-run resolves
`rustls 0.23.46`, `rustls-webpki 0.103.17`, `webpki-roots 1.0.12`,
`quinn 0.11.12`, `quinn-proto 0.11.19`, `quinn-udp 0.5.16`,
`rustls-platform-verifier 0.7.1`, `webpki-root-certs 1.0.12`, and
`tokio 1.54.1`.

Exit: default TLS verification uses the fixed trust store; the listed default
graph advisories are absent; insecure TLS is documented and warned; transport
tests and broad Rust gates are green.

### Milestone S2 — SSH dependency advisory migration

Class: invariant (optional feature).

Plan: `plans/implementation/transports/005-russh-065-advisory-migration.md`

Objective: migrate `eggress-transport-ssh` from `russh 0.62.6` to a fixed
`0.65.x` line, preserving known-hosts verification, compatibility host-key
scoping, password/private-key redaction, and the restricted forwarding surface.

Dependencies: none hard; SSH remains optional. `cargo update` only offers
`russh 0.62.7`, which does not carry the `>=0.63.1`/`>=0.65.0` fixes, so this is
an explicit breaking API migration.

Exit: `ssh` and `ssh,pproxy-compat` feature slices compile; OpenSSH regression
is required and green where available; no SSH graph advisory remains; no
host-key/auth regression.

### Milestone S3 — Inbound authentication and UDP destination policy

Class: invariant.

Plan: `plans/implementation/protocols-edge/003-socks4-auth-and-udp-private-egress.md`

Objective: enforce the configured SOCKS4 credential as pproxy does; apply
`allow_private_egress` after DNS on all locally resolving UDP paths; extend
special-purpose address classification; warn when non-loopback password auth
runs without TLS.

Dependencies: none hard. The SOCKS4 behavior is defined by the pinned pproxy
2.7.9 oracle (`userid in users`, where `users` holds full `user:pass` strings).

Exit: SOCKS4 username-only and wrong-password connections fail; UDP domains
resolving private are dropped when the policy disallows private egress;
classification tests cover CGNAT/NAT64/6to4/Teredo; listener warning is emitted.

### Milestone S4 — CIDR canonicalization and admin exposure

Class: invariant.

Plan: `plans/implementation/routing-health-observability/003-cidr-canonicalization-and-admin-exposure.md`

Objective: make CIDR matching family-correct for IPv4-mapped IPv6, make the
literal-only CIDR contract explicit, and block admin DNS-rebinding reads with
Host/Origin validation and a non-loopback plaintext warning.

Dependencies: none hard. The decision to keep routing CIDR matching literal-only
preserves the existing synchronous route contract; domain private-egress
protection remains the responsibility of the resolved-target guard.

Exit: `::ffff:10.0.0.1` matches `10.0.0.0/8` in destination and source matchers;
config/docs warn that CIDR does not resolve names; admin rejects unexpected
`Host`/`Origin`; non-loopback admin without TLS is warned; admin tests green.

### Milestone S5 — Updater trust, advisory gate, and credential-output hardening

Class: invariant / infrastructure.

Plan: `plans/implementation/delivery/004-updater-trust-and-advisory-gate.md`

Objective: remove production updater test hooks, make checksum verification
independent of PATH, use private staging, redact `system-proxy inspect --json`,
and add an advisory gate to CI and release preflight.

Dependencies: S1/S2/S6 soft for a clean gate. The `zip 6.0.0` advisory remains
blocked on an upstream `eggup-archive` release with `zip >= 9.0.0`; the plan
must either carry a narrow documented exception or defer the clean-gate claim.

Exit: release builds reject `EGRESS_UPDATE_BASE_URL`/`file://`; no PATH-resolved
hash tool; staging is private and non-symlink-following; JSON has no
credentials; the gate runs and any temporary `zip` exception is explicit and
tracked.

### Milestone S6 — Compatibility auth reuse and redaction

Class: invariant / polish.

Plan: `plans/implementation/parity-compat/003-auth-reuse-and-compat-redaction.md`

Objective: make pproxy `--auth` semantics and the global source-IP cache
explicitly documented and warned; redact raw compatibility URIs in errors and
`translate --annotate`; redact compat Debug structures.

Dependencies: none hard. Any behavior change (rather than documentation/warning)
must update the manifest and run the oracle/differential suite.

Exit: security-differences docs correctly describe credentials-from-fragment and
`--auth` as the reuse interval; startup warns about the 30-day global IP cache;
raw UDP remote URIs and annotated argv are redacted; compat Debug output hides
`auth_password`.

## 8. Cross-cutting requirements

### Config and migration

- Preserve pproxy-compatible defaults unless a finding requires fail-closed
  behavior. New config fields must be optional with secure defaults.
- No listener-topology hot-reload semantics change.
- Admin Host validation must allow configured reverse-proxy hostnames, not only
  loopback names.

### Protocol and compatibility

- H2 and SOCKS4 behavior changes are compatibility-sensitive. If a manifest
  claim changes, update `docs/parity/pproxy_capability_manifest.toml` and run the
  oracle/differential/interop suite; generated reports follow the manifest.
- The SOCKS4 full-credential comparison is intended to match the pinned pproxy
  behavior, not to introduce a new tier.

### Security and authorization

- No credential, private key, or secret may appear in errors, logs, metrics
  labels, admin JSON, Debug, or `--annotate`.
- Destination policy failures are fail-closed and observable (metrics/reason),
  never a silent fallback.
- Insecure TLS remains opt-in; any warning must not echo credentials.

### Concurrency, cancellation, and recovery

- UDP resolution and validation must occur before flow creation and must respect
  existing idle/cancellation bounds.
- Admin Host validation must not introduce blocking work in the request path.
- Updater staging must remain safe under concurrent update attempts; use
  create-new/private directory semantics.

### Observability and audit

- Auth failures remain counted; SOCKS4 failures should join the same metric.
- Advisory-gate exceptions must be visible in `deny.toml` comments and the
  registry blocked-work section.
- Security docs are part of closure evidence, not optional polish.

### Performance and resource use

- No new DNS resolution on hot paths where a literal target already exists.
- UDP domain validation must reuse the existing lookup result; do not resolve
  twice.
- Admin Host checks are constant-time enough for a local operator surface; no
  unbounded header parsing is added.

### Documentation and operations

- Update the security docs listed in each plan with the code.
- Do not claim a control is enforced when only warned.
- Do not claim “system roots” when bundled roots are used.

## 9. Verification strategy

Each plan owns its focused tests. Program-level closure additionally requires:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo audit --ignore RUSTSEC-2023-0071
cargo deny check
```

Optional-feature slices:

```bash
cargo check -p eggress-cli --locked --no-default-features \
  --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins
cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat
EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked \
  --no-default-features --features ssh,pproxy-compat --test ssh -- --nocapture
```

For SOCKS4/auth changes, run the oracle/differential suite only if the manifest
claim changes; if the change restores the existing `matched` claim, add focused
regression evidence and record it in closure.

## 10. Risks and decision points

- **`zip` upstream block.** `eggup-archive 0.1.2` and `0.2.0` both pin
  `zip = "6.0.0"`; no registry version currently resolves the zip advisories.
  The vulnerable `extract()` APIs are not used by Eggup’s bounded extractor, so
  the interim risk is accepted only with an explicit gate exception and tracked
  upstream dependency.
- **`russh` breaking migration.** `0.65.2` changes `Handler::check_server_key`
  to an async/future signature and `PublicKeyOrCertificate`; the SSH feature is
  optional, so this can land after the default-graph TLS fix.
- **CIDR domain limitation.** Routing remains literal-only; private-egress
  protection for names is enforced by the resolved-target guard, not CIDR.
  Upstream chains remain trust-dependent and must be documented.
- **Admin TLS.** This program adds Host/Origin validation and warnings, not
  native admin TLS. Remote admin remains a loopback/VPN/TLS-terminator
  responsibility.
- **TCP literal private egress.** This program does not change the documented
  pproxy-compatible default that allows explicit private IP targets. Any future
  unified `allow_private_egress` for TCP is a separate decision.

## 11. Completion definition

The program is complete when:

- every milestone S1–S6 is closed with a closure record and evidence;
- the default and optional feature graphs are advisory-clean, or every remaining
  advisory has a narrow, reviewed, expiring exception with a tracked upstream
  dependency;
- the security docs agree with code and tests;
- `plans/registry.md` and `docs/ROADMAP.md` agree that no security corrective
  milestone remains active;
- no high/medium audit finding remains unclosed or explicitly accepted with a
  recorded owner and date.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| S1 TLS trust/default advisories | closed | `plans/implementation/transports/004-tls-trust-and-dependency-advisory-remediation.md` | `plans/closure/transports/004-status.md` | — |
| S2 SSH `russh` migration | closed | `plans/implementation/transports/005-russh-065-advisory-migration.md` | `plans/closure/transports/005-status.md` | RUSTSEC-2026-0543 (`chrono` unmaintained via `russh-util`) is a tracked, commented `deny.toml` exception with owner and review date; optional `ssh` only |
| S3 SOCKS4/UDP/classification | closed | `plans/implementation/protocols-edge/003-socks4-auth-and-udp-private-egress.md` | `plans/closure/protocols-edge/003-status.md` | — |
| S4 CIDR/admin exposure | closed | `plans/implementation/routing-health-observability/003-cidr-canonicalization-and-admin-exposure.md` | `plans/closure/routing-health-observability/003-status.md` | — |
| S5 Updater/advisory gate/output | closed | `plans/implementation/delivery/004-updater-trust-and-advisory-gate.md` | `plans/closure/delivery/004-status.md` | Seven `zip 6.0.0` advisories remain as a tracked, commented `deny.toml` exception with an owner and review date pending upstream `eggup-archive` with `zip >= 9.0.0` |
| S6 Compat auth/reuse/redaction | closed | `plans/implementation/parity-compat/003-auth-reuse-and-compat-redaction.md` | `plans/closure/parity-compat/003-status.md` | — |

Program disposition (2026-11-16): all six milestones are implemented and their
closure records are `closed`. The coordinator broad gate passed — workspace
`fmt`, `clippy -D warnings`, `test --workspace --locked`, the documented CLI and
embed feature slices, the embed OpenSSH regression, and
`cargo deny --workspace check` (advisories/bans/licenses/sources). The
remaining audit items are the seven `zip 6.0.0` advisories in the updater graph
and the unmaintained `chrono` transitive through `russh-util` on the optional
`ssh` path, both tracked as narrow, commented `deny.toml` exceptions with owners
and a 2027-05-01 review date pending an upstream `eggup-archive` release with
`zip >= 9.0.0` and a `russh`/`russh-util` update. No commit, tag, or publication
was authorized or created by this program.
