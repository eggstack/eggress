//! Private pproxy compatibility boundary for outbound chains.
//!
//! Owns pproxy-specific constructor adaptation, credential-term extraction,
//! redaction/scrubbing (over-redaction preferred, bracket-aware `@`
//! handling, `#` auth fragment masking), and compat error mapping. All
//! credentials stay absent from returned/displayed errors. The tolerant
//! fallback scrubber is retained; `eggress-uri` is not forced to understand
//! malformed pproxy-only syntax.

#[cfg(feature = "pproxy-compat")]
use crate::OutboundError;

/// Redact credentials in a pproxy remote expression without requiring it
/// to parse successfully.
///
/// Valid hops use the typed `PproxyUri::redacted_display` so bind addresses
/// and plugin names survive; unparseable segments fall back to aggressive
/// syntax-local redaction. The exact rendering is not contractual; absence
/// of secrets is.
#[cfg(feature = "pproxy-compat")]
pub(crate) fn redact_pproxy_expression(input: &str) -> String {
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
pub(crate) fn split_redaction_hops(input: &str) -> Vec<&str> {
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
pub(crate) fn redact_pproxy_hop(segment: &str) -> String {
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
pub(crate) fn fallback_redact_hop(segment: &str) -> String {
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
pub(crate) fn find_last_at_outside_brackets(s: &str) -> Option<usize> {
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
pub(crate) fn redact_credentials_in_text(text: &str) -> String {
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
pub(crate) fn percent_encode_for_scrub(s: &str) -> String {
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
pub(crate) fn chain_credential_terms(
    chain: &eggress_pproxy_compat::uri::PproxyChain,
) -> Vec<String> {
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
pub(crate) fn scrub_message_with_chain(
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
pub(crate) fn map_compat_parse_error(
    uri: &str,
    redacted_expr: &str,
    error: eggress_pproxy_compat::CompatError,
) -> OutboundError {
    let raw = error.to_string();
    let mut detail = raw.replace(uri, redacted_expr);
    detail = redact_credentials_in_text(&detail);
    let message = format!("invalid pproxy chain '{redacted_expr}': {detail}");
    match error {
        eggress_pproxy_compat::CompatError::UnsupportedProtocol(protocol) => {
            OutboundError::UnsupportedFeature {
                feature: protocol,
                message,
            }
        }
        eggress_pproxy_compat::CompatError::UnsupportedFeature { feature, .. } => {
            OutboundError::UnsupportedFeature {
                feature: feature.to_string(),
                message,
            }
        }
        _ => OutboundError::Config(message),
    }
}

#[cfg(feature = "pproxy-compat")]
pub(crate) fn map_compat_translate_error(
    chain: &eggress_pproxy_compat::uri::PproxyChain,
    uri: &str,
    redacted_expr: &str,
    error: eggress_pproxy_compat::CompatError,
) -> OutboundError {
    let detail = scrub_message_with_chain(chain, uri, redacted_expr, error.to_string());
    let message = format!(
        "pproxy chain '{}' failed translation: {}",
        chain.redacted_display(),
        detail
    );
    match error {
        eggress_pproxy_compat::CompatError::UnsupportedProtocol(protocol) => {
            OutboundError::UnsupportedFeature {
                feature: protocol,
                message,
            }
        }
        eggress_pproxy_compat::CompatError::UnsupportedFeature { feature, .. } => {
            OutboundError::UnsupportedFeature {
                feature: feature.to_string(),
                message,
            }
        }
        _ => OutboundError::Config(message),
    }
}
