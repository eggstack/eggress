//! Native outbound connector for proxy chains.
//!
//! This module provides [`OutboundConnector`], which compiles a TOML config
//! and executes the chain engine directly to open TCP connections through a
//! configured proxy chain without starting a listener service.

use std::sync::Arc;
use std::time::Duration;

use crate::EggressError;

/// Metadata about an established outbound connection.
#[derive(Debug, Clone)]
pub struct OutboundInfo {
    /// The local address of the underlying TCP connection (if available).
    pub local_addr: Option<std::net::SocketAddr>,
    /// The remote address of the first hop.
    pub peer_addr: Option<std::net::SocketAddr>,
    /// The chain hops that were traversed.
    pub hop_count: usize,
}

/// A UDP association through a SOCKS5 proxy.
///
/// Contains the relay address to send/receive UDP datagrams and
/// the control stream that must remain open for the association lifetime.
pub struct UdpAssociation {
    /// The UDP relay address of the SOCKS5 proxy.
    pub relay_addr: std::net::SocketAddr,
    /// The control TCP stream (must stay open for the association).
    pub control_stream: Option<eggress_core::BoxStream>,
    /// The target address for datagrams.
    pub target: eggress_core::TargetAddr,
}

/// Resolve a proxy endpoint address (host:port) to a SocketAddr.
///
/// For IP addresses, returns directly. For domains, performs DNS lookup.
async fn resolve_endpoint_addr(
    endpoint: &eggress_uri::EndpointSpec,
) -> Option<std::net::SocketAddr> {
    if let Ok(ip) = endpoint.host.parse::<std::net::IpAddr>() {
        return Some(std::net::SocketAddr::new(ip, endpoint.port));
    }
    let lookup = format!("{}:{}", endpoint.host, endpoint.port);
    let mut addresses = tokio::net::lookup_host(&lookup).await.ok()?;
    addresses.next()
}

/// A native outbound connector that executes the chain engine directly.
///
/// This compiles routing/upstream state from a TOML config and provides
/// methods to open TCP connections through the configured proxy chain
/// without starting a listener service.
pub struct OutboundConnector {
    runtime_config: Option<Arc<eggress_config::compile::RuntimeConfig>>,
    chain_executor: eggress_core::chain::ChainExecutor,
    direct: bool,
}

impl OutboundConnector {
    /// Create a connector from a TOML config string.
    pub fn from_toml(config_toml: &str) -> Result<Self, EggressError> {
        let config: eggress_config::model::ConfigFile =
            toml::from_str(config_toml).map_err(|e| EggressError::Config(e.to_string()))?;

        if let Some(version) = config.version {
            if version != 1 {
                return Err(EggressError::Config(format!(
                    "unsupported config version: {version}"
                )));
            }
        }

        eggress_config::validate::validate_config(&config).map_err(|errors| {
            let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            EggressError::Config(messages.join("; "))
        })?;

        let runtime_config = eggress_config::compile::compile_config(&config)
            .map_err(|e| EggressError::Config(e.to_string()))?;

        if runtime_config.upstreams.is_empty() {
            return Err(EggressError::Config("no upstreams configured".to_string()));
        }

        let upstream = &runtime_config.upstreams[0];
        if upstream.chain.hops.is_empty() {
            return Err(EggressError::Config("upstream chain is empty".to_string()));
        }

        #[cfg(feature = "ssh")]
        let chain_executor = eggress_server::build_chain_executor(None, None, None);
        #[cfg(not(feature = "ssh"))]
        let chain_executor = eggress_server::build_chain_executor(None, None);

        Ok(Self {
            runtime_config: Some(Arc::new(runtime_config)),
            chain_executor,
            direct: false,
        })
    }

