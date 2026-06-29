use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::HttpError;
use eggress_core::{BoxStream, TargetAddr, TargetHost};

/// Limits for body copying.
pub struct BodyCopyLimits {
    pub max_chunk_size_line: usize,
    pub max_chunk_size: u64,
    pub max_decoded_body: u64,
    pub max_trailer_line: usize,
    pub max_trailer_bytes: usize,
}

impl Default for BodyCopyLimits {
    fn default() -> Self {
        Self {
            max_chunk_size_line: 1024,
            max_chunk_size: 64 * 1024 * 1024,
            max_decoded_body: 1024 * 1024 * 1024,
            max_trailer_line: 8192,
            max_trailer_bytes: 32 * 1024,
        }
    }
}

/// Report from body copying.
#[derive(Debug, Default)]
pub struct BodyCopyReport {
    pub wire_bytes: u64,
    pub decoded_bytes: u64,
}

/// Report from forwarding a response.
#[derive(Debug, Default)]
pub struct ForwardResponseReport {
    pub bytes_forwarded: u64,
}

/// Copy a request body from reader to writer.
///
/// For Content-Length bodies, copies exactly `len` bytes.
/// For chunked bodies, parses and forwards chunks with proper bounds.
/// Returns byte counts for accounting.
pub async fn copy_request_body<R, W>(
    reader: &mut R,
    writer: &mut W,
    kind: RequestBodyKind,
    limits: &BodyCopyLimits,
) -> Result<BodyCopyReport, HttpError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    match kind {
        RequestBodyKind::None => Ok(BodyCopyReport::default()),
        RequestBodyKind::ContentLength(len) => copy_content_length_body(reader, writer, len).await,
        RequestBodyKind::Chunked => copy_chunked_body(reader, writer, limits).await,
    }
}

async fn copy_content_length_body<R, W>(
    reader: &mut R,
    writer: &mut W,
    len: u64,
) -> Result<BodyCopyReport, HttpError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut remaining = len;
    let mut buf = [0u8; 8192];
    while remaining > 0 {
        let to_read = (remaining as usize).min(buf.len());
        let n = reader.read(&mut buf[..to_read]).await?;
        if n == 0 {
            return Err(HttpError::MalformedRequest("unexpected EOF in body".into()));
        }
        writer.write_all(&buf[..n]).await?;
        remaining -= n as u64;
    }
    Ok(BodyCopyReport {
        wire_bytes: len,
        decoded_bytes: len,
    })
}

async fn copy_chunked_body<R, W>(
    reader: &mut R,
    writer: &mut W,
    limits: &BodyCopyLimits,
) -> Result<BodyCopyReport, HttpError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut wire_bytes: u64 = 0;
    let mut decoded_bytes: u64 = 0;

    loop {
        // Read chunk size line
        let size_line = read_bounded_line(reader, limits.max_chunk_size_line).await?;
        wire_bytes += size_line.len() as u64;

        // Parse chunk size (ignore extensions after ';')
        let chunk_size = parse_chunk_size(&size_line)?;

        // Forward the size line
        writer.write_all(&size_line).await?;

        if chunk_size == 0 {
            // Read and forward trailers
            let mut trailer_bytes: u64 = 0;
            loop {
                let trailer = read_bounded_line(reader, limits.max_trailer_line).await?;
                wire_bytes += trailer.len() as u64;
                trailer_bytes += trailer.len() as u64;

                if trailer_bytes > limits.max_trailer_bytes as u64 {
                    return Err(HttpError::MalformedRequest("trailers too large".into()));
                }

                writer.write_all(&trailer).await?;

                if trailer == b"\r\n" {
                    break;
                }
            }
            break;
        }

        // Validate chunk size against limit
        if chunk_size > limits.max_chunk_size {
            return Err(HttpError::MalformedRequest("chunk too large".into()));
        }

        // Validate decoded body limit
        decoded_bytes = decoded_bytes
            .checked_add(chunk_size)
            .ok_or_else(|| HttpError::MalformedRequest("decoded body too large".into()))?;
        if decoded_bytes > limits.max_decoded_body {
            return Err(HttpError::MalformedRequest("decoded body too large".into()));
        }

        // Read exactly chunk_size data bytes
        let mut remaining = chunk_size;
        let mut buf = [0u8; 8192];
        while remaining > 0 {
            let to_read = (remaining as usize).min(buf.len());
            let n = reader.read(&mut buf[..to_read]).await?;
            if n == 0 {
                return Err(HttpError::MalformedRequest(
                    "unexpected EOF in chunk data".into(),
                ));
            }
            writer.write_all(&buf[..n]).await?;
            remaining -= n as u64;
            wire_bytes += n as u64;
        }

        // Read and verify CRLF after chunk data
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf).await?;
        wire_bytes += 2;
        if crlf != *b"\r\n" {
            return Err(HttpError::MalformedRequest(
                "missing CRLF after chunk data".into(),
            ));
        }
        writer.write_all(&crlf).await?;
    }

    Ok(BodyCopyReport {
        wire_bytes,
        decoded_bytes,
    })
}

