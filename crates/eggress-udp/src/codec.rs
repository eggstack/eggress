pub use eggress_protocol_socks::socks5::server::{SocksAddr, ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6};
pub use eggress_protocol_socks::socks5::udp_codec::*;

use crate::error::UdpError;
use crate::limits::UdpLimits;

pub fn decode_packet<'a>(
    packet: &'a [u8],
    limits: &UdpLimits,
) -> Result<Socks5UdpRequest<'a>, UdpError> {
    if packet.len() > limits.max_datagram_size {
        return Err(UdpError::DatagramTooLarge(
            packet.len(),
            limits.max_datagram_size,
        ));
    }
    Ok(decode_socks5_udp_datagram(packet)?)
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

    #[test]
    fn decode_within_limits() {
        let limits = UdpLimits {
            max_datagram_size: 65535,
            ..Default::default()
        };
        let pkt = ipv4_packet([192, 168, 1, 1], 8080, b"hello");
        let req = decode_packet(&pkt, &limits).unwrap();
        assert_eq!(req.target, SocksAddr::IPv4([192, 168, 1, 1], 8080));
        assert_eq!(req.payload, b"hello");
    }

    #[test]
    fn decode_rejects_oversized_packet() {
        let limits = UdpLimits {
            max_datagram_size: 10,
            ..Default::default()
        };
        let pkt = ipv4_packet([192, 168, 1, 1], 8080, b"hello world this is long");
        // 4 (header) + 4 (ip) + 2 (port) + 24 (payload) = 34
        assert_eq!(pkt.len(), 34);
        let result = decode_packet(&pkt, &limits);
        assert!(matches!(result, Err(UdpError::DatagramTooLarge(34, 10))));
    }

    #[test]
    fn decode_exactly_at_limit() {
        let limits = UdpLimits {
            max_datagram_size: 12,
            ..Default::default()
        };
        let pkt = ipv4_packet([192, 168, 1, 1], 80, b"hi");
        // 4 (header) + 4 (ip) + 2 (port) + 2 (payload) = 12
        assert_eq!(pkt.len(), 12);
        let req = decode_packet(&pkt, &limits).unwrap();
        assert_eq!(req.payload, b"hi");
    }

    #[test]
    fn decode_one_over_limit() {
        let limits = UdpLimits {
            max_datagram_size: 11,
            ..Default::default()
        };
        let pkt = ipv4_packet([192, 168, 1, 1], 80, b"hi");
        // 4 (header) + 4 (ip) + 2 (port) + 2 (payload) = 12
        assert_eq!(pkt.len(), 12);
        let result = decode_packet(&pkt, &limits);
        assert!(matches!(result, Err(UdpError::DatagramTooLarge(12, 11))));
    }

    #[test]
    fn codec_error_converts() {
        let limits = UdpLimits {
            max_datagram_size: 65535,
            ..Default::default()
        };
        let pkt = vec![0x00, 0x00];
        let result = decode_packet(&pkt, &limits);
        assert!(matches!(
            result,
            Err(UdpError::Codec(UdpCodecError::TooShort))
        ));
    }
}
