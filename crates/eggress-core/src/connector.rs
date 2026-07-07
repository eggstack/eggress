use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use tokio::net::TcpStream;

use crate::{BoxStream, ConnectError, TargetAddr, TargetHost};

/// Returns `true` if the IP address is reserved, private, or otherwise
/// unsuitable for direct outbound connections (DNS rebinding protection).
///
/// Used as a domain-resolution guard: after resolving a domain name,
/// this checks whether the result points to a private/reserved/special-use
/// range. Literal IP targets bypass this check for pproxy compatibility.
///
/// Rejected ranges:
/// - IPv4: loopback (127.0.0.0/8), link-local (169.254.0.0/16),
///   private (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16), unspecified (0.0.0.0),
///   broadcast (255.255.255.255), multicast (224.0.0.0/4),
///   documentation (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24),
///   benchmarking (198.18.0.0/15), reserved future (240.0.0.0/4),
///   this-network (0.0.0.0/8)
/// - IPv6: loopback (::1), link-local (fe80::/10), unique-local (fc00::/7),
///   unspecified (::), multicast (ff00::/8),
///   documentation (2001:db8::/32), discard prefix (0100::/64)
pub fn is_reserved_or_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_link_local()
                || v4.is_private()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.is_broadcast()
                || is_v4_documentation(v4)
                || is_v4_benchmarking(v4)
                || is_v4_reserved(v4)
                || is_v4_this_network(v4)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || is_v6_documentation(v6)
                || is_unicast_link_local_v6(v6)
                || is_unique_local_v6(v6)
                || is_v6_discard_prefix(v6)
        }
    }
}

/// Check if an IPv6 address is in the fc00::/7 unique-local range.
fn is_unique_local_v6(ip: &Ipv6Addr) -> bool {
    let octets = ip.octets();
    (octets[0] & 0xfe) == 0xfc
}

/// Check if an IPv6 address is in the fe80::/10 link-local unicast range.
fn is_unicast_link_local_v6(ip: &Ipv6Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80
}

/// Check if an IPv6 address is in the 0100::/64 discard prefix.
fn is_v6_discard_prefix(ip: &Ipv6Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 0x01 && octets[1..8].iter().all(|b| *b == 0)
}

/// Check if an IPv4 address is in the 0.0.0.0/8 "this network" range.
fn is_v4_this_network(ip: &std::net::Ipv4Addr) -> bool {
    ip.octets()[0] == 0
}

/// Check if an IPv4 address is in any of the documentation ranges
/// (TEST-NET-1: 192.0.2.0/24, TEST-NET-2: 198.51.100.0/24,
/// TEST-NET-3: 203.0.113.0/24, 192.88.99.0/24).
fn is_v4_documentation(ip: &std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    matches!(
        octets,
        [192, 0, 2, _] | [198, 51, 100, _] | [203, 0, 113, _] | [192, 88, 99, _]
    )
}

/// Check if an IPv4 address is in the benchmarking range (198.18.0.0/15).
fn is_v4_benchmarking(ip: &std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 198 && (octets[1] == 18 || octets[1] == 19)
}

/// Check if an IPv4 address is in the reserved-for-future-use range
/// (240.0.0.0/4 — first octet >= 240 and not broadcast).
fn is_v4_reserved(ip: &std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] >= 240 && octets[0] < 255
}

/// Check if an IPv6 address is in the documentation range (2001:db8::/32).
fn is_v6_documentation(ip: &Ipv6Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 0x20 && octets[1] == 0x01 && octets[2] == 0x0d && octets[3] == 0xb8
}

/// Check if a resolved IP address represents a DNS rebinding risk.
///
/// Used as a domain-resolution guard: after resolving a domain name,
/// this checks whether the result points to a private/reserved range.
/// Literal IP targets bypass this check for pproxy compatibility.
pub fn is_dns_rebinding_risk(ip: &IpAddr) -> bool {
    is_reserved_or_private_ip(ip)
}

/// Trait for connecting to target servers.
#[trait_variant::make(Connector: Send)]
pub trait LocalConnector {
    async fn connect(&self, target: &TargetAddr) -> Result<BoxStream, ConnectError>;
}

/// Connector that makes direct TCP connections.
pub struct DirectConnector;

