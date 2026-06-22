use eggress_uri::ProtocolSpec;
use eggress_uri::ProxyChainSpec;

pub enum UdpRelayCapability {
    SupportedSocks5,
    UnsupportedProtocol { protocol: String },
    UnsupportedMultiHop,
}

pub fn udp_capability(chain: &ProxyChainSpec) -> UdpRelayCapability {
    match chain.hops.len() {
        0 => UdpRelayCapability::UnsupportedProtocol {
            protocol: "direct".to_string(),
        },
        1 => {
            let hop = &chain.hops[0];
            if hop.protocols.len() == 1 {
                match &hop.protocols[0] {
                    ProtocolSpec::Socks5 => UdpRelayCapability::SupportedSocks5,
                    other => UdpRelayCapability::UnsupportedProtocol {
                        protocol: format!("{:?}", other),
                    },
                }
            } else {
                UdpRelayCapability::UnsupportedProtocol {
                    protocol: format!("{:?}", hop.protocols),
                }
            }
        }
        _ => UdpRelayCapability::UnsupportedMultiHop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_uri::{CredentialSpec, EndpointSpec, ProxyHopSpec};

    fn chain(hops: Vec<ProxyHopSpec>) -> ProxyChainSpec {
        ProxyChainSpec { hops }
    }

    fn hop(protocols: Vec<ProtocolSpec>) -> ProxyHopSpec {
        ProxyHopSpec {
            protocols,
            endpoint: EndpointSpec {
                host: "proxy.example".to_string(),
                port: 1080,
            },
            credentials: None,
            rule: None,
            local_bind: None,
        }
    }

    fn hop_with_creds(protocols: Vec<ProtocolSpec>) -> ProxyHopSpec {
        ProxyHopSpec {
            protocols,
            endpoint: EndpointSpec {
                host: "proxy.example".to_string(),
                port: 1080,
            },
            credentials: Some(CredentialSpec {
                username: "user".to_string(),
                password: "pass".to_string(),
            }),
            rule: None,
            local_bind: None,
        }
    }

    #[test]
    fn single_socks5_hop_supported() {
        let c = chain(vec![hop(vec![ProtocolSpec::Socks5])]);
        assert!(matches!(
            udp_capability(&c),
            UdpRelayCapability::SupportedSocks5
        ));
    }

    #[test]
    fn single_socks5_with_credentials_supported() {
        let c = chain(vec![hop_with_creds(vec![ProtocolSpec::Socks5])]);
        assert!(matches!(
            udp_capability(&c),
            UdpRelayCapability::SupportedSocks5
        ));
    }

    #[test]
    fn single_http_hop_unsupported() {
        let c = chain(vec![hop(vec![ProtocolSpec::Http])]);
        let result = udp_capability(&c);
        assert!(matches!(
            result,
            UdpRelayCapability::UnsupportedProtocol { ref protocol }
            if protocol == "Http"
        ));
    }

    #[test]
    fn single_socks4_hop_unsupported() {
        let c = chain(vec![hop(vec![ProtocolSpec::Socks4])]);
        let result = udp_capability(&c);
        assert!(matches!(
            result,
            UdpRelayCapability::UnsupportedProtocol { ref protocol }
            if protocol == "Socks4"
        ));
    }

    #[test]
    fn multi_protocol_hop_unsupported() {
        let c = chain(vec![hop(vec![ProtocolSpec::Http, ProtocolSpec::Socks5])]);
        let result = udp_capability(&c);
        assert!(matches!(
            result,
            UdpRelayCapability::UnsupportedProtocol { .. }
        ));
    }

    #[test]
    fn multi_hop_chain_unsupported() {
        let c = chain(vec![
            hop(vec![ProtocolSpec::Socks5]),
            hop(vec![ProtocolSpec::Http]),
        ]);
        assert!(matches!(
            udp_capability(&c),
            UdpRelayCapability::UnsupportedMultiHop
        ));
    }

    #[test]
    fn empty_hops_unsupported_direct() {
        let c = chain(vec![]);
        let result = udp_capability(&c);
        assert!(matches!(
            result,
            UdpRelayCapability::UnsupportedProtocol { ref protocol }
            if protocol == "direct"
        ));
    }
}
