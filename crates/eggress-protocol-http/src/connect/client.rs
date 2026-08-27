use base64::Engine;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf};

use crate::error::HttpError;
use eggress_core::{BoxStream, TargetAddr, TargetHost};

/// Configuration limits for HTTP CONNECT response parsing.
#[derive(Debug, Clone)]
pub struct HttpConnectLimits {
    /// Maximum length of the status line (e.g., "HTTP/1.1 200 OK\r\n").
    pub max_status_line: usize,
    /// Maximum total bytes for response headers.
    pub max_headers_bytes: usize,
    /// Maximum number of header lines (excluding the status line).
    pub max_header_count: usize,
}

impl Default for HttpConnectLimits {
    fn default() -> Self {
        Self {
            max_status_line: 1024,
            max_headers_bytes: 32_768,
            max_header_count: 100,
        }
    }
}

/// Validate that a credential string contains no control characters.
///
/// Control characters are bytes < 0x20 (Space) or 0x7F (DEL).
pub fn validate_credentials(value: &str) -> Result<(), HttpError> {
    for byte in value.bytes() {
        if byte < 0x20 || byte == 0x7F {
            return Err(HttpError::InvalidCredentials);
        }
    }
    Ok(())
}

/// Send an HTTP CONNECT request to an upstream proxy and return the
/// upgraded stream on success.
///
/// # Arguments
/// * `stream` - The stream to the upstream proxy
/// * `target` - The target address to connect to
/// * `auth` - Optional (username, password) for Proxy-Authorization
/// * `limits` - Parsing limits for the response
///
/// # Returns
/// The stream after receiving a 2xx response, ready for bidirectional
/// forwarding.
pub async fn http_connect(
    stream: BoxStream,
    target: &TargetAddr,
    auth: Option<(&str, &str)>,
    limits: &HttpConnectLimits,
) -> Result<BoxStream, HttpError> {
    // BufReader preserves any response bytes read ahead of the CONNECT head
    // while avoiding a syscall for every response byte. Writes delegate to
    // the same underlying stream so the upgraded connection remains usable.
    let mut stream: BoxStream = Box::new(BufferedStream {
        reader: BufReader::new(stream),
    });

    // Validate credentials before sending anything
    if let Some((user, pass)) = auth {
        validate_credentials(user)?;
        validate_credentials(pass)?;
    }

    // Build CONNECT request
    let host_header = match &target.host {
        TargetHost::Ip(ip) => format!("{}", ip),
        TargetHost::Domain(domain) => domain.clone(),
    };

    let mut request = format!(
        "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n",
        host_header, target.port, host_header, target.port
    );

    // Add Proxy-Authorization if provided
    if let Some((user, pass)) = auth {
        let credentials = format!("{}:{}", user, pass);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        request.push_str(&format!("Proxy-Authorization: Basic {}\r\n", encoded));
    }

    request.push_str("\r\n");

    stream.write_all(request.as_bytes()).await?;
    stream.flush().await?;

    // Read response
    let response = read_response_head(&mut stream, limits).await?;

    // Parse status code
    let status = parse_status_code(&response, limits)?;

    match status {
        200..=299 => Ok(stream),
        407 => Err(HttpError::AuthRequired),
        403 => Err(HttpError::AuthFailed),
        502 => Err(HttpError::BadGateway),
        504 => Err(HttpError::GatewayTimeout),
        code => Err(HttpError::UnexpectedStatus(code)),
    }
}

struct BufferedStream {
    reader: BufReader<BoxStream>,
}

impl AsyncRead for BufferedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl AsyncWrite for BufferedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(self.reader.get_mut()).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(self.reader.get_mut()).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(self.reader.get_mut()).poll_shutdown(cx)
    }
}