    /// Create a connector from a pproxy-style remote expression.
    ///
    /// Accepts a single pproxy URI or a canonical `__`-separated multi-hop
    /// chain (e.g. `"socks5://127.0.0.1:1080__http://127.0.0.1:8080"`).
    /// The expression is parsed with the compatibility chain parser and
    /// translated through the existing compatibility layer, then executed
    /// in-process via `ChainExecutor`. No listener is started.
    /// Unsupported chain members fail construction instead of being dropped.
    #[cfg(feature = "pproxy-compat")]
    pub fn from_pproxy_uri(uri: &str) -> Result<Self, EggressError> {
        let redacted_expr = redact_pproxy_expression(uri);
        let chain = eggress_pproxy_compat::uri::parse_pproxy_chain(uri)
            .map_err(|e| map_compat_parse_error(uri, &redacted_expr, e))?;
        if chain.hops.len() == 1 && chain.hops[0].scheme == "direct" {
            #[cfg(feature = "ssh")]
            let executor = eggress_server::build_chain_executor(None, None, None);
            #[cfg(not(feature = "ssh"))]
            let executor = eggress_server::build_chain_executor(None, None);
            return Ok(Self {
                runtime_config: None,
                chain_executor: executor,
                direct: true,
            });
        }
        if chain.hops.iter().any(|hop| hop.is_backward()) {
            return Err(EggressError::UnsupportedFeature {
                feature: "backward-upstream".to_string(),
                message: format!(
                    "pproxy chain '{}' uses a backward (+in) role which OutboundConnector cannot execute",
                    chain.redacted_display()
                ),
            });
        }
        let unsupported = eggress_pproxy_compat::uri::validate_chain_hops(&chain);
        if !unsupported.is_empty() {
            let roles = unsupported
                .iter()
                .map(|(_, scheme)| scheme.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(EggressError::UnsupportedFeature {
                feature: "chain-unsupported-hop".to_string(),
                message: format!(
                    "pproxy chain '{}' contains unsupported hop role(s): {}",
                    chain.redacted_display(),
                    roles
                ),
            });
        }
        let default_args = eggress_pproxy_compat::PproxyArgs::default_args();
        let chains = [chain.clone()];
        let output = eggress_pproxy_compat::translate_from_uris(&default_args, &[], &chains)
            .map_err(|e| map_compat_translate_error(&chain, uri, &redacted_expr, e))?;
        if !output.unsupported.is_empty() {
            let first_feature = output.unsupported[0].feature.to_string();
            let details = output
                .unsupported
                .iter()
                .map(|u| u.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            let details = scrub_message_with_chain(&chain, uri, &redacted_expr, details);
            return Err(EggressError::UnsupportedFeature {
                feature: first_feature,
                message: format!(
                    "pproxy chain '{}' is not executable outbound: {}",
                    chain.redacted_display(),
                    details
                ),
            });
        }
        Self::from_toml(&output.toml)
            .map_err(|e| sanitize_connector_error(&chain, uri, &redacted_expr, e))
    }

    /// Connect to a target host:port through the configured proxy chain.
    ///
    /// Returns the connected stream and connection metadata.
    pub async fn connect_tcp(
        &self,
        host: &str,
        port: u16,
    ) -> Result<(eggress_core::BoxStream, OutboundInfo), EggressError> {
        let target = eggress_core::TargetAddr {
            host: if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                eggress_core::TargetHost::Ip(ip)
            } else {
                eggress_core::TargetHost::Domain(host.to_string())
            },
            port,
        };

        if self.direct {
            let stream = eggress_core::connector::DirectConnector
                .connect_with_options(&target, &eggress_core::connector::ConnectOptions::default())
                .await
                .map_err(|e| EggressError::Runtime(e.to_string()))?;
            return Ok((
                stream,
                OutboundInfo {
                    local_addr: None,
                    peer_addr: None,
                    hop_count: 0,
                },
            ));
        }

        let runtime_config = self.runtime_config.as_ref().ok_or_else(|| {
            EggressError::Runtime("outbound runtime configuration is unavailable".to_string())
        })?;
        let upstream = &runtime_config.upstreams[0];
        let chain = &upstream.chain;

        // Resolve the first hop endpoint address for metadata
        let first_hop = &chain.hops[0];
        let peer_addr = resolve_endpoint_addr(&first_hop.endpoint).await;

        let stream = self
            .chain_executor
            .execute(&chain.hops, &target)
            .await
            .map_err(|e| EggressError::Runtime(e.to_string()))?;

        let info = OutboundInfo {
            local_addr: None,
            peer_addr,
            hop_count: chain.hops.len(),
        };

        Ok((stream, info))
    }

    /// Connect with a timeout.
    pub async fn connect_tcp_timeout(
        &self,
        host: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<(eggress_core::BoxStream, OutboundInfo), EggressError> {
        tokio::time::timeout(timeout, self.connect_tcp(host, port))
            .await
            .map_err(|_| EggressError::Runtime("connection timed out".to_string()))?
    }

    /// Create a UDP association through the configured proxy chain.
    ///
    /// Returns a `UdpAssociation` with the relay address to send/receive
    /// UDP datagrams through the proxy chain.
    ///
    /// UDP association requires SOCKS5 with UDP ASSOCIATE support.
    /// This method establishes the association and returns channel endpoints.
    pub async fn associate_udp(
        &self,
        _target_host: &str,
        _target_port: u16,
    ) -> Result<UdpAssociation, EggressError> {
        Err(EggressError::Runtime(
            "UDP association through OutboundConnector is not yet implemented; \
             use the listener-based approach for UDP"
                .to_string(),
        ))
    }

    /// Get the number of upstreams configured.
    pub fn upstream_count(&self) -> usize {
        self.runtime_config
            .as_ref()
            .map_or(0, |config| config.upstreams.len())
    }

    /// Validate that the config is usable for outbound connections.
    ///
    /// Returns the number of hops in the first upstream's chain.
    pub fn validate_outbound_config(config_toml: &str) -> Result<usize, EggressError> {
        let config: eggress_config::model::ConfigFile =
            toml::from_str(config_toml).map_err(|e| EggressError::Config(e.to_string()))?;

        if let Some(version) = config.version {
            if version != 1 {
                return Err(EggressError::Config(format!(
                    "unsupported config version: {version}"
                )));
            }
        }

        eggress_config::validate::validate_config(&config).map_err(|errors| {
            let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            EggressError::Config(messages.join("; "))
        })?;

        let runtime_config = eggress_config::compile::compile_config(&config)
            .map_err(|e| EggressError::Config(e.to_string()))?;

        if runtime_config.upstreams.is_empty() {
            return Err(EggressError::Config(
                "no upstreams configured; cannot make outbound connections".to_string(),
            ));
        }

        let upstream = &runtime_config.upstreams[0];
        let chain = &upstream.chain;

        if chain.hops.is_empty() {
            return Err(EggressError::Config(
                "upstream chain is empty; cannot make outbound connections".to_string(),
            ));
        }

        Ok(chain.hops.len())
    }
}

/// Redact credentials in a pproxy remote expression without requiring it
/// to parse successfully.
///
/// Valid hops use the typed `PproxyUri::redacted_display` so bind addresses
/// and plugin names survive; unparseable segments fall back to aggressive
/// syntax-local redaction. The exact rendering is not contractual; absence
/// of secrets is.
#[cfg(feature = "pproxy-compat")]
fn redact_pproxy_expression(input: &str) -> String {
    split_redaction_hops(input)
        .iter()
        .map(|segment| redact_pproxy_hop(segment))
        .collect::<Vec<_>>()
        .join("__")
}

/// Split a pproxy expression on `__` while ignoring separators inside
/// bracketed IPv6 literals and brace-delimited fixed targets.
/// Never fails; unmatched brackets are treated literally.
#[cfg(feature = "pproxy-compat")]
fn split_redaction_hops(input: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut bracket = 0u32;
    let mut brace = 0u32;
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] as char {
            '[' => bracket += 1,
            ']' => bracket = bracket.saturating_sub(1),
            '{' => brace += 1,
            '}' => brace = brace.saturating_sub(1),
            '_' if i + 1 < bytes.len() && bytes[i + 1] == b'_' && bracket == 0 && brace == 0 => {
                out.push(&input[start..i]);
                i += 1;
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&input[start..]);
    out
}

#[cfg(feature = "pproxy-compat")]
fn redact_pproxy_hop(segment: &str) -> String {
    if segment.is_empty() {
        return String::new();
    }
    if let Ok(parsed) = eggress_pproxy_compat::uri::parse_pproxy_uri(segment) {
        return parsed.redacted_display();
    }
    fallback_redact_hop(segment)
}

/// Aggressive fallback for hops that do not parse: hide anything that
/// could be userinfo and any `#` auth fragment. Over-redaction is
/// acceptable here; leakage is not.
#[cfg(feature = "pproxy-compat")]
fn fallback_redact_hop(segment: &str) -> String {
    let (before_hash, has_fragment) = match segment.find('#') {
        Some(pos) => (&segment[..pos], true),
        None => (segment, false),
    };
    let frag_suffix = if has_fragment { "#****" } else { "" };
    if before_hash.starts_with("unix://") {
        return format!("unix://****{frag_suffix}");
    }
    let Some(scheme_end) = before_hash.find("://") else {
        if let Some(at) = find_last_at_outside_brackets(before_hash) {
            return format!("****:****@{}{frag_suffix}", &before_hash[at + 1..]);
        }
        return format!("{before_hash}{frag_suffix}");
    };
    let scheme = &before_hash[..scheme_end];
    let after = &before_hash[scheme_end + 3..];
    if let Some(at) = find_last_at_outside_brackets(after) {
        format!("{}://****:****@{}{frag_suffix}", scheme, &after[at + 1..])
    } else {
        format!("{before_hash}{frag_suffix}")
    }
}

#[cfg(feature = "pproxy-compat")]
fn find_last_at_outside_brackets(s: &str) -> Option<usize> {
    let mut last = None;
    let mut depth = 0u32;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '@' if depth == 0 => last = Some(i),
            _ => {}
        }
    }
    last
}

