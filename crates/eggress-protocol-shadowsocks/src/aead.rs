use aes_gcm::{
    aead::{consts::U12, Aead},
    aes::Aes192,
    Aes128Gcm, Aes256Gcm, AesGcm, KeyInit, Nonce,
};
use chacha20poly1305::ChaCha20Poly1305;

use crate::error::ShadowsocksError;
use crate::method::CipherMethod;

/// Maximum plaintext payload per AEAD chunk in the standard Shadowsocks framing.
///
/// pproxy limits each AEAD packet to 16 KiB - 1 bytes even though the length
/// field itself is a u16.
pub const MAX_CHUNK_PAYLOAD: usize = 16 * 1024 - 1;

type Aes192Gcm = AesGcm<Aes192, U12>;

pub(crate) enum AeadCipher {
    Aes128(Aes128Gcm),
    Aes192(Aes192Gcm),
    Aes256(Aes256Gcm),
    ChaCha20(ChaCha20Poly1305),
}

impl AeadCipher {
    pub(crate) fn new(method: CipherMethod, key: &[u8]) -> Result<Self, ShadowsocksError> {
        match method {
            CipherMethod::Aes128Gcm => Aes128Gcm::new_from_slice(key)
                .map(Self::Aes128)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
            CipherMethod::Aes192Gcm => Aes192Gcm::new_from_slice(key)
                .map(Self::Aes192)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
            CipherMethod::Aes256Gcm => Aes256Gcm::new_from_slice(key)
                .map(Self::Aes256)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
            CipherMethod::ChaCha20IetfPoly1305 => ChaCha20Poly1305::new_from_slice(key)
                .map(Self::ChaCha20)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
        }
    }

