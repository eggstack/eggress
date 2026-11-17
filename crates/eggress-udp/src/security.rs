use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use eggress_core::connector::is_reserved_or_private_ip;
use eggress_protocol_socks::socks5::server::SocksAddr;

use crate::error::UdpError;

pub fn validate_target(target: &SocksAddr) -> Result<(), UdpError> {
    validate_target_shape(target, true)
}

fn validate_target_shape(target: &SocksAddr, reject_loopback: bool) -> Result<(), UdpError> {
    match target {
        SocksAddr::IPv4(addr, port) => {
            let ip = Ipv4Addr::from(*addr);
            if *port == 0 {
                return Err(UdpError::PortZero);
            }
            if ip.is_multicast() {
                return Err(UdpError::MulticastTarget);
            }
            if ip.is_broadcast() {
                return Err(UdpError::BroadcastTarget);
            }
            if ip.is_unspecified() {
                return Err(UdpError::UnspecifiedTarget);
            }
            if reject_loopback && ip.is_loopback() {
                return Err(UdpError::Other("loopback target not allowed".to_string()));
            }
            Ok(())
        }
        SocksAddr::IPv6(addr, port) => {
            let ip = Ipv6Addr::from(*addr);
            if *port == 0 {
                return Err(UdpError::PortZero);
            }
            if ip.is_multicast() {
                return Err(UdpError::MulticastTarget);
            }
            if ip.is_unspecified() {
                return Err(UdpError::UnspecifiedTarget);
            }
            if reject_loopback && ip.is_loopback() {
                return Err(UdpError::Other("loopback target not allowed".to_string()));
            }
            Ok(())
        }
        SocksAddr::Domain(_, port) => {
            if *port == 0 {
                return Err(UdpError::PortZero);
            }
            Ok(())
        }
    }
}

fn is_private_ipv4(ip: &Ipv4Addr) -> bool {
    is_reserved_or_private_ip(&IpAddr::V4(*ip))
}

fn is_private_ipv6(ip: &Ipv6Addr) -> bool {
    is_reserved_or_private_ip(&IpAddr::V6(*ip))
}

/// Validate a datagram destination before a flow is created.
///
/// Shape checks (port zero, multicast, broadcast, unspecified, and loopback
/// when `allow_private_egress` is false) always run. Literal private/reserved
/// addresses are rejected here when the policy is restrictive; domain targets
/// cannot be judged yet and are enforced post-DNS by
/// `direct::resolve_target`, which filters every resolved address through the
/// same shared classifier before connecting.
pub fn validate_standalone_target(
    target: &SocksAddr,
    allow_private_egress: bool,
) -> Result<(), UdpError> {
    validate_target_shape(target, !allow_private_egress)?;

    if !allow_private_egress {
        match target {
            SocksAddr::IPv4(addr, _) => {
                if is_private_ipv4(&Ipv4Addr::from(*addr)) {
                    return Err(UdpError::PrivateEgressBlocked);
                }
            }
            SocksAddr::IPv6(addr, _) => {
                if is_private_ipv6(&Ipv6Addr::from(*addr)) {
                    return Err(UdpError::PrivateEgressBlocked);
                }
            }
            // A domain carries no address yet; the resolved-address policy is
            // applied by `direct::resolve_target` (single lookup, all answers
            // filtered).
            SocksAddr::Domain(..) => {}
        }
    }

    Ok(())
}

