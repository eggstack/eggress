//! HTTP-forward accept parsing: CONNECT requests, authority parsing,
//! proxy-auth challenges, and SOCKS/target conversions.

use std::net::IpAddr;

use eggress_core::{ClientIdentity, TargetAddr, TargetHost};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::auth::parse_basic_auth;

use super::{auth_credentials, cached_identity, record_authenticated};
use super::{AcceptError, InboundAuthentication};

pub(crate) struct ConnectRequest {
    pub(crate) target: TargetAddr,
    pub(crate) identity: ClientIdentity,
}

pub(crate) async fn parse_connect_request<W: AsyncWrite + Unpin>(
    head_buf: &[u8],
    stream: &mut W,
    auth: &InboundAuthentication,
    peer_ip: Option<IpAddr>,
) -> Result<ConnectRequest, AcceptError> {
    let head_str = String::from_utf8_lossy(head_buf);
    let mut lines = head_str.split("\r\n");

    let request_line = lines.next().ok_or_else(|| {
        AcceptError::Protocol(
            eggress_protocol_http::HttpError::MalformedRequest("empty request".into()).into(),
        )
    })?;

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(AcceptError::Protocol(
            eggress_protocol_http::HttpError::MalformedRequest(format!(
                "expected 3 parts in request line, got {}",
                parts.len()
            ))
            .into(),
        ));
    }

    let authority = parts[1];
    let target = parse_authority(authority)?;

    // Parse Proxy-Authorization header
    let mut proxy_auth = None;
    let mut parsed_username: Option<String> = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = parse_header_line_str(line) {
            if name.eq_ignore_ascii_case("Proxy-Authorization") {
                proxy_auth = parse_basic_auth(&value);
                if let Some((user, _)) = &proxy_auth {
                    parsed_username = Some(user.clone());
                }
            }
        }
    }

    // Validate auth if required. A compatibility cache hit is sufficient and
    // intentionally ignores credentials on the new connection, matching the
    // pproxy AuthTable behavior.
    let cached = cached_identity(auth, peer_ip);
    if cached.is_none() {
        if let Some((username, password, _)) = auth_credentials(auth) {
            match proxy_auth {
                Some((user, pass)) => {
                    use subtle::ConstantTimeEq;
                    let user_ok: bool = user.as_bytes().ct_eq(username.as_bytes()).into();
                    let pass_ok: bool = pass.as_bytes().ct_eq(password.as_bytes()).into();
                    if !user_ok || !pass_ok {
                        let _ = write_proxy_auth_required(stream).await;
                        return Err(AcceptError::AuthenticationFailed);
                    }
                }
                None => {
                    let _ = write_proxy_auth_required(stream).await;
                    return Err(AcceptError::AuthenticationFailed);
                }
            }
        }
    }

    let identity = cached.unwrap_or(match parsed_username {
        Some(user) => ClientIdentity::Username(user),
        None => ClientIdentity::Anonymous,
    });
    if matches!(identity, ClientIdentity::Username(_)) {
        record_authenticated(auth, peer_ip, &identity);
    }

    Ok(ConnectRequest { target, identity })
}

pub(crate) async fn read_http_head<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Vec<u8>, AcceptError> {
    let mut head_buf = Vec::with_capacity(1024);
    let mut line = Vec::with_capacity(256);
    let mut header_count = 0;

    loop {
        if head_buf.len() >= MAX_HEAD_SIZE {
            return Err(AcceptError::Protocol(
                eggress_protocol_http::HttpError::HeaderTooLarge.into(),
            ));
        }

        line.clear();
        let remaining = MAX_HEAD_SIZE - head_buf.len();
        let n = reader
            .take((remaining + 1) as u64)
            .read_until(b'\n', &mut line)
            .await
            .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
        if n == 0 {
            return Err(AcceptError::Protocol(
                eggress_protocol_http::HttpError::MalformedRequest(
                    "unexpected EOF reading request".into(),
                )
                .into(),
            ));
        }

        if head_buf.len() + line.len() > MAX_HEAD_SIZE {
            return Err(AcceptError::Protocol(
                eggress_protocol_http::HttpError::HeaderTooLarge.into(),
            ));
        }
        head_buf.extend_from_slice(&line);

        if line.ends_with(b"\r\n") {
            header_count += 1;
            if header_count > MAX_HEADER_LINES {
                return Err(AcceptError::Protocol(
                    eggress_protocol_http::HttpError::TooManyHeaders.into(),
                ));
            }
        }
        if head_buf.ends_with(b"\r\n\r\n") {
            return Ok(head_buf);
        }
    }
}

pub(crate) fn parse_authority(
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

        if authority
            .as_bytes()
            .get(bracket_end + 1)
            .is_none_or(|&b| b != b':')
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

pub(crate) fn socks_addr_to_target(
    addr: &eggress_protocol_socks::socks5::server::SocksAddr,
) -> TargetAddr {
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
pub(crate) const MAX_HEAD_SIZE: usize = 32 * 1024;

/// Maximum number of header lines.
pub(crate) const MAX_HEADER_LINES: usize = 128;

/// Parse a header line into (name, value).
pub(crate) fn parse_header_line_str(line: &str) -> Option<(String, String)> {
    let colon_pos = line.find(':')?;
    let name = line[..colon_pos].trim().to_string();
    let value = line[colon_pos + 1..].trim().to_string();
    Some((name, value))
}

/// Parse Basic authentication from a Proxy-Authorization header value.
/// Write a 407 Proxy Authentication Required response.
pub(crate) async fn write_proxy_auth_required<W: AsyncWrite + Unpin>(
    stream: &mut W,
) -> Result<(), std::io::Error> {
    let response = b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"eggress\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(response).await?;
    stream.flush().await?;
    Ok(())
}
