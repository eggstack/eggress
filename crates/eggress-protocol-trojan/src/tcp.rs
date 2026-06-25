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
pub async fn trojan_connect(
    stream: BoxStream,
    target: &TargetAddr,
    password: &str,
    server_name: &str,
) -> Result<BoxStream, TrojanError> {
    // Build TLS config with webpki root certificates
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));

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
}