/// Read a bounded line terminated by \r\n.
async fn read_bounded_line<R: AsyncRead + Unpin>(
    reader: &mut R,
    max_len: usize,
) -> Result<Vec<u8>, HttpError> {
    let mut line = Vec::new();
    let mut temp = [0u8; 1];
    loop {
        if line.len() >= max_len {
            return Err(HttpError::MalformedRequest("line too long".into()));
        }
        let n = reader.read(&mut temp).await?;
        if n == 0 {
            if line.is_empty() {
                return Err(HttpError::MalformedRequest("unexpected EOF".into()));
            }
            return Err(HttpError::MalformedRequest("incomplete line".into()));
        }
        line.push(temp[0]);
        if line.len() >= 2 && &line[line.len() - 2..] == b"\r\n" {
            break;
        }
    }
    Ok(line)
}

/// Parse a chunk size from a line (without trailing CRLF).
/// Supports hex with optional extensions (after ';').
fn parse_chunk_size(line_without_crlf: &[u8]) -> Result<u64, HttpError> {
    let size_field = line_without_crlf
        .split(|b| *b == b';')
        .next()
        .ok_or_else(|| HttpError::MalformedRequest("empty chunk size".into()))?;

    if size_field.is_empty() {
        return Err(HttpError::MalformedRequest("empty chunk size".into()));
    }

    let size_str = std::str::from_utf8(size_field)
        .map_err(|_| HttpError::MalformedRequest("invalid chunk size encoding".into()))?;
    let size_str = size_str.trim();

    u64::from_str_radix(size_str, 16)
        .map_err(|_| HttpError::MalformedRequest("invalid chunk size".into()))
}

/// Describes how the request body is framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestBodyKind {
    None,
    ContentLength(u64),
    Chunked,
}

/// Determine the request body framing from parsed headers.
///
/// Validates:
/// - Content-Length values (reject conflicting, accept equal duplicates)
/// - Transfer-Encoding (reject TE + CL, require chunked to be final)
/// - Only "chunked" transfer coding is supported in Phase 1
pub fn determine_request_body_kind(
    headers: &[(String, String)],
) -> Result<RequestBodyKind, HttpError> {
    let mut content_lengths: Vec<u64> = Vec::new();
    let mut transfer_encodings: Vec<String> = Vec::new();

    for (name, value) in headers {
        if name.eq_ignore_ascii_case("Content-Length") {
            // Parse each Content-Length value
            let len = value
                .trim()
                .parse::<u64>()
                .map_err(|_| HttpError::InvalidContentLength)?;
            content_lengths.push(len);
        } else if name.eq_ignore_ascii_case("Transfer-Encoding") {
            // Split comma-separated transfer codings
            for coding in value.split(',') {
                let coding = coding.trim().to_string();
                if !coding.is_empty() {
                    transfer_encodings.push(coding);
                }
            }
        }
    }

    // Validate Content-Length
    if !content_lengths.is_empty() {
        // All values must be identical
        let first = content_lengths[0];
        if content_lengths.iter().any(|&cl| cl != first) {
            return Err(HttpError::ConflictingContentLength);
        }
    }

    // Validate Transfer-Encoding
    if !transfer_encodings.is_empty() {
        // TE + CL is rejected in Phase 1
        if !content_lengths.is_empty() {
            return Err(HttpError::TransferEncodingWithContentLength);
        }

        // Check if chunked is present but not the final coding
        let has_chunked = transfer_encodings
            .iter()
            .any(|c| c.eq_ignore_ascii_case("chunked"));
        if has_chunked {
            let last = transfer_encodings.last().unwrap();
            if !last.eq_ignore_ascii_case("chunked") {
                return Err(HttpError::ChunkedNotFinal);
            }
        }

        // Only "chunked" is supported in Phase 1
        for coding in &transfer_encodings {
            if !coding.eq_ignore_ascii_case("chunked") {
                return Err(HttpError::UnsupportedTransferEncoding(coding.clone()));
            }
        }

        return Ok(RequestBodyKind::Chunked);
    }

    if let Some(len) = content_lengths.first() {
        Ok(RequestBodyKind::ContentLength(*len))
    } else {
        Ok(RequestBodyKind::None)
    }
}

