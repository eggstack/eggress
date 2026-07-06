use std::net::IpAddr;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use tokio::io::AsyncWriteExt;
use tokio_rustls::TlsConnector;

use crate::error::TrojanError;
use crate::hash::password_hash;
use eggress_core::{BoxStream, TargetAddr, TargetHost};

/// Build the Trojan request bytes for a target.
///
/// Layout: `hash(56) + CRLF + CONNECT(1) + ATYP + addr + port(2) + CRLF`.
/// Domain targets must be 1-255 bytes; other lengths return
/// [`TrojanError::Protocol`].
pub fn encode_trojan_request(target: &TargetAddr, password: &str) -> Result<Vec<u8>, TrojanError> {
    let mut request = Vec::new();

    let hash = password_hash(password);
    request.extend_from_slice(hash.as_bytes());
    request.extend_from_slice(b"\r\n");

    request.push(0x01);

    match &target.host {
        TargetHost::Ip(IpAddr::V4(ip)) => {
            request.push(0x01);
            request.extend_from_slice(&ip.octets());
        }
        TargetHost::Ip(IpAddr::V6(ip)) => {
            request.push(0x04);
            request.extend_from_slice(&ip.octets());
        }
        TargetHost::Domain(domain) => {
            if domain.is_empty() || domain.len() > 255 {
                return Err(TrojanError::Protocol(format!(
                    "invalid domain length: {} (must be 1-255)",
                    domain.len()
                )));
            }
            request.push(0x03);
            request.push(domain.len() as u8);
            request.extend_from_slice(domain.as_bytes());
        }
    }

    request.extend_from_slice(&target.port.to_be_bytes());
    request.extend_from_slice(b"\r\n");

    Ok(request)
}

/// Parsed result from accepting an inbound Trojan connection.
#[derive(Debug)]
pub struct TrojanAcceptResult {
    /// The target address requested by the client.
    pub target: TargetAddr,
}

