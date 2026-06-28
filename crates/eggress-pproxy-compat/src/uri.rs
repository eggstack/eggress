use crate::error::CompatError;

/// Parsed pproxy-style URI.
#[derive(Debug, Clone)]
pub struct PproxyUri {
    /// Protocol scheme (e.g. "socks5", "http", "socks4", "trojan").
    pub scheme: String,
    /// Optional username.
    pub username: Option<String>,
    /// Optional password.
    pub password: Option<String>,
    /// Host (empty string means bind to all interfaces).
    pub host: String,
    /// Port number.
    pub port: u16,
    /// Whether TLS is requested (+tls suffix).
    pub tls: bool,
    /// Optional rule parameter from query string.
    pub rule: Option<String>,
}

impl PproxyUri {
    /// Redacted display — credentials shown as `****:****`.
    pub fn redacted_display(&self) -> String {
        let cred_str = if self.username.is_some() {
            "****:****@"
        } else {
            ""
        };
        let tls_suffix = if self.tls { "+tls" } else { "" };
        let host_display = if self.host.is_empty() {
            String::new()
        } else if self.host.contains(':') {
            // IPv6 — wrap in brackets per RFC 3986
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let rule_str = match &self.rule {
            Some(r) => format!("?rule={}", r),
            None => String::new(),
        };
        format!(
            "{}://{}{}:{}{}{}",
            self.scheme, cred_str, host_display, self.port, tls_suffix, rule_str,
        )
    }
}

/// Parse a single pproxy-style URI into our typed representation.
///
/// Supports:
/// - `scheme://host:port`
/// - `scheme://user:pass@host:port`
/// - `scheme+tls://host:port`
/// - `scheme://host:port?rule=regex`
pub fn parse_pproxy_uri(uri: &str) -> Result<PproxyUri, CompatError> {
    let mut remaining = uri;

    // Split off query string
    let (before_query, query) = if let Some(q_pos) = remaining.find('?') {
        let q = &remaining[q_pos + 1..];
        remaining = &remaining[..q_pos];
        (remaining, Some(q))
    } else {
        (remaining, None)
    };

    // Extract scheme
    let (scheme_part, after_scheme) = if let Some(colon_pos) = before_query.find("://") {
        let scheme = &before_query[..colon_pos];
        let rest = &before_query[colon_pos + 3..];
        (scheme.to_string(), rest)
    } else {
        return Err(CompatError::InvalidUri {
            message: format!("missing scheme in URI: {}", uri),
        });
    };

    // Parse +tls suffix
    let (scheme, tls) = if let Some(tls_pos) = scheme_part.find("+tls") {
        let base = &scheme_part[..tls_pos];
        if &scheme_part[tls_pos..] == "+tls" {
            (base.to_string(), true)
        } else {
            (scheme_part, false)
        }
    } else {
        (scheme_part, false)
    };

    // Validate known schemes
    match scheme.as_str() {
        "http" | "https" | "socks4" | "socks4a" | "socks5" | "trojan" | "ss" | "shadowsocks"
        | "direct" | "ssh" | "unix" | "redir" => {}
        other => {
            return Err(CompatError::UnsupportedProtocol(other.to_string()));
        }
    }

    // Extract credentials
    let (credentials, endpoint_str) = if let Some(at_pos) = after_scheme.find('@') {
        let userinfo = &after_scheme[..at_pos];
        let ep = &after_scheme[at_pos + 1..];
        let (user, pass) = parse_userinfo(userinfo)?;
        (Some((user, pass)), ep)
    } else {
        (None, after_scheme)
    };

    // Parse host:port
    let (host, port) = parse_endpoint(endpoint_str)?;

    // Parse query parameters
    let rule = query.and_then(extract_rule);

    Ok(PproxyUri {
        scheme,
        username: credentials.as_ref().map(|c| c.0.clone()),
        password: credentials.as_ref().map(|c| c.1.clone()),
        host,
        port,
        tls,
        rule,
    })
}

fn parse_userinfo(userinfo: &str) -> Result<(String, String), CompatError> {
    match userinfo.find(':') {
        Some(colon_pos) => {
            let user = userinfo[..colon_pos].to_string();
            let pass = userinfo[colon_pos + 1..].to_string();
            Ok((user, pass))
        }
        None => {
            // No colon: treat as password-only (e.g. Trojan: trojan://password@host:port)
            Ok((String::new(), userinfo.to_string()))
        }
    }
}

fn parse_endpoint(endpoint: &str) -> Result<(String, u16), CompatError> {
    if endpoint.is_empty() {
        return Ok((String::new(), 0));
    }

    // Handle bracketed IPv6: [::1]:8080
    if endpoint.starts_with('[') {
        let close = endpoint.find(']').ok_or_else(|| CompatError::InvalidUri {
            message: "unterminated IPv6 bracket".to_string(),
        })?;
        let host = &endpoint[1..close];
        let after = &endpoint[close + 1..];
        if !after.starts_with(':') {
            return Err(CompatError::InvalidUri {
                message: "expected ':' after IPv6 bracket".to_string(),
            });
        }
        let port = after[1..]
            .parse::<u16>()
            .map_err(|e| CompatError::InvalidUri {
                message: format!("invalid port: {}", e),
            })?;
        return Ok((host.to_string(), port));
    }

    // Regular host:port
    let colon_pos = endpoint.rfind(':').ok_or_else(|| CompatError::InvalidUri {
        message: format!("missing port in endpoint: {}", endpoint),
    })?;
    let host = &endpoint[..colon_pos];
    let port_str = &endpoint[colon_pos + 1..];
    let port = port_str
        .parse::<u16>()
        .map_err(|e| CompatError::InvalidUri {
            message: format!("invalid port '{}': {}", port_str, e),
        })?;

    Ok((host.to_string(), port))
}

fn extract_rule(query: &str) -> Option<String> {
    for param in query.split('&') {
        if let Some(eq_pos) = param.find('=') {
            let key = &param[..eq_pos];
            let value = &param[eq_pos + 1..];
            if key == "rule" && !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_socks5() {
        let uri = parse_pproxy_uri("socks5://127.0.0.1:1080").unwrap();
        assert_eq!(uri.scheme, "socks5");
        assert_eq!(uri.host, "127.0.0.1");
        assert_eq!(uri.port, 1080);
        assert!(uri.username.is_none());
        assert!(!uri.tls);
    }

    #[test]
    fn test_http_with_auth() {
        let uri = parse_pproxy_uri("http://user:pass@proxy:8080").unwrap();
        assert_eq!(uri.scheme, "http");
        assert_eq!(uri.username.as_deref(), Some("user"));
        assert_eq!(uri.password.as_deref(), Some("pass"));
        assert_eq!(uri.host, "proxy");
        assert_eq!(uri.port, 8080);
    }

    #[test]
    fn test_socks4() {
        let uri = parse_pproxy_uri("socks4://0.0.0.0:1080").unwrap();
        assert_eq!(uri.scheme, "socks4");
        assert_eq!(uri.host, "0.0.0.0");
        assert_eq!(uri.port, 1080);
    }

    #[test]
    fn test_tls_suffix() {
        let uri = parse_pproxy_uri("socks5+tls://proxy:1080").unwrap();
        assert!(uri.tls);
        assert_eq!(uri.scheme, "socks5");
    }

    #[test]
    fn test_with_rule() {
        let uri = parse_pproxy_uri("socks5://127.0.0.1:1080?rule=.*\\.com").unwrap();
        assert_eq!(uri.rule.as_deref(), Some(".*\\.com"));
    }

    #[test]
    fn test_trojan() {
        let uri = parse_pproxy_uri("trojan://password@server:443").unwrap();
        assert_eq!(uri.scheme, "trojan");
        assert_eq!(uri.password.as_deref(), Some("password"));
    }

    #[test]
    fn test_empty_host() {
        let uri = parse_pproxy_uri("socks5://:1080").unwrap();
        assert_eq!(uri.host, "");
        assert_eq!(uri.port, 1080);
    }

    #[test]
    fn test_ipv6() {
        let uri = parse_pproxy_uri("socks5://[::1]:1080").unwrap();
        assert_eq!(uri.host, "::1");
        assert_eq!(uri.port, 1080);
    }

    #[test]
    fn test_unsupported_scheme() {
        let err = parse_pproxy_uri("ftp://host:22").unwrap_err();
        match err {
            CompatError::UnsupportedProtocol(p) => assert_eq!(p, "ftp"),
            _ => panic!("expected UnsupportedProtocol"),
        }
    }

    #[test]
    fn test_missing_scheme() {
        let err = parse_pproxy_uri("host:8080").unwrap_err();
        match err {
            CompatError::InvalidUri { .. } => {}
            _ => panic!("expected InvalidUri"),
        }
    }

    #[test]
    fn test_redacted_display() {
        let uri = parse_pproxy_uri("http://user:pass@proxy:8080").unwrap();
        let display = uri.redacted_display();
        assert!(display.contains("****:****@"));
        assert!(!display.contains("pass"));
    }

    #[test]
    fn test_redacted_display_no_creds() {
        let uri = parse_pproxy_uri("socks5://127.0.0.1:1080").unwrap();
        let display = uri.redacted_display();
        assert_eq!(display, "socks5://127.0.0.1:1080");
    }
}