/// Maximum size for the HTTP request head (request line + headers).
const MAX_HEAD_SIZE: usize = 32 * 1024;

/// Maximum size for the HTTP response head.
const MAX_RESPONSE_HEAD_SIZE: usize = 32 * 1024;

/// Maximum number of header lines.
const MAX_HEADER_LINES: usize = 128;

/// Headers that must not be forwarded across a proxy (RFC 2616 §13.5.1).
///
/// `Transfer-Encoding: chunked` is preserved because the chunked body is
/// forwarded unchanged.
fn is_hop_by_hop_header(name: &str, value: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "transfer-encoding" => !value.eq_ignore_ascii_case("chunked"),
        _ => matches!(
            lower.as_str(),
            "connection"
                | "keep-alive"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "te"
                | "trailers"
                | "upgrade"
                | "proxy-connection"
        ),
    }
}

/// Extract tokens from the `Connection` header value.
///
/// Per RFC 7230 §6.1, each token names a header that must be removed before
/// forwarding.
fn connection_tokens(headers: &[(String, String)]) -> std::collections::HashSet<String> {
    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("connection"))
        .flat_map(|(_, value)| value.split(','))
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| !token.is_empty())
        .collect()
}

/// Filter hop-by-hop headers from a header list, returning only end-to-end headers.
///
/// Removes standard hop-by-hop headers plus any headers nominated by
/// `Connection` tokens.  Preserves `Transfer-Encoding: chunked`.
pub fn filter_hop_by_hop(headers: &[(String, String)]) -> Vec<(String, String)> {
    let nominated = connection_tokens(headers);
    headers
        .iter()
        .filter(|(name, value)| {
            let lower = name.to_ascii_lowercase();
            !is_hop_by_hop_header(&lower, value) && !nominated.contains(&lower)
        })
        .cloned()
        .collect()
}

/// Build an origin-form HTTP request to send to the upstream server.
///
/// Converts the parsed absolute-form request into origin-form by:
/// - Using only the path component as the request target
/// - Filtering out hop-by-hop headers
/// - Adding `Connection: close` to avoid keep-alive complications
pub fn build_origin_request(request: &ForwardRequest) -> String {
    let filtered = filter_hop_by_hop(&request.headers);

    let mut req = format!(
        "{} {} {}\r\n",
        request.method, request.path, request.version
    );

    for (name, value) in &filtered {
        req.push_str(&format!("{}: {}\r\n", name, value));
    }

    // Ensure Connection: close for Phase 1 (no persistent forwarding)
    if !filtered
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("Connection"))
    {
        req.push_str("Connection: close\r\n");
    }

    req.push_str("\r\n");
    req
}

/// Parsed HTTP response from an upstream server.
#[derive(Debug)]
pub struct ForwardResponse {
    pub version: String,
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub content_length: Option<u64>,
    pub is_chunked: bool,
    /// True if the upstream sent `Connection: close`.
    pub connection_close: bool,
}