/// Accept an inbound Trojan connection from a TLS-terminated stream.
///
/// Reads the Trojan handshake: `hash(56) + CRLF + CMD(1) + ATYP + addr + port(2) + CRLF`.
/// Verifies the password hash and returns the parsed target address.
///
/// This function expects to receive a stream **after** TLS termination.
/// The first 58 bytes (56-char SHA224 hash + `\r\n`) are the password hash,
/// followed by the CONNECT command and target address.
///
/// # Arguments
/// * `stream` - The TLS-terminated stream
/// * `password` - The expected password (will be hashed and compared)
pub async fn trojan_accept(
    mut stream: BoxStream,
    password: &str,
) -> Result<(BoxStream, TrojanAcceptResult), TrojanError> {
    use tokio::io::AsyncReadExt;

    let expected_hash = password_hash(password);

    // Read the hash (56 bytes) + CRLF (2 bytes) = 58 bytes
    let mut hash_buf = [0u8; 58];
    stream
        .read_exact(&mut hash_buf)
        .await
        .map_err(TrojanError::Io)?;

    let received_hash = std::str::from_utf8(&hash_buf[..56])
        .map_err(|_| TrojanError::Protocol("invalid hash encoding".into()))?;

    if received_hash != expected_hash {
        return Err(TrojanError::AuthFailed);
    }

    if &hash_buf[56..58] != b"\r\n" {
        return Err(TrojanError::Protocol("missing CRLF after hash".into()));
    }

    // Read CMD (1 byte) — must be 0x01 (CONNECT)
    let mut cmd_buf = [0u8; 1];
    stream
        .read_exact(&mut cmd_buf)
        .await
        .map_err(TrojanError::Io)?;

    if cmd_buf[0] != 0x01 {
        return Err(TrojanError::Protocol(format!(
            "unsupported command: {:#04x} (only CONNECT is supported)",
            cmd_buf[0]
        )));
    }

    // Read ATYP (1 byte)
    let mut atyp_buf = [0u8; 1];
    stream
        .read_exact(&mut atyp_buf)
        .await
        .map_err(TrojanError::Io)?;

    let target = match atyp_buf[0] {
        // IPv4
        0x01 => {
            let mut addr_buf = [0u8; 4 + 2]; // 4 bytes IP + 2 bytes port
            stream
                .read_exact(&mut addr_buf)
                .await
                .map_err(TrojanError::Io)?;
            let ip = std::net::Ipv4Addr::new(addr_buf[0], addr_buf[1], addr_buf[2], addr_buf[3]);
            let port = u16::from_be_bytes([addr_buf[4], addr_buf[5]]);
            TargetAddr {
                host: TargetHost::Ip(std::net::IpAddr::V4(ip)),
                port,
            }
        }
        // Domain
        0x03 => {
            let mut len_buf = [0u8; 1];
            stream
                .read_exact(&mut len_buf)
                .await
                .map_err(TrojanError::Io)?;
            let domain_len = len_buf[0] as usize;
            if domain_len == 0 {
                return Err(TrojanError::Protocol("empty domain".into()));
            }
            let mut domain_buf = vec![0u8; domain_len + 2]; // domain + port
            stream
                .read_exact(&mut domain_buf)
                .await
                .map_err(TrojanError::Io)?;
            let domain = String::from_utf8(domain_buf[..domain_len].to_vec())
                .map_err(|_| TrojanError::Protocol("invalid domain UTF-8".into()))?;
            let port = u16::from_be_bytes([domain_buf[domain_len], domain_buf[domain_len + 1]]);
            TargetAddr {
                host: TargetHost::Domain(domain),
                port,
            }
        }
        // IPv6
        0x04 => {
            let mut addr_buf = [0u8; 16 + 2]; // 16 bytes IP + 2 bytes port
            stream
                .read_exact(&mut addr_buf)
                .await
                .map_err(TrojanError::Io)?;
            let ip = std::net::Ipv6Addr::from(<[u8; 16]>::try_from(&addr_buf[..16]).unwrap());
            let port = u16::from_be_bytes([addr_buf[16], addr_buf[17]]);
            TargetAddr {
                host: TargetHost::Ip(std::net::IpAddr::V6(ip)),
                port,
            }
        }
        other => {
            return Err(TrojanError::Protocol(format!(
                "unsupported ATYP: {:#04x}",
                other
            )));
        }
    };

    // Read trailing CRLF
    let mut crlf_buf = [0u8; 2];
    stream
        .read_exact(&mut crlf_buf)
        .await
        .map_err(TrojanError::Io)?;

    if &crlf_buf != b"\r\n" {
        return Err(TrojanError::Protocol("missing trailing CRLF".into()));
    }

    Ok((stream, TrojanAcceptResult { target }))
}

