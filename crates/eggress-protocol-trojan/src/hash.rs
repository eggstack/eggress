use sha2::{Digest, Sha224};
use subtle::ConstantTimeEq;

/// Compute Trojan password hash (SHA224 hex).
///
/// The Trojan protocol authenticates using a hex-encoded SHA224 hash of the
/// password, sent as the first line of the handshake.
pub fn password_hash(password: &str) -> String {
    let hash = Sha224::digest(password.as_bytes());
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Check whether a received 56-byte hash matches the expected password.
///
/// Uses constant-time comparison to prevent timing attacks.
/// Returns `true` if the hash matches, `false` otherwise.
pub fn trojan_check_password(received_hash: &[u8; 56], password: &str) -> bool {
    let expected = password_hash(password);
    received_hash.ct_eq(expected.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hash_known_value() {
        let hash = password_hash("password");
        assert_eq!(
            hash,
            "d63dc919e201d7bc4c825630d2cf25fdc93d4b2f0d46706d29038d01"
        );
    }

    #[test]
    fn test_password_hash_empty() {
        let hash = password_hash("");
        assert_eq!(
            hash,
            "d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f"
        );
    }

    #[test]
    fn test_password_hash_length() {
        let hash = password_hash("test");
        assert_eq!(hash.len(), 56); // SHA224 = 28 bytes = 56 hex chars
    }

    #[test]
    fn test_trojan_check_password_match() {
        let hash = password_hash("mypassword");
        let bytes: [u8; 56] = hash.as_bytes().try_into().unwrap();
        assert!(trojan_check_password(&bytes, "mypassword"));
    }

    #[test]
    fn test_trojan_check_password_mismatch() {
        let hash = password_hash("wrongpassword");
        let bytes: [u8; 56] = hash.as_bytes().try_into().unwrap();
        assert!(!trojan_check_password(&bytes, "mypassword"));
    }

    #[test]
    fn test_trojan_check_password_empty() {
        let hash = password_hash("");
        let bytes: [u8; 56] = hash.as_bytes().try_into().unwrap();
        assert!(trojan_check_password(&bytes, ""));
        assert!(!trojan_check_password(&bytes, "notempty"));
    }
}