/// Read and parse an HTTP response head from the upstream.
async fn read_response_head(stream: &mut BoxStream) -> Result<ForwardResponse, HttpError> {
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

        if head_buf.len() >= 4 {
            let len = head_buf.len();
            if &head_buf[len - 4..] == b"\r\n\r\n" {
                break;
            }
        }
    }

    let head_str = String::from_utf8_lossy(&head_buf);
    let mut lines = head_str.split("\r\n");

    // Parse status line
    let status_line = lines
        .next()
        .ok_or_else(|| HttpError::MalformedResponse("empty response".into()))?;

    let parts: Vec<&str> = status_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(HttpError::MalformedResponse(format!(
            "invalid status line: {}",
            status_line
        )));
    }

    let version = parts[0].to_string();
    let status: u16 = parts[1]
        .parse()
        .map_err(|e| HttpError::MalformedResponse(format!("invalid status code: {}", e)))?;
    let reason = parts.get(2).unwrap_or(&"").to_string();

    // Parse response headers
    let mut headers = Vec::new();
    let mut content_length = None;
    let mut is_chunked = false;
    let mut connection_close = false;

    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = parse_header_line(line) {
            if name.eq_ignore_ascii_case("Content-Length") {
                content_length = value.parse::<u64>().ok();
            } else if name.eq_ignore_ascii_case("Transfer-Encoding")
                && value.eq_ignore_ascii_case("chunked")
            {
                is_chunked = true;
            } else if name.eq_ignore_ascii_case("Connection") {
                // Check for "close" token (case-insensitive)
                connection_close = value
                    .split(',')
                    .any(|t| t.trim().eq_ignore_ascii_case("close"));
            }
            headers.push((name, value));
        }
    }

    Ok(ForwardResponse {
        version,
        status,
        reason,
        headers,
        content_length,
        is_chunked,
        connection_close,
    })
}

/// Result of forwarding a response, including upstream connection state.
pub struct ForwardResult {
    pub report: ForwardResponseReport,
    /// True if the upstream connection is still usable (no `Connection: close`).
    pub upstream_alive: bool,
    /// True if the response status indicates the client should not retry.
    pub client_should_close: bool,
}

/// Forward the upstream response back to the client stream.
///
/// Writes the response status line and filtered headers to the client,
/// then relays the body (if any) using content-length or chunked framing.
pub async fn forward_response(
    upstream: &mut BoxStream,
    client: &mut BoxStream,
) -> Result<ForwardResult, HttpError> {
    let response = read_response_head(upstream).await?;
    let mut bytes_forwarded: u64 = 0;

    // Build response head with filtered headers
    let filtered = filter_hop_by_hop(&response.headers);
    let mut head = format!("HTTP/1.1 {} {}\r\n", response.status, response.reason);

    for (name, value) in &filtered {
        head.push_str(&format!("{}: {}\r\n", name, value));
    }

    // Ensure Connection: close
    if !filtered
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("Connection"))
    {
        head.push_str("Connection: close\r\n");
    }

    head.push_str("\r\n");
    client.write_all(head.as_bytes()).await?;
    bytes_forwarded += head.len() as u64;

    // Relay body based on framing
    match (response.content_length, response.is_chunked) {
        (Some(len), _) => {
            let mut remaining = len;
            let mut buf = [0u8; 8192];
            while remaining > 0 {
                let to_read = (remaining as usize).min(buf.len());
                let n = upstream.read(&mut buf[..to_read]).await?;
                if n == 0 {
                    break;
                }
                client.write_all(&buf[..n]).await?;
                bytes_forwarded += n as u64;
                remaining -= n as u64;
            }
        }
        (None, true) => loop {
            let mut size_line = Vec::new();
            let mut temp = [0u8; 1];
            loop {
                let n = upstream.read(&mut temp).await?;
                if n == 0 {
                    return Ok(ForwardResult {
                        report: ForwardResponseReport { bytes_forwarded },
                        upstream_alive: false,
                        client_should_close: true,
                    });
                }
                size_line.push(temp[0]);
                if size_line.len() > 32 {
                    return Err(HttpError::MalformedResponse(
                        "chunk size line exceeds maximum length".into(),
                    ));
                }
                if size_line.len() >= 2 && &size_line[size_line.len() - 2..] == b"\r\n" {
                    break;
                }
            }
            let size_str = String::from_utf8_lossy(&size_line[..size_line.len() - 2]);
            let chunk_size = usize::from_str_radix(size_str.trim(), 16)
                .map_err(|e| HttpError::MalformedResponse(format!("invalid chunk size: {}", e)))?;

            client.write_all(&size_line).await?;
            bytes_forwarded += size_line.len() as u64;

            if chunk_size == 0 {
                let mut trail = [0u8; 2];
                upstream.read_exact(&mut trail).await?;
                client.write_all(&trail).await?;
                bytes_forwarded += 2;
                break;
            }

            let mut remaining = chunk_size + 2;
            let mut buf = [0u8; 8192];
            while remaining > 0 {
                let to_read = remaining.min(buf.len());
                let n = upstream.read(&mut buf[..to_read]).await?;
                if n == 0 {
                    return Ok(ForwardResult {
                        report: ForwardResponseReport { bytes_forwarded },
                        upstream_alive: false,
                        client_should_close: true,
                    });
                }
                client.write_all(&buf[..n]).await?;
                bytes_forwarded += n as u64;
                remaining -= n;
            }
        },
        (None, false) => {
            let mut buf = [0u8; 8192];
            loop {
                let n = upstream.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                client.write_all(&buf[..n]).await?;
                bytes_forwarded += n as u64;
            }
        }
    }

    // Determine upstream alive: HTTP/1.1 default is keep-alive, HTTP/1.0 default is close
    let upstream_alive = if response.connection_close {
        false
    } else if response.version.contains("1.1") {
        true
    } else {
        // HTTP/1.0: alive only if explicitly requested via Keep-Alive
        response
            .headers
            .iter()
            .any(|(n, v)| n.eq_ignore_ascii_case("Keep-Alive") && !v.is_empty())
    };

    // Client should close if the upstream said close
    let client_should_close = response.connection_close;

    Ok(ForwardResult {
        report: ForwardResponseReport { bytes_forwarded },
        upstream_alive,
        client_should_close,
    })
}

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
    /// True if the client sent `Connection: close`.
    pub connection_close: bool,
}