impl Connector for DirectConnector {
    async fn connect(&self, target: &TargetAddr) -> Result<BoxStream, ConnectError> {
        let addr: SocketAddr = match &target.host {
            TargetHost::Ip(ip) => SocketAddr::new(*ip, target.port),
            TargetHost::Domain(domain) => {
                let lookup = format!("{}:{}", domain, target.port);
                let mut addrs = tokio::net::lookup_host(&lookup)
                    .await
                    .map_err(|e| ConnectError::DnsResolution(e.to_string()))?;
                let resolved = addrs
                    .next()
                    .ok_or_else(|| ConnectError::DnsResolution("no addresses found".to_string()))?;

                if is_dns_rebinding_risk(&resolved.ip()) {
                    return Err(ConnectError::ReservedTarget(resolved.ip()));
                }

                resolved
            }
        };

        let stream = TcpStream::connect(addr).await?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_direct_connect_echo() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let jh = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await.unwrap();
            stream.write_all(&buf[..n]).await.unwrap();
        });

        let target = TargetAddr {
            host: TargetHost::Ip(addr.ip()),
            port: addr.port(),
        };

        let connector = DirectConnector;
        let mut stream = Connector::connect(&connector, &target).await.unwrap();

        stream.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");

        jh.await.unwrap();
    }

    #[test]
    fn reserved_ipv4_loopback() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            127, 0, 0, 1
        ))));
    }

    #[test]
    fn reserved_ipv4_private_10() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            10, 0, 0, 1
        ))));
    }

    #[test]
    fn reserved_ipv4_private_172() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            172, 16, 0, 1
        ))));
    }

    #[test]
    fn reserved_ipv4_private_192() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            192, 168, 1, 1
        ))));
    }

    #[test]
    fn reserved_ipv4_link_local() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            169, 254, 1, 1
        ))));
    }

    #[test]
    fn reserved_ipv4_unspecified() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(
            Ipv4Addr::UNSPECIFIED
        )));
    }

    #[test]
    fn not_reserved_ipv4_public() {
        assert!(!is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            8, 8, 8, 8
        ))));
    }

    #[test]
    fn reserved_ipv6_loopback() {
        assert!(is_reserved_or_private_ip(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn reserved_ipv6_link_local() {
        let ip = "fe80::1".parse::<Ipv6Addr>().unwrap();
        assert!(is_reserved_or_private_ip(&IpAddr::V6(ip)));
    }

    #[test]
    fn reserved_ipv6_unique_local() {
        let ip = "fd00::1".parse::<Ipv6Addr>().unwrap();
        assert!(is_reserved_or_private_ip(&IpAddr::V6(ip)));
    }

    #[test]
    fn reserved_ipv6_unspecified() {
        assert!(is_reserved_or_private_ip(&IpAddr::V6(
            Ipv6Addr::UNSPECIFIED
        )));
    }

    #[test]
    fn not_reserved_ipv6_public() {
        let ip = "2606:4700:4700::1111".parse::<Ipv6Addr>().unwrap();
        assert!(!is_reserved_or_private_ip(&IpAddr::V6(ip)));
    }

    #[test]
    fn reserved_ipv4_multicast() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            224, 0, 0, 1
        ))));
    }

    #[test]
    fn reserved_ipv4_broadcast() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::BROADCAST)));
    }

    #[test]
    fn reserved_ipv4_documentation() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            192, 0, 2, 1
        ))));
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            198, 51, 100, 1
        ))));
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            203, 0, 113, 1
        ))));
    }

    #[test]
    fn reserved_ipv4_benchmarking() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            198, 18, 0, 1
        ))));
    }

    #[test]
    fn reserved_ipv4_reserved_future() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            240, 0, 0, 1
        ))));
    }

    #[test]
    fn reserved_ipv4_this_network() {
        assert!(is_reserved_or_private_ip(&IpAddr::V4(Ipv4Addr::new(
            0, 1, 2, 3
        ))));
    }

    #[test]
    fn reserved_ipv6_multicast() {
        let ip = "ff02::1".parse::<Ipv6Addr>().unwrap();
        assert!(is_reserved_or_private_ip(&IpAddr::V6(ip)));
    }

    #[test]
    fn reserved_ipv6_documentation() {
        let ip = "2001:db8::1".parse::<Ipv6Addr>().unwrap();
        assert!(is_reserved_or_private_ip(&IpAddr::V6(ip)));
    }

    #[test]
    fn reserved_ipv6_discard_prefix() {
        let ip = "0100::1".parse::<Ipv6Addr>().unwrap();
        assert!(is_reserved_or_private_ip(&IpAddr::V6(ip)));
    }

    #[tokio::test]
    async fn reject_domain_resolving_to_loopback() {
        let connector = DirectConnector;
        let target = TargetAddr {
            host: TargetHost::Domain("localhost".to_string()),
            port: 1,
        };
        let result = Connector::connect(&connector, &target).await;
        assert!(matches!(result, Err(ConnectError::ReservedTarget(_))));
    }
}