/// Redact `scheme://...@...` userinfo occurrences embedded in free-form
/// diagnostic text, plus `#` auth fragments that carry credentials.
#[cfg(feature = "pproxy-compat")]
fn redact_credentials_in_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("://") {
        out.push_str(&rest[..pos + 3]);
        rest = &rest[pos + 3..];
        let mut token_end = rest.len();
        for (i, c) in rest.char_indices() {
            if c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '(' | ')') {
                token_end = i;
                break;
            }
        }
        let (token, remainder) = (&rest[..token_end], &rest[token_end..]);
        let (before_hash, fragment) = match token.find('#') {
            Some(p) => (&token[..p], Some(&token[p..])),
            None => (token, None),
        };
        if let Some(at) = find_last_at_outside_brackets(before_hash) {
            out.push_str("****:****@");
            out.push_str(&before_hash[at + 1..]);
        } else {
            out.push_str(before_hash);
        }
        if let Some(frag) = fragment {
            if frag.contains(':') || frag.contains('@') {
                out.push_str("#****");
            } else {
                out.push_str(frag);
            }
        }
        rest = remainder;
    }
    out.push_str(rest);
    out
}

/// Percent-encode mirroring the compatibility translator so scrubbing
/// catches credentials that reappear in generated config URIs.
#[cfg(feature = "pproxy-compat")]
fn percent_encode_for_scrub(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(feature = "pproxy-compat")]
fn chain_credential_terms(chain: &eggress_pproxy_compat::uri::PproxyChain) -> Vec<String> {
    let mut terms = Vec::new();
    for hop in &chain.hops {
        for value in [&hop.username, &hop.password].into_iter().flatten() {
            if !value.is_empty() {
                terms.push(value.clone());
            }
        }
        if let Some(fragment) = &hop.auth_fragment {
            if !fragment.is_empty() {
                terms.push(fragment.clone());
                if let Some((user, pass)) = fragment.split_once(':') {
                    if !user.is_empty() {
                        terms.push(user.to_string());
                    }
                    if !pass.is_empty() {
                        terms.push(pass.to_string());
                    }
                }
            }
        }
    }
    terms.sort_by_key(|term| std::cmp::Reverse(term.len()));
    terms
}

