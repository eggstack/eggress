use crate::ProtocolId;

/// Result of a protocol detection attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectResult {
    /// The protocol was matched with the given confidence level (0–100).
    Match { confidence: u8 },
    /// More data is needed; `minimum` is the smallest number of total prefix
    /// bytes required before a definitive result can be given.
    NeedMore { minimum: usize },
    /// This prefix does not match the protocol.
    NoMatch,
}

/// A trait for protocol detectors that can identify a protocol from the
/// initial bytes of a stream.
pub trait ProtocolDetector: Send + Sync {
    /// Returns the unique identifier for the protocol this detector handles.
    fn id(&self) -> ProtocolId;

    /// Attempts to detect the protocol from the given byte prefix.
    ///
    /// # Arguments
    /// * `prefix` - The bytes read from the start of the stream so far.
    fn detect(&self, prefix: &[u8]) -> DetectResult;
}

/// A simple prefix-based detector useful for testing and simple protocols.
pub struct PrefixDetector {
    id: ProtocolId,
    prefix: Vec<u8>,
    min_length: usize,
}

impl PrefixDetector {
    pub fn new(id: ProtocolId, prefix: Vec<u8>) -> Self {
        let min_length = prefix.len();
        Self {
            id,
            prefix,
            min_length,
        }
    }

    pub fn with_min_length(id: ProtocolId, prefix: Vec<u8>, min_length: usize) -> Self {
        Self {
            id,
            prefix,
            min_length,
        }
    }
}

impl ProtocolDetector for PrefixDetector {
    fn id(&self) -> ProtocolId {
        self.id
    }

    fn detect(&self, data: &[u8]) -> DetectResult {
        if data.len() < self.min_length {
            DetectResult::NeedMore {
                minimum: self.min_length,
            }
        } else if data.starts_with(&self.prefix) {
            DetectResult::Match { confidence: 100 }
        } else {
            DetectResult::NoMatch
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_detector_match() {
        let detector = PrefixDetector::new("http", b"GET ".to_vec());
        assert_eq!(
            detector.detect(b"GET / HTTP/1.1"),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[test]
    fn test_prefix_detector_no_match() {
        let detector = PrefixDetector::new("http", b"GET ".to_vec());
        assert_eq!(detector.detect(b"POST /"), DetectResult::NoMatch);
    }

    #[test]
    fn test_prefix_detector_need_more() {
        let detector = PrefixDetector::new("http", b"GET ".to_vec());
        assert_eq!(
            detector.detect(b"GE"),
            DetectResult::NeedMore { minimum: 4 }
        );
    }

    #[test]
    fn test_prefix_detector_empty_input() {
        let detector = PrefixDetector::new("http", b"GET ".to_vec());
        assert_eq!(detector.detect(b""), DetectResult::NeedMore { minimum: 4 });
    }

    #[test]
    fn test_prefix_detector_exact_match() {
        let detector = PrefixDetector::new("socks5", b"\x05".to_vec());
        assert_eq!(
            detector.detect(b"\x05"),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[test]
    fn test_prefix_detector_with_min_length() {
        let detector = PrefixDetector::with_min_length("http", b"GET ".to_vec(), 8);
        // Have 4 bytes which is the prefix length but min_length is 8
        assert_eq!(
            detector.detect(b"GET /"),
            DetectResult::NeedMore { minimum: 8 }
        );
    }
}
