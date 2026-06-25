use std::sync::Arc;

use eggress_core::BoxStream;
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::error::TlsError;

/// Perform a client-side TLS handshake, wrapping the stream in TLS.
///
/// The returned `BoxStream` is a `TlsStream<BoxStream>` that can be used
/// for further protocol handshakes or data transfer.
pub async fn tls_connect(
    stream: BoxStream,
    config: Arc<rustls::ClientConfig>,
    server_name: &str,
) -> Result<BoxStream, TlsError> {
    let connector = TlsConnector::from(config);
    let domain = rustls::pki_types::ServerName::try_from(server_name.to_string())
        .map_err(|_| TlsError::InvalidServerName(server_name.to_string()))?;
    let tls_stream = connector.connect(domain, stream).await?;
    Ok(Box::new(tls_stream))
}

/// Perform a server-side TLS handshake, wrapping the stream in TLS.
///
/// The returned `BoxStream` is a `TlsStream<BoxStream>` that can be used
/// for protocol detection and further handling.
pub async fn tls_accept(
    stream: BoxStream,
    config: Arc<rustls::ServerConfig>,
) -> Result<BoxStream, TlsError> {
    let acceptor = TlsAcceptor::from(config);
    let tls_stream = acceptor.accept(stream).await?;
    Ok(Box::new(tls_stream))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::TlsClientConfigBuilder;
    use crate::server::TlsServerConfigBuilder;

    fn init() {
        crate::install_default_crypto_provider();
    }

    #[tokio::test]
    async fn round_trip_tls_handshake() {
        init();
        let (cert_pem, key_pem) = crate::self_signed_cert();

        // Build server config
        let server_config = TlsServerConfigBuilder::new()
            .with_certificate_pem(cert_pem.as_bytes())
            .unwrap()
            .with_key_pem(key_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();

        // Build client config (insecure, since self-signed)
        let client_config = TlsClientConfigBuilder::new()
            .with_insecure()
            .build()
            .unwrap();

        // Create a TCP listener and connect
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let mut tls_stream = tls_accept(boxed, server_config).await.unwrap();
            // Echo back: read and write
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 1024];
            let n = tls_stream.read(&mut buf).await.unwrap();
            tls_stream.write_all(&buf[..n]).await.unwrap();
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(tcp);
        let mut tls_stream = tls_connect(boxed, client_config, "localhost")
            .await
            .unwrap();

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        tls_stream.write_all(b"hello").await.unwrap();
        let mut buf = [0u8; 1024];
        let n = tls_stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello");

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn wrong_server_name_fails() {
        init();
        let (cert_pem, key_pem) = crate::self_signed_cert();

        let server_config = TlsServerConfigBuilder::new()
            .with_certificate_pem(cert_pem.as_bytes())
            .unwrap()
            .with_key_pem(key_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();

        // Use system roots (not insecure) so certificate verification happens
        let client_config = TlsClientConfigBuilder::new()
            .with_system_roots()
            .unwrap()
            .build()
            .unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let _ = tls_accept(boxed, server_config).await;
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(tcp);

        // Connect with wrong server name — should fail because:
        // 1. Self-signed cert is not in system roots, AND
        // 2. Server name doesn't match cert's SAN
        let result = tls_connect(boxed, client_config, "wrong.example.com").await;
        assert!(result.is_err());

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn plaintext_to_tls_server_fails() {
        init();
        let (cert_pem, key_pem) = crate::self_signed_cert();

        let server_config = TlsServerConfigBuilder::new()
            .with_certificate_pem(cert_pem.as_bytes())
            .unwrap()
            .with_key_pem(key_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            // Should fail because client sends plaintext
            let result = tls_accept(boxed, server_config).await;
            assert!(result.is_err());
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send raw HTTP-like bytes (not a TLS ClientHello)
        use tokio::io::AsyncWriteExt;
        let mut tcp = tcp;
        tcp.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();

        server_handle.await.unwrap();
    }
}
