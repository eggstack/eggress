//! Admin `Host` allowlist normalization.
//!
//! The admin HTTP surface validates the `Host` header (and the `Origin` host of
//! state-changing requests) against this allowlist to block DNS-rebinding
//! reads. Entries are stored normalized — lowercase, without IPv6 brackets and
//! without a port — so comparisons are case-insensitive and port-insensitive.

use std::net::IpAddr;

/// Normalize a host entry to a bare lowercase host.
///
/// Accepts the forms a `Host` header, an `Origin` authority, or a configured
/// `admin.allowed_hosts` entry can take: `example.com`, `example.com:9090`,
/// `[::1]:9090`, `::1`, and `[::1]`. Returns `None` for empty input, a
/// malformed bracket/port suffix, or characters that cannot appear in a host
/// (whitespace, `/`, `\`, `@`, `*`, `?`, `#`).
///
/// A literal `null` normalizes to `null`; the admin server accepts
/// `Origin: null` only when the operator lists `null` explicitly.
pub fn normalize_host_entry(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let host = if let Some(rest) = trimmed.strip_prefix('[') {
        let close = rest.find(']')?;
        let after = &rest[close + 1..];
        if !after.is_empty() {
            // Only `:port` may follow the closing bracket.
            let port = after.strip_prefix(':')?;
            port.parse::<u16>().ok()?;
        }
        &rest[..close]
    } else if trimmed.matches(':').count() == 1 {
        let (host, port) = trimmed.rsplit_once(':')?;
        port.parse::<u16>().ok()?;
        host
    } else {
        // No colon, or an unbracketed IPv6 literal.
        trimmed
    };

    let host = host.trim();
    if host.is_empty() || !host.chars().all(is_host_char) {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

fn is_host_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':')
}

/// Effective admin `Host` allowlist: bind-derived defaults plus configured
/// entries (deduplicated, order preserved).
pub fn effective_allowed_hosts(configured: Option<&[String]>, bind: &str) -> Vec<String> {
    let mut hosts = derive_allowed_hosts(bind);
    if let Some(configured) = configured {
        for entry in configured {
            if let Some(host) = normalize_host_entry(entry) {
                push_unique(&mut hosts, host);
            }
        }
    }
    hosts
}

/// Default allowlist implied by an admin bind address.
///
/// * loopback bind (`127.0.0.1`, `[::1]`, …) and unspecified bind (`0.0.0.0`,
///   `::`): the bind host plus `localhost`, `127.0.0.1`, and `::1`, so plain
///   loopback clients keep working without configuration;
/// * any other non-loopback bind: the bind host plus `localhost`. An operator
///   behind a reverse proxy or using a DNS name must list that host explicitly
///   in `[admin] allowed_hosts`.
pub fn derive_allowed_hosts(bind: &str) -> Vec<String> {
    let bind_host = normalize_host_entry(host_part(bind)).unwrap_or_default();
    let loopback_reachable = is_loopback_host(&bind_host) || is_unspecified_host(&bind_host);

    let mut hosts = Vec::new();
    if !bind_host.is_empty() {
        hosts.push(bind_host);
    }
    push_unique(&mut hosts, "localhost".to_string());
    if loopback_reachable {
        push_unique(&mut hosts, "127.0.0.1".to_string());
        push_unique(&mut hosts, "::1".to_string());
    }
    hosts
}

fn push_unique(hosts: &mut Vec<String>, host: String) {
    if !hosts.iter().any(|existing| existing == &host) {
        hosts.push(host);
    }
}

/// Extract the host part of a `host:port` bind string, tolerating bracketed
/// IPv6 (`[::1]:9090`) and unbracketed forms (`::`, `0.0.0.0:9090`).
fn host_part(bind: &str) -> &str {
    let bind = bind.trim();
    if let Some(rest) = bind.strip_prefix('[') {
        return match rest.find(']') {
            Some(close) => &rest[..close],
            None => bind,
        };
    }
    match bind.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && port.parse::<u16>().is_ok() => host,
        _ => bind,
    }
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4.is_loopback(),
        Ok(IpAddr::V6(v6)) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
        Err(_) => false,
    }
}

