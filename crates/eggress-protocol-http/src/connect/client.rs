use base64::Engine;
use std::pin::Pin;
use std::task::{Context, Poll};

use eggfetch_http_connect::{encode_connect_request, ConnectError, ConnectRequest, ConnectTarget};
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

/// Compatibility-unbounded bound for the serialized outbound CONNECT request head.
///
/// Eggress historically had no independently documented outbound
/// request-head-size limit: the URI/config layers place no length bound on
/// hosts or credentials, and the previous local builder grew a `Vec`
/// without a cap. `encode_connect_request()` requires a caller-provided
/// bound, so this adapter passes `usize::MAX` to preserve the pre-migration
/// contract: valid credentials/targets must not be newly rejected at an
/// adapter-imposed size. A true request-size hardening policy, if desired,
/// must be designed separately at the input/config layer before credential
/// and authority allocations occur.
const COMPAT_UNBOUNDED_REQUEST_HEAD: usize = usize::MAX;

/// Build the validated outbound CONNECT target.
///
/// Eggress target validation stays local because its accepted domain is
/// narrower than `ConnectTarget::new()`: Eggress additionally rejects `:`,
/// `@`, and `/` in domains, as well as empty hosts and bytes that could
/// split the request line (CR, LF, other ASCII controls, DEL, and space).
/// After the established validation passes, the approved host and port
/// construct a `ConnectTarget` so authority rendering (including IPv6
/// bracketing) is owned by `eggfetch-http-connect` and cannot diverge
/// between the request line and the `Host` header: domains and IPv4
/// literals render as `host:port`, IPv6 bracketed as `[addr]:port`.
fn connect_target(target: &TargetAddr) -> Result<ConnectTarget, HttpError> {
    match &target.host {
        TargetHost::Ip(ip) => ConnectTarget::new(ip.to_string(), target.port)
            .map_err(|_| HttpError::TargetParseError("invalid CONNECT target host".into())),
        TargetHost::Domain(domain) => {
            if domain.is_empty() {
                return Err(HttpError::TargetParseError(
                    "empty CONNECT target host".into(),
                ));
            }
            for byte in domain.bytes() {
                if byte <= 0x20 || byte == 0x7F {
                    return Err(HttpError::TargetParseError(
                        "invalid CONNECT target host".into(),
                    ));
                }
            }
            if domain.contains(':') || domain.contains('@') || domain.contains('/') {
                return Err(HttpError::TargetParseError(
                    "invalid CONNECT target host".into(),
                ));
            }
            ConnectTarget::new(domain.clone(), target.port)
                .map_err(|_| HttpError::TargetParseError("invalid CONNECT target host".into()))
        }
    }
}

/// Map an `eggfetch-http-connect` construction error into the existing
/// `HttpError` categories without adding public variants.
///
/// Upstream diagnostics never echo credentials or unbounded proxy bytes;
/// the mapping additionally collapses every variant to a fixed,
/// non-leaking Eggress error. The request path only exercises the target
/// and credential arms under the compatibility-unbounded head bound: no
/// extra headers are supplied and no I/O or response parsing happens here,
/// so the head-size arm is retained only as a fixed mapping and the
/// catch-all covers construction states that cannot arise from the inputs
/// above.
fn map_connect_error(error: ConnectError) -> HttpError {
    match error {
        ConnectError::InvalidTarget => {
            HttpError::TargetParseError("invalid CONNECT target host".into())
        }
        ConnectError::InvalidCredentials => HttpError::InvalidCredentials,
        ConnectError::RequestHeadTooLarge { .. } => HttpError::HeaderTooLarge,
        _ => HttpError::MalformedRequest("invalid CONNECT request".into()),
    }
}

