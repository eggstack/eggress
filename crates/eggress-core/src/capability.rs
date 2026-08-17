use eggress_uri::ProtocolSpec;
use eggress_uri::ProxyChainSpec;

/// Transport capability for upstream chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportCapability {
    TcpConnect,
    UdpAssociate,
}

/// Result of checking a specific transport capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityResult {
    Supported,
    UnsupportedProtocol { protocol: String },
    UnsupportedChain { reason: String },
}

/// Combined capabilities for an upstream chain.
#[derive(Debug, Clone)]
pub struct UpstreamCapabilities {
    pub tcp_connect: CapabilityResult,
    pub udp_associate: CapabilityResult,
}

impl UpstreamCapabilities {
    pub fn is_tcp_supported(&self) -> bool {
        self.tcp_connect == CapabilityResult::Supported
    }

    pub fn is_udp_supported(&self) -> bool {
        self.udp_associate == CapabilityResult::Supported
    }
}

/// Classify the capabilities of a proxy chain.
///
/// Rules:
/// - Direct route (0 hops): TCP and UDP are not handled via upstream capability
/// - HTTP upstream: TCP CONNECT supported; UDP unsupported
/// - SOCKS4 upstream: TCP CONNECT supported; UDP unsupported
/// - SOCKS5 upstream: TCP CONNECT supported; UDP supported
/// - Shadowsocks upstream: TCP not advertised (non-standard AEAD framing);
///   UDP supported (standard AEAD format)
/// - Multi-hop: TCP may be supported; UDP is supported when every hop has a
///   proven UDP codec (SOCKS5 or Shadowsocks)
pub fn classify_upstream_chain(chain: &ProxyChainSpec) -> UpstreamCapabilities {
    match chain.hops.len() {
        0 => UpstreamCapabilities {
            tcp_connect: CapabilityResult::UnsupportedChain {
                reason: "direct".to_string(),
            },
            udp_associate: CapabilityResult::UnsupportedChain {
                reason: "direct".to_string(),
            },
        },
        1 => {
            let hop = &chain.hops[0];
            if hop.protocols.len() == 1 {
                classify_single_protocol(hop.protocols[0])
            } else if hop.protocols.contains(&ProtocolSpec::Quic)
                && hop.protocols.iter().any(|protocol| {
                    matches!(
                        protocol,
                        ProtocolSpec::Http
                            | ProtocolSpec::HttpOnly
                            | ProtocolSpec::Socks4
                            | ProtocolSpec::Socks5
                            | ProtocolSpec::Raw
                    )
                })
            {
                UpstreamCapabilities {
                    tcp_connect: CapabilityResult::Supported,
                    udp_associate: CapabilityResult::UnsupportedProtocol {
                        protocol: "QUIC UDP stream mapping".to_string(),
                    },
                }
            } else {
                UpstreamCapabilities {
                    tcp_connect: CapabilityResult::UnsupportedChain {
                        reason: "multi-protocol".to_string(),
                    },
                    udp_associate: CapabilityResult::UnsupportedChain {
                        reason: "multi-protocol".to_string(),
                    },
                }
            }
        }
        _ if chain.hops.len() > 1 && chain.hops[0].protocols.contains(&ProtocolSpec::Quic) => {
            UpstreamCapabilities {
                tcp_connect: CapabilityResult::Supported,
                udp_associate: CapabilityResult::UnsupportedChain {
                    reason: "QUIC UDP stream mapping is only supported at the first hop"
                        .to_string(),
                },
            }
        }
        _ => {
            let udp_supported = chain.hops.iter().all(|hop| {
                hop.protocols.len() == 1
                    && matches!(
                        hop.protocols[0],
                        ProtocolSpec::Socks5 | ProtocolSpec::Shadowsocks
                    )
            });
            UpstreamCapabilities {
                tcp_connect: CapabilityResult::Supported,
                udp_associate: if udp_supported {
                    CapabilityResult::Supported
                } else {
                    CapabilityResult::UnsupportedChain {
                        reason: "multi-hop contains a non-UDP protocol".to_string(),
                    }
                },
            }
        }
    }
}

