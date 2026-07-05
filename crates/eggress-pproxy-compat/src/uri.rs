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
    /// Whether SSL modifier was used (+ssl suffix, treated as unsupported variant of +tls).
    pub ssl: bool,
    /// Whether this is a reverse/inbound URI (+in suffix).
    pub inbound: bool,
    /// Count of `+in` tokens parsed from the scheme (backward connection count).
    pub backward_num: u32,
    /// Optional rule parameter from query string.
    pub rule: Option<String>,
    /// Optional path (used for unix:// scheme).
    pub path: Option<String>,
}

impl PproxyUri {
    /// Returns true if this is a reverse proxy listener URI (bind/listen/backward/rebind scheme).
    pub fn is_reverse_listener(&self) -> bool {
        matches!(
            self.scheme.as_str(),
            "bind" | "listen" | "backward" | "rebind"
        )
    }

    /// Returns true if this is a backward/upstream URI with the `+in` modifier
    /// (e.g., `socks5+in://...`).
    pub fn is_backward(&self) -> bool {
        self.inbound
    }

    /// Returns the number of `+in` tokens parsed from the scheme (the backward
    /// connection count). A single `+in` yields 1; multiple `+in+in` yields 2, etc.
    /// Returns 0 if no `+in` modifier is present.
    pub fn backward_num(&self) -> u32 {
        self.backward_num
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

    // Parse +tls suffix and +in modifier (supports multiple occurrences)
    let mut tls = false;
    let mut ssl = false;
    let mut inbound = false;
    let mut backward_num: u32 = 0;
    let mut scheme = scheme_part;
    loop {
        if scheme.ends_with("+tls") {
            tls = true;
            scheme = scheme[..scheme.len() - 4].to_string();
            continue;
        }
        if scheme.ends_with("+ssl") {
            ssl = true;
            tls = true;
            scheme = scheme[..scheme.len() - 4].to_string();
            continue;
        }
        if scheme.ends_with("+in") {
            inbound = true;
            backward_num += 1;
            scheme = scheme[..scheme.len() - 3].to_string();
            continue;
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
            ssl,
            inbound,
            backward_num,
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
            ssl,
            inbound,
            backward_num,
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
        ssl,
        inbound,
        backward_num,
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

/// A parsed pproxy chain (one or more hops separated by `__`).
#[derive(Debug, Clone)]
pub struct PproxyChain {
    /// The raw input string.
    pub raw: String,
    /// Parsed hops in order (left = first hop, right = the last hop).
    pub hops: Vec<PproxyUri>,
}

impl PproxyChain {
    /// Redacted display showing all hops separated by `__`.
    pub fn redacted_display(&self) -> String {
        self.hops
            .iter()
            .map(|h| h.redacted_display())
            .collect::<Vec<_>>()
            .join("__")
    }
}

/// Parse a pproxy chain URI (one or more hops separated by `__`).
///
/// Single-hop URIs without `__` are valid chains with one hop.
/// Returns an error for:
/// - Leading, trailing, or doubled `__` separators
/// - Empty hop segments
/// - Semicolon or comma separators (not supported in pproxy)
pub fn parse_pproxy_chain(uri: &str) -> Result<PproxyChain, CompatError> {
    // Detect semicolon or comma separators with structured error
    if uri.contains(';') || uri.contains(',') {
        return Err(CompatError::InvalidUri {
            message: format!(
                "semicolon and comma are not chain separators in pproxy; use '__' (double underscore) to separate hops: {}",
                uri
            ),
        });
    }

    // Check for leading/trailing __
    if uri.starts_with("__") || uri.ends_with("__") {
        return Err(CompatError::InvalidUri {
            message: format!("chain URI has leading or trailing '__' separator: {}", uri),
        });
    }

    // Check for doubled ____
    if uri.contains("____") {
        return Err(CompatError::InvalidUri {
            message: format!("chain URI has doubled '____' separator: {}", uri),
        });
    }

    let mut hops = Vec::new();
    for segment in uri.split("__") {
        if segment.is_empty() {
            return Err(CompatError::InvalidUri {
                message: format!("chain URI has empty hop segment: {}", uri),
            });
        }
        let hop = parse_pproxy_uri(segment)?;
        hops.push(hop);
    }

    Ok(PproxyChain {
        raw: uri.to_string(),
        hops,
    })
}

/// Check if any hop in a chain uses an unsupported protocol for chaining.
///
/// Returns a list of (hop_index, protocol_name) for unsupported hops.
pub fn validate_chain_hops(chain: &PproxyChain) -> Vec<(usize, String)> {
    let mut unsupported = Vec::new();
    for (idx, hop) in chain.hops.iter().enumerate() {
        match hop.scheme.as_str() {
            "ssh" | "ssr" | "unix" | "redir" | "direct" | "h2" | "ws" | "wss" | "raw"
            | "tunnel" => {
                unsupported.push((idx, hop.scheme.clone()));
            }
            _ => {} // http, https, socks4, socks4a, socks5, trojan, ss, shadowsocks are supported
        }
    }
    unsupported
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
        assert!(uri.is_reverse_listener());
        assert!(!uri.inbound);
    }

    #[test]
    fn test_listen_uri() {
        let uri = parse_pproxy_uri("listen://127.0.0.1:9090").unwrap();
        assert_eq!(uri.scheme, "listen");
        assert!(uri.is_reverse_listener());
    }

    #[test]
    fn test_backward_uri() {
        let uri = parse_pproxy_uri("backward://0.0.0.0:8080").unwrap();
        assert_eq!(uri.scheme, "backward");
        assert!(uri.is_reverse_listener());
    }

    #[test]
    fn test_rebind_uri() {
        let uri = parse_pproxy_uri("rebind://0.0.0.0:8080").unwrap();
        assert_eq!(uri.scheme, "rebind");
        assert!(uri.is_reverse_listener());
    }

    #[test]
    fn test_bind_with_auth() {
        let uri = parse_pproxy_uri("bind://user:pass@0.0.0.0:8080").unwrap();
        assert_eq!(uri.scheme, "bind");
        assert_eq!(uri.username.as_deref(), Some("user"));
        assert_eq!(uri.password.as_deref(), Some("pass"));
        assert!(uri.is_reverse_listener());
    }

    #[test]
    fn test_bind_with_tls() {
        let uri = parse_pproxy_uri("bind+tls://0.0.0.0:8443").unwrap();
        assert_eq!(uri.scheme, "bind");
        assert!(uri.tls);
        assert!(uri.is_reverse_listener());
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
        assert!(!uri.is_reverse_listener());

        let uri = parse_pproxy_uri("http://proxy:8080").unwrap();
        assert!(!uri.is_reverse_listener());
    }

    #[test]
    fn test_inbound_modifier() {
        let uri = parse_pproxy_uri("socks5+in://acceptor:1080").unwrap();
        assert!(uri.is_backward());
        assert!(!uri.is_reverse_listener());
        assert_eq!(uri.backward_num(), 1);
    }

    #[test]
    fn test_multiple_inbound_tokens() {
        let uri = parse_pproxy_uri("socks5+in+in://acceptor:1080").unwrap();
        assert!(uri.is_backward());
        assert_eq!(uri.backward_num(), 2);
    }

    #[test]
    fn test_backward_num_zero_without_in() {
        let uri = parse_pproxy_uri("socks5://proxy:1080").unwrap();
        assert!(!uri.is_backward());
        assert_eq!(uri.backward_num(), 0);
    }

    #[test]
    fn test_parse_two_hop_chain() {
        let chain = parse_pproxy_chain("http://hop1:8080__socks5://hop2:1080").unwrap();
        assert_eq!(chain.hops.len(), 2);
        assert_eq!(chain.hops[0].scheme, "http");
        assert_eq!(chain.hops[0].host, "hop1");
        assert_eq!(chain.hops[0].port, 8080);
        assert_eq!(chain.hops[1].scheme, "socks5");
        assert_eq!(chain.hops[1].host, "hop2");
        assert_eq!(chain.hops[1].port, 1080);
    }

    #[test]
    fn test_parse_three_hop_chain() {
        let chain = parse_pproxy_chain("http://h1:80__socks5://h2:1080__socks4://h3:1080").unwrap();
        assert_eq!(chain.hops.len(), 3);
    }

    #[test]
    fn test_parse_single_hop_chain() {
        let chain = parse_pproxy_chain("socks5://proxy:1080").unwrap();
        assert_eq!(chain.hops.len(), 1);
        assert_eq!(chain.hops[0].scheme, "socks5");
    }

    #[test]
    fn test_parse_chain_with_creds() {
        let chain = parse_pproxy_chain("http://user:pass@h1:80__socks5://h2:1080").unwrap();
        assert_eq!(chain.hops.len(), 2);
        assert_eq!(chain.hops[0].username.as_deref(), Some("user"));
        assert_eq!(chain.hops[0].password.as_deref(), Some("pass"));
    }

    #[test]
    fn test_parse_chain_with_tls_modifier() {
        let chain = parse_pproxy_chain("socks5+tls://h1:1080__http://h2:80").unwrap();
        assert!(chain.hops[0].tls);
        assert!(!chain.hops[1].tls);
    }

    #[test]
    fn test_parse_chain_semicolon_rejected() {
        let err = parse_pproxy_chain("http://h1:80;socks5://h2:1080").unwrap_err();
        match err {
            CompatError::InvalidUri { message } => {
                assert!(message.contains("semicolon"));
            }
            _ => panic!("expected InvalidUri for semicolon"),
        }
    }

    #[test]
    fn test_parse_chain_comma_rejected() {
        let err = parse_pproxy_chain("http://h1:80,socks5://h2:1080").unwrap_err();
        match err {
            CompatError::InvalidUri { message } => {
                assert!(message.contains("comma"));
            }
            _ => panic!("expected InvalidUri for comma"),
        }
    }

    #[test]
    fn test_parse_chain_leading_separator() {
        let err = parse_pproxy_chain("__http://h1:80").unwrap_err();
        match err {
            CompatError::InvalidUri { message } => {
                assert!(message.contains("leading"));
            }
            _ => panic!("expected InvalidUri for leading separator"),
        }
    }

    #[test]
    fn test_parse_chain_trailing_separator() {
        let err = parse_pproxy_chain("http://h1:80__").unwrap_err();
        match err {
            CompatError::InvalidUri { message } => {
                assert!(message.contains("trailing"));
            }
            _ => panic!("expected InvalidUri for trailing separator"),
        }
    }

    #[test]
    fn test_parse_chain_empty_segment() {
        let err = parse_pproxy_chain("http://h1:80____socks5://h2:1080").unwrap_err();
        match err {
            CompatError::InvalidUri { message } => {
                assert!(message.contains("doubled"));
            }
            _ => panic!("expected InvalidUri for doubled separator"),
        }
    }

    #[test]
    fn test_chain_redacted_display() {
        let chain = parse_pproxy_chain("http://user:pass@h1:80__socks5://h2:1080").unwrap();
        let display = chain.redacted_display();
        assert!(display.contains("****"));
        assert!(!display.contains("pass"));
        assert!(display.contains("__"));
    }

    #[test]
    fn test_validate_chain_hops_all_supported() {
        let chain = parse_pproxy_chain("http://h1:80__socks5://h2:1080").unwrap();
        let unsupported = validate_chain_hops(&chain);
        assert!(unsupported.is_empty());
    }

    #[test]
    fn test_validate_chain_hops_ssh_unsupported() {
        let chain = parse_pproxy_chain("http://h1:80__ssh://h2:22").unwrap();
        let unsupported = validate_chain_hops(&chain);
        assert_eq!(unsupported.len(), 1);
        assert_eq!(unsupported[0], (1, "ssh".to_string()));
    }

    #[test]
    fn test_validate_chain_hops_ssr_unsupported() {
        let chain = parse_pproxy_chain("http://h1:80__ssr://h2:8388").unwrap();
        let unsupported = validate_chain_hops(&chain);
        assert_eq!(unsupported.len(), 1);
        assert_eq!(unsupported[0], (1, "ssr".to_string()));
    }
}