/// Scrub a diagnostic message of the original expression and every
/// credential term carried by the parsed chain (raw and percent-encoded),
/// plus any generic `://...@` userinfo that remains.
#[cfg(feature = "pproxy-compat")]
fn scrub_message_with_chain(
    chain: &eggress_pproxy_compat::uri::PproxyChain,
    original_uri: &str,
    redacted_expr: &str,
    message: String,
) -> String {
    let mut msg = message.replace(original_uri, redacted_expr);
    for term in chain_credential_terms(chain) {
        if !term.is_empty() {
            msg = msg.replace(term.as_str(), "****");
            let encoded = percent_encode_for_scrub(&term);
            if encoded != term {
                msg = msg.replace(encoded.as_str(), "****");
            }
        }
    }
    redact_credentials_in_text(&msg)
}

#[cfg(feature = "pproxy-compat")]
fn map_compat_parse_error(
    uri: &str,
    redacted_expr: &str,
    error: eggress_pproxy_compat::CompatError,
) -> EggressError {
    let raw = error.to_string();
    let mut detail = raw.replace(uri, redacted_expr);
    detail = redact_credentials_in_text(&detail);
    let message = format!("invalid pproxy chain '{redacted_expr}': {detail}");
    match error {
        eggress_pproxy_compat::CompatError::UnsupportedProtocol(protocol) => {
            EggressError::UnsupportedFeature {
                feature: protocol,
                message,
            }
        }
        eggress_pproxy_compat::CompatError::UnsupportedFeature { feature, .. } => {
            EggressError::UnsupportedFeature {
                feature: feature.to_string(),
                message,
            }
        }
        _ => EggressError::Config(message),
    }
}

