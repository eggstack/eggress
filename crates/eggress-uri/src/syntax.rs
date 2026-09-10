//! Shared URI lexical / endpoint primitives.
//!
//! These helpers implement syntax operations that are equally valid for the
//! native Eggress grammar and the pproxy compatibility grammar:
//!
//! ```text
//! shared lexical primitives != shared grammar
//! ```
//!
//! The native parser (`parse_proxy_chain`) and the compatibility parser
//! (`eggress-pproxy-compat::uri`) both build on these primitives but apply
//! their own semantic interpretation afterward. In particular:
//!
//! - Chain splitting tracks `[]` (IPv6) and `{}` (compat fixed-target) so a
//!   `__` inside either never splits hops. Empty / duplicate-separator policy
//!   stays with the owning grammar.
//! - Authority parsing is neutral: it splits userinfo / host / port without
//!   percent-decoding, credential validation, or default-port policy.
//! - Redaction funnels through [`crate::redact_proxy_uri`], which itself uses
//!   [`find_userinfo_separator`].
//!
//! Helpers here must not know about routing rules, reverse semantics, plugins,
//! TLS policy, or runtime protocols.

/// Neutral syntax error used by shared helpers.
///
/// Callers map this into their own error type (`UriParseError` natively,
/// `CompatError` in the compatibility crate) so helper reuse does not leak
/// one grammar's diagnostics into the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    /// Human-readable reason.
    pub message: String,
    /// Byte offset when the failure has a stable location.
    pub span: Option<usize>,
}

impl SyntaxError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
        }
    }

    pub fn with_span(message: impl Into<String>, span: usize) -> Self {
        Self {
            message: message.into(),
            span: Some(span),
        }
    }
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SyntaxError {}

