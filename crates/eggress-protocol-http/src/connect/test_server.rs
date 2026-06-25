use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Mode of operation for the synthetic HTTP CONNECT proxy test server.
pub enum ProxyMode {
    /// Returns 200 Connection Established.
    Success,
    /// Returns 407 Proxy Authentication Required.
    AuthRequired,
    /// Returns 403 Forbidden.
    Forbidden,
    /// Returns garbage bytes instead of an HTTP response.
    MalformedStatus,
    /// Delays before responding (configurable duration).
    SlowResponse(std::time::Duration),
    /// Sends headers exceeding the size limit.
    HeadersTooLarge,
}

/// A synthetic HTTP CONNECT proxy server for unit testing.
pub struct TestProxyServer {
    pub addr: SocketAddr,
    join: JoinHandle<()>,
}

impl TestProxyServer {
    /// Start a test proxy server in the given mode.
    pub async fn start(mode: ProxyMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let join = tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => break,
                };

                let mode = match &mode {
                    ProxyMode::Success => ProxyMode::Success,
                    ProxyMode::AuthRequired => ProxyMode::AuthRequired,
                    ProxyMode::Forbidden => ProxyMode::Forbidden,
                    ProxyMode::MalformedStatus => ProxyMode::MalformedStatus,
                    ProxyMode::SlowResponse(d) => ProxyMode::SlowResponse(*d),
                    ProxyMode::HeadersTooLarge => ProxyMode::HeadersTooLarge,
                };

                tokio::spawn(async move {
                    // Read the CONNECT request
                    let mut buf = Vec::with_capacity(1024);
                    let mut temp = [0u8; 1];
                    loop {
                        if stream.read(&mut temp).await.unwrap_or(0) == 0 {
                            return;
                        }
                        buf.push(temp[0]);
                        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
                            break;
                        }
                    }

                    match mode {
                        ProxyMode::Success => {
                            let resp = b"HTTP/1.1 200 Connection Established\r\n\r\n";
                            let _ = stream.write_all(resp).await;
                        }
                        ProxyMode::AuthRequired => {
                            let resp = b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n";
                            let _ = stream.write_all(resp).await;
                        }
                        ProxyMode::Forbidden => {
                            let resp = b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n";
                            let _ = stream.write_all(resp).await;
                        }
                        ProxyMode::MalformedStatus => {
                            let garbage = [0xFFu8; 64];
                            let _ = stream.write_all(&garbage).await;
                        }
                        ProxyMode::SlowResponse(delay) => {
                            tokio::time::sleep(delay).await;
                            let resp = b"HTTP/1.1 200 Connection Established\r\n\r\n";
                            let _ = stream.write_all(resp).await;
                        }
                        ProxyMode::HeadersTooLarge => {
                            let header_line = format!("X-Pad: {}\r\n", "A".repeat(1024));
                            let mut resp = b"HTTP/1.1 200 OK\r\n".to_vec();
                            // Send enough headers to exceed 32KB
                            for _ in 0..64 {
                                resp.extend_from_slice(header_line.as_bytes());
                            }
                            resp.extend_from_slice(b"\r\n");
                            let _ = stream.write_all(&resp).await;
                        }
                    }
                });
            }
        });

        Self { addr, join }
    }

    /// Stop the test proxy server.
    pub async fn stop(self) {
        self.join.abort();
    }
}