pub fn validate_datagram_size(size: usize, max_size: usize) -> Result<(), UdpError> {
    if size > max_size {
        return Err(UdpError::DatagramTooLarge(size, max_size));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_ipv4_target() {
        let target = SocksAddr::IPv4([192, 168, 1, 1], 8080);
        assert!(validate_target(&target).is_ok());
    }

    #[test]
    fn valid_ipv6_target() {
        let target = SocksAddr::IPv6(
            [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            443,
        );
        assert!(validate_target(&target).is_ok());
    }

    #[test]
    fn valid_domain_target() {
        let target = SocksAddr::Domain("example.com".to_string(), 53);
        assert!(validate_target(&target).is_ok());
    }

    #[test]
    fn reject_ipv4_multicast() {
        let target = SocksAddr::IPv4([224, 0, 0, 1], 80);
        assert!(matches!(
            validate_target(&target),
            Err(UdpError::MulticastTarget)
        ));
    }

    #[test]
    fn reject_ipv4_multicast_239() {
        let target = SocksAddr::IPv4([239, 255, 255, 250], 1900);
        assert!(matches!(
            validate_target(&target),
            Err(UdpError::MulticastTarget)
        ));
    }

    #[test]
    fn reject_ipv4_broadcast() {
        let target = SocksAddr::IPv4([255, 255, 255, 255], 80);
        assert!(matches!(
            validate_target(&target),
            Err(UdpError::BroadcastTarget)
        ));
    }

    #[test]
    fn reject_ipv4_unspecified() {
        let target = SocksAddr::IPv4([0, 0, 0, 0], 80);
        assert!(matches!(
            validate_target(&target),
            Err(UdpError::UnspecifiedTarget)
        ));
    }

    #[test]
    fn reject_ipv6_multicast() {
        let target = SocksAddr::IPv6([0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], 80);
        assert!(matches!(
            validate_target(&target),
            Err(UdpError::MulticastTarget)
        ));
    }

    #[test]
    fn reject_ipv6_unspecified() {
        let target = SocksAddr::IPv6([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 80);
        assert!(matches!(
            validate_target(&target),
            Err(UdpError::UnspecifiedTarget)
        ));
    }

    #[test]
    fn reject_port_zero_ipv4() {
        let target = SocksAddr::IPv4([192, 168, 1, 1], 0);
        assert!(matches!(validate_target(&target), Err(UdpError::PortZero)));
    }

    #[test]
    fn reject_port_zero_ipv6() {
        let target = SocksAddr::IPv6([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], 0);
        assert!(matches!(validate_target(&target), Err(UdpError::PortZero)));
    }

    #[test]
    fn reject_port_zero_domain() {
        let target = SocksAddr::Domain("example.com".to_string(), 0);
        assert!(matches!(validate_target(&target), Err(UdpError::PortZero)));
    }

    #[test]
    fn reject_loopback() {
        let target = SocksAddr::IPv4([127, 0, 0, 1], 8080);
        assert!(validate_target(&target).is_err());
    }

    #[test]
    fn reject_private_ipv4_egress() {
        let target = SocksAddr::IPv4([192, 168, 1, 1], 8080);
        assert!(validate_standalone_target(&target, false).is_err());
    }

    #[test]
    fn allow_private_ipv4_egress_when_permitted() {
        let target = SocksAddr::IPv4([192, 168, 1, 1], 8080);
        assert!(validate_standalone_target(&target, true).is_ok());
    }

    #[test]
    fn reject_loopback_egress() {
        let target = SocksAddr::IPv4([127, 0, 0, 1], 8080);
        assert!(validate_standalone_target(&target, false).is_err());
    }

    #[test]
    fn allow_loopback_egress_when_permitted() {
        let target = SocksAddr::IPv4([127, 0, 0, 1], 8080);
        assert!(validate_standalone_target(&target, true).is_ok());
    }

    #[test]
    fn reject_link_local_ipv4_egress() {
        let target = SocksAddr::IPv4([169, 254, 1, 1], 8080);
        assert!(validate_standalone_target(&target, false).is_err());
    }

    #[test]
    fn reject_mapped_loopback_ipv6_egress() {
        let target = SocksAddr::IPv6(
            "::ffff:127.0.0.1"
                .parse::<std::net::Ipv6Addr>()
                .unwrap()
                .octets(),
            8080,
        );
        assert!(validate_standalone_target(&target, false).is_err());
    }

    #[test]
    fn reject_10_private_egress() {
        let target = SocksAddr::IPv4([10, 0, 0, 1], 8080);
        assert!(validate_standalone_target(&target, false).is_err());
    }

    #[test]
    fn reject_172_private_egress() {
        let target = SocksAddr::IPv4([172, 16, 0, 1], 8080);
        assert!(validate_standalone_target(&target, false).is_err());
    }

    #[test]
    fn allow_public_ipv4_egress() {
        let target = SocksAddr::IPv4([8, 8, 8, 8], 80);
        assert!(validate_standalone_target(&target, false).is_ok());
    }

    #[test]
    fn reject_ipv6_loopback_egress() {
        let target = SocksAddr::IPv6([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], 8080);
        assert!(validate_standalone_target(&target, false).is_err());
    }

    #[test]
    fn allow_ipv6_loopback_egress_when_permitted() {
        let target = SocksAddr::IPv6([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], 8080);
        assert!(validate_standalone_target(&target, true).is_ok());
    }

    #[test]
    fn reject_ipv6_link_local_egress() {
        let target = SocksAddr::IPv6([0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], 8080);
        assert!(validate_standalone_target(&target, false).is_err());
    }

    #[test]
    fn allow_public_ipv6_egress() {
        // 2606:4700:4700::1111 is globally reachable (not documentation or
        // another special-purpose range).
        let target = SocksAddr::IPv6(
            [
                0x26, 0x06, 0x47, 0x00, 0x47, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0x11, 0x11,
            ],
            80,
        );
        assert!(validate_standalone_target(&target, false).is_ok());
    }

    #[test]
    fn reject_documentation_ipv6_egress() {
        // 2001:db8::/32 previously slipped past the UDP-local classifier.
        let target = SocksAddr::IPv6(
            [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            80,
        );
        assert!(matches!(
            validate_standalone_target(&target, false),
            Err(UdpError::PrivateEgressBlocked)
        ));
        assert!(validate_standalone_target(&target, true).is_ok());
    }

    #[test]
    fn reject_cgnat_ipv4_egress() {
        let target = SocksAddr::IPv4([100, 64, 0, 1], 8080);
        assert!(matches!(
            validate_standalone_target(&target, false),
            Err(UdpError::PrivateEgressBlocked)
        ));
        assert!(validate_standalone_target(&target, true).is_ok());
    }

    #[test]
    fn reject_nat64_and_teredo_egress() {
        let nat64 = SocksAddr::IPv6(
            "64:ff9b::808:808"
                .parse::<std::net::Ipv6Addr>()
                .unwrap()
                .octets(),
            53,
        );
        let teredo = SocksAddr::IPv6(
            "2001:0:4136:e378:8000:63bf:3fff:fdd2"
                .parse::<std::net::Ipv6Addr>()
                .unwrap()
                .octets(),
            53,
        );
        let sixtofour = SocksAddr::IPv6(
            "2002:808:808::1"
                .parse::<std::net::Ipv6Addr>()
                .unwrap()
                .octets(),
            53,
        );
        for target in [nat64, teredo, sixtofour] {
            assert!(matches!(
                validate_standalone_target(&target, false),
                Err(UdpError::PrivateEgressBlocked)
            ));
            assert!(validate_standalone_target(&target, true).is_ok());
        }
    }

    #[test]
    fn domain_target_passes_pre_check_and_is_checked_post_dns() {
        // A domain carries no address at validation time; the post-DNS policy
        // in `direct::resolve_target` owns the resolved-address decision.
        let target = SocksAddr::Domain("example.com".to_string(), 80);
        assert!(validate_standalone_target(&target, false).is_ok());
    }

    #[test]
    fn standalone_rejects_multicast() {
        let target = SocksAddr::IPv4([224, 0, 0, 1], 80);
        assert!(validate_standalone_target(&target, true).is_err());
    }

    #[test]
    fn standalone_rejects_broadcast() {
        let target = SocksAddr::IPv4([255, 255, 255, 255], 80);
        assert!(validate_standalone_target(&target, true).is_err());
    }

    #[test]
    fn standalone_rejects_port_zero() {
        let target = SocksAddr::IPv4([8, 8, 8, 8], 0);
        assert!(validate_standalone_target(&target, true).is_err());
    }

    #[test]
    fn standalone_rejects_unspecified() {
        let target = SocksAddr::IPv4([0, 0, 0, 0], 80);
        assert!(validate_standalone_target(&target, true).is_err());
    }

    #[test]
    fn standalone_path_allows_public_egress() {
        let target = SocksAddr::IPv4([8, 8, 4, 4], 53);
        assert!(validate_standalone_target(&target, false).is_ok());
    }

    #[test]
    fn standalone_path_rejects_192_168_egress() {
        let target = SocksAddr::IPv4([192, 168, 0, 1], 8080);
        assert!(validate_standalone_target(&target, false).is_err());
    }

    #[test]
    fn validate_datagram_size_within_limit() {
        assert!(validate_datagram_size(100, 200).is_ok());
    }

    #[test]
    fn validate_datagram_size_at_limit() {
        assert!(validate_datagram_size(200, 200).is_ok());
    }

    #[test]
    fn validate_datagram_size_over_limit() {
        let result = validate_datagram_size(201, 200);
        assert!(matches!(result, Err(UdpError::DatagramTooLarge(201, 200))));
    }

    #[test]
    fn validate_datagram_size_zero() {
        assert!(validate_datagram_size(0, 100).is_ok());
    }

    #[test]
    fn validate_datagram_size_exact_max() {
        assert!(validate_datagram_size(65535, 65535).is_ok());
    }

    #[test]
    fn validate_datagram_size_one_over() {
        assert!(matches!(
            validate_datagram_size(65536, 65535),
            Err(UdpError::DatagramTooLarge(65536, 65535))
        ));
    }
}
