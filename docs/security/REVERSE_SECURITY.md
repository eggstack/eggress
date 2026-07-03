# Reverse Proxy Security

## Overview

The reverse proxy allows remote clients to expose local services through a
central server. This creates a high-risk surface because:

1. The server accepts connections from untrusted clients
2. Clients can request proxying to arbitrary destinations
3. The control channel is plaintext by default

## Trust Model

- **Reverse server**: Trusted — operated by the network administrator
- **Reverse client**: Semi-trusted — authenticated but may be compromised
- **External bind targets**: Untrusted — arbitrary network destinations

## Authentication

### Username/Password

- Optional `auth_username` and `auth_password` in config
- Supports `auth_password_env` for environment variable injection
- **No challenge-response** — credentials sent in plaintext
- **No replay protection** — same credentials accepted on reconnect

### Auth Bypass

If both `auth_username` and `auth_password` are `None`, the control channel
accepts any client without authentication.

## Bind Address Controls

### control_bind

The address the server listens for control connections.

- Non-loopback without auth emits config warning
- Should be loopback unless remote clients are explicitly needed

### external_bind

The address clients can request for external listeners.

- Not yet exposed in TOML model (planned)
- `allow_bind` policy in compiled config (currently not enforced)
- `validate_config_safety()` in server crate enforces auth+allowlist for non-loopback

## Resource Limits

| Limit | Default | Description |
|-------|---------|-------------|
| `max_control_connections` | 256 | Max simultaneous control connections |
| `max_streams_per_listener` | 1024 | Max concurrent streams per listener |
| `max_listeners_per_client` | 1 | Max listeners per client |
| `max_pending_external` | 1024 | Max pending external connections |

## Risks

1. **Plaintext auth**: Credentials visible to network observers
2. **Auth replay**: Same credentials accepted on every reconnect
3. **No forward secrecy**: Session compromise reveals all past traffic
4. **Resource exhaustion**: Client churn can consume server resources

## Mitigations

- Use loopback `control_bind` where possible
- Configure strong passwords via `auth_password_env`
- Use TLS transport for remote control channels
- Monitor `/-/reverse` admin endpoint for active connections
- Set conservative resource limits

## Admin Endpoint

The `/-/reverse` admin route shows:
- Active control connections
- Listener states
- Connection counts

No credentials or target addresses are exposed in admin output.
