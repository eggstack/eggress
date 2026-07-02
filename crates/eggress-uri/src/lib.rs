use std::fmt;

use serde::{Deserialize, Serialize};

/// Specification for a proxy chain (one or more hops).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyChainSpec {
    /// Ordered list of proxy hops.
    pub hops: Vec<ProxyHopSpec>,
}

/// Specification for a single proxy hop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyHopSpec {
    /// Supported protocols for this hop.
    pub protocols: Vec<ProtocolSpec>,
    /// Endpoint address.
    pub endpoint: EndpointSpec,
    /// Optional credentials.
    #[serde(default)]
    pub credentials: Option<CredentialSpec>,
    /// Optional routing rule.
    #[serde(default)]
    pub rule: Option<String>,
    /// Optional local bind address.
    #[serde(default)]
    pub local_bind: Option<String>,
    /// Whether to wrap this hop in TLS.
    #[serde(default)]
    pub tls: bool,
    /// Optional SNI override for TLS (defaults to endpoint host).
    #[serde(default)]
    pub server_name: Option<String>,
}

/// Supported proxy protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolSpec {
    Http,
    Socks4,
    Socks5,
    Shadowsocks,
    Trojan,
    Http2,
    WebSocket,
    Raw,
}

/// Endpoint address specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointSpec {
    pub host: String,
    pub port: u16,
}

/// Credential specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialSpec {
    pub username: String,
    pub password: String,
}

/// Errors that can occur during URI parsing.
#[derive(Debug, thiserror::Error)]
pub enum UriParseError {
    #[error("invalid URI format: {message}")]
    InvalidFormat {
        message: String,
        span: Option<usize>,
    },
    #[error("unsupported protocol: {0}")]
    UnsupportedProtocol(String),
    #[error("missing host")]
    MissingHost,
    #[error("invalid port: {0}")]
    InvalidPort(String),
    #[error("empty host not allowed")]
    EmptyHost,
    #[error("duplicate hop separator")]
    DuplicateHopSeparator,
}

/// A redacted display wrapper that hides credentials.
pub struct RedactedUri<'a> {
    chain: &'a ProxyChainSpec,
}

impl<'a> RedactedUri<'a> {
    pub fn new(chain: &'a ProxyChainSpec) -> Self {
        Self { chain }
    }
}

impl<'a> fmt::Display for RedactedUri<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hops: Vec<String> = self
            .chain
            .hops
            .iter()
            .map(|hop| {
                let mut proto_parts: Vec<&str> = hop
                    .protocols
                    .iter()
                    .map(|p| match p {
                        ProtocolSpec::Http => "http",
                        ProtocolSpec::Socks4 => "socks4",
                        ProtocolSpec::Socks5 => "socks5",
                        ProtocolSpec::Shadowsocks => "shadowsocks",
                        ProtocolSpec::Trojan => "trojan",
                        ProtocolSpec::Http2 => "h2",
                        ProtocolSpec::WebSocket => "ws",
                        ProtocolSpec::Raw => "raw",
                    })
                    .collect();
                if hop.tls {
                    proto_parts.push("tls");
                }
                let proto_str = proto_parts.join("+");

                let endpoint_str = if hop.endpoint.host.contains(':') {
                    format!("[{}]:{}", hop.endpoint.host, hop.endpoint.port)
                } else {
                    format!("{}:{}", hop.endpoint.host, hop.endpoint.port)
                };

                let cred_str = if hop.credentials.is_some() {
                    "****:****@"
                } else {
                    ""
                };

                let rule_str = match &hop.rule {
                    Some(rule) => format!("?rule={}", rule),
                    None => String::new(),
                };

                let bind_str = match &hop.local_bind {
                    Some(bind) => format!("@{}", bind),
                    None => String::new(),
                };

                format!(
                    "{}://{}{}{}{}",
                    proto_str, cred_str, endpoint_str, rule_str, bind_str
                )
            })
            .collect();

        write!(f, "{}", hops.join("__"))
    }
}

