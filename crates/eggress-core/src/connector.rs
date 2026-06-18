use std::net::SocketAddr;

use tokio::net::TcpStream;

use crate::{BoxStream, ConnectError, TargetAddr, TargetHost};

/// Trait for connecting to target servers.
#[trait_variant::make(Connector: Send)]
pub trait LocalConnector {
    async fn connect(&self, target: &TargetAddr) -> Result<BoxStream, ConnectError>;
}

/// Connector that makes direct TCP connections.
pub struct DirectConnector;

impl Connector for DirectConnector {
    async fn connect(&self, target: &TargetAddr) -> Result<BoxStream, ConnectError> {
        let addr: SocketAddr = match &target.host {
            TargetHost::Ip(ip) => SocketAddr::new(*ip, target.port),
            TargetHost::Domain(domain) => {
                use std::net::ToSocketAddrs;
                let lookup = format!("{}:{}", domain, target.port);
                lookup
                    .to_socket_addrs()
                    .map_err(|e| ConnectError::DnsResolution(e.to_string()))?
                    .next()
                    .ok_or_else(|| ConnectError::DnsResolution("no addresses found".to_string()))?
            }
        };

        let stream = TcpStream::connect(addr).await?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_direct_connect_echo() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let jh = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await.unwrap();
            stream.write_all(&buf[..n]).await.unwrap();
        });

        let target = TargetAddr {
            host: TargetHost::Ip(addr.ip()),
            port: addr.port(),
        };

        let connector = DirectConnector;
        let mut stream = Connector::connect(&connector, &target).await.unwrap();

        stream.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");

        jh.await.unwrap();
    }
}
