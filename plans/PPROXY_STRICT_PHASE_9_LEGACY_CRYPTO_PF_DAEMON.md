# pproxy Strict Phase 9 — Legacy Crypto, macOS PF, and Daemonization Tail

## Objective

Handle the remaining low-value/high-cost strict-parity tail without making it part of normal Eggress builds or native defaults.

This phase contains three independent work packages. They may be declined individually; any declined package remains an explicit final non-parity item in Phase 10.

---

## Work package A — legacy Shadowsocks stream ciphers and OTA

## Scope source

Phase 0 must provide the exact `cipher.py` + `cipherpy.py` 2.7.9 inventory. Do not implement names from other Shadowsocks projects unless they appear in that inventory.

Expected families include variants of:

- table;
- RC4 / RC4-MD5;
- ChaCha20 / ChaCha20-IETF / XChaCha variants;
- Salsa20;
- AES CFB/CFB8/CFB1/OFB/CTR;
- Blowfish/CAST5/DES;
- Camellia;
- IDEA;
- SEED;
- RC2;
- any exact `-py` aliases exposed by pproxy.

Modern AEAD methods are Phase 1 and must not be regressed here.

## Architecture

Put insecure/legacy methods behind a non-default feature such as `legacy-crypto` or `pproxy-legacy-crypto`.

Create a stateful stream-cipher abstraction separate from AEAD framing. Requirements:

- method-specific key/IV length;
- EVP_BytesToKey-compatible derivation where pproxy uses it;
- first-write IV emission;
- first-read IV capture with partial-read buffering;
- continuous stream state;
- UDP packet-local IV + cipher state;
- exact pproxy method-name aliases.

Prefer maintained RustCrypto primitives. Do not import OpenSSL merely to reproduce old algorithms. For algorithms without a maintained safe Rust implementation, explicitly evaluate whether a small compatibility implementation is acceptable; do not use direct unsafe code because the workspace denies it.

## OTA

Implement pproxy 2.7.9 `!` OTA behavior exactly:

- destination-header OTA flag/address tag;
- header HMAC-SHA1 truncation;
- per-chunk two-byte length;
- per-chunk HMAC keyed by IV plus chunk counter;
- monotonic chunk sequence;
- incremental reader buffering;
- malformed HMAC -> hard failure.

OTA remains disabled unless the user requests the method syntax that enables it.

## Security policy

Every use of an unauthenticated legacy method must produce one concise compatibility warning. Documentation must label these methods insecure/deprecated. Native examples must continue to prefer AEAD.

## Tests

- known-answer cipher vectors against pproxy for every implemented method;
- fragmented TCP read/write;
- UDP packet roundtrip against pproxy;
- OTA header/chunk vectors;
- wrong HMAC rejection;
- feature-off rejection path;
- release binary size with feature disabled/enabled.

---

## Work package B — macOS `pf://` original-destination recovery

## Exact target

pproxy's `Pf` protocol opens `/dev/pf` and performs the PF NAT lookup ioctl to recover the pre-redirection destination for accepted connections.

Eggress already has Linux transparent/redir behavior; do not rework it here.

## Constraints

The workspace denies direct unsafe Rust. Implement PF lookup only through a safe maintained crate/wrapper if available and sufficient. If every viable implementation requires project-local unsafe FFI, treat that as an explicit architecture/security decision rather than weakening the workspace lint casually.

## Implementation requirements

- macOS-only compilation;
- open PF device with clear permission errors;
- construct source/destination query from accepted socket peer/local addresses;
- recover original IPv4 destination and IPv6 if pproxy 2.7.9 supports it reliably;
- no global PF rule mutation by Eggress itself;
- startup/runtime diagnostic explains that the user must configure PF redirect rules and privileges;
- file descriptor lifecycle bounded to the listener/runtime.

## Tests

Unit-test structure/translation where possible. Real acceptance requires a disposable macOS PF environment with root privileges:

- configure loopback redirect to Eggress;
- connect to an original local target;
- verify recovered destination and relay;
- remove PF rules after test.

Do not make this privileged test part of routine CI if the environment is unavailable.

---

## Work package C — Linux `--daemon`

## Exact target

pproxy 2.7.9 conditionally uses the `python-daemon` package when `--daemon` is requested. Reproduce the observable process behavior, not the implementation library.

Before implementation, oracle:

- parent exit behavior/status;
- working directory;
- stdio handling;
- signal behavior;
- file descriptor inheritance relevant to listeners/logging;
- whether pid files or umask changes are observable in the default `DaemonContext` use.

## Implementation approach

Prefer a small maintained daemonization crate behind a non-default compatibility feature. Do not hand-roll fork/session code with project-local unsafe.

Order is critical:

1. parse/validate configuration;
2. perform daemon transition at the oracle-equivalent point;
3. initialize runtime/listeners according to captured behavior;
4. ensure `--sys` rollback ownership remains with the daemon process;
5. shutdown cleanly on supported signals.

If exact daemon semantics conflict with reliable modern process management, keep daemonization compatibility-only and continue recommending systemd/launchd/etc. for native Eggress use.

## Non-goals for entire phase

- Enabling any legacy item by default.
- Adding new native configuration examples encouraging obsolete ciphers.
- Implementing a PF firewall manager.
- Replacing OS service managers with Eggress daemon management.
- Lowering the workspace unsafe-code policy without explicit approval.

## Acceptance criteria

### Legacy crypto/OTA

1. Every implemented method comes from the exact 2.7.9 inventory.
2. Feature-off builds contain none of the optional legacy algorithm stack where Cargo allows elimination.
3. Implemented methods pass pproxy known-answer/interop tests for TCP and applicable UDP.
4. OTA passes pproxy header/chunk interop and rejects bad HMACs.
5. Insecure methods emit a compatibility warning and are never selected implicitly.

### PF

6. `pf://` compiles only on macOS and fails clearly without required privilege/device access.
7. A privileged local test demonstrates original-destination recovery and relay, or PF remains explicitly unimplemented if the safe-code constraint blocks it.

### Daemon

8. `--daemon` is either functional behind an optional compatibility feature with oracle-aligned parent/child behavior, or remains a final explicit non-parity item.
9. Daemon mode preserves clean signal shutdown and `--sys` rollback.
10. None of these work packages increase default runtime scope without an explicit feature enable.

## Phase 9 outcome

- Work package A is implemented behind `legacy-crypto`. The maintained
  RustCrypto subset has pproxy 2.7.9 known-answer vectors, fragmented stream
  coverage, OTA HMAC rejection, and packet-local UDP coverage. `cast5-cfb`,
  `idea-cfb`, `rc2-cfb`, and `seed-cfb` remain explicit refusals.
- Work package B remains intentionally unimplemented. Phase 9 found no
  maintained safe `/dev/pf` ioctl wrapper; the existing macOS capability refusal
  and ADR remain the honest boundary.
- Work package C is implemented behind `pproxy-daemon` on Linux using safe
  re-exec after validation and `--test`. Feature-off, non-Linux, and launch
  failures remain fail-closed; the child owns signals and `--sys` rollback.
