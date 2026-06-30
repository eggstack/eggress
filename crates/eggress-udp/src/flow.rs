use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use crate::direct::UdpTargetFlow;
use crate::error::UdpError;
use crate::limits::UdpLimits;
use crate::metrics::UdpMetrics;
use eggress_core::{TargetAddr, TargetHost, UpstreamId};
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_routing::lease::ActiveLease;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub fn local_udp_bind_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
}

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

        let target_addr = socks_to_target_addr(target);
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

pub struct ClientFlowState {
    pub last_activity: Instant,
    pub target_flows: HashMap<UdpFlowKey, TargetFlowEntry>,
}

impl ClientFlowState {
    pub fn new() -> Self {
        Self {
            last_activity: Instant::now(),
            target_flows: HashMap::new(),
        }
    }

    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }
}

impl Default for ClientFlowState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn total_target_flows(clients: &HashMap<SocketAddr, ClientFlowState>) -> usize {
    clients.values().map(|s| s.target_flows.len()).sum()
}

pub fn max_standalone_flows(limits: &UdpLimits) -> usize {
    if limits.max_standalone_flows > 0 {
        limits.max_standalone_flows
    } else {
        limits.max_associations_global
    }
}

pub fn can_use_flow(
    state: &ClientFlowState,
    key: &UdpFlowKey,
    total_flows: usize,
    limits: &UdpLimits,
) -> bool {
    state.target_flows.contains_key(key)
        || (state.target_flows.len() < limits.max_targets_per_association
            && total_flows < max_standalone_flows(limits))
}

pub fn shutdown_flow(entry: &TargetFlowEntry) {
    entry.recv_task.abort();
    if let UdpFlowKind::Socks5Upstream(flow) = &entry.flow {
        flow.control_cancel.cancel();
    }
}

pub fn reap_idle_flows(
    clients: &mut HashMap<SocketAddr, ClientFlowState>,
    limits: &UdpLimits,
    metrics: &UdpMetrics,
) {
    let now = Instant::now();
    let target_timeout = limits.target_idle_timeout;

    for state in clients.values_mut() {
        state.target_flows.retain(|_, entry| {
            let keep = now.duration_since(entry.last_activity()) < target_timeout;
            if !keep {
                shutdown_flow(entry);
                metrics.record_standalone_flow_reap();
            }
            keep
        });
    }

    let client_timeout = limits.idle_timeout;
    clients.retain(|_, state| now.duration_since(state.last_activity) < client_timeout);
}

pub fn close_all_flows(clients: &mut HashMap<SocketAddr, ClientFlowState>, metrics: &UdpMetrics) {
    for state in clients.values_mut() {
        for entry in state.target_flows.drain().map(|(_, entry)| entry) {
            shutdown_flow(&entry);
            metrics.record_standalone_flow_closed();
        }
    }
}

pub fn socks_to_target_addr(addr: &SocksAddr) -> TargetAddr {
    match addr {
        SocksAddr::IPv4(octets, port) => TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V4((*octets).into())),
            port: *port,
        },
        SocksAddr::IPv6(octets, port) => TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V6((*octets).into())),
            port: *port,
        },
        SocksAddr::Domain(domain, port) => TargetAddr {
            host: TargetHost::Domain(domain.clone()),
            port: *port,
        },
    }
}

pub fn target_to_socks_addr(target: &TargetAddr) -> SocksAddr {
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => SocksAddr::IPv4(ip.octets(), target.port),
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => SocksAddr::IPv6(ip.octets(), target.port),
        TargetHost::Domain(domain) => SocksAddr::Domain(domain.clone(), target.port),
    }
}

pub fn socks_addr_equivalent(a: &SocksAddr, b: &SocksAddr) -> bool {
    match (a, b) {
        (SocksAddr::IPv4(a_addr, a_port), SocksAddr::IPv4(b_addr, b_port)) => {
            a_addr == b_addr && a_port == b_port
        }
        (SocksAddr::IPv6(a_addr, a_port), SocksAddr::IPv6(b_addr, b_port)) => {
            a_addr == b_addr && a_port == b_port
        }
        (SocksAddr::IPv4(a_addr, a_port), SocksAddr::IPv6(b_addr, b_port)) => {
            matches!(
                std::net::IpAddr::from(*b_addr),
                std::net::IpAddr::V4(v4) if v4.octets() == *a_addr && a_port == b_port
            )
        }
        (SocksAddr::IPv6(a_addr, a_port), SocksAddr::IPv4(b_addr, b_port)) => {
            matches!(
                std::net::IpAddr::from(*a_addr),
                std::net::IpAddr::V4(v4) if v4.octets() == *b_addr && a_port == b_port
            )
        }
        (SocksAddr::Domain(a_dom, a_port), SocksAddr::Domain(b_dom, b_port)) => {
            a_dom == b_dom && a_port == b_port
        }
        _ => false,
    }
}

pub async fn resolve_endpoint(
    endpoint: &eggress_uri::EndpointSpec,
) -> Result<SocketAddr, UdpError> {
    let mut addrs = tokio::net::lookup_host((endpoint.host.as_str(), endpoint.port)).await?;
    addrs.next().ok_or(UdpError::UnresolvedTarget)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_endpoint_accepts_ipv6_literals_without_brackets() {
        let endpoint = eggress_uri::EndpointSpec {
            host: "::1".to_string(),
            port: 8388,
        };

        let addr = resolve_endpoint(&endpoint).await.unwrap();

        assert_eq!(addr, SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 8388)));
    }

    #[test]
    fn local_udp_bind_addr_is_loopback_ephemeral() {
        assert_eq!(local_udp_bind_addr(), SocketAddr::from(([127, 0, 0, 1], 0)));
    }
}
