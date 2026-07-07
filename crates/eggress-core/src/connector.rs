use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use tokio::net::TcpStream;

use crate::{BoxStream, ConnectError, TargetAddr, TargetHost};

/// Returns `true` if the IP address is reserved, private, or otherwise
/// unsuitable for direct outbound connections (DNS rebinding protection).
///
/// Used as a domain-resolution guard: after resolving a domain name,
/// this checks whether the result points to a private/reserved range.
/// Literal IP targets bypass this check for pproxy compatibility.
///
/// Rejected ranges:
/// - IPv4: loopback (127.0.0.0/8), link-local (169.254.0.0/16),
///   private (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16), unspecified (0.0.0.0)
/// - IPv6: loopback (::1), link-local (fe80::/10), unique-local (fc00::/7),
///   unspecified (::)
pub fn is_reserved_or_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_link_local() || v4.is_private() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || is_unicast_link_local_v6(v6)
                || is_unique_local_v6(v6)
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
        let ip = "2001:db8::1".parse::<Ipv6Addr>().unwrap();
        assert!(!is_reserved_or_private_ip(&IpAddr::V6(ip)));
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