/// Build the CONNECT request head as bytes.
///
/// Validates credentials before producing any wire bytes. Errors never
/// include credential material.
///
/// Credential acceptance stays an Eggress contract: `validate_credentials()`
/// permits `:` in usernames while the upstream `basic_auth_value()` helper
/// rejects it, so the `Basic` value is encoded locally with the established
/// `username:password` Base64 semantics and passed pre-encoded through
/// `ConnectRequest::proxy_authorization`. Request framing itself is owned
/// by `encode_connect_request()`.
fn build_connect_request(
    target: &ConnectTarget,
    auth: Option<(&str, &str)>,
) -> Result<Vec<u8>, HttpError> {
    if let Some((user, pass)) = auth {
        validate_credentials(user)?;
        validate_credentials(pass)?;
    }
    let auth_value = auth.map(|(user, pass)| {
        let credentials = format!("{}:{}", user, pass);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        format!("Basic {encoded}")
    });
    let request = ConnectRequest {
        target,
        proxy_authorization: auth_value.as_deref(),
        extra_headers: &[],
        max_head_bytes: COMPAT_UNBOUNDED_REQUEST_HEAD,
    };
    encode_connect_request(&request).map_err(map_connect_error)
}

/// Parse a status code from status-line bytes.
///
/// Only the status line must be valid UTF-8; header values elsewhere may
/// carry arbitrary obs-text bytes. Mirrors the validation of the
/// string-based [`parse_status_code`] compatibility helper.
fn parse_status_from_bytes(
    status_line: &[u8],
    limits: &HttpConnectLimits,
) -> Result<u16, HttpError> {
    if status_line.len() > limits.max_status_line {
        return Err(HttpError::HeaderTooLarge);
    }
    let line = std::str::from_utf8(status_line)
        .map_err(|e| HttpError::MalformedResponse(format!("invalid UTF-8: {}", e)))?;
    // Reuse the same token rules as the public helper without requiring
    // the full head to be UTF-8.
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(HttpError::MalformedResponse(format!(
            "invalid status line: {}",
            line
        )));
    }
    parts[1]
        .parse::<u16>()
        .map_err(|e| HttpError::MalformedResponse(format!("invalid status code: {}", e)))
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

    // Authority is computed first so request-line and Host agree, including
    // bracketed IPv6 form. Credential validation happens inside the builder
    // before any wire bytes are produced.
    let target = connect_target(target)?;
    let request = build_connect_request(&target, auth)?;

    stream.write_all(&request).await?;
    stream.flush().await?;

    // Byte-preserving response-head read: only the status line must be
    // UTF-8; header values may carry obs-text bytes.
    let status = read_response_status(&mut stream, limits).await?;

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

