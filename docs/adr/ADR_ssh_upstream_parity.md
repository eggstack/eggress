# ADR: Optional SSH Upstream Compatibility

| Field | Value |
|-------|-------|
| Status | Supersedes the Phase 47 intentional-non-parity decision |
| Date | Phase 7 |
| Decision makers | Eggress maintainers |
| Related | `docs/architecture/transport-ssh.md`, `plans/PPROXY_STRICT_PHASE_7_SSH_TRANSPORT.md`, `docs/parity/pproxy_capability_manifest.toml` |

## Context

pproxy 2.7.9 supports `ssh://` upstreams through SSH `direct-tcpip` and Unix
streamlocal channels. Phase 47 deliberately rejected this surface because an
embedded SSH client would add substantial protocol and security scope.

Phase 7 revisited that decision against the current repository constraints.
`russh` provides an async pure-Rust client, the project already carries its
cryptographic runtime dependencies, and the compatibility surface can remain
narrowly scoped to byte streams.

## Decision

Implement SSH upstream compatibility behind an opt-in `ssh` feature using a new
`eggress-transport-ssh` crate. The feature provides password and pproxy key-path
authentication, direct TCP and Unix channels, cached sessions, chained SSH
hops, keepalives, and explicit remote TCP forwarding.

SSH remains upstream-only. Listener use is rejected with a structured
diagnostic. Default and `common` builds do not enable or link the transport.
The workspace MSRV is raised to 1.85 for the maintained russh release.

## Compatibility and security boundary

pproxy passes `known_hosts=None`; the compatibility client therefore accepts the
server key and emits a warning for each new session. This behavior is isolated
to the compatibility transport and is not a native secure SSH API. Passwords
are redacted from diagnostics and debug output. Remote command, SFTP, agent
forwarding, and unbounded forwarding are not exposed. Private-key passphrases
remain unsupported because pproxy loads these keys without a passphrase.

## Evidence and consequences

The local OpenSSH integration suite covers public-key authentication, optional
password authentication, direct TCP echo, concurrent cached channels, chained
SSH hops, remote Unix sockets, remote TCP forwarding, redaction, and explicit
reconnect. The transport cache is scoped to one service lifetime and is cleared
after connection draining.

This adds an optional dependency and raises MSRV, but preserves the default
binary feature set and removes the previous pproxy SSH upstream gap. The
permissive host-key behavior remains an explicit compatibility warning and must
not be generalized to native configuration.
