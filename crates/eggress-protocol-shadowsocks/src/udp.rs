use crate::address::{decode_address, encode_address};
use crate::aead::{aead_decrypt_raw, aead_encrypt_raw};
use crate::error::ShadowsocksError;
use crate::method::CipherMethod;
use eggress_core::{TargetAddr, TargetHost};

/// Maximum domain name length per RFC 1035.
const MAX_DOMAIN_LEN: usize = 255;

/// Maximum Shadowsocks UDP datagram size.
const MAX_UDP_PACKET_SIZE: usize = 65535;

/// Encode a Shadowsocks UDP packet using the standard AEAD format.
///
/// Packet format: salt (16 bytes) + AEAD(address + payload, nonce=0)
///
/// The salt is generated randomly and used to derive the subkey from the password.
/// The receiver extracts the salt to derive the same subkey for decryption.
pub fn encode_udp_packet(
    method: CipherMethod,
    password: &[u8],
    target: &TargetAddr,
    payload: &[u8],
    salt: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    if salt.len() != method.salt_size() {
        return Err(ShadowsocksError::DecryptionFailed(format!(
            "salt must be {} bytes, got {}",
            method.salt_size(),
            salt.len()
        )));
    }

    // Derive subkey from password and salt
    let subkey = method.derive_key(password, salt)?;

    // Validate domain length before encoding
    if let TargetHost::Domain(ref domain) = target.host {
        if domain.len() > MAX_DOMAIN_LEN {
            return Err(ShadowsocksError::InvalidAddress(format!(
                "domain too long: {} exceeds maximum {}",
                domain.len(),
                MAX_DOMAIN_LEN
            )));
        }
    }

    // Build plaintext: address + payload
    let address = encode_address(target)?;
    let mut plaintext = Vec::with_capacity(address.len() + payload.len());
    plaintext.extend_from_slice(&address);
    plaintext.extend_from_slice(payload);

    // Encrypt with AEAD using nonce=0 (all zeros)
    let nonce_size = method.nonce_size();
    let nonce = vec![0u8; nonce_size];
    let ciphertext = aead_encrypt_raw(method, &subkey, &nonce, &plaintext)?;

    // Build output: salt + ciphertext
    let mut output = Vec::with_capacity(salt.len() + ciphertext.len());
    output.extend_from_slice(salt);
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

/// Decode a Shadowsocks UDP packet using the standard AEAD format.
///
/// Input: salt (16 bytes) + AEAD ciphertext
/// Returns: (target address, payload)
///
/// The salt is extracted from the packet prefix and used to derive the subkey.
pub fn decode_udp_packet(
    method: CipherMethod,
    password: &[u8],
    packet: &[u8],
) -> Result<(TargetAddr, Vec<u8>), ShadowsocksError> {
    let salt_size = method.salt_size();
    let tag_size = method.tag_size();

    // Minimum packet: salt + tag (for empty address + empty payload)
    let min_size = salt_size + tag_size;
    if packet.len() < min_size {
        return Err(ShadowsocksError::DecryptionFailed(
            "packet too short".into(),
        ));
    }

    if packet.len() > MAX_UDP_PACKET_SIZE {
        return Err(ShadowsocksError::DecryptionFailed(format!(
            "packet too large: {} exceeds maximum {}",
            packet.len(),
            MAX_UDP_PACKET_SIZE
        )));
    }

    // Extract salt and derive subkey
    let salt = &packet[..salt_size];
    let subkey = method.derive_key(password, salt)?;

    // Decrypt ciphertext with nonce=0
    let ciphertext = &packet[salt_size..];
    let nonce_size = method.nonce_size();
    let nonce = vec![0u8; nonce_size];
    let plaintext = aead_decrypt_raw(method, &subkey, &nonce, ciphertext)?;

    // Parse address from plaintext
    let (target, addr_len) = decode_address(&plaintext)?;

    // Validate domain length if applicable
    if let TargetHost::Domain(ref domain) = target.host {
        if domain.len() > MAX_DOMAIN_LEN {
            return Err(ShadowsocksError::InvalidAddress(format!(
                "domain too long: {} exceeds maximum {}",
                domain.len(),
                MAX_DOMAIN_LEN
            )));
        }
    }

    let payload = plaintext[addr_len..].to_vec();

    Ok((target, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::{TargetAddr, TargetHost};

    fn test_password() -> &'static [u8] {
        b"test-password-for-udp"
    }

    fn test_salt() -> [u8; 16] {
        [0x42u8; 16]
    }

    #[test]
    fn test_encode_decode_roundtrip_ipv4() {
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Ip("192.168.1.1".parse().unwrap()),
            port: 8080,
        };
        let payload = b"hello shadowsocks udp";

        let packet =
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, payload, &salt).unwrap();
        let (decoded_target, decoded_payload) =
            decode_udp_packet(CipherMethod::Aes256Gcm, password, &packet).unwrap();

        assert_eq!(decoded_target, target);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_encode_decode_roundtrip_ipv6() {
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Ip("::1".parse().unwrap()),
            port: 443,
        };
        let payload = b"ipv6 udp test";

        let packet =
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, payload, &salt).unwrap();
        let (decoded_target, decoded_payload) =
            decode_udp_packet(CipherMethod::Aes256Gcm, password, &packet).unwrap();

        assert_eq!(decoded_target, target);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_encode_decode_roundtrip_domain() {
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };
        let payload = b"domain udp test";

        let packet =
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, payload, &salt).unwrap();
        let (decoded_target, decoded_payload) =
            decode_udp_packet(CipherMethod::Aes256Gcm, password, &packet).unwrap();

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
        let password = test_password();
        let salt = test_salt();

        for method in methods.iter() {
            let target = TargetAddr {
                host: TargetHost::Domain("test.example.com".to_string()),
                port: 9090,
            };
            let payload = b"method-specific test";

            let packet = encode_udp_packet(*method, password, &target, payload, &salt).unwrap();
            let (decoded_target, decoded_payload) =
                decode_udp_packet(*method, password, &packet).unwrap();

            assert_eq!(decoded_target, target, "method {} failed", method);
            assert_eq!(decoded_payload, payload, "method {} failed", method);
        }
    }

    #[test]
    fn test_tampered_packet_fails() {
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };
        let payload = b"tamper test";

        let mut packet =
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, payload, &salt).unwrap();

        // Tamper with the ciphertext (after the salt)
        let last = packet.len() - 1;
        packet[last] ^= 0xFF;

        assert!(decode_udp_packet(CipherMethod::Aes256Gcm, password, &packet).is_err());
    }

    #[test]
    fn test_wrong_password_fails() {
        let password1 = b"correct-password-123456";
        let password2 = b"wrong-password-678901";
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };
        let payload = b"wrong password test";

        let packet =
            encode_udp_packet(CipherMethod::Aes256Gcm, password1, &target, payload, &salt).unwrap();
        assert!(decode_udp_packet(CipherMethod::Aes256Gcm, password2, &packet).is_err());
    }

    #[test]
    fn test_empty_payload() {
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };

        let packet =
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, b"", &salt).unwrap();
        let (decoded_target, decoded_payload) =
            decode_udp_packet(CipherMethod::Aes256Gcm, password, &packet).unwrap();

        assert_eq!(decoded_target, target);
        assert!(decoded_payload.is_empty());
    }

    #[test]
    fn test_large_payload() {
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };
        let payload = vec![0xABu8; 1400]; // Typical UDP payload size

        let packet =
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, &payload, &salt).unwrap();
        let (decoded_target, decoded_payload) =
            decode_udp_packet(CipherMethod::Aes256Gcm, password, &packet).unwrap();

        assert_eq!(decoded_target, target);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_packet_too_short() {
        let password = test_password();
        // Packet shorter than salt + tag
        let packet = vec![0u8; 5];
        assert!(decode_udp_packet(CipherMethod::Aes256Gcm, password, &packet).is_err());
    }

    #[test]
    fn test_unique_salts() {
        let password = test_password();
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };
        let payload = b"salts uniqueness test";

        let salt1 = [0x01u8; 16];
        let salt2 = [0x02u8; 16];

        let p1 =
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, payload, &salt1).unwrap();
        let p2 =
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, payload, &salt2).unwrap();

        // Different salts should produce different packets
        assert_ne!(p1, p2);
    }

    #[test]
    fn test_overlong_domain_rejects() {
        let password = test_password();
        let salt = test_salt();
        let long_domain = "a".repeat(256); // 256 > MAX_DOMAIN_LEN (255)
        let target = TargetAddr {
            host: TargetHost::Domain(long_domain),
            port: 443,
        };
        let payload = b"test";

        // Encoding should reject the overlong domain
        assert!(
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, payload, &salt,).is_err()
        );
    }

    #[test]
    fn test_oversized_datagram_rejects() {
        let password = test_password();
        // Build a packet that exceeds MAX_UDP_PACKET_SIZE
        let oversized = vec![0u8; MAX_UDP_PACKET_SIZE + 1];
        assert!(decode_udp_packet(CipherMethod::Aes256Gcm, password, &oversized).is_err());
    }

    // ===== Structural byte inspection tests =====
    // These verify the raw packet layout without relying on roundtrip helpers.

    #[test]
    fn test_packet_layout_ipv4_structure() {
        let method = CipherMethod::Aes256Gcm;
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };
        let payload = b"structure test";

        let packet = encode_udp_packet(method, password, &target, payload, &salt).unwrap();

        let salt_size = method.salt_size();
        let tag_size = method.tag_size();

        // Total length: salt + (address + payload + tag)
        // IPv4 address: ATYP(1) + IP(4) + PORT(2) = 7 bytes
        let addr_len = 7;
        let expected_ciphertext_len = addr_len + payload.len() + tag_size;
        assert_eq!(
            packet.len(),
            salt_size + expected_ciphertext_len,
            "packet length mismatch"
        );

        // Salt at offset 0
        assert_eq!(&packet[..salt_size], &salt, "salt not at offset 0");

        // Ciphertext follows immediately after salt
        assert_eq!(packet.len() - salt_size, expected_ciphertext_len);
    }

    #[test]
    fn test_packet_layout_domain_structure() {
        let method = CipherMethod::Aes256Gcm;
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };
        let payload = b"domain structure";

        let packet = encode_udp_packet(method, password, &target, payload, &salt).unwrap();

        let salt_size = method.salt_size();
        let tag_size = method.tag_size();

        // Domain address: ATYP(1) + LEN(1) + domain(11) + PORT(2) = 15 bytes
        let addr_len = 1 + 1 + "example.com".len() + 2;
        let expected_ciphertext_len = addr_len + payload.len() + tag_size;
        assert_eq!(
            packet.len(),
            salt_size + expected_ciphertext_len,
            "packet length mismatch for domain"
        );

        // Salt at offset 0
        assert_eq!(&packet[..salt_size], &salt);
    }

    #[test]
    fn test_packet_layout_ipv6_structure() {
        let method = CipherMethod::ChaCha20IetfPoly1305;
        let password = test_password();
        let salt = [0xAAu8; 16];
        let target = TargetAddr {
            host: TargetHost::Ip("::1".parse().unwrap()),
            port: 8080,
        };
        let payload = b"ipv6 struct";

        let packet = encode_udp_packet(method, password, &target, payload, &salt).unwrap();

        let salt_size = method.salt_size();
        let tag_size = method.tag_size();

        // IPv6 address: ATYP(1) + IP(16) + PORT(2) = 19 bytes
        let addr_len = 19;
        let expected_ciphertext_len = addr_len + payload.len() + tag_size;
        assert_eq!(
            packet.len(),
            salt_size + expected_ciphertext_len,
            "packet length mismatch for IPv6"
        );

        // Salt at offset 0
        assert_eq!(&packet[..salt_size], &salt);
    }

    #[test]
    fn test_packet_layout_all_methods_consistent() {
        let methods = [
            CipherMethod::Aes128Gcm,
            CipherMethod::Aes256Gcm,
            CipherMethod::ChaCha20IetfPoly1305,
        ];
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Ip("192.168.1.1".parse().unwrap()),
            port: 12345,
        };
        let payload = b"method consistency";

        for method in methods.iter() {
            let packet = encode_udp_packet(*method, password, &target, payload, &salt).unwrap();

            let salt_size = method.salt_size();
            let tag_size = method.tag_size();

            // All methods use 16-byte salt
            assert_eq!(salt_size, 16, "method {} salt_size != 16", method);

            // IPv4 address: 7 bytes
            let addr_len = 7;
            let expected_ciphertext_len = addr_len + payload.len() + tag_size;
            assert_eq!(
                packet.len(),
                salt_size + expected_ciphertext_len,
                "method {} packet length mismatch",
                method
            );

            // Salt at offset 0 matches input
            assert_eq!(
                &packet[..salt_size],
                &salt,
                "method {} salt mismatch",
                method
            );
        }
    }

    #[test]
    fn test_tampered_salt_fails() {
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };
        let payload = b"salt tamper";

        let mut packet =
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, payload, &salt).unwrap();

        // Flip a byte in the salt
        packet[0] ^= 0xFF;

        assert!(decode_udp_packet(CipherMethod::Aes256Gcm, password, &packet).is_err());
    }

    #[test]
    fn test_tampered_ciphertext_tag_fails() {
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };
        let payload = b"tag tamper";

        let mut packet =
            encode_udp_packet(CipherMethod::Aes256Gcm, password, &target, payload, &salt).unwrap();

        // Flip a byte in the AEAD tag (last 16 bytes)
        let tag_start = packet.len() - 16;
        packet[tag_start] ^= 0xFF;

        assert!(decode_udp_packet(CipherMethod::Aes256Gcm, password, &packet).is_err());
    }

    #[test]
    fn test_empty_payload_produces_valid_packet() {
        let method = CipherMethod::Aes256Gcm;
        let password = test_password();
        let salt = test_salt();
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 80,
        };

        let packet = encode_udp_packet(method, password, &target, b"", &salt).unwrap();

        let salt_size = method.salt_size();
        let tag_size = method.tag_size();

        // Empty payload: packet = salt + AEAD(address + empty) = salt + (7 + 0 + 16)
        let expected_ciphertext_len = 7 + tag_size;
        assert_eq!(packet.len(), salt_size + expected_ciphertext_len);

        // Verify roundtrip works
        let (decoded_target, decoded_payload) =
            decode_udp_packet(method, password, &packet).unwrap();
        assert_eq!(decoded_target, target);
        assert!(decoded_payload.is_empty());
    }
}
