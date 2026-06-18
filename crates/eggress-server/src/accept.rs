use std::pin::Pin;
use std::task::{Context, Poll};

use eggress_core::BoxStream;
use eggress_core::{TargetAddr, TargetHost};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

/// The result of accepting an inbound connection.
pub enum AcceptedSession {
    Tunnel(PendingTunnel),
    HttpForward(PendingHttpForward),
}

/// A pending tunnel connection (HTTP CONNECT, SOCKS4, SOCKS5).
/// Success reply has NOT been sent yet.
pub struct PendingTunnel {
    pub target: TargetAddr,
    pub client: BoxStream,
    pub protocol: TunnelProtocol,
    pub reply_context: ReplyContext,
}

/// Describes how the request body is framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestBodyKind {
    None,
    ContentLength(u64),
    Chunked,
}

/// A pending HTTP forward-proxy request.
pub struct PendingHttpForward {
    pub target: TargetAddr,
    pub client: BoxStream,
    pub request: eggress_protocol_http::forward::ForwardRequest,
    pub body_kind: RequestBodyKind,
}

/// Which tunnel protocol was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelProtocol {
    HttpConnect,
    Socks4,
    Socks5,
}

/// Information needed to send a protocol-specific reply later.
pub enum ReplyContext {
    Http,
    Socks4,
    Socks5,
}

/// A stream that returns `prefix` bytes first, then delegates to `inner`.
struct PrefixedStream {
    prefix: std::io::Cursor<Vec<u8>>,
    inner: BoxStream,
}

impl PrefixedStream {
    fn new(prefix: Vec<u8>, inner: BoxStream) -> Self {
        Self {
            prefix: std::io::Cursor::new(prefix),
            inner,
        }
    }
}

impl AsyncRead for PrefixedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let pos = self.prefix.position() as usize;
        let len = self.prefix.get_ref().len();
        if pos < len {
            let remaining = &self.prefix.get_ref()[pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.prefix.set_position((pos + to_copy) as u64);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Read the first byte from the stream, detect the protocol, perform the
/// handshake parsing, and return an `AcceptedSession` **without** opening
/// any outbound connection or sending any success/failure reply.
pub async fn accept(
    client: BoxStream,
) -> Result<AcceptedSession, Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = client;
    let mut first_byte = [0u8; 1];
    stream.read_exact(&mut first_byte).await?;

    let proto = match first_byte[0] {
        0x05 => "socks5",
        0x04 => "socks4",
        _ => "http",
    };
    tracing::trace!(
        "detected protocol: {proto} (first_byte={:#04x})",
        first_byte[0]
    );

    // Reconstruct stream with the first byte prepended so protocol
    // handlers see the full request starting from the version byte.
    let stream: BoxStream = Box::new(PrefixedStream::new(first_byte.to_vec(), stream));

    match proto {
        "socks5" => accept_socks5(stream).await,
        "socks4" => accept_socks4(stream).await,
        _ => accept_http(stream).await,
    }
}

async fn accept_socks5(
    stream: BoxStream,
) -> Result<AcceptedSession, Box<dyn std::error::Error + Send + Sync>> {
    use eggress_protocol_socks::socks5::server::{
        read_connect_request, read_method_negotiation, send_method_selection,
    };

    let (mut reader, mut writer) = tokio::io::split(stream);
    let methods = read_method_negotiation(&mut reader).await?;
    send_method_selection(&mut writer, &methods, None).await?;
    let socks_addr = read_connect_request(&mut reader).await?;

    let target = socks_addr_to_target(&socks_addr);
    let stream: BoxStream = Box::new(tokio::io::join(reader, writer));

    Ok(AcceptedSession::Tunnel(PendingTunnel {
        target,
        client: stream,
        protocol: TunnelProtocol::Socks5,
        reply_context: ReplyContext::Socks5,
    }))
}

async fn accept_socks4(
    stream: BoxStream,
) -> Result<AcceptedSession, Box<dyn std::error::Error + Send + Sync>> {
    use eggress_protocol_socks::socks4::server::read_socks4_request;

    let (mut reader, writer) = tokio::io::split(stream);
    let request = read_socks4_request(&mut reader).await?;
    let target = TargetAddr {
        host: TargetHost::Ip(request.addr.ip()),
        port: request.addr.port(),
    };
    let stream: BoxStream = Box::new(tokio::io::join(reader, writer));

    Ok(AcceptedSession::Tunnel(PendingTunnel {
        target,
        client: stream,
        protocol: TunnelProtocol::Socks4,
        reply_context: ReplyContext::Socks4,
    }))
}

async fn accept_http(
    mut stream: BoxStream,
) -> Result<AcceptedSession, Box<dyn std::error::Error + Send + Sync>> {
    // Read the request line to determine method
    let mut head_buf = Vec::with_capacity(256);
    let mut temp = [0u8; 1];

    loop {
        if head_buf.len() >= MAX_HEAD_SIZE {
            return Err(eggress_protocol_http::HttpError::HeaderTooLarge.into());
        }
        let n = stream.read(&mut temp).await?;
        if n == 0 {
            return Err(eggress_protocol_http::HttpError::MalformedRequest(
                "unexpected EOF".into(),
            )
            .into());
        }
        head_buf.push(temp[0]);
        if head_buf.len() >= 2 && &head_buf[head_buf.len() - 2..] == b"\r\n" {
            break;
        }
    }

    let method = {
        let request_line = String::from_utf8_lossy(&head_buf);
        request_line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    };

    // Reconstruct stream with the request line bytes prepended
    let mut stream: BoxStream = Box::new(PrefixedStream::new(head_buf, stream));

    if method == "connect" {
        let request = read_connect_request_from_stream(&mut stream).await?;
        Ok(AcceptedSession::Tunnel(PendingTunnel {
            target: request.target,
            client: stream,
            protocol: TunnelProtocol::HttpConnect,
            reply_context: ReplyContext::Http,
        }))
    } else {
        let (request, client_stream) = eggress_protocol_http::forward_request(stream).await?;
        let target = request.target.clone();
        let body_kind = if request.is_chunked {
            RequestBodyKind::Chunked
        } else if let Some(len) = request.content_length {
            RequestBodyKind::ContentLength(len)
        } else {
            RequestBodyKind::None
        };
        Ok(AcceptedSession::HttpForward(PendingHttpForward {
            target,
            client: client_stream,
            request,
            body_kind,
        }))
    }
}

struct ConnectRequest {
    target: TargetAddr,
}

async fn read_connect_request_from_stream(
    stream: &mut BoxStream,
) -> Result<ConnectRequest, Box<dyn std::error::Error + Send + Sync>> {
    let mut head_buf = Vec::with_capacity(1024);
    let mut temp = [0u8; 1];
    let mut header_count = 0;

    loop {
        if head_buf.len() >= MAX_HEAD_SIZE {
            return Err(eggress_protocol_http::HttpError::HeaderTooLarge.into());
        }

        let n = stream.read(&mut temp).await?;
        if n == 0 {
            return Err(eggress_protocol_http::HttpError::MalformedRequest(
                "unexpected EOF reading request".into(),
            )
            .into());
        }

        head_buf.push(temp[0]);

        if head_buf.len() >= 4 {
            let len = head_buf.len();
            if &head_buf[len - 4..] == b"\r\n\r\n" {
                break;
            }
            if head_buf.len() >= 2 && &head_buf[len - 2..] == b"\r\n" {
                header_count += 1;
                if header_count > MAX_HEADER_LINES {
                    return Err(eggress_protocol_http::HttpError::TooManyHeaders.into());
                }
            }
        }
    }

    let head_str = String::from_utf8_lossy(&head_buf);
    let mut lines = head_str.split("\r\n");

    let request_line = lines.next().ok_or_else(|| {
        eggress_protocol_http::HttpError::MalformedRequest("empty request".into())
    })?;

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(eggress_protocol_http::HttpError::MalformedRequest(format!(
            "expected 3 parts in request line, got {}",
            parts.len()
        ))
        .into());
    }

    let authority = parts[1];
    let target = parse_authority(authority)?;

    Ok(ConnectRequest { target })
}

