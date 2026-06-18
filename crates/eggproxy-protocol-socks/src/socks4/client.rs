use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::error::Socks4Error;
use super::server::Socks4Status;
use eggproxy_core::{BoxStream, TargetAddr, TargetHost};

/// Maximum length for the SOCKS4 user ID field.
const MAX_USER_ID_LEN: usize = 255;

/// Send a SOCKS4/4a CONNECT request through a stream and return the
/// upgraded stream on success.
///
/// For IP targets, a standard SOCKS4 CONNECT request is sent.
/// For domain targets, a SOCKS4a request is sent with IP 0.0.0.x
/// and the domain appended after the NUL-terminated user ID.
pub async fn socks4_connect(
    mut stream: BoxStream,
    target: &TargetAddr,
    user_id: Option<&str>,
) -> Result<BoxStream, Socks4Error> {
    if let Some(uid) = user_id {
        if uid.len() > MAX_USER_ID_LEN {
            return Err(Socks4Error::UserIdTooLong);
        }
    }

    let mut request = Vec::with_capacity(64);
    // Version
    request.push(0x04);
    // Command: CONNECT
    request.push(0x01);
    // Port (big-endian)
    request.extend_from_slice(&target.port.to_be_bytes());

    match &target.host {
        TargetHost::Ip(IpAddr::V4(ip)) => {
            request.extend_from_slice(&ip.octets());
        }
        TargetHost::Ip(IpAddr::V6(_)) => {
            // IPv6 not supported in SOCKS4; use 0.0.0.1 for SOCKS4a-style
            request.extend_from_slice(&[0, 0, 0, 1]);
        }
        TargetHost::Domain(_) => {
            // SOCKS4a: IP = 0.0.0.1, domain appended after user ID
            request.extend_from_slice(&[0, 0, 0, 1]);
        }
    }

    // User ID (NUL-terminated)
    if let Some(uid) = user_id {
        request.extend_from_slice(uid.as_bytes());
    }
    request.push(0x00);

    // For SOCKS4a domain targets, append the domain NUL-terminated.
    if let TargetHost::Domain(domain) = &target.host {
        request.extend_from_slice(domain.as_bytes());
        request.push(0x00);
    }

    stream.write_all(&request).await?;

    // Read reply (8 bytes).
    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply).await?;

    let version = reply[0];
    if version != 0x00 {
        return Err(Socks4Error::InvalidVersion(version));
    }

    let status = reply[1];
    let port = u16::from_be_bytes([reply[2], reply[3]]);
    let ip = Ipv4Addr::new(reply[4], reply[5], reply[6], reply[7]);
    let _bound_addr = SocketAddr::new(IpAddr::V4(ip), port);

    match Socks4Status::from_u8(status) {
        Some(Socks4Status::Granted) => Ok(stream),
        Some(Socks4Status::Failed) => Err(Socks4Error::ConnectionFailed),
        Some(Socks4Status::FailedNoIdent) => Err(Socks4Error::FailedNoIdent),
        Some(Socks4Status::FailedDifferentUser) => Err(Socks4Error::FailedDifferentUser),
        None => Err(Socks4Error::UnknownStatus(status)),
    }
}
