//! Route matching: matchers plus normalization/matching helpers.
//!
//! Hostname normalization, suffix behavior, raw regex semantics, CIDR
//! matching, port matching, source matching, transport matching, and
//! reverse-listener matching are preserved exactly during module movement.

use std::borrow::Cow;
use std::net::IpAddr;
use std::sync::Arc;

use eggress_core::{ClientIdentity, ProtocolId, TargetHost};

use crate::model::{RouteRequest, TransportKind};

#[derive(Debug, Clone)]
pub enum PortMatcher {
    Exact(u16),
    Range {
        start: u16,
        end: u16,
    },
    /// Port set. The slice must be sorted ascending so matching can use a
    /// binary search; construction sites are responsible for sorting.
    Set(Arc<[u16]>),
}

impl PortMatcher {
    pub fn matches(&self, port: u16) -> bool {
        match self {
            PortMatcher::Exact(p) => port == *p,
            PortMatcher::Range { start, end } => {
                if start > end {
                    debug_assert!(
                        start <= end,
                        "inverted PortMatcher::Range ({start}..{end}) matches nothing"
                    );
                    tracing::warn!("inverted PortMatcher::Range ({start}..{end}) matches nothing");
                    return false;
                }
                port >= *start && port <= *end
            }
            PortMatcher::Set(ports) => {
                // PortMatcher::Set is documented to hold a sorted slice.
                // Construction sites (including `new_set` and config compilation)
                // enforce sorting; matching uses binary search without per-call
                // validation to avoid O(n) scans and log amplification.
                debug_assert!(
                    ports.windows(2).all(|w| w[0] < w[1]),
                    "PortMatcher::Set must be sorted and deduped"
                );
                ports.binary_search(&port).is_ok()
            }
        }
    }

    /// Construct a validated range matcher, returning an error if `start > end`.
    pub fn new_range(start: u16, end: u16) -> Result<Self, String> {
        if start > end {
            return Err(format!(
                "PortMatcher::Range start ({start}) must be <= end ({end})"
            ));
        }
        Ok(PortMatcher::Range { start, end })
    }

    /// Construct a sorted, deduplicated set matcher. The input is sorted and
    /// deduplicated at construction so `matches` can use binary search without
    /// per-call validation.
    pub fn new_set(mut ports: Vec<u16>) -> Self {
        ports.sort_unstable();
        ports.dedup();
        PortMatcher::Set(Arc::from(ports.as_slice()))
    }
}

#[derive(Debug, Clone)]
pub enum MatchExpr {
    Any,
    All(Vec<MatchExpr>),
    AnyOf(Vec<MatchExpr>),
    Not(Box<MatchExpr>),
    HostExact(Arc<str>),
    HostSuffix(Arc<str>),
    /// Match the raw host string without normalization.
    ///
    /// Unlike `HostExact` and `HostSuffix`, regex matching is case-sensitive
    /// and preserves a trailing dot. Use an inline `(?i)` flag when a regex
    /// should match hostnames case-insensitively.
    HostRegex(regex::Regex),
    DestinationCidr(ipnet::IpNet),
    DestinationPort(PortMatcher),
    /// Match the decimal destination port against a compatibility regex.
    /// This is used by pproxy URI rules, which test both the hostname and
    /// `str(port)` rather than limiting rules to hostnames.
    DestinationPortRegex(regex::Regex),
    SourceCidr(ipnet::IpNet),
    SourcePort(PortMatcher),
    Listener(Arc<str>),
    Protocol(ProtocolId),
    Identity(Arc<str>),
    Transport(TransportKind),
    /// Match a reverse tunnel listener name. Only effective when the request's
    /// transport is `ReverseTcp`.
    ReverseListener(Arc<str>),
}

pub fn normalize_host_for_exact(host: &str) -> String {
    let h = host.strip_suffix('.').unwrap_or(host);
    if let Ok(ip) = h.parse::<IpAddr>() {
        // Canonical form lowercases IPv6 hex digits and collapses padding,
        // so `FE80::1` and `fe80::1` compare equal. IPv4-mapped IPv6
        // literals canonicalize to plain IPv4, so a dual-stack client
        // sending `::ffff:a.b.c.d` matches a rule written for `a.b.c.d`.
        match ip {
            IpAddr::V6(v6) => v6
                .to_ipv4_mapped()
                .map_or_else(|| ip.to_string(), |v4| v4.to_string()),
            IpAddr::V4(_) => ip.to_string(),
        }
    } else {
        h.to_ascii_lowercase()
    }
}

fn target_host_str(host: &TargetHost) -> Cow<'_, str> {
    match host {
        TargetHost::Domain(domain) => Cow::Borrowed(domain),
        TargetHost::Ip(ip) => Cow::Owned(ip.to_string()),
    }
}

