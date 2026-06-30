use std::sync::Arc;

use rustls::pki_types::CertificateDer;
use rustls::ClientConfig;

pub use crate::error::TlsError;

/// Builder for constructing `rustls::ClientConfig` from declarative configuration.
pub struct TlsClientConfigBuilder {
    root_store: rustls::RootCertStore,
    alpn_protocols: Vec<Vec<u8>>,
    server_name_override: Option<String>,
    insecure: bool,
}

impl TlsClientConfigBuilder {
    /// Create a new builder with an empty root store.
    pub fn new() -> Self {
        Self {
            root_store: rustls::RootCertStore::empty(),
            alpn_protocols: Vec::new(),
            server_name_override: None,
            insecure: false,
        }
    }

    /// Load system root certificates (via webpki-roots).
    pub fn with_system_roots(mut self) -> Result<Self, TlsError> {
        self.root_store
            .extend(webpki_roots::TLS_SERVER_ROOTS.iter().map(|r| r.to_owned()));
        Ok(self)
    }

    /// Load custom CA certificates from PEM bytes.
    pub fn with_custom_ca_pem(self, pem_bytes: &[u8]) -> Result<Self, TlsError> {
        let roots = crate::roots::load_pem_roots(pem_bytes)?;
        let mut builder = self;
        builder.root_store = roots;
        Ok(builder)
    }

    /// Set ALPN protocols (e.g., `b"h2"`, `b"http/1.1"`).
    pub fn with_alpn(mut self, protocols: Vec<Vec<u8>>) -> Self {
        self.alpn_protocols = protocols;
        self
    }

    /// Set ALPN for HTTP/2 negotiation (h2 + http/1.1 fallback).
    pub fn with_h2_alpn(self) -> Self {
        self.with_alpn(vec![b"h2".to_vec(), b"http/1.1".to_vec()])
    }

    /// Accept any server certificate (insecure, for testing only).
    pub fn with_insecure(mut self) -> Self {
        self.insecure = true;
        self
    }

    /// Set a default server name override used when `tls_connect` is called
    /// without an explicit server name.
    pub fn with_server_name_override(mut self, name: String) -> Self {
        self.server_name_override = Some(name);
        self
    }

    /// Get the server name override, if set.
    pub fn server_name_override(&self) -> Option<&str> {
        self.server_name_override.as_deref()
    }

    /// Build the shared `ClientConfig`.
    pub fn build(self) -> Result<Arc<ClientConfig>, TlsError> {
        let mut config = if self.insecure {
            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(InsecureVerifier))
                .with_no_client_auth()
        } else {
            ClientConfig::builder()
                .with_root_certificates(self.root_store)
                .with_no_client_auth()
        };

        config.alpn_protocols = self.alpn_protocols;
        Ok(Arc::new(config))
    }
}

impl Default for TlsClientConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A certificate verifier that accepts any server certificate.
/// Only for testing — never use in production.
#[derive(Debug)]
struct InsecureVerifier;

impl rustls::client::danger::ServerCertVerifier for InsecureVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        crate::install_default_crypto_provider();
    }

    #[test]
    fn builder_default() {
        let builder = TlsClientConfigBuilder::new();
        assert!(builder.root_store.is_empty());
        assert!(builder.alpn_protocols.is_empty());
        assert!(!builder.insecure);
        assert!(builder.server_name_override.is_none());
    }

    #[test]
    fn builder_system_roots() {
        init();
        let config = TlsClientConfigBuilder::new()
            .with_system_roots()
            .unwrap()
            .build()
            .unwrap();
        // Config built successfully with system roots
        assert!(config.alpn_protocols.is_empty());
    }

    #[test]
    fn builder_insecure() {
        init();
        let config = TlsClientConfigBuilder::new()
            .with_insecure()
            .build()
            .unwrap();
        assert!(config.alpn_protocols.is_empty());
    }

    #[test]
    fn builder_with_server_name_override() {
        let builder = TlsClientConfigBuilder::new()
            .with_server_name_override("custom.example.com".to_string());
        assert_eq!(builder.server_name_override(), Some("custom.example.com"));
    }

    #[test]
    fn builder_with_custom_ca_pem() {
        init();
        // Generate a self-signed cert to use as a custom CA
        let cert_params = rcgen::CertificateParams::new(vec!["test-ca".to_string()]).unwrap();
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert_der = cert_params.self_signed(&key_pair).unwrap();
        let cert_pem = cert_der.pem();

        let config = TlsClientConfigBuilder::new()
            .with_custom_ca_pem(cert_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();
        // Config built successfully with custom CA
        assert!(config.alpn_protocols.is_empty());
    }

    #[tokio::test]
    async fn insecure_connects_to_self_signed_server() {
        init();
        let (cert_pem, key_pem) = crate::self_signed_cert();

        let server_config = crate::TlsServerConfigBuilder::new()
            .with_certificate_pem(cert_pem.as_bytes())
            .unwrap()
            .with_key_pem(key_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();

        let client_config = TlsClientConfigBuilder::new()
            .with_insecure()
            .build()
            .unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: eggress_core::BoxStream = Box::new(stream);
            let mut tls_stream = crate::tls_accept(boxed, server_config).await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 1024];
            let n = tls_stream.read(&mut buf).await.unwrap();
            tls_stream.write_all(&buf[..n]).await.unwrap();
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: eggress_core::BoxStream = Box::new(tcp);
        let mut tls_stream = crate::tls_connect(boxed, client_config, "localhost")
            .await
            .unwrap();

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        tls_stream.write_all(b"hello").await.unwrap();
        let mut buf = [0u8; 1024];
        let n = tls_stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello");

        server_handle.await.unwrap();
    }

    #[test]
    fn builder_with_alpn() {
        init();
        let config = TlsClientConfigBuilder::new()
            .with_insecure()
            .with_alpn(vec![b"h2".to_vec(), b"http/1.1".to_vec()])
            .build()
            .unwrap();
        assert_eq!(config.alpn_protocols.len(), 2);
        assert_eq!(config.alpn_protocols[0], b"h2");
        assert_eq!(config.alpn_protocols[1], b"http/1.1");
    }
}
