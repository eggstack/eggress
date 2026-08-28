use std::net::SocketAddr;
use std::sync::Arc;

use eggress_core::connector::{ConnectOptions, DirectConnector};
use eggress_core::{ConnectError, TargetAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

use crate::error::RawTunnelError;

/// Default maximum concurrent connections for the raw tunnel listener.
const DEFAULT_MAX_CONNECTIONS: usize = 1024;

pub struct RawTunnelListener {
    listener: TcpListener,
    target: TargetAddr,
    semaphore: Arc<Semaphore>,
    enforce_dns_rebinding_check: bool,
}

impl RawTunnelListener {
    pub async fn bind(bind_addr: &str, target: TargetAddr) -> Result<Self, RawTunnelError> {
        let listener = TcpListener::bind(bind_addr).await?;
        Ok(Self {
            listener,
            target,
            semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONNECTIONS)),
            enforce_dns_rebinding_check: true,
        })
    }

    #[cfg(test)]
    async fn bind_unchecked(bind_addr: &str, target: TargetAddr) -> Result<Self, RawTunnelError> {
        let mut listener = Self::bind(bind_addr, target).await?;
        listener.enforce_dns_rebinding_check = false;
        Ok(listener)
    }

    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }

    pub async fn run(&self) -> Result<(), RawTunnelError> {
        loop {
            let (stream, peer) = self.listener.accept().await?;
            let permit = match self.semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => {
                    tracing::warn!("raw tunnel connection limit reached, rejecting {}", peer);
                    drop(stream);
                    continue;
                }
            };
            let target = self.target.clone();
            let enforce_dns_rebinding_check = self.enforce_dns_rebinding_check;
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(e) =
                    handle_raw_connection(stream, target, enforce_dns_rebinding_check).await
                {
                    tracing::warn!("raw tunnel error from {}: {}", peer, e);
                }
            });
        }
    }
}

async fn handle_raw_connection(
    mut client: TcpStream,
    target: TargetAddr,
    enforce_dns_rebinding_check: bool,
) -> Result<(), RawTunnelError> {
    let mut upstream = DirectConnector
        .connect_with_options(
            &target,
            &ConnectOptions {
                enforce_dns_rebinding_check,
                enforce_literal_ip_check: enforce_dns_rebinding_check,
                ..ConnectOptions::default()
            },
        )
        .await
        .map_err(|error| match error {
            ConnectError::ReservedTarget(ip) => RawTunnelError::DnsRebinding(ip),
            error => RawTunnelError::TargetConnect(error.to_string()),
        })?;

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
        let listener = RawTunnelListener::bind_unchecked("127.0.0.1:0", target)
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        assert_eq!(addr.ip(), "127.0.0.1".parse::<std::net::IpAddr>().unwrap());
        assert!(addr.port() > 0);
    }

    #[tokio::test]
    async fn test_local_addr_returns_listening_address() {
        let target: TargetAddr = "127.0.0.1:9999".parse().unwrap();
        let listener = RawTunnelListener::bind_unchecked("127.0.0.1:0", target)
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
        let tunnel_listener = RawTunnelListener::bind_unchecked("127.0.0.1:0", target)
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
        let tunnel_listener = RawTunnelListener::bind_unchecked("127.0.0.1:0", target)
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
        let tunnel_listener = RawTunnelListener::bind_unchecked("127.0.0.1:0", target)
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

    #[tokio::test]
    async fn test_reserved_literal_target_is_rejected() {
        let tunnel_listener =
            RawTunnelListener::bind("127.0.0.1:0", "127.0.0.1:1".parse().unwrap())
                .await
                .unwrap();
        let tunnel_addr = tunnel_listener.local_addr().unwrap();
        let tunnel_handle = tokio::spawn(async move {
            tunnel_listener.run().await.unwrap();
        });

        let mut client = TcpStream::connect(tunnel_addr).await.unwrap();
        let mut byte = [0u8; 1];
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(1), client.read(&mut byte))
                .await
                .unwrap();
        assert_eq!(result.unwrap(), 0);

        tunnel_handle.abort();
    }
}
