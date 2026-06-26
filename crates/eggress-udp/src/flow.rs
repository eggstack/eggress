use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use crate::direct::UdpTargetFlow;
use eggress_core::UpstreamId;
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_routing::lease::ActiveLease;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UdpFlowKey {
    Direct {
        target: SocksAddr,
    },
    Socks5Upstream {
        target: SocksAddr,
        upstream_id: UpstreamId,
    },
    ShadowsocksUpstream {
        target: SocksAddr,
        upstream_id: UpstreamId,
    },
}

impl UdpFlowKey {
    pub fn target(&self) -> &SocksAddr {
        match self {
            UdpFlowKey::Direct { target } => target,
            UdpFlowKey::Socks5Upstream { target, .. } => target,
            UdpFlowKey::ShadowsocksUpstream { target, .. } => target,
        }
    }
}

pub enum UdpFlowKind {
    Direct(UdpTargetFlow),
    Socks5Upstream(Socks5UdpTargetFlow),
    ShadowsocksUpstream(ShadowsocksUdpTargetFlow),
}

pub struct Socks5UdpTargetFlow {
    pub target: SocksAddr,
    pub upstream_id: UpstreamId,
    pub upstream_relay_addr: SocketAddr,
    pub udp_socket: Arc<tokio::net::UdpSocket>,
    pub control_cancel: CancellationToken,
    pub control_task: JoinHandle<()>,
    pub lease: ActiveLease,
    pub last_activity: Instant,
}

impl Socks5UdpTargetFlow {
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn last_activity(&self) -> Instant {
        self.last_activity
    }

    pub async fn send(&self, target: &SocksAddr, payload: &[u8]) -> Result<(), std::io::Error> {
        use crate::codec::encode_socks5_udp_datagram;
        let mut out = Vec::new();
        encode_socks5_udp_datagram(target, payload, &mut out);
        self.udp_socket
            .send_to(&out, self.upstream_relay_addr)
            .await?;
        Ok(())
    }
}

pub struct ShadowsocksUdpTargetFlow {
    pub target: SocksAddr,
    pub upstream_id: UpstreamId,
    pub upstream_addr: SocketAddr,
    pub udp_socket: Arc<tokio::net::UdpSocket>,
    pub method: eggress_protocol_shadowsocks::CipherMethod,
    pub password: Vec<u8>,
    pub lease: ActiveLease,
    pub last_activity: Instant,
}

impl ShadowsocksUdpTargetFlow {
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn last_activity(&self) -> Instant {
        self.last_activity
    }

    pub async fn send(
        &self,
        target: &SocksAddr,
        payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use eggress_protocol_shadowsocks::udp::encode_udp_packet;
        use rand::RngCore;

        let target_addr = socks_to_shadowsocks_target(target)?;
        let mut salt = vec![0u8; self.method.salt_size()];
        rand::thread_rng().fill_bytes(&mut salt);
        let packet = encode_udp_packet(self.method, &self.password, &target_addr, payload, &salt)?;
        self.udp_socket.send_to(&packet, self.upstream_addr).await?;
        Ok(())
    }
}

pub struct TargetFlowEntry {
    pub flow: UdpFlowKind,
    pub recv_task: JoinHandle<()>,
}

impl TargetFlowEntry {
    pub fn touch(&mut self) {
        match &mut self.flow {
            UdpFlowKind::Direct(f) => f.touch(),
            UdpFlowKind::Socks5Upstream(f) => f.touch(),
            UdpFlowKind::ShadowsocksUpstream(f) => f.touch(),
        }
    }

    pub fn last_activity(&self) -> Instant {
        match &self.flow {
            UdpFlowKind::Direct(f) => f.last_activity,
            UdpFlowKind::Socks5Upstream(f) => f.last_activity(),
            UdpFlowKind::ShadowsocksUpstream(f) => f.last_activity(),
        }
    }
}

fn socks_to_shadowsocks_target(
    addr: &SocksAddr,
) -> Result<eggress_core::TargetAddr, Box<dyn std::error::Error + Send + Sync>> {
    use eggress_core::{TargetAddr, TargetHost};
    match addr {
        SocksAddr::IPv4(octets, port) => Ok(TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V4((*octets).into())),
            port: *port,
        }),
        SocksAddr::IPv6(octets, port) => Ok(TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V6((*octets).into())),
            port: *port,
        }),
        SocksAddr::Domain(domain, port) => Ok(TargetAddr {
            host: TargetHost::Domain(domain.clone()),
            port: *port,
        }),
    }
}
