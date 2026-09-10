use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub mod syntax;

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
    /// Explicit certificate bypass for compatibility transports.
    #[serde(default)]
    pub insecure: bool,
    /// Ordered pproxy SSR plugin names.
    #[serde(default)]
    pub plugins: Vec<String>,
    /// Optional pproxy SSR user/auth prefix from the URI fragment.
    #[serde(default)]
    pub auth_prefix: Option<String>,
}

/// Supported proxy protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolSpec {
    Http,
    HttpOnly,
    Socks4,
    Socks5,
    Shadowsocks,
    ShadowsocksR,
    Trojan,
    Http2,
    Http3,
    Quic,
    WebSocket,
    Raw,
    Ssh,
    Unix,
}

impl ProtocolSpec {
    /// Canonical URI name for this protocol (matches redacted display).
    ///
    /// This is the single native recognition point: [`ProtocolSpec::parse_name`]
    /// inverts this mapping plus explicit aliases. Compatibility parsing
    /// delegates native-capable tokens here and keeps only pproxy-specific
    /// pseudo-protocols (`bind`, `listen`, `backward`, `rebind`, `direct`,
    /// `redir`, `echo`, `https`, ...) in its own small table.
    pub fn canonical_name(self) -> &'static str {
        match self {
            ProtocolSpec::Http => "http",
            ProtocolSpec::HttpOnly => "httponly",
            ProtocolSpec::Socks4 => "socks4",
            ProtocolSpec::Socks5 => "socks5",
            ProtocolSpec::Shadowsocks => "shadowsocks",
            ProtocolSpec::ShadowsocksR => "ssr",
            ProtocolSpec::Trojan => "trojan",
            ProtocolSpec::Http2 => "h2",
            ProtocolSpec::Http3 => "h3",
            ProtocolSpec::Quic => "quic",
            ProtocolSpec::WebSocket => "ws",
            ProtocolSpec::Raw => "raw",
            ProtocolSpec::Ssh => "ssh",
            ProtocolSpec::Unix => "unix",
        }
    }

    /// Parse one native protocol token, including explicit aliases.
    ///
    /// Accepted aliases (tested):
    /// `socks4a` -> `Socks4`, `ss` -> `Shadowsocks`, `wss` -> `WebSocket`,
    /// `tunnel` -> `Raw`. Transport modifier `tls` is NOT a protocol and
    /// returns `None` so callers handle it separately. Compatibility-only
    /// names (`https`, `direct`, `redir`, `echo`, `bind`, `listen`,
    /// `backward`, `rebind`, `websocket`, `secure`, `in`, ...) intentionally
    /// return `None`.
    pub fn parse_name(name: &str) -> Option<Self> {
        match name {
            "http" => Some(ProtocolSpec::Http),
            "httponly" => Some(ProtocolSpec::HttpOnly),
            "socks4" | "socks4a" => Some(ProtocolSpec::Socks4),
            "socks5" => Some(ProtocolSpec::Socks5),
            "shadowsocks" | "ss" => Some(ProtocolSpec::Shadowsocks),
            "ssr" => Some(ProtocolSpec::ShadowsocksR),
            "trojan" => Some(ProtocolSpec::Trojan),
            "h2" => Some(ProtocolSpec::Http2),
            "h3" => Some(ProtocolSpec::Http3),
            "quic" => Some(ProtocolSpec::Quic),
            "ws" | "wss" => Some(ProtocolSpec::WebSocket),
            "raw" | "tunnel" => Some(ProtocolSpec::Raw),
            "ssh" => Some(ProtocolSpec::Ssh),
            "unix" => Some(ProtocolSpec::Unix),
            _ => None,
        }
    }

    /// All native variants in canonical order (for exhaustive disposition tests).
    pub fn all_variants() -> &'static [ProtocolSpec] {
        &[
            ProtocolSpec::Http,
            ProtocolSpec::HttpOnly,
            ProtocolSpec::Socks4,
            ProtocolSpec::Socks5,
            ProtocolSpec::Shadowsocks,
            ProtocolSpec::ShadowsocksR,
            ProtocolSpec::Trojan,
            ProtocolSpec::Http2,
            ProtocolSpec::Http3,
            ProtocolSpec::Quic,
            ProtocolSpec::WebSocket,
            ProtocolSpec::Raw,
            ProtocolSpec::Ssh,
            ProtocolSpec::Unix,
        ]
    }
}

