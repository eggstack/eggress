use tokio::io::{self, AsyncWriteExt};

use crate::BoxStream;

/// Reason the relay terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    ClientClosed,
    ServerClosed,
    BothClosed,
    Error,
    Cancelled,
}

/// Result of a relay operation.
#[derive(Debug)]
pub struct RelayResult {
    pub bytes_upstream: u64,
    pub bytes_downstream: u64,
    pub termination_reason: TerminationReason,
}

/// Relay data bidirectionally between two streams.
///
/// When one side closes its write half, the other side's write half is shut down
/// (half-close semantics). Both directions must complete before returning.
pub async fn relay(client: BoxStream, server: BoxStream) -> RelayResult {
    let (mut client_read, mut client_write) = io::split(client);
    let (mut server_read, mut server_write) = io::split(server);

    let client_to_server = tokio::spawn(async move {
        let n = io::copy(&mut client_read, &mut server_write).await?;
        server_write.shutdown().await?;
        Ok::<u64, std::io::Error>(n)
    });

    let server_to_client = tokio::spawn(async move {
        let n = io::copy(&mut server_read, &mut client_write).await?;
        client_write.shutdown().await?;
        Ok::<u64, std::io::Error>(n)
    });

    let a_result = client_to_server.await;
    let b_result = server_to_client.await;

    let (a_bytes, a_error) = match a_result {
        Ok(Ok(n)) => (n, false),
        Ok(Err(_)) => (0, true),
        Err(_) => (0, true),
    };

    let (b_bytes, b_error) = match b_result {
        Ok(Ok(n)) => (n, false),
        Ok(Err(_)) => (0, true),
        Err(_) => (0, true),
    };

    let termination_reason = match (a_error, b_error) {
        (true, true) => TerminationReason::BothClosed,
        (true, false) => TerminationReason::ServerClosed,
        (false, true) => TerminationReason::ClientClosed,
        (false, false) => TerminationReason::BothClosed,
    };

    RelayResult {
        bytes_upstream: a_bytes,
        bytes_downstream: b_bytes,
        termination_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_relay_echo() {
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo.local_addr().unwrap();

        let jh = tokio::spawn(async move {
            let (stream, _) = echo.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                loop {
                    let n = reader.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    writer.write_all(&buf[..n]).await.unwrap();
                }
            });
        });

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (client_stream, _) = proxy_listener.accept().await.unwrap();
            let server_stream = tokio::net::TcpStream::connect(echo_addr).await.unwrap();
            relay(Box::new(client_stream), Box::new(server_stream)).await
        });

        let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"hello relay").await.unwrap();
        client.shutdown().await.unwrap();

        let mut buf = String::new();
        client.read_to_string(&mut buf).await.unwrap();
        assert_eq!(buf, "hello relay");

        let result = proxy_jh.await.unwrap();
        assert_eq!(result.bytes_upstream, 11);
        assert_eq!(result.bytes_downstream, 11);

        jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_relay_half_close() {
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo.local_addr().unwrap();

        let jh = tokio::spawn(async move {
            let (mut stream, _) = echo.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await.unwrap();
            stream.write_all(&buf[..n]).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (client_stream, _) = proxy_listener.accept().await.unwrap();
            let server_stream = tokio::net::TcpStream::connect(echo_addr).await.unwrap();
            relay(Box::new(client_stream), Box::new(server_stream)).await
        });

        let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"data").await.unwrap();
        client.shutdown().await.unwrap();

        let mut buf = [0u8; 4];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"data");

        let result = proxy_jh.await.unwrap();
        assert_eq!(result.bytes_upstream, 4);
        assert_eq!(result.bytes_downstream, 4);

        jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_relay_cancellation() {
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo.local_addr().unwrap();

        let jh = tokio::spawn(async move {
            let (stream, _) = echo.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                loop {
                    let n = reader.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    writer.write_all(&buf[..n]).await.unwrap();
                }
            });
        });

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (client_stream, _) = proxy_listener.accept().await.unwrap();
            let server_stream = tokio::net::TcpStream::connect(echo_addr).await.unwrap();
            relay(Box::new(client_stream), Box::new(server_stream)).await
        });

        let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"data").await.unwrap();
        drop(client);

        let result = proxy_jh.await.unwrap();
        assert!(result.bytes_upstream > 0 || result.bytes_downstream > 0);

        jh.await.unwrap();
    }
}
