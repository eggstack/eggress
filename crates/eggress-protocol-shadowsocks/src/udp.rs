use rand::RngCore;

use crate::address::{decode_address, encode_address};
use crate::error::ShadowsocksError;
use crate::method::CipherMethod;
use aes_gcm::{aead::Aead, Aes128Gcm, Aes256Gcm, KeyInit, Nonce};
use chacha20poly1305::ChaCha20Poly1305;
use eggress_core::TargetAddr;

/// Encode a Shadowsocks UDP packet.
///
/// Packet format: salt (16 bytes) + nonce (12 bytes) + encrypted(address + payload)
///
/// The salt allows the receiver to derive the same subkey from the password.
pub fn encode_udp_packet(
    method: CipherMethod,
    key: &[u8],
    target: &TargetAddr,
    payload: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    let nonce_size = method.nonce_size();

    // Generate random nonce
    let mut nonce = vec![0u8; nonce_size];
    rand::thread_rng().fill_bytes(&mut nonce);

    // Build plaintext: address + payload
    let address = encode_address(target);
    let mut plaintext = Vec::with_capacity(address.len() + payload.len());
    plaintext.extend_from_slice(&address);
    plaintext.extend_from_slice(payload);

    // Encrypt with the pre-derived key
    let ciphertext = aead_encrypt(method, key, &nonce, &plaintext)?;

    // Build output: nonce + ciphertext
    let mut output = Vec::with_capacity(nonce_size + ciphertext.len());
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

/// Decode a Shadowsocks UDP packet.
///
/// Input: nonce (12 bytes) + encrypted(address + payload)
/// Returns: (target address, payload)
pub fn decode_udp_packet(
    method: CipherMethod,
    key: &[u8],
    packet: &[u8],
) -> Result<(TargetAddr, Vec<u8>), ShadowsocksError> {
    let nonce_size = method.nonce_size();
    let tag_size = method.tag_size();

    // Minimum packet: nonce + tag (for empty address + empty payload)
    let min_size = nonce_size + tag_size;
    if packet.len() < min_size {
        return Err(ShadowsocksError::DecryptionFailed(
            "packet too short".into(),
        ));
    }

    // Extract nonce and ciphertext
    let nonce = &packet[..nonce_size];
    let ciphertext = &packet[nonce_size..];

    // Decrypt
    let plaintext = aead_decrypt(method, key, nonce, ciphertext)?;

    // Parse address from plaintext
    let (target, addr_len) = decode_address(&plaintext)?;
    let payload = plaintext[addr_len..].to_vec();

    Ok((target, payload))
}

/// Internal AEAD encryption.
fn aead_encrypt(
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

/// Internal AEAD decryption.
fn aead_decrypt(
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

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::{TargetAddr, TargetHost};

    #[test]
    fn test_encode_decode_roundtrip_ipv4() {
        let key = b"0123456789abcdef0123456789abcdef";
        let target = TargetAddr {
            host: TargetHost::Ip("192.168.1.1".parse().unwrap()),
            port: 8080,
        };
        let payload = b"hello shadowsocks udp";

        let packet = encode_udp_packet(CipherMethod::Aes256Gcm, key, &target, payload).unwrap();
        let (decoded_target, decoded_payload) =
            decode_udp_packet(CipherMethod::Aes256Gcm, key, &packet).unwrap();

        assert_eq!(decoded_target, target);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_encode_decode_roundtrip_ipv6() {
        let key = b"0123456789abcdef0123456789abcdef";
        let target = TargetAddr {
            host: TargetHost::Ip("::1".parse().unwrap()),
            port: 443,
        };
        let payload = b"ipv6 udp test";

        let packet = encode_udp_packet(CipherMethod::Aes256Gcm, key, &target, payload).unwrap();
        let (decoded_target, decoded_payload) =
            decode_udp_packet(CipherMethod::Aes256Gcm, key, &packet).unwrap();

        assert_eq!(decoded_target, target);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_encode_decode_roundtrip_domain() {
        let key = b"0123456789abcdef0123456789abcdef";
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };
        let payload = b"domain udp test";

        let packet = encode_udp_packet(CipherMethod::Aes256Gcm, key, &target, payload).unwrap();
        let (decoded_target, decoded_payload) =
            decode_udp_packet(CipherMethod::Aes256Gcm, key, &packet).unwrap();

        assert_eq!(decoded_target, target);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_encode_decode_all_methods() {
        let methods = [
            CipherMethod::Aes128Gcm,
            CipherMethod::Aes256Gcm,
            CipherMethod::ChaCha20IetfPoly1305,
        ];
        let keys: Vec<&[u8]> = vec![
            b"0123456789abcdef",
            b"0123456789abcdef0123456789abcdef",
            b"0123456789abcdef0123456789abcdef",
        ];

        for (method, key) in methods.iter().zip(keys.iter()) {
            let target = TargetAddr {
                host: TargetHost::Domain("test.example.com".to_string()),
                port: 9090,
            };
            let payload = b"method-specific test";

            let packet = encode_udp_packet(*method, key, &target, payload).unwrap();
            let (decoded_target, decoded_payload) =
                decode_udp_packet(*method, key, &packet).unwrap();

            assert_eq!(decoded_target, target, "method {} failed", method);
            assert_eq!(decoded_payload, payload, "method {} failed", method);
        }
    }

    #[test]
    fn test_tampered_packet_fails() {
        let key = b"0123456789abcdef0123456789abcdef";
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };
        let payload = b"tamper test";

        let mut packet = encode_udp_packet(CipherMethod::Aes256Gcm, key, &target, payload).unwrap();

        // Tamper with the ciphertext (after the nonce)
        let last = packet.len() - 1;
        packet[last] ^= 0xFF;

        assert!(decode_udp_packet(CipherMethod::Aes256Gcm, key, &packet).is_err());
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = b"0123456789abcdef0123456789abcdef";
        let key2 = b"fedcba9876543210fedcba9876543210";
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };
        let payload = b"wrong key test";

        let packet = encode_udp_packet(CipherMethod::Aes256Gcm, key1, &target, payload).unwrap();
        assert!(decode_udp_packet(CipherMethod::Aes256Gcm, key2, &packet).is_err());
    }

    #[test]
    fn test_empty_payload() {
        let key = b"0123456789abcdef0123456789abcdef";
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };

        let packet = encode_udp_packet(CipherMethod::Aes256Gcm, key, &target, b"").unwrap();
        let (decoded_target, decoded_payload) =
            decode_udp_packet(CipherMethod::Aes256Gcm, key, &packet).unwrap();

        assert_eq!(decoded_target, target);
        assert!(decoded_payload.is_empty());
    }

    #[test]
    fn test_large_payload() {
        let key = b"0123456789abcdef0123456789abcdef";
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };
        let payload = vec![0xABu8; 1400]; // Typical UDP payload size

        let packet = encode_udp_packet(CipherMethod::Aes256Gcm, key, &target, &payload).unwrap();
        let (decoded_target, decoded_payload) =
            decode_udp_packet(CipherMethod::Aes256Gcm, key, &packet).unwrap();

        assert_eq!(decoded_target, target);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_packet_too_short() {
        let key = b"0123456789abcdef0123456789abcdef";
        // Packet shorter than nonce + tag
        let packet = vec![0u8; 5];
        assert!(decode_udp_packet(CipherMethod::Aes256Gcm, key, &packet).is_err());
    }

    #[test]
    fn test_unique_nonces() {
        let key = b"0123456789abcdef0123456789abcdef";
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };
        let payload = b"nonce uniqueness test";

        let p1 = encode_udp_packet(CipherMethod::Aes256Gcm, key, &target, payload).unwrap();
        let p2 = encode_udp_packet(CipherMethod::Aes256Gcm, key, &target, payload).unwrap();

        // Different random nonces should produce different packets
        assert_ne!(p1, p2);
    }
}
