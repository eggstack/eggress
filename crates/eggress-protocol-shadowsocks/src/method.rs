use hkdf::Hkdf;
use sha1::Sha1;

use crate::error::ShadowsocksError;

/// Supported Shadowsocks AEAD cipher methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherMethod {
    Aes128Gcm,
    Aes256Gcm,
    ChaCha20IetfPoly1305,
}

impl CipherMethod {
    /// Parse a method name string into a CipherMethod.
    pub fn parse_method(s: &str) -> Result<Self, ShadowsocksError> {
        match s.to_lowercase().as_str() {
            "aes-128-gcm" => Ok(CipherMethod::Aes128Gcm),
            "aes-256-gcm" => Ok(CipherMethod::Aes256Gcm),
            "chacha20-ietf-poly1305" => Ok(CipherMethod::ChaCha20IetfPoly1305),
            _ => {
                if is_legacy_method(s) {
                    Err(ShadowsocksError::LegacyMethodUnsupported(s.to_string()))
                } else {
                    Err(ShadowsocksError::UnsupportedMethod(s.to_string()))
                }
            }
        }
    }

    /// Key size in bytes.
    pub fn key_size(&self) -> usize {
        match self {
            CipherMethod::Aes128Gcm => 16,
            CipherMethod::Aes256Gcm => 32,
            CipherMethod::ChaCha20IetfPoly1305 => 32,
        }
    }

    /// Salt size in bytes.
    pub fn salt_size(&self) -> usize {
        16
    }

    /// Nonce size in bytes.
    pub fn nonce_size(&self) -> usize {
        12
    }

    /// Authentication tag size in bytes.
    pub fn tag_size(&self) -> usize {
        16
    }

    /// Derive an AEAD subkey from password and salt using HKDF-SHA1.
    ///
    /// Uses `EVP_BytesToKey(password)` as IKM (matching OpenSSL/shadowsocks-rust),
    /// then HKDF-SHA1 with info="ss-subkey" per SIP003.
    pub fn derive_key(&self, password: &[u8], salt: &[u8]) -> Result<Vec<u8>, ShadowsocksError> {
        let ikm = evp_bytes_to_key(password);
        let hk = Hkdf::<Sha1>::new(Some(salt), &ikm);
        let mut key = vec![0u8; self.key_size()];
        hk.expand(b"ss-subkey", &mut key)
            .map_err(|e| ShadowsocksError::Other(format!("HKDF expand failed: {e}")))?;
        Ok(key)
    }
}

/// Known legacy Shadowsocks stream cipher method names.
///
/// These are recognized for diagnostic purposes but NOT supported.
/// Legacy stream ciphers lack authentication and are vulnerable to bit-flipping attacks.
pub fn is_legacy_method(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "aes-128-ctr"
            | "aes-192-ctr"
            | "aes-256-ctr"
            | "aes-128-cfb"
            | "aes-192-cfb"
            | "aes-256-cfb"
            | "aes-128-cfb1"
            | "aes-192-cfb1"
            | "aes-256-cfb1"
            | "aes-128-cfb8"
            | "aes-192-cfb8"
            | "aes-256-cfb8"
            | "aes-128-cfb11"
            | "aes-192-cfb11"
            | "aes-256-cfb11"
            | "rc4"
            | "rc4-md5"
            | "rc4-md5-6"
            | "chacha20-ietf"
            | "xchacha20"
            | "salsa20"
            | "xsalsa20"
            | "seed-cfb"
            | "camellia-128-cfb"
            | "camellia-192-cfb"
            | "camellia-256-cfb"
            | "bf-cfb"
            | "cast5-cfb"
            | "des-cfb"
            | "idea-cfb"
            | "rc2-cfb"
            | "blowfish-cfb"
    )
}

impl std::fmt::Display for CipherMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CipherMethod::Aes128Gcm => write!(f, "aes-128-gcm"),
            CipherMethod::Aes256Gcm => write!(f, "aes-256-gcm"),
            CipherMethod::ChaCha20IetfPoly1305 => write!(f, "chacha20-ietf-poly1305"),
        }
    }
}

