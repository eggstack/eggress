use aes_gcm::{aead::Aead, Aes128Gcm, Aes256Gcm, KeyInit, Nonce};
use chacha20poly1305::ChaCha20Poly1305;

use crate::error::ShadowsocksError;
use crate::method::CipherMethod;

/// Encrypt plaintext using AEAD with a random salt.
///
/// Returns: salt + encrypted(plaintext)
pub fn encrypt_frame(
    method: CipherMethod,
    key: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    use rand::RngCore;

    let salt_size = method.salt_size();
    let nonce_size = method.nonce_size();

    // Generate random salt
    let mut salt = vec![0u8; salt_size];
    rand::thread_rng().fill_bytes(&mut salt);

    // Derive subkey
    let subkey = method.derive_key(key, &salt);

    // Generate nonce (12 bytes, starts at 0)
    let nonce_bytes = vec![0u8; nonce_size];

    // Encrypt
    let ciphertext = aead_encrypt(method, &subkey, &nonce_bytes, plaintext)?;

    // Build output: salt + ciphertext
    let mut output = Vec::with_capacity(salt_size + ciphertext.len());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

/// Decrypt ciphertext using AEAD. The salt must be prepended to the ciphertext.
///
/// Input: salt + encrypted(plaintext)
/// Returns: plaintext
pub fn decrypt_frame(
    method: CipherMethod,
    key: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    let salt_size = method.salt_size();
    let nonce_size = method.nonce_size();

    if data.len() < salt_size {
        return Err(ShadowsocksError::DecryptionFailed(
            "data too short for salt".into(),
        ));
    }

    // Extract salt
    let salt = &data[..salt_size];
    let ciphertext = &data[salt_size..];

    // Derive subkey
    let subkey = method.derive_key(key, salt);

    // Generate nonce (12 bytes, starts at 0)
    let nonce_bytes = vec![0u8; nonce_size];

    // Decrypt
    aead_decrypt(method, &subkey, &nonce_bytes, ciphertext)
}

/// Encrypt a chunk with AEAD (for streaming).
///
/// Input: plaintext
/// Returns: encrypted(2-byte length + plaintext)
pub fn encrypt_chunk(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    // Prepend length (2 bytes big-endian)
    let len = plaintext.len() as u16;
    let mut payload = Vec::with_capacity(2 + plaintext.len());
    payload.extend_from_slice(&len.to_be_bytes());
    payload.extend_from_slice(plaintext);

    aead_encrypt(method, key, nonce, &payload)
}

/// Decrypt a chunk with AEAD (for streaming).
///
/// Input: encrypted(2-byte length + plaintext)
/// Returns: plaintext
pub fn decrypt_chunk(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    let plaintext = aead_decrypt(method, key, nonce, data)?;

    if plaintext.len() < 2 {
        return Err(ShadowsocksError::DecryptionFailed("chunk too short".into()));
    }

    let len = u16::from_be_bytes([plaintext[0], plaintext[1]]) as usize;
    if plaintext.len() < 2 + len {
        return Err(ShadowsocksError::DecryptionFailed(
            "chunk length mismatch".into(),
        ));
    }

    Ok(plaintext[2..2 + len].to_vec())
}

/// Raw AEAD encryption without salt derivation (for address header).
pub fn aead_encrypt_raw(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    let nonce = Nonce::from_slice(nonce);

    match method {
        CipherMethod::Aes128Gcm => {
            let cipher = Aes128Gcm::new_from_slice(key)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))?;
            cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))
        }
        CipherMethod::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))?;
            cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))
        }
        CipherMethod::ChaCha20IetfPoly1305 => {
            let cipher = ChaCha20Poly1305::new_from_slice(key)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))?;
            cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))
        }
    }
}

