use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use eggress_core::UpstreamId;
use eggress_protocol_socks::socks5::server::{SocksAddr, ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6};
use eggress_uri::ProxyHopSpec;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::time;
use tokio_util::sync::CancellationToken;

const SOCKS5_VERSION: u8 = 0x05;
const AUTH_NONE: u8 = 0x00;
const AUTH_USERNAME_PASSWORD: u8 = 0x02;
const AUTH_VERSION: u8 = 0x01;
const CMD_UDP_ASSOCIATE: u8 = 0x03;

#[derive(Debug, thiserror::Error)]
pub enum UdpUpstreamError {
    #[error("unsupported protocol")]
    UnsupportedProtocol,
    #[error("unsupported multi-hop chain")]
    UnsupportedMultiHop,
    #[error("TCP connect failed: {0}")]
    TcpConnect(#[source] std::io::Error),
    #[error("SOCKS5 method negotiation rejected")]
    SocksMethodRejected,
    #[error("SOCKS5 authentication failed")]
    SocksAuthFailed,
    #[error("SOCKS5 UDP ASSOCIATE rejected: {0}")]
    SocksAssociateRejected(u8),
    #[error("malformed SOCKS5 reply")]
    MalformedSocksReply,
    #[error("UDP relay address invalid")]
    UdpRelayAddressInvalid,
    #[error("handshake timed out")]
    Timeout,
    #[error("I/O error: {0}")]
    Io(#[source] std::io::Error),
}

impl UdpUpstreamError {
    pub fn reason_label(&self) -> &'static str {
        match self {
            Self::UnsupportedProtocol => "unsupported_protocol",
            Self::UnsupportedMultiHop => "unsupported_multi_hop",
            Self::TcpConnect(_) => "tcp_connect",
            Self::SocksMethodRejected => "method_rejected",
            Self::SocksAuthFailed => "auth_failed",
            Self::SocksAssociateRejected(_) => "associate_rejected",
            Self::MalformedSocksReply => "malformed_reply",
            Self::UdpRelayAddressInvalid => "bad_relay_addr",
            Self::Timeout => "timeout",
            Self::Io(_) => "io",
        }
    }
}

pub struct Socks5UdpUpstreamConfig {
    pub upstream_id: UpstreamId,
    pub hop: ProxyHopSpec,
    pub connect_timeout: Duration,
    pub udp_bind: SocketAddr,
}

pub struct Socks5UdpUpstreamAssociation {
    pub upstream_id: UpstreamId,
    pub relay_addr: SocketAddr,
    pub control_task: JoinHandle<()>,
    pub control_cancel: CancellationToken,
    pub udp_socket: Arc<tokio::net::UdpSocket>,
}

pub async fn open_socks5_udp_upstream(
    config: Socks5UdpUpstreamConfig,
    target_hint: Option<SocksAddr>,
) -> Result<Socks5UdpUpstreamAssociation, UdpUpstreamError> {
    if config.hop.protocols.len() > 1 {
        return Err(UdpUpstreamError::UnsupportedMultiHop);
    }

    let has_socks5 = config
        .hop
        .protocols
        .contains(&eggress_uri::ProtocolSpec::Socks5);
    if !has_socks5 {
        return Err(UdpUpstreamError::UnsupportedProtocol);
    }

    let endpoint = &config.hop.endpoint;
    let connect_addr: SocketAddr = resolve_endpoint(endpoint)
        .await
        .map_err(UdpUpstreamError::TcpConnect)?;

    let tcp_stream = time::timeout(config.connect_timeout, TcpStream::connect(connect_addr))
        .await
        .map_err(|_| UdpUpstreamError::Timeout)?
        .map_err(UdpUpstreamError::TcpConnect)?;

    let mut stream = tcp_stream;
    stream.set_nodelay(true).map_err(UdpUpstreamError::Io)?;

    let auth = config
        .hop
        .credentials
        .as_ref()
        .map(|c| (c.username.as_str(), c.password.as_str()));
    socks5_method_negotiate(&mut stream, auth).await?;
    if let Some((username, password)) = auth {
        socks5_auth(&mut stream, username, password).await?;
    }

    let hint = target_hint.unwrap_or(SocksAddr::IPv4([0, 0, 0, 0], 0));
    let relay_addr = socks5_udp_associate(&mut stream, &hint).await?;

    let relay_addr = if is_unspecified(&relay_addr) {
        let peer_ip = stream.peer_addr().map_err(UdpUpstreamError::Io)?.ip();
        SocketAddr::new(peer_ip, relay_addr.port())
    } else {
        relay_addr
    };

    let udp_socket = tokio::net::UdpSocket::bind(config.udp_bind)
        .await
        .map_err(UdpUpstreamError::Io)?;
    let udp_socket = Arc::new(udp_socket);

    let control_cancel = CancellationToken::new();
    let control_task_cancel = control_cancel.clone();
    let control_task = tokio::spawn(async move {
        let mut buf = [0u8; 1];
        let _ = tokio::time::timeout(Duration::from_secs(300), stream.read_exact(&mut buf)).await;
        control_task_cancel.cancel();
    });

    Ok(Socks5UdpUpstreamAssociation {
        upstream_id: config.upstream_id,
        relay_addr,
        control_task,
        control_cancel,
        udp_socket,
    })
}

async fn resolve_endpoint(
    endpoint: &eggress_uri::EndpointSpec,
) -> Result<SocketAddr, std::io::Error> {
    let host = if endpoint.host.is_empty() {
        "127.0.0.1"
    } else {
        &endpoint.host
    };

    let socket_addr = tokio::net::lookup_host(format!("{}:{}", host, endpoint.port))
        .await?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no addresses found"))?;

    Ok(socket_addr)
}

fn is_unspecified(addr: &SocketAddr) -> bool {
    match addr {
        SocketAddr::V4(v4) => {
            let ip = v4.ip();
            *ip == Ipv4Addr::UNSPECIFIED || *ip == Ipv4Addr::new(0, 0, 0, 0)
        }
        SocketAddr::V6(v6) => {
            let ip = v6.ip();
            *ip == Ipv6Addr::UNSPECIFIED
        }
    }
}

async fn socks5_method_negotiate(
    stream: &mut TcpStream,
    auth: Option<(&str, &str)>,
) -> Result<(), UdpUpstreamError> {
    let mut methods = vec![AUTH_NONE];
    if auth.is_some() {
        methods.push(AUTH_USERNAME_PASSWORD);
    }

    let mut buf = Vec::with_capacity(2 + methods.len());
    buf.push(SOCKS5_VERSION);
    buf.push(methods.len() as u8);
    buf.extend_from_slice(&methods);
    stream.write_all(&buf).await.map_err(UdpUpstreamError::Io)?;
    stream.flush().await.map_err(UdpUpstreamError::Io)?;

    let version = stream.read_u8().await.map_err(UdpUpstreamError::Io)?;
    if version != SOCKS5_VERSION {
        return Err(UdpUpstreamError::SocksMethodRejected);
    }
    let selected = stream.read_u8().await.map_err(UdpUpstreamError::Io)?;
    match selected {
        AUTH_NONE => Ok(()),
        AUTH_USERNAME_PASSWORD if auth.is_some() => Ok(()),
        0xFF => Err(UdpUpstreamError::SocksMethodRejected),
        _ => Err(UdpUpstreamError::SocksMethodRejected),
    }
}

async fn socks5_auth(
    stream: &mut TcpStream,
    username: &str,
    password: &str,
) -> Result<(), UdpUpstreamError> {
    let mut buf = Vec::with_capacity(3 + username.len() + password.len());
    buf.push(AUTH_VERSION);
    buf.push(username.len() as u8);
    buf.extend_from_slice(username.as_bytes());
    buf.push(password.len() as u8);
    buf.extend_from_slice(password.as_bytes());
    stream.write_all(&buf).await.map_err(UdpUpstreamError::Io)?;
    stream.flush().await.map_err(UdpUpstreamError::Io)?;

    let version = stream.read_u8().await.map_err(UdpUpstreamError::Io)?;
    if version != AUTH_VERSION {
        return Err(UdpUpstreamError::SocksAuthFailed);
    }
    let status = stream.read_u8().await.map_err(UdpUpstreamError::Io)?;
    if status != 0x00 {
        return Err(UdpUpstreamError::SocksAuthFailed);
    }
    Ok(())
}

async fn socks5_udp_associate(
    stream: &mut TcpStream,
    target: &SocksAddr,
) -> Result<SocketAddr, UdpUpstreamError> {
    let mut buf = Vec::with_capacity(32);
    buf.push(SOCKS5_VERSION);
    buf.push(CMD_UDP_ASSOCIATE);
    buf.push(0x00);
    encode_socks_addr(target, &mut buf);
    stream.write_all(&buf).await.map_err(UdpUpstreamError::Io)?;
    stream.flush().await.map_err(UdpUpstreamError::Io)?;

    let version = stream.read_u8().await.map_err(UdpUpstreamError::Io)?;
    if version != SOCKS5_VERSION {
        return Err(UdpUpstreamError::MalformedSocksReply);
    }
    let rep = stream.read_u8().await.map_err(UdpUpstreamError::Io)?;
    if rep != 0x00 {
        return Err(UdpUpstreamError::SocksAssociateRejected(rep));
    }
    let _rsv = stream.read_u8().await.map_err(UdpUpstreamError::Io)?;

    let atyp = stream.read_u8().await.map_err(UdpUpstreamError::Io)?;
    let addr = match atyp {
        ATYP_IPV4 => {
            let mut addr = [0u8; 4];
            stream
                .read_exact(&mut addr)
                .await
                .map_err(UdpUpstreamError::Io)?;
            let port = stream.read_u16().await.map_err(UdpUpstreamError::Io)?;
            SocketAddr::new(IpAddr::V4(Ipv4Addr::from(addr)), port)
        }
        ATYP_IPV6 => {
            let mut addr = [0u8; 16];
            stream
                .read_exact(&mut addr)
                .await
                .map_err(UdpUpstreamError::Io)?;
            let port = stream.read_u16().await.map_err(UdpUpstreamError::Io)?;
            SocketAddr::new(IpAddr::V6(Ipv6Addr::from(addr)), port)
        }
        _ => return Err(UdpUpstreamError::UdpRelayAddressInvalid),
    };

    Ok(addr)
}

fn encode_socks_addr(addr: &SocksAddr, buf: &mut Vec<u8>) {
    match addr {
        SocksAddr::IPv4(addr, port) => {
            buf.push(ATYP_IPV4);
            buf.extend_from_slice(addr);
            buf.extend_from_slice(&port.to_be_bytes());
        }
        SocksAddr::Domain(domain, port) => {
            buf.push(ATYP_DOMAIN);
            buf.push(domain.len() as u8);
            buf.extend_from_slice(domain.as_bytes());
            buf.extend_from_slice(&port.to_be_bytes());
        }
        SocksAddr::IPv6(addr, port) => {
            buf.push(ATYP_IPV6);
            buf.extend_from_slice(addr);
            buf.extend_from_slice(&port.to_be_bytes());
        }
    }
}

pub fn encode_method_negotiation(methods: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(2 + methods.len());
    buf.push(SOCKS5_VERSION);
    buf.push(methods.len() as u8);
    buf.extend_from_slice(methods);
    buf
}

pub fn encode_auth_request(username: &str, password: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(3 + username.len() + password.len());
    buf.push(AUTH_VERSION);
    buf.push(username.len() as u8);
    buf.extend_from_slice(username.as_bytes());
    buf.push(password.len() as u8);
    buf.extend_from_slice(password.as_bytes());
    buf
}

pub fn encode_udp_associate_request(target: &SocksAddr) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32);
    buf.push(SOCKS5_VERSION);
    buf.push(CMD_UDP_ASSOCIATE);
    buf.push(0x00);
    encode_socks_addr(target, &mut buf);
    buf
}