fn is_unspecified_host(host: &str) -> bool {
    host.parse::<IpAddr>().is_ok_and(|ip| ip.is_unspecified())
}

#[cfg(test)]
mod tests {
    use super::{derive_allowed_hosts, effective_allowed_hosts, normalize_host_entry};

    #[test]
    fn normalizes_host_and_port_forms() {
        assert_eq!(
            normalize_host_entry("Example.COM"),
            Some("example.com".into())
        );
        assert_eq!(
            normalize_host_entry("admin.example.com:9090"),
            Some("admin.example.com".into())
        );
        assert_eq!(
            normalize_host_entry("127.0.0.1:9090"),
            Some("127.0.0.1".into())
        );
        assert_eq!(normalize_host_entry("[::1]:9090"), Some("::1".into()));
        assert_eq!(normalize_host_entry("[::1]"), Some("::1".into()));
        assert_eq!(normalize_host_entry("::1"), Some("::1".into()));
        assert_eq!(
            normalize_host_entry(" ::ffff:127.0.0.1 "),
            Some("::ffff:127.0.0.1".into())
        );
    }

    #[test]
    fn rejects_malformed_and_injectable_entries() {
        assert_eq!(normalize_host_entry(""), None);
        assert_eq!(normalize_host_entry("   "), None);
        assert_eq!(normalize_host_entry("[::1"), None);
        assert_eq!(normalize_host_entry("[::1]junk"), None);
        assert_eq!(normalize_host_entry("host:notaport"), None);
        assert_eq!(normalize_host_entry("*.example.com"), None);
        assert_eq!(normalize_host_entry("host/path"), None);
        assert_eq!(normalize_host_entry("user@host"), None);
        assert_eq!(normalize_host_entry("host name"), None);
    }

    #[test]
    fn loopback_bind_allows_loopback_names() {
        assert_eq!(
            derive_allowed_hosts("127.0.0.1:9090"),
            vec![
                "127.0.0.1".to_string(),
                "localhost".to_string(),
                "::1".to_string()
            ]
        );
        assert_eq!(
            derive_allowed_hosts("[::1]:9090"),
            vec![
                "::1".to_string(),
                "localhost".to_string(),
                "127.0.0.1".to_string()
            ]
        );
    }

    #[test]
    fn wildcard_bind_still_allows_loopback_clients() {
        let hosts = derive_allowed_hosts("0.0.0.0:9090");
        assert!(hosts.contains(&"0.0.0.0".to_string()));
        assert!(hosts.contains(&"127.0.0.1".to_string()));
        assert!(hosts.contains(&"::1".to_string()));
        assert!(hosts.contains(&"localhost".to_string()));
    }

    #[test]
    fn non_loopback_bind_allows_bind_host_and_localhost_only() {
        let hosts = derive_allowed_hosts("10.1.2.3:9090");
        assert_eq!(hosts, vec!["10.1.2.3".to_string(), "localhost".to_string()]);
    }

    #[test]
    fn configured_hosts_extend_bind_defaults_without_duplicates() {
        let configured = vec![
            "Admin.Example".to_string(),
            "127.0.0.1:9090".to_string(),
            "null".to_string(),
        ];
        let hosts = effective_allowed_hosts(Some(&configured), "127.0.0.1:9090");
        assert_eq!(
            hosts,
            vec![
                "127.0.0.1".to_string(),
                "localhost".to_string(),
                "::1".to_string(),
                "admin.example".to_string(),
                "null".to_string(),
            ]
        );
        // An absent or empty list is a no-op: the bind defaults stand alone.
        assert_eq!(
            effective_allowed_hosts(None, "127.0.0.1:9090"),
            derive_allowed_hosts("127.0.0.1:9090")
        );
        assert_eq!(
            effective_allowed_hosts(Some(&[]), "127.0.0.1:9090"),
            derive_allowed_hosts("127.0.0.1:9090")
        );
    }
}
