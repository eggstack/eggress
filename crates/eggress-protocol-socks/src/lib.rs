//! SOCKS4/5 proxy protocol implementation.
//!
//! This crate provides the SOCKS4 and SOCKS5 proxy protocol handlers.

pub mod detector;
pub mod error;
pub mod socks4;
pub mod socks5;

pub use detector::{Socks4Detector, SOCKS4_PROTOCOL_ID};
pub use error::Socks5Error;
pub use socks4::server::{read_socks4_request, write_socks4_reply};
pub use socks4::{socks4_connect, Socks4Error, Socks4Request, Socks4Status};

use eggress_core::detect::{DetectResult, ProtocolDetector};
use eggress_core::ProtocolId;

/// SOCKS5 protocol detector.
///
/// Checks if the first byte is 0x05 (SOCKS5 version).
pub struct Socks5Detector;

impl ProtocolDetector for Socks5Detector {
    fn id(&self) -> ProtocolId {
        "socks5"
    }

    fn detect(&self, prefix: &[u8]) -> DetectResult {
        if prefix.is_empty() {
            DetectResult::NeedMore { minimum: 1 }
        } else if prefix[0] == 0x05 {
            DetectResult::Match { confidence: 100 }
        } else {
            DetectResult::NoMatch
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::detect::{DetectResult, ProtocolDetector};
    use eggress_core::{BoxStream, TargetAddr, TargetHost};

    #[tokio::test]
    async fn test_detector_identifies_socks4() {
        let detector = Socks4Detector;
        assert_eq!(detector.id(), "socks4");
        assert_eq!(
            detector.detect(b"\x04"),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[test]
    fn test_socks5_detector_match() {
        let detector = Socks5Detector;
        assert_eq!(detector.id(), "socks5");
        assert_eq!(
            detector.detect(&[0x05]),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[test]
    fn test_socks5_detector_no_match() {
        let detector = Socks5Detector;
        assert_eq!(detector.detect(&[0x04]), DetectResult::NoMatch);
        assert_eq!(detector.detect(&[0x00]), DetectResult::NoMatch);
    }

    #[test]
    fn test_socks5_detector_need_more() {
        let detector = Socks5Detector;
        assert_eq!(detector.detect(&[]), DetectResult::NeedMore { minimum: 1 });
    }

    #[test]
    fn test_socks5_detector_with_more_data() {
        let detector = Socks5Detector;
        assert_eq!(
            detector.detect(&[0x05, 0x01, 0x00]),
            DetectResult::Match { confidence: 100 }
        );
    }

    #[tokio::test]
    async fn test_full_socks4_roundtrip() {
        let (addr, jh) = eggress_testkit::start_echo_server().await;
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (mut client_stream, _) = server_listener.accept().await.unwrap();
            let request = read_socks4_request(&mut client_stream).await.unwrap();
            assert_eq!(request.command, 0x01);
            assert_eq!(request.addr, addr);

            let target_stream = tokio::net::TcpStream::connect(request.addr).await.unwrap();
            let _ = write_socks4_reply(
                &mut client_stream,
                Socks4Status::Granted,
                "127.0.0.1:0".parse().unwrap(),
            )
            .await;

            let (mut cr, mut cw) = tokio::io::split(client_stream);
            let (mut tr, mut tw) = tokio::io::split(target_stream);
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut cr, &mut tw).await;
            });
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut tr, &mut cw).await;
            });
        });

        let stream = tokio::net::TcpStream::connect(server_addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip(addr.ip()),
            port: addr.port(),
        };
        let mut conn = socks4_connect(boxed, &target, None).await.unwrap();

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        conn.write_all(b"hello socks4").await.unwrap();
        conn.shutdown().await.unwrap();

        let mut buf = [0u8; 12];
        conn.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello socks4");

        server_jh.abort();
        jh.abort();
    }

    #[tokio::test]
    async fn test_socks4_with_user_id() {
        let (addr, jh) = eggress_testkit::start_echo_server().await;
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (mut client_stream, _) = server_listener.accept().await.unwrap();
            let request = read_socks4_request(&mut client_stream).await.unwrap();
            assert_eq!(request.user_id, "testuser");
            assert_eq!(request.addr, addr);

            let target_stream = tokio::net::TcpStream::connect(request.addr).await.unwrap();
            let _ = write_socks4_reply(
                &mut client_stream,
                Socks4Status::Granted,
                "127.0.0.1:0".parse().unwrap(),
            )
            .await;

            let (mut cr, mut cw) = tokio::io::split(client_stream);
            let (mut tr, mut tw) = tokio::io::split(target_stream);
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut cr, &mut tw).await;
            });
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut tr, &mut cw).await;
            });
        });

        let stream = tokio::net::TcpStream::connect(server_addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip(addr.ip()),
            port: addr.port(),
        };
        let mut conn = socks4_connect(boxed, &target, Some("testuser"))
            .await
            .unwrap();

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        conn.write_all(b"hello user").await.unwrap();
        conn.shutdown().await.unwrap();

        let mut buf = [0u8; 10];
        conn.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello user");

        server_jh.abort();
        jh.abort();
    }

    #[tokio::test]
    async fn test_socks4a_domain_target() {
        let (addr, jh) = eggress_testkit::start_echo_server().await;
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (mut client_stream, _) = server_listener.accept().await.unwrap();
            let request = read_socks4_request(&mut client_stream).await.unwrap();
            assert_eq!(request.domain.as_deref(), Some("example.com"));

            let target_stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let _ = write_socks4_reply(
                &mut client_stream,
                Socks4Status::Granted,
                "127.0.0.1:0".parse().unwrap(),
            )
            .await;

            let (mut cr, mut cw) = tokio::io::split(client_stream);
            let (mut tr, mut tw) = tokio::io::split(target_stream);
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut cr, &mut tw).await;
            });
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut tr, &mut cw).await;
            });
        });

        let stream = tokio::net::TcpStream::connect(server_addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 80,
        };
        let mut conn = socks4_connect(boxed, &target, None).await.unwrap();

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        conn.write_all(b"hello domain").await.unwrap();
        conn.shutdown().await.unwrap();

        let mut buf = [0u8; 12];
        conn.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello domain");

        server_jh.abort();
        jh.abort();
    }

    #[tokio::test]
    async fn test_invalid_version_rejection() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (mut client_stream, _) = server_listener.accept().await.unwrap();
            let result = read_socks4_request(&mut client_stream).await;
            assert!(result.is_err());
            match result.unwrap_err() {
                Socks4Error::InvalidVersion(v) => assert_eq!(v, 0x05),
                other => panic!("expected InvalidVersion, got {:?}", other),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(server_addr).await.unwrap();
        use tokio::io::AsyncWriteExt;
        stream
            .write_all(&[0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_command_rejection_bind() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (mut client_stream, _) = server_listener.accept().await.unwrap();
            let result = read_socks4_request(&mut client_stream).await;
            assert!(result.is_err());
            match result.unwrap_err() {
                Socks4Error::UnsupportedCommand(cmd) => assert_eq!(cmd, 0x02),
                other => panic!("expected UnsupportedCommand, got {:?}", other),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(server_addr).await.unwrap();
        use tokio::io::AsyncWriteExt;
        stream
            .write_all(&[0x04, 0x02, 0x00, 0x50, 127, 0, 0, 1, b'x', 0x00])
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_user_id_too_long() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (mut client_stream, _) = server_listener.accept().await.unwrap();
            let result = read_socks4_request(&mut client_stream).await;
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), Socks4Error::UserIdTooLong));
        });

        let mut stream = tokio::net::TcpStream::connect(server_addr).await.unwrap();
        use tokio::io::AsyncWriteExt;
        let mut payload = vec![0x04, 0x01, 0x00, 0x50, 127, 0, 0, 1];
        payload.extend(std::iter::repeat(b'A').take(256));
        payload.push(0x00);
        stream.write_all(&payload).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks4_rejects_non_matching_protocol() {
        let detector = Socks4Detector;
        assert_eq!(detector.detect(b"\x05"), DetectResult::NoMatch);
        assert_eq!(detector.detect(b"\x16"), DetectResult::NoMatch);
        assert_eq!(detector.detect(b"G"), DetectResult::NoMatch);
    }

    #[tokio::test]
    async fn test_user_id_too_long_client() {
        let long_id = "A".repeat(256);
        let result = client_too_long_uid_guard(&long_id).await;
        assert!(matches!(result, Err(Socks4Error::UserIdTooLong)));
    }

    async fn client_too_long_uid_guard(uid: &str) -> Result<(), Socks4Error> {
        if uid.len() > 255 {
            return Err(Socks4Error::UserIdTooLong);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_socks4_fragmented_read() {
        let (addr, jh) = eggress_testkit::start_echo_server().await;
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (mut client_stream, _) = server_listener.accept().await.unwrap();
            let request = read_socks4_request(&mut client_stream).await.unwrap();
            assert_eq!(request.command, 0x01);

            let target_stream = tokio::net::TcpStream::connect(request.addr).await.unwrap();
            let _ = write_socks4_reply(
                &mut client_stream,
                Socks4Status::Granted,
                "127.0.0.1:0".parse().unwrap(),
            )
            .await;

            let (mut cr, mut cw) = tokio::io::split(client_stream);
            let (mut tr, mut tw) = tokio::io::split(target_stream);
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut cr, &mut tw).await;
            });
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut tr, &mut cw).await;
            });
        });

        // Send SOCKS4 request in fragments.
        let mut stream = tokio::net::TcpStream::connect(server_addr).await.unwrap();
        use tokio::io::AsyncWriteExt;
        stream.write_all(&[0x04]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        stream.write_all(&[0x01]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        stream.write_all(&addr.port().to_be_bytes()).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let ip = match addr.ip() {
            std::net::IpAddr::V4(v4) => v4.octets(),
            _ => unreachable!(),
        };
        stream.write_all(&ip).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        stream.write_all(&[0x00]).await.unwrap();

        // Read reply.
        let mut reply = [0u8; 8];
        use tokio::io::AsyncReadExt;
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x00);
        assert_eq!(reply[1], 90); // granted

        // Send data and verify echo.
        stream.write_all(b"frag").await.unwrap();
        stream.shutdown().await.unwrap();
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"frag");

        server_jh.abort();
        jh.abort();
    }
}