    pub(crate) fn encrypt(
        &self,
        nonce: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, ShadowsocksError> {
        let nonce = Nonce::from_slice(nonce);
        match self {
            Self::Aes128(cipher) => cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
            Self::Aes192(cipher) => cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
            Self::Aes256(cipher) => cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
            Self::ChaCha20(cipher) => cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
        }
    }

    pub(crate) fn decrypt(
        &self,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, ShadowsocksError> {
        let nonce = Nonce::from_slice(nonce);
        match self {
            Self::Aes128(cipher) => cipher
                .decrypt(nonce, ciphertext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
            Self::Aes192(cipher) => cipher
                .decrypt(nonce, ciphertext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
            Self::Aes256(cipher) => cipher
                .decrypt(nonce, ciphertext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
            Self::ChaCha20(cipher) => cipher
                .decrypt(nonce, ciphertext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string())),
        }
    }
}

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
    let mut salt_buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut salt_buf[..salt_size]);
    let salt = &salt_buf[..salt_size];

    // Derive subkey
    let subkey = method.derive_key(key, salt)?;

    // Generate nonce (12 bytes, starts at 0)
    let nonce_bytes = vec![0u8; nonce_size];

    // Encrypt
    let ciphertext = aead_encrypt(method, &subkey, &nonce_bytes, plaintext)?;

    // Build output: salt + ciphertext
    let mut output = Vec::with_capacity(salt_size + ciphertext.len());
    output.extend_from_slice(salt);
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
    if plaintext.len() > MAX_CHUNK_PAYLOAD {
        return Err(ShadowsocksError::Other(format!(
            "plaintext too large for AEAD chunk: {} bytes (max {})",
            plaintext.len(),
            MAX_CHUNK_PAYLOAD
        )));
    }
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
    let mut payload_nonce = [0u8; 12];
    nonce_increment(nonce, &mut payload_nonce)?;

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
    let len_block_size = 2 + tag_size;

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
    if payload_len > MAX_CHUNK_PAYLOAD {
        return Err(ShadowsocksError::DecryptionFailed(format!(
            "payload length {} exceeds maximum {}",
            payload_len, MAX_CHUNK_PAYLOAD
        )));
    }
    let expected_total = len_block_size + payload_len + tag_size;

    if data.len() < expected_total {
        return Err(ShadowsocksError::DecryptionFailed(format!(
            "insufficient data: expected {} bytes, got {}",
            expected_total,
            data.len(),
        )));
    }

    // Compute payload nonce = nonce + 1
    let mut payload_nonce = [0u8; 12];
    nonce_increment(nonce, &mut payload_nonce)?;

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

/// Encrypt a sequence of pproxy AEAD chunks with consecutive nonces.
pub(crate) fn encrypt_standard_chunks(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    let mut output = Vec::new();
    let mut current_nonce = [0u8; 12];
    current_nonce.copy_from_slice(nonce);
    for chunk in plaintext.chunks(MAX_CHUNK_PAYLOAD) {
        output.extend_from_slice(&encrypt_chunk_standard(method, key, &current_nonce, chunk)?);
        let mut length_nonce = [0u8; 12];
        nonce_increment(&current_nonce, &mut length_nonce)?;
        nonce_increment(&length_nonce, &mut current_nonce)?;
    }
    Ok(output)
}

/// Decrypt a sequence of pproxy AEAD chunks with consecutive nonces.
pub(crate) fn decrypt_standard_chunks(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    let tag_size = method.tag_size();
    let len_block_size = 2 + tag_size;
    let mut current_nonce = [0u8; 12];
    current_nonce.copy_from_slice(nonce);
    let mut offset = 0;
    let mut plaintext = Vec::new();

    while offset < data.len() {
        if data.len() - offset < len_block_size {
            return Err(ShadowsocksError::DecryptionFailed(
                "data too short for pproxy length block".into(),
            ));
        }
        let length_plaintext = aead_decrypt_raw(
            method,
            key,
            &current_nonce,
            &data[offset..offset + len_block_size],
        )?;
        let payload_len = u16::from_be_bytes([length_plaintext[0], length_plaintext[1]]) as usize;
        if payload_len > MAX_CHUNK_PAYLOAD {
            return Err(ShadowsocksError::DecryptionFailed(format!(
                "payload length {} exceeds maximum {}",
                payload_len, MAX_CHUNK_PAYLOAD
            )));
        }
        offset += len_block_size;
        let payload_wire_len = payload_len + tag_size;
        if data.len() - offset < payload_wire_len {
            return Err(ShadowsocksError::DecryptionFailed(
                "data too short for pproxy payload block".into(),
            ));
        }
        let mut length_nonce = [0u8; 12];
        nonce_increment(&current_nonce, &mut length_nonce)?;
        let payload = aead_decrypt_raw(
            method,
            key,
            &length_nonce,
            &data[offset..offset + payload_wire_len],
        )?;
        plaintext.extend_from_slice(&payload);
        offset += payload_wire_len;
        nonce_increment(&length_nonce, &mut current_nonce)?;
    }

    Ok(plaintext)
}

/// Increment a nonce by 1 (little-endian in first 8 bytes, increment first byte with carry).
fn nonce_increment(nonce: &[u8], result: &mut [u8]) -> Result<(), ShadowsocksError> {
    if nonce.len() != result.len() {
        return Err(ShadowsocksError::Other("nonce size mismatch".into()));
    }
    result.copy_from_slice(nonce);
    let end = result.len().min(8);
    for byte in result[..end].iter_mut() {
        let (val, carry) = byte.overflowing_add(1);
        *byte = val;
        if !carry {
            return Ok(());
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
    AeadCipher::new(method, key)?.encrypt(nonce, plaintext)
}

/// Raw AEAD decryption without salt derivation (for address header).
pub fn aead_decrypt_raw(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    AeadCipher::new(method, key)?.decrypt(nonce, ciphertext)
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
    fn test_encrypt_decrypt_roundtrip_aes192() {
        let key = b"0123456789abcdef01234567";
        let plaintext = b"hello shadowsocks";
        let encrypted = encrypt_frame(CipherMethod::Aes192Gcm, key, plaintext).unwrap();
        let decrypted = decrypt_frame(CipherMethod::Aes192Gcm, key, &encrypted).unwrap();
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
    fn test_encrypt_decrypt_chunk_standard_roundtrip_aes192() {
        let key = b"0123456789abcdef01234567";
        let nonce = vec![0u8; 12];
        let payload = b"aes-192 standard chunk test";
        let wire = encrypt_chunk_standard(CipherMethod::Aes192Gcm, key, &nonce, payload).unwrap();
        let decrypted =
            decrypt_chunk_standard(CipherMethod::Aes192Gcm, key, &nonce, &wire).unwrap();
        assert_eq!(decrypted, payload);
    }

    #[test]
    fn test_pproxy_known_answer_vectors() {
        // Captured from pproxy==2.7.9's AEADCipher implementation with the
        // fixed password, method-sized salt, zero nonce, and plaintext below.
        let password = b"phase1-vector-password";
        let plaintext = b"phase-1-known-answer";
        let cases = [
            (
                CipherMethod::Aes128Gcm,
                "000102030405060708090a0b0c0d0e0f",
                "9c9ad70264cd061c56f1e3492fbf1528",
                "a8e3c5be9630d97db3fdc8024c606d78aba455a8d0d5c7d7c5148cc4e076ee7fc2367b6f",
            ),
            (
                CipherMethod::Aes192Gcm,
                "000102030405060708090a0b0c0d0e0f1011121314151617",
                "7b56c2ce83b11c5ca9d5401b08d0cea7bcc15428500352d6",
                "e4345ba47ae93b62f6a6450a55e96f01803c46b46500f30d290566a1617c5b97f16995d5",
            ),
            (
                CipherMethod::Aes256Gcm,
                "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
                "9e3a9a86f6293e1d6ddb2f4285a818bb9ebb9a9fe08498735ba4967425f88fc9",
                "506e8c5971d7e254b42b6fcc6c7919c11baf60bd7406966bfaf1838bbdcd1d4ade826c22",
            ),
            (
                CipherMethod::ChaCha20IetfPoly1305,
                "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
                "9e3a9a86f6293e1d6ddb2f4285a818bb9ebb9a9fe08498735ba4967425f88fc9",
                "cdaa76308bb8e130e2f4d351494ca1efca87098c744a0c2b8afe27f77f1999dbd13bee5e",
            ),
        ];

        for (method, salt_hex, subkey_hex, ciphertext_hex) in cases {
            let salt = hex_bytes(salt_hex);
            let expected_subkey = hex_bytes(subkey_hex);
            let expected_ciphertext = hex_bytes(ciphertext_hex);
            let subkey = method.derive_key(password, &salt).unwrap();
            assert_eq!(subkey, expected_subkey, "subkey mismatch for {method}");
            let ciphertext = aead_encrypt_raw(method, &subkey, &[0u8; 12], plaintext).unwrap();
            assert_eq!(
                ciphertext, expected_ciphertext,
                "ciphertext mismatch for {method}"
            );
            assert_eq!(
                aead_decrypt_raw(method, &subkey, &[0u8; 12], &ciphertext).unwrap(),
                plaintext
            );
        }
    }

    fn hex_bytes(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
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
        let mut result = vec![0u8; nonce.len()];
        nonce_increment(&nonce, &mut result).unwrap();
        assert_eq!(result[0], 1);
    }

    #[test]
    fn test_nonce_increment_carry() {
        let mut nonce = vec![0u8; 12];
        nonce[0] = 0xFF;
        let mut result = vec![0u8; nonce.len()];
        nonce_increment(&nonce, &mut result).unwrap();
        assert_eq!(result[0], 0);
        assert_eq!(result[1], 1);
    }

    #[test]
    fn test_nonce_increment_overflow() {
        let nonce = vec![0xFFu8; 12];
        let mut result = vec![0u8; nonce.len()];
        let result = nonce_increment(&nonce, &mut result);
        assert!(result.is_err());
    }
}