/// Read the HTTP response head (status line + headers) from the stream.
async fn read_response_head(
    stream: &mut BoxStream,
    limits: &HttpConnectLimits,
) -> Result<String, HttpError> {
    let mut head_buf = Vec::with_capacity(1024);
    let mut temp = [0u8; 1];
    let mut header_count: usize = 0;
    let mut last_was_cr = false;

    loop {
        if head_buf.len() >= limits.max_headers_bytes {
            return Err(HttpError::HeaderTooLarge);
        }

        let n = stream.read(&mut temp).await?;
        if n == 0 {
            return Err(HttpError::MalformedResponse(
                "unexpected EOF reading response".into(),
            ));
        }

        head_buf.push(temp[0]);

        // Count header lines (each \r\n after status line is a header)
        if temp[0] == b'\n' && last_was_cr {
            header_count += 1;
            if header_count > limits.max_header_count {
                return Err(HttpError::TooManyHeaders);
            }
        }
        last_was_cr = temp[0] == b'\r';

        // Check for end of headers
        if head_buf.len() >= 4 {
            let len = head_buf.len();
            if &head_buf[len - 4..] == b"\r\n\r\n" {
                break;
            }
        }
    }

    String::from_utf8(head_buf)
        .map_err(|e| HttpError::MalformedResponse(format!("invalid UTF-8: {}", e)))
}