/// Parse a proxy chain URI string into a chain specification.
///
/// Grammar:
/// - Protocol lists joined with `+` (e.g., `http+socks4+socks5`)
/// - Proxy hops joined with `__` (e.g., `socks5://hop1:1080__http://hop2:8080`)
/// - Standard URI components: scheme, host, port
/// - Bracketed IPv6 (e.g., `[::1]`)
/// - Credentials in userinfo (e.g., `user:pass@host:port`)
/// - Query parameters for rules (e.g., `?rule=regex`)
/// - Local bind modifier (e.g., `@127.0.0.1`)
pub fn parse_proxy_chain(uri: &str) -> Result<ProxyChainSpec, UriParseError> {
    if uri.is_empty() {
        return Err(UriParseError::InvalidFormat {
            message: "empty URI".to_string(),
            span: None,
        });
    }

    // Split on `__` for hop separator
    let hop_strings = split_hops(uri)?;

    if hop_strings.is_empty() {
        return Err(UriParseError::InvalidFormat {
            message: "no hops found".to_string(),
            span: None,
        });
    }

    let hops: Vec<ProxyHopSpec> = hop_strings
        .iter()
        .enumerate()
        .map(|(i, s)| parse_hop(s, i).map_err(|e| add_hop_context(e, i)))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ProxyChainSpec { hops })
}

fn split_hops(uri: &str) -> Result<Vec<String>, UriParseError> {
    // Split on `__` but not inside brackets or other contexts
    let mut hops = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = uri.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut bracket_depth = 0;

    while i < len {
        if chars[i] == '[' {
            bracket_depth += 1;
            current.push(chars[i]);
        } else if chars[i] == ']' {
            bracket_depth -= 1;
            current.push(chars[i]);
        } else if bracket_depth == 0 && i + 1 < len && chars[i] == '_' && chars[i + 1] == '_' {
            hops.push(current.clone());
            current.clear();
            i += 2;
            continue;
        } else {
            current.push(chars[i]);
        }
        i += 1;
    }

    if !current.is_empty() {
        hops.push(current);
    }

    Ok(hops)
}

fn parse_hop(hop_str: &str, _hop_index: usize) -> Result<ProxyHopSpec, UriParseError> {
    let mut remaining = hop_str;

    // Parse local bind modifier (trailing `@<bind>`)
    // We look for the LAST '@' after "://". If what follows is a bare
    // host or host:port (no scheme), it is treated as a bind address.
    // The earlier '@' (if any) is the credentials separator.
    let local_bind = if let Some(at_pos) = find_last_at_outside_scheme(remaining) {
        let after_at = &remaining[at_pos + 1..];
        let before_at = &remaining[..at_pos];
        if before_at.contains("://") || after_at.contains('/') || after_at.contains('?') {
            // Has scheme prefix, slash, or query after — not a bind
            None
        } else {
            let bind = after_at.to_string();
            remaining = before_at;
            Some(bind)
        }
    } else {
        None
    };

    // Split scheme from the rest
    let (protocols, tls, after_scheme) = if let Some(colon_pos) = remaining.find("://") {
        let scheme_part = &remaining[..colon_pos];
        let rest = &remaining[colon_pos + 3..];
        let (protocols, tls) = parse_protocols(scheme_part)?;
        (protocols, tls, rest)
    } else {
        return Err(UriParseError::InvalidFormat {
            message: "missing scheme (expected protocol://)".to_string(),
            span: None,
        });
    };

    // Check for empty host
    if after_scheme.is_empty() {
        return Err(UriParseError::MissingHost);
    }

    // Split credentials and endpoint+query
    let (credentials, endpoint_and_query) =
        if let Some(at_pos) = find_at_outside_brackets(after_scheme) {
            let userinfo = &after_scheme[..at_pos];
            let rest = &after_scheme[at_pos + 1..];
            let creds = parse_credentials(userinfo, &protocols)?;
            (Some(creds), rest)
        } else {
            (None, after_scheme)
        };

    // Split endpoint from query string
    let (endpoint_str, query_str) = if let Some(q_pos) = endpoint_and_query.find('?') {
        let ep = &endpoint_and_query[..q_pos];
        let q = &endpoint_and_query[q_pos + 1..];
        (ep, Some(q))
    } else {
        (endpoint_and_query, None)
    };

    // Parse endpoint
    let endpoint = parse_endpoint(endpoint_str)?;

    // Parse query parameters
    let rule = parse_query_rule(query_str);

    // Validate port range
    if endpoint.port == 0 {
        return Err(UriParseError::InvalidPort("port cannot be 0".to_string()));
    }

    Ok(ProxyHopSpec {
        protocols,
        endpoint,
        credentials,
        rule,
        local_bind,
        tls,
        server_name: None,
    })
}

