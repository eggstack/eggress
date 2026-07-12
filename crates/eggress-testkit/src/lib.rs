//! Test utilities for eggress.
//!
//! Provides async test servers, port allocation helpers, and
//! protocol test harnesses for fragmented and slow I/O scenarios.

pub mod canonical_manifest;
pub mod case_model;
pub mod composition;
pub mod corpus;
pub mod differential;
pub mod eggress_runner;
pub mod fixtures;
pub mod manifest;
pub mod oracle;
pub mod pproxy_oracle;
pub mod report;

use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
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

/// Start a minimal HTTP origin server that responds to any request.
///
/// Returns a 200 OK with a fixed body. Useful for testing HTTP forward proxying.
pub async fn start_http_origin_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let jh = tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            tokio::spawn(async move {
                // Read the full request (headers + any body)
                let mut buf = [0u8; 4096];
                let mut request_data = Vec::new();
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => return,
                        Ok(n) => {
                            request_data.extend_from_slice(&buf[..n]);
                            if request_data.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => return,
                    }
                }

                // Check if there's a Content-Length to read body
                let _response_str = String::from_utf8_lossy(&request_data);
                let body = b"hello from origin";
                let response = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\
                     \r\n",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.write_all(body).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    (addr, jh)
}

/// A wrapper that reads from an inner stream with an artificial delay.
///
/// Useful for testing timeout behavior and slow-reader scenarios.
pub struct SlowReader {
    inner: tokio::net::tcp::OwnedReadHalf,
    delay: Duration,
}

impl SlowReader {
    pub fn new(inner: tokio::net::tcp::OwnedReadHalf, delay: Duration) -> Self {
        Self { inner, delay }
    }
}

impl AsyncRead for SlowReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // First poll the inner read
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if result.is_ready() {
            // After a successful read, insert a delay by re-registering waker
            let waker = cx.waker().clone();
            let delay = self.delay;
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                waker.wake();
            });
            // Return Pending to simulate slowness
            Poll::Pending
        } else {
            result
        }
    }
}

/// A wrapper that writes to an inner stream with an artificial delay.
///
/// Useful for testing timeout behavior and slow-writer scenarios.
pub struct SlowWriter {
    inner: tokio::net::tcp::OwnedWriteHalf,
    delay: Duration,
}

impl SlowWriter {
    pub fn new(inner: tokio::net::tcp::OwnedWriteHalf, delay: Duration) -> Self {
        Self { inner, delay }
    }
}

impl AsyncWrite for SlowWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, buf);
        if result.is_ready() {
            let waker = cx.waker().clone();
            let delay = self.delay;
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                waker.wake();
            });
            Poll::Pending
        } else {
            result
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// A stream wrapper that fragments all writes into small chunks.
///
/// Useful for testing protocol parsers with fragmented data.
pub struct FragmentedStream {
    inner: Box<dyn AsyncStream>,
    fragment_size: usize,
    write_buf: Vec<u8>,
}

/// A trait combining AsyncRead + AsyncWrite + Send + Unpin for test streams.
pub trait AsyncStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncStream for T {}

impl FragmentedStream {
    pub fn new(inner: Box<dyn AsyncStream>, fragment_size: usize) -> Self {
        Self {
            inner,
            fragment_size,
            write_buf: Vec::new(),
        }
    }
}

impl AsyncRead for FragmentedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for FragmentedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        // Buffer the data and return that we accepted it all
        // The actual flushing happens in poll_flush
        self.write_buf.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // Write fragments from the buffer
        while !self.write_buf.is_empty() {
            let chunk_len = self.write_buf.len().min(self.fragment_size);
            let chunk = self.write_buf[..chunk_len].to_vec();

            match Pin::new(&mut self.inner).poll_write(cx, &chunk) {
                Poll::Ready(Ok(n)) => {
                    self.write_buf.drain(..n);
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // Flush remaining buffered data first
        if !self.write_buf.is_empty() {
            let _ = self.as_mut().poll_flush(cx);
        }
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
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
    async fn test_http_origin_server() {
        let (addr, jh) = start_http_origin_server().await;

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf);
        assert!(response.contains("200 OK"));
        assert!(response.contains("hello from origin"));

        jh.abort();
    }

    #[tokio::test]
    async fn test_get_free_port() {
        let port = get_free_port().await;
        assert!(port > 0);
    }

    #[tokio::test]
    async fn test_fragmented_stream() {
        let (addr, jh) = start_echo_server().await;

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (read_half, write_half) = stream.into_split();
        let fragmented = FragmentedStream::new(
            Box::new(tokio::io::join(read_half, write_half)),
            3, // 3-byte fragments
        );

        let mut stream = fragmented;
        stream.write_all(b"hello world").await.unwrap();
        stream.shutdown().await.unwrap();

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello world");

        jh.abort();
    }
}
