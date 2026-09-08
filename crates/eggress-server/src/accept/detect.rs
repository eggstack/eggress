//! Protocol detection: first-byte dispatch for inbound streams.

pub(crate) enum DetectResult {
    Match,
    NeedMore,
    NoMatch,
}

pub(crate) fn detect_http_method(prefix: &[u8]) -> DetectResult {
    // Look for a space in the prefix to find the end of the method token
    if let Some(space_pos) = prefix.iter().position(|&b| b == b' ') {
        let method_token = &prefix[..space_pos];
        if method_token.is_empty() || method_token.len() > 16 {
            return DetectResult::NoMatch;
        }
        // HTTP methods use the RFC token grammar, so extension methods may
        // legitimately contain lowercase letters and hyphens as well.
        let is_valid_method = method_token
            .iter()
            .all(|&b| b.is_ascii_uppercase() || b == b'-' || b.is_ascii_lowercase());
        if is_valid_method {
            DetectResult::Match
        } else {
            DetectResult::NoMatch
        }
    } else {
        // No space found yet - check if what we have so far looks like a valid method prefix
        if prefix.len() > 16 {
            return DetectResult::NoMatch;
        }
        // Check if all bytes so far are valid method characters
        let is_valid_prefix = prefix
            .iter()
            .all(|&b| b.is_ascii_uppercase() || b == b'-' || b.is_ascii_lowercase());
        if is_valid_prefix {
            DetectResult::NeedMore
        } else {
            DetectResult::NoMatch
        }
    }
}