fn parse_protocols(scheme: &str) -> Result<(Vec<ProtocolSpec>, bool), UriParseError> {
    let parts: Vec<&str> = scheme.split('+').collect();
    if parts.is_empty() {
        return Err(UriParseError::InvalidFormat {
            message: "empty protocol list".to_string(),
            span: None,
        });
    }

    let mut protocols = Vec::new();
    let mut tls = false;

    for p in &parts {
        match *p {
            "http" => protocols.push(ProtocolSpec::Http),
            "socks4" | "socks4a" => protocols.push(ProtocolSpec::Socks4),
            "socks5" => protocols.push(ProtocolSpec::Socks5),
            "shadowsocks" | "ss" => protocols.push(ProtocolSpec::Shadowsocks),
            "trojan" => protocols.push(ProtocolSpec::Trojan),
            "h2" => protocols.push(ProtocolSpec::Http2),
            "ws" | "wss" => protocols.push(ProtocolSpec::WebSocket),
            "raw" | "tunnel" => protocols.push(ProtocolSpec::Raw),
            "tls" => tls = true,
            _ => return Err(UriParseError::UnsupportedProtocol(p.to_string())),
        }
    }

    if protocols.is_empty() {
        return Err(UriParseError::InvalidFormat {
            message: "no protocol specified".to_string(),
            span: None,
        });
    }

    Ok((protocols, tls))
}

fn parse_endpoint(endpoint: &str) -> Result<EndpointSpec, UriParseError> {
    if endpoint.is_empty() {
        return Err(UriParseError::MissingHost);
    }

    // Handle bracketed IPv6: [::1]:8080
    if endpoint.starts_with('[') {
        let close_bracket = endpoint
            .find(']')
            .ok_or_else(|| UriParseError::InvalidFormat {
                message: "unterminated IPv6 bracket".to_string(),
                span: None,
            })?;

        let host = &endpoint[1..close_bracket];

        let after_bracket = &endpoint[close_bracket + 1..];
        if !after_bracket.starts_with(':') {
            return Err(UriParseError::InvalidFormat {
                message: "expected ':' after IPv6 bracket".to_string(),
                span: None,
            });
        }

        let port_str = &after_bracket[1..];
        let port = parse_port(port_str)?;

        return Ok(EndpointSpec {
            host: host.to_string(),
            port,
        });
    }

    // Regular host:port
    let colon_pos = endpoint
        .rfind(':')
        .ok_or_else(|| UriParseError::InvalidFormat {
            message: "missing port".to_string(),
            span: None,
        })?;

    let host = &endpoint[..colon_pos];
    let port_str = &endpoint[colon_pos + 1..];

    let port = parse_port(port_str)?;

    Ok(EndpointSpec {
        host: host.to_string(),
        port,
    })
}

fn parse_port(port_str: &str) -> Result<u16, UriParseError> {
    if port_str.is_empty() {
        return Err(UriParseError::InvalidPort("empty port".to_string()));
    }

    port_str
        .parse::<u16>()
        .map_err(|e| UriParseError::InvalidPort(format!("{}: {}", port_str, e)))
}