/// Read one CONNECT response head and return its status code.
///
/// Response parsing intentionally remains local to Eggress (Phase 5
/// Outcome B): the public `HttpConnectLimits::max_headers_bytes` accounts
/// the complete response head while reading, whereas
/// `eggfetch-http-connect 0.2.0`'s `ConnectResponseLimits` separates a
/// per-line cap from an aggregate header-field-bytes sum that excludes the
/// status line and CRLF framing — and additionally requires `name: value`
/// header shape where Eggress counts lines. Delegating would silently
/// narrow or widen the accepted domain, so only request/authority
/// ownership is shared; this parser is not an accidental duplicate.
///
/// Reads bytes until the terminating `\r\n\r\n`, enforcing the total-head
/// limit during the read. Header-count semantics match the documented
/// contract: the count is the number of actual header fields, excluding the
/// status line and the terminating empty line. Header values are retained
/// as bytes and are never required to be UTF-8; only the status line is
/// decoded to extract the code.
///
/// Any bytes already buffered beyond the terminator stay in the `BufReader`
/// inside `BufferedStream`, so the returned stream yields them first.
async fn read_response_status(
    stream: &mut BoxStream,
    limits: &HttpConnectLimits,
) -> Result<u16, HttpError> {
    let mut head_buf = Vec::with_capacity(1024);
    let mut temp = [0u8; 1];

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

        // Check for end of headers
        if head_buf.len() >= 4 {
            let len = head_buf.len();
            if &head_buf[len - 4..] == b"\r\n\r\n" {
                break;
            }
        }
    }

    // Split status line from the remainder without converting headers.
    let status_end = head_buf
        .windows(2)
        .position(|w| w == b"\r\n")
        .ok_or_else(|| HttpError::MalformedResponse("empty response".into()))?;
    let status_line = &head_buf[..status_end];
    if status_line.is_empty() {
        return Err(HttpError::MalformedResponse("empty response".into()));
    }
    let status = parse_status_from_bytes(status_line, limits)?;

    // Count actual header fields: every CRLF-terminated line after the
    // status line, excluding the final empty terminator line.
    let mut header_count: usize = 0;
    let mut line_start = status_end + 2;
    while line_start < head_buf.len() {
        let rest = &head_buf[line_start..];
        let next = rest
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| HttpError::MalformedResponse("truncated header section".into()))?;
        if next == 0 {
            // Terminating empty line: not a header field.
            break;
        }
        header_count += 1;
        if header_count > limits.max_header_count {
            return Err(HttpError::TooManyHeaders);
        }
        line_start += next + 2;
    }

    Ok(status)
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

    // ===== Workstream 6 regression matrix (public boundary) =====

    async fn canned_exchange(
        target: TargetAddr,
        auth: Option<(&str, &str)>,
        limits: HttpConnectLimits,
        response: Vec<u8>,
    ) -> (Result<BoxStream, HttpError>, Vec<u8>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            let mut tmp = [0u8; 1];
            loop {
                let n = sock.read(&mut tmp).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                req.push(tmp[0]);
                if req.len() >= 4 && &req[req.len() - 4..] == b"\r\n\r\n" {
                    break;
                }
                // Test-harness bound only: the production adapter is
                // compatibility-unbounded, so allow heads well above the
                // former 64 KiB cap (the >64 KiB regression is ~96 KiB).
                if req.len() > 2 * 1024 * 1024 {
                    break;
                }
            }
            let _ = sock.write_all(&response).await;
            let _ = sock.flush().await;
            // Keep the tunnel open briefly so the client can read
            // pipelined bytes and the server side stays usable.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            req
        });
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        // Clone auth strings to satisfy lifetimes inside async call.
        let result = match auth {
            Some((u, p)) => {
                let u = u.to_owned();
                let p = p.to_owned();
                http_connect(boxed, &target, Some((u.as_str(), p.as_str())), &limits).await
            }
            None => http_connect(boxed, &target, None, &limits).await,
        };
        let captured = server.await.unwrap();
        (result, captured)
    }

    #[test]
    fn test_authority_form_brackets_ipv6() {
        let v6 = TargetAddr {
            host: TargetHost::Ip("::1".parse().unwrap()),
            port: 443,
        };
        assert_eq!(connect_target(&v6).unwrap().authority(), "[::1]:443");
        let v6_full = TargetAddr {
            host: TargetHost::Ip("2001:db8::1".parse().unwrap()),
            port: 8080,
        };
        assert_eq!(
            connect_target(&v6_full).unwrap().authority(),
            "[2001:db8::1]:8080"
        );
        let v4 = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        assert_eq!(connect_target(&v4).unwrap().authority(), "127.0.0.1:80");
        let domain = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 443,
        };
        assert_eq!(
            connect_target(&domain).unwrap().authority(),
            "example.com:443"
        );
    }

    #[test]
    fn test_authority_form_rejects_injection() {
        for bad in ["", "a\rb", "a\nb", "a b", "a:b", "a/b", "a@b", "a\x7Fb"] {
            let target = TargetAddr {
                host: TargetHost::Domain(bad.into()),
                port: 80,
            };
            assert!(
                connect_target(&target).is_err(),
                "host {bad:?} must be rejected"
            );
        }
    }

    #[tokio::test]
    async fn test_wire_domain_authority_and_host_agree() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 8443,
        };
        let (result, req) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 Connection Established\r\n\r\n".to_vec(),
        )
        .await;
        assert!(result.is_ok());
        let text = String::from_utf8(req).unwrap();
        assert!(text.starts_with("CONNECT example.com:8443 HTTP/1.1\r\n"));
        assert!(text.contains("\r\nHost: example.com:8443\r\n"));
    }

    #[tokio::test]
    async fn test_wire_ipv4_authority_and_host_agree() {
        let target = TargetAddr {
            host: TargetHost::Ip("192.0.2.1".parse().unwrap()),
            port: 3128,
        };
        let (result, req) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 Connection Established\r\n\r\n".to_vec(),
        )
        .await;
        assert!(result.is_ok());
        let text = String::from_utf8(req).unwrap();
        assert!(text.starts_with("CONNECT 192.0.2.1:3128 HTTP/1.1\r\n"));
        assert!(text.contains("\r\nHost: 192.0.2.1:3128\r\n"));
    }

    #[tokio::test]
    async fn test_wire_ipv6_bracketed_in_request_and_host() {
        let target = TargetAddr {
            host: TargetHost::Ip("::1".parse().unwrap()),
            port: 443,
        };
        let (result, req) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 Connection Established\r\n\r\n".to_vec(),
        )
        .await;
        assert!(result.is_ok());
        let text = String::from_utf8(req).unwrap();
        assert!(
            text.starts_with("CONNECT [::1]:443 HTTP/1.1\r\n"),
            "request line must bracket IPv6, got: {text:?}"
        );
        assert!(
            text.contains("\r\nHost: [::1]:443\r\n"),
            "Host must bracket IPv6, got: {text:?}"
        );
        assert!(!text.contains("CONNECT ::1:443"));
    }

    #[tokio::test]
    async fn test_wire_non_default_port_preserved() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 8443,
        };
        let (result, req) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 OK\r\n\r\n".to_vec(),
        )
        .await;
        assert!(result.is_ok());
        let text = String::from_utf8(req).unwrap();
        assert!(text.contains("example.com:8443"));
    }

    #[tokio::test]
    async fn test_wire_basic_auth_header_present() {
        use base64::Engine;
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, req) = canned_exchange(
            target,
            Some(("user", "pass")),
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 OK\r\n\r\n".to_vec(),
        )
        .await;
        assert!(result.is_ok());
        let expected = base64::engine::general_purpose::STANDARD.encode("user:pass");
        let text = String::from_utf8(req).unwrap();
        assert!(text.contains(&format!("Proxy-Authorization: Basic {}", expected)));
    }

    #[tokio::test]
    async fn test_wire_colon_username_auth_header_preserved() {
        // Eggress accepts `:` in usernames; the upstream `basic_auth_value`
        // helper rejects them, so this wire case pins the compatibility
        // adapter: the value is encoded locally and framed upstream.
        use base64::Engine;
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, req) = canned_exchange(
            target,
            Some(("user:name", "pass")),
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 OK\r\n\r\n".to_vec(),
        )
        .await;
        assert!(result.is_ok());
        let expected = base64::engine::general_purpose::STANDARD.encode("user:name:pass");
        let text = String::from_utf8(req).unwrap();
        assert!(text.contains(&format!("Proxy-Authorization: Basic {}", expected)));
        assert!(text.starts_with("CONNECT example.com:80 HTTP/1.1\r\n"));
        assert!(text.contains("\r\nHost: example.com:80\r\n"));
    }

    #[tokio::test]
    async fn test_wire_large_credentials_above_64kib_preserved() {
        // Corrective regression: the pre-migration builder had no
        // request-head cap, so a valid >64 KiB CONNECT head must succeed.
        // A 72 KiB password serializes to ~96 KiB on the wire and would
        // have failed under the removed 64 KiB adapter bound.
        use base64::Engine;
        let large_password = "a".repeat(72 * 1024);
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, req) = canned_exchange(
            target,
            Some(("user", large_password.as_str())),
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 OK\r\n\r\n".to_vec(),
        )
        .await;
        assert!(result.is_ok(), "large valid head must not be rejected");
        assert!(
            req.len() > 64 * 1024,
            "regression must actually exceed 64 KiB, got {}",
            req.len()
        );
        let expected =
            base64::engine::general_purpose::STANDARD.encode(format!("user:{large_password}"));
        let text = String::from_utf8(req).unwrap();
        assert!(text.contains(&format!("Proxy-Authorization: Basic {expected}")));
        assert!(text.starts_with("CONNECT example.com:80 HTTP/1.1\r\n"));
        assert!(text.contains("\r\nHost: example.com:80\r\n"));
    }

    #[tokio::test]
    async fn test_wire_empty_and_non_ascii_credentials_preserved() {
        use base64::Engine;
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (empty_result, empty_req) = canned_exchange(
            target.clone(),
            Some(("", "")),
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 OK\r\n\r\n".to_vec(),
        )
        .await;
        assert!(empty_result.is_ok());
        let expected_empty = base64::engine::general_purpose::STANDARD.encode(":");
        let empty_text = String::from_utf8(empty_req).unwrap();
        assert!(empty_text.contains(&format!("Proxy-Authorization: Basic {}", expected_empty)));

        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (unicode_result, unicode_req) = canned_exchange(
            target,
            Some(("usér", "pâss")),
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 OK\r\n\r\n".to_vec(),
        )
        .await;
        assert!(unicode_result.is_ok());
        let expected_unicode =
            base64::engine::general_purpose::STANDARD.encode("usér:pâss".as_bytes());
        let unicode_text = String::from_utf8(unicode_req).unwrap();
        assert!(unicode_text.contains(&format!("Proxy-Authorization: Basic {}", expected_unicode)));
    }

    #[tokio::test]
    async fn test_response_header_without_colon_accepted_outcome_b_pin() {
        // Outcome B pin: the local response parser counts header lines
        // without requiring `name: value` shape, while the upstream
        // `read_connect_response_head` rejects colon-less lines as
        // `MalformedHeader`. This acceptance must survive the migration.
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, _) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 Connection Established\r\nNotAHeaderLine\r\n\r\n".to_vec(),
        )
        .await;
        assert!(
            result.is_ok(),
            "colon-less header lines must be counted, not rejected"
        );
    }

    #[tokio::test]
    async fn test_response_long_single_header_line_accepted_outcome_b_pin() {
        // Outcome B pin: `max_headers_bytes` accounts the total head, with
        // no per-line cap. A single 20 KiB header line fits the default
        // 32 KiB total but would exceed the upstream per-line default
        // (8192 B), so delegation would newly reject it.
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let mut resp = b"HTTP/1.1 200 Connection Established\r\nX-Pad: ".to_vec();
        resp.extend_from_slice(&vec![b'A'; 20 * 1024]);
        resp.extend_from_slice(b"\r\n\r\n");
        let (result, _) = canned_exchange(target, None, HttpConnectLimits::default(), resp).await;
        assert!(
            result.is_ok(),
            "long single header line within total budget must pass"
        );
    }

    #[tokio::test]
    async fn test_credentials_rejected_before_write() {
        use tokio::io::AsyncReadExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            // If any request byte arrives within 200ms, the client wrote
            // before validating credentials.
            let mut tmp = [0u8; 1];
            let read = sock.read(&mut tmp);
            match tokio::time::timeout(std::time::Duration::from_millis(200), read).await {
                Ok(Ok(n)) => n,
                _ => 0,
            }
        });
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let result = http_connect(
            boxed,
            &target,
            Some(("bad\x00user", "pass")),
            &HttpConnectLimits::default(),
        )
        .await;
        assert!(matches!(result, Err(HttpError::InvalidCredentials)));
        assert_eq!(
            server.await.unwrap(),
            0,
            "no request bytes may precede validation"
        );
    }

    #[tokio::test]
    async fn test_no_credential_leak_in_errors() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, _) = canned_exchange(
            target,
            Some(("secretuser", "secretpass123")),
            HttpConnectLimits::default(),
            b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n".to_vec(),
        )
        .await;
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected 407 error"),
        };
        let display = format!("{}", err);
        let debug = format!("{:?}", err);
        assert!(!display.contains("secretuser"));
        assert!(!display.contains("secretpass123"));
        assert!(!debug.contains("secretuser"));
        assert!(!debug.contains("secretpass123"));
        // Base64(user:pass) must not appear either.
        assert!(!display.contains("c2VjcmV0dXNlcjpzZWNyZXRwYXNzMTIz"));
        assert!(!debug.contains("c2VjcmV0dXNlcjpzZWNyZXRwYXNzMTIz"));
    }

    #[tokio::test]
    async fn test_status_201_succeeds_preserving_any_2xx() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, _) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 201 Created\r\n\r\n".to_vec(),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_status_204_succeeds() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, _) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 204 No Content\r\n\r\n".to_vec(),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_status_502_504_mappings() {
        let target = || TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (r502, _) = canned_exchange(
            target(),
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 502 Bad Gateway\r\n\r\n".to_vec(),
        )
        .await;
        assert!(matches!(r502, Err(HttpError::BadGateway)));
        let (r504, _) = canned_exchange(
            target(),
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 504 Gateway Timeout\r\n\r\n".to_vec(),
        )
        .await;
        assert!(matches!(r504, Err(HttpError::GatewayTimeout)));
    }

    #[tokio::test]
    async fn test_status_arbitrary_non_2xx_unexpected() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, _) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 500 Internal Server Error\r\n\r\n".to_vec(),
        )
        .await;
        assert!(matches!(result, Err(HttpError::UnexpectedStatus(500))));
    }

    #[tokio::test]
    async fn test_truncated_response_rejected() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, _) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            b"HTTP/1.1 200 OK\r\nX-A: 1\r\n".to_vec(),
        )
        .await;
        assert!(matches!(result, Err(HttpError::MalformedResponse(_))));
    }

    #[tokio::test]
    async fn test_overlong_status_rejected() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let long = format!("HTTP/1.1 200 {}\r\n\r\n", "A".repeat(2000));
        let (result, _) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            long.into_bytes(),
        )
        .await;
        assert!(
            matches!(
                result,
                Err(HttpError::HeaderTooLarge | HttpError::MalformedResponse(_))
            ),
            "overlong status must be rejected"
        );
    }

    #[tokio::test]
    async fn test_total_head_limit_enforced() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let limits = HttpConnectLimits {
            max_headers_bytes: 256,
            ..Default::default()
        };
        let mut resp = b"HTTP/1.1 200 OK\r\n".to_vec();
        for _ in 0..32 {
            resp.extend_from_slice(b"X-Pad: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\r\n");
        }
        resp.extend_from_slice(b"\r\n");
        let (result, _) = canned_exchange(target, None, limits, resp).await;
        assert!(matches!(result, Err(HttpError::HeaderTooLarge)));
    }

    fn headers_response(count: usize) -> Vec<u8> {
        let mut resp = b"HTTP/1.1 200 OK\r\n".to_vec();
        for i in 0..count {
            resp.extend_from_slice(format!("X-Pad-{:03}: a\r\n", i).as_bytes());
        }
        resp.extend_from_slice(b"\r\n");
        resp
    }

    #[tokio::test]
    async fn test_exactly_max_headers_accepted() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, _) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            headers_response(100),
        )
        .await;
        assert!(result.is_ok(), "100 headers must be accepted");
    }

    #[tokio::test]
    async fn test_max_plus_one_headers_rejected() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let (result, _) = canned_exchange(
            target,
            None,
            HttpConnectLimits::default(),
            headers_response(101),
        )
        .await;
        assert!(
            matches!(result, Err(HttpError::TooManyHeaders)),
            "101 headers must be rejected"
        );
    }

    #[tokio::test]
    async fn test_non_utf8_header_value_accepted() {
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let mut resp = b"HTTP/1.1 200 Connection Established\r\nX-Bin: ".to_vec();
        resp.extend_from_slice(&[0xFF, 0xFE, 0x80]);
        resp.extend_from_slice(b"\r\n\r\n");
        let (result, _) = canned_exchange(target, None, HttpConnectLimits::default(), resp).await;
        assert!(result.is_ok(), "obs-text header must not fail");
    }

    #[tokio::test]
    async fn test_read_ahead_bytes_preserved() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            let mut tmp = [0u8; 1];
            loop {
                let n = sock.read(&mut tmp).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                req.push(tmp[0]);
                if req.len() >= 4 && &req[req.len() - 4..] == b"\r\n\r\n" {
                    break;
                }
            }
            // Send head plus pipelined tunnel bytes in one write.
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\n\r\nPIPELINED-BYTES")
                .await;
            let _ = sock.flush().await;
            // Then send later bytes after a short delay.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let _ = sock.write_all(b"-LATER").await;
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        });
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
        let mut tunnel = http_connect(boxed, &target, None, &HttpConnectLimits::default())
            .await
            .expect("200 must succeed");
        let mut buf = Vec::new();
        let mut tmp = [0u8; 32];
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        while buf.len() < b"PIPELINED-BYTES-LATER".len() {
            if tokio::time::Instant::now() > deadline {
                break;
            }
            match tokio::time::timeout(std::time::Duration::from_millis(500), tunnel.read(&mut tmp))
                .await
            {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => buf.extend_from_slice(&tmp[..n]),
                _ => break,
            }
        }
        assert_eq!(buf, b"PIPELINED-BYTES-LATER");
        server.abort();
    }
}
