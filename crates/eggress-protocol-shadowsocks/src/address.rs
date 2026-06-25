use eggress_core::{TargetAddr, TargetHost};

use crate::error::ShadowsocksError;

/// ATYP values for Shadowsocks address format.
const ATYP_IPV4: u8 = 0x01;
const ATYP_DOMAIN: u8 = 0x03;
const ATYP_IPV6: u8 = 0x04;

/// Encode a TargetAddr into Shadowsocks wire format.
pub fn encode_address(target: &TargetAddr) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + 4 + 2); // ATYP + max IP + port
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => {
            buf.push(ATYP_IPV4);
            buf.extend_from_slice(&ip.octets());
        }
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => {
            buf.push(ATYP_IPV6);
            buf.extend_from_slice(&ip.octets());
        }
        TargetHost::Domain(domain) => {
            buf.push(ATYP_DOMAIN);
            buf.push(domain.len() as u8);
            buf.extend_from_slice(domain.as_bytes());
        }
    }
    buf.extend_from_slice(&target.port.to_be_bytes());
    buf
}

/// Decode a Shadowsocks address from a byte slice.
///
/// Returns the decoded TargetAddr and the number of bytes consumed.
pub fn decode_address(data: &[u8]) -> Result<(TargetAddr, usize), ShadowsocksError> {
    if data.is_empty() {
        return Err(ShadowsocksError::InvalidAddress("empty data".into()));
    }

    let atyp = data[0];

    let (host, mut pos) = match atyp {
        ATYP_IPV4 => {
            if data.len() < 5 {
                return Err(ShadowsocksError::InvalidAddress(
                    "truncated IPv4 address".into(),
                ));
            }
            let ip = std::net::Ipv4Addr::new(data[1], data[2], data[3], data[4]);
            (TargetHost::Ip(std::net::IpAddr::V4(ip)), 5)
        }
        ATYP_IPV6 => {
            if data.len() < 17 {
                return Err(ShadowsocksError::InvalidAddress(
                    "truncated IPv6 address".into(),
                ));
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&data[1..17]);
            let ip = std::net::Ipv6Addr::from(octets);
            (TargetHost::Ip(std::net::IpAddr::V6(ip)), 17)
        }
        ATYP_DOMAIN => {
            if data.len() < 2 {
                return Err(ShadowsocksError::InvalidAddress(
                    "truncated domain length".into(),
                ));
            }
            let len = data[1] as usize;
            if data.len() < 2 + len {
                return Err(ShadowsocksError::InvalidAddress("truncated domain".into()));
            }
            let domain = String::from_utf8(data[2..2 + len].to_vec())
                .map_err(|e| ShadowsocksError::InvalidAddress(format!("invalid UTF-8: {}", e)))?;
            (TargetHost::Domain(domain), 2 + len)
        }
        _ => {
            return Err(ShadowsocksError::InvalidAddress(format!(
                "unknown ATYP: {:#04x}",
                atyp
            )));
        }
    };

    if data.len() < pos + 2 {
        return Err(ShadowsocksError::InvalidAddress("truncated port".into()));
    }

    let port = u16::from_be_bytes([data[pos], data[pos + 1]]);
    pos += 2;

    Ok((TargetAddr { host, port }, pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_ipv4() {
        let target = TargetAddr {
            host: TargetHost::Ip("192.168.1.1".parse().unwrap()),
            port: 8080,
        };
        let encoded = encode_address(&target);
        assert_eq!(encoded[0], ATYP_IPV4);
        let (decoded, consumed) = decode_address(&encoded).unwrap();
        assert_eq!(decoded, target);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_encode_decode_ipv6() {
        let target = TargetAddr {
            host: TargetHost::Ip("::1".parse().unwrap()),
            port: 443,
        };
        let encoded = encode_address(&target);
        assert_eq!(encoded[0], ATYP_IPV6);
        let (decoded, consumed) = decode_address(&encoded).unwrap();
        assert_eq!(decoded, target);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_encode_decode_domain() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };
        let encoded = encode_address(&target);
        assert_eq!(encoded[0], ATYP_DOMAIN);
        assert_eq!(encoded[1], 11); // "example.com".len()
        let (decoded, consumed) = decode_address(&encoded).unwrap();
        assert_eq!(decoded, target);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_decode_empty() {
        assert!(decode_address(&[]).is_err());
    }

    #[test]
    fn test_decode_truncated_ipv4() {
        assert!(decode_address(&[ATYP_IPV4, 192, 168]).is_err());
    }

    #[test]
    fn test_decode_truncated_port() {
        let data = vec![ATYP_IPV4, 192, 168, 1, 1]; // missing port
        assert!(decode_address(&data).is_err());
    }

    #[test]
    fn test_decode_unknown_atyp() {
        assert!(decode_address(&[0xFF]).is_err());
    }

    #[test]
    fn test_encode_decode_roundtrip_various_ports() {
        let ports = [0, 1, 80, 443, 8080, 65535];
        for port in ports {
            let target = TargetAddr {
                host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
                port,
            };
            let encoded = encode_address(&target);
            let (decoded, _) = decode_address(&encoded).unwrap();
            assert_eq!(decoded.port, port);
        }
    }
}
