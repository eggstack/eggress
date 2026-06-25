use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    mut stream: BoxStream,
    target: &TargetAddr,
    auth: Option<(&str, &str)>,
    limits: &HttpConnectLimits,
) -> Result<BoxStream, HttpError> {
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
        let encoded = base64_encode(credentials.as_bytes());
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
        407 => {
            let _ = write_error_response(&mut stream, 407, "Proxy Authentication Required").await;
            Err(HttpError::AuthRequired)
        }
        403 => {
            let _ = write_error_response(&mut stream, 403, "Forbidden").await;
            Err(HttpError::AuthFailed)
        }
        502 => {
            let _ = write_error_response(&mut stream, 502, "Bad Gateway").await;
            Err(HttpError::BadGateway)
        }
        504 => {
            let _ = write_error_response(&mut stream, 504, "Gateway Timeout").await;
            Err(HttpError::GatewayTimeout)
        }
        code => {
            let _ = write_error_response(&mut stream, code, "Upstream Error").await;
            Err(HttpError::UnexpectedStatus(code))
        }
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
fn parse_status_code(response: &str, limits: &HttpConnectLimits) -> Result<u16, HttpError> {
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

/// Simple base64 encoder (no-std compatible, no external dependency).
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::with_capacity(input.len().div_ceil(3) * 4);

    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        result.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// Write an HTTP error response.
async fn write_error_response(
    stream: &mut BoxStream,
    status: u16,
    reason: &str,
) -> Result<(), HttpError> {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        status, reason
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"test"), "dGVzdA==");
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(b"user:pass"), "dXNlcjpwYXNz");
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
