pub mod error;

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Buf, BytesMut};
use eggress_core::BoxStream;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{Sink, Stream, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::error::WebSocketError;

const DEFAULT_MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

pub struct WebSocketStreamAdapter<S> {
    read_half: SplitStream<WebSocketStream<S>>,
    write_half: SplitSink<WebSocketStream<S>, Message>,
    read_buf: BytesMut,
    max_message_size: usize,
}

impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static>
    WebSocketStreamAdapter<S>
{
    pub fn new(ws: WebSocketStream<S>, max_message_size: usize) -> Self {
        let (write_half, read_half) = ws.split();
        Self {
            read_half,
            write_half,
            read_buf: BytesMut::new(),
            max_message_size,
        }
    }

    pub fn into_boxed(self) -> BoxStream {
        Box::new(self)
    }

    fn poll_next_message(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Message, WebSocketError>>> {
        match Pin::new(&mut self.read_half).poll_next(cx) {
            Poll::Ready(Some(Ok(msg))) => Poll::Ready(Some(Ok(msg))),
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Some(Err(WebSocketError::Protocol(e.to_string()))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static> AsyncRead
    for WebSocketStreamAdapter<S>
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if !self.read_buf.is_empty() {
            let to_copy = std::cmp::min(self.read_buf.len(), buf.remaining());
            buf.put_slice(&self.read_buf[..to_copy]);
            self.read_buf.advance(to_copy);
            return Poll::Ready(Ok(()));
        }

        loop {
            match self.as_mut().poll_next_message(cx) {
                Poll::Ready(Some(Ok(Message::Binary(data)))) => {
                    if data.len() > self.max_message_size {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            WebSocketError::MessageTooLarge {
                                size: data.len(),
                                max: self.max_message_size,
                            },
                        )));
                    }
                    if data.len() <= buf.remaining() {
                        buf.put_slice(&data);
                    } else {
                        let to_copy = buf.remaining();
                        buf.put_slice(&data[..to_copy]);
                        self.read_buf.extend_from_slice(&data[to_copy..]);
                    }
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(Some(Ok(Message::Close(_)))) => {
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(Some(Ok(Message::Ping(_)))) => {
                    continue;
                }
                Poll::Ready(Some(Ok(Message::Pong(_)))) => {
                    continue;
                }
                Poll::Ready(Some(Ok(Message::Text(_)))) => {
                    tracing::warn!("received text frame on WebSocket tunnel, skipping");
                    continue;
                }
                Poll::Ready(Some(Ok(Message::Frame(_)))) => {
                    continue;
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(std::io::Error::other(e)));
                }
                Poll::Ready(None) => {
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static> AsyncWrite
    for WebSocketStreamAdapter<S>
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match Pin::new(&mut self.write_half).poll_flush(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => {
                return Poll::Ready(Err(std::io::Error::other(WebSocketError::Protocol(
                    e.to_string(),
                ))));
            }
            Poll::Pending => return Poll::Pending,
        }

        match Pin::new(&mut self.write_half).start_send(Message::Binary(buf.to_vec().into())) {
            Ok(()) => {}
            Err(e) => {
                return Poll::Ready(Err(std::io::Error::other(WebSocketError::Protocol(
                    e.to_string(),
                ))));
            }
        }

        match Pin::new(&mut self.write_half).poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(buf.len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(
                WebSocketError::Protocol(e.to_string()),
            ))),
            Poll::Pending => Poll::Ready(Ok(buf.len())),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match Pin::new(&mut self.write_half).poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(
                WebSocketError::Protocol(e.to_string()),
            ))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match Pin::new(&mut self.write_half).poll_close(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(
                WebSocketError::Protocol(e.to_string()),
            ))),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct WebSocketTunnelServer {
    max_message_size: usize,
}

impl WebSocketTunnelServer {
    pub fn new(max_message_size: usize) -> Self {
        Self { max_message_size }
    }

    pub fn with_default_config() -> Self {
        Self {
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }

    pub async fn accept_upgrade(
        &self,
        stream: tokio::net::TcpStream,
    ) -> Result<BoxStream, WebSocketError> {
        let ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .map_err(|e| WebSocketError::Handshake(e.to_string()))?;

        Ok(WebSocketStreamAdapter::new(ws_stream, self.max_message_size).into_boxed())
    }

    pub async fn accept_upgrade_with_config(
        &self,
        stream: tokio::net::TcpStream,
        config: tokio_tungstenite::tungstenite::protocol::WebSocketConfig,
    ) -> Result<BoxStream, WebSocketError> {
        let ws_stream = tokio_tungstenite::accept_async_with_config(stream, Some(config))
            .await
            .map_err(|e| WebSocketError::Handshake(e.to_string()))?;

        Ok(WebSocketStreamAdapter::new(ws_stream, self.max_message_size).into_boxed())
    }
}

pub struct WebSocketTunnelClient {
    max_message_size: usize,
}

impl WebSocketTunnelClient {
    pub fn new(max_message_size: usize) -> Self {
        Self { max_message_size }
    }

    pub fn with_default_config() -> Self {
        Self {
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }

    pub async fn connect(&self, url: &str) -> Result<BoxStream, WebSocketError> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| WebSocketError::Connect(e.to_string()))?;

        Ok(WebSocketStreamAdapter::new(ws_stream, self.max_message_size).into_boxed())
    }

    pub async fn connect_with_config(
        &self,
        url: &str,
        config: tokio_tungstenite::tungstenite::protocol::WebSocketConfig,
    ) -> Result<BoxStream, WebSocketError> {
        let (ws_stream, _) = tokio_tungstenite::connect_async_with_config(url, Some(config), false)
            .await
            .map_err(|e| WebSocketError::Connect(e.to_string()))?;

        Ok(WebSocketStreamAdapter::new(ws_stream, self.max_message_size).into_boxed())
    }

    pub async fn connect_over_stream<S>(
        &self,
        url: &str,
        stream: S,
    ) -> Result<BoxStream, WebSocketError>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (ws_stream, _) = tokio_tungstenite::client_async(url, stream)
            .await
            .map_err(|e| WebSocketError::Connect(e.to_string()))?;

        Ok(WebSocketStreamAdapter::new(ws_stream, self.max_message_size).into_boxed())
    }

    pub async fn connect_over_stream_with_config<S>(
        &self,
        url: &str,
        stream: S,
        config: tokio_tungstenite::tungstenite::protocol::WebSocketConfig,
    ) -> Result<BoxStream, WebSocketError>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (ws_stream, _) = tokio_tungstenite::client_async_with_config(url, stream, Some(config))
            .await
            .map_err(|e| WebSocketError::Connect(e.to_string()))?;

        Ok(WebSocketStreamAdapter::new(ws_stream, self.max_message_size).into_boxed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::SinkExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_websocket_echo() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let server = WebSocketTunnelServer::with_default_config();
            let mut bs = server.accept_upgrade(stream).await.unwrap();
            let mut buf = [0u8; 15];
            bs.read_exact(&mut buf).await.unwrap();
            bs.write_all(&buf).await.unwrap();
            bs.shutdown().await.unwrap();
        });

        let (ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{}", server_addr))
            .await
            .unwrap();
        let (mut sink, mut stream) = ws_stream.split();

        sink.send(Message::Binary(b"hello websocket".to_vec().into()))
            .await
            .unwrap();

        let msg = stream.next().await.unwrap().unwrap();
        match msg {
            Message::Binary(data) => assert_eq!(&*data, b"hello websocket"),
            _ => panic!("expected binary frame"),
        }

        sink.send(Message::Close(None)).await.unwrap();
        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_max_message_size_enforced() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let server = WebSocketTunnelServer::new(1024);
            let mut bs = server.accept_upgrade(stream).await.unwrap();
            let mut buf = [0u8; 2048];
            let result = bs.read_exact(&mut buf).await;
            assert!(result.is_err());
        });

        let (ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{}", server_addr))
            .await
            .unwrap();
        let (mut sink, _stream) = ws_stream.split();

        let large_msg = vec![0u8; 2048];
        sink.send(Message::Binary(large_msg.into())).await.unwrap();

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_close_frame_yields_eof() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let server = WebSocketTunnelServer::with_default_config();
            let mut bs = server.accept_upgrade(stream).await.unwrap();
            let mut buf = [0u8; 1];
            let result = bs.read_exact(&mut buf).await;
            assert!(result.is_err());
        });

        let (ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{}", server_addr))
            .await
            .unwrap();
        let (mut sink, _stream) = ws_stream.split();

        sink.send(Message::Close(None)).await.unwrap();

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_ping_pong_skipped() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let server = WebSocketTunnelServer::with_default_config();
            let mut bs = server.accept_upgrade(stream).await.unwrap();
            let mut buf = [0u8; 10];
            bs.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"after-ping");
        });

        let (ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{}", server_addr))
            .await
            .unwrap();
        let (mut sink, _stream) = ws_stream.split();

        sink.send(Message::Ping(b"ping-data".to_vec().into()))
            .await
            .unwrap();
        sink.send(Message::Pong(b"pong-data".to_vec().into()))
            .await
            .unwrap();
        sink.send(Message::Binary(b"after-ping".to_vec().into()))
            .await
            .unwrap();
        sink.send(Message::Close(None)).await.unwrap();

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_text_frame_skipped() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let server = WebSocketTunnelServer::with_default_config();
            let mut bs = server.accept_upgrade(stream).await.unwrap();
            let mut buf = [0u8; 4];
            bs.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"data");
        });

        let (ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{}", server_addr))
            .await
            .unwrap();
        let (mut sink, _stream) = ws_stream.split();

        sink.send(Message::Text("skipped-text".into()))
            .await
            .unwrap();
        sink.send(Message::Binary(b"data".to_vec().into()))
            .await
            .unwrap();
        sink.send(Message::Close(None)).await.unwrap();

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_partial_read_buffering() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let server = WebSocketTunnelServer::with_default_config();
            let mut bs = server.accept_upgrade(stream).await.unwrap();

            let mut buf1 = [0u8; 5];
            bs.read_exact(&mut buf1).await.unwrap();
            assert_eq!(&buf1, b"hello");

            let mut buf2 = [0u8; 5];
            bs.read_exact(&mut buf2).await.unwrap();
            assert_eq!(&buf2, b"world");

            let mut buf3 = [0u8; 4];
            bs.read_exact(&mut buf3).await.unwrap();
            assert_eq!(&buf3, b"done");
        });

        let (ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{}", server_addr))
            .await
            .unwrap();
        let (mut sink, _stream) = ws_stream.split();

        sink.send(Message::Binary(b"helloworld".to_vec().into()))
            .await
            .unwrap();
        sink.send(Message::Binary(b"done".to_vec().into()))
            .await
            .unwrap();
        sink.send(Message::Close(None)).await.unwrap();

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_upgrade_with_config() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let server = WebSocketTunnelServer::with_default_config();
            let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
                .max_message_size(Some(8192));
            let mut bs = server
                .accept_upgrade_with_config(stream, config)
                .await
                .unwrap();
            let mut buf = [0u8; 6];
            bs.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"config");
        });

        let (ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{}", server_addr))
            .await
            .unwrap();
        let (mut sink, _stream) = ws_stream.split();

        sink.send(Message::Binary(b"config".to_vec().into()))
            .await
            .unwrap();
        sink.send(Message::Close(None)).await.unwrap();

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_bidirectional_large_payload() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let server = WebSocketTunnelServer::with_default_config();
            let mut bs = server.accept_upgrade(stream).await.unwrap();

            let mut buf = [0u8; 65536];
            bs.read_exact(&mut buf).await.unwrap();

            bs.write_all(&buf).await.unwrap();
            bs.shutdown().await.unwrap();
        });

        let (ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{}", server_addr))
            .await
            .unwrap();
        let (mut sink, mut stream) = ws_stream.split();

        let payload: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
        sink.send(Message::Binary(payload.clone().into()))
            .await
            .unwrap();

        let mut received = Vec::new();
        loop {
            match stream.next().await {
                Some(Ok(Message::Binary(data))) => {
                    received.extend_from_slice(&data);
                    if received.len() >= 65536 {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) => break,
                _ => break,
            }
        }
        assert_eq!(&received, &payload);

        sink.send(Message::Close(None)).await.unwrap();
        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_websocket_error_display() {
        let err = WebSocketError::Handshake("test handshake".into());
        assert!(err.to_string().contains("test handshake"));

        let err = WebSocketError::Connect("test connect".into());
        assert!(err.to_string().contains("test connect"));

        let err = WebSocketError::Protocol("test protocol".into());
        assert!(err.to_string().contains("test protocol"));

        let err = WebSocketError::MessageTooLarge {
            size: 2048,
            max: 1024,
        };
        assert!(err.to_string().contains("2048"));
        assert!(err.to_string().contains("1024"));
    }

    #[tokio::test]
    async fn test_websocket_client_new() {
        let client = WebSocketTunnelClient::new(4096);
        assert_eq!(client.max_message_size, 4096);

        let client = WebSocketTunnelClient::with_default_config();
        assert_eq!(client.max_message_size, DEFAULT_MAX_MESSAGE_SIZE);
    }

    #[tokio::test]
    async fn test_websocket_client_connect() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let server = WebSocketTunnelServer::with_default_config();
            let mut bs = server.accept_upgrade(stream).await.unwrap();
            let mut buf = [0u8; 5];
            bs.read_exact(&mut buf).await.unwrap();
            bs.write_all(b"reply").await.unwrap();
            bs.shutdown().await.unwrap();
        });

        let client = WebSocketTunnelClient::with_default_config();
        let mut bs = client
            .connect(&format!("ws://{}", server_addr))
            .await
            .unwrap();
        bs.write_all(b"hello").await.unwrap();
        let mut reply = [0u8; 5];
        bs.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"reply");

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_connect_over_stream() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let server = WebSocketTunnelServer::with_default_config();
            let mut bs = server.accept_upgrade(stream).await.unwrap();
            let mut buf = [0u8; 12];
            bs.read_exact(&mut buf).await.unwrap();
            bs.write_all(b"over-stream!").await.unwrap();
            bs.shutdown().await.unwrap();
        });

        let tcp_stream = tokio::net::TcpStream::connect(server_addr).await.unwrap();

        let client = WebSocketTunnelClient::with_default_config();
        let mut bs = client
            .connect_over_stream(&format!("ws://{}", server_addr), tcp_stream)
            .await
            .unwrap();
        bs.write_all(b"hello stream").await.unwrap();
        let mut reply = [0u8; 12];
        bs.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"over-stream!");

        server_handle.await.unwrap();
    }
}