fn host_matches_suffix(host: &str, suffix: &str) -> bool {
    let host = host.strip_suffix('.').unwrap_or(host);
    let suffix = suffix.strip_suffix('.').unwrap_or(suffix);

    if host.eq_ignore_ascii_case(suffix) || host.len() <= suffix.len() {
        return host.eq_ignore_ascii_case(suffix);
    }

    let suffix_start = host.len() - suffix.len();
    host.get(..suffix_start)
        .zip(host.get(suffix_start..))
        .is_some_and(|(prefix, host_suffix)| {
            prefix.ends_with('.') && host_suffix.eq_ignore_ascii_case(suffix)
        })
}

impl MatchExpr {
    pub fn matches(&self, request: &RouteRequest<'_>) -> bool {
        match self {
            MatchExpr::Any => true,
            MatchExpr::All(exprs) => exprs.iter().all(|e| e.matches(request)),
            MatchExpr::AnyOf(exprs) => exprs.iter().any(|e| e.matches(request)),
            MatchExpr::Not(inner) => !inner.matches(request),
            MatchExpr::HostExact(expected) => match &request.target.host {
                // Fast path for domain targets: case-insensitive compare
                // without heap-allocating a normalized copy per rule (O-02).
                // `expected` is stored normalized (lowercase, no trailing
                // dot) at compile time.
                TargetHost::Domain(domain) => {
                    let host = domain.strip_suffix('.').unwrap_or(domain);
                    host.eq_ignore_ascii_case(expected.as_ref())
                }
                TargetHost::Ip(_) => {
                    let host_str = target_host_str(&request.target.host);
                    normalize_host_for_exact(host_str.as_ref()) == expected.as_ref()
                }
            },
            MatchExpr::HostSuffix(suffix) => match &request.target.host {
                // Suffix constants are normalized once at compile (O-01), so
                // domain targets match allocation-free via the case-insensitive
                // suffix helper; IP literals keep canonicalization for
                // IPv4-mapped/hex equivalence.
                TargetHost::Domain(domain) => host_matches_suffix(domain, suffix),
                TargetHost::Ip(_) => {
                    let host_str = target_host_str(&request.target.host);
                    let normalized_host = normalize_host_for_exact(host_str.as_ref());
                    host_matches_suffix(&normalized_host, suffix)
                }
            },
            MatchExpr::HostRegex(re) => {
                // HostRegex intentionally matches the raw host string (case-
                // sensitive, trailing-dot preserved) unlike HostExact/Suffix.
                // Use an inline `(?i)` flag when case-insensitive regex
                // semantics are desired. IPv4-mapped canonicalization is not
                // applied here to preserve regex author expectations.
                let host_str = target_host_str(&request.target.host);
                re.is_match(host_str.as_ref())
            }
            MatchExpr::DestinationCidr(cidr) => {
                if let TargetHost::Ip(ip) = &request.target.host {
                    cidr.contains(ip)
                } else {
                    false
                }
            }
            MatchExpr::DestinationPort(matcher) => matcher.matches(request.target.port),
            MatchExpr::DestinationPortRegex(re) => {
                let mut port_buf = [0u8; 5];
                fmt_port(request.target.port, &mut port_buf).is_some_and(|port| re.is_match(port))
            }
            MatchExpr::SourceCidr(cidr) => {
                if let Some(addr) = request.source {
                    cidr.contains(&addr.ip())
                } else {
                    false
                }
            }
            MatchExpr::SourcePort(matcher) => {
                if let Some(addr) = request.source {
                    matcher.matches(addr.port())
                } else {
                    false
                }
            }
            MatchExpr::Listener(name) => request.listener == name.as_ref(),
            MatchExpr::Protocol(proto) => request.inbound_protocol == *proto,
            MatchExpr::Identity(name) => match &request.identity {
                ClientIdentity::Anonymous => false,
                ClientIdentity::Username(u) => u == name.as_ref(),
                ClientIdentity::Opaque(o) => o == name.as_ref(),
            },
            MatchExpr::Transport(kind) => request.transport == *kind,
            MatchExpr::ReverseListener(expected) => {
                request.transport == TransportKind::ReverseTcp
                    && request.listener == expected.as_ref()
            }
        }
    }
}

pub(crate) fn fmt_port(port: u16, buf: &mut [u8; 5]) -> Option<&str> {
    // A u16 has at most five decimal digits (u16::MAX is 65535).
    let mut digits = [0u8; 5];
    let mut count = 0;
    let mut remaining = port;
    loop {
        digits[count] = b'0' + (remaining % 10) as u8;
        count += 1;
        remaining /= 10;
        if remaining == 0 {
            break;
        }
    }
    for (dst, src) in buf[..count].iter_mut().zip(digits[..count].iter().rev()) {
        *dst = *src;
    }
    std::str::from_utf8(&buf[..count]).ok()
}
