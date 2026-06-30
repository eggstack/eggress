use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use h2::server::Connection;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error::HttpError;
use eggress_core::TargetAddr;

#[derive(Debug, thiserror::Error)]
pub enum H2ConnectError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("H2 protocol error: {0}")]
    H2(String),
    #[error("HTTP error: {0}")]
    Http(#[from] HttpError),
}

impl From<h2::Error> for H2ConnectError {
    fn from(e: h2::Error) -> Self {
        H2ConnectError::H2(e.to_string())
    }
}

struct H2StreamWrite {
    send_stream: h2::SendStream<Bytes>,
    capacity: usize,
}

impl H2StreamWrite {
    fn new(send_stream: h2::SendStream<Bytes>) -> Self {
        Self {
            send_stream,
            capacity: 0,
        }
    }
}

impl tokio::io::AsyncWrite for H2StreamWrite {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        if self.capacity == 0 {
            self.send_stream.reserve_capacity(buf.len());
            match self.send_stream.poll_capacity(cx) {
                Poll::Ready(Some(Ok(capacity))) => {
                    self.capacity = capacity;
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(std::io::Error::other(e)));
                }
                Poll::Ready(None) => {
                    return Poll::Ready(Err(std::io::Error::other("h2 stream closed")));
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        let len = buf.len().min(self.capacity);
        self.send_stream
            .send_data(Bytes::copy_from_slice(&buf[..len]), false)
            .map_err(std::io::Error::other)?;
        self.capacity -= len;
        Poll::Ready(Ok(len))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.send_stream
            .send_data(Bytes::new(), true)
            .map_err(std::io::Error::other)?;
        Poll::Ready(Ok(()))
    }
}

pub async fn h2_connect_relay(
    mut recv_stream: h2::RecvStream,
    send_stream: h2::SendStream<Bytes>,
    target: TargetAddr,
) -> Result<(), H2ConnectError> {
    let mut tcp = TcpStream::connect(target.to_string()).await?;
    let (mut tcp_read, mut tcp_write) = tokio::io::split(&mut tcp);
    let mut h2_write = H2StreamWrite::new(send_stream);

    let h2_to_tcp = async {
        loop {
            match recv_stream.data().await {
                Some(Ok(data)) => {
                    tcp_write.write_all(&data).await?;
                }
                Some(Err(e)) => {
                    return Err(std::io::Error::other(e));
                }
                None => break,
            }
        }
        Ok::<(), std::io::Error>(())
    };

    let tcp_to_h2 = async {
        let mut buf = [0u8; 8192];
        loop {
            let n = tcp_read.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            h2_write.write_all(&buf[..n]).await?;
        }
        Ok::<(), std::io::Error>(())
    };

    tokio::select! {
        result = h2_to_tcp => { result?; }
        result = tcp_to_h2 => { result?; }
    }

    Ok(())
}

pub async fn handle_h2_connect(
    mut connection: Connection<TcpStream, Bytes>,
) -> Result<(), H2ConnectError> {
    loop {
        match connection.accept().await {
            Some(Ok((request, mut send_response))) => {
                if *request.method() == http::Method::CONNECT {
                    let authority = request
                        .uri()
                        .authority()
                        .ok_or_else(|| H2ConnectError::H2("missing authority".into()))?;

                    let target_str = match authority.port_u16() {
                        Some(port) => format!("{}:{}", authority.host(), port),
                        None => format!("{}:443", authority.host()),
                    };

                    let target: TargetAddr = target_str
                        .parse()
                        .map_err(|e: String| H2ConnectError::H2(e))?;

                    let response = http::Response::builder().status(200).body(()).unwrap();

                    let send_stream = send_response.send_response(response, false)?;
                    let recv_stream = request.into_body();

                    tokio::spawn(async move {
                        if let Err(e) = h2_connect_relay(recv_stream, send_stream, target).await {
                            tracing::warn!("h2 connect relay error: {}", e);
                        }
                    });
                } else {
                    send_response.send_reset(h2::Reason::PROTOCOL_ERROR);
                }
            }
            Some(Err(e)) => {
                return Err(H2ConnectError::H2(e.to_string()));
            }
            None => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_h2_connect_error_display() {
        let err = H2ConnectError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "test",
        ));
        assert!(err.to_string().contains("IO error"));
    }

    #[test]
    fn test_h2_connect_error_from_h2() {
        let err = H2ConnectError::H2("test error".into());
        assert_eq!(err.to_string(), "H2 protocol error: test error");
    }
}
