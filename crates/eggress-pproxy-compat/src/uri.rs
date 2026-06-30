use crate::error::CompatError;

/// Parsed pproxy-style URI.
#[derive(Debug, Clone)]
pub struct PproxyUri {
    /// Protocol scheme (e.g. "socks5", "http", "socks4", "trojan", "bind", "listen", "backward").
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
    /// Whether this is a reverse/inbound URI (+in suffix).
    pub inbound: bool,
    /// Optional rule parameter from query string.
    pub rule: Option<String>,
    /// Optional path (used for unix:// scheme).
    pub path: Option<String>,
}

impl PproxyUri {
    /// Returns true if this is a reverse proxy URI (bind/listen/backward/rebind).
    pub fn is_reverse(&self) -> bool {
        matches!(
            self.scheme.as_str(),
            "bind" | "listen" | "backward" | "rebind"
        )
    }

    /// Redacted display — credentials shown as `****:****`, Unix paths shown as `unix://****`.
    pub fn redacted_display(&self) -> String {
        if self.scheme == "unix" {
            if let Some(ref p) = self.path {
                let redacted_path = redact_unix_path(p);
                return format!("unix://{}", redacted_path);
            }
            return "unix://****".to_string();
        }

        let cred_str = if self.username.is_some() {
            "****:****@"
        } else {
            ""
        };
        let rule_str = match &self.rule {
            Some(r) => format!("?rule={}", r),
            None => String::new(),
        };
        format!(
            "{}://{}{}{}",
            self.scheme_with_tls(),
            cred_str,
            self.endpoint_display(),
            rule_str,
        )
    }

    pub(crate) fn scheme_with_tls(&self) -> String {
        let mut s = self.scheme.clone();
        if self.tls {
            s.push_str("+tls");
        }
        if self.inbound {
            s.push_str("+in");
        }
        s
    }

    pub(crate) fn endpoint_display(&self) -> String {
        format!("{}:{}", format_host_for_uri(&self.host), self.port)
    }

    pub(crate) fn bind_display(&self) -> String {
        if self.host.is_empty() {
            format!("0.0.0.0:{}", self.port)
        } else {
            self.endpoint_display()
        }
    }
}

/// Redact a Unix socket path for display, preserving only the filename.
fn redact_unix_path(path: &str) -> String {
    match path.rfind('/') {
        Some(pos) => {
            let dir = &path[..=pos];
            format!("{}****", dir)
        }
        None => "****".to_string(),
    }
}

fn format_host_for_uri(host: &str) -> String {
    if host.is_empty() {
        String::new()
    } else if host.contains(':') {
        format!("[{}]", host)
    } else {
        host.to_string()
    }
}

