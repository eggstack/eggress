use crate::socks5::server::{SocksAddr, ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6};

pub const MAX_UDP_DATAGRAM_SIZE: usize = 65535;

pub struct Socks5UdpRequest<'a> {
    pub target: SocksAddr,
    pub payload: &'a [u8],
}

#[derive(Debug, thiserror::Error)]
pub enum UdpCodecError {
    #[error("packet too short")]
    TooShort,
    #[error("non-zero reserved field")]
    BadReserved,
    #[error("fragmentation not supported")]
    FragmentationUnsupported,
    #[error("unknown address type: {0}")]
    UnknownAddressType(u8),
    #[error("zero domain length")]
    BadDomainLength,
    #[error("malformed domain name")]
    MalformedDomain,
    #[error("missing port")]
    MissingPort,
    #[error("packet too large: {0} > {1}")]
    PacketTooLarge(usize, usize),
}

pub fn decode_socks5_udp_request(packet: &[u8]) -> Result<Socks5UdpRequest<'_>, UdpCodecError> {
    if packet.len() < 4 {
        return Err(UdpCodecError::TooShort);
    }

    if packet[0] != 0x00 || packet[1] != 0x00 {
        return Err(UdpCodecError::BadReserved);
    }

    if packet[2] != 0x00 {
        return Err(UdpCodecError::FragmentationUnsupported);
    }

    let atyp = packet[3];
    let mut offset = 4;

    let target = match atyp {
        ATYP_IPV4 => {
            if packet.len() < offset + 4 + 2 {
                return Err(UdpCodecError::TooShort);
            }
            let mut addr = [0u8; 4];
            addr.copy_from_slice(&packet[offset..offset + 4]);
            offset += 4;
            let port = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
            offset += 2;
            SocksAddr::IPv4(addr, port)
        }
        ATYP_IPV6 => {
            if packet.len() < offset + 16 + 2 {
                return Err(UdpCodecError::TooShort);
            }
            let mut addr = [0u8; 16];
            addr.copy_from_slice(&packet[offset..offset + 16]);
            offset += 16;
            let port = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
            offset += 2;
            SocksAddr::IPv6(addr, port)
        }
        ATYP_DOMAIN => {
            if packet.len() < offset + 1 {
                return Err(UdpCodecError::TooShort);
            }
            let domain_len = packet[offset] as usize;
            offset += 1;
            if domain_len == 0 {
                return Err(UdpCodecError::BadDomainLength);
            }
            if packet.len() < offset + domain_len + 2 {
                return Err(UdpCodecError::TooShort);
            }
            let domain = std::str::from_utf8(&packet[offset..offset + domain_len])
                .map_err(|_| UdpCodecError::MalformedDomain)?;
            offset += domain_len;
            let port = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
            offset += 2;
            SocksAddr::Domain(domain.to_string(), port)
        }
        _ => return Err(UdpCodecError::UnknownAddressType(atyp)),
    };

    let payload = &packet[offset..];
    Ok(Socks5UdpRequest { target, payload })
}