pub fn decode_method_selection(buf: &[u8]) -> Result<u8, UdpUpstreamError> {
    if buf.len() < 2 {
        return Err(UdpUpstreamError::MalformedSocksReply);
    }
    if buf[0] != SOCKS5_VERSION {
        return Err(UdpUpstreamError::MalformedSocksReply);
    }
    Ok(buf[1])
}

pub fn decode_auth_response(buf: &[u8]) -> Result<(), UdpUpstreamError> {
    if buf.len() < 2 {
        return Err(UdpUpstreamError::MalformedSocksReply);
    }
    if buf[0] != AUTH_VERSION {
        return Err(UdpUpstreamError::MalformedSocksReply);
    }
    if buf[1] != 0x00 {
        return Err(UdpUpstreamError::SocksAuthFailed);
    }
    Ok(())
}

pub fn decode_udp_associate_reply(buf: &[u8]) -> Result<SocketAddr, UdpUpstreamError> {
    if buf.len() < 4 {
        return Err(UdpUpstreamError::MalformedSocksReply);
    }
    if buf[0] != SOCKS5_VERSION {
        return Err(UdpUpstreamError::MalformedSocksReply);
    }
    let rep = buf[1];
    if rep != 0x00 {
        return Err(UdpUpstreamError::SocksAssociateRejected(rep));
    }
    let atyp = buf[3];
    match atyp {
        ATYP_IPV4 => {
            if buf.len() < 10 {
                return Err(UdpUpstreamError::MalformedSocksReply);
            }
            let mut addr = [0u8; 4];
            addr.copy_from_slice(&buf[4..8]);
            let port = u16::from_be_bytes([buf[8], buf[9]]);
            Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(addr)), port))
        }
        ATYP_IPV6 => {
            if buf.len() < 22 {
                return Err(UdpUpstreamError::MalformedSocksReply);
            }
            let mut addr = [0u8; 16];
            addr.copy_from_slice(&buf[4..20]);
            let port = u16::from_be_bytes([buf[20], buf[21]]);
            Ok(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(addr)), port))
        }
        _ => Err(UdpUpstreamError::UdpRelayAddressInvalid),
    }
}

