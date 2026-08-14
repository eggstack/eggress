# pproxy Strict Phase 0 — Exact 2.7.9 Oracle and Contract Reset

## Objective

Make the active Eggress compatibility documents describe the exact `pproxy==2.7.9` target before any further parity code is written.

This is a contract-correction phase, not a feature implementation phase. Its purpose is to eliminate false work, expose real work, and make later acceptance criteria mechanically checkable.

## Frozen upstream inputs

Use only the `2.7.9` tag/commit `09d4752f17ed6787e1a073c93980eec019887ee3` for the oracle inventory.

At minimum inspect:

- `pproxy/server.py`;
- `pproxy/proto.py`;
- `pproxy/cipher.py`;
- `pproxy/cipherpy.py`;
- `pproxy/plugin.py`;
- `pproxy/sysproxy.py`;
- `pproxy/verbose.py`;
- `pproxy/__init__.py`;
- `pproxy/__main__.py`;
- `pproxy/__doc__.py`;
- `setup.py`;
- tagged `README.rst` only as secondary documentation.

Do not import expectations from upstream `master` without proving they also exist in the tag.

## Repository files to inspect/update

Primary active authorities:

- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/README.md` if it defines status vocabulary or source-of-truth rules
- top-level `README.md` compatibility wording

Historical files may remain, but must be clearly marked historical if they contain stale targets:

- `docs/REAL_PPROXY_PARITY_ROADMAP.md`
- `docs/parity/pproxy_2_7_9_strict_manifest.toml`
- `plans/PPROXY_FULL_DROP_IN_ROADMAP.md`
- completed practical/corrective plans

Do not rewrite completed historical records merely to make them look current. Prefer a banner pointing to the active contract.

## Required inventory

Create or refresh one compact machine-readable inventory in the existing parity source of truth. Every record should contain:

- stable capability id;
- upstream surface: CLI / URI / protocol / Python / process / platform;
- exact upstream evidence location;
- Eggress implementation location;
- status: matched / supported_difference / gap / intentional_non_parity / platform_limited;
- whether strict closure is required;
- focused test reference where one exists.

Do not introduce another registry if `pproxy_capability_manifest.toml` can carry the information.

## Correct false gaps

Prove directly from 2.7.9 and then remove from active strict-gap accounting unless the oracle contradicts the source audit:

- `--log` — not a 2.7.9 parser option;
- `-f` / `--config` — not a 2.7.9 parser option;
- `--rulefile` — not a 2.7.9 parser option;
- SOCKS4 BIND — pproxy's SOCKS4 accept path handles CONNECT only;
- SOCKS5 BIND — pproxy's SOCKS5 accept path requires command `0x01`/CONNECT.

If Eggress supports any of these natively, keep them documented as Eggress extensions rather than parity requirements.

## Establish the actual remaining list

At minimum classify:

- Shadowsocks modern AEAD methods and exact salt/IV sizes;
- H2 listener/client roles;
- H3 listener/client roles;
- WebSocket listener/client roles;
- QUIC listener/client/UDP roles;
- SSH client, SSH jump, and remote-forward/listener compositions;
- SSR core framing;
- six built-in plugins: `plain`, `origin`, `http_simple`, `tls1.2_ticket_auth`, `verify_simple`, `verify_deflate`;
- legacy Shadowsocks stream ciphers and OTA;
- `--auth` semantics;
- `--sys` semantics;
- `--daemon` semantics;
- `-d`, `-v`, `-vv` observable behavior;
- reverse/backward `+in` compositions;
- UDP chain compositions;
- Linux redir and macOS PF;
- complete installed Python module list and top-level exports.

## Python inventory probe

Add a small test helper, not a new framework, that can run against both a clean `pproxy==2.7.9` environment and the built Eggress wheel and emit:

- importable module names;
- exported names from `pproxy.__init__`;
- callable signatures for explicitly tracked functions/classes;
- async-vs-sync classification;
- class bases for tracked proxy/protocol classes.

Store only the minimal deterministic expectations needed by tests. Do not snapshot every private object in the package.

## Oracle probes

Use runtime probes only where reading tagged source does not settle observable behavior. Examples:

- argparse error code/stdout/stderr placement;
- `-d` exception propagation;
- `-v/-vv` output behavior;
- `--auth` expiry/reuse;
- shutdown cleanup after `--sys`.

Keep probes local and disposable. Do not make the entire development workflow depend on installing pproxy.

## Non-goals

- No protocol code changes.
- No new compatibility framework.
- No CI expansion beyond a focused optional oracle command if one is already useful.
- No aggregate parity percentage.
- No implementation of features discovered during inventory; those belong to later phases.

## Verification

Run focused documentation/manifest validation plus existing compatibility tests. If a TOML parser/test validates the manifest, update it rather than bypassing it.

Suggested commands:

```bash
cargo test -p eggress-pproxy-compat
python -m pytest python/tests -k 'pproxy or compat'
```

Add a small manifest consistency test only if none exists.

## Acceptance criteria

Phase 0 is complete when all of the following are true:

1. Active compatibility authorities explicitly target `pproxy==2.7.9` tag commit `09d4752f...`.
2. `--log`, `-f/--config`, `--rulefile`, SOCKS4 BIND, and SOCKS5 BIND are not listed as missing strict-parity work unless a recorded 2.7.9 oracle demonstrates otherwise.
3. Every actual 2.7.9 CLI flag is represented exactly once in the active manifest/matrix.
4. Every upstream `pproxy` package module in 2.7.9 is classified against the installed Eggress wheel.
5. Modern AEAD cipher names and method-specific IV/salt sizes are explicitly recorded.
6. The remaining-gap list maps one-to-one onto Phases 1-10 of the strict completion roadmap.
7. Historical plans that disagree with the active target are visibly historical and do not present themselves as the current contract.
8. No runtime behavior is changed in this phase.
