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
    /// Optional rules_file parameter from query string (pproxy URI-attached rule file).
    pub rules_file: Option<String>,
    /// Canonical pproxy rule suffix when the query is not `rule=`/`rules_file=`.
    pub rule_suffix: Option<String>,
    /// Optional path (used for unix:// scheme).
    pub path: Option<String>,
    /// Protocol tokens in the original `scheme`, excluding transport modifiers.
    pub protocol_chain: Vec<String>,
    /// Non-protocol scheme modifiers, in source order (`tls`, `ssl`, `in`, ...).
    pub transport_modifiers: Vec<String>,
    /// pproxy's optional outbound source binding (`/@localbind`).
    pub local_bind: Option<String>,
    /// Fixed destination used by tunnel-style protocols.
    pub fixed_target: Option<String>,
    /// Comma-delimited plugin metadata. Plugins are parsed but not executed here.
    pub plugins: Vec<PproxyPluginSpec>,
    /// Fragment authentication, kept separately from URL userinfo.
    pub auth_fragment: Option<String>,
    /// The raw URI, retained for diagnostics.
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PproxyPluginSpec {
    pub name: String,
    pub options: Option<String>,
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
        let rules_file_str = match &self.rules_file {
            Some(rf) => format!("?rules_file={}", rf),
            None => String::new(),
        };
        let suffix = self
            .rule_suffix
            .as_deref()
            .map(|r| format!("?{r}"))
            .unwrap_or_default();
        let bind = self
            .local_bind
            .as_deref()
            .map(|b| format!("/@{}", b))
            .unwrap_or_default();
        let plugins = if self.plugins.is_empty() {
            String::new()
        } else {
            format!(
                ",{}",
                self.plugins
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let target = self
            .fixed_target
            .as_deref()
            .map(|t| format!("{{{t}}}"))
            .unwrap_or_else(|| self.endpoint_display());
        format!(
            "{}://{}{}{}{}{}{}{}",
            self.scheme_with_tls(),
            cred_str,
            target,
            rule_str,
            rules_file_str,
            suffix,
            bind,
            plugins,
        )
    }

    pub(crate) fn scheme_with_tls(&self) -> String {
        let mut parts = if self.protocol_chain.is_empty() {
            vec![self.scheme.clone()]
        } else {
            self.protocol_chain.clone()
        };
        if self.tls && !parts.iter().any(|p| p == "tls") {
            parts.push("tls".to_string());
        }
        if self.ssl && !parts.iter().any(|p| p == "ssl") {
            parts.push("ssl".to_string());
        }
        if self.inbound {
            for _ in 0..self.backward_num.max(1) {
                parts.push("in".to_string());
            }
        }
        parts.join("+")
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
    // The Python compatibility helpers historically accepted a listener URI
    // followed by a legacy `;` or `__` remote suffix. The typed single-URI
    // parser describes the listener portion; callers that need every hop use
    // `parse_pproxy_chain`.
    let parse_uri = if uri.contains("__") {
        split_chain_hops(uri).into_iter().next().unwrap_or(uri)
    } else {
        uri.split_once(';').map_or(uri, |(head, _)| head)
    };
    let (without_fragment, auth_fragment) = split_top_level(parse_uri, '#');
    let (before_query, query) = split_top_level(without_fragment, '?');

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

    // Parse protocol tokens and transport modifiers. Keeping both lists avoids
    // treating a combined listener as one protocol during translation.
    let mut tls = false;
    let mut ssl = false;
    let mut inbound = false;
    let mut backward_num: u32 = 0;
    let mut protocol_chain = Vec::new();
    let mut transport_modifiers = Vec::new();
    for token in scheme_part.split('+') {
        match token {
            "tls" => {
                tls = true;
                transport_modifiers.push(token.to_string());
            }
            "ssl" | "secure" => {
                ssl = true;
                tls = true;
                transport_modifiers.push(token.to_string());
            }
            "in" => {
                inbound = true;
                backward_num += 1;
                transport_modifiers.push(token.to_string());
            }
            "" => {
                return Err(CompatError::InvalidUri {
                    message: "empty protocol or modifier in scheme".to_string(),
                })
            }
            token => protocol_chain.push(token.to_string()),
        }
    }
    if protocol_chain.is_empty() {
        return Err(CompatError::InvalidUri {
            message: "URI scheme has no protocol".to_string(),
        });
    }
    let scheme = protocol_chain.join("+");

    // Validate known schemes
    for protocol in &protocol_chain {
        match protocol.as_str() {
            "http" | "https" | "socks4" | "socks4a" | "socks5" | "trojan" | "ss"
            | "shadowsocks" | "ssr" | "direct" | "ssh" | "unix" | "redir" | "h2" | "ws" | "wss"
            | "raw" | "tunnel" | "bind" | "listen" | "backward" | "rebind" | "httponly" => {}
            other => {
                return Err(CompatError::UnsupportedProtocol(other.to_string()));
            }
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
        let (rule, rules_file, rule_suffix) = query
            .map(extract_query_params)
            .unwrap_or((None, None, None));
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
            rules_file,
            rule_suffix,
            path: Some(path),
            protocol_chain,
            transport_modifiers,
            local_bind: None,
            fixed_target: None,
            plugins: Vec::new(),
            auth_fragment: auth_fragment.map(str::to_string),
            raw: uri.to_string(),
        });
    }

    let (endpoint_part, path_part) = split_top_level(after_scheme, '/');
    let (credentials, endpoint_str) =
        if let Some(at_pos) = find_last_at_outside_brackets(endpoint_part) {
            let (user, pass) = parse_userinfo(&endpoint_part[..at_pos])?;
            (Some((user, pass)), &endpoint_part[at_pos + 1..])
        } else {
            (None, endpoint_part)
        };
    let fixed_target = if endpoint_str.starts_with('{') && endpoint_str.ends_with('}') {
        Some(endpoint_str[1..endpoint_str.len() - 1].to_string())
    } else {
        None
    };
    let endpoint_for_parse = fixed_target.as_deref().unwrap_or(endpoint_str);
    let (host, mut port, port_specified) = parse_endpoint(endpoint_for_parse)?;
    if !port_specified && !host.is_empty() {
        if let Some(default) = default_port_for_scheme(&scheme) {
            port = default;
        }
    }

    let (local_bind, plugins) = parse_path_metadata(path_part);
    let (rule, rules_file, rule_suffix) = query
        .map(extract_query_params)
        .unwrap_or((None, None, None));
    let (fragment_user, fragment_password) = auth_fragment
        .filter(|a| !a.is_empty())
        .map(parse_userinfo)
        .transpose()?
        .map_or((None, None), |(u, p)| (Some(u), Some(p)));
    let credentials = credentials.or_else(|| fragment_user.zip(fragment_password));

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
        rules_file,
        path: None,
        rule_suffix,
        protocol_chain,
        transport_modifiers,
        local_bind,
        fixed_target,
        plugins,
        auth_fragment: auth_fragment.map(str::to_string),
        raw: uri.to_string(),
    })
}

