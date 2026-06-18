use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::error::Socks4Error;

/// Maximum length for the SOCKS4 user ID field.
const MAX_USER_ID_LEN: usize = 255;

/// SOCKS4 reply status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Socks4Status {
    Granted = 90,
    Failed = 91,
    FailedNoIdent = 92,
    FailedDifferentUser = 93,
}

impl Socks4Status {
    /// Convert from raw status byte.
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            90 => Some(Self::Granted),
            91 => Some(Self::Failed),
            92 => Some(Self::FailedNoIdent),
            93 => Some(Self::FailedDifferentUser),
            _ => None,
        }
    }
}

/// Parsed SOCKS4/4a CONNECT request.
#[derive(Debug, Clone)]
pub struct Socks4Request {
    pub command: u8,
    pub port: u16,
    pub addr: SocketAddr,
    pub user_id: String,
    /// For SOCKS4a: the domain name when IP is 0.0.0.x (x != 0).
    pub domain: Option<String>,
}

/// Read a SOCKS4/4a request from the stream.
///
/// Format:
///   +----+----+----+----+----+----+----+----+----+----+....+----+
///   | VN | CD | DSTPORT |      DSTIP        | USERID       |0x00|
///   +----+----+----+----+----+----+----+----+----+----+....+----+
///     1    1      2            4              variable       1
///
/// For SOCKS4a, when DSTIP is 0.0.0.x (x != 0), the domain follows
/// after the NUL-terminated USERID.
pub async fn read_socks4_request<S: tokio::io::AsyncRead + Unpin>(
    stream: &mut S,
) -> Result<Socks4Request, Socks4Error> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header).await?;

    let version = header[0];
    if version != 0x04 {
        return Err(Socks4Error::InvalidVersion(version));
    }

    let command = header[1];
    if command != 0x01 {
        return Err(Socks4Error::UnsupportedCommand(command));
    }

    let port = u16::from_be_bytes([header[2], header[3]]);
    let ip = Ipv4Addr::new(header[4], header[5], header[6], header[7]);

    // Read NUL-terminated user ID (bounded at MAX_USER_ID_LEN + 1 for NUL).
    let mut user_id_bytes = Vec::with_capacity(64);
    let mut buf = [0u8; 1];
    loop {
        if user_id_bytes.len() > MAX_USER_ID_LEN {
            return Err(Socks4Error::UserIdTooLong);
        }
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(Socks4Error::MalformedRequest(
                "unexpected EOF reading user ID".into(),
            ));
        }
        if buf[0] == 0x00 {
            break;
        }
        user_id_bytes.push(buf[0]);
    }

    let user_id = String::from_utf8_lossy(&user_id_bytes).into_owned();

    // SOCKS4a: if IP is 0.0.0.x (x != 0), read domain after user ID.
    let domain =
        if ip.octets()[0] == 0 && ip.octets()[1] == 0 && ip.octets()[2] == 0 && ip.octets()[3] != 0
        {
            let mut domain_bytes = Vec::with_capacity(256);
            loop {
                if domain_bytes.len() > 255 {
                    return Err(Socks4Error::DomainTooLong);
                }
                let n = stream.read(&mut buf).await?;
                if n == 0 {
                    return Err(Socks4Error::MalformedRequest(
                        "unexpected EOF reading domain".into(),
                    ));
                }
                if buf[0] == 0x00 {
                    break;
                }
                domain_bytes.push(buf[0]);
            }
            let domain = String::from_utf8_lossy(&domain_bytes).into_owned();
            if domain.is_empty() {
                return Err(Socks4Error::MalformedRequest(
                    "empty domain in SOCKS4a request".into(),
                ));
            }
            Some(domain)
        } else {
            None
        };

    let addr = SocketAddr::new(IpAddr::V4(ip), port);

    Ok(Socks4Request {
        command,
        port,
        addr,
        user_id,
        domain,
    })
}

/// Write a SOCKS4 reply to the stream.
///
/// Format:
///   +----+----+----+----+----+----+----+----+
///   | VN | CD | DSTPORT |      DSTIP        |
///   +----+----+----+----+----+----+----+----+
///     1    1      2            4
pub async fn write_socks4_reply<S: tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    status: Socks4Status,
    addr: SocketAddr,
) -> Result<(), Socks4Error> {
    let ip = match addr.ip() {
        IpAddr::V4(v4) => v4.octets(),
        IpAddr::V6(_) => Ipv4Addr::UNSPECIFIED.octets(),
    };
    let port = addr.port().to_be_bytes();
    let reply: [u8; 8] = [
        0x00,
        status as u8,
        port[0],
        port[1],
        ip[0],
        ip[1],
        ip[2],
        ip[3],
    ];
    stream.write_all(&reply).await?;
    stream.flush().await?;
    Ok(())
}
