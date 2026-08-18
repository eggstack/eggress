use crate::address::{decode_address, encode_address};
use crate::aead::{
    aead_decrypt_raw, aead_encrypt_raw, decrypt_standard_chunks, encrypt_standard_chunks,
};
use crate::error::ShadowsocksError;
use crate::method::CipherMethod;
use eggress_core::{TargetAddr, TargetHost};

/// Maximum domain name length per RFC 1035.
const MAX_DOMAIN_LEN: usize = 255;

/// Maximum Shadowsocks UDP datagram size.
const MAX_UDP_PACKET_SIZE: usize = 65535;

/// Encode a Shadowsocks UDP packet using the standard AEAD format.
///
/// Packet format: method-sized salt + AEAD(address + payload, nonce=0)
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
/// Input: method-sized salt + AEAD ciphertext
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

/// Encode a pproxy 2.7.9 Shadowsocks UDP packet.
///
/// pproxy reuses its stream AEAD chunker for PacketCipher, so this variant
/// uses encrypted length and payload blocks after the method-sized salt.
pub fn encode_pproxy_udp_packet(
    method: CipherMethod,
    password: &[u8],
    target: &TargetAddr,
    payload: &[u8],
    salt: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    let plaintext = udp_plaintext(target, payload)?;
    if salt.len() != method.salt_size() {
        return Err(ShadowsocksError::DecryptionFailed(format!(
            "salt must be {} bytes, got {}",
            method.salt_size(),
            salt.len()
        )));
    }
    let subkey = method.derive_key(password, salt)?;
    let nonce = vec![0u8; method.nonce_size()];
    let ciphertext = encrypt_standard_chunks(method, &subkey, &nonce, &plaintext)?;
    let mut output = Vec::with_capacity(salt.len() + ciphertext.len());
    output.extend_from_slice(salt);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decode a pproxy 2.7.9 Shadowsocks UDP packet.
pub fn decode_pproxy_udp_packet(
    method: CipherMethod,
    password: &[u8],
    packet: &[u8],
) -> Result<(TargetAddr, Vec<u8>), ShadowsocksError> {
    let salt_size = method.salt_size();
    let min_size = salt_size + 2 + method.tag_size() + method.tag_size();
    if packet.len() < min_size {
        return Err(ShadowsocksError::DecryptionFailed(
            "packet too short".into(),
        ));
    }
    let salt = &packet[..salt_size];
    let subkey = method.derive_key(password, salt)?;
    let nonce = vec![0u8; method.nonce_size()];
    let plaintext = decrypt_standard_chunks(method, &subkey, &nonce, &packet[salt_size..])?;
    let (target, addr_len) = decode_address(&plaintext)?;
    Ok((target, plaintext[addr_len..].to_vec()))
}

/// Encode a legacy stream-cipher pproxy UDP packet.
#[cfg(feature = "legacy-crypto")]
pub fn encode_legacy_udp_packet(
    method: crate::legacy::LegacyMethod,
    password: &[u8],
    target: &TargetAddr,
    payload: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    crate::legacy::legacy_udp_encode(method, password, target, payload, iv)
}

/// Decode a legacy stream-cipher pproxy UDP packet.
#[cfg(feature = "legacy-crypto")]
pub fn decode_legacy_udp_packet(
    method: crate::legacy::LegacyMethod,
    password: &[u8],
    packet: &[u8],
) -> Result<(TargetAddr, Vec<u8>), ShadowsocksError> {
    crate::legacy::legacy_udp_decode(method, password, packet)
}

fn udp_plaintext(target: &TargetAddr, payload: &[u8]) -> Result<Vec<u8>, ShadowsocksError> {
    if let TargetHost::Domain(ref domain) = target.host {
        if domain.len() > MAX_DOMAIN_LEN {
            return Err(ShadowsocksError::InvalidAddress(format!(
                "domain too long: {} exceeds maximum {}",
                domain.len(),
                MAX_DOMAIN_LEN
            )));
        }
    }
    let address = encode_address(target)?;
    let mut plaintext = Vec::with_capacity(address.len() + payload.len());
    plaintext.extend_from_slice(&address);
    plaintext.extend_from_slice(payload);
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::{TargetAddr, TargetHost};

    fn test_password() -> &'static [u8] {
        b"test-password-for-udp"
    }

    fn test_salt(method: CipherMethod) -> Vec<u8> {
        vec![0x42u8; method.salt_size()]
    }

    #[test]
    fn test_encode_decode_roundtrip_ipv4() {
        let password = test_password();
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
            CipherMethod::Aes192Gcm,
            CipherMethod::Aes256Gcm,
            CipherMethod::ChaCha20IetfPoly1305,
        ];
        let password = test_password();
        for method in methods.iter() {
            let salt = test_salt(*method);
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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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

        let salt1 = [0x01u8; 32];
        let salt2 = [0x02u8; 32];

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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
        let salt = vec![0xAAu8; method.salt_size()];
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
            CipherMethod::Aes192Gcm,
            CipherMethod::Aes256Gcm,
            CipherMethod::ChaCha20IetfPoly1305,
        ];
        let password = test_password();
        let target = TargetAddr {
            host: TargetHost::Ip("192.168.1.1".parse().unwrap()),
            port: 12345,
        };
        let payload = b"method consistency";

        for method in methods.iter() {
            let salt = test_salt(*method);
            let packet = encode_udp_packet(*method, password, &target, payload, &salt).unwrap();

            let salt_size = method.salt_size();
            let tag_size = method.tag_size();

            assert_eq!(salt_size, method.key_size());

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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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
        let salt = test_salt(CipherMethod::Aes256Gcm);
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

    #[test]
    fn test_pproxy_known_answer_vectors() {
        // Captured from pproxy==2.7.9's PacketCipher with the fixed password,
        // method-sized salt, target 127.0.0.1:8080, and payload "udp-kat".
        let password = b"phase1-vector-password";
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 8080,
        };
        let cases = [
            (
                CipherMethod::Aes128Gcm,
                "000102030405060708090a0b0c0d0e0f"
                    .to_string()
                    + "d8859519208261d5de7d73499ac1e39e2e19593d8ee9539eacfd374ac5103ae856532f42dfa75dce86f8de482c035909",
            ),
            (
                CipherMethod::Aes192Gcm,
                "000102030405060708090a0b0c0d0e0f1011121314151617"
                    .to_string()
                    + "9452ed4ba65ab9c6adf18892a30f88a96a23deabb59496f3b8495add82eb852342754a282a38ee92b63cffd7ed66d0c3",
            ),
            (
                CipherMethod::Aes256Gcm,
                "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
                    .to_string()
                    + "2008e0c179dee6c1998171ed6aad218fa05ef3ad33e2b969107ba2e69cf1cb88abe4de339c08575bc0ca5f3f1b7a4b57",
            ),
            (
                CipherMethod::ChaCha20IetfPoly1305,
                "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
                    .to_string()
                    + "bdcc3fda30ca751f89923b48aefa3e0877b8cfcda5c6cdb40288236d6557d423757110511e946e068ff90319ed42f685",
            ),
        ];

        for (method, expected_hex) in cases {
            let salt = (0..method.salt_size())
                .map(|value| value as u8)
                .collect::<Vec<_>>();
            let packet =
                encode_pproxy_udp_packet(method, password, &target, b"udp-kat", &salt).unwrap();
            assert_eq!(packet, hex_bytes(&expected_hex), "method {method}");
            let (decoded_target, decoded_payload) =
                decode_pproxy_udp_packet(method, password, &packet).unwrap();
            assert_eq!(decoded_target, target);
            assert_eq!(decoded_payload, b"udp-kat");
        }
    }

    fn hex_bytes(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
}
