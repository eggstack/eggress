# pproxy 2.7.9 Strict Parity Completion Roadmap

## Status

Proposed implementation program for the remaining strict-compatibility line.

This roadmap is intentionally separate from the completed practical-parity roadmap. It does not invalidate the practical compatibility claim already earned by Eggress. Instead, it defines the additional work required if the project chooses to move from practical replacement toward literal `pproxy==2.7.9` feature and behavioral parity.

## Frozen reference

All work in this roadmap targets exactly:

- package: `pproxy==2.7.9`
- upstream tag: `2.7.9`
- upstream tag commit: `09d4752f17ed6787e1a073c93980eec019887ee3`

Do not use upstream `master` as the contract. When documentation, historical Eggress plans, and the tagged implementation disagree, the tagged implementation plus a small focused runtime oracle probe wins.

## Why this roadmap exists

The previous practical-parity work intentionally excluded high-cost or legacy behavior. A follow-up audit against the exact 2.7.9 tag found two important things:

1. some items currently described as parity gaps are not actually 2.7.9 requirements and should be removed from the strict target;
2. at least one high-priority wire-compatibility issue remains in the current implementation: Shadowsocks AEAD salt sizing is not method-specific even though upstream 2.7.9 uses 16/24/32-byte salt/IV lengths depending on cipher.

The first phase therefore corrects the target before more code is written.

## Product boundary

Eggress continues to have two surfaces:

1. **native Eggress behavior** — secure, typed, maintainable, and free to improve on historical pproxy behavior;
2. **pproxy compatibility behavior** — intentionally reproduces pproxy 2.7.9 quirks when required for drop-in compatibility.

Compatibility quirks must remain behind compatibility adapters or optional feature gates. Native defaults must not be weakened merely to reproduce historical behavior.

## Explicit false gaps to remove before implementation

The strict target must not require features absent from the 2.7.9 executable/package contract. Phase 0 must verify and remove at least the following from active strict-parity gap accounting unless a direct 2.7.9 oracle proves otherwise:

- `--log` CLI support;
- `-f` / `--config` pproxy CLI support;
- `--rulefile` as a pproxy 2.7.9 CLI flag;
- SOCKS4 BIND;
- SOCKS5 BIND.

Do not implement these solely to improve a parity percentage.

## Scope guardrails

- No new generalized certification framework.
- No broad CI matrix expansion.
- External pproxy/OpenSSH/QUIC/interop evidence is targeted and phase-local.
- Do not refactor stable runtime crates when a narrow adapter suffices.
- Do not implement a full historical ShadowsocksR ecosystem; implement only the exact pproxy 2.7.9 surface.
- Legacy stream ciphers, PF, and daemonization remain optional tail work and must not block the high-value phases.
- SSH and QUIC/H3 are optional-by-default transport crates/features because they add substantial dependency and maintenance cost.
- Security regressions in native mode are not acceptable in the name of compatibility.

## Phase sequence

### Phase 0 — exact oracle and contract reset

Correct the canonical parity matrix/manifest against the exact 2.7.9 tag, remove false gaps, add a compact source inventory, and establish the exact remaining work list.

Detailed plan: `plans/PPROXY_STRICT_PHASE_0_ORACLE_CONTRACT_RESET.md`

### Phase 1 — Shadowsocks AEAD wire correction

Fix method-specific salt lengths, add missing AES-192-GCM parity, verify TCP/UDP framing against real pproxy 2.7.9 and an external standards implementation, and correct any other AEAD framing differences uncovered by those tests.

Detailed plan: `plans/PPROXY_STRICT_PHASE_1_SHADOWSOCKS_AEAD_CORRECTION.md`

### Phase 2 — listener, auth-reuse, and system-proxy closure

Close relatively local high-value gaps: H2 and WS/WSS listener roles already backed by existing transport machinery, pproxy `--auth` per-source-IP authentication reuse, and `--sys` compatibility lifecycle using the existing system-proxy crate.

Detailed plan: `plans/PPROXY_STRICT_PHASE_2_LISTENER_AUTH_SYSTEM_PROXY.md`

### Phase 3 — exact SSR core and bounded plugin family

Implement only pproxy 2.7.9's SSR framing and the six concrete built-in plugin transforms. Keep this code isolated behind a compatibility feature if practical.

Detailed plan: `plans/PPROXY_STRICT_PHASE_3_SSR_AND_PLUGINS.md`

### Phase 4 — Python drop-in completion

Finish the installable `pproxy` namespace and decide, from the exact 2.7.9 symbol inventory, which currently structural helpers must become functional adapters. Add `__main__`, `__doc__`, `cipherpy`, `sysproxy`, and `verbose` compatibility modules where required.

Detailed plan: `plans/PPROXY_STRICT_PHASE_4_PYTHON_DROP_IN_COMPLETION.md`

### Phase 5 — reverse/backward and UDP composition closure

