use proptest::prelude::*;

use eggress_protocol_socks::socks5::server::{SocksAddr, ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6};
use eggress_protocol_socks::socks5::udp_codec::{
    decode_socks5_udp_datagram, encode_socks5_udp_datagram, UdpCodecError,
};

fn arb_ipv4_port() -> impl Strategy<Value = (SocksAddr, Vec<u8>)> {
    (
        any::<[u8; 4]>(),
        any::<u16>(),
        proptest::collection::vec(any::<u8>(), 0..1024),
    )
        .prop_map(|(addr, port, payload)| (SocksAddr::IPv4(addr, port), payload))
}

fn arb_ipv6_port() -> impl Strategy<Value = (SocksAddr, Vec<u8>)> {
    (
        any::<[u8; 16]>(),
        any::<u16>(),
        proptest::collection::vec(any::<u8>(), 0..1024),
    )
        .prop_map(|(addr, port, payload)| (SocksAddr::IPv6(addr, port), payload))
}

fn arb_domain_port() -> impl Strategy<Value = (SocksAddr, Vec<u8>)> {
    (
        "[a-z]{1,63}",
        any::<u16>(),
        proptest::collection::vec(any::<u8>(), 0..1024),
    )
        .prop_map(|(domain, port, payload)| (SocksAddr::Domain(domain, port), payload))
}

fn arb_socks_addr() -> impl Strategy<Value = (SocksAddr, Vec<u8>)> {
    prop_oneof![arb_ipv4_port(), arb_ipv6_port(), arb_domain_port(),]
}

