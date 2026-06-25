use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::error::Socks4Error;
use super::server::Socks4Status;
use eggress_core::{BoxStream, TargetAddr, TargetHost};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socks4::test_server::{TestServerHandle, TestServerMode};
    use eggress_core::{TargetAddr, TargetHost};

    fn ipv4_target(addr: SocketAddr) -> TargetAddr {
        TargetAddr {
            host: TargetHost::Ip(addr.ip()),
            port: addr.port(),
        }
    }

    fn domain_target(domain: &str, port: u16) -> TargetAddr {
        TargetAddr {
            host: TargetHost::Domain(domain.to_string()),
            port,
        }
    }

    #[tokio::test]
    async fn ipv4_connect_success() {
        let server = TestServerHandle::spawn(TestServerMode::Success).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = ipv4_target(server.addr);

        let mut conn = socks4_connect(boxed, &target, None).await.unwrap();
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        conn.write_all(b"ping").await.unwrap();
        conn.shutdown().await.unwrap();
        let mut buf = [0u8; 4];
        conn.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
    }

    #[tokio::test]
    async fn domain_socks4a_success() {
        let server = TestServerHandle::spawn(TestServerMode::DomainSuccess).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = domain_target("example.com", 80);

        let mut conn = socks4_connect(boxed, &target, None).await.unwrap();
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        conn.write_all(b"hello").await.unwrap();
        conn.shutdown().await.unwrap();
        let mut buf = [0u8; 5];
        conn.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
    }

    #[tokio::test]
    async fn request_rejected() {
        let server = TestServerHandle::spawn(TestServerMode::Rejected).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = ipv4_target(server.addr);

        let result = socks4_connect(boxed, &target, None).await;
        assert!(matches!(result, Err(Socks4Error::ConnectionFailed)));
    }

    #[tokio::test]
    async fn identd_unavailable() {
        let server = TestServerHandle::spawn(TestServerMode::NoIdent).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = ipv4_target(server.addr);

        let result = socks4_connect(boxed, &target, None).await;
        assert!(matches!(result, Err(Socks4Error::FailedNoIdent)));
    }

    #[tokio::test]
    async fn different_user() {
        let server = TestServerHandle::spawn(TestServerMode::DifferentUser).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = ipv4_target(server.addr);

        let result = socks4_connect(boxed, &target, Some("alice")).await;
        assert!(matches!(result, Err(Socks4Error::FailedDifferentUser)));
    }

    #[tokio::test]
    async fn malformed_response() {
        let server = TestServerHandle::spawn(TestServerMode::MalformedResponse).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = ipv4_target(server.addr);

        let result = socks4_connect(boxed, &target, None).await;
        assert!(matches!(result, Err(Socks4Error::InvalidVersion(_))));
    }

    #[tokio::test]
    async fn unknown_status() {
        let server = TestServerHandle::spawn(TestServerMode::UnknownStatus).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = ipv4_target(server.addr);

        let result = socks4_connect(boxed, &target, None).await;
        assert!(matches!(result, Err(Socks4Error::UnknownStatus(99))));
    }

    #[tokio::test]
    async fn slow_response_timeout() {
        let server = TestServerHandle::spawn(TestServerMode::SlowResponse).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = ipv4_target(server.addr);

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            socks4_connect(boxed, &target, None),
        )
        .await;

        assert!(result.is_err(), "expected timeout");
    }

    #[tokio::test]
    async fn user_id_length_limit() {
        let server = TestServerHandle::spawn(TestServerMode::Success).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let long_id = "A".repeat(256);

        let result = socks4_connect(boxed, &ipv4_target(server.addr), Some(&long_id)).await;
        assert!(matches!(result, Err(Socks4Error::UserIdTooLong)));
    }

    #[tokio::test]
    async fn user_id_with_connect() {
        let server = TestServerHandle::spawn(TestServerMode::Success).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = ipv4_target(server.addr);

        let mut conn = socks4_connect(boxed, &target, Some("testuser"))
            .await
            .unwrap();
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        conn.write_all(b"uid").await.unwrap();
        conn.shutdown().await.unwrap();
        let mut buf = [0u8; 3];
        conn.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"uid");
    }

    #[tokio::test]
    async fn no_reply_causes_eof() {
        let server = TestServerHandle::spawn(TestServerMode::NoReply).await;
        let stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = ipv4_target(server.addr);

        let result = socks4_connect(boxed, &target, None).await;
        assert!(matches!(result, Err(Socks4Error::Io(_))));
    }
}