pub fn encode_socks5_udp_response(target: &SocksAddr, payload: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.extend_from_slice(&[0x00, 0x00]);
    out.push(0x00);
    out.extend_from_slice(&target.encode_reply());
    out.extend_from_slice(payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ipv4_packet(target: [u8; 4], port: u16, payload: &[u8]) -> Vec<u8> {
        let mut pkt = vec![0x00, 0x00, 0x00, ATYP_IPV4];
        pkt.extend_from_slice(&target);
        pkt.extend_from_slice(&port.to_be_bytes());
        pkt.extend_from_slice(payload);
        pkt
    }

    fn ipv6_packet(target: [u8; 16], port: u16, payload: &[u8]) -> Vec<u8> {
        let mut pkt = vec![0x00, 0x00, 0x00, ATYP_IPV6];
        pkt.extend_from_slice(&target);
        pkt.extend_from_slice(&port.to_be_bytes());
        pkt.extend_from_slice(payload);
        pkt
    }

    fn domain_packet(domain: &str, port: u16, payload: &[u8]) -> Vec<u8> {
        let mut pkt = vec![0x00, 0x00, 0x00, ATYP_DOMAIN, domain.len() as u8];
        pkt.extend_from_slice(domain.as_bytes());
        pkt.extend_from_slice(&port.to_be_bytes());
        pkt.extend_from_slice(payload);
        pkt
    }

    #[test]
    fn decode_ipv4_target() {
        let pkt = ipv4_packet([192, 168, 1, 1], 8080, b"hello");
        let req = decode_socks5_udp_request(&pkt).unwrap();
        assert_eq!(req.target, SocksAddr::IPv4([192, 168, 1, 1], 8080));
        assert_eq!(req.payload, b"hello");
    }

    #[test]
    fn decode_ipv6_target() {
        let addr = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let pkt = ipv6_packet(addr, 443, b"data");
        let req = decode_socks5_udp_request(&pkt).unwrap();
        assert_eq!(req.target, SocksAddr::IPv6(addr, 443));
        assert_eq!(req.payload, b"data");
    }

    #[test]
    fn decode_domain_target() {
        let pkt = domain_packet("example.com", 53, b"\x01\x02");
        let req = decode_socks5_udp_request(&pkt).unwrap();
        assert_eq!(req.target, SocksAddr::Domain("example.com".to_string(), 53));
        assert_eq!(req.payload, b"\x01\x02");
    }

    #[test]
    fn encode_decode_roundtrip_ipv4() {
        let target = SocksAddr::IPv4([10, 0, 0, 1], 9999);
        let payload = b"roundtrip";
        let mut buf = Vec::new();
        encode_socks5_udp_response(&target, payload, &mut buf);

        let req = decode_socks5_udp_request(&buf).unwrap();
        assert_eq!(req.target, target);
        assert_eq!(req.payload, payload);
    }

    #[test]
    fn encode_decode_roundtrip_ipv6() {
        let addr = [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let target = SocksAddr::IPv6(addr, 80);
        let payload = b"test payload";
        let mut buf = Vec::new();
        encode_socks5_udp_response(&target, payload, &mut buf);

        let req = decode_socks5_udp_request(&buf).unwrap();
        assert_eq!(req.target, target);
        assert_eq!(req.payload, payload);
    }

    #[test]
    fn encode_decode_roundtrip_domain() {
        let target = SocksAddr::Domain("localhost".to_string(), 5353);
        let payload = b"dns query";
        let mut buf = Vec::new();
        encode_socks5_udp_response(&target, payload, &mut buf);

        let req = decode_socks5_udp_request(&buf).unwrap();
        assert_eq!(req.target, target);
        assert_eq!(req.payload, payload);
    }

    #[test]
    fn reject_bad_rsv() {
        let mut pkt = ipv4_packet([1, 2, 3, 4], 80, b"");
        pkt[0] = 0x01;
        assert!(matches!(
            decode_socks5_udp_request(&pkt),
            Err(UdpCodecError::BadReserved)
        ));
    }

    #[test]
    fn reject_bad_rsv_second_byte() {
        let mut pkt = ipv4_packet([1, 2, 3, 4], 80, b"");
        pkt[1] = 0x01;
        assert!(matches!(
            decode_socks5_udp_request(&pkt),
            Err(UdpCodecError::BadReserved)
        ));
    }

    #[test]
    fn reject_frag_nonzero() {
        let mut pkt = ipv4_packet([1, 2, 3, 4], 80, b"");
        pkt[2] = 0x01;
        assert!(matches!(
            decode_socks5_udp_request(&pkt),
            Err(UdpCodecError::FragmentationUnsupported)
        ));
    }

    #[test]
    fn reject_short_packet_too_few_bytes() {
        assert!(matches!(
            decode_socks5_udp_request(&[0x00, 0x00]),
            Err(UdpCodecError::TooShort)
        ));
    }

    #[test]
    fn reject_short_packet_ipv4() {
        // ATYP_IPV4 but missing address bytes
        let pkt = vec![0x00, 0x00, 0x00, ATYP_IPV4, 1, 2, 3];
        assert!(matches!(
            decode_socks5_udp_request(&pkt),
            Err(UdpCodecError::TooShort)
        ));
    }

    #[test]
    fn reject_short_packet_ipv6() {
        let pkt = vec![0x00, 0x00, 0x00, ATYP_IPV6, 0, 0, 0, 0];
        assert!(matches!(
            decode_socks5_udp_request(&pkt),
            Err(UdpCodecError::TooShort)
        ));
    }

    #[test]
    fn reject_short_packet_domain_no_len() {
        let pkt = vec![0x00, 0x00, 0x00, ATYP_DOMAIN];
        assert!(matches!(
            decode_socks5_udp_request(&pkt),
            Err(UdpCodecError::TooShort)
        ));
    }

    #[test]
    fn reject_short_packet_domain_truncated() {
        let pkt = vec![0x00, 0x00, 0x00, ATYP_DOMAIN, 5, b'a', b'b'];
        assert!(matches!(
            decode_socks5_udp_request(&pkt),
            Err(UdpCodecError::TooShort)
        ));
    }

    #[test]
    fn reject_unknown_atyp() {
        let pkt = vec![0x00, 0x00, 0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x00, 0x50];
        assert!(matches!(
            decode_socks5_udp_request(&pkt),
            Err(UdpCodecError::UnknownAddressType(0x05))
        ));
    }

    #[test]
    fn reject_zero_domain_length() {
        let pkt = vec![0x00, 0x00, 0x00, ATYP_DOMAIN, 0x00];
        assert!(matches!(
            decode_socks5_udp_request(&pkt),
            Err(UdpCodecError::BadDomainLength)
        ));
    }

    #[test]
    fn preserve_payload_bytes_exactly() {
        let payload: Vec<u8> = (0..=255).collect();
        let pkt = ipv4_packet([10, 0, 0, 1], 1234, &payload);
        let req = decode_socks5_udp_request(&pkt).unwrap();
        assert_eq!(req.payload.len(), 256);
        for (i, &b) in req.payload.iter().enumerate() {
            assert_eq!(b, i as u8);
        }
    }

    #[test]
    fn zero_length_payload() {
        let pkt = ipv4_packet([10, 0, 0, 1], 80, b"");
        let req = decode_socks5_udp_request(&pkt).unwrap();
        assert_eq!(req.payload.len(), 0);
    }

    #[test]
    fn encode_response_format() {
        let target = SocksAddr::IPv4([192, 168, 1, 1], 80);
        let payload = b"test";
        let mut buf = Vec::new();
        encode_socks5_udp_response(&target, payload, &mut buf);

        assert_eq!(buf[0], 0x00);
        assert_eq!(buf[1], 0x00);
        assert_eq!(buf[2], 0x00);
        assert_eq!(buf[3], ATYP_IPV4);
        assert_eq!(&buf[4..8], &[192, 168, 1, 1]);
        assert_eq!(&buf[8..10], &80u16.to_be_bytes());
        assert_eq!(&buf[10..], b"test");
    }
}