pub fn substitute_unspecified(addr: SocketAddr, peer_ip: IpAddr) -> SocketAddr {
    if is_unspecified(&addr) {
        SocketAddr::new(peer_ip, addr.port())
    } else {
        addr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn encode_method_negotiation_no_auth() {
        let buf = encode_method_negotiation(&[AUTH_NONE]);
        assert_eq!(buf, vec![SOCKS5_VERSION, 1, AUTH_NONE]);
    }

    #[test]
    fn encode_method_negotiation_with_auth() {
        let buf = encode_method_negotiation(&[AUTH_NONE, AUTH_USERNAME_PASSWORD]);
        assert_eq!(
            buf,
            vec![SOCKS5_VERSION, 2, AUTH_NONE, AUTH_USERNAME_PASSWORD]
        );
    }

    #[test]
    fn encode_auth_request_wire_format() {
        let buf = encode_auth_request("user", "pass");
        let expected = vec![
            AUTH_VERSION,
            4,
            b'u',
            b's',
            b'e',
            b'r',
            4,
            b'p',
            b'a',
            b's',
            b's',
        ];
        assert_eq!(buf, expected);
    }

    #[test]
    fn encode_udp_associate_request_ipv4() {
        let target = SocksAddr::IPv4([192, 168, 1, 1], 8080);
        let buf = encode_udp_associate_request(&target);
        assert_eq!(buf[0], SOCKS5_VERSION);
        assert_eq!(buf[1], CMD_UDP_ASSOCIATE);
        assert_eq!(buf[2], 0x00);
        assert_eq!(buf[3], ATYP_IPV4);
        assert_eq!(&buf[4..8], &[192, 168, 1, 1]);
        assert_eq!(&buf[8..10], &8080u16.to_be_bytes());
    }

    #[test]
    fn encode_udp_associate_request_domain() {
        let target = SocksAddr::Domain("example.com".to_string(), 443);
        let buf = encode_udp_associate_request(&target);
        assert_eq!(buf[0], SOCKS5_VERSION);
        assert_eq!(buf[1], CMD_UDP_ASSOCIATE);
        assert_eq!(buf[3], ATYP_DOMAIN);
        assert_eq!(buf[4], 11);
        assert_eq!(&buf[5..16], b"example.com");
        assert_eq!(&buf[16..18], &443u16.to_be_bytes());
    }

    #[test]
    fn decode_method_selection_success() {
        let buf = [SOCKS5_VERSION, AUTH_NONE];
        assert_eq!(decode_method_selection(&buf).unwrap(), AUTH_NONE);
    }

    #[test]
    fn decode_method_selection_rejected() {
        let buf = [SOCKS5_VERSION, 0xFF];
        assert_eq!(decode_method_selection(&buf).unwrap(), 0xFF);
    }

    #[test]
    fn decode_method_selection_bad_version() {
        let buf = [0x04, AUTH_NONE];
        assert!(matches!(
            decode_method_selection(&buf),
            Err(UdpUpstreamError::MalformedSocksReply)
        ));
    }

    #[test]
    fn decode_method_selection_too_short() {
        let buf = [SOCKS5_VERSION];
        assert!(matches!(
            decode_method_selection(&buf),
            Err(UdpUpstreamError::MalformedSocksReply)
        ));
    }

    #[test]
    fn decode_auth_response_success() {
        let buf = [AUTH_VERSION, 0x00];
        assert!(decode_auth_response(&buf).is_ok());
    }

    #[test]
    fn decode_auth_response_failure() {
        let buf = [AUTH_VERSION, 0x01];
        assert!(matches!(
            decode_auth_response(&buf),
            Err(UdpUpstreamError::SocksAuthFailed)
        ));
    }

    #[test]
    fn decode_auth_response_bad_version() {
        let buf = [0x02, 0x00];
        assert!(matches!(
            decode_auth_response(&buf),
            Err(UdpUpstreamError::MalformedSocksReply)
        ));
    }

    #[test]
    fn decode_udp_associate_reply_success_ipv4() {
        let mut buf = vec![SOCKS5_VERSION, 0x00, 0x00, ATYP_IPV4];
        buf.extend_from_slice(&[10, 0, 0, 1]);
        buf.extend_from_slice(&9090u16.to_be_bytes());
        let addr = decode_udp_associate_reply(&buf).unwrap();
        assert_eq!(
            addr,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 9090)
        );
    }

    #[test]
    fn decode_udp_associate_reply_success_ipv6() {
        let mut buf = vec![SOCKS5_VERSION, 0x00, 0x00, ATYP_IPV6];
        buf.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        buf.extend_from_slice(&53u16.to_be_bytes());
        let addr = decode_udp_associate_reply(&buf).unwrap();
        assert_eq!(addr, SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 53));
    }

    #[test]
    fn decode_udp_associate_reply_failure() {
        let mut buf = vec![SOCKS5_VERSION, 0x01, 0x00, ATYP_IPV4];
        buf.extend_from_slice(&[0, 0, 0, 0]);
        buf.extend_from_slice(&0u16.to_be_bytes());
        assert!(matches!(
            decode_udp_associate_reply(&buf),
            Err(UdpUpstreamError::SocksAssociateRejected(0x01))
        ));
    }

    #[test]
    fn decode_udp_associate_reply_bad_version() {
        let mut buf = vec![0x04, 0x00, 0x00, ATYP_IPV4];
        buf.extend_from_slice(&[0, 0, 0, 0]);
        buf.extend_from_slice(&0u16.to_be_bytes());
        assert!(matches!(
            decode_udp_associate_reply(&buf),
            Err(UdpUpstreamError::MalformedSocksReply)
        ));
    }

    #[test]
    fn decode_udp_associate_reply_unsupported_atyp() {
        let buf = vec![
            SOCKS5_VERSION,
            0x00,
            0x00,
            0x05,
            0x01,
            0x02,
            0x03,
            0x04,
            0x00,
            0x50,
        ];
        assert!(matches!(
            decode_udp_associate_reply(&buf),
            Err(UdpUpstreamError::UdpRelayAddressInvalid)
        ));
    }

    #[test]
    fn substitute_unspecified_replaces_zero_ipv4() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 1080);
        let result = substitute_unspecified(addr, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(
            result,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1080)
        );
    }

    #[test]
    fn substitute_unspecified_keeps_nonzero() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 1080);
        let result = substitute_unspecified(addr, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(
            result,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 1080)
        );
    }

    #[test]
    fn substitute_unspecified_replaces_zero_ipv6() {
        let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 53);
        let result = substitute_unspecified(addr, IpAddr::V6(Ipv6Addr::LOCALHOST));
        assert_eq!(result, SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 53));
    }

    #[test]
    fn is_unspecified_true_for_zero_v4() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 80);
        assert!(is_unspecified(&addr));
    }

    #[test]
    fn is_unspecified_false_for_nonzero_v4() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 80);
        assert!(!is_unspecified(&addr));
    }

    #[test]
    fn reason_labels() {
        let cases: Vec<(UdpUpstreamError, &str)> = vec![
            (
                UdpUpstreamError::UnsupportedProtocol,
                "unsupported_protocol",
            ),
            (
                UdpUpstreamError::UnsupportedMultiHop,
                "unsupported_multi_hop",
            ),
            (UdpUpstreamError::SocksMethodRejected, "method_rejected"),
            (UdpUpstreamError::SocksAuthFailed, "auth_failed"),
            (
                UdpUpstreamError::SocksAssociateRejected(0x01),
                "associate_rejected",
            ),
            (UdpUpstreamError::MalformedSocksReply, "malformed_reply"),
            (UdpUpstreamError::UdpRelayAddressInvalid, "bad_relay_addr"),
            (UdpUpstreamError::Timeout, "timeout"),
        ];
        for (err, label) in cases {
            assert_eq!(err.reason_label(), label);
        }

        let io_err = std::io::Error::other("test");
        let err = UdpUpstreamError::Io(io_err);
        assert_eq!(err.reason_label(), "io");

        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "test");
        let err = UdpUpstreamError::TcpConnect(io_err);
        assert_eq!(err.reason_label(), "tcp_connect");
    }
}
