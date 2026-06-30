use aes_gcm::{aead::Aead, Aes128Gcm, Aes256Gcm, KeyInit, Nonce};
use chacha20poly1305::ChaCha20Poly1305;

use crate::error::ShadowsocksError;
use crate::method::CipherMethod;

/// Maximum plaintext payload per AEAD chunk in the standard Shadowsocks framing.
///
/// The length field is a u16, so the maximum payload is 65535 bytes.
pub const MAX_CHUNK_PAYLOAD: usize = 65535;

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
    let subkey = method.derive_key(key, &salt)?;

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
    let subkey = method.derive_key(key, salt)?;

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
    let len: u16 = plaintext.len().try_into().map_err(|_| {
        ShadowsocksError::Other(format!(
            "plaintext too large for AEAD chunk: {} bytes (max 65535)",
            plaintext.len()
        ))
    })?;
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

/// Encrypt a standard Shadowsocks AEAD TCP chunk.
///
/// Wire format: AEAD(len_u16_be, nonce) + AEAD(payload, nonce+1)
/// Returns the combined wire bytes (18 + payload.len() + 16 bytes).
pub fn encrypt_chunk_standard(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    payload: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    if payload.len() > MAX_CHUNK_PAYLOAD {
        return Err(ShadowsocksError::Other(format!(
            "payload too large for standard AEAD chunk: {} bytes (max {})",
            payload.len(),
            MAX_CHUNK_PAYLOAD,
        )));
    }

    let len_bytes = (payload.len() as u16).to_be_bytes();

    // Encrypt length block with nonce
    let len_ct = aead_encrypt_raw(method, key, nonce, &len_bytes)?;

    // Compute payload nonce = nonce + 1 (increment last byte with carry)
    let payload_nonce = nonce_increment(nonce)?;

    // Encrypt payload with payload nonce
    let payload_ct = aead_encrypt_raw(method, key, &payload_nonce, payload)?;

    let mut output = Vec::with_capacity(len_ct.len() + payload_ct.len());
    output.extend_from_slice(&len_ct);
    output.extend_from_slice(&payload_ct);
    Ok(output)
}

/// Decrypt a standard Shadowsocks AEAD TCP chunk.
///
/// Input: the full wire bytes of one chunk (length block + payload block).
/// Returns: the decrypted plaintext payload.
pub fn decrypt_chunk_standard(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    let tag_size = method.tag_size();
    let len_block_size = 2 + tag_size; // 18 bytes

    if data.len() < len_block_size {
        return Err(ShadowsocksError::DecryptionFailed(
            "data too short for length block".into(),
        ));
    }

    // Decrypt length block (first 18 bytes)
    let len_plaintext = aead_decrypt_raw(method, key, nonce, &data[..len_block_size])?;
    if len_plaintext.len() != 2 {
        return Err(ShadowsocksError::DecryptionFailed(
            "length block plaintext invalid".into(),
        ));
    }

    let payload_len = u16::from_be_bytes([len_plaintext[0], len_plaintext[1]]) as usize;
    let expected_total = len_block_size + payload_len + tag_size;

    if data.len() < expected_total {
        return Err(ShadowsocksError::DecryptionFailed(format!(
            "insufficient data: expected {} bytes, got {}",
            expected_total,
            data.len(),
        )));
    }

    // Compute payload nonce = nonce + 1
    let payload_nonce = nonce_increment(nonce)?;

    // Decrypt payload block
    let payload_start = len_block_size;
    let payload_end = payload_start + payload_len + tag_size;
    let plaintext = aead_decrypt_raw(
        method,
        key,
        &payload_nonce,
        &data[payload_start..payload_end],
    )?;

    Ok(plaintext)
}