/// Find the userinfo separator: the LAST `@` outside `[]` brackets.
///
/// A raw `@` inside a password must not truncate userinfo, and `@` inside
/// bracketed IPv6 is never a separator. Callers that also need to ignore
/// `{}` fixed-target braces should check braces separately; passwords never
/// legitimately contain bare braces outside a fixed target, and both grammars
/// treat `@` inside `[]` identically.
pub fn find_userinfo_separator(s: &str) -> Option<usize> {
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

/// Split `input` at the first `delimiter` outside `[]` and `{}`.
///
/// Returns `(head, None)` when the delimiter never appears at top level.
/// Both bracket kinds are tracked so compat fixed-targets (`{host:port}`)
/// and IPv6 literals never cause a premature split. Native inputs without
/// braces behave exactly as a bracket-only scan.
pub fn split_once_outside_brackets(input: &str, delimiter: char) -> (&str, Option<&str>) {
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

/// Split a chain string on top-level `__` separators.
///
/// Tracks `[]` and `{}` depth and fails closed on unmatched brackets/braces.
/// This is a pure lexical split: it does NOT enforce empty-segment,
/// leading/trailing-separator, or triple-underscore policy. Each grammar
/// applies its own validation afterward so historical diagnostics are
/// preserved.
pub fn split_chain_hops(s: &str) -> Result<Vec<&str>, SyntaxError> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut bracket = 0u32;
    let mut brace = 0u32;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] as char {
            '[' => bracket += 1,
            ']' => {
                if bracket == 0 {
                    return Err(SyntaxError::with_span("unmatched ']'", i));
                }
                bracket -= 1;
            }
            '{' => brace += 1,
            '}' => {
                if brace == 0 {
                    return Err(SyntaxError::with_span("unmatched '}'", i));
                }
                brace -= 1;
            }
            '_' if i + 1 < bytes.len() && bytes[i + 1] == b'_' && bracket == 0 && brace == 0 => {
                result.push(&s[start..i]);
                i += 1;
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if bracket != 0 {
        return Err(SyntaxError::new("unmatched '['"));
    }
    if brace != 0 {
        return Err(SyntaxError::new("unmatched '{'"));
    }
    result.push(&s[start..]);
    Ok(result)
}

/// Neutral host/port split.
///
/// Returns the raw host string plus an optional port. `port_specified` is
/// false when no top-level `:` is present (caller may apply default ports).
/// Validation of empty hosts, port 0, and required ports stays with the
/// owning grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPort {
    pub host: String,
    pub port: Option<u16>,
    pub port_specified: bool,
}

/// Parse `host:port`, `[ipv6]:port`, bare `host`, or empty endpoint.
///
/// Errors are neutral; callers map them to `UriParseError` / `CompatError`.
pub fn parse_host_port(endpoint: &str) -> Result<HostPort, SyntaxError> {
    if endpoint.is_empty() {
        return Ok(HostPort {
            host: String::new(),
            port: None,
            port_specified: false,
        });
    }
    if endpoint.starts_with('[') {
        let close = endpoint
            .find(']')
            .ok_or_else(|| SyntaxError::new("unterminated IPv6 bracket"))?;
        let host = endpoint[1..close].to_string();
        let after = &endpoint[close + 1..];
        if !after.starts_with(':') {
            return Err(SyntaxError::new("expected ':' after IPv6 bracket"));
        }
        let port_str = &after[1..];
        let port = parse_port_str(port_str)?;
        return Ok(HostPort {
            host,
            port: Some(port),
            port_specified: true,
        });
    }
    match endpoint.rfind(':') {
        None => Ok(HostPort {
            host: endpoint.to_string(),
            port: None,
            port_specified: false,
        }),
        Some(colon_pos) => {
            let host = &endpoint[..colon_pos];
            let port_str = &endpoint[colon_pos + 1..];
            if host.contains(':') {
                return Err(SyntaxError::new(format!(
                    "endpoint '{endpoint}' contains multiple ':' separators; \
                     use [ipv6]:port form for IPv6 literals"
                )));
            }
            let port = parse_port_str(port_str)?;
            Ok(HostPort {
                host: host.to_string(),
                port: Some(port),
                port_specified: true,
            })
        }
    }
}

fn parse_port_str(port_str: &str) -> Result<u16, SyntaxError> {
    if port_str.is_empty() {
        return Err(SyntaxError::new("empty port"));
    }
    port_str
        .parse::<u16>()
        .map_err(|e| SyntaxError::new(format!("invalid port '{port_str}': {e}")))
}

/// Split raw userinfo on the FIRST `:`.
///
/// Returns `(user, pass, has_colon)`. No percent-decoding or credential
/// validation is applied here; the native grammar percent-decodes and the
/// compatibility grammar keeps values verbatim.
pub fn split_userinfo(userinfo: &str) -> (String, String, bool) {
    match userinfo.find(':') {
        Some(colon_pos) => (
            userinfo[..colon_pos].to_string(),
            userinfo[colon_pos + 1..].to_string(),
            true,
        ),
        None => (String::new(), userinfo.to_string(), false),
    }
}

/// Format a host back into URI form, bracketing IPv6 literals.
pub fn format_host(host: &str) -> String {
    if host.is_empty() {
        String::new()
    } else if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn userinfo_separator_is_last_at_outside_brackets() {
        assert_eq!(
            find_userinfo_separator("user:p@ss@proxy:8080"),
            Some("user:p@ss".len())
        );
        assert_eq!(find_userinfo_separator("proxy:8080"), None);
        // IPv6 brackets never yield a separator.
        assert_eq!(find_userinfo_separator("[::1]:8080"), None);
    }

    #[test]
    fn chain_split_tracks_brackets_and_braces() {
        let hops = split_chain_hops("a:1__b:2").unwrap();
        assert_eq!(hops, vec!["a:1", "b:2"]);
        // `__` inside braces (compat fixed target) must not split.
        let hops = split_chain_hops("tunnel://{a:1__b:2}").unwrap();
        assert_eq!(hops.len(), 1);
        assert!(split_chain_hops("http://[::1:8080").is_err());
        assert!(split_chain_hops("]__http://a:1").is_err());
    }

    #[test]
    fn host_port_parses_shared_cases() {
        let hp = parse_host_port("proxy.example:8080").unwrap();
        assert_eq!(hp.host, "proxy.example");
        assert_eq!(hp.port, Some(8080));
        assert!(hp.port_specified);

        let hp = parse_host_port("[::1]:8080").unwrap();
        assert_eq!(hp.host, "::1");
        assert_eq!(hp.port, Some(8080));

        let hp = parse_host_port("bare-host").unwrap();
        assert_eq!(hp.host, "bare-host");
        assert_eq!(hp.port, None);
        assert!(!hp.port_specified);

        assert!(parse_host_port("::1:8080").is_err());
        assert!(parse_host_port("host:notaport").is_err());
    }

    #[test]
    fn userinfo_split_is_first_colon_without_decoding() {
        let (u, p, has) = split_userinfo("user:p%40ss");
        assert_eq!((u.as_str(), p.as_str(), has), ("user", "p%40ss", true));
        let (u, p, has) = split_userinfo("justpassword");
        assert_eq!((u.as_str(), p.as_str(), has), ("", "justpassword", false));
    }

    #[test]
    fn host_formatting_brackets_ipv6() {
        assert_eq!(format_host("proxy"), "proxy");
        assert_eq!(format_host("::1"), "[::1]");
        assert_eq!(format_host(""), "");
    }
}
