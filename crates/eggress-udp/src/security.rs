use std::net::{Ipv4Addr, Ipv6Addr};

use eggress_protocol_socks::socks5::server::SocksAddr;

use crate::error::UdpError;

pub fn validate_target(target: &SocksAddr) -> Result<(), UdpError> {
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
        let target = SocksAddr::IPv6([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], 443);
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
    fn valid_loopback() {
        let target = SocksAddr::IPv4([127, 0, 0, 1], 8080);
        assert!(validate_target(&target).is_ok());
    }
}
