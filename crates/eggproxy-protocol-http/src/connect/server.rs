use std::net::IpAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::HttpError;
use eggproxy_core::{BoxStream, TargetAddr, TargetHost};

/// Maximum size for the HTTP request head (request line + headers).
const MAX_HEAD_SIZE: usize = 32 * 1024;

/// Maximum number of header lines.
const MAX_HEADER_LINES: usize = 128;

/// Parsed CONNECT request.
#[derive(Debug, Clone)]
pub struct ConnectRequest {
    pub target: TargetAddr,
    pub proxy_auth: Option<(String, String)>,
}

/// Handle an HTTP CONNECT request from a client stream.
///
/// Parses the CONNECT request, validates it, and returns the stream
/// ready for bidirectional forwarding after sending a 200 response.
///
/// # Arguments
/// * `stream` - The client stream to read the CONNECT request from
/// * `require_auth` - Whether proxy authentication is required
/// * `valid_credentials` - Valid (username, password) pair for auth validation
///
/// # Returns
/// The parsed CONNECT request and the stream (with any bytes after the
/// request head preserved).
pub async fn handle_connect(
    mut stream: BoxStream,
    require_auth: bool,
    valid_credentials: Option<(&str, &str)>,
) -> Result<(ConnectRequest, BoxStream), HttpError> {
    let request = read_connect_request(&mut stream).await?;

    // Validate authentication if required
    if require_auth {
        match &request.proxy_auth {
            Some((user, pass)) => {
                if let Some((valid_user, valid_pass)) = valid_credentials {
                    if user != valid_user || pass != valid_pass {
                        write_error_response(&mut stream, 407, "Proxy Authentication Required")
                            .await?;
                        return Err(HttpError::AuthRequired);
                    }
                } else {
                    write_error_response(&mut stream, 407, "Proxy Authentication Required").await?;
                    return Err(HttpError::AuthRequired);
                }
            }
            None => {
                write_error_response(&mut stream, 407, "Proxy Authentication Required").await?;
                return Err(HttpError::AuthRequired);
            }
        }
    }

    // Send success response
    stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;
    stream.flush().await?;

    Ok((request, stream))
}

/// Read and parse an HTTP CONNECT request from the stream.
async fn read_connect_request(stream: &mut BoxStream) -> Result<ConnectRequest, HttpError> {
    let mut head_buf = Vec::with_capacity(1024);
    let mut temp = [0u8; 1];
    let mut header_count = 0;

    loop {
        if head_buf.len() >= MAX_HEAD_SIZE {
            return Err(HttpError::HeaderTooLarge);
        }

        let n = stream.read(&mut temp).await?;
        if n == 0 {
            return Err(HttpError::MalformedRequest(
                "unexpected EOF reading request".into(),
            ));
        }

        head_buf.push(temp[0]);

        // Check for end of headers (\r\n\r\n)
        if head_buf.len() >= 4 {
            let len = head_buf.len();
            if &head_buf[len - 4..] == b"\r\n\r\n" {
                break;
            }
            // Also count individual \r\n for header line limits
            if head_buf.len() >= 2 && &head_buf[len - 2..] == b"\r\n" {
                header_count += 1;
                if header_count > MAX_HEADER_LINES {
                    return Err(HttpError::TooManyHeaders);
                }
            }
        }
    }

    let head_str = String::from_utf8_lossy(&head_buf);
    let mut lines = head_str.split("\r\n");

    // Parse request line
    let request_line = lines
        .next()
        .ok_or_else(|| HttpError::MalformedRequest("empty request".into()))?;

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(HttpError::MalformedRequest(format!(
            "expected 3 parts in request line, got {}",
            parts.len()
        )));
    }

    if parts[0] != "CONNECT" {
        return Err(HttpError::MalformedRequest(format!(
            "expected CONNECT method, got {}",
            parts[0]
        )));
    }

    if parts[2] != "HTTP/1.1" && parts[2] != "HTTP/1.0" {
        return Err(HttpError::UnsupportedVersion(parts[2].to_string()));
    }

    // Parse authority (host:port)
    let authority = parts[1];
    let target = parse_authority(authority)?;

    // Parse headers
    let mut proxy_auth = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = parse_header_line(line) {
            if name.eq_ignore_ascii_case("Proxy-Authorization") {
                proxy_auth = parse_basic_auth(&value);
            }
        }
    }

    Ok(ConnectRequest { target, proxy_auth })
}

