# Credential Redaction Policy

## Principle

Credentials must never appear in logs, metrics, admin output, error messages,
diagnostics, or any other observable output. All credential-bearing paths use
canonical redaction.

## Canonical Redaction

### URI Display

`PproxyUri::redacted_display()` replaces credentials with `****:****@`:

```
socks5://user:pass@proxy.example:1080 → socks5://****:****@proxy.example:1080
http://proxy.example:8080 → http://proxy.example:8080
```

### Unix Paths

Unix socket paths are redacted to show only the directory:

```
/var/run/eggress/sock.sock → /var/run/eggress/****
```

## Redacted Paths

| Path | Redaction | Location |
|------|-----------|----------|
| URI userinfo | `****:****@` | `PproxyUri::redacted_display()` |
| Unix socket filename | `****` | `PproxyUri::redacted_display()` |
| Config display | Credentials omitted | `RuntimeConfig` display impl |
| Admin snapshots | Metadata only | `/-/status`, `/-/config` |
| Tracing spans | Credentials redacted | All protocol handlers |
| Error messages | Credentials omitted | Config validation |
| Metrics labels | No credential labels | `MetricsRegistry` |
| Python exceptions | Credentials omitted | `EggressException` |
| JSON diagnostics | Credentials redacted | `DiagnosticCode` output |

## Regressions

Redaction is tested by:
- `test_redacted_display` in `eggress-pproxy-compat`
- `test_unix_redacted_display` in `eggress-pproxy-compat`
- Config validation tests never assert on credential values
- Admin snapshot tests verify no credential leakage

## Adding New Credential Paths

When adding new code that handles credentials:

1. Use `PproxyUri::redacted_display()` for URI display
2. Never log raw passwords or URIs
3. Add redaction test for the new path
4. Verify metrics labels don't contain credentials
5. Check admin output doesn't expose secrets