proptest! {
    #[test]
    fn encode_decode_roundtrip_ipv4(addr: [u8; 4], port: u16, payload in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let target = SocksAddr::IPv4(addr, port);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, &payload, &mut buf);
        let req = decode_socks5_udp_datagram(&buf).unwrap();
        prop_assert_eq!(req.target, target);
        prop_assert_eq!(req.payload, payload.as_slice());
    }

    #[test]
    fn encode_decode_roundtrip_ipv6(addr: [u8; 16], port: u16, payload in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let target = SocksAddr::IPv6(addr, port);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, &payload, &mut buf);
        let req = decode_socks5_udp_datagram(&buf).unwrap();
        prop_assert_eq!(req.target, target);
        prop_assert_eq!(req.payload, payload.as_slice());
    }

    #[test]
    fn encode_decode_roundtrip_domain(domain in "[a-z]{1,63}", port: u16, payload in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let target = SocksAddr::Domain(domain, port);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, &payload, &mut buf);
        let req = decode_socks5_udp_datagram(&buf).unwrap();
        prop_assert_eq!(req.target, target);
        prop_assert_eq!(req.payload, payload.as_slice());
    }

    #[test]
    fn encode_decode_roundtrip_any(target in arb_socks_addr()) {
        let (target, payload) = target;
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, &payload, &mut buf);
        let req = decode_socks5_udp_datagram(&buf).unwrap();
        prop_assert_eq!(req.target, target);
        prop_assert_eq!(req.payload, payload.as_slice());
    }

    #[test]
    fn reject_nonzero_frag(frag in 1u8..=255u8) {
        let payload = b"test";
        let target = SocksAddr::IPv4([1, 2, 3, 4], 80);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, payload, &mut buf);
        // Tamper with FRAG byte
        buf[2] = frag;
        let result = decode_socks5_udp_datagram(&buf);
        prop_assert!(matches!(result, Err(UdpCodecError::FragmentationUnsupported)));
    }

    #[test]
    fn reject_bad_rsv(byte0 in 1u8..=255u8) {
        let payload = b"test";
        let target = SocksAddr::IPv4([1, 2, 3, 4], 80);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, payload, &mut buf);
        // Tamper with RSV byte 0
        buf[0] = byte0;
        let result = decode_socks5_udp_datagram(&buf);
        prop_assert!(matches!(result, Err(UdpCodecError::BadReserved)));
    }

    #[test]
    fn reject_bad_rsv_second_byte(byte1 in 1u8..=255u8) {
        let payload = b"test";
        let target = SocksAddr::IPv4([1, 2, 3, 4], 80);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, payload, &mut buf);
        // Tamper with RSV byte 1
        buf[1] = byte1;
        let result = decode_socks5_udp_datagram(&buf);
        prop_assert!(matches!(result, Err(UdpCodecError::BadReserved)));
    }

    #[test]
    fn zero_domain_length_rejected(port: u16) {
        // Manually construct a packet with zero-length domain
        let mut pkt = vec![0x00, 0x00, 0x00, ATYP_DOMAIN, 0x00];
        pkt.extend_from_slice(&port.to_be_bytes());
        pkt.extend_from_slice(b"extra");
        let result = decode_socks5_udp_datagram(&pkt);
        prop_assert!(matches!(result, Err(UdpCodecError::BadDomainLength)));
    }

    #[test]
    fn payload_preserved_exactly(payload in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let target = SocksAddr::IPv4([10, 0, 0, 1], 1234);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, &payload, &mut buf);
        let req = decode_socks5_udp_datagram(&buf).unwrap();
        prop_assert_eq!(req.payload.len(), payload.len());
        for (i, &b) in req.payload.iter().enumerate() {
            prop_assert_eq!(b, payload[i]);
        }
    }

    #[test]
    fn empty_payload_roundtrips(port: u16, target_type in 0u8..=2u8) {
        let target = match target_type {
            0 => SocksAddr::IPv4([127, 0, 0, 1], port),
            1 => SocksAddr::IPv6([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], port),
            _ => SocksAddr::Domain("a".to_string(), port),
        };
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, b"", &mut buf);
        let req = decode_socks5_udp_datagram(&buf).unwrap();
        prop_assert_eq!(req.target, target);
        prop_assert_eq!(req.payload.len(), 0);
    }

    #[test]
    fn domain_encoding_length(domain in "[a-z]{1,255}", port: u16) {
        let target = SocksAddr::Domain(domain.clone(), port);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, b"", &mut buf);
        let req = decode_socks5_udp_datagram(&buf).unwrap();
        if let SocksAddr::Domain(decoded_domain, decoded_port) = &req.target {
            prop_assert_eq!(decoded_domain.len(), domain.len());
            prop_assert_eq!(decoded_domain.as_str(), domain.as_str());
            prop_assert_eq!(*decoded_port, port);
        } else {
            prop_assert!(false, "expected Domain variant");
        }
    }

    #[test]
    fn ipv4_fixed_size_layout(addr: [u8; 4], port: u16) {
        let target = SocksAddr::IPv4(addr, port);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, b"X", &mut buf);
        // RSV(2) + FRAG(1) + ATYP(1) + ADDR(4) + PORT(2) + PAYLOAD(1) = 11
        prop_assert_eq!(buf.len(), 11);
        prop_assert_eq!(buf[3], ATYP_IPV4);
        prop_assert_eq!(&buf[4..8], &addr);
        prop_assert_eq!(&buf[8..10], &port.to_be_bytes());
    }

    #[test]
    fn ipv6_fixed_size_layout(addr: [u8; 16], port: u16) {
        let target = SocksAddr::IPv6(addr, port);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, b"X", &mut buf);
        // RSV(2) + FRAG(1) + ATYP(1) + ADDR(16) + PORT(2) + PAYLOAD(1) = 23
        prop_assert_eq!(buf.len(), 23);
        prop_assert_eq!(buf[3], ATYP_IPV6);
        prop_assert_eq!(&buf[4..20], &addr);
        prop_assert_eq!(&buf[20..22], &port.to_be_bytes());
    }

    #[test]
    fn domain_variable_size_layout(domain in "[a-z]{1,63}", port: u16) {
        let target = SocksAddr::Domain(domain.clone(), port);
        let mut buf = Vec::new();
        encode_socks5_udp_datagram(&target, b"X", &mut buf);
        // RSV(2) + FRAG(1) + ATYP(1) + LEN(1) + domain + PORT(2) + PAYLOAD(1)
        let expected = 5 + domain.len() + 2 + 1;
        prop_assert_eq!(buf.len(), expected);
        prop_assert_eq!(buf[3], ATYP_DOMAIN);
        prop_assert_eq!(buf[4], domain.len() as u8);
    }
}
