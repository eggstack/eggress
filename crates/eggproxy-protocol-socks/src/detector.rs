use eggproxy_core::detect::{DetectResult, ProtocolDetector};
use eggproxy_core::ProtocolId;

/// Protocol identifier for SOCKS4.
pub const SOCKS4_PROTOCOL_ID: ProtocolId = "socks4";

/// Detector for SOCKS4/4a protocol.
///
/// SOCKS4 requests always start with version byte 0x04.
pub struct Socks4Detector;

impl ProtocolDetector for Socks4Detector {
    fn id(&self) -> ProtocolId {
        SOCKS4_PROTOCOL_ID
    }

    fn detect(&self, prefix: &[u8]) -> DetectResult {
        if prefix.is_empty() {
            DetectResult::NeedMore { minimum: 1 }
        } else if prefix[0] == 0x04 {
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
    fn test_detect_empty() {
        let detector = Socks4Detector;
        assert_eq!(detector.detect(b""), DetectResult::NeedMore { minimum: 1 });
    }

    #[test]
    fn test_detect_match() {
        let detector = Socks4Detector;
        assert_eq!(
            detector.detect(b"\x04"),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[test]
    fn test_detect_match_full_header() {
        let detector = Socks4Detector;
        // Full SOCKS4 CONNECT header
        assert_eq!(
            detector.detect(b"\x04\x01\x00\x50\x7f\x00\x00\x01"),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[test]
    fn test_detect_no_match() {
        let detector = Socks4Detector;
        assert_eq!(detector.detect(b"\x05"), DetectResult::NoMatch);
        assert_eq!(detector.detect(b"\x03"), DetectResult::NoMatch);
    }

    #[test]
    fn test_detector_id() {
        let detector = Socks4Detector;
        assert_eq!(detector.id(), "socks4");
    }
}