fn split_top_level(input: &str, delimiter: char) -> (&str, Option<&str>) {
    let mut bracket = 0u32;
    let mut brace = 0u32;
    for (idx, ch) in input.char_indices() {
        match ch {
            '[' => bracket += 1,
            ']' => bracket = bracket.saturating_sub(1),
            '{' => brace += 1,
            '}' => brace = brace.saturating_sub(1),
            _ => {}
        }
        if ch == delimiter && bracket == 0 && brace == 0 {
            return (&input[..idx], Some(&input[idx + 1..]));
        }
    }
    (input, None)
}

fn parse_path_metadata(path: Option<&str>) -> (Option<String>, Vec<PproxyPluginSpec>) {
    let Some(path) = path else {
        return (None, Vec::new());
    };
    let (bind, plugin_text) = if let Some(rest) = path.strip_prefix("@") {
        (Some(rest), None)
    } else if let Some(pos) = path.find("/@") {
        (Some(&path[pos + 2..]), None)
    } else {
        (None, Some(path.trim_start_matches('/')))
    };
    let (bind, plugin_text) = if let Some(bind) = bind {
        let (b, p) = bind
            .split_once(',')
            .map_or((bind, None), |(b, p)| (b, Some(p)));
        (Some(b.to_string()), p)
    } else {
        (bind.map(str::to_string), plugin_text)
    };
    let plugins = plugin_text
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|spec| {
            let (name, options) = spec
                .split_once('=')
                .map_or((spec, None), |(n, o)| (n, Some(o.to_string())));
            PproxyPluginSpec {
                name: name.to_string(),
                options,
            }
        })
        .collect();
    (bind, plugins)
}

