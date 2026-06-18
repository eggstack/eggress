use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::HttpError;
use eggress_core::{BoxStream, TargetAddr, TargetHost};

/// Maximum size for the HTTP response head.
const MAX_RESPONSE_HEAD_SIZE: usize = 32 * 1024;

/// Send an HTTP CONNECT request to an upstream proxy and return the
/// upgraded stream on success.
///
/// # Arguments
/// * `stream` - The stream to the upstream proxy
/// * `target` - The target address to connect to
/// * `auth` - Optional (username, password) for Proxy-Authorization
///
/// # Returns
/// The stream after receiving a 200 response, ready for bidirectional
/// forwarding.
pub async fn http_connect(
    mut stream: BoxStream,
    target: &TargetAddr,
    auth: Option<(&str, &str)>,
) -> Result<BoxStream, HttpError> {
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
    let response = read_response_head(&mut stream).await?;

    // Parse status code
    let status = parse_status_code(&response)?;

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
async fn read_response_head(stream: &mut BoxStream) -> Result<String, HttpError> {
    let mut head_buf = Vec::with_capacity(1024);
    let mut temp = [0u8; 1];

    loop {
        if head_buf.len() >= MAX_RESPONSE_HEAD_SIZE {
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

    String::from_utf8(head_buf)
        .map_err(|e| HttpError::MalformedResponse(format!("invalid UTF-8: {}", e)))
}

/// Parse the HTTP status code from a response head string.
fn parse_status_code(response: &str) -> Result<u16, HttpError> {
    let first_line = response
        .lines()
        .next()
        .ok_or_else(|| HttpError::MalformedResponse("empty response".into()))?;

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
        assert_eq!(
            parse_status_code("HTTP/1.1 200 Connection Established\r\n").unwrap(),
            200
        );
        assert_eq!(
            parse_status_code("HTTP/1.1 407 Proxy Authentication Required\r\n").unwrap(),
            407
        );
    }

    #[test]
    fn test_parse_status_code_invalid() {
        assert!(parse_status_code("HTTP/1.1").is_err());
        assert!(parse_status_code("HTTP/1.1 abc\r\n").is_err());
    }
}