/// OpenSSL `EVP_BytesToKey` with MD5 and no salt (type 0).
///
/// Used by Shadowsocks to derive the initial keying material from a password.
/// Produces `d1 || d2 || ...` where `d1 = MD5(password)`, `d2 = MD5(d1 + password)`, etc.
fn evp_bytes_to_key(password: &[u8]) -> Vec<u8> {
    use md5::Digest as _;
    use md5::Md5;

    let mut key = Vec::new();
    let mut prev = Vec::new();

    // Produce enough key material (up to 48 bytes covers all AEAD key sizes)
    while key.len() < 48 {
        let mut hasher = Md5::new();
        hasher.update(&prev);
        hasher.update(password);
        let digest = hasher.finalize();
        prev = digest.to_vec();
        key.extend_from_slice(&prev);
    }

    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_aes_128_gcm() {
        assert_eq!(
            CipherMethod::parse_method("aes-128-gcm").unwrap(),
            CipherMethod::Aes128Gcm
        );
        assert_eq!(
            CipherMethod::parse_method("AES-128-GCM").unwrap(),
            CipherMethod::Aes128Gcm
        );
    }

    #[test]
    fn test_parse_aes_256_gcm() {
        assert_eq!(
            CipherMethod::parse_method("aes-256-gcm").unwrap(),
            CipherMethod::Aes256Gcm
        );
    }

    #[test]
    fn test_parse_chacha20() {
        assert_eq!(
            CipherMethod::parse_method("chacha20-ietf-poly1305").unwrap(),
            CipherMethod::ChaCha20IetfPoly1305
        );
    }

    #[test]
    fn test_parse_unknown() {
        assert!(CipherMethod::parse_method("").is_err());
    }

    #[test]
    fn test_legacy_method_detection() {
        assert!(is_legacy_method("aes-128-ctr"));
        assert!(is_legacy_method("aes-256-cfb"));
        assert!(is_legacy_method("rc4"));
        assert!(is_legacy_method("rc4-md5"));
        assert!(is_legacy_method("RC4"));
        assert!(!is_legacy_method("aes-128-gcm"));
        assert!(!is_legacy_method("aes-256-gcm"));
        assert!(!is_legacy_method("chacha20-ietf-poly1305"));
        assert!(!is_legacy_method("totally-unknown"));
    }

    #[test]
    fn test_parse_legacy_method_gives_legacy_error() {
        match CipherMethod::parse_method("aes-128-ctr") {
            Err(ShadowsocksError::LegacyMethodUnsupported(m)) => assert_eq!(m, "aes-128-ctr"),
            other => panic!("expected LegacyMethodUnsupported, got {:?}", other),
        }
    }

    #[test]
    fn test_key_sizes() {
        assert_eq!(CipherMethod::Aes128Gcm.key_size(), 16);
        assert_eq!(CipherMethod::Aes256Gcm.key_size(), 32);
        assert_eq!(CipherMethod::ChaCha20IetfPoly1305.key_size(), 32);
    }

    #[test]
    fn test_salt_nonce_tag_sizes() {
        for method in [
            CipherMethod::Aes128Gcm,
            CipherMethod::Aes256Gcm,
            CipherMethod::ChaCha20IetfPoly1305,
        ] {
            assert_eq!(method.salt_size(), 16);
            assert_eq!(method.nonce_size(), 12);
            assert_eq!(method.tag_size(), 16);
        }
    }

    #[test]
    fn test_derive_key_deterministic() {
        let method = CipherMethod::Aes256Gcm;
        let password = b"test-password";
        let salt = b"0123456789abcdef";
        let key1 = method.derive_key(password, salt).unwrap();
        let key2 = method.derive_key(password, salt).unwrap();
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn test_derive_key_different_salts() {
        let method = CipherMethod::Aes256Gcm;
        let password = b"test-password";
        let key1 = method.derive_key(password, b"0000000000000000").unwrap();
        let key2 = method.derive_key(password, b"1111111111111111").unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_display() {
        assert_eq!(CipherMethod::Aes128Gcm.to_string(), "aes-128-gcm");
        assert_eq!(CipherMethod::Aes256Gcm.to_string(), "aes-256-gcm");
        assert_eq!(
            CipherMethod::ChaCha20IetfPoly1305.to_string(),
            "chacha20-ietf-poly1305"
        );
    }

    #[test]
    fn test_evp_bytes_to_key() {
        // Verified against Python's OpenSSL EVP_BytesToKey for "testpass"
        let key = evp_bytes_to_key(b"testpass");
        // First 16 bytes (for AES-128)
        assert_eq!(
            &key[..16],
            &[
                0x17, 0x9a, 0xd4, 0x5c, 0x6c, 0xe2, 0xcb, 0x97, 0xcf, 0x10, 0x29, 0xe2, 0x12, 0x04,
                0x6e, 0x81
            ]
        );
        // Full 32 bytes (for AES-256)
        assert_eq!(
            &key[..32],
            &[
                0x17, 0x9a, 0xd4, 0x5c, 0x6c, 0xe2, 0xcb, 0x97, 0xcf, 0x10, 0x29, 0xe2, 0x12, 0x04,
                0x6e, 0x81, 0x9c, 0x8f, 0x2c, 0x70, 0x95, 0xd2, 0x8b, 0xf6, 0x24, 0xab, 0x97, 0x14,
                0x3b, 0x51, 0xac, 0x4b
            ]
        );
    }
}