/// Find the position of the LAST unbracketed `@` in `s`. The userinfo
/// separator is the last `@` after the scheme, not the first; a raw
/// password containing `@` must not be truncated by the parser.
fn find_last_at_outside_brackets(s: &str) -> Option<usize> {
    let mut last_at: Option<usize> = None;
    let mut bracket_depth = 0u32;
    for (i, c) in s.char_indices() {
        match c {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '@' if bracket_depth == 0 => last_at = Some(i),
            _ => {}
        }
    }
    last_at
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

fn parse_endpoint(endpoint: &str) -> Result<(String, u16, bool), CompatError> {
    if endpoint.is_empty() {
        return Ok((String::new(), 0, false));
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
        return Ok((host.to_string(), port, true));
    }

    // Regular host:port
    let colon_pos = match endpoint.rfind(':') {
        Some(pos) => pos,
        None => {
            return Ok((endpoint.to_string(), 0, false));
        }
    };
    let host = &endpoint[..colon_pos];
    let port_str = &endpoint[colon_pos + 1..];
    let port = port_str
        .parse::<u16>()
        .map_err(|e| CompatError::InvalidUri {
            message: format!("invalid port '{}': {}", port_str, e),
        })?;

    Ok((host.to_string(), port, true))
}

fn default_port_for_scheme(scheme: &str) -> Option<u16> {
    match scheme {
        // pproxy's proxy_by_uri() uses 8080 for every non-SSH endpoint when
        // the port is omitted. Keep conventional native defaults out of this
        // compatibility parser.
        "ssh" => Some(22),
        "unix" | "direct" => None,
        _ => Some(8080),
    }
}

fn extract_query_params(query: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut rule = None;
    let mut rules_file = None;
    for param in query.split('&') {
        if let Some(eq_pos) = param.find('=') {
            let key = &param[..eq_pos];
            let value = &param[eq_pos + 1..];
            if !value.is_empty() {
                match key {
                    "rule" => rule = Some(value.to_string()),
                    "rules_file" => rules_file = Some(value.to_string()),
                    _ => {}
                }
            }
        }
    }
    let suffix = if rule.is_none() && rules_file.is_none() && !query.is_empty() {
        Some(query.to_string())
    } else {
        None
    };
    (rule, rules_file, suffix)
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
    // Semicolons are never pproxy chain separators. Commas belong to plugin
    // metadata and must remain inside their hop.
    if uri.contains(';') {
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
    for segment in split_chain_hops(uri) {
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

fn split_chain_hops(uri: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut bracket = 0u32;
    let mut brace = 0u32;
    let bytes = uri.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] as char {
            '[' => bracket += 1,
            ']' => bracket = bracket.saturating_sub(1),
            '{' => brace += 1,
            '}' => brace = brace.saturating_sub(1),
            '_' if i + 1 < bytes.len() && bytes[i + 1] == b'_' && bracket == 0 && brace == 0 => {
                result.push(&uri[start..i]);
                i += 1;
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    result.push(&uri[start..]);
    result
}

/// Check if any hop in a chain uses an unsupported protocol for chaining.
///
/// Returns a list of (hop_index, protocol_name) for unsupported hops.
pub fn validate_chain_hops(chain: &PproxyChain) -> Vec<(usize, String)> {
    let mut unsupported = Vec::new();
    for (idx, hop) in chain.hops.iter().enumerate() {
        match hop.scheme.as_str() {
            "ssh" | "ssr" | "unix" | "redir" | "direct" => {
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
    fn test_redacted_display_explicit_zero_port() {
        let uri = parse_pproxy_uri("socks5://host:0").unwrap();
        assert_eq!(uri.redacted_display(), "socks5://host:0");
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
    fn test_parse_chain_plugin_comma_preserved() {
        let chain = parse_pproxy_chain("http://h1:80/,plugin").unwrap();
        assert_eq!(chain.hops[0].plugins[0].name, "plugin");
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

    #[test]
    fn test_default_port_socks5() {
        let uri = parse_pproxy_uri("socks5://host").unwrap();
        assert_eq!(uri.host, "host");
        assert_eq!(uri.port, 8080);
    }

    #[test]
    fn test_default_port_http() {
        let uri = parse_pproxy_uri("http://host").unwrap();
        assert_eq!(uri.host, "host");
        assert_eq!(uri.port, 8080);
    }

    #[test]
    fn test_default_port_https() {
        let uri = parse_pproxy_uri("https://host").unwrap();
        assert_eq!(uri.host, "host");
        assert_eq!(uri.port, 8080);
    }

    #[test]
    fn test_default_port_trojan() {
        let uri = parse_pproxy_uri("trojan://password@host").unwrap();
        assert_eq!(uri.host, "host");
        assert_eq!(uri.port, 8080);
    }

    #[test]
    fn test_default_port_shadowsocks() {
        let uri = parse_pproxy_uri("ss://method:pass@host").unwrap();
        assert_eq!(uri.host, "host");
        assert_eq!(uri.port, 8080);
    }

    #[test]
    fn test_explicit_port_overrides_default() {
        let uri = parse_pproxy_uri("socks5://host:9090").unwrap();
        assert_eq!(uri.host, "host");
        assert_eq!(uri.port, 9090);
    }

    #[test]
    fn test_empty_port_with_colon() {
        let uri = parse_pproxy_uri("socks5://:1080").unwrap();
        assert_eq!(uri.host, "");
        assert_eq!(uri.port, 1080);
    }

    #[test]
    fn test_chain_default_ports() {
        let chain = parse_pproxy_chain("socks5://h1__http://h2").unwrap();
        assert_eq!(chain.hops[0].port, 8080);
        assert_eq!(chain.hops[1].port, 8080);
    }

    #[test]
    fn test_explicit_zero_port_preserved_socks5() {
        let uri = parse_pproxy_uri("socks5://127.0.0.1:0").unwrap();
        assert_eq!(uri.host, "127.0.0.1");
        assert_eq!(uri.port, 0);
    }

    #[test]
    fn test_explicit_zero_port_preserved_http() {
        let uri = parse_pproxy_uri("http://example.com:0").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(uri.port, 0);
    }

    #[test]
    fn test_explicit_zero_port_preserved_https() {
        let uri = parse_pproxy_uri("https://example.com:0").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(uri.port, 0);
    }

    #[test]
    fn test_explicit_zero_port_preserved_trojan() {
        let uri = parse_pproxy_uri("trojan://password@example.com:0").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(uri.port, 0);
    }

    #[test]
    fn test_password_containing_at_sign() {
        // Regression: raw '@' inside the password must not be treated as
        // the userinfo/host separator. The userinfo separator is the LAST
        // unbracketed '@' after the scheme.
        let uri = parse_pproxy_uri("socks5://admin:s3cret_p@ssw0rd@127.0.0.1:1080").unwrap();
        assert_eq!(uri.scheme, "socks5");
        assert_eq!(uri.username.as_deref(), Some("admin"));
        assert_eq!(uri.password.as_deref(), Some("s3cret_p@ssw0rd"));
        assert_eq!(uri.host, "127.0.0.1");
        assert_eq!(uri.port, 1080);
    }

    #[test]
    fn test_password_containing_at_sign_redacted_display() {
        // Regression: redacted display must not leak any part of the
        // password even when the password contains '@'.
        let uri = parse_pproxy_uri("socks5://admin:s3cret_p@ssw0rd@127.0.0.1:1080").unwrap();
        let display = uri.redacted_display();
        assert_eq!(display, "socks5://****:****@127.0.0.1:1080");
        assert!(!display.contains("s3cret_p"));
        assert!(!display.contains("ssw0rd"));
        assert!(!display.contains("admin"));
    }

    #[test]
    fn test_password_containing_at_sign_chain() {
        // Regression: last '@' must also be honored inside chain hops.
        let chain = parse_pproxy_chain("socks5://user:p@ss@proxy1:1080__http://h2:8080").unwrap();
        assert_eq!(chain.hops.len(), 2);
        assert_eq!(chain.hops[0].username.as_deref(), Some("user"));
        assert_eq!(chain.hops[0].password.as_deref(), Some("p@ss"));
        assert_eq!(chain.hops[0].host, "proxy1");
        assert_eq!(chain.hops[0].port, 1080);
    }

    #[test]
    fn test_password_containing_at_sign_redir() {
        // Regression: redir:// must also use the LAST unbracketed '@'.
        let uri = parse_pproxy_uri("redir://admin:s3cret_p@ssw0rd@127.0.0.1:12345").unwrap();
        assert_eq!(uri.scheme, "redir");
        assert_eq!(uri.username.as_deref(), Some("admin"));
        assert_eq!(uri.password.as_deref(), Some("s3cret_p@ssw0rd"));
        assert_eq!(uri.host, "127.0.0.1");
        assert_eq!(uri.port, 12345);
        assert_eq!(uri.redacted_display(), "redir://****:****@127.0.0.1:12345");
    }

    #[test]
    fn test_password_containing_at_sign_shadowsocks() {
        // Regression: ss:// userinfo "method:password@host:port" with '@' in
        // the password must keep the full password.
        let uri = parse_pproxy_uri("ss://aes-256-gcm:p@ssw0rd@proxy:8388").unwrap();
        assert_eq!(uri.scheme, "ss");
        assert_eq!(uri.username.as_deref(), Some("aes-256-gcm"));
        assert_eq!(uri.password.as_deref(), Some("p@ssw0rd"));
        assert_eq!(uri.host, "proxy");
        assert_eq!(uri.port, 8388);
    }

    #[test]
    fn test_password_containing_at_sign_trojan() {
        // Regression: trojan:// accepts password-only creds; '@' in the
        // password must be preserved.
        let uri = parse_pproxy_uri("trojan://my_p@ssw0rd@server:443").unwrap();
        assert_eq!(uri.scheme, "trojan");
        assert_eq!(uri.username.as_deref(), Some(""));
        assert_eq!(uri.password.as_deref(), Some("my_p@ssw0rd"));
        assert_eq!(uri.host, "server");
        assert_eq!(uri.port, 443);
    }

    #[test]
    fn test_with_rules_file() {
        let uri =
            parse_pproxy_uri("socks5://127.0.0.1:1080?rules_file=/path/to/rules.txt").unwrap();
        assert_eq!(uri.rules_file.as_deref(), Some("/path/to/rules.txt"));
    }

    #[test]
    fn test_with_rules_file_and_rule() {
        let uri =
            parse_pproxy_uri("socks5://127.0.0.1:1080?rule=.*\\.com&rules_file=/path/to/rules.txt")
                .unwrap();
        assert_eq!(uri.rule.as_deref(), Some(".*\\.com"));
        assert_eq!(uri.rules_file.as_deref(), Some("/path/to/rules.txt"));
    }

    #[test]
    fn test_combined_listener_tokens_and_modifiers() {
        let uri = parse_pproxy_uri("http+socks4+socks5+tls+in+in://:8080").unwrap();
        assert_eq!(uri.protocol_chain, ["http", "socks4", "socks5"]);
        assert_eq!(uri.backward_num(), 2);
        assert!(uri.tls);
        assert_eq!(uri.scheme, "http+socks4+socks5");
    }

    #[test]
    fn test_fragment_auth_local_bind_and_plugins() {
        let uri =
            parse_pproxy_uri("http://proxy:8080/@192.0.2.1,obfs=secret,plain#user:pass").unwrap();
        assert_eq!(uri.local_bind.as_deref(), Some("192.0.2.1"));
        assert_eq!(uri.plugins.len(), 2);
        assert_eq!(uri.plugins[0].name, "obfs");
        assert_eq!(uri.auth_fragment.as_deref(), Some("user:pass"));
        assert_eq!(uri.username.as_deref(), Some("user"));
        assert!(!uri.redacted_display().contains("secret"));
        assert!(!uri.redacted_display().contains("pass"));
    }

    #[test]
    fn test_fixed_target_and_raw_rule_suffix() {
        let uri = parse_pproxy_uri("tunnel://{example.com:443}?example\\.com$").unwrap();
        assert_eq!(uri.fixed_target.as_deref(), Some("example.com:443"));
        assert_eq!(uri.rule_suffix.as_deref(), Some("example\\.com$"));
    }

    #[test]
    fn test_chain_split_does_not_split_fixed_target() {
        let chain = parse_pproxy_chain("tunnel://{example.com:443}__socks5://proxy:1080").unwrap();
        assert_eq!(chain.hops.len(), 2);
        assert_eq!(
            chain.hops[0].fixed_target.as_deref(),
            Some("example.com:443")
        );
    }
}
