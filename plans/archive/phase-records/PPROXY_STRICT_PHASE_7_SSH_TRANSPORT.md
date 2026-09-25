# pproxy Strict Phase 7 — Optional SSH Transport Parity

## Objective

Implement the exact SSH transport behaviors exposed by `pproxy==2.7.9` while keeping SSH optional and isolated from the default Eggress dependency/binary surface.

This is a product-decision phase: it is required only if the project wants SSH included in the eventual unqualified parity claim.

## Upstream behavior to reproduce

The 2.7.9 target includes:

- `ssh://host[:port]/#login:password` client transport;
- private-key form using the documented double-colon credential convention;
- default SSH port 22;
- disabled known-host verification in pproxy compatibility behavior;
- a cached/reused SSH session;
- keepalive behavior;
- `direct-tcpip` forwarding to the requested destination;
- SSH-through-SSH chains using `__`;
- SSH remote-forward/listener compositions where SSH is followed by a listener/tunnel in the chain;
- Unix target forwarding where the SSH implementation supports it.

Confirm each item against Phase 0/oracle before coding.

## Dependency/MSRV decision gate

Eggress currently declares Rust 1.75. Contemporary `russh` releases observed during planning require a newer compiler. Do not silently raise MSRV as a side effect.

Before implementation:

1. evaluate the current maintained `russh` release and its MSRV/security posture;
2. evaluate whether a still-supported version can satisfy Eggress's current MSRV;
3. record the binary-size and dependency impact with default features minimized;
4. choose one of:
   - deliberately raise workspace MSRV;
   - make SSH feature require a newer MSRV and document that build constraint if Cargo/tooling can represent it cleanly;
   - retain SSH as intentional non-parity.

Do not pin an obsolete SSH library merely to preserve Rust 1.75.

## Preferred architecture

Add a dedicated optional crate, for example:

```text
crates/eggress-transport-ssh/
```

Responsibilities:

- SSH session establishment/authentication;
- session cache/lifecycle;
- direct TCP channels;
- optional Unix channels if library support is mature;
- SSH jump tunneling;
- remote forwarding for the exact pproxy listener composition;
- keepalive/reconnect semantics.

The generic runtime should see an `AsyncRead + AsyncWrite`-like stream/channel and should not contain SSH packet logic.

Feature topology should keep SSH out of default builds, e.g. `pproxy-ssh` or `ssh` feature from CLI/runtime down to the transport crate.

## Authentication

### Password

Parse exact pproxy fragment semantics and authenticate username/password.

### Private key

Support the exact double-colon key-path convention. Determine from oracle whether encrypted keys are accepted and, if so, how passphrases are supplied. Do not invent new URI syntax inside compatibility mode.

### Host-key policy

pproxy disables known-host verification. For compatibility mode, reproduce that behavior only behind an explicit compatibility/insecure policy and emit an actionable warning.

Native Eggress SSH APIs, if exposed, must default to host-key verification and must not inherit this permissive behavior.

## Session lifecycle

Implement:

- one reusable session per configured SSH hop where safe;
- concurrent direct-tcpip channels;
- keepalive interval matching the oracle closely enough to preserve liveness;
- reset/reconnect when the underlying SSH session terminates;
- deterministic cancellation on Eggress shutdown;
- no credential leakage in tracing.

## SSH chains

For `ssh://server1__ssh://server2__ssh://server3`:

- establish server1 from the local host;
- establish server2 through a channel/tunnel from server1;
- establish server3 through server2;
- open final target channels from the last session.

Do not fall back to opening later SSH hops directly from the local host.

## Remote forwarding

Implement the exact pproxy forms only after ordinary client/jump behavior passes.

Required behavior includes the documented model where Eggress establishes SSH to a remote host and asks that host to listen on a configured address/port, then routes accepted remote channels through the remainder of the Eggress compatibility chain.

Bind-address policy must be explicit. Native secure mode may require allowlists even if pproxy is permissive; compatibility mode should be scoped to the exact configured bind and warn when exposing non-loopback addresses.

## Tests

Use a real OpenSSH server fixture/container or local test daemon.

Required:

- password auth success/failure;
- public-key auth success/failure;
- direct TCP channel to local echo target;
- concurrent channels over one session;
- forced session termination/reconnect;
- two-hop SSH chain;
- pproxy client configuration mirrored by Eggress;
- one remote-forward example;
- shutdown cleanup.

Where possible run pproxy 2.7.9 against the same SSH fixture to confirm URI/auth semantics.

## Security checks

- host-key verification disabled only in compatibility mode and clearly warned;
- private key contents never logged;
- password redaction in errors/tracing;
- remote-forward bind exposure documented;
- no shell/command execution capability is required for this feature;
- disable unnecessary SSH algorithms/features in dependency configuration if this does not break pproxy/OpenSSH interop.

## Non-goals

- General SSH client CLI.
- Remote command execution.
- SFTP/SCP.
- Agent forwarding unless Phase 0 proves pproxy 2.7.9 requires it.
- SSH server implementation.

## Acceptance criteria

1. MSRV/dependency decision is explicit before the implementation merge.
2. SSH is non-default and does not increase the normal binary unless enabled.
3. Password and documented key-path authentication work against OpenSSH.
4. `ssh://` upstream can proxy TCP targets.
5. Multi-SSH `__` chains route later hops through earlier SSH sessions.
6. The documented remote-forward/listener form works.
7. Session reuse, keepalive, reconnect, and shutdown are covered by tests.
8. Compatibility-mode insecure host-key behavior is isolated and warned; native behavior remains secure by default.
9. Python and CLI URI paths select the same SSH implementation when the feature is compiled.
