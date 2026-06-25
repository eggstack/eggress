use eggress_core::classify_upstream_chain;
use eggress_core::CapabilityResult;
use eggress_uri::ProtocolSpec;
use eggress_uri::ProxyChainSpec;

#[derive(Debug)]
pub enum UdpRelayCapability {
    SupportedSocks5,
    SupportedShadowsocks {
        method: eggress_protocol_shadowsocks::CipherMethod,
        password: String,
    },
    UnsupportedProtocol {
        protocol: String,
    },
    UnsupportedMultiHop,
}

fn extract_shadowsocks_creds(
    hop: &eggress_uri::ProxyHopSpec,
) -> Option<(eggress_protocol_shadowsocks::CipherMethod, String)> {
    if hop.protocols.len() == 1 && hop.protocols[0] == ProtocolSpec::Shadowsocks {
        if let Some(ref creds) = hop.credentials {
            let method =
                eggress_protocol_shadowsocks::CipherMethod::parse_method(&creds.username).ok()?;
            Some((method, creds.password.clone()))
        } else {
            None
        }
    } else {
        None
    }
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
                    ProtocolSpec::Shadowsocks => {
                        if let Some((method, password)) = extract_shadowsocks_creds(hop) {
                            UdpRelayCapability::SupportedShadowsocks { method, password }
                        } else {
                            UdpRelayCapability::UnsupportedProtocol {
                                protocol: "Shadowsocks (missing credentials)".to_string(),
                            }
                        }
                    }
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

pub fn udp_capability_from_chain(chain: &ProxyChainSpec) -> UdpRelayCapability {
    let caps = classify_upstream_chain(chain);
    match caps.udp_associate {
        CapabilityResult::Supported => {
            if chain.hops.len() == 1 {
                if let Some((method, password)) = extract_shadowsocks_creds(&chain.hops[0]) {
                    UdpRelayCapability::SupportedShadowsocks { method, password }
                } else if chain.hops[0].protocols.len() == 1
                    && chain.hops[0].protocols[0] == ProtocolSpec::Shadowsocks
                {
                    UdpRelayCapability::UnsupportedProtocol {
                        protocol: "Shadowsocks (missing credentials)".to_string(),
                    }
                } else {
                    UdpRelayCapability::SupportedSocks5
                }
            } else {
                UdpRelayCapability::SupportedSocks5
            }
        }
        CapabilityResult::UnsupportedProtocol { protocol } => {
            UdpRelayCapability::UnsupportedProtocol { protocol }
        }
        CapabilityResult::UnsupportedChain { reason } => {
            if reason == "multi-hop" {
                UdpRelayCapability::UnsupportedMultiHop
            } else if reason == "multi-protocol" && chain.hops.len() == 1 {
                UdpRelayCapability::UnsupportedProtocol {
                    protocol: format!("{:?}", chain.hops[0].protocols),
                }
            } else if reason == "direct" {
                UdpRelayCapability::UnsupportedProtocol {
                    protocol: "direct".to_string(),
                }
            } else {
                UdpRelayCapability::UnsupportedProtocol { protocol: reason }
            }
        }
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
            tls: false,
            server_name: None,
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
            tls: false,
            server_name: None,
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

    #[test]
    fn from_chain_matches_original_socks5() {
        let c = chain(vec![hop(vec![ProtocolSpec::Socks5])]);
        let original = udp_capability(&c);
        let new = udp_capability_from_chain(&c);
        assert_eq!(format!("{:?}", original), format!("{:?}", new));
    }

    #[test]
    fn from_chain_matches_original_socks5_with_creds() {
        let c = chain(vec![hop_with_creds(vec![ProtocolSpec::Socks5])]);
        let original = udp_capability(&c);
        let new = udp_capability_from_chain(&c);
        assert_eq!(format!("{:?}", original), format!("{:?}", new));
    }

    #[test]
    fn from_chain_matches_original_http() {
        let c = chain(vec![hop(vec![ProtocolSpec::Http])]);
        let original = udp_capability(&c);
        let new = udp_capability_from_chain(&c);
        assert_eq!(format!("{:?}", original), format!("{:?}", new));
    }

    #[test]
    fn from_chain_matches_original_socks4() {
        let c = chain(vec![hop(vec![ProtocolSpec::Socks4])]);
        let original = udp_capability(&c);
        let new = udp_capability_from_chain(&c);
        assert_eq!(format!("{:?}", original), format!("{:?}", new));
    }

    #[test]
    fn from_chain_matches_original_multi_protocol() {
        let c = chain(vec![hop(vec![ProtocolSpec::Http, ProtocolSpec::Socks5])]);
        let original = udp_capability(&c);
        let new = udp_capability_from_chain(&c);
        assert_eq!(format!("{:?}", original), format!("{:?}", new));
    }

    #[test]
    fn from_chain_matches_original_multi_hop() {
        let c = chain(vec![
            hop(vec![ProtocolSpec::Socks5]),
            hop(vec![ProtocolSpec::Http]),
        ]);
        let original = udp_capability(&c);
        let new = udp_capability_from_chain(&c);
        assert_eq!(format!("{:?}", original), format!("{:?}", new));
    }

    #[test]
    fn from_chain_matches_original_empty() {
        let c = chain(vec![]);
        let original = udp_capability(&c);
        let new = udp_capability_from_chain(&c);
        assert_eq!(format!("{:?}", original), format!("{:?}", new));
    }

    #[test]
    fn from_chain_matches_original_shadowsocks() {
        let c = chain(vec![hop_with_creds(vec![ProtocolSpec::Shadowsocks])]);
        let original = udp_capability(&c);
        let new = udp_capability_from_chain(&c);
        assert_eq!(format!("{:?}", original), format!("{:?}", new));
    }

    #[test]
    fn from_chain_matches_original_shadowsocks_no_creds() {
        let c = chain(vec![hop(vec![ProtocolSpec::Shadowsocks])]);
        let original = udp_capability(&c);
        let new = udp_capability_from_chain(&c);
        assert_eq!(format!("{:?}", original), format!("{:?}", new));
    }
}
