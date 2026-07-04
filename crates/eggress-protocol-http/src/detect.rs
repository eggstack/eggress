use eggress_core::detect::{DetectResult, ProtocolDetector};
use eggress_core::ProtocolId;

/// HTTP protocol detector.
///
/// Checks if the input starts with a known HTTP method or "HTTP/".
pub struct HttpDetector;

const HTTP_METHODS: &[&[u8]] = &[
    b"GET ",
    b"POST ",
    b"PUT ",
    b"DELETE ",
    b"HEAD ",
    b"OPTIONS ",
    b"PATCH ",
    b"CONNECT ",
    b"TRACE ",
];

impl ProtocolDetector for HttpDetector {
    fn id(&self) -> ProtocolId {
        ProtocolId::Http
    }

    fn detect(&self, prefix: &[u8]) -> DetectResult {
        if prefix.is_empty() {
            return DetectResult::NeedMore { minimum: 1 };
        }

        // Check for "HTTP/" (response prefix)
        if prefix.starts_with(b"HTTP/") {
            if prefix.len() >= 5 {
                return DetectResult::Match { confidence: 95 };
            }
            return DetectResult::NeedMore { minimum: 5 };
        }

        // Partial "HTTP/" prefix must not be treated as no-match.
        if b"HTTP/".starts_with(prefix) {
            return DetectResult::NeedMore { minimum: 5 };
        }

        // Check for known HTTP methods
        for method in HTTP_METHODS {
            if prefix.starts_with(method) {
                return DetectResult::Match { confidence: 100 };
            }
            // Check if prefix is a partial match for this method
            if method.starts_with(prefix) {
                return DetectResult::NeedMore {
                    minimum: method.len(),
                };
            }
        }

        DetectResult::NoMatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_get_match() {
        let detector = HttpDetector;
        assert_eq!(
            detector.detect(b"GET / HTTP/1.1\r\n"),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[test]
    fn test_http_connect_match() {
        let detector = HttpDetector;
        assert_eq!(
            detector.detect(b"CONNECT example.com:443 HTTP/1.1\r\n"),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[test]
    fn test_http_response_match() {
        let detector = HttpDetector;
        assert_eq!(
            detector.detect(b"HTTP/1.1 200 OK\r\n"),
            DetectResult::Match { confidence: 95 }
        );
    }

    #[test]
    fn test_http_need_more() {
        let detector = HttpDetector;
        assert_eq!(
            detector.detect(b"GE"),
            DetectResult::NeedMore { minimum: 4 }
        );
    }

    #[test]
    fn test_http_empty() {
        let detector = HttpDetector;
        assert_eq!(detector.detect(b""), DetectResult::NeedMore { minimum: 1 });
    }

    #[test]
    fn test_http_no_match() {
        let detector = HttpDetector;
        assert_eq!(detector.detect(b"\x05"), DetectResult::NoMatch);
        assert_eq!(detector.detect(b"\x04"), DetectResult::NoMatch);
    }

    #[test]
    fn test_http_post_match() {
        let detector = HttpDetector;
        assert_eq!(
            detector.detect(b"POST /api HTTP/1.1\r\n"),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[test]
    fn test_http_partial_method() {
        let detector = HttpDetector;
        // "P" could be POST, PUT, PATCH
        assert!(matches!(
            detector.detect(b"P"),
            DetectResult::NeedMore { .. }
        ));
    }

    #[test]
    fn test_http_partial_response_prefix_needs_more() {
        let detector = HttpDetector;
        // Single byte that could be the start of "HTTP/" must not be rejected
        // as NoMatch (which would happen on slow server-side dribbles).
        assert!(matches!(
            detector.detect(b"H"),
            DetectResult::NeedMore { minimum: 5 }
        ));
        assert!(matches!(
            detector.detect(b"HT"),
            DetectResult::NeedMore { minimum: 5 }
        ));
        assert!(matches!(
            detector.detect(b"HTT"),
            DetectResult::NeedMore { minimum: 5 }
        ));
        assert!(matches!(
            detector.detect(b"HTTP"),
            DetectResult::NeedMore { minimum: 5 }
        ));
    }
}
