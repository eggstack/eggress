use hkdf::Hkdf;
use sha2::{Digest, Sha256};

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
            _ => Err(ShadowsocksError::UnsupportedMethod(s.to_string())),
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

    /// Derive an AEAD subkey from password and salt using HKDF-SHA256.
    pub fn derive_key(&self, password: &[u8], salt: &[u8]) -> Vec<u8> {
        let ikm = Sha256::digest(password);
        let hk = Hkdf::<Sha256>::new(Some(salt), &ikm);
        let mut key = vec![0u8; self.key_size()];
        hk.expand(b"ss-subkey", &mut key)
            .expect("HKDF expand failed");
        key
    }
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
        assert!(CipherMethod::parse_method("rc4").is_err());
        assert!(CipherMethod::parse_method("").is_err());
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
        let key1 = method.derive_key(password, salt);
        let key2 = method.derive_key(password, salt);
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn test_derive_key_different_salts() {
        let method = CipherMethod::Aes256Gcm;
        let password = b"test-password";
        let key1 = method.derive_key(password, b"0000000000000000");
        let key2 = method.derive_key(password, b"1111111111111111");
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
}