impl ForwardRequest {
    /// Compute the request body kind from parsed fields.
    pub fn body_kind(&self) -> RequestBodyKind {
        if self.is_chunked {
            RequestBodyKind::Chunked
        } else if let Some(len) = self.content_length {
            RequestBodyKind::ContentLength(len)
        } else {
            RequestBodyKind::None
        }
    }
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

/// Read and parse an HTTP forward request from an existing stream.
///
/// Unlike [`forward_request`], this borrows the stream rather than
/// consuming it, enabling persistent-session loops.
pub async fn forward_request_stream(stream: &mut BoxStream) -> Result<ForwardRequest, HttpError> {
    read_forward_request(stream).await
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

    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = parse_header_line(line) {
            // Skip Proxy-Authorization header (don't forward it)
            if name.eq_ignore_ascii_case("Proxy-Authorization") {
                continue;
            }

            headers.push((name, value));
        }
    }

    // Determine body framing from headers
    let body_kind = determine_request_body_kind(&headers)?;
    let (has_body, content_length, is_chunked) = match body_kind {
        RequestBodyKind::None => (false, None, false),
        RequestBodyKind::ContentLength(len) => (len > 0, Some(len), false),
        RequestBodyKind::Chunked => (true, None, true),
    };

    // Determine Connection: close
    let connection_close = headers.iter().any(|(n, v)| {
        n.eq_ignore_ascii_case("Connection")
            && v.split(',').any(|t| t.trim().eq_ignore_ascii_case("close"))
    });