/// Connect to a target through a Trojan server.
///
/// Performs a TLS handshake, sends the Trojan password hash and CONNECT
/// request, and returns the upgraded bidirectional stream.
///
/// # Arguments
/// * `stream` - The TCP stream to the Trojan server
/// * `target` - The target address to connect to
/// * `password` - The Trojan password (will be SHA224-hashed for auth)
/// * `server_name` - The TLS server name for SNI and certificate verification
/// * `tls_config` - Optional shared TLS client config. If `None`, builds one with system roots.
pub async fn trojan_connect(
    stream: BoxStream,
    target: &TargetAddr,
    password: &str,
    server_name: &str,
    tls_config: Option<Arc<rustls::ClientConfig>>,
) -> Result<BoxStream, TrojanError> {
    let config = match tls_config {
        Some(c) => c,
        None => {
            let builder = eggress_transport_tls::TlsClientConfigBuilder::new();
            let builder = builder
                .with_system_roots()
                .map_err(|e| TrojanError::Tls(format!("failed to load system roots: {e}")))?;
            builder
                .build()
                .map_err(|e| TrojanError::Tls(format!("failed to build TLS config: {e}")))?
        }
    };

    let connector = TlsConnector::from(config);

    let domain = ServerName::try_from(server_name.to_string())
        .map_err(|e| TrojanError::Tls(e.to_string()))?;

    let tls_stream = connector
        .connect(domain, stream)
        .await
        .map_err(|e| TrojanError::Tls(e.to_string()))?;

    let request = encode_trojan_request(target, password)?;
    let mut boxed: BoxStream = Box::new(tls_stream);
    boxed.write_all(&request).await?;
    boxed.flush().await?;

    Ok(boxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[test]
    fn test_password_hash_length() {
        let hash = password_hash("testpassword");
        assert_eq!(hash.len(), 56);
    }

    #[test]
    fn test_password_hash_deterministic() {
        let h1 = password_hash("mypassword");
        let h2 = password_hash("mypassword");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_password_hash_different_passwords() {
        let h1 = password_hash("password1");
        let h2 = password_hash("password2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn encode_trojan_request_domain_layout() {
        let hash = password_hash("test");

        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };
        let request = encode_trojan_request(&target, "test").unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(hash.as_bytes());
        expected.extend_from_slice(b"\r\n");
        expected.push(0x01);
        expected.push(0x03);
        expected.push(b"example.com".len() as u8);
        expected.extend_from_slice(b"example.com");
        expected.extend_from_slice(&443u16.to_be_bytes());
        expected.extend_from_slice(b"\r\n");

        assert_eq!(request, expected);
        assert_eq!(request.len(), 76);
    }

    #[test]
    fn encode_trojan_request_ipv4_layout() {
        let hash = password_hash("pass");

        let target = TargetAddr {
            host: TargetHost::Ip("93.184.216.34".parse().unwrap()),
            port: 80,
        };
        let request = encode_trojan_request(&target, "pass").unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(hash.as_bytes());
        expected.extend_from_slice(b"\r\n");
        expected.push(0x01);
        expected.push(0x01);
        expected.extend_from_slice(&[93, 184, 216, 34]);
        expected.extend_from_slice(&80u16.to_be_bytes());
        expected.extend_from_slice(b"\r\n");

        assert_eq!(request, expected);
        assert_eq!(request.len(), 68);
    }

    #[test]
    fn encode_trojan_request_ipv6_layout() {
        let hash = password_hash("pass");
        let ip: std::net::Ipv6Addr = "::1".parse().unwrap();

        let target = TargetAddr {
            host: TargetHost::Ip("::1".parse().unwrap()),
            port: 443,
        };
        let request = encode_trojan_request(&target, "pass").unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(hash.as_bytes());
        expected.extend_from_slice(b"\r\n");
        expected.push(0x01);
        expected.push(0x04);
        expected.extend_from_slice(&ip.octets());
        expected.extend_from_slice(&443u16.to_be_bytes());
        expected.extend_from_slice(b"\r\n");

        assert_eq!(request, expected);
        assert_eq!(request.len(), 80);
    }

    #[test]
    fn encode_trojan_request_rejects_domain_length_256() {
        let target = TargetAddr {
            host: TargetHost::Domain("a".repeat(256)),
            port: 443,
        };
        let err = encode_trojan_request(&target, "pass").unwrap_err();
        assert!(matches!(err, TrojanError::Protocol(_)));
    }

    #[test]
    fn encode_trojan_request_rejects_empty_domain() {
        let target = TargetAddr {
            host: TargetHost::Domain(String::new()),
            port: 443,
        };
        let err = encode_trojan_request(&target, "pass").unwrap_err();
        assert!(matches!(err, TrojanError::Protocol(_)));
    }

    #[test]
    fn encode_trojan_request_accepts_domain_length_255() {
        let target = TargetAddr {
            host: TargetHost::Domain("a".repeat(255)),
            port: 443,
        };
        let request = encode_trojan_request(&target, "pass").unwrap();
        // hash(56) + CRLF(2) + CMD(1) + ATYP(1) + len(1) + domain(255) + port(2) + CRLF(2) = 320
        assert_eq!(request.len(), 320);
        assert_eq!(request[60], 255);
    }

    #[tokio::test]
    async fn trojan_connect_through_synthetic_tls_server_uses_exported_function() {
        eggress_transport_tls::install_default_crypto_provider();
        let subject_alt_names = vec!["localhost".to_string()];
        let cert_params = rcgen::CertificateParams::new(subject_alt_names).expect("valid params");
        let cert_key = rcgen::KeyPair::generate().expect("key gen");
        let cert = cert_params
            .self_signed(&cert_key)
            .expect("self-signed cert");

        let cert_der = cert.der().clone();
        let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert_key.serialize_der());

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], key_der.into())
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let expected_password = "my-secret-password";
        let expected_hash = password_hash(expected_password);

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let tls_stream = acceptor.accept(stream).await.unwrap();

            let mut buf = vec![0u8; 4096];
            let mut reader = tls_stream;
            let n = reader.read(&mut buf).await.unwrap();
            buf.truncate(n);

            assert!(buf.len() > 60);
            let received_hash = std::str::from_utf8(&buf[..56]).unwrap();
            assert_eq!(received_hash, expected_hash);
            assert_eq!(&buf[56..58], b"\r\n");
            assert_eq!(buf[58], 0x01);
            assert_eq!(buf[59], 0x01);
            assert_eq!(&buf[60..64], &[127, 0, 0, 1]);
            assert_eq!(&buf[64..66], &8080u16.to_be_bytes());
            assert_eq!(&buf[66..68], b"\r\n");

            tokio::io::AsyncWriteExt::write_all(&mut reader, b"hello from trojan server")
                .await
                .unwrap();
        });

        let tcp_stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(tcp_stream);

        let mut root_store = rustls::RootCertStore::empty();
        root_store.add(cert_der).unwrap();
        let tls_config = Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth(),
        );

        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 8080,
        };

        let mut stream = trojan_connect(
            boxed,
            &target,
            expected_password,
            "localhost",
            Some(tls_config),
        )
        .await
        .unwrap();

        let mut response = vec![0u8; 256];
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            stream.read(&mut response).await
        })
        .await
        .unwrap()
        .unwrap();
        response.truncate(n);

        assert_eq!(&response, b"hello from trojan server");

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_trojan_connect_wrong_password_rejected() {
        eggress_transport_tls::install_default_crypto_provider();
        let subject_alt_names = vec!["localhost".to_string()];
        let cert_params = rcgen::CertificateParams::new(subject_alt_names).expect("valid params");
        let cert_key = rcgen::KeyPair::generate().expect("key gen");
        let cert = cert_params
            .self_signed(&cert_key)
            .expect("self-signed cert");

        let cert_der = cert.der().clone();
        let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert_key.serialize_der());

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], key_der.into())
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let correct_password = "correct-password";
        let correct_hash = password_hash(correct_password);

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let tls_stream = acceptor.accept(stream).await.unwrap();

            let mut buf = vec![0u8; 4096];
            let mut reader = tls_stream;
            let n = reader.read(&mut buf).await.unwrap();
            buf.truncate(n);

            let received_hash = std::str::from_utf8(&buf[..56]).unwrap();
            assert_ne!(
                received_hash, correct_hash,
                "server should see a password hash different from the correct one"
            );
        });

        let tcp_stream = tokio::net::TcpStream::connect(addr).await.unwrap();

        let mut root_store = rustls::RootCertStore::empty();
        root_store.add(cert_der).unwrap();
        let tls_config = Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth(),
        );

        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };

        let mut stream = trojan_connect(
            Box::new(tcp_stream) as BoxStream,
            &target,
            "wrong-password",
            "localhost",
            Some(tls_config),
        )
        .await
        .unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            let mut buf = [0u8; 1];
            stream.read(&mut buf).await
        })
        .await;

        match result {
            Ok(Ok(0)) => {}
            Ok(Ok(_)) => {}
            Ok(Err(_)) => {}
            Err(_) => {}
        }

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn trojan_connect_accepts_domain_length_255() {
        eggress_transport_tls::install_default_crypto_provider();
        let subject_alt_names = vec!["localhost".to_string()];
        let cert_params = rcgen::CertificateParams::new(subject_alt_names).expect("valid params");
        let cert_key = rcgen::KeyPair::generate().expect("key gen");
        let cert = cert_params
            .self_signed(&cert_key)
            .expect("self-signed cert");

        let cert_der = cert.der().clone();
        let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert_key.serialize_der());

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], key_der.into())
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let tls_stream = acceptor.accept(stream).await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                let mut reader = tls_stream;
                let _ = reader.read(&mut buf).await;
            })
            .await;
        });

        let tcp_stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(tcp_stream);

        let mut root_store = rustls::RootCertStore::empty();
        root_store.add(cert_der).unwrap();
        let tls_config = Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth(),
        );

        let target = TargetAddr {
            host: TargetHost::Domain("a".repeat(255)),
            port: 443,
        };

        let result = trojan_connect(boxed, &target, "pass", "localhost", Some(tls_config)).await;
        assert!(result.is_ok());

        server_jh.await.unwrap();
    }

    // ── trojan_accept round-trip tests ──

    #[tokio::test]
    async fn trojan_accept_roundtrip_ipv4() {
        let password = "server-secret";
        let target = TargetAddr {
            host: TargetHost::Ip("93.184.216.34".parse().unwrap()),
            port: 80,
        };
        let request = encode_trojan_request(&target, password).unwrap();
        let stream: BoxStream = Box::new(std::io::Cursor::new(request));

        let (mut stream, result) = trojan_accept(stream, password).await.unwrap();
        assert_eq!(
            result.target.host,
            TargetHost::Ip("93.184.216.34".parse().unwrap())
        );
        assert_eq!(result.target.port, 80);

        // Stream should be usable for data after the handshake
        let mut buf = [0u8; 5];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(n, 0, "cursor is at EOF after handshake");
    }

    #[tokio::test]
    async fn trojan_accept_roundtrip_domain() {
        let password = "my-pass";
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };
        let request = encode_trojan_request(&target, password).unwrap();
        let stream: BoxStream = Box::new(std::io::Cursor::new(request));

        let (_, result) = trojan_accept(stream, password).await.unwrap();
        assert_eq!(
            result.target.host,
            TargetHost::Domain("example.com".to_string())
        );
        assert_eq!(result.target.port, 443);
    }

    #[tokio::test]
    async fn trojan_accept_roundtrip_ipv6() {
        let password = "pw";
        let target = TargetAddr {
            host: TargetHost::Ip("::1".parse().unwrap()),
            port: 8080,
        };
        let request = encode_trojan_request(&target, password).unwrap();
        let stream: BoxStream = Box::new(std::io::Cursor::new(request));

        let (_, result) = trojan_accept(stream, password).await.unwrap();
        assert_eq!(result.target.host, TargetHost::Ip("::1".parse().unwrap()));
        assert_eq!(result.target.port, 8080);
    }

    #[tokio::test]
    async fn trojan_accept_wrong_password_returns_auth_failed() {
        let password = "correct-password";
        let target = TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        };
        let request = encode_trojan_request(&target, password).unwrap();
        let stream: BoxStream = Box::new(std::io::Cursor::new(request));

        let result = trojan_accept(stream, "wrong-password").await;
        assert!(
            matches!(result, Err(TrojanError::AuthFailed)),
            "expected AuthFailed, got {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn trojan_accept_bad_atyp_returns_protocol_error() {
        let password = "pass";
        let hash = password_hash(password);
        let mut bad_handshake = Vec::new();
        bad_handshake.extend_from_slice(hash.as_bytes());
        bad_handshake.extend_from_slice(b"\r\n");
        bad_handshake.push(0x01); // CONNECT
        bad_handshake.push(0xFF); // invalid ATYP

        let stream: BoxStream = Box::new(std::io::Cursor::new(bad_handshake));
        let result = trojan_accept(stream, password).await;
        match result {
            Err(TrojanError::Protocol(msg)) => {
                assert!(msg.contains("unsupported ATYP"), "unexpected: {}", msg);
            }
            other => panic!("expected Protocol error, got {:?}", other.err()),
        }
    }

    #[tokio::test]
    async fn trojan_accept_non_connect_command_returns_protocol_error() {
        let password = "pass";
        let mut request = Vec::new();
        let hash = password_hash(password);
        request.extend_from_slice(hash.as_bytes());
        request.extend_from_slice(b"\r\n");
        request.push(0x02); // UDP ASSOCIATE (unsupported)
        request.push(0x01); // ATYP IPv4
        request.extend_from_slice(&[127, 0, 0, 1]);
        request.extend_from_slice(&80u16.to_be_bytes());
        request.extend_from_slice(b"\r\n");

        let stream: BoxStream = Box::new(std::io::Cursor::new(request));
        let result = trojan_accept(stream, password).await;
        match result {
            Err(TrojanError::Protocol(msg)) => {
                assert!(msg.contains("unsupported command"), "unexpected: {}", msg);
            }
            other => panic!("expected Protocol error, got {:?}", other.err()),
        }
    }
}
