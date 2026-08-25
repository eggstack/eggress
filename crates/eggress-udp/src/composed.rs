//! Runtime construction for a composed UDP hop stack.

use crate::error::UdpError;
use crate::flow::ComposedUdpTargetFlow;
use crate::hop::{UdpHop, UdpHopStack};
use crate::upstream_socks5::{open_socks5_udp_upstream, Socks5UdpUpstreamConfig};
use eggress_core::{TargetAddr, TargetHost, UpstreamId};
use eggress_uri::ProxyChainSpec;
use std::net::SocketAddr;
#[cfg(feature = "shadowsocks")]
use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "shadowsocks")]
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Drop-forget of partially established hops: without this, a failure at hop
/// N would leak the control connections of hops 0..N-1 until their keepalive
/// windows expire.
fn discard_partial_control_channels(
    control_cancels: &mut Vec<CancellationToken>,
    control_tasks: &mut Vec<JoinHandle<()>>,
) {
    for cancel in control_cancels.drain(..) {
        cancel.cancel();
    }
    for task in control_tasks.drain(..) {
        task.abort();
    }
}

pub async fn open_composed_udp_upstream(
    upstream_id: UpstreamId,
    chain: ProxyChainSpec,
    udp_bind: SocketAddr,
    lease: eggress_routing::lease::ActiveLease,
) -> Result<ComposedUdpTargetFlow, UdpError> {
    let stack = UdpHopStack::from_chain(&chain)
        .map_err(|error| UdpError::Other(format!("invalid UDP hop stack: {error}")))?;
    let mut transport_targets = Vec::with_capacity(stack.hops().len());
    let mut control_cancels = Vec::new();
    let mut control_tasks: Vec<JoinHandle<()>> = Vec::new();
    let mut outer_socket = None;
    let mut outer_relay_addr = None;

    for (index, (hop, spec)) in stack.hops().iter().zip(chain.hops.iter()).enumerate() {
        match hop {
            UdpHop::Socks5 { .. } => {
                let association = match open_socks5_udp_upstream(
                    Socks5UdpUpstreamConfig {
                        upstream_id: upstream_id.clone(),
                        hop: spec.clone(),
                        connect_timeout: Duration::from_secs(10),
                        udp_bind,
                    },
                    None,
                )
                .await
                {
                    Ok(association) => association,
                    Err(error) => {
                        discard_partial_control_channels(&mut control_cancels, &mut control_tasks);
                        return Err(UdpError::Other(error.to_string()));
                    }
                };
                let relay = association.relay_addr;
                if index == 0 {
                    outer_socket = Some(association.udp_socket.clone());
                    outer_relay_addr = Some(relay);
                }
                control_cancels.push(association.control_cancel);
                control_tasks.push(association.control_task);
                transport_targets.push(relay);
            }
            #[cfg(feature = "shadowsocks")]
            UdpHop::Shadowsocks { .. } => {
                let endpoint = match resolve_target(hop.endpoint()).await {
                    Ok(endpoint) => endpoint,
                    Err(error) => {
                        discard_partial_control_channels(&mut control_cancels, &mut control_tasks);
                        return Err(error);
                    }
                };
                if index == 0 {
                    let socket = match UdpSocket::bind(udp_bind).await {
                        Ok(socket) => socket,
                        Err(error) => {
                            discard_partial_control_channels(
                                &mut control_cancels,
                                &mut control_tasks,
                            );
                            return Err(UdpError::Other(error.to_string()));
                        }
                    };
                    outer_socket = Some(Arc::new(socket));
                    outer_relay_addr = Some(endpoint);
                }
                transport_targets.push(endpoint);
            }
        }
    }

    let socket = outer_socket.ok_or_else(|| UdpError::Other("empty UDP hop stack".into()))?;
    let outer_relay_addr =
        outer_relay_addr.ok_or_else(|| UdpError::Other("missing outer UDP relay".into()))?;
    let relay_targets = transport_targets
        .into_iter()
        .skip(1)
        .map(socket_target)
        .collect();
    Ok(ComposedUdpTargetFlow {
        target: None,
        upstream_id,
        socket,
        outer_relay_addr,
        stack,
        relay_targets,
        control_cancels,
        control_tasks,
        lease,
        last_activity: std::time::Instant::now(),
    })
}

#[cfg(feature = "shadowsocks")]
async fn resolve_target(target: &TargetAddr) -> Result<SocketAddr, UdpError> {
    match &target.host {
        TargetHost::Ip(ip) => Ok(SocketAddr::new(*ip, target.port)),
        TargetHost::Domain(domain) => tokio::net::lookup_host((domain.as_str(), target.port))
            .await
            .map_err(|error| UdpError::Other(error.to_string()))?
            .next()
            .ok_or(UdpError::UnresolvedTarget),
    }
}

fn socket_target(addr: SocketAddr) -> TargetAddr {
    TargetAddr {
        host: TargetHost::Ip(addr.ip()),
        port: addr.port(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_uri::{CredentialSpec, EndpointSpec, ProtocolSpec, ProxyHopSpec};

    #[test]
    fn stack_construction_is_closed_over_udp_protocols() {
        let chain = ProxyChainSpec {
            hops: vec![ProxyHopSpec {
                protocols: vec![ProtocolSpec::Http],
                endpoint: EndpointSpec {
                    host: "127.0.0.1".into(),
                    port: 8080,
                },
                credentials: None,
                rule: None,
                local_bind: None,
                tls: false,
                server_name: None,
                insecure: false,
                plugins: vec![],
                auth_prefix: None,
            }],
        };
        assert!(UdpHopStack::from_chain(&chain).is_err());
        let _ = CredentialSpec {
            username: String::new(),
            password: String::new(),
        };
    }
}