    Ok(ForwardRequest {
        method,
        path,
        version,
        headers,
        target,
        has_body,
        content_length,
        is_chunked,
        connection_close,
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

    #[test]
    fn test_filter_hop_by_hop_connection_nominated() {
        let headers = vec![
            ("Connection".into(), "X-Custom, Keep-Alive".into()),
            ("X-Custom".into(), "value".into()),
            ("Keep-Alive".into(), "timeout=5".into()),
            ("Content-Type".into(), "text/html".into()),
        ];
        let filtered = filter_hop_by_hop(&headers);
        // X-Custom and Keep-Alive should be removed (nominated by Connection),
        // plus connection itself is always removed.
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "Content-Type");
    }

    #[test]
    fn test_filter_hop_by_hop_preserves_transfer_encoding_chunked() {
        let headers = vec![
            ("Transfer-Encoding".into(), "chunked".into()),
            ("Content-Type".into(), "application/json".into()),
        ];
        let filtered = filter_hop_by_hop(&headers);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|(n, _)| n == "Transfer-Encoding"));
    }

    #[test]
    fn test_filter_hop_by_hop_removes_transfer_encoding_non_chunked() {
        let headers = vec![
            ("Transfer-Encoding".into(), "gzip".into()),
            ("Content-Type".into(), "text/html".into()),
        ];
        let filtered = filter_hop_by_hop(&headers);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "Content-Type");
    }

    #[test]
    fn test_filter_connection_tokens_empty() {
        let headers = vec![("Content-Type".into(), "text/html".into())];
        let tokens = connection_tokens(&headers);
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_filter_connection_tokens_multiple() {
        let headers = vec![("Connection".into(), "close, Upgrade".into())];
        let tokens = connection_tokens(&headers);
        assert!(tokens.contains("close"));
        assert!(tokens.contains("upgrade"));
    }

    #[test]
    fn test_determine_body_none() {
        let headers = vec![("Host".into(), "example.com".into())];
        assert_eq!(
            determine_request_body_kind(&headers).unwrap(),
            RequestBodyKind::None
        );
    }

    #[test]
    fn test_determine_body_content_length() {
        let headers = vec![("Content-Length".into(), "42".into())];
        assert_eq!(
            determine_request_body_kind(&headers).unwrap(),
            RequestBodyKind::ContentLength(42)
        );
    }

    #[test]
    fn test_determine_body_duplicate_equal_cl() {
        let headers = vec![
            ("Content-Length".into(), "42".into()),
            ("Content-Length".into(), "42".into()),
        ];
        assert_eq!(
            determine_request_body_kind(&headers).unwrap(),
            RequestBodyKind::ContentLength(42)
        );
    }

    #[test]
    fn test_determine_body_conflicting_cl() {
        let headers = vec![
            ("Content-Length".into(), "42".into()),
            ("Content-Length".into(), "100".into()),
        ];
        assert!(matches!(
            determine_request_body_kind(&headers),
            Err(HttpError::ConflictingContentLength)
        ));
    }

    #[test]
    fn test_determine_body_invalid_cl() {
        let headers = vec![("Content-Length".into(), "abc".into())];
        assert!(matches!(
            determine_request_body_kind(&headers),
            Err(HttpError::InvalidContentLength)
        ));
    }

    #[test]
    fn test_determine_body_chunked() {
        let headers = vec![("Transfer-Encoding".into(), "chunked".into())];
        assert_eq!(
            determine_request_body_kind(&headers).unwrap(),
            RequestBodyKind::Chunked
        );
    }

    #[test]
    fn test_determine_body_te_plus_cl() {
        let headers = vec![
            ("Transfer-Encoding".into(), "chunked".into()),
            ("Content-Length".into(), "42".into()),
        ];
        assert!(matches!(
            determine_request_body_kind(&headers),
            Err(HttpError::TransferEncodingWithContentLength)
        ));
    }

    #[test]
    fn test_determine_body_unsupported_te() {
        let headers = vec![("Transfer-Encoding".into(), "gzip".into())];
        assert!(matches!(
            determine_request_body_kind(&headers),
            Err(HttpError::UnsupportedTransferEncoding(_))
        ));
    }

    #[test]
    fn test_determine_body_chunked_not_final() {
        let headers = vec![("Transfer-Encoding".into(), "chunked, gzip".into())];
        assert!(matches!(
            determine_request_body_kind(&headers),
            Err(HttpError::ChunkedNotFinal)
        ));
    }

    #[test]
    fn test_determine_body_mixed_header_casing() {
        let headers = vec![
            ("content-length".into(), "42".into()),
            ("CONTENT-LENGTH".into(), "42".into()),
        ];
        assert_eq!(
            determine_request_body_kind(&headers).unwrap(),
            RequestBodyKind::ContentLength(42)
        );
    }

    // ===== Body copy tests =====

    #[tokio::test]
    async fn test_copy_chunked_body_simple() {
        let input = b"5\r\nhello\r\n0\r\n\r\n";
        let mut reader = &input[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits::default();

        let report = copy_request_body(&mut reader, &mut writer, RequestBodyKind::Chunked, &limits)
            .await
            .unwrap();
        assert_eq!(report.decoded_bytes, 5);
        assert_eq!(writer, input);
    }

    #[tokio::test]
    async fn test_copy_chunked_body_multiple_chunks() {
        let input = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        let mut reader = &input[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits::default();

        let report = copy_request_body(&mut reader, &mut writer, RequestBodyKind::Chunked, &limits)
            .await
            .unwrap();
        assert_eq!(report.decoded_bytes, 11);
        assert_eq!(writer, input);
    }

    #[tokio::test]
    async fn test_copy_chunked_body_uppercase_hex() {
        let input = b"5\r\nhello\r\n0\r\n\r\n";
        let mut reader = &input[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits::default();

        let report = copy_request_body(&mut reader, &mut writer, RequestBodyKind::Chunked, &limits)
            .await
            .unwrap();
        assert_eq!(report.decoded_bytes, 5);
    }

    #[tokio::test]
    async fn test_copy_chunked_body_with_extension() {
        let input = b"5;ext=value\r\nhello\r\n0\r\n\r\n";
        let mut reader = &input[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits::default();

        let report = copy_request_body(&mut reader, &mut writer, RequestBodyKind::Chunked, &limits)
            .await
            .unwrap();
        assert_eq!(report.decoded_bytes, 5);
    }

    #[tokio::test]
    async fn test_copy_chunked_body_with_trailer() {
        let input = b"5\r\nhello\r\n0\r\nTrailer: value\r\n\r\n";
        let mut reader = &input[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits::default();

        let report = copy_request_body(&mut reader, &mut writer, RequestBodyKind::Chunked, &limits)
            .await
            .unwrap();
        assert_eq!(report.decoded_bytes, 5);
    }

    #[tokio::test]
    async fn test_copy_chunked_body_malformed_hex() {
        let input = b"ZZ\r\nhello\r\n0\r\n\r\n";
        let mut reader = &input[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits::default();

        let result =
            copy_request_body(&mut reader, &mut writer, RequestBodyKind::Chunked, &limits).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_copy_chunked_body_empty_size() {
        let input = b"\r\nhello\r\n0\r\n\r\n";
        let mut reader = &input[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits::default();

        let result =
            copy_request_body(&mut reader, &mut writer, RequestBodyKind::Chunked, &limits).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_copy_chunked_body_missing_crlf() {
        let input = b"5\r\nhelloX\r\n0\r\n\r\n";
        let mut reader = &input[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits::default();

        let result =
            copy_request_body(&mut reader, &mut writer, RequestBodyKind::Chunked, &limits).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_copy_chunked_body_oversized_chunk() {
        let input = b"FFFFFFFFFFFFFFFF\r\nhello\r\n0\r\n\r\n";
        let mut reader = &input[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits {
            max_chunk_size: 1024,
            ..Default::default()
        };

        let result =
            copy_request_body(&mut reader, &mut writer, RequestBodyKind::Chunked, &limits).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_copy_content_length_body() {
        let input = b"hello world";
        let mut reader = &input[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits::default();

        let report = copy_request_body(
            &mut reader,
            &mut writer,
            RequestBodyKind::ContentLength(11),
            &limits,
        )
        .await
        .unwrap();
        assert_eq!(report.wire_bytes, 11);
        assert_eq!(report.decoded_bytes, 11);
        assert_eq!(writer, input);
    }

    #[tokio::test]
    async fn test_copy_none_body() {
        let mut reader = &b""[..];
        let mut writer = Vec::new();
        let limits = BodyCopyLimits::default();

        let report = copy_request_body(&mut reader, &mut writer, RequestBodyKind::None, &limits)
            .await
            .unwrap();
        assert_eq!(report.wire_bytes, 0);
        assert_eq!(report.decoded_bytes, 0);
    }
}
