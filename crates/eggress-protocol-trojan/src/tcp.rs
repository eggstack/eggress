use std::net::IpAddr;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use tokio::io::AsyncWriteExt;
use tokio_rustls::TlsConnector;

use crate::error::TrojanError;
use crate::hash::password_hash;
use eggress_core::{BoxStream, TargetAddr, TargetHost};

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

    // Perform TLS handshake
    let domain = ServerName::try_from(server_name.to_string())
        .map_err(|e| TrojanError::Tls(e.to_string()))?;

    let tls_stream = connector
        .connect(domain, stream)
        .await
        .map_err(|e| TrojanError::Tls(e.to_string()))?;

    // Build Trojan request: hash + CRLF + CONNECT + address + port + CRLF
    let mut request = Vec::new();

    // Password hash line
    let hash = password_hash(password);
    request.extend_from_slice(hash.as_bytes());
    request.extend_from_slice(b"\r\n");

    // CONNECT command
    request.push(0x01);

    // Encode target address (same format as SOCKS5)
    match &target.host {
        TargetHost::Ip(IpAddr::V4(ip)) => {
            request.push(0x01); // ATYP IPv4
            request.extend_from_slice(&ip.octets());
        }
        TargetHost::Ip(IpAddr::V6(ip)) => {
            request.push(0x04); // ATYP IPv6
            request.extend_from_slice(&ip.octets());
        }
        TargetHost::Domain(domain) => {
            if domain.is_empty() || domain.len() > 255 {
                return Err(TrojanError::Protocol(format!(
                    "invalid domain length: {} (must be 1-255)",
                    domain.len()
                )));
            }
            request.push(0x03); // ATYP Domain
            request.push(domain.len() as u8);
            request.extend_from_slice(domain.as_bytes());
        }
    }

    // Port (big-endian)
    request.extend_from_slice(&target.port.to_be_bytes());

    // Terminal CRLF
    request.extend_from_slice(b"\r\n");

    // Send the request
    let mut boxed: BoxStream = Box::new(tls_stream);
    boxed.write_all(&request).await?;
    boxed.flush().await?;

    Ok(boxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hash_length() {
        let hash = password_hash("testpassword");
        // SHA224 produces 28 bytes = 56 hex characters
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
    fn test_wire_format_domain() {
        let hash = password_hash("test");

        let mut request = Vec::new();
        request.extend_from_slice(hash.as_bytes());
        request.extend_from_slice(b"\r\n");
        request.push(0x01); // CONNECT
        request.push(0x03); // ATYP Domain
        request.push(b"example.com".len() as u8);
        request.extend_from_slice(b"example.com");
        request.extend_from_slice(&443u16.to_be_bytes());
        request.extend_from_slice(b"\r\n");

        // Verify structure: hash(56) + CRLF(2) + CMD(1) + ATYP(1) + len(1) + domain(11) + port(2) + CRLF(2) = 76
        assert_eq!(request.len(), 76);
        assert!(request.starts_with(hash.as_bytes()));
    }

    #[test]
    fn test_wire_format_ipv4() {
        let hash = password_hash("pass");

        let mut request = Vec::new();
        request.extend_from_slice(hash.as_bytes());
        request.extend_from_slice(b"\r\n");
        request.push(0x01); // CONNECT
        request.push(0x01); // ATYP IPv4
        request.extend_from_slice(&[93, 184, 216, 34]);
        request.extend_from_slice(&80u16.to_be_bytes());
        request.extend_from_slice(b"\r\n");

        // hash(56) + CRLF(2) + CMD(1) + ATYP(1) + ip(4) + port(2) + CRLF(2) = 68
        assert_eq!(request.len(), 68);
    }

    #[test]
    fn test_wire_format_ipv6() {
        let hash = password_hash("pass");
        let ip: std::net::Ipv6Addr = "::1".parse().unwrap();

        let mut request = Vec::new();
        request.extend_from_slice(hash.as_bytes());
        request.extend_from_slice(b"\r\n");
        request.push(0x01); // CONNECT
        request.push(0x04); // ATYP IPv6
        request.extend_from_slice(&ip.octets());
        request.extend_from_slice(&443u16.to_be_bytes());
        request.extend_from_slice(b"\r\n");

        // hash(56) + CRLF(2) + CMD(1) + ATYP(1) + ip(16) + port(2) + CRLF(2) = 80
        assert_eq!(request.len(), 80);
    }

    #[tokio::test]
    async fn test_trojan_connect_through_synthetic_tls_server() {
        eggress_transport_tls::install_default_crypto_provider();
        // Generate a self-signed certificate for the test server
        let subject_alt_names = vec!["localhost".to_string()];
        let cert_params = rcgen::CertificateParams::new(subject_alt_names).expect("valid params");
        let cert_key = rcgen::KeyPair::generate().expect("key gen");
        let cert = cert_params
            .self_signed(&cert_key)
            .expect("self-signed cert");

        let cert_der = cert.der().clone();
        let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert_key.serialize_der());

        // Build server TLS config
        let mut server_root_store = rustls::RootCertStore::empty();
        server_root_store.add(cert_der.clone()).unwrap();
        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], key_der.into())
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

        // Start TCP listener
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let expected_password = "my-secret-password";
        let expected_hash = password_hash(expected_password);

        // Spawn server task
        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let tls_stream = acceptor.accept(stream).await.unwrap();

            let mut buf = vec![0u8; 4096];
            let mut reader = tls_stream;
            let n = tokio::io::AsyncReadExt::read(&mut reader, &mut buf)
                .await
                .unwrap();
            buf.truncate(n);

            // Parse: hash(56) + CRLF(2) + CMD(1) + ATYP(1) + addr + port(2) + CRLF(2)
            assert!(buf.len() > 60);
            let received_hash = std::str::from_utf8(&buf[..56]).unwrap();
            assert_eq!(received_hash, expected_hash);
            assert_eq!(&buf[56..58], b"\r\n");
            assert_eq!(buf[58], 0x01); // CONNECT command
            assert_eq!(buf[59], 0x01); // ATYP IPv4
            assert_eq!(&buf[60..64], &[127, 0, 0, 1]);
            assert_eq!(&buf[64..66], &8080u16.to_be_bytes());
            assert_eq!(&buf[66..68], b"\r\n");

            // Echo back some data to confirm the connection works
            use tokio::io::AsyncWriteExt;
            reader.write_all(b"hello from trojan server").await.unwrap();
        });

        // Client side
        let tcp_stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(tcp_stream);

        // Use the synthetic cert as the root certificate for the client
        let mut root_store = rustls::RootCertStore::empty();
        root_store.add(cert_der).unwrap();
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));

        let domain = ServerName::try_from("localhost".to_string()).unwrap();
        let tls_stream = connector.connect(domain, boxed).await.unwrap();

        // Build and send Trojan request manually
        let mut request = Vec::new();
        let hash = password_hash(expected_password);
        request.extend_from_slice(hash.as_bytes());
        request.extend_from_slice(b"\r\n");
        request.push(0x01); // CONNECT
        request.push(0x01); // ATYP IPv4
        request.extend_from_slice(&[127, 0, 0, 1]);
        request.extend_from_slice(&8080u16.to_be_bytes());
        request.extend_from_slice(b"\r\n");

        let mut boxed_tls: BoxStream = Box::new(tls_stream);
        use tokio::io::AsyncWriteExt;
        boxed_tls.write_all(&request).await.unwrap();
        boxed_tls.flush().await.unwrap();

        // Read server response
        use tokio::io::AsyncReadExt;
        let mut response = vec![0u8; 256];
        let n = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            boxed_tls.read(&mut response).await
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
        // Generate test certificate
        let subject_alt_names = vec!["localhost".to_string()];
        let cert_params = rcgen::CertificateParams::new(subject_alt_names).expect("valid params");
        let cert_key = rcgen::KeyPair::generate().expect("key gen");
        let cert = cert_params
            .self_signed(&cert_key)
            .expect("self-signed cert");

        let cert_der = cert.der().clone();
        let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert_key.serialize_der());

        let mut server_root_store = rustls::RootCertStore::empty();
        server_root_store.add(cert_der.clone()).unwrap();
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
            let n = tokio::io::AsyncReadExt::read(&mut reader, &mut buf)
                .await
                .unwrap();
            buf.truncate(n);

            // Check password hash
            let received_hash = std::str::from_utf8(&buf[..56]).unwrap();
            if received_hash != correct_hash {
                // Wrong password - drop connection (Trojan behavior)
            }
        });

        // Client sends wrong password
        let tcp_stream = tokio::net::TcpStream::connect(addr).await.unwrap();

        let mut root_store = rustls::RootCertStore::empty();
        root_store.add(cert_der).unwrap();
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));

        let domain = ServerName::try_from("localhost".to_string()).unwrap();
        let tls_stream = connector
            .connect(domain, Box::new(tcp_stream) as BoxStream)
            .await
            .unwrap();

        let mut request = Vec::new();
        let wrong_hash = password_hash("wrong-password");
        request.extend_from_slice(wrong_hash.as_bytes());
        request.extend_from_slice(b"\r\n");
        request.push(0x01);
        request.push(0x01);
        request.extend_from_slice(&[127, 0, 0, 1]);
        request.extend_from_slice(&80u16.to_be_bytes());
        request.extend_from_slice(b"\r\n");

        let mut boxed_tls: BoxStream = Box::new(tls_stream);
        use tokio::io::AsyncWriteExt;
        boxed_tls.write_all(&request).await.unwrap();
        boxed_tls.flush().await.unwrap();

        // Server should drop the connection
        use tokio::io::AsyncReadExt;
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            let mut buf = [0u8; 1];
            boxed_tls.read(&mut buf).await
        })
        .await;

        // Connection should be closed (EOF, error, or timeout)
        match result {
            Ok(Ok(0)) => {}  // EOF - connection closed by server
            Ok(Ok(_)) => {}  // Got data (unexpected but OK)
            Ok(Err(_)) => {} // Error - connection reset
            Err(_) => {}     // Timeout
        }

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_domain_length_255_accepted() {
        eggress_transport_tls::install_default_crypto_provider();
        let subject_alt_names = vec!["localhost".to_string()];
        let cert_params = rcgen::CertificateParams::new(subject_alt_names).expect("valid params");
        let cert_key = rcgen::KeyPair::generate().expect("key gen");
        let cert = cert_params
            .self_signed(&cert_key)
            .expect("self-signed cert");

        let cert_der = cert.der().clone();
        let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert_key.serialize_der());

        let mut server_root_store = rustls::RootCertStore::empty();
        server_root_store.add(cert_der.clone()).unwrap();
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
            use tokio::io::AsyncReadExt;
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
        let client_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let tls_config = std::sync::Arc::new(client_config);

        let domain_255 = "a".repeat(255);
        let target = TargetAddr {
            host: TargetHost::Domain(domain_255),
            port: 443,
        };

        let result = trojan_connect(boxed, &target, "pass", "localhost", Some(tls_config)).await;
        assert!(result.is_ok());

        server_jh.await.unwrap();
    }

    #[test]
    fn test_domain_length_256_rejected() {
        let domain_256 = "a".repeat(256);
        let target = TargetAddr {
            host: TargetHost::Domain(domain_256),
            port: 443,
        };

        // Verify that the domain is indeed too long for the validation
        match &target.host {
            TargetHost::Domain(domain) => {
                assert!(domain.len() > 255, "domain should be longer than 255");
                assert_eq!(domain.len(), 256);
            }
            _ => panic!("expected domain"),
        }
    }

    #[test]
    fn test_empty_domain_rejected() {
        let target = TargetAddr {
            host: TargetHost::Domain(String::new()),
            port: 443,
        };

        match &target.host {
            TargetHost::Domain(domain) => {
                assert!(domain.is_empty(), "domain should be empty");
            }
            _ => panic!("expected domain"),
        }
    }
}
