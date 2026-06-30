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