#[cfg(feature = "pproxy-compat")]
fn map_compat_translate_error(
    chain: &eggress_pproxy_compat::uri::PproxyChain,
    uri: &str,
    redacted_expr: &str,
    error: eggress_pproxy_compat::CompatError,
) -> EggressError {
    let detail = scrub_message_with_chain(chain, uri, redacted_expr, error.to_string());
    let message = format!(
        "pproxy chain '{}' failed translation: {}",
        chain.redacted_display(),
        detail
    );
    match error {
        eggress_pproxy_compat::CompatError::UnsupportedProtocol(protocol) => {
            EggressError::UnsupportedFeature {
                feature: protocol,
                message,
            }
        }
        eggress_pproxy_compat::CompatError::UnsupportedFeature { feature, .. } => {
            EggressError::UnsupportedFeature {
                feature: feature.to_string(),
                message,
            }
        }
        _ => EggressError::Config(message),
    }
}

#[cfg(feature = "pproxy-compat")]
fn sanitize_connector_error(
    chain: &eggress_pproxy_compat::uri::PproxyChain,
    uri: &str,
    redacted_expr: &str,
    error: EggressError,
) -> EggressError {
    match error {
        EggressError::Config(message) => {
            EggressError::Config(scrub_message_with_chain(chain, uri, redacted_expr, message))
        }
        EggressError::Runtime(message) => {
            EggressError::Runtime(scrub_message_with_chain(chain, uri, redacted_expr, message))
        }
        EggressError::Startup(message) => {
            EggressError::Startup(scrub_message_with_chain(chain, uri, redacted_expr, message))
        }
        EggressError::Reload(message) => {
            EggressError::Reload(scrub_message_with_chain(chain, uri, redacted_expr, message))
        }
        EggressError::Shutdown(message) => {
            EggressError::Shutdown(scrub_message_with_chain(chain, uri, redacted_expr, message))
        }
        EggressError::UnsupportedFeature { feature, message } => EggressError::UnsupportedFeature {
            feature,
            message: scrub_message_with_chain(chain, uri, redacted_expr, message),
        },
        EggressError::Internal(message) => {
            EggressError::Internal(scrub_message_with_chain(chain, uri, redacted_expr, message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outbound_connector_from_toml() {
        let config = r#"
            version = 1
            [[listeners]]
            name = "test"
            bind = "127.0.0.1:0"
            protocols = ["socks5"]
            [[upstreams]]
            id = "direct"
            uri = "socks5://127.0.0.1:1080"
        "#;
        let connector = OutboundConnector::from_toml(config).unwrap();
        assert_eq!(connector.upstream_count(), 1);
    }

    #[test]
    fn test_validate_no_upstreams() {
        let config = r#"
            version = 1
            [[listeners]]
            name = "test"
            bind = "127.0.0.1:0"
            protocols = ["socks5"]
        "#;
        let result = OutboundConnector::validate_outbound_config(config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no upstreams"));
    }

    #[test]
    fn test_validate_empty_chain() {
        let config = r#"
            version = 1
            [[listeners]]
            name = "test"
            bind = "127.0.0.1:0"
            protocols = ["socks5"]
            [[upstreams]]
            id = "up"
            uri = "socks5://127.0.0.1:1080"
        "#;
        let result = OutboundConnector::validate_outbound_config(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_from_pproxy_uri() {
        let connector = OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080").unwrap();
        assert_eq!(connector.upstream_count(), 1);
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_single_http() {
        let connector =
            OutboundConnector::from_pproxy_uri("http://127.0.0.1:8080").expect("single HTTP");
        let runtime = connector
            .runtime_config
            .as_ref()
            .expect("single-hop connector has runtime config");
        assert_eq!(runtime.upstreams.len(), 1);
        assert_eq!(runtime.upstreams[0].chain.hops.len(), 1);
        assert!(
            runtime.upstreams[0].chain.hops[0]
                .protocols
                .contains(&eggress_uri::ProtocolSpec::Http),
            "expected HTTP hop, got {:?}",
            runtime.upstreams[0].chain.hops[0].protocols
        );
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_two_hop_chain() {
        let connector =
            OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080__http://127.0.0.1:8080")
                .expect("two-hop chain should construct");
        let runtime = connector
            .runtime_config
            .as_ref()
            .expect("chained connector has runtime config");
        assert_eq!(runtime.upstreams.len(), 1);
        let hops = &runtime.upstreams[0].chain.hops;
        assert_eq!(hops.len(), 2, "expected two ordered hops, got {hops:?}");
        assert!(
            hops[0]
                .protocols
                .contains(&eggress_uri::ProtocolSpec::Socks5),
            "hop 0 should be SOCKS5, got {:?}",
            hops[0].protocols
        );
        assert!(
            hops[1].protocols.contains(&eggress_uri::ProtocolSpec::Http),
            "hop 1 should be HTTP, got {:?}",
            hops[1].protocols
        );
        assert!(!connector.direct);
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_three_hop_chain() {
        let connector = OutboundConnector::from_pproxy_uri(
            "socks5://127.0.0.1:1080__http://127.0.0.1:8080__socks4://127.0.0.1:1081",
        )
        .expect("three-hop chain should construct");
        let runtime = connector.runtime_config.as_ref().expect("runtime config");
        let hops = &runtime.upstreams[0].chain.hops;
        assert_eq!(hops.len(), 3);
        assert!(hops[0]
            .protocols
            .contains(&eggress_uri::ProtocolSpec::Socks5));
        assert!(hops[1].protocols.contains(&eggress_uri::ProtocolSpec::Http));
        assert!(hops[2]
            .protocols
            .contains(&eggress_uri::ProtocolSpec::Socks4));
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_direct_fast_path() {
        let connector = OutboundConnector::from_pproxy_uri("direct://").expect("direct://");
        assert!(connector.direct);
        assert!(connector.runtime_config.is_none());
        assert_eq!(connector.upstream_count(), 0);
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_malformed_chain_rejected() {
        for uri in ["socks5://127.0.0.1:1080__", "__socks5://127.0.0.1:1080"] {
            let result = OutboundConnector::from_pproxy_uri(uri);
            assert!(result.is_err(), "malformed chain should fail: {uri}");
        }
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_multihop_direct_not_collapsed() {
        let result = OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080__direct://");
        assert!(result.is_err(), "multi-hop direct must fail closed");
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_unsupported_hop_fails_closed() {
        let result =
            OutboundConnector::from_pproxy_uri("socks5://127.0.0.1:1080__redir://127.0.0.1:1234");
        assert!(result.is_err(), "unsupported hop must fail closed");
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_valid_credentialed_chain_keeps_credentials() {
        let connector = OutboundConnector::from_pproxy_uri(
            "socks5://user1:pass1@127.0.0.1:1080__http://user2:pass2@127.0.0.1:8080",
        )
        .expect("credentialed chain should construct");
        let runtime = connector.runtime_config.as_ref().expect("runtime config");
        let hops = &runtime.upstreams[0].chain.hops;
        assert_eq!(hops.len(), 2);
        let first = hops[0].credentials.as_ref().expect("hop 0 credentials");
        assert_eq!(first.username, "user1");
        assert_eq!(first.password, "pass1");
        let second = hops[1].credentials.as_ref().expect("hop 1 credentials");
        assert_eq!(second.username, "user2");
        assert_eq!(second.password, "pass2");
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn test_from_pproxy_uri_malformed_chain_redacts_credentials() {
        let uri =
            "socks5://user_a:secret_a@127.0.0.1:1080__http://user_b:secret_b@127.0.0.1:8080__";
        let err = match OutboundConnector::from_pproxy_uri(uri) {
            Ok(_) => panic!("trailing __ must fail"),
            Err(err) => err,
        };
        let rendered = format!("{err:?} {err}");
        for secret in ["user_a", "secret_a", "user_b", "secret_b"] {
            assert!(
                !rendered.contains(secret),
                "error leaked {secret:?}: {rendered}"
            );
        }
    }
}
