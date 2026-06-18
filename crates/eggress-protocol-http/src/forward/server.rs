use tokio::io::AsyncReadExt;

use crate::error::HttpError;
use eggress_core::{BoxStream, TargetAddr, TargetHost};

/// Maximum size for the HTTP request head (request line + headers).
const MAX_HEAD_SIZE: usize = 32 * 1024;

/// Maximum number of header lines.
const MAX_HEADER_LINES: usize = 128;

/// A parsed HTTP request ready for forwarding.
#[derive(Debug, Clone)]
pub struct ForwardRequest {
    pub method: String,
    pub path: String,
    pub version: String,
    pub headers: Vec<(String, String)>,
    pub target: TargetAddr,
    pub has_body: bool,
    pub content_length: Option<u64>,
    pub is_chunked: bool,
}

/// Forward an HTTP request from a client to the target server.
///
/// Parses the absolute-form request, converts to origin-form, forwards
/// the request, and returns the response.
///
/// # Arguments
/// * `stream` - The client stream
///
/// # Returns
/// The parsed forward request and the target address to connect to.
pub async fn forward_request(
    mut stream: BoxStream,
) -> Result<(ForwardRequest, BoxStream), HttpError> {
    let request = read_forward_request(&mut stream).await?;
    Ok((request, stream))
}

/// Read and parse an HTTP forward request with absolute-form target.
async fn read_forward_request(stream: &mut BoxStream) -> Result<ForwardRequest, HttpError> {
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

        // Check for end of headers
        if head_buf.len() >= 4 {
            let len = head_buf.len();
            if &head_buf[len - 4..] == b"\r\n\r\n" {
                break;
            }
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

    let method = parts[0].to_string();
    let raw_target = parts[1].to_string();
    let version = parts[2].to_string();

    // Parse absolute-form target: http://host:port/path
    let (target, path) = parse_absolute_uri(&raw_target)?;

    // Parse headers
    let mut headers = Vec::new();
    let mut has_body = false;
    let mut content_length = None;
    let mut is_chunked = false;

    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = parse_header_line(line) {
            // Skip Proxy-Authorization header (don't forward it)
            if name.eq_ignore_ascii_case("Proxy-Authorization") {
                continue;
            }

            if name.eq_ignore_ascii_case("Content-Length") {
                content_length = value.parse::<u64>().ok();
                has_body = true;
            } else if name.eq_ignore_ascii_case("Transfer-Encoding")
                && value.eq_ignore_ascii_case("chunked")
            {
                is_chunked = true;
                has_body = true;
            }

            headers.push((name, value));
        }
    }

    // Determine if there's a body based on method
    if method != "GET"
        && method != "HEAD"
        && method != "DELETE"
        && method != "OPTIONS"
        && (content_length.is_some() || is_chunked)
    {
        has_body = true;
    }

    Ok(ForwardRequest {
        method,
        path,
        version,
        headers,
        target,
        has_body,
        content_length,
        is_chunked,
    })
}

/// Parse an absolute-form URI into target and path.
///
/// Supports: http://host:port/path, http://host/path
fn parse_absolute_uri(uri: &str) -> Result<(TargetAddr, String), HttpError> {
    // Remove scheme and determine default port
    let (rest, default_port) = if let Some(stripped) = uri.strip_prefix("http://") {
        (stripped, 80)
    } else if let Some(stripped) = uri.strip_prefix("https://") {
        // For HTTPS, we'd need TLS, but for now treat as HTTP
        (stripped, 443)
    } else {
        return Err(HttpError::MalformedRequest(format!(
            "absolute URI required, got: {}",
            uri
        )));
    };

    // Find path separator
    let path_start = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..path_start];
    let path = if path_start < rest.len() {
        &rest[path_start..]
    } else {
        "/"
    };

    // Parse authority with default port
    let target = parse_authority_with_default(authority, default_port)?;

    Ok((target, path.to_string()))
}

/// Parse an authority (host:port) into a TargetAddr with a default port.
fn parse_authority_with_default(
    authority: &str,
    default_port: u16,
) -> Result<TargetAddr, HttpError> {
    // Handle IPv6 bracketed addresses
    if authority.starts_with('[') {
        let bracket_end = authority.find(']').ok_or_else(|| {
            HttpError::TargetParseError("unclosed bracket in IPv6 address".into())
        })?;

        let ip_str = &authority[1..bracket_end];
        let ip: std::net::IpAddr = ip_str
            .parse()
            .map_err(|e| HttpError::TargetParseError(format!("invalid IPv6 address: {}", e)))?;

        // Check for port after bracket
        let port = if authority
            .as_bytes()
            .get(bracket_end + 1)
            .is_some_and(|&b| b == b':')
        {
            let port_str = authority.get(bracket_end + 2..).ok_or_else(|| {
                HttpError::TargetParseError("missing port after IPv6 address".into())
            })?;
            port_str
                .parse()
                .map_err(|e| HttpError::TargetParseError(format!("invalid port: {}", e)))?
        } else {
            default_port
        };

        return Ok(TargetAddr {
            host: TargetHost::Ip(ip),
            port,
        });
    }

    // Find the last ':' to split host and port
    let colon_pos = authority.rfind(':');

    let (host_str, port) = if let Some(colon_pos) = colon_pos {
        let host_str = &authority[..colon_pos];
        let port_str = &authority[colon_pos + 1..];
        let port: u16 = port_str
            .parse()
            .map_err(|e| HttpError::TargetParseError(format!("invalid port: {}", e)))?;
        (host_str, port)
    } else {
        (authority, default_port)
    };

    // Try to parse as IP first
    if let Ok(ip) = host_str.parse::<std::net::IpAddr>() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_absolute_uri() {
        let (target, path) = parse_absolute_uri("http://example.com:8080/path").unwrap();
        assert_eq!(
            target,
            TargetAddr {
                host: TargetHost::Domain("example.com".to_string()),
                port: 8080,
            }
        );
        assert_eq!(path, "/path");
    }

    #[test]
    fn test_parse_absolute_uri_no_path() {
        let (target, path) = parse_absolute_uri("http://example.com:80").unwrap();
        assert_eq!(
            target,
            TargetAddr {
                host: TargetHost::Domain("example.com".to_string()),
                port: 80,
            }
        );
        assert_eq!(path, "/");
    }

    #[test]
    fn test_parse_absolute_uri_ipv4() {
        let (target, path) = parse_absolute_uri("http://192.168.1.1:3000/api").unwrap();
        assert_eq!(
            target,
            TargetAddr {
                host: TargetHost::Ip("192.168.1.1".parse().unwrap()),
                port: 3000,
            }
        );
        assert_eq!(path, "/api");
    }

    #[test]
    fn test_parse_absolute_uri_no_scheme() {
        assert!(parse_absolute_uri("example.com/path").is_err());
    }

    #[test]
    fn test_parse_header_line() {
        let (name, value) = parse_header_line("Content-Type: text/html").unwrap();
        assert_eq!(name, "Content-Type");
        assert_eq!(value, "text/html");
    }

    #[test]
    fn test_parse_header_line_no_colon() {
        assert!(parse_header_line("NoColon").is_none());
    }
}