fn parse_credentials(
    userinfo: &str,
    protocols: &[ProtocolSpec],
) -> Result<CredentialSpec, UriParseError> {
    let Some(colon_pos) = userinfo.find(':') else {
        if protocols.contains(&ProtocolSpec::Trojan) && !userinfo.is_empty() {
            return Ok(CredentialSpec {
                username: String::new(),
                password: userinfo.to_string(),
            });
        }

        return Err(UriParseError::InvalidFormat {
            message: "missing ':' in credentials".to_string(),
            span: None,
        });
    };

    let username = userinfo[..colon_pos].to_string();
    let password = userinfo[colon_pos + 1..].to_string();

    if username.is_empty() && password.is_empty() {
        return Err(UriParseError::InvalidFormat {
            message: "empty credentials".to_string(),
            span: None,
        });
    }

    Ok(CredentialSpec { username, password })
}

fn parse_query_rule(query: Option<&str>) -> Option<String> {
    let query = query?;

    // Look for rule=<value> parameter
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

/// Find '@' position that's not inside brackets
fn find_at_outside_brackets(s: &str) -> Option<usize> {
    let mut bracket_depth = 0u32;
    for (i, c) in s.char_indices() {
        match c {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '@' if bracket_depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Find last '@' that's outside brackets and not part of a scheme.
/// Returns the position of the last '@' after `://` that could be the
/// bind separator. The caller must still check whether the part after
/// the '@' looks like a bare address (no colon → bind) vs. a
/// `user:pass` credential pair (contains colon).
fn find_last_at_outside_scheme(s: &str) -> Option<usize> {
    let scheme_end = s.find("://")?;
    let after_scheme = &s[scheme_end + 3..];
    // Find the LAST '@' in the part after ://, outside brackets
    let mut last_at: Option<usize> = None;
    let mut bracket_depth = 0u32;
    for (i, c) in after_scheme.char_indices() {
        match c {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '@' if bracket_depth == 0 => {
                last_at = Some(scheme_end + 3 + i);
            }
            _ => {}
        }
    }
    last_at
}

fn add_hop_context(mut err: UriParseError, hop_index: usize) -> UriParseError {
    if let UriParseError::InvalidFormat {
        ref mut message, ..
    } = err
    {
        *message = format!("hop {}: {}", hop_index, message);
    }
    err
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_uri() {
        assert!(parse_proxy_chain("").is_err());
    }

    #[test]
    fn test_protocol_spec_serialization() {
        let spec = ProtocolSpec::Socks5;
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(json, "\"Socks5\"");
    }

    #[test]
    fn test_simple_http() {
        let result = parse_proxy_chain("http://:8080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Http]);
        assert_eq!(result.hops[0].endpoint.host, "");
        assert_eq!(result.hops[0].endpoint.port, 8080);
        assert!(result.hops[0].credentials.is_none());
    }

    #[test]
    fn test_simple_socks4() {
        let result = parse_proxy_chain("socks4://:1080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Socks4]);
        assert_eq!(result.hops[0].endpoint.port, 1080);
    }

    #[test]
    fn test_simple_socks5() {
        let result = parse_proxy_chain("socks5://:1080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Socks5]);
    }

    #[test]
    fn test_multiple_protocols() {
        let result = parse_proxy_chain("http+socks4+socks5://:8080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(
            result.hops[0].protocols,
            vec![
                ProtocolSpec::Http,
                ProtocolSpec::Socks4,
                ProtocolSpec::Socks5
            ]
        );
    }

    #[test]
    fn test_credentials() {
        let result = parse_proxy_chain("http+socks5://user:pass@:8080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert!(result.hops[0].credentials.is_some());
        let creds = result.hops[0].credentials.as_ref().unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass");
    }

    #[test]
    fn test_trojan_password_only_credentials() {
        let result = parse_proxy_chain("trojan://secret@proxy.example:443").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Trojan]);
        let creds = result.hops[0].credentials.as_ref().unwrap();
        assert_eq!(creds.username, "");
        assert_eq!(creds.password, "secret");
    }

    #[test]
    fn test_password_only_credentials_rejected_for_non_trojan() {
        let err = parse_proxy_chain("http://secret@proxy.example:8080").unwrap_err();
        assert!(matches!(err, UriParseError::InvalidFormat { .. }));
    }

    #[test]
    fn test_named_host() {
        let result = parse_proxy_chain("socks5://proxy.example:1080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].endpoint.host, "proxy.example");
        assert_eq!(result.hops[0].endpoint.port, 1080);
    }

    #[test]
    fn test_two_hops() {
        let result = parse_proxy_chain("socks5://hop1:1080__http://hop2:8080").unwrap();
        assert_eq!(result.hops.len(), 2);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Socks5]);
        assert_eq!(result.hops[0].endpoint.host, "hop1");
        assert_eq!(result.hops[0].endpoint.port, 1080);
        assert_eq!(result.hops[1].protocols, vec![ProtocolSpec::Http]);
        assert_eq!(result.hops[1].endpoint.host, "hop2");
        assert_eq!(result.hops[1].endpoint.port, 8080);
    }

    #[test]
    fn test_ipv6_bracketed() {
        let result = parse_proxy_chain("http://[::1]:8080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].endpoint.host, "::1");
        assert_eq!(result.hops[0].endpoint.port, 8080);
    }

    #[test]
    fn test_ipv6_full() {
        let result = parse_proxy_chain("http://[2001:db8::1]:1080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].endpoint.host, "2001:db8::1");
        assert_eq!(result.hops[0].endpoint.port, 1080);
    }

    #[test]
    fn test_unsupported_protocol() {
        let result = parse_proxy_chain("ftp://host:80");
        assert!(result.is_err());
        match result {
            Err(UriParseError::UnsupportedProtocol(p)) => assert_eq!(p, "ftp"),
            _ => panic!("expected UnsupportedProtocol error"),
        }
    }

    #[test]
    fn test_quic_scheme_rejected_with_structured_diagnostic() {
        let result = parse_proxy_chain("quic://host:443");
        match result {
            Err(UriParseError::UnsupportedProtocol(p)) => assert_eq!(p, "quic"),
            other => panic!("expected UnsupportedProtocol for quic, got {other:?}"),
        }
    }

    #[test]
    fn test_h3_scheme_rejected_with_structured_diagnostic() {
        let result = parse_proxy_chain("h3://host:443");
        match result {
            Err(UriParseError::UnsupportedProtocol(p)) => assert_eq!(p, "h3"),
            other => panic!("expected UnsupportedProtocol for h3, got {other:?}"),
        }
    }

    #[test]
    fn test_missing_scheme() {
        let result = parse_proxy_chain("host:80");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_host_with_port() {
        let result = parse_proxy_chain("http://:80");
        // Empty host with port is allowed (format for binding to all interfaces)
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_port() {
        let result = parse_proxy_chain("http://host:99999");
        assert!(result.is_err());
    }

    #[test]
    fn test_port_zero() {
        let result = parse_proxy_chain("http://host:0");
        assert!(result.is_err());
    }

    #[test]
    fn test_query_rule() {
        let result = parse_proxy_chain("http://host:80?rule=regex").unwrap();
        assert_eq!(result.hops[0].rule.as_deref(), Some("regex"));
    }

    #[test]
    fn test_query_no_rule() {
        let result = parse_proxy_chain("http://host:80?foo=bar").unwrap();
        assert!(result.hops[0].rule.is_none());
    }

    #[test]
    fn test_redacted_display() {
        let spec = ProxyChainSpec {
            hops: vec![ProxyHopSpec {
                protocols: vec![ProtocolSpec::Http],
                endpoint: EndpointSpec {
                    host: "proxy.example".to_string(),
                    port: 8080,
                },
                credentials: Some(CredentialSpec {
                    username: "user".to_string(),
                    password: "secret".to_string(),
                }),
                rule: None,
                local_bind: None,
                tls: false,
                server_name: None,
            }],
        };
        let redacted = RedactedUri::new(&spec);
        let display = format!("{}", redacted);
        assert!(display.contains("****:****@"));
        assert!(!display.contains("secret"));
    }

    #[test]
    fn test_redacted_display_no_creds() {
        let spec = ProxyChainSpec {
            hops: vec![ProxyHopSpec {
                protocols: vec![ProtocolSpec::Socks5],
                endpoint: EndpointSpec {
                    host: "proxy.example".to_string(),
                    port: 1080,
                },
                credentials: None,
                rule: None,
                local_bind: None,
                tls: false,
                server_name: None,
            }],
        };
        let redacted = RedactedUri::new(&spec);
        let display = format!("{}", redacted);
        assert_eq!(display, "socks5://proxy.example:1080");
    }

    #[test]
    fn test_roundtrip_simple() {
        let original = "http://proxy.example:8080";
        let spec = parse_proxy_chain(original).unwrap();
        let redacted = RedactedUri::new(&spec).to_string();
        assert_eq!(redacted, original);
    }

    #[test]
    fn test_roundtrip_multi_hop() {
        let original = "socks5://hop1:1080__http://hop2:8080";
        let spec = parse_proxy_chain(original).unwrap();
        let redacted = RedactedUri::new(&spec).to_string();
        assert_eq!(redacted, original);
    }

    #[test]
    fn test_roundtrip_multi_protocol() {
        let original = "http+socks5://proxy:8080";
        let spec = parse_proxy_chain(original).unwrap();
        let redacted = RedactedUri::new(&spec).to_string();
        assert_eq!(redacted, original);
    }

    #[test]
    fn test_roundtrip_ipv6() {
        let original = "http://[::1]:8080";
        let spec = parse_proxy_chain(original).unwrap();
        let redacted = RedactedUri::new(&spec).to_string();
        assert_eq!(redacted, original);
    }

    #[test]
    fn test_roundtrip_with_rule() {
        let original = "http://proxy:8080?rule=regex";
        let spec = parse_proxy_chain(original).unwrap();
        let redacted = RedactedUri::new(&spec).to_string();
        assert_eq!(redacted, original);
    }

    #[test]
    fn test_complex_multi_hop_with_creds() {
        let original = "socks5://hop1:1080__http://user:pass@hop2:8080";
        let spec = parse_proxy_chain(original).unwrap();
        assert_eq!(spec.hops.len(), 2);
        assert!(spec.hops[1].credentials.is_some());
    }

    #[test]
    fn test_unterminated_bracket() {
        let result = parse_proxy_chain("http://[::1:8080");
        assert!(result.is_err());
    }

    #[test]
    fn test_shadowsocks_scheme() {
        let result =
            parse_proxy_chain("shadowsocks://aes-256-gcm:secret@proxy.example:8388").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Shadowsocks]);
        assert_eq!(result.hops[0].endpoint.host, "proxy.example");
        assert_eq!(result.hops[0].endpoint.port, 8388);
        let creds = result.hops[0].credentials.as_ref().unwrap();
        assert_eq!(creds.username, "aes-256-gcm");
        assert_eq!(creds.password, "secret");
    }

    #[test]
    fn test_shadowsocks_ss_scheme() {
        let result = parse_proxy_chain("ss://aes-128-gcm:pass@host:1080").unwrap();
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Shadowsocks]);
    }

    #[test]
    fn test_shadowsocks_roundtrip() {
        let original = "shadowsocks://aes-256-gcm:secret@proxy.example:8388";
        let spec = parse_proxy_chain(original).unwrap();
        assert_eq!(spec.hops.len(), 1);
        assert_eq!(spec.hops[0].protocols, vec![ProtocolSpec::Shadowsocks]);
        let redacted = RedactedUri::new(&spec).to_string();
        assert!(redacted.starts_with("shadowsocks://"));
        assert!(redacted.contains("****:****@"));
        assert!(redacted.contains("proxy.example:8388"));
    }

    #[test]
    fn test_tls_suffix_parses_to_tls_flag() {
        let result = parse_proxy_chain("socks5+tls://proxy.example:1080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Socks5]);
        assert!(result.hops[0].tls);
        assert_eq!(result.hops[0].endpoint.host, "proxy.example");
        assert_eq!(result.hops[0].endpoint.port, 1080);
    }

    #[test]
    fn test_tls_only_protocol_with_other() {
        let result = parse_proxy_chain("http+tls://proxy.example:443").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Http]);
        assert!(result.hops[0].tls);
    }

    #[test]
    fn test_tls_suffix_roundtrip() {
        let original = "socks5+tls://proxy.example:1080";
        let spec = parse_proxy_chain(original).unwrap();
        let redacted = RedactedUri::new(&spec).to_string();
        assert_eq!(redacted, original);
    }

    #[test]
    fn test_socks4a_scheme() {
        let result = parse_proxy_chain("socks4a://host:1080").unwrap();
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Socks4]);
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_protocol() -> impl Strategy<Value = ProtocolSpec> {
        prop_oneof![
            Just(ProtocolSpec::Http),
            Just(ProtocolSpec::Socks4),
            Just(ProtocolSpec::Socks5),
            Just(ProtocolSpec::Shadowsocks),
            Just(ProtocolSpec::Trojan),
            Just(ProtocolSpec::Http2),
            Just(ProtocolSpec::WebSocket),
            Just(ProtocolSpec::Raw),
        ]
    }

    fn arb_host() -> impl Strategy<Value = String> {
        prop_oneof![
            // Regular hostname
            "[a-z][a-z0-9]{0,15}".prop_map(|s| format!("host-{}", s)),
            // Simple IP-like
            "[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}",
        ]
    }

    fn arb_port() -> impl Strategy<Value = u16> {
        (1u16..65535).boxed()
    }

    fn arb_hop() -> impl Strategy<Value = ProxyHopSpec> {
        (
            prop::collection::vec(arb_protocol(), 1..4),
            arb_host(),
            arb_port(),
            prop::option::of("[a-z]{1,10}".prop_map(|s| (s.clone(), s))),
            prop::option::of("[a-z]{1,10}"),
        )
            .prop_map(|(protocols, host, port, credentials, rule)| ProxyHopSpec {
                protocols,
                endpoint: EndpointSpec { host, port },
                credentials: credentials.map(|(u, p)| CredentialSpec {
                    username: u,
                    password: p,
                }),
                rule,
                local_bind: None,
                tls: false,
                server_name: None,
            })
    }

    fn arb_chain() -> impl Strategy<Value = ProxyChainSpec> {
        prop::collection::vec(arb_hop(), 1..3).prop_map(|hops| ProxyChainSpec { hops })
    }

    proptest! {
        #[test]
        fn test_parse_never_panics(input in ".*{0,100}") {
            let _ = parse_proxy_chain(&input);
        }

        #[test]
        fn test_valid_chain_roundtrips(spec in arb_chain()) {
            let display = RedactedUri::new(&spec).to_string();
            let parsed = parse_proxy_chain(&display);
            prop_assert!(parsed.is_ok(), "Failed to parse: {}", display);
        }

        #[test]
        fn test_hop_separator_split(port in 1u16..65535u16) {
            let input = format!("http://a:{}__http://b:{}", port, port);
            let result = parse_proxy_chain(&input);
            prop_assert!(result.is_ok(), "Failed to parse: {}", input);
        }

        #[test]
        fn test_protocol_separator(port in 1u16..65535u16) {
            let input = format!("http+socks5://a:{}", port);
            let result = parse_proxy_chain(&input);
            prop_assert!(result.is_ok(), "Failed to parse: {}", input);
        }
    }
}
