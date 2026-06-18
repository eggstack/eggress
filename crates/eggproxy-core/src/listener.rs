use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener as TokioTcpListener;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::{BoxStream, ProtocolId};

/// Configuration for a TCP listener.
#[derive(Debug, Clone)]
pub struct TcpListenerConfig {
    pub bind_addr: SocketAddr,
    pub protocols: Vec<ProtocolId>,
    pub auth_required: bool,
    pub handshake_timeout: std::time::Duration,
    pub connection_limit: usize,
}

/// An accepted TCP connection.
pub struct AcceptedConnection {
    pub stream: BoxStream,
    pub peer_addr: SocketAddr,
    pub local_addr: SocketAddr,
}

/// TCP listener that accepts connections with concurrency limiting.
pub struct TcpListener {
    listener: TokioTcpListener,
    semaphore: Arc<Semaphore>,
    cancel_token: CancellationToken,
}

impl TcpListener {
    pub async fn new(
        config: &TcpListenerConfig,
        cancel_token: CancellationToken,
    ) -> std::io::Result<Self> {
        let listener = TokioTcpListener::bind(config.bind_addr).await?;
        Ok(Self {
            listener,
            semaphore: Arc::new(Semaphore::new(config.connection_limit)),
            cancel_token,
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub async fn accept(&self) -> std::io::Result<AcceptedConnection> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| std::io::Error::other("semaphore closed"))?;

        let (stream, peer_addr) = tokio::select! {
            result = self.listener.accept() => result?,
            _ = self.cancel_token.cancelled() => {
                return Err(std::io::Error::other(
                    "listener cancelled",
                ));
            }
        };

        let local_addr = stream.local_addr()?;
        let _permit = permit;

        Ok(AcceptedConnection {
            stream: Box::new(stream),
            peer_addr,
            local_addr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_listener_accept() {
        let config = TcpListenerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            protocols: vec![],
            auth_required: false,
            handshake_timeout: std::time::Duration::from_secs(10),
            connection_limit: 10,
        };

        let cancel_token = CancellationToken::new();
        let listener = TcpListener::new(&config, cancel_token.clone())
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        let jh = tokio::spawn(async move {
            let conn = listener.accept().await.unwrap();
            let mut stream = conn.stream;
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await.unwrap();
            stream.write_all(&buf[..n]).await.unwrap();
        });

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        client.write_all(b"hello").await.unwrap();

        let mut buf = [0u8; 5];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        jh.await.unwrap();
        cancel_token.cancel();
    }

    #[tokio::test]
    async fn test_listener_cancellation() {
        let config = TcpListenerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            protocols: vec![],
            auth_required: false,
            handshake_timeout: std::time::Duration::from_secs(10),
            connection_limit: 10,
        };

        let cancel_token = CancellationToken::new();
        let listener = TcpListener::new(&config, cancel_token.clone())
            .await
            .unwrap();

        cancel_token.cancel();

        let result = listener.accept().await;
        assert!(result.is_err());
    }
}
