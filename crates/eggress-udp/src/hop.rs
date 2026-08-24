//! Transport-neutral UDP hop codecs.
//!
//! A stack is encoded from the destination inward (last hop first) and
//! decoded from the outside inward. This keeps protocol framing independent
//! from the flow registry and avoids a pairwise chain matrix.

use eggress_core::{TargetAddr, TargetHost};
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_uri::{ProtocolSpec, ProxyChainSpec, ProxyHopSpec};

#[derive(Debug, thiserror::Error)]
pub enum UdpHopError {
    #[error("UDP hop must contain exactly one protocol")]
    MultipleProtocols,
    #[error("protocol {0:?} has no UDP implementation")]
    UnsupportedProtocol(ProtocolSpec),
    #[error("Shadowsocks UDP hop is missing credentials or has an invalid method")]
    InvalidShadowsocksCredentials,
    #[error("UDP hop codec failed: {0}")]
    Codec(String),
}

#[derive(Debug, Clone)]
pub enum UdpHop {
    Socks5 {
        endpoint: TargetAddr,
    },
    #[cfg(feature = "shadowsocks")]
    Shadowsocks {
        endpoint: TargetAddr,
        method: eggress_protocol_shadowsocks::CipherMethod,
        password: Vec<u8>,
        password_ikm: Vec<u8>,
    },
}

impl UdpHop {
    pub fn endpoint(&self) -> &TargetAddr {
        match self {
            Self::Socks5 { endpoint } => endpoint,
            #[cfg(feature = "shadowsocks")]
            Self::Shadowsocks { endpoint, .. } => endpoint,
        }
    }

    fn encode(&self, target: &TargetAddr, payload: &[u8]) -> Result<Vec<u8>, UdpHopError> {
        match self {
            Self::Socks5 { .. } => {
                let mut out = Vec::new();
                encode_socks_target(target, payload, &mut out)?;
                Ok(out)
            }
            #[cfg(feature = "shadowsocks")]
            Self::Shadowsocks {
                method,
                password_ikm,
                ..
            } => {
                let mut salt_buf = [0u8; 32];
                rand::RngCore::fill_bytes(
                    &mut rand::thread_rng(),
                    &mut salt_buf[..method.salt_size()],
                );
                let salt = &salt_buf[..method.salt_size()];
                eggress_protocol_shadowsocks::udp::encode_udp_packet_with_ikm(
                    *method,
                    password_ikm,
                    target,
                    payload,
                    salt,
                )
                .map_err(|error| UdpHopError::Codec(error.to_string()))
            }
        }
    }

    fn decode(&self, packet: &[u8]) -> Result<(TargetAddr, Vec<u8>), UdpHopError> {
        match self {
            Self::Socks5 { .. } => {
                let decoded =
                    eggress_protocol_socks::socks5::udp_codec::decode_socks5_udp_datagram(packet)
                        .map_err(|error| UdpHopError::Codec(error.to_string()))?;
                Ok((socks_to_target(&decoded.target), decoded.payload.to_vec()))
            }
            #[cfg(feature = "shadowsocks")]
            Self::Shadowsocks {
                method,
                password_ikm,
                ..
            } => eggress_protocol_shadowsocks::udp::decode_udp_packet_with_ikm(
                *method,
                password_ikm,
                packet,
            )
            .map_err(|error| UdpHopError::Codec(error.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UdpHopStack {
    hops: Vec<UdpHop>,
}

impl UdpHopStack {
    pub fn from_chain(chain: &ProxyChainSpec) -> Result<Self, UdpHopError> {
        let mut hops = Vec::with_capacity(chain.hops.len());
        for hop in &chain.hops {
            hops.push(UdpHop::from_spec(hop)?);
        }
        Ok(Self { hops })
    }

    pub fn from_hops(hops: Vec<UdpHop>) -> Result<Self, UdpHopError> {
        if hops.is_empty() {
            return Err(UdpHopError::Codec("empty UDP hop stack".into()));
        }
        Ok(Self { hops })
    }

    pub fn hops(&self) -> &[UdpHop] {
        &self.hops
    }

    pub fn encode_request(
        &self,
        target: &TargetAddr,
        payload: &[u8],
    ) -> Result<Vec<u8>, UdpHopError> {
        self.encode_request_with_next_targets(target, payload, &[])
    }

    /// Encode nested datagrams. next_targets[i] can replace the configured
    /// endpoint of hop i + 1 with a resolved UDP relay address.
    pub fn encode_request_with_next_targets(
        &self,
        target: &TargetAddr,
        payload: &[u8],
        next_targets: &[TargetAddr],
    ) -> Result<Vec<u8>, UdpHopError> {
        let mut packet = payload.to_vec();
        for index in (0..self.hops.len()).rev() {
            let next_target = if index + 1 == self.hops.len() {
                target.clone()
            } else {
                next_targets
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| self.hops[index + 1].endpoint().clone())
            };
            packet = self.hops[index].encode(&next_target, &packet)?;
        }
        Ok(packet)
    }

    /// Decode in reverse wire order: outer hop first, final destination hop
    /// last.
    pub fn decode_response(&self, packet: &[u8]) -> Result<(TargetAddr, Vec<u8>), UdpHopError> {
        let mut target = None;
        let mut packet = std::borrow::Cow::Borrowed(packet);
        for hop in &self.hops {
            let (decoded_target, decoded_payload) = hop.decode(&packet)?;
            target = Some(decoded_target);
            packet = std::borrow::Cow::Owned(decoded_payload);
        }
        Ok((
            target.ok_or_else(|| UdpHopError::Codec("empty UDP hop stack".into()))?,
            packet.into_owned(),
        ))
    }
}

impl UdpHop {
    fn from_spec(hop: &ProxyHopSpec) -> Result<Self, UdpHopError> {
        if hop.protocols.len() != 1 {
            return Err(UdpHopError::MultipleProtocols);
        }
        let endpoint = endpoint_target(&hop.endpoint.host, hop.endpoint.port);
        match hop.protocols[0] {
            ProtocolSpec::Socks5 => Ok(Self::Socks5 { endpoint }),
            #[cfg(feature = "shadowsocks")]
            ProtocolSpec::Shadowsocks => {
                let credentials = hop
                    .credentials
                    .as_ref()
                    .ok_or(UdpHopError::InvalidShadowsocksCredentials)?;
                let method =
                    eggress_protocol_shadowsocks::CipherMethod::parse_method(&credentials.username)
                        .map_err(|_| UdpHopError::InvalidShadowsocksCredentials)?;
                let password = credentials.password.as_bytes().to_vec();
                let password_ikm =
                    eggress_protocol_shadowsocks::CipherMethod::password_key_material(&password);
                Ok(Self::Shadowsocks {
                    endpoint,
                    method,
                    password,
                    password_ikm,
                })
            }
            protocol => Err(UdpHopError::UnsupportedProtocol(protocol)),
        }
    }
}

fn endpoint_target(host: &str, port: u16) -> TargetAddr {
    let host = host
        .parse()
        .map(TargetHost::Ip)
        .unwrap_or_else(|_| TargetHost::Domain(host.to_string()));
    TargetAddr { host, port }
}

fn socks_to_target(addr: &SocksAddr) -> TargetAddr {
    match addr {
        SocksAddr::IPv4(ip, port) => TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V4((*ip).into())),
            port: *port,
        },
        SocksAddr::IPv6(ip, port) => TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V6((*ip).into())),
            port: *port,
        },
        SocksAddr::Domain(domain, port) => TargetAddr {
            host: TargetHost::Domain(domain.clone()),
            port: *port,
        },
    }
}

