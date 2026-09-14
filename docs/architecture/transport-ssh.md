# SSH Transport

`eggress-transport-ssh` is an optional upstream transport enabled by the `ssh`
feature. It is intentionally separate from the native protocol crates so
default and `common` binaries do not link SSH support. The embed facade uses
the same transport for native TOML and pproxy-compatible outbound chains.

## Scope

The transport implements the stream operations used by pproxy 2.7.9's
`ProxySSH`:

- password authentication (`user:password`), or a private key selected by the
  pproxy convention `user::/path/to/key`;
- direct TCP channels and remote Unix-socket channels;
- reusable authenticated sessions keyed by host, port, username, credential,
  and hop index;
- chained SSH hops, where the next SSH connection consumes the prior hop's
  direct-tcpip stream;
- explicit SSH TCP remote forwarding via `SshRemoteForward`.

The transport does not expose remote commands, SFTP, agent forwarding, or an
SSH server. Private-key passphrases are not supported because pproxy loads keys
without a passphrase.

## Security boundary

Native callers use the verified `SshSessionCache::new()` policy, which checks
the default OpenSSH known_hosts file. The explicit pproxy compatibility path
uses `new_compatibility()` because pproxy 2.7.9 passes `known_hosts=None`; it
accepts the server key and emits a warning when a new session is created. This
compatibility exception must not weaken native TOML SSH merely because both
features are enabled.
Passwords are redacted from debug/error output. Private-key paths are treated as
operator configuration and are never included in authentication errors.

Keep remote forwarding explicit: callers must request the bind address and port
and retain the returned forward while accepting channels. Do not turn SSH into
an implicit listener or unrestricted port-forwarding service.

## Lifecycle and testing

The runtime creates one cache per service lifetime, shares it across connection
tasks, and clears it after connection draining during shutdown. A failed or
closed session is removed before reconnecting; `keepalive_interval` is 60
seconds with three missed keepalives.

The OpenSSH integration suite covers public-key and optional password auth,
direct TCP echo, concurrent channels, two-hop chaining, Unix sockets,
redaction/reconnect behavior, and remote TCP forwarding:

```bash
cargo test -p eggress-transport-ssh --test openssh -- --nocapture
```
