use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::UdpSocket;

use crate::error::UdpError;
use eggress_core::connector::is_reserved_or_private_ip;
use eggress_protocol_socks::socks5::server::SocksAddr;

pub struct UdpTargetFlow {
    pub target: SocksAddr,
    pub socket: Arc<UdpSocket>,
    pub last_activity: Instant,
    pub packets_up: AtomicU64,
    pub packets_down: AtomicU64,
    pub bytes_up: AtomicU64,
    pub bytes_down: AtomicU64,
}

impl UdpTargetFlow {
    /// Open a direct datagram flow to `target`.
    ///
    /// `allow_private_egress` carries the listener's destination policy into
    /// the post-DNS check: with the policy restrictive, every answer of the
    /// single `lookup_host` call is filtered and only a permitted address is
    /// connected.
    pub async fn new(
        target: SocksAddr,
        local_bind: SocketAddr,
        allow_private_egress: bool,
    ) -> Result<Self, UdpError> {
        let socket = UdpSocket::bind(local_bind).await?;
        let resolved = resolve_target(&target, allow_private_egress).await?;
        socket.connect(resolved).await?;
        Ok(Self {
            target,
            socket: Arc::new(socket),
            last_activity: Instant::now(),
            packets_up: AtomicU64::new(0),
            packets_down: AtomicU64::new(0),
            bytes_up: AtomicU64::new(0),
            bytes_down: AtomicU64::new(0),
        })
    }

    pub async fn send(&self, payload: &[u8]) -> Result<(), UdpError> {
        self.socket.send(payload).await?;
        self.packets_up.fetch_add(1, Ordering::Relaxed);
        self.bytes_up
            .fetch_add(payload.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    pub async fn recv(&self, buf: &mut [u8]) -> Result<usize, UdpError> {
        let n = self.socket.recv(buf).await?;
        self.packets_down.fetch_add(1, Ordering::Relaxed);
        self.bytes_down.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }

    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.socket.local_addr()
    }

    pub fn peer_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.socket.peer_addr()
    }
}

async fn resolve_target(
    target: &SocksAddr,
    allow_private_egress: bool,
) -> Result<SocketAddr, UdpError> {
    let addr_str = format!("{}:{}", target.host_str(), target.port());
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(&addr_str)
        .await
        .map_err(UdpError::Io)?
        .collect();
    select_resolved_addr(&addrs, allow_private_egress)
}

/// Apply the destination policy to the answers of one resolution.
///
/// With `allow_private_egress` the first answer is used unchanged (pproxy
/// compatibility). Otherwise every answer is filtered through the shared
/// special-purpose/private classifier and the first permitted address is
/// chosen, so a mixed public/private answer set cannot smuggle a private
/// destination through as the first answer. A fully private/reserved answer
/// set fails closed without creating a flow.
fn select_resolved_addr(
    addrs: &[SocketAddr],
    allow_private_egress: bool,
) -> Result<SocketAddr, UdpError> {
    let first = addrs.first().ok_or(UdpError::UnresolvedTarget)?;
    if allow_private_egress {
        return Ok(*first);
    }
    addrs
        .iter()
        .copied()
        .find(|addr| !is_reserved_or_private_ip(&addr.ip()))
        .ok_or(UdpError::PrivateEgressBlocked)
}