fn target_to_socks(addr: &TargetAddr) -> SocksAddr {
    match &addr.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => SocksAddr::IPv4(ip.octets(), addr.port),
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => SocksAddr::IPv6(ip.octets(), addr.port),
        TargetHost::Domain(domain) => SocksAddr::Domain(domain.clone(), addr.port),
    }
}

fn encode_socks_target(
    target: &TargetAddr,
    payload: &[u8],
    out: &mut Vec<u8>,
) -> Result<(), UdpHopError> {
    eggress_protocol_socks::socks5::udp_codec::encode_socks5_udp_datagram(
        &target_to_socks(target),
        payload,
        out,
    )
    .map_err(|error| UdpHopError::Codec(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_uri::{EndpointSpec, ProxyHopSpec};

    fn hop(protocol: ProtocolSpec, host: &str, port: u16) -> ProxyHopSpec {
        ProxyHopSpec {
            protocols: vec![protocol],
            endpoint: EndpointSpec {
                host: host.into(),
                port,
            },
            credentials: None,
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
            insecure: false,
            plugins: Vec::new(),
            auth_prefix: None,
        }
    }

    #[test]
    fn nested_socks5_encoding_and_reverse_decoding() {
        let chain = ProxyChainSpec {
            hops: vec![
                hop(ProtocolSpec::Socks5, "outer.example", 1080),
                hop(ProtocolSpec::Socks5, "inner.example", 1081),
            ],
        };
        let stack = UdpHopStack::from_chain(&chain).unwrap();
        let target = endpoint_target("target.example", 443);
        let packet = stack.encode_request(&target, b"payload").unwrap();
        let (outer_target, outer_payload) = stack.hops()[0].decode(&packet).unwrap();
        assert_eq!(outer_target, endpoint_target("inner.example", 1081));
        let (inner_target, payload) = stack.hops()[1].decode(&outer_payload).unwrap();
        assert_eq!(inner_target, target);
        assert_eq!(payload, b"payload");
    }

    #[cfg(feature = "shadowsocks")]
    #[test]
    fn preserves_ipv4_ipv6_and_domain_targets() {
        let mut chain = ProxyChainSpec {
            hops: vec![hop(ProtocolSpec::Shadowsocks, "proxy.example", 8388)],
        };
        chain.hops[0].credentials = Some(eggress_uri::CredentialSpec {
            username: "aes-256-gcm".into(),
            password: "secret".into(),
        });
        let stack = UdpHopStack::from_chain(&chain).unwrap();
        for target in [
            endpoint_target("127.0.0.1", 53),
            endpoint_target("::1", 5353),
            endpoint_target("domain.example", 443),
        ] {
            let packet = stack.encode_request(&target, b"x").unwrap();
            let (decoded, payload) = stack.decode_response(&packet).unwrap();
            assert_eq!(decoded, target);
            assert_eq!(payload, b"x");
        }
    }

    #[test]
    fn rejects_non_udp_protocols() {
        let chain = ProxyChainSpec {
            hops: vec![hop(ProtocolSpec::Http, "proxy.example", 8080)],
        };
        assert!(matches!(
            UdpHopStack::from_chain(&chain),
            Err(UdpHopError::UnsupportedProtocol(ProtocolSpec::Http))
        ));
    }
}
