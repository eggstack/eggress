use std::net::SocketAddr;

use eggress_core::TargetAddr;
use tokio::net::{TcpListener, TcpStream};

use crate::error::RawTunnelError;

pub struct RawTunnelListener {
    listener: TcpListener,
    target: TargetAddr,
}

impl RawTunnelListener {
    pub async fn bind(bind_addr: &str, target: TargetAddr) -> Result<Self, RawTunnelError> {
        let listener = TcpListener::bind(bind_addr).await?;
        Ok(Self { listener, target })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }

    pub async fn run(&self) -> Result<(), RawTunnelError> {
        loop {
            let (stream, peer) = self.listener.accept().await?;
            let target = self.target.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_raw_connection(stream, target).await {
                    tracing::warn!("raw tunnel error from {}: {}", peer, e);
                }
            });
        }
    }
}

async fn handle_raw_connection(
    mut client: TcpStream,
    target: TargetAddr,
) -> Result<(), RawTunnelError> {
    let target_str = format!("{}:{}", target.host, target.port);
    let mut upstream = TcpStream::connect(&target_str)
        .await
        .map_err(|e| RawTunnelError::TargetConnect(e.to_string()))?;

    let (bytes_copied, _) = tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
    tracing::trace!("raw tunnel relayed {} bytes", bytes_copied);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_bind_success() {
        let target: TargetAddr = "127.0.0.1:9999".parse().unwrap();
        let listener = RawTunnelListener::bind("127.0.0.1:0", target)
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        assert_eq!(addr.ip(), "127.0.0.1".parse::<std::net::IpAddr>().unwrap());
        assert!(addr.port() > 0);
    }

    #[tokio::test]
    async fn test_local_addr_returns_listening_address() {
        let target: TargetAddr = "127.0.0.1:9999".parse().unwrap();
        let listener = RawTunnelListener::bind("127.0.0.1:0", target)
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = TcpStream::connect(addr).await.unwrap();
        assert_eq!(stream.peer_addr().unwrap().port(), addr.port());
    }

    #[tokio::test]
    async fn test_bind_failure_invalid_address() {
        let target: TargetAddr = "127.0.0.1:9999".parse().unwrap();
        let result = RawTunnelListener::bind("invalid-not-an-address", target).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_relay_bidirectional() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();

        let upstream_handle = tokio::spawn(async move {
            let (mut stream, _) = upstream_listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await.unwrap();
            stream.write_all(&buf[..n]).await.unwrap();
        });

        let target: TargetAddr = format!("{}:{}", upstream_addr.ip(), upstream_addr.port())
            .parse()
            .unwrap();
        let tunnel_listener = RawTunnelListener::bind("127.0.0.1:0", target)
            .await
            .unwrap();
        let tunnel_addr = tunnel_listener.local_addr().unwrap();

        let tunnel_handle = tokio::spawn(async move {
            tunnel_listener.run().await.unwrap();
        });

        let mut client = TcpStream::connect(tunnel_addr).await.unwrap();
        client.write_all(b"hello raw tunnel").await.unwrap();
        client.shutdown().await.unwrap();

        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        assert_eq!(response, b"hello raw tunnel");

        tunnel_handle.abort();
        upstream_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_upstream_connect_failure() {
        let target: TargetAddr = "127.0.0.1:1".parse().unwrap();
        let tunnel_listener = RawTunnelListener::bind("127.0.0.1:0", target)
            .await
            .unwrap();
        let tunnel_addr = tunnel_listener.local_addr().unwrap();

        let tunnel_handle = tokio::spawn(async move {
            tunnel_listener.run().await.unwrap();
        });

        let mut client = TcpStream::connect(tunnel_addr).await.unwrap();
        client.write_all(b"data").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let n = client.read(&mut [0u8; 1]).await.unwrap_or_default();
        assert_eq!(n, 0);

        tunnel_handle.abort();
    }

    #[tokio::test]
    async fn test_multiple_concurrent_connections() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();

        let upstream_handle = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut stream, _) = upstream_listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let n = stream.read(&mut buf).await.unwrap();
                    stream.write_all(&buf[..n]).await.unwrap();
                });
            }
        });

        let target: TargetAddr = format!("{}:{}", upstream_addr.ip(), upstream_addr.port())
            .parse()
            .unwrap();
        let tunnel_listener = RawTunnelListener::bind("127.0.0.1:0", target)
            .await
            .unwrap();
        let tunnel_addr = tunnel_listener.local_addr().unwrap();

        let tunnel_handle = tokio::spawn(async move {
            tunnel_listener.run().await.unwrap();
        });

        let mut handles = Vec::new();
        for i in 0..3 {
            let addr = tunnel_addr;
            handles.push(tokio::spawn(async move {
                let mut client = TcpStream::connect(addr).await.unwrap();
                let msg = format!("msg{}", i);
                client.write_all(msg.as_bytes()).await.unwrap();
                client.shutdown().await.unwrap();
                let mut response = Vec::new();
                client.read_to_end(&mut response).await.unwrap();
                assert_eq!(response, msg.as_bytes());
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        tunnel_handle.abort();
        upstream_handle.await.unwrap();
    }
}
