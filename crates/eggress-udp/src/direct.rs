use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::net::UdpSocket;

use crate::error::UdpError;
use eggress_protocol_socks::socks5::server::SocksAddr;

pub struct UdpTargetFlow {
    pub target: SocksAddr,
    pub socket: UdpSocket,
    pub last_activity: Instant,
    pub packets_up: AtomicU64,
    pub packets_down: AtomicU64,
    pub bytes_up: AtomicU64,
    pub bytes_down: AtomicU64,
}

impl UdpTargetFlow {
    pub async fn new(target: SocksAddr, local_bind: SocketAddr) -> Result<Self, UdpError> {
        let socket = UdpSocket::bind(local_bind).await?;
        let resolved = resolve_target(&target).await?;
        socket.connect(resolved).await?;
        Ok(Self {
            target,
            socket,
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

async fn resolve_target(target: &SocksAddr) -> Result<SocketAddr, UdpError> {
    use std::net::ToSocketAddrs;
    let addr_str = format!("{}:{}", target.host_str(), target.port());
    addr_str
        .to_socket_addrs()
        .map_err(|e| UdpError::Io(std::io::Error::other(e)))?
        .next()
        .ok_or(UdpError::UnresolvedTarget)
}

pub fn encode_response(target: &SocksAddr, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    eggress_protocol_socks::socks5::udp_codec::encode_socks5_udp_response(
        target, payload, &mut buf,
    );
    buf
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
        let encoded = encode_response(&target, payload);
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
        let resolved = resolve_target(&target).await.unwrap();
        assert_eq!(
            resolved,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
        );
    }

    #[tokio::test]
    async fn flow_touch_updates_time() {
        let echo_addr = start_udp_echo_server().await;
        let mut flow = UdpTargetFlow::new(
            SocksAddr::IPv4([127, 0, 0, 1], echo_addr.port()),
            "127.0.0.1:0".parse().unwrap(),
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
}