pub fn encode_response(target: &SocksAddr, payload: &[u8]) -> Result<Vec<u8>, UdpError> {
    let mut buf = Vec::new();
    eggress_protocol_socks::socks5::udp_codec::encode_socks5_udp_datagram(
        target, payload, &mut buf,
    )
    .map_err(UdpError::Codec)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn direct_flow_ipv4_echo() {
        let echo_addr = start_udp_echo_server().await;

        let flow = UdpTargetFlow::new(
            SocksAddr::IPv4([127, 0, 0, 1], echo_addr.port()),
            "127.0.0.1:0".parse().unwrap(),
            true,
        )
        .await
        .unwrap();

        flow.send(b"hello udp").await.unwrap();

        let mut buf = [0u8; 65535];
        let n = flow.recv(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello udp");
    }

    #[tokio::test]
    async fn direct_flow_tracks_metrics() {
        let echo_addr = start_udp_echo_server().await;

        let flow = UdpTargetFlow::new(
            SocksAddr::IPv4([127, 0, 0, 1], echo_addr.port()),
            "127.0.0.1:0".parse().unwrap(),
            true,
        )
        .await
        .unwrap();

        assert_eq!(flow.packets_up.load(Ordering::Relaxed), 0);
        assert_eq!(flow.bytes_up.load(Ordering::Relaxed), 0);

        flow.send(b"test").await.unwrap();
        assert_eq!(flow.packets_up.load(Ordering::Relaxed), 1);
        assert_eq!(flow.bytes_up.load(Ordering::Relaxed), 4);

        let mut buf = [0u8; 65535];
        let n = flow.recv(&mut buf).await.unwrap();
        assert_eq!(flow.packets_down.load(Ordering::Relaxed), 1);
        assert_eq!(flow.bytes_down.load(Ordering::Relaxed), n as u64);
    }

    #[tokio::test]
    async fn direct_flow_multiple_packets() {
        let echo_addr = start_udp_echo_server().await;

        let flow = UdpTargetFlow::new(
            SocksAddr::IPv4([127, 0, 0, 1], echo_addr.port()),
            "127.0.0.1:0".parse().unwrap(),
            true,
        )
        .await
        .unwrap();

        for i in 0..5 {
            let msg = format!("packet {i}");
            flow.send(msg.as_bytes()).await.unwrap();
            let mut buf = [0u8; 65535];
            let n = flow.recv(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], msg.as_bytes());
        }

        assert_eq!(flow.packets_up.load(Ordering::Relaxed), 5);
        assert_eq!(flow.packets_down.load(Ordering::Relaxed), 5);
    }

    #[tokio::test]
    async fn encode_response_format() {
        let target = SocksAddr::IPv4([10, 0, 0, 1], 80);
        let payload = b"response data";
        let encoded = encode_response(&target, payload).unwrap();
        assert_eq!(encoded[0], 0x00);
        assert_eq!(encoded[1], 0x00);
        assert_eq!(encoded[2], 0x00);
        assert_eq!(encoded[3], 0x01); // ATYP_IPV4
        assert_eq!(&encoded[4..8], &[10, 0, 0, 1]);
        assert_eq!(&encoded[8..10], &80u16.to_be_bytes());
        assert_eq!(&encoded[10..], b"response data");
    }

    #[tokio::test]
    async fn resolve_target_ipv4() {
        use std::net::IpAddr;
        let target = SocksAddr::IPv4([127, 0, 0, 1], 8080);
        let resolved = resolve_target(&target, true).await.unwrap();
        assert_eq!(
            resolved,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
        );
    }

    /// `allow_private_egress = false` must filter every answer of the one
    /// lookup, not truncate to the first answer. A private-only answer set
    /// fails closed instead of connecting.
    #[test]
    fn restrictive_policy_selects_first_permitted_answer() {
        let private = SocketAddr::from(([127, 0, 0, 1], 9999));
        let public = SocketAddr::from(([8, 8, 8, 8], 9999));
        let cgnat = SocketAddr::from(([100, 64, 0, 1], 9999));

        assert_eq!(
            select_resolved_addr(&[public, private], false).unwrap(),
            public,
            "public answer must be chosen over a preceding private answer"
        );
        assert_eq!(
            select_resolved_addr(&[cgnat, public], false).unwrap(),
            public,
            "CGNAT answers must be filtered before connecting"
        );
        assert_eq!(
            select_resolved_addr(&[private], false)
                .unwrap_err()
                .to_string(),
            UdpError::PrivateEgressBlocked.to_string()
        );
        assert!(matches!(
            select_resolved_addr(&[], false),
            Err(UdpError::UnresolvedTarget)
        ));
    }

    #[test]
    fn permissive_policy_keeps_first_answer() {
        let private = SocketAddr::from(([127, 0, 0, 1], 9999));
        let public = SocketAddr::from(([8, 8, 8, 8], 9999));
        assert_eq!(
            select_resolved_addr(&[private, public], true).unwrap(),
            private
        );
        assert_eq!(select_resolved_addr(&[public], true).unwrap(), public);
    }

    /// A domain whose answers are all private is dropped before any socket is
    /// connected, so no datagram reaches a local echo server.
    #[tokio::test]
    async fn private_domain_is_dropped_when_policy_disallows() {
        let (echo_addr, received) = start_counting_echo_server().await;
        let result = UdpTargetFlow::new(
            SocksAddr::Domain("localhost".to_string(), echo_addr.port()),
            "127.0.0.1:0".parse().unwrap(),
            false,
        )
        .await;

        assert!(
            matches!(result, Err(UdpError::PrivateEgressBlocked)),
            "private-resolving domain must be rejected by the post-DNS policy"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            received.load(Ordering::Relaxed),
            0,
            "no datagram may reach the private target"
        );
    }

    /// The same target is accepted when the operator permits private egress.
    #[tokio::test]
    async fn private_domain_is_allowed_when_policy_permits() {
        let (echo_addr, received) = start_counting_echo_server().await;
        // The numeric domain form keeps a real `lookup_host` call (domain
        // branch) while staying independent of the host resolver's family
        // preference.
        let flow = UdpTargetFlow::new(
            SocksAddr::Domain("127.0.0.1".to_string(), echo_addr.port()),
            "127.0.0.1:0".parse().unwrap(),
            true,
        )
        .await
        .expect("permissive policy must keep private UDP egress working");

        flow.send(b"permitted").await.unwrap();
        let mut buf = [0u8; 64];
        let n = flow.recv(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"permitted");
        assert_eq!(received.load(Ordering::Relaxed), 1);
    }

    /// A public destination still resolves and is selected under the
    /// restrictive policy without any external DNS dependency.
    #[tokio::test]
    async fn public_domain_still_resolves_when_policy_disallows_private() {
        use std::net::IpAddr;
        let target = SocksAddr::Domain("8.8.8.8".to_string(), 53);
        let resolved = resolve_target(&target, false).await.unwrap();
        assert_eq!(
            resolved,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 53)
        );
    }

    #[tokio::test]
    async fn flow_touch_updates_time() {
        let echo_addr = start_udp_echo_server().await;
        let mut flow = UdpTargetFlow::new(
            SocksAddr::IPv4([127, 0, 0, 1], echo_addr.port()),
            "127.0.0.1:0".parse().unwrap(),
            true,
        )
        .await
        .unwrap();

        let before = flow.last_activity;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        flow.touch();
        assert!(flow.last_activity > before);
    }

    async fn start_udp_echo_server() -> SocketAddr {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 65535];
            while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
                let _ = socket.send_to(&buf[..n], peer).await;
            }
        });
        addr
    }

    async fn start_counting_echo_server() -> (SocketAddr, Arc<AtomicU64>) {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        let received = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&received);
        tokio::spawn(async move {
            let mut buf = [0u8; 65535];
            while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
                counter.fetch_add(1, Ordering::Relaxed);
                let _ = socket.send_to(&buf[..n], peer).await;
            }
        });
        (addr, received)
    }
}