/// Raw AEAD decryption without salt derivation (for address header).
pub fn aead_decrypt_raw(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    let nonce = Nonce::from_slice(nonce);

    match method {
        CipherMethod::Aes128Gcm => {
            let cipher = Aes128Gcm::new_from_slice(key)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))?;
            cipher
                .decrypt(nonce, ciphertext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))
        }
        CipherMethod::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))?;
            cipher
                .decrypt(nonce, ciphertext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))
        }
        CipherMethod::ChaCha20IetfPoly1305 => {
            let cipher = ChaCha20Poly1305::new_from_slice(key)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))?;
            cipher
                .decrypt(nonce, ciphertext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))
        }
    }
}

/// Internal AEAD encryption.
fn aead_encrypt(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    aead_encrypt_raw(method, key, nonce, plaintext)
}

/// Internal AEAD decryption.
fn aead_decrypt(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    aead_decrypt_raw(method, key, nonce, ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip_aes128() {
        let key = b"0123456789abcdef";
        let plaintext = b"hello shadowsocks";
        let encrypted = encrypt_frame(CipherMethod::Aes128Gcm, key, plaintext).unwrap();
        let decrypted = decrypt_frame(CipherMethod::Aes128Gcm, key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip_aes256() {
        let key = b"0123456789abcdef0123456789abcdef";
        let plaintext = b"hello shadowsocks";
        let encrypted = encrypt_frame(CipherMethod::Aes256Gcm, key, plaintext).unwrap();
        let decrypted = decrypt_frame(CipherMethod::Aes256Gcm, key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip_chacha20() {
        let key = b"0123456789abcdef0123456789abcdef";
        let plaintext = b"hello shadowsocks";
        let encrypted = encrypt_frame(CipherMethod::ChaCha20IetfPoly1305, key, plaintext).unwrap();
        let decrypted = decrypt_frame(CipherMethod::ChaCha20IetfPoly1305, key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let key = b"0123456789abcdef";
        let plaintext = b"hello shadowsocks";
        let mut encrypted = encrypt_frame(CipherMethod::Aes128Gcm, key, plaintext).unwrap();
        // Tamper with ciphertext
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;
        assert!(decrypt_frame(CipherMethod::Aes128Gcm, key, &encrypted).is_err());
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = b"0123456789abcdef";
        let key2 = b"fedcba9876543210";
        let plaintext = b"hello shadowsocks";
        let encrypted = encrypt_frame(CipherMethod::Aes128Gcm, key1, plaintext).unwrap();
        assert!(decrypt_frame(CipherMethod::Aes128Gcm, key2, &encrypted).is_err());
    }

    #[test]
    fn test_encrypt_decrypt_chunk_roundtrip() {
        let key = b"0123456789abcdef";
        let nonce = [0u8; 12];
        let plaintext = b"chunk data";
        let encrypted = encrypt_chunk(CipherMethod::Aes128Gcm, key, &nonce, plaintext).unwrap();
        let decrypted = decrypt_chunk(CipherMethod::Aes128Gcm, key, &nonce, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_empty_plaintext() {
        let key = b"0123456789abcdef";
        let encrypted = encrypt_frame(CipherMethod::Aes128Gcm, key, b"").unwrap();
        let decrypted = decrypt_frame(CipherMethod::Aes128Gcm, key, &encrypted).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_large_plaintext() {
        let key = b"0123456789abcdef";
        let plaintext = vec![0xABu8; 65536];
        let encrypted = encrypt_frame(CipherMethod::Aes128Gcm, key, &plaintext).unwrap();
        let decrypted = decrypt_frame(CipherMethod::Aes128Gcm, key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonces_produce_different_ciphertext() {
        let key = b"0123456789abcdef";
        let plaintext = b"same data";
        let enc1 = encrypt_frame(CipherMethod::Aes128Gcm, key, plaintext).unwrap();
        let enc2 = encrypt_frame(CipherMethod::Aes128Gcm, key, plaintext).unwrap();
        // Different salts should produce different ciphertexts
        assert_ne!(enc1, enc2);
    }
}
