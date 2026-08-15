# pproxy CLI inventory

This file records the executable contract for the frozen `pproxy==2.7.9`
parser. The source evidence is `compat/pproxy-2.7.9/cli-baseline.json`, based
on `pproxy.server.main` at the pinned oracle commit.

## Strict executable surface

| Option | Arity | Repetition/default | Eggress process behavior |
|---|---:|---|---|
| `-l LISTEN` | one | repeatable; default mixed `http+socks4+socks5://:8080` when no listener is supplied | TCP listener URI |
| `-r RSERVER` | one | repeatable; default direct routing | upstream URI, declaration order preserved |
| `-ul ULISTEN` | one | repeatable; default none | UDP listener URI |
| `-ur URSERVER` | one | repeatable; default direct | UDP upstream URI |
| `-b BLOCK` | one | optional | block regex/rule bridge |
| `-a ALIVED` | integer | default `0` | positive values enable native health probes |
| `-s {fa,rr,rc,lc}` | one of four choices | default `fa` | native scheduler mapping |
| `-d` | flag | `count`; `-dd` is two | compatibility task failures are surfaced as error diagnostics |
| `-v` | flag | `count`; `-vv` adds traffic totals | connection events at `-v`, byte statistics at `-vv` |
| `--ssl SSLFILE` | one | optional | listener certificate/key configuration |
| `--pac PAC` | one | optional | admin PAC route |
| `--get GETS` | one | repeatable | admin static content (`PATH,FILE`) |
| `--auth AUTHTIME` | integer | default `2592000` | bounded source-IP auth reuse when listener credentials exist |
| `--sys` | flag | false | apply after bind and restore on shutdown/failure |
| `--reuse` | flag | false | set SO_REUSEPORT before TCP bind where supported |
| `--daemon` | flag | false | parsed, then rejected before startup (exit 5) |
| `--test TEST` | one | optional | test every remote in order and exit before listeners start |
| `--version` | flag | false | print version and exit |
| `-h/--help` | flag | — | print help and exit |

The Rust compatibility executable and `eggress pproxy run` share the same
execution gate. Missing values, invalid integers, invalid scheduler choices,
unknown options, malformed URI values, and unsupported options fail before any
listener or system-proxy side effect. Parser errors use exit code 2;
unsupported features use exit code 5.

`-d` and `-v` follow argparse count semantics, including repeated separate
flags and short clusters such as `-dd`, `-vv`, and `-dv`. Explicit `RUST_LOG`
controls tracing filtering; compatibility verbosity events use the existing
session reports and do not create a parallel counter system.

## Deliberate non-surface options

The pinned parser does not declare `--log`, `-f/--config`, `--rulefile`,
`--listen`, `--remote`, `--udp-listen`, or `--udp-remote`. Positional URIs are
also rejected. Native Eggress config flags and migration-only Python
translation helpers may support similarly named extensions, but the standalone
`pproxy` executable must not advertise or accept them as 2.7.9 options.

## Entry points and verification

```text
pproxy [OPTIONS]
eggress pproxy run -- [OPTIONS]
eggress pproxy check -- [OPTIONS]
eggress pproxy translate -- [OPTIONS]
python -m pproxy [OPTIONS]
```

`python -m pproxy` validates through the native parser and uses the native
upstream-test bridge for `--test`. Normal service startup installs SIGINT and
SIGTERM cleanup through the Rust-backed service handle. None of these entry
points should start a partial service after a parser, translation, bind, or
optional-feature failure.

Focused tests:

```bash
cargo test -p eggress-pproxy-compat --lib
cargo test -p eggress-cli --test pproxy_binary --test pproxy_run_process
cargo test -p eggress-core --lib reuse_port
```

The checked-in oracle baseline is intentionally small and records output
categories rather than nondeterministic full banners. Differential tests that
need the external oracle are gated by `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`.