fn classify_single_protocol(protocol: ProtocolSpec) -> UpstreamCapabilities {
    match protocol {
        ProtocolSpec::Http | ProtocolSpec::HttpOnly => UpstreamCapabilities {
            tcp_connect: CapabilityResult::Supported,
            udp_associate: CapabilityResult::UnsupportedProtocol {
                protocol: "Http".to_string(),
            },
        },
        ProtocolSpec::Socks4 => UpstreamCapabilities {
            tcp_connect: CapabilityResult::Supported,
            udp_associate: CapabilityResult::UnsupportedProtocol {
                protocol: "Socks4".to_string(),
            },
        },
        ProtocolSpec::Socks5 => UpstreamCapabilities {
            tcp_connect: CapabilityResult::Supported,
            udp_associate: CapabilityResult::Supported,
        },
        ProtocolSpec::Shadowsocks => UpstreamCapabilities {
            tcp_connect: CapabilityResult::Supported,
            udp_associate: CapabilityResult::Supported,
        },
        ProtocolSpec::ShadowsocksR => UpstreamCapabilities {
            tcp_connect: CapabilityResult::Supported,
            udp_associate: CapabilityResult::UnsupportedProtocol {
                protocol: "ShadowsocksR".to_string(),
            },
        },
        ProtocolSpec::Trojan => UpstreamCapabilities {
            tcp_connect: CapabilityResult::Supported,
            udp_associate: CapabilityResult::UnsupportedProtocol {
                protocol: "Trojan".to_string(),
            },
        },
        ProtocolSpec::Http2 => UpstreamCapabilities {
            tcp_connect: CapabilityResult::Supported,
            udp_associate: CapabilityResult::UnsupportedProtocol {
                protocol: "Http2".to_string(),
            },
        },
        ProtocolSpec::Http3 => UpstreamCapabilities {
            tcp_connect: CapabilityResult::Supported,
            udp_associate: CapabilityResult::UnsupportedProtocol {
                protocol: "Http3".to_string(),
            },
        },
        ProtocolSpec::Quic => UpstreamCapabilities {
            tcp_connect: CapabilityResult::UnsupportedChain {
                reason: "QUIC requires an application protocol".to_string(),
            },
            udp_associate: CapabilityResult::UnsupportedProtocol {
                protocol: "Quic".to_string(),
            },
        },
        ProtocolSpec::WebSocket => UpstreamCapabilities {
            tcp_connect: CapabilityResult::Supported,
            udp_associate: CapabilityResult::UnsupportedProtocol {
                protocol: "WebSocket".to_string(),
            },
        },
        ProtocolSpec::Raw | ProtocolSpec::Ssh | ProtocolSpec::Unix => UpstreamCapabilities {
            tcp_connect: CapabilityResult::Supported,
            udp_associate: CapabilityResult::UnsupportedProtocol {
                protocol: "Raw".to_string(),
            },
        },
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
            insecure: false,
            plugins: Vec::new(),
            auth_prefix: None,
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
            insecure: false,
            plugins: Vec::new(),
            auth_prefix: None,
        }
    }

    #[test]
    fn single_socks5_hop_supported() {
        let c = chain(vec![hop(vec![ProtocolSpec::Socks5])]);
        let caps = classify_upstream_chain(&c);
        assert!(caps.is_tcp_supported());
        assert!(caps.is_udp_supported());
        assert_eq!(caps.tcp_connect, CapabilityResult::Supported);
        assert_eq!(caps.udp_associate, CapabilityResult::Supported);
    }

    #[test]
    fn single_socks5_with_credentials_supported() {
        let c = chain(vec![hop_with_creds(vec![ProtocolSpec::Socks5])]);
        let caps = classify_upstream_chain(&c);
        assert!(caps.is_tcp_supported());
        assert!(caps.is_udp_supported());
    }

    #[test]
    fn single_http_hop() {
        let c = chain(vec![hop(vec![ProtocolSpec::Http])]);
        let caps = classify_upstream_chain(&c);
        assert!(caps.is_tcp_supported());
        assert!(!caps.is_udp_supported());
        assert_eq!(caps.tcp_connect, CapabilityResult::Supported);
        assert_eq!(
            caps.udp_associate,
            CapabilityResult::UnsupportedProtocol {
                protocol: "Http".to_string()
            }
        );
    }

    #[test]
    fn single_socks4_hop() {
        let c = chain(vec![hop(vec![ProtocolSpec::Socks4])]);
        let caps = classify_upstream_chain(&c);
        assert!(caps.is_tcp_supported());
        assert!(!caps.is_udp_supported());
        assert_eq!(
            caps.udp_associate,
            CapabilityResult::UnsupportedProtocol {
                protocol: "Socks4".to_string()
            }
        );
    }

    #[test]
    fn single_shadowsocks_hop() {
        let c = chain(vec![hop(vec![ProtocolSpec::Shadowsocks])]);
        let caps = classify_upstream_chain(&c);
        assert!(caps.is_tcp_supported());
        assert!(caps.is_udp_supported());
        assert_eq!(caps.tcp_connect, CapabilityResult::Supported);
        assert_eq!(caps.udp_associate, CapabilityResult::Supported);
    }

    #[test]
    fn multi_protocol_hop_unsupported() {
        let c = chain(vec![hop(vec![ProtocolSpec::Http, ProtocolSpec::Socks5])]);
        let caps = classify_upstream_chain(&c);
        assert!(!caps.is_tcp_supported());
        assert!(!caps.is_udp_supported());
        assert_eq!(
            caps.tcp_connect,
            CapabilityResult::UnsupportedChain {
                reason: "multi-protocol".to_string()
            }
        );
    }

    #[test]
    fn multi_hop_chain_tcp_supported_udp_unsupported() {
        let c = chain(vec![
            hop(vec![ProtocolSpec::Socks5]),
            hop(vec![ProtocolSpec::Http]),
        ]);
        let caps = classify_upstream_chain(&c);
        assert!(caps.is_tcp_supported());
        assert!(!caps.is_udp_supported());
        assert_eq!(caps.tcp_connect, CapabilityResult::Supported);
        assert_eq!(
            caps.udp_associate,
            CapabilityResult::UnsupportedChain {
                reason: "multi-hop contains a non-UDP protocol".to_string()
            }
        );
    }

    #[test]
    fn empty_hops_direct() {
        let c = chain(vec![]);
        let caps = classify_upstream_chain(&c);
        assert!(!caps.is_tcp_supported());
        assert!(!caps.is_udp_supported());
        assert_eq!(
            caps.tcp_connect,
            CapabilityResult::UnsupportedChain {
                reason: "direct".to_string()
            }
        );
        assert_eq!(
            caps.udp_associate,
            CapabilityResult::UnsupportedChain {
                reason: "direct".to_string()
            }
        );
    }

    #[test]
    fn unsupported_reason_labels_stable() {
        let c = chain(vec![]);
        let caps = classify_upstream_chain(&c);
        match &caps.tcp_connect {
            CapabilityResult::UnsupportedChain { reason } => {
                assert_eq!(reason, "direct");
            }
            _ => panic!("expected UnsupportedChain"),
        }

        let c = chain(vec![
            hop(vec![ProtocolSpec::Socks5]),
            hop(vec![ProtocolSpec::Http]),
        ]);
        let caps = classify_upstream_chain(&c);
        match &caps.udp_associate {
            CapabilityResult::UnsupportedChain { reason } => {
                assert_eq!(reason, "multi-hop contains a non-UDP protocol");
            }
            _ => panic!("expected UnsupportedChain"),
        }

        let c = chain(vec![hop(vec![ProtocolSpec::Http])]);
        let caps = classify_upstream_chain(&c);
        match &caps.udp_associate {
            CapabilityResult::UnsupportedProtocol { protocol } => {
                assert_eq!(protocol, "Http");
            }
            _ => panic!("expected UnsupportedProtocol"),
        }
    }
}