/// Increment a nonce by 1 (little-endian in first 8 bytes, increment first byte with carry).
fn nonce_increment(nonce: &[u8]) -> Result<Vec<u8>, ShadowsocksError> {
    let mut result = nonce.to_vec();
    let end = result.len().min(8);
    for byte in result[..end].iter_mut() {
        let (val, carry) = byte.overflowing_add(1);
        *byte = val;
        if !carry {
            return Ok(result);
        }
    }
    Err(ShadowsocksError::Other("nonce increment overflow".into()))
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

    #[test]
    fn test_encrypt_decrypt_chunk_standard_roundtrip_aes128() {
        let key = b"0123456789abcdef";
        let nonce = vec![0u8; 12];
        let payload = b"hello shadowsocks standard";
        let wire = encrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, payload).unwrap();
        // 18 (len block) + 26 (payload) + 16 (tag) = 60
        assert_eq!(wire.len(), 18 + payload.len() + 16);
        let decrypted =
            decrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, &wire).unwrap();
        assert_eq!(decrypted, payload);
    }

    #[test]
    fn test_encrypt_decrypt_chunk_standard_roundtrip_aes256() {
        let key = b"0123456789abcdef0123456789abcdef";
        let nonce = vec![0u8; 12];
        let payload = b"standard chunk test";
        let wire = encrypt_chunk_standard(CipherMethod::Aes256Gcm, key, &nonce, payload).unwrap();
        let decrypted =
            decrypt_chunk_standard(CipherMethod::Aes256Gcm, key, &nonce, &wire).unwrap();
        assert_eq!(decrypted, payload);
    }

    #[test]
    fn test_encrypt_decrypt_chunk_standard_roundtrip_chacha20() {
        let key = b"0123456789abcdef0123456789abcdef";
        let nonce = vec![0u8; 12];
        let payload = b"chacha standard chunk";
        let wire = encrypt_chunk_standard(CipherMethod::ChaCha20IetfPoly1305, key, &nonce, payload)
            .unwrap();
        let decrypted =
            decrypt_chunk_standard(CipherMethod::ChaCha20IetfPoly1305, key, &nonce, &wire).unwrap();
        assert_eq!(decrypted, payload);
    }

    #[test]
    fn test_encrypt_decrypt_chunk_standard_empty_payload() {
        let key = b"0123456789abcdef";
        let nonce = vec![0u8; 12];
        let payload = b"";
        let wire = encrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, payload).unwrap();
        // 18 (len block) + 0 (payload) + 16 (tag) = 34
        assert_eq!(wire.len(), 34);
        let decrypted =
            decrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, &wire).unwrap();
        assert_eq!(decrypted, payload);
    }

    #[test]
    fn test_encrypt_decrypt_chunk_standard_max_payload() {
        let key = b"0123456789abcdef";
        let nonce = vec![0u8; 12];
        let payload = vec![0xABu8; MAX_CHUNK_PAYLOAD];
        let wire = encrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, &payload).unwrap();
        assert_eq!(wire.len(), 18 + MAX_CHUNK_PAYLOAD + 16);
        let decrypted =
            decrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, &wire).unwrap();
        assert_eq!(decrypted, payload);
    }

    #[test]
    fn test_encrypt_chunk_standard_payload_too_large() {
        let key = b"0123456789abcdef";
        let nonce = vec![0u8; 12];
        let payload = vec![0xABu8; MAX_CHUNK_PAYLOAD + 1];
        let result = encrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, &payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_chunk_standard_tampered_length_block() {
        let key = b"0123456789abcdef";
        let nonce = vec![0u8; 12];
        let payload = b"secret data";
        let mut wire =
            encrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, payload).unwrap();
        // Tamper with the length block (first byte)
        wire[0] ^= 0xFF;
        let result = decrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, &wire);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_chunk_standard_tampered_payload_block() {
        let key = b"0123456789abcdef";
        let nonce = vec![0u8; 12];
        let payload = b"secret data";
        let mut wire =
            encrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, payload).unwrap();
        // Tamper with the payload block (byte after length block)
        wire[18] ^= 0xFF;
        let result = decrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, &wire);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_chunk_standard_too_short() {
        let key = b"0123456789abcdef";
        let nonce = vec![0u8; 12];
        let data = vec![0u8; 10]; // too short for length block (need 18)
        let result = decrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, &data);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_chunk_standard_wrong_key() {
        let key1 = b"0123456789abcdef";
        let key2 = b"fedcba9876543210";
        let nonce = vec![0u8; 12];
        let payload = b"secret data";
        let wire = encrypt_chunk_standard(CipherMethod::Aes128Gcm, key1, &nonce, payload).unwrap();
        let result = decrypt_chunk_standard(CipherMethod::Aes128Gcm, key2, &nonce, &wire);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_chunk_standard_wrong_nonce() {
        let key = b"0123456789abcdef";
        let nonce1 = vec![0u8; 12];
        let mut nonce2 = vec![0u8; 12];
        nonce2[0] = 1; // different nonce (little-endian)
        let payload = b"secret data";
        let wire = encrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce1, payload).unwrap();
        let result = decrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce2, &wire);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_decrypt_chunk_standard_sequential_nonces() {
        let key = b"0123456789abcdef";
        let mut nonce = vec![0u8; 12];
        nonce[0] = 1; // start at nonce 1 (little-endian)

        let payload1 = b"first chunk";
        let wire1 = encrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, payload1).unwrap();
        let dec1 = decrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, &wire1).unwrap();
        assert_eq!(dec1, payload1);

        // Advance nonce by 2 (one for length, one for payload)
        nonce[0] = 3;
        let payload2 = b"second chunk";
        let wire2 = encrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, payload2).unwrap();
        let dec2 = decrypt_chunk_standard(CipherMethod::Aes128Gcm, key, &nonce, &wire2).unwrap();
        assert_eq!(dec2, payload2);
    }

    #[test]
    fn test_nonce_increment_basic() {
        let nonce = vec![0u8; 12];
        let result = nonce_increment(&nonce).unwrap();
        assert_eq!(result[0], 1);
    }

    #[test]
    fn test_nonce_increment_carry() {
        let mut nonce = vec![0u8; 12];
        nonce[0] = 0xFF;
        let result = nonce_increment(&nonce).unwrap();
        assert_eq!(result[0], 0);
        assert_eq!(result[1], 1);
    }

    #[test]
    fn test_nonce_increment_overflow() {
        let nonce = vec![0xFFu8; 12];
        let result = nonce_increment(&nonce);
        assert!(result.is_err());
    }
}