/// Parse an authority-form target (host:port).
fn parse_authority(authority: &str) -> Result<TargetAddr, HttpError> {
    // Handle IPv6 bracketed addresses: [::1]:port
    if authority.starts_with('[') {
        let bracket_end = authority.find(']').ok_or_else(|| {
            HttpError::TargetParseError("unclosed bracket in IPv6 address".into())
        })?;

        let ip_str = &authority[1..bracket_end];
        let ip: IpAddr = ip_str
            .parse()
            .map_err(|e| HttpError::TargetParseError(format!("invalid IPv6 address: {}", e)))?;

        let port_str = authority
            .get(bracket_end + 2..)
            .ok_or_else(|| HttpError::TargetParseError("missing port after IPv6 address".into()))?;

        if !authority
            .as_bytes()
            .get(bracket_end + 1)
            .is_some_and(|&b| b == b':')
        {
            return Err(HttpError::TargetParseError(
                "expected ':' between IPv6 address and port".into(),
            ));
        }

        let port: u16 = port_str
            .parse()
            .map_err(|e| HttpError::TargetParseError(format!("invalid port: {}", e)))?;

        return Ok(TargetAddr {
            host: TargetHost::Ip(ip),
            port,
        });
    }

    // Handle IPv4 or domain
    // Find the last ':' to split host and port
    let colon_pos = authority
        .rfind(':')
        .ok_or_else(|| HttpError::TargetParseError("missing port in authority".into()))?;

    let host_str = &authority[..colon_pos];
    let port_str = &authority[colon_pos + 1..];

    let port: u16 = port_str
        .parse()
        .map_err(|e| HttpError::TargetParseError(format!("invalid port: {}", e)))?;

    // Try to parse as IP first
    if let Ok(ip) = host_str.parse::<IpAddr>() {
        return Ok(TargetAddr {
            host: TargetHost::Ip(ip),
            port,
        });
    }

    // Otherwise treat as domain
    if host_str.is_empty() {
        return Err(HttpError::TargetParseError("empty host".into()));
    }

    Ok(TargetAddr {
        host: TargetHost::Domain(host_str.to_string()),
        port,
    })
}

/// Parse a header line into (name, value).
fn parse_header_line(line: &str) -> Option<(String, String)> {
    let colon_pos = line.find(':')?;
    let name = line[..colon_pos].trim().to_string();
    let value = line[colon_pos + 1..].trim().to_string();
    Some((name, value))
}

/// Parse Basic authentication from a Proxy-Authorization header value.
fn parse_basic_auth(value: &str) -> Option<(String, String)> {
    let value = value.trim();
    if !value.starts_with("Basic ") {
        return None;
    }

    let encoded = &value[6..];
    let decoded = base64_decode(encoded)?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    let colon_pos = decoded_str.find(':')?;
    let username = decoded_str[..colon_pos].to_string();
    let password = decoded_str[colon_pos + 1..].to_string();
    Some((username, password))
}

/// Simple base64 decoder (no-std compatible, no external dependency).
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let input = input.trim_end_matches('=');
    let input_bytes = input.as_bytes();

    let mut result = Vec::with_capacity(input_bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for &byte in input_bytes {
        let val = TABLE.iter().position(|&b| b == byte)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
        }
    }

    Some(result)
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
    fn test_parse_authority_ipv4() {
        let target = parse_authority("192.168.1.1:8080").unwrap();
        assert_eq!(
            target,
            TargetAddr {
                host: TargetHost::Ip("192.168.1.1".parse().unwrap()),
                port: 8080,
            }
        );
    }

    #[test]
    fn test_parse_authority_ipv6() {
        let target = parse_authority("[::1]:443").unwrap();
        assert_eq!(
            target,
            TargetAddr {
                host: TargetHost::Ip("::1".parse().unwrap()),
                port: 443,
            }
        );
    }

    #[test]
    fn test_parse_authority_domain() {
        let target = parse_authority("example.com:443").unwrap();
        assert_eq!(
            target,
            TargetAddr {
                host: TargetHost::Domain("example.com".to_string()),
                port: 443,
            }
        );
    }

    #[test]
    fn test_parse_authority_missing_port() {
        assert!(parse_authority("example.com").is_err());
    }

    #[test]
    fn test_parse_header_line() {
        let (name, value) = parse_header_line("Host: example.com").unwrap();
        assert_eq!(name, "Host");
        assert_eq!(value, "example.com");
    }

    #[test]
    fn test_parse_basic_auth() {
        // "user:pass" base64 encoded is "dXNlcjpwYXNz"
        let result = parse_basic_auth("Basic dXNlcjpwYXNz").unwrap();
        assert_eq!(result, ("user".to_string(), "pass".to_string()));
    }

    #[test]
    fn test_parse_basic_auth_no_prefix() {
        assert!(parse_basic_auth("Bearer token").is_none());
    }

    #[test]
    fn test_base64_decode() {
        let decoded = base64_decode("dGVzdA==").unwrap();
        assert_eq!(decoded, b"test");
    }
}