impl FromStr for ProtocolSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ProtocolSpec::parse_name(s).ok_or_else(|| format!("unsupported protocol: {s}"))
    }
}

impl fmt::Display for ProtocolSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.canonical_name())
    }
}

/// Endpoint address specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointSpec {
    pub host: String,
    pub port: u16,
}

/// Credential specification.
#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct CredentialSpec {
    pub username: String,
    pub password: String,
}

impl Serialize for CredentialSpec {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Never emit the plaintext password through serialization: any
        // diagnostic or audit path that JSON-encodes a parsed chain must
        // stay credential-free, mirroring the redacting `Debug` impl.
        use serde::ser::SerializeStruct;
        const PASSWORD_PLACEHOLDER: &str = "****";
        let mut state = serializer.serialize_struct("CredentialSpec", 2)?;
        state.serialize_field("username", &self.username)?;
        state.serialize_field("password", PASSWORD_PLACEHOLDER)?;
        state.end()
    }
}

impl fmt::Debug for CredentialSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never emit the plaintext password through Debug (logging, panics,
        // assertion messages); mirror the redacting `Display` behavior.
        f.debug_struct("CredentialSpec")
            .field("username", &self.username)
            .field("password", &"****")
            .finish()
    }
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

/// Redact userinfo from an arbitrary proxy URI while preserving its endpoint.
///
/// This is intentionally tolerant of URI forms that are not parseable as a
/// native [`ProxyChainSpec`], so callers can safely use it for diagnostics.
/// It is the single canonical tolerant credential-hiding primitive:
/// [`syntax::find_userinfo_separator`] owns `@` detection for both native
/// and compatibility diagnostics.
pub fn redact_proxy_uri(uri: &str) -> String {
    let Some(scheme_end) = uri.find("://") else {
        // No `://` — still redact `user:pass@host` style credentials so
        // raw `user:pass@host` or base64-decoded blobs never leak when
        // callers fallback via `unwrap_or_else(|_| redact_proxy_uri(uri))`.
        if let Some(at_pos) = syntax::find_userinfo_separator(uri) {
            if uri[..at_pos].contains(':') {
                return format!("****@{}", &uri[at_pos + 1..]);
            }
            // Conservative: any `@` outside brackets may carry userinfo
            return format!("****@{}", &uri[at_pos + 1..]);
        }
        return uri.to_string();
    };
    let after_scheme = &uri[scheme_end + 3..];
    match syntax::find_userinfo_separator(after_scheme) {
        Some(at_pos) => format!(
            "{}****@{}",
            &uri[..scheme_end + 3],
            &after_scheme[at_pos + 1..]
        ),
        None => uri.to_string(),
    }
}

impl<'a> RedactedUri<'a> {
    pub fn new(chain: &'a ProxyChainSpec) -> Self {
        Self { chain }
    }
}