/// Parse the HTTP status code from a response head string.
///
/// Exposed for fuzzing; takes the full response head (status line + headers)
/// and returns the numeric status code from the first whitespace-separated
/// token after the HTTP version.
pub fn parse_status_code(response: &str, limits: &HttpConnectLimits) -> Result<u16, HttpError> {
    let first_line = response
        .lines()
        .next()
        .ok_or_else(|| HttpError::MalformedResponse("empty response".into()))?;

    if first_line.len() > limits.max_status_line {
        return Err(HttpError::MalformedResponse("status line too long".into()));
    }

    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(HttpError::MalformedResponse(format!(
            "invalid status line: {}",
            first_line
        )));
    }

    parts[1]
        .parse::<u16>()
        .map_err(|e| HttpError::MalformedResponse(format!("invalid status code: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(
            base64::engine::general_purpose::STANDARD.encode(b"test"),
            "dGVzdA=="
        );
        assert_eq!(
            base64::engine::general_purpose::STANDARD.encode(b"hello"),
            "aGVsbG8="
        );
        assert_eq!(
            base64::engine::general_purpose::STANDARD.encode(b"user:pass"),
            "dXNlcjpwYXNz"
        );
    }

    #[test]
    fn test_parse_status_code() {
        let limits = HttpConnectLimits::default();
        assert_eq!(
            parse_status_code("HTTP/1.1 200 Connection Established\r\n", &limits).unwrap(),
            200
        );
        assert_eq!(
            parse_status_code("HTTP/1.1 407 Proxy Authentication Required\r\n", &limits).unwrap(),
            407
        );
    }

    #[test]
    fn test_parse_status_code_invalid() {
        let limits = HttpConnectLimits::default();
        assert!(parse_status_code("HTTP/1.1", &limits).is_err());
        assert!(parse_status_code("HTTP/1.1 abc\r\n", &limits).is_err());
    }

    #[test]
    fn test_parse_status_code_too_long() {
        let limits = HttpConnectLimits {
            max_status_line: 10,
            ..Default::default()
        };
        assert!(parse_status_code("HTTP/1.1 200 OK\r\n", &limits).is_err());
    }

    #[test]
    fn test_validate_credentials_rejects_control_chars() {
        assert!(validate_credentials("user\x00name").is_err());
        assert!(validate_credentials("user\x1Fname").is_err());
        assert!(validate_credentials("user\x7Fname").is_err());
        assert!(validate_credentials("\x01").is_err());
        assert!(validate_credentials("\x09").is_err()); // TAB
    }

    #[test]
    fn test_validate_credentials_accepts_normal() {
        assert!(validate_credentials("user").is_ok());
        assert!(validate_credentials("user name").is_ok());
        assert!(validate_credentials("p@ss:word!").is_ok());
        assert!(validate_credentials("a]b[c").is_ok());
    }

    #[test]
    fn test_http_connect_limits_defaults() {
        let limits = HttpConnectLimits::default();
        assert_eq!(limits.max_status_line, 1024);
        assert_eq!(limits.max_headers_bytes, 32_768);
        assert_eq!(limits.max_header_count, 100);
    }

    #[test]
    fn test_parse_status_code_empty_response() {
        let limits = HttpConnectLimits::default();
        assert!(parse_status_code("", &limits).is_err());
    }

    #[test]
    fn test_parse_status_code_whitespace_only() {
        let limits = HttpConnectLimits::default();
        assert!(parse_status_code("   ", &limits).is_err());
    }

    // ===== Synthetic server integration tests =====

    #[tokio::test]
    async fn test_connect_200_success() {
        use crate::connect::test_server::{ProxyMode, TestProxyServer};

        let server = TestProxyServer::start(ProxyMode::Success).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let result = http_connect(boxed, &target, None, &HttpConnectLimits::default()).await;
        assert!(result.is_ok());
        server.stop().await;
    }

    #[tokio::test]
    async fn test_connect_407_auth_required() {
        use crate::connect::test_server::{ProxyMode, TestProxyServer};

        let server = TestProxyServer::start(ProxyMode::AuthRequired).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let result = http_connect(boxed, &target, None, &HttpConnectLimits::default()).await;
        assert!(matches!(result, Err(HttpError::AuthRequired)));
        server.stop().await;
    }

    #[tokio::test]
    async fn test_connect_403_forbidden() {
        use crate::connect::test_server::{ProxyMode, TestProxyServer};

        let server = TestProxyServer::start(ProxyMode::Forbidden).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let result = http_connect(boxed, &target, None, &HttpConnectLimits::default()).await;
        assert!(matches!(result, Err(HttpError::AuthFailed)));
        server.stop().await;
    }

    #[tokio::test]
    async fn test_connect_malformed_status() {
        use crate::connect::test_server::{ProxyMode, TestProxyServer};

        let server = TestProxyServer::start(ProxyMode::MalformedStatus).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let result = http_connect(boxed, &target, None, &HttpConnectLimits::default()).await;
        assert!(matches!(result, Err(HttpError::MalformedResponse(_))));
        server.stop().await;
    }

    #[tokio::test]
    async fn test_connect_slow_response_timeout() {
        use crate::connect::test_server::{ProxyMode, TestProxyServer};

        let server =
            TestProxyServer::start(ProxyMode::SlowResponse(std::time::Duration::from_secs(10)))
                .await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            http_connect(boxed, &target, None, &HttpConnectLimits::default()),
        )
        .await;
        assert!(result.is_err()); // timeout
        server.stop().await;
    }

    #[tokio::test]
    async fn test_connect_basic_auth_success() {
        use crate::connect::test_server::{ProxyMode, TestProxyServer};

        let server = TestProxyServer::start(ProxyMode::Success).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let result = http_connect(
            boxed,
            &target,
            Some(("user", "pass")),
            &HttpConnectLimits::default(),
        )
        .await;
        assert!(result.is_ok());
        server.stop().await;
    }

    #[tokio::test]
    async fn test_connect_basic_auth_wrong() {
        use crate::connect::test_server::{ProxyMode, TestProxyServer};

        let server = TestProxyServer::start(ProxyMode::AuthRequired).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let result = http_connect(
            boxed,
            &target,
            Some(("user", "wrong")),
            &HttpConnectLimits::default(),
        )
        .await;
        assert!(matches!(result, Err(HttpError::AuthRequired)));
        server.stop().await;
    }

    #[tokio::test]
    async fn test_connect_credentials_with_control_chars_rejected() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let jh = tokio::spawn(async move {
            let _ = listener.accept().await;
        });
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let result = http_connect(
            boxed,
            &target,
            Some(("user\x00", "pass")),
            &HttpConnectLimits::default(),
        )
        .await;
        assert!(matches!(result, Err(HttpError::InvalidCredentials)));
        jh.abort();
    }

    #[tokio::test]
    async fn test_connect_headers_too_large() {
        use crate::connect::test_server::{ProxyMode, TestProxyServer};

        let server = TestProxyServer::start(ProxyMode::HeadersTooLarge).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let result = http_connect(boxed, &target, None, &HttpConnectLimits::default()).await;
        assert!(matches!(
            result,
            Err(HttpError::HeaderTooLarge | HttpError::TooManyHeaders)
        ));
        server.stop().await;
    }
}