/// Parse a single pproxy-style URI into our typed representation.
///
/// Supports:
/// - `scheme://host:port`
/// - `scheme://user:pass@host:port`
/// - `scheme+tls://host:port`
/// - `scheme://host:port?rule=regex`
/// - `unix:///path/to/socket`
/// - `redir://:12345`
/// - `redir://127.0.0.1:12345`
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

    // Parse +tls suffix and +in modifier
    let mut tls = false;
    let mut inbound = false;
    let mut scheme = scheme_part;
    loop {
        if let Some(tls_pos) = scheme.find("+tls") {
            if &scheme[tls_pos..] == "+tls" {
                tls = true;
                scheme = scheme[..tls_pos].to_string();
                continue;
            }
        }
        if let Some(in_pos) = scheme.find("+in") {
            if &scheme[in_pos..] == "+in" {
                inbound = true;
                scheme = scheme[..in_pos].to_string();
                continue;
            }
        }
        break;
    }

    // Validate known schemes
    match scheme.as_str() {
        "http" | "https" | "socks4" | "socks4a" | "socks5" | "trojan" | "ss" | "shadowsocks"
        | "ssr" | "direct" | "ssh" | "unix" | "redir" | "h2" | "ws" | "wss" | "raw" | "tunnel"
        | "bind" | "listen" | "backward" | "rebind" => {}
        other => {
            return Err(CompatError::UnsupportedProtocol(other.to_string()));
        }
    }

    // Handle unix:// scheme — path-based, not host:port
    if scheme == "unix" {
        let path = if after_scheme.starts_with('/') {
            after_scheme.to_string()
        } else if after_scheme.is_empty() {
            return Err(CompatError::InvalidUri {
                message: "unix:// URI requires a path (e.g. unix:///tmp/socket)".to_string(),
            });
        } else {
            // Treat bare content as a relative path
            format!("/{}", after_scheme)
        };
        let rule = query.and_then(extract_rule);
        return Ok(PproxyUri {
            scheme,
            username: None,
            password: None,
            host: String::new(),
            port: 0,
            tls,
            inbound,
            rule,
            path: Some(path),
        });
    }

    // Handle redir:// scheme — supports host:port or just :port
    if scheme == "redir" {
        let (credentials, endpoint_str) = if let Some(at_pos) = after_scheme.find('@') {
            let userinfo = &after_scheme[..at_pos];
            let ep = &after_scheme[at_pos + 1..];
            let (user, pass) = parse_userinfo(userinfo)?;
            (Some((user, pass)), ep)
        } else {
            (None, after_scheme)
        };

        // redir://:12345 means empty host (bind all), redir://127.0.0.1:12345 means specific
        let (host, port) = parse_endpoint(endpoint_str)?;
        let rule = query.and_then(extract_rule);
        return Ok(PproxyUri {
            scheme,
            username: credentials.as_ref().map(|c| c.0.clone()),
            password: credentials.as_ref().map(|c| c.1.clone()),
            host,
            port,
            tls,
            inbound,
            rule,
            path: None,
        });
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
        inbound,
        rule,
        path: None,
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

/// Check if a Shadowsocks method name is a known legacy stream cipher.
///
/// Legacy stream ciphers lack authentication and are not supported by eggress.
/// This function is used for diagnostic purposes in the pproxy compat layer.
pub fn is_legacy_ss_method(method: &str) -> bool {
    matches!(
        method.to_lowercase().as_str(),
        "aes-128-ctr"
            | "aes-192-ctr"
            | "aes-256-ctr"
            | "aes-128-cfb"
            | "aes-192-cfb"
            | "aes-256-cfb"
            | "rc4"
            | "rc4-md5"
            | "chacha20-ietf"
    )
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

    #[test]
    fn test_redacted_display_tls_suffix_in_scheme() {
        let uri = parse_pproxy_uri("socks5+tls://proxy:1080").unwrap();
        assert_eq!(uri.redacted_display(), "socks5+tls://proxy:1080");
    }

    #[test]
    fn test_endpoint_display_brackets_ipv6() {
        let uri = parse_pproxy_uri("socks5://[::1]:1080").unwrap();
        assert_eq!(uri.endpoint_display(), "[::1]:1080");
    }

    #[test]
    fn test_unix_socket_path() {
        let uri = parse_pproxy_uri("unix:///tmp/eggress.sock").unwrap();
        assert_eq!(uri.scheme, "unix");
        assert_eq!(uri.path.as_deref(), Some("/tmp/eggress.sock"));
        assert!(uri.host.is_empty());
        assert_eq!(uri.port, 0);
    }

    #[test]
    fn test_unix_socket_relative_path() {
        let uri = parse_pproxy_uri("unix://var/run/proxy.sock").unwrap();
        assert_eq!(uri.scheme, "unix");
        assert_eq!(uri.path.as_deref(), Some("/var/run/proxy.sock"));
    }

    #[test]
    fn test_unix_socket_empty_path_errors() {
        let err = parse_pproxy_uri("unix://").unwrap_err();
        match err {
            CompatError::InvalidUri { message } => {
                assert!(message.contains("requires a path"));
            }
            _ => panic!("expected InvalidUri for empty unix path"),
        }
    }

    #[test]
    fn test_unix_redacted_display() {
        let uri = parse_pproxy_uri("unix:///tmp/secret.sock").unwrap();
        let display = uri.redacted_display();
        assert_eq!(display, "unix:///tmp/****");
        assert!(!display.contains("secret"));
    }

    #[test]
    fn test_unix_redacted_display_nested() {
        let uri = parse_pproxy_uri("unix:///var/run/myapp/secret.sock").unwrap();
        let display = uri.redacted_display();
        assert_eq!(display, "unix:///var/run/myapp/****");
    }

    #[test]
    fn test_redir_colon_port() {
        let uri = parse_pproxy_uri("redir://:12345").unwrap();
        assert_eq!(uri.scheme, "redir");
        assert_eq!(uri.host, "");
        assert_eq!(uri.port, 12345);
        assert!(uri.path.is_none());
    }

    #[test]
    fn test_redir_host_port() {
        let uri = parse_pproxy_uri("redir://127.0.0.1:12345").unwrap();
        assert_eq!(uri.scheme, "redir");
        assert_eq!(uri.host, "127.0.0.1");
        assert_eq!(uri.port, 12345);
    }

    #[test]
    fn test_redir_bind_display() {
        let uri = parse_pproxy_uri("redir://:12345").unwrap();
        assert_eq!(uri.bind_display(), "0.0.0.0:12345");
    }

    #[test]
    fn test_redir_specific_bind_display() {
        let uri = parse_pproxy_uri("redir://127.0.0.1:12345").unwrap();
        assert_eq!(uri.bind_display(), "127.0.0.1:12345");
    }

    #[test]
    fn test_redir_redacted_display() {
        let uri = parse_pproxy_uri("redir://:12345").unwrap();
        assert_eq!(uri.redacted_display(), "redir://:12345");
    }

    #[test]
    fn test_bind_uri() {
        let uri = parse_pproxy_uri("bind://0.0.0.0:8080").unwrap();
        assert_eq!(uri.scheme, "bind");
        assert_eq!(uri.host, "0.0.0.0");
        assert_eq!(uri.port, 8080);
        assert!(uri.is_reverse());
        assert!(!uri.inbound);
    }

    #[test]
    fn test_listen_uri() {
        let uri = parse_pproxy_uri("listen://127.0.0.1:9090").unwrap();
        assert_eq!(uri.scheme, "listen");
        assert!(uri.is_reverse());
    }

    #[test]
    fn test_backward_uri() {
        let uri = parse_pproxy_uri("backward://0.0.0.0:8080").unwrap();
        assert_eq!(uri.scheme, "backward");
        assert!(uri.is_reverse());
    }

    #[test]
    fn test_rebind_uri() {
        let uri = parse_pproxy_uri("rebind://0.0.0.0:8080").unwrap();
        assert_eq!(uri.scheme, "rebind");
        assert!(uri.is_reverse());
    }

    #[test]
    fn test_bind_with_auth() {
        let uri = parse_pproxy_uri("bind://user:pass@0.0.0.0:8080").unwrap();
        assert_eq!(uri.scheme, "bind");
        assert_eq!(uri.username.as_deref(), Some("user"));
        assert_eq!(uri.password.as_deref(), Some("pass"));
        assert!(uri.is_reverse());
    }

    #[test]
    fn test_bind_with_tls() {
        let uri = parse_pproxy_uri("bind+tls://0.0.0.0:8443").unwrap();
        assert_eq!(uri.scheme, "bind");
        assert!(uri.tls);
        assert!(uri.is_reverse());
    }

    #[test]
    fn test_bind_with_inbound_modifier() {
        let uri = parse_pproxy_uri("socks5+in://0.0.0.0:1080").unwrap();
        assert_eq!(uri.scheme, "socks5");
        assert!(uri.inbound);
    }

    #[test]
    fn test_bind_redacted_display() {
        let uri = parse_pproxy_uri("bind://user:pass@0.0.0.0:8080").unwrap();
        let display = uri.redacted_display();
        assert!(display.contains("****:****@"));
        assert!(!display.contains("pass"));
    }

    #[test]
    fn test_bind_tls_in_redacted_display() {
        let uri = parse_pproxy_uri("bind+tls://0.0.0.0:8443").unwrap();
        assert_eq!(uri.redacted_display(), "bind+tls://0.0.0.0:8443");
    }

    #[test]
    fn test_not_reverse_schemes() {
        let uri = parse_pproxy_uri("socks5://127.0.0.1:1080").unwrap();
        assert!(!uri.is_reverse());

        let uri = parse_pproxy_uri("http://proxy:8080").unwrap();
        assert!(!uri.is_reverse());
    }
}