Separate the native secure reverse protocol from a pproxy-compatible backward-wire adapter, then replace one-hop UDP special cases with a bounded composable UDP-hop model for the protocols that pproxy 2.7.9 actually supports over UDP.

Detailed plan: `plans/PPROXY_STRICT_PHASE_5_REVERSE_AND_UDP_COMPOSITION.md`

### Phase 6 — executable/process behavior closure

Match only real 2.7.9 process-surface semantics: `-d`, `-v/-vv`, stdout/stderr placement, startup/failure behavior, signal shutdown, `--test`, `--reuse`, and the remaining actual flags. Do not re-add false-gap flags.

Detailed plan: `plans/PPROXY_STRICT_PHASE_6_CLI_PROCESS_BEHAVIOR.md`

### Phase 7 — SSH transport parity

Add SSH upstream/jump/remote-forward behavior behind an optional feature. Make an explicit MSRV decision before choosing the SSH dependency version.

Detailed plan: `plans/PPROXY_STRICT_PHASE_7_SSH_TRANSPORT.md`

### Phase 8 — QUIC and HTTP/3 parity

Add optional QUIC stream transport and HTTP/3 CONNECT compatibility with exact pproxy stream mapping, not merely generic standards-compliant QUIC.

Detailed plan: `plans/PPROXY_STRICT_PHASE_8_QUIC_HTTP3.md`

### Phase 9 — legacy tail: stream ciphers/OTA, macOS PF, daemonization

Handle the expensive/low-value tail behind explicit compatibility gates. None of these items should be required to merge Phases 0-8.

Detailed plan: `plans/PPROXY_STRICT_PHASE_9_LEGACY_CRYPTO_PF_DAEMON.md`

### Phase 10 — final differential closure and claim reset

Re-run a compact strict matrix, reconcile docs/manifests, and decide whether the final evidence supports an unqualified parity/drop-in claim or still requires listed exclusions.

Detailed plan: `plans/PPROXY_STRICT_PHASE_10_FINAL_DIFFERENTIAL_CLOSURE.md`

## Dependency graph

```text
Phase 0
  -> Phase 1
      -> Phase 2
          -> Phase 3
          -> Phase 4
              -> Phase 5
                  -> Phase 6

Phase 6 -> Phase 7 (optional transport)
Phase 6 -> Phase 8 (optional transport)
Phase 6 -> Phase 9 (legacy/platform/process tail)

Phases 7, 8, and 9 may proceed independently after Phase 6.
All enabled parity targets -> Phase 10.
```

Phase 1 is a hard prerequisite because current protocol claims must not be expanded while an existing modern cipher path may still be self-compatible but externally incompatible.

## Verification policy

Routine implementation verification remains lean:

```bash
cargo fmt --check
cargo test -p <changed-crate>
python -m pytest python/tests/<focused-test-file>
```

Use workspace-wide tests only when shared runtime/configuration behavior changes.

Use direct oracle/interoperability tests only where they establish a contract that local unit tests cannot prove. Examples:

- Eggress client -> pproxy 2.7.9 server;
- pproxy 2.7.9 client -> Eggress server;
- Eggress Shadowsocks -> a standards implementation;
- Eggress SSH -> OpenSSH server;
- Eggress QUIC/H3 <-> pproxy 2.7.9.

Do not make all external implementations mandatory for every normal CI run. A focused local/release verification command or narrow optional job is sufficient.

## Merge policy by value

### Must-do before expanding parity claims

- Phase 0;
- Phase 1.

### Recommended strict-compatibility core

- Phases 2 through 6.

These are bounded enough to justify implementation and materially improve replacement quality.

### Explicit product-decision tail

- Phase 7 SSH;
- Phase 8 QUIC/H3;
- Phase 9 legacy crypto/PF/daemonization.

The roadmap authorizes implementation plans for these items but does not require enabling them in default builds.

## Definition of completion

The strict-parity line is complete only when:

- the canonical matrix contains no requirements invented from later upstream versions or historical planning assumptions;
- supported Shadowsocks AEAD methods interoperate bidirectionally with pproxy 2.7.9 and a standards implementation;
- every claimed 2.7.9 listener/upstream role has runtime evidence;
- `--auth` and `--sys` reproduce their actual pproxy 2.7.9 observable behavior where enabled;
- SSR/plugin support, if claimed, matches the exact 2.7.9 built-in set;
- the installed Python `pproxy` namespace has the agreed strict module/symbol contract and functional behavior for every symbol classified as public/required;
- reverse and UDP compositions accepted by compatibility mode are actually executable, not parser-only;
- real CLI/process semantics are represented accurately;
- optional SSH/QUIC/legacy/platform features are either implemented with evidence or remain explicitly excluded;
- the final compatibility claim matches evidence and contains no aggregate percentage unsupported by a weighting model.

## Public-claim rule

Until Phase 10 explicitly changes the claim, retain the existing qualified wording around practical pproxy 2.7.9 compatibility. Do not publish `100% compatible`, `full parity`, or unqualified `drop-in replacement` language merely because this roadmap exists.