fn parse_authority(
    authority: &str,
) -> Result<TargetAddr, Box<dyn std::error::Error + Send + Sync>> {
    if authority.starts_with('[') {
        let bracket_end = authority.find(']').ok_or_else(|| {
            eggress_protocol_http::HttpError::TargetParseError(
                "unclosed bracket in IPv6 address".into(),
            )
        })?;

        let ip_str = &authority[1..bracket_end];
        let ip: std::net::IpAddr = ip_str.parse().map_err(|e| {
            eggress_protocol_http::HttpError::TargetParseError(format!("invalid IPv6 address: {e}"))
        })?;

        let port_str = authority.get(bracket_end + 2..).ok_or_else(|| {
            eggress_protocol_http::HttpError::TargetParseError("missing port".into())
        })?;

        if !authority
            .as_bytes()
            .get(bracket_end + 1)
            .is_some_and(|&b| b == b':')
        {
            return Err(eggress_protocol_http::HttpError::TargetParseError(
                "expected ':' between IPv6 address and port".into(),
            )
            .into());
        }

        let port: u16 = port_str.parse().map_err(|e| {
            eggress_protocol_http::HttpError::TargetParseError(format!("invalid port: {e}"))
        })?;

        return Ok(TargetAddr {
            host: TargetHost::Ip(ip),
            port,
        });
    }

    let colon_pos = authority.rfind(':').ok_or_else(|| {
        eggress_protocol_http::HttpError::TargetParseError("missing port in authority".into())
    })?;

    let host_str = &authority[..colon_pos];
    let port_str = &authority[colon_pos + 1..];

    let port: u16 = port_str.parse().map_err(|e| {
        eggress_protocol_http::HttpError::TargetParseError(format!("invalid port: {e}"))
    })?;

    if let Ok(ip) = host_str.parse::<std::net::IpAddr>() {
        return Ok(TargetAddr {
            host: TargetHost::Ip(ip),
            port,
        });
    }

    if host_str.is_empty() {
        return Err(eggress_protocol_http::HttpError::TargetParseError("empty host".into()).into());
    }

    Ok(TargetAddr {
        host: TargetHost::Domain(host_str.to_string()),
        port,
    })
}

fn socks_addr_to_target(addr: &eggress_protocol_socks::socks5::server::SocksAddr) -> TargetAddr {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    match addr {
        SocksAddr::IPv4(octets, port) => TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V4((*octets).into())),
            port: *port,
        },
        SocksAddr::IPv6(octets, port) => TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V6((*octets).into())),
            port: *port,
        },
        SocksAddr::Domain(domain, port) => TargetAddr {
            host: TargetHost::Domain(domain.clone()),
            port: *port,
        },
    }
}

/// Maximum size for the HTTP request head (request line + headers).
const MAX_HEAD_SIZE: usize = 32 * 1024;

/// Maximum number of header lines.
const MAX_HEADER_LINES: usize = 128;

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_accept_socks5() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed).await.unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_socks4() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed).await.unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks4);
                    assert_eq!(pending.target.port, 80);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(&[0x04, 0x01, 0x00, 0x50, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&[0x00]).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_http_connect() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed).await.unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_http_forward() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed).await.unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.target.port, 80);
                    assert_eq!(pending.request.method, "GET");
                }
                _ => panic!("expected http forward"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET http://example.com/index.html HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }
}