impl fmt::Display for RedactedUri<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hops: Vec<String> = self
            .chain
            .hops
            .iter()
            .map(|hop| {
                let mut proto_parts: Vec<&str> =
                    hop.protocols.iter().map(|p| p.canonical_name()).collect();
                if hop.tls {
                    proto_parts.push("tls");
                }
                let proto_str = proto_parts.join("+");

                let endpoint_str = format!(
                    "{}:{}",
                    syntax::format_host(&hop.endpoint.host),
                    hop.endpoint.port
                );

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
    // Shared lexical split tracks `[]`/`{}` and fails closed on unmatched
    // brackets. Triple-underscore policy stays native: any `___` run outside
    // brackets is `DuplicateHopSeparator`, preserving historical diagnostics.
    if has_triple_underscore_outside_brackets(uri) {
        return Err(UriParseError::DuplicateHopSeparator);
    }
    let hops = syntax::split_chain_hops(uri).map_err(|e| UriParseError::InvalidFormat {
        message: e.message,
        span: e.span,
    })?;
    Ok(hops.into_iter().map(str::to_string).collect())
}

/// Detect `___` (or longer) outside `[]` without splitting.
///
/// Bracket-only on purpose: this preserves the historical native split policy
/// exactly (braces were never tracked here). The shared splitter below also
/// tracks `{}` so compat fixed-targets never split; brace-containing native
/// inputs still fail closed, only via endpoint validation instead.
fn has_triple_underscore_outside_brackets(s: &str) -> bool {
    let mut bracket = 0u32;
    let bytes = s.as_bytes();
    let mut run = 0u32;
    for &b in bytes {
        match b as char {
            '[' => {
                bracket += 1;
                run = 0;
            }
            ']' => {
                bracket = bracket.saturating_sub(1);
                run = 0;
            }
            '_' if bracket == 0 => {
                run += 1;
                if run >= 3 {
                    return true;
                }
            }
            _ => run = 0,
        }
    }
    false
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
        let scheme_end = before_at.find("://").unwrap_or(0);
        let before_endpoint = &before_at[scheme_end + 3..];
        let base_endpoint = before_endpoint
            .split_once('?')
            .map_or(before_endpoint, |(endpoint, _)| endpoint);
        let has_earlier_userinfo = before_endpoint.contains('@');
        // Small per-hop speculative parse (1-3 hops typical); double parse is
        // bounded and keeps the @-heuristic without extra allocation. B-06.
        let base_is_endpoint = parse_endpoint(base_endpoint).is_ok();
        let trailing_bind_has_endpoint = before_endpoint
            .rsplit_once('@')
            .map(|(_, candidate)| {
                parse_endpoint(candidate.split_once('?').map_or(candidate, |(ep, _)| ep)).is_ok()
            })
            .unwrap_or(false);
        let is_trailing_bind = if has_earlier_userinfo {
            trailing_bind_has_endpoint
        } else {
            base_is_endpoint
        };
        let has_userinfo = !is_trailing_bind;
        // pproxy @<bind> only accepts IP/socket literals (B-06); hostname
        // binds are intentionally rejected here.
        let bind_is_address = after_at.parse::<std::net::IpAddr>().is_ok()
            || after_at.parse::<std::net::SocketAddr>().is_ok();
        if has_userinfo || after_at.contains('/') || after_at.contains('?') || !bind_is_address {
            // A credential separator, path/query suffix, or non-address @ is
            // not the native trailing local-bind modifier.
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

    let (after_scheme, auth_prefix) = after_scheme
        .split_once('#')
        .map_or((after_scheme, None), |(value, auth)| {
            (value, Some(auth.to_string()))
        });

    // Check for empty host
    if after_scheme.is_empty() {
        return Err(UriParseError::MissingHost);
    }

    if protocols == [ProtocolSpec::Unix] {
        return Ok(ProxyHopSpec {
            protocols,
            endpoint: EndpointSpec {
                host: after_scheme.to_string(),
                port: 0,
            },
            credentials: None,
            rule: None,
            local_bind,
            tls,
            server_name: None,
            insecure: false,
            plugins: Vec::new(),
            auth_prefix,
        });
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

    let (endpoint_and_query, plugin_path) = endpoint_and_query
        .split_once('/')
        .map_or((endpoint_and_query, None), |(endpoint, path)| {
            (endpoint, Some(path))
        });
    let plugins = plugin_path
        .unwrap_or_default()
        .trim_start_matches(',')
        .split(',')
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect();

    // Split endpoint from query string
    let (endpoint_str, query_str) = if let Some(q_pos) = endpoint_and_query.find('?') {
        let ep = &endpoint_and_query[..q_pos];
        let q = &endpoint_and_query[q_pos + 1..];
        (ep, Some(q))
    } else {
        (endpoint_and_query, None)
    };

    // pproxy defaults SSH's endpoint port to 22. Keep the native shorthand
    // aligned with that compatibility form while retaining strict host:port
    // parsing for every other protocol.
    let endpoint = if protocols == [ProtocolSpec::Ssh]
        && !endpoint_str.starts_with('[')
        && !endpoint_str.contains(':')
    {
        EndpointSpec {
            host: endpoint_str.to_string(),
            port: 22,
        }
    } else {
        parse_endpoint(endpoint_str)?
    };

    // Empty hosts are valid for listener bind addresses, but proxy-chain hops
    // are outbound endpoints and cannot be executed without a host.
    if endpoint.host.is_empty() {
        return Err(UriParseError::EmptyHost);
    }

    // Parse query parameters
    let rule = parse_query_rule(query_str);
    let insecure = query_str.is_some_and(|query| {
        query
            .split('&')
            .any(|param| param == "insecure" || param == "insecure=true")
    });

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
        insecure,
        plugins,
        auth_prefix,
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
        // Canonical native recognition lives in `ProtocolSpec::parse_name`;
        // `tls` remains a transport modifier, not a protocol.
        if *p == "tls" {
            tls = true;
            continue;
        }
        match ProtocolSpec::parse_name(p) {
            Some(spec) => protocols.push(spec),
            None => return Err(UriParseError::UnsupportedProtocol(p.to_string())),
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
    // Shared lexical split: bracketed IPv6, host:port, bare host.
    // Port-required / empty-host / port-zero policy stays native below.
    let parsed = syntax::parse_host_port(endpoint).map_err(|e| {
        let msg = e.message.clone();
        if msg.contains("port") || msg.contains("Port") {
            // Preserve historical variant: missing/invalid ports surface as
            // `InvalidPort` except bare "missing port" which was InvalidFormat.
            if msg == "missing port" {
                UriParseError::InvalidFormat {
                    message: msg,
                    span: e.span,
                }
            } else if msg.starts_with("invalid port") || msg == "empty port" {
                UriParseError::InvalidPort(msg)
            } else {
                UriParseError::InvalidFormat {
                    message: msg,
                    span: e.span,
                }
            }
        } else {
            UriParseError::InvalidFormat {
                message: msg,
                span: e.span,
            }
        }
    })?;
    let Some(port) = parsed.port else {
        return Err(UriParseError::InvalidFormat {
            message: "missing port".to_string(),
            span: None,
        });
    };
    Ok(EndpointSpec {
        host: parsed.host,
        port,
    })
}

fn parse_credentials(
    userinfo: &str,
    protocols: &[ProtocolSpec],
) -> Result<CredentialSpec, UriParseError> {
    // Shared split on first `:`; percent-decoding and Trojan password-only
    // policy remain native semantics.
    let (raw_user, raw_pass, has_colon) = syntax::split_userinfo(userinfo);
    if !has_colon {
        if protocols.contains(&ProtocolSpec::Trojan) && !userinfo.is_empty() {
            return Ok(CredentialSpec {
                username: String::new(),
                password: percent_decode(userinfo)?,
            });
        }

        return Err(UriParseError::InvalidFormat {
            message: "missing ':' in credentials".to_string(),
            span: None,
        });
    }

    let username = percent_decode(&raw_user)?;
    let password = percent_decode(&raw_pass)?;

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

/// Percent-decode `input`, tolerating invalid `%` escapes as literals.
///
/// Invalid escapes (e.g. `%ZZ` or trailing `%`) are kept verbatim rather
/// than rejected. This intentionally mirrors Python `pproxy`/`urllib.parse.unquote`
/// behaviour so credential round-trips remain compatible; strict rejection
/// would cause auth mismatches for passwords containing literal `%`.
fn percent_decode(input: &str) -> Result<String, UriParseError> {
    let mut result = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_val(bytes[i + 1]);
            let lo = hex_val(bytes[i + 2]);
            if let (Some(h), Some(l)) = (hi, lo) {
                result.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    let decoded = String::from_utf8(result).map_err(|_| UriParseError::InvalidFormat {
        message: "invalid UTF-8 in percent-encoded sequence".to_string(),
        span: None,
    })?;
    if decoded.contains('\0') {
        return Err(UriParseError::InvalidFormat {
            message: "NUL byte in percent-decoded credentials".to_string(),
            span: None,
        });
    }
    Ok(decoded)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Find the position of the LAST `@` that's not inside brackets.
/// The userinfo separator is the last unbracketed `@` after the
/// Shared userinfo-separator primitive (last `@` outside `[]`).
///
/// Thin wrapper over [`syntax::find_userinfo_separator`] so native call sites
/// no longer own an independent scan.
fn find_at_outside_brackets(s: &str) -> Option<usize> {
    syntax::find_userinfo_separator(s)
}

/// Find last '@' that's outside brackets and not part of a scheme.
/// Returns the position of the last '@' after `://` that could be the
/// bind separator. The caller must still check whether the part after
/// the '@' looks like a bare address (no colon → bind) vs. a
/// `user:pass` credential pair (contains colon).
fn find_last_at_outside_scheme(s: &str) -> Option<usize> {
    let scheme_end = s.find("://")?;
    let after_scheme = &s[scheme_end + 3..];
    syntax::find_userinfo_separator(after_scheme).map(|pos| scheme_end + 3 + pos)
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
    fn test_credential_serialization_redacts_password() {
        let creds = CredentialSpec {
            username: "alice".to_string(),
            password: "hunter2".to_string(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        assert!(json.contains("alice"), "username should survive: {json}");
        assert!(
            !json.contains("hunter2"),
            "plaintext password must never be serialized: {json}"
        );
        assert!(json.contains("****"), "password should be redacted: {json}");
    }

    #[test]
    fn test_redact_proxy_uri_handles_ipv6_and_at_signs() {
        assert_eq!(
            redact_proxy_uri("http://user:p@ss@[::1]:8080"),
            "http://****@[::1]:8080"
        );
    }

    #[test]
    fn test_simple_http() {
        let result = parse_proxy_chain("http://host:8080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Http]);
        assert_eq!(result.hops[0].endpoint.host, "host");
        assert_eq!(result.hops[0].endpoint.port, 8080);
        assert!(result.hops[0].credentials.is_none());
    }

    #[test]
    fn test_simple_socks4() {
        let result = parse_proxy_chain("socks4://host:1080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Socks4]);
        assert_eq!(result.hops[0].endpoint.port, 1080);
    }

    #[test]
    fn test_simple_socks5() {
        let result = parse_proxy_chain("socks5://host:1080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Socks5]);
    }

    #[test]
    fn test_multiple_protocols() {
        let result = parse_proxy_chain("http+socks4+socks5://host:8080").unwrap();
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
        let result = parse_proxy_chain("http+socks5://user:pass@host:8080").unwrap();
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
    fn test_percent_decoded_at_in_password() {
        let result = parse_proxy_chain("http://user:p%40ss@proxy:8080").unwrap();
        assert_eq!(result.hops.len(), 1);
        let creds = result.hops[0].credentials.as_ref().unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "p@ss");
    }

    #[test]
    fn test_percent_decoded_colon_in_password() {
        let result = parse_proxy_chain("http://user:pass%3Aword@proxy:8080").unwrap();
        assert_eq!(result.hops.len(), 1);
        let creds = result.hops[0].credentials.as_ref().unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass:word");
    }

    #[test]
    fn test_percent_decoded_at_in_username() {
        let result = parse_proxy_chain("http://user%40name:pass@proxy:8080").unwrap();
        assert_eq!(result.hops.len(), 1);
        let creds = result.hops[0].credentials.as_ref().unwrap();
        assert_eq!(creds.username, "user@name");
        assert_eq!(creds.password, "pass");
    }

    #[test]
    fn test_percent_decoded_utf8_credentials() {
        let result = parse_proxy_chain("http://user:p%C3%A9ss@proxy:8080").unwrap();
        let creds = result.hops[0].credentials.as_ref().unwrap();
        assert_eq!(creds.password, "péss");
    }

    #[test]
    fn test_named_host() {
        let result = parse_proxy_chain("socks5://proxy.example:1080").unwrap();
        assert_eq!(result.hops.len(), 1);
        assert_eq!(result.hops[0].endpoint.host, "proxy.example");
        assert_eq!(result.hops[0].endpoint.port, 1080);
    }

    #[test]
    fn test_ssh_defaults_to_port_22() {
        let result = parse_proxy_chain("ssh://user:pass@proxy.example").unwrap();
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Ssh]);
        assert_eq!(result.hops[0].endpoint.host, "proxy.example");
        assert_eq!(result.hops[0].endpoint.port, 22);
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
    fn test_triple_hop_separator_is_rejected() {
        assert!(matches!(
            parse_proxy_chain("socks5://a:1080___http://b:8080"),
            Err(UriParseError::DuplicateHopSeparator)
        ));
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
    fn test_quic_scheme_is_accepted() {
        let result = parse_proxy_chain("quic+http://host:443").unwrap();
        assert_eq!(
            result.hops[0].protocols,
            vec![ProtocolSpec::Quic, ProtocolSpec::Http]
        );
    }

    #[test]
    fn test_h3_scheme_is_accepted() {
        let result = parse_proxy_chain("h3://host:443").unwrap();
        assert_eq!(result.hops[0].protocols, vec![ProtocolSpec::Http3]);
    }

    #[test]
    fn test_missing_scheme() {
        let result = parse_proxy_chain("host:80");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_host_with_port_is_rejected() {
        let result = parse_proxy_chain("http://:80");
        assert!(matches!(result, Err(UriParseError::EmptyHost)));
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
                insecure: false,
                plugins: Vec::new(),
                auth_prefix: None,
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
                insecure: false,
                plugins: Vec::new(),
                auth_prefix: None,
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
    fn test_mismatched_brackets_are_rejected() {
        // A stray ']' must not silently shift the hop-splitting grammar.
        assert!(parse_proxy_chain("]__http://host:80").is_err());
        assert!(parse_proxy_chain("http://]host:80").is_err());
    }

    #[test]
    fn test_credential_debug_is_redacted() {
        let creds = CredentialSpec {
            username: "alice".to_string(),
            password: "s3cret".to_string(),
        };
        let rendered = format!("{:?}", creds);
        assert!(rendered.contains("alice"));
        assert!(
            !rendered.contains("s3cret"),
            "debug leaked password: {rendered}"
        );
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

    #[test]
    fn test_password_containing_at_sign() {
        // Regression: a raw '@' inside the password must not be treated
        // as the userinfo/host separator. The userinfo separator is the
        // LAST unbracketed '@' after the scheme.
        let result = parse_proxy_chain("http://admin:s3cret_p@ssw0rd@proxy:8080").unwrap();
        let creds = result.hops[0].credentials.as_ref().unwrap();
        assert_eq!(creds.username, "admin");
        assert_eq!(creds.password, "s3cret_p@ssw0rd");
        assert_eq!(result.hops[0].endpoint.host, "proxy");
        assert_eq!(result.hops[0].endpoint.port, 8080);
    }

    #[test]
    fn test_password_containing_at_sign_redacted() {
        // Regression: the redacted display must not leak any part of a
        // password that contains '@'.
        let result = parse_proxy_chain("http://admin:s3cret_p@ssw0rd@proxy:8080").unwrap();
        let redacted = RedactedUri::new(&result).to_string();
        assert_eq!(redacted, "http://****:****@proxy:8080");
        assert!(!redacted.contains("s3cret_p"));
        assert!(!redacted.contains("ssw0rd"));
    }

    #[test]
    fn test_password_containing_at_sign_ipv6_endpoint() {
        // Regression: bracketed IPv6 must still allow '@' inside the
        // userinfo without being confused for an endpoint '@'.
        let result = parse_proxy_chain("http://user:p@ss@[::1]:8080").unwrap();
        let creds = result.hops[0].credentials.as_ref().unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "p@ss");
        assert_eq!(result.hops[0].endpoint.host, "::1");
        assert_eq!(result.hops[0].endpoint.port, 8080);
    }

    #[test]
    fn test_protocol_canonical_names_roundtrip() {
        for spec in ProtocolSpec::all_variants() {
            let name = spec.canonical_name();
            assert_eq!(
                ProtocolSpec::parse_name(name),
                Some(*spec),
                "canonical name '{name}' must parse back"
            );
            assert_eq!(
                spec.to_string(),
                name,
                "Display must equal canonical name for {spec:?}"
            );
            assert_eq!(
                (*spec).to_string().parse::<ProtocolSpec>(),
                Ok(*spec),
                "FromStr must roundtrip for {spec:?}"
            );
        }
    }

    #[test]
    fn test_protocol_aliases_are_explicit() {
        assert_eq!(
            ProtocolSpec::parse_name("socks4a"),
            Some(ProtocolSpec::Socks4)
        );
        assert_eq!(
            ProtocolSpec::parse_name("ss"),
            Some(ProtocolSpec::Shadowsocks)
        );
        assert_eq!(
            ProtocolSpec::parse_name("wss"),
            Some(ProtocolSpec::WebSocket)
        );
        assert_eq!(ProtocolSpec::parse_name("tunnel"), Some(ProtocolSpec::Raw));
        // `tls` is a modifier, not a protocol.
        assert_eq!(ProtocolSpec::parse_name("tls"), None);
        // Compatibility-only pseudo-protocols stay out of the native enum.
        for compat_only in [
            "https",
            "direct",
            "redir",
            "echo",
            "bind",
            "listen",
            "backward",
            "rebind",
            "secure",
            "in",
            "websocket",
        ] {
            assert_eq!(
                ProtocolSpec::parse_name(compat_only),
                None,
                "'{compat_only}' must remain compatibility-owned"
            );
        }
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
                insecure: false,
                plugins: Vec::new(),
                auth_prefix: None,
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
