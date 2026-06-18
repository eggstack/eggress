//! Test utilities for eggproxy.
//!
//! Provides async test servers and port allocation helpers.

use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Get a free port by binding to port 0.
pub async fn get_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Start an async echo server that echoes back received bytes.
///
/// Returns the address the server is listening on and a join handle.
pub async fn start_echo_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if stream.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });

    (addr, jh)
}

/// Start a half-close server: reads until EOF, then sends a response and closes.
///
/// Returns the address the server is listening on and a join handle.
pub async fn start_half_close_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => data.extend_from_slice(&buf[..n]),
                        Err(_) => return,
                    }
                }
                let _ = stream.write_all(&data).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    (addr, jh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_echo_server() {
        let (addr, jh) = start_echo_server().await;

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"hello").await.unwrap();
        stream.shutdown().await.unwrap();

        let mut buf = [0u8; 5];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        jh.abort();
    }

    #[tokio::test]
    async fn test_half_close_server() {
        let (addr, jh) = start_half_close_server().await;

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"request").await.unwrap();
        stream.shutdown().await.unwrap();

        let mut buf = [0u8; 7];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"request");

        jh.abort();
    }

    #[tokio::test]
    async fn test_get_free_port() {
        let port = get_free_port().await;
        assert!(port > 0);
    }
}
