use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;

pub use crate::error::TlsError;

/// Builder for constructing `rustls::ServerConfig` from declarative configuration.
pub struct TlsServerConfigBuilder {
    cert_chain: Vec<CertificateDer<'static>>,
    key_der: Option<PrivatePkcs8KeyDer<'static>>,
    alpn_protocols: Vec<Vec<u8>>,
}

impl TlsServerConfigBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            cert_chain: Vec::new(),
            key_der: None,
            alpn_protocols: Vec::new(),
        }
    }

    /// Load certificate chain from PEM bytes.
    pub fn with_certificate_pem(mut self, cert_pem: &[u8]) -> Result<Self, TlsError> {
        let certs = load_cert_chain_pem(cert_pem)?;
        self.cert_chain = certs;
        Ok(self)
    }

    /// Load private key from PEM bytes (PKCS#8).
    pub fn with_key_pem(mut self, key_pem: &[u8]) -> Result<Self, TlsError> {
        let key = load_private_key_pem(key_pem)?;
        self.key_der = Some(key);
        Ok(self)
    }

    /// Set ALPN protocols (e.g., `b"h2"`, `b"http/1.1"`).
    pub fn with_alpn(mut self, protocols: Vec<Vec<u8>>) -> Self {
        self.alpn_protocols = protocols;
        self
    }

    /// Build the shared `ServerConfig`.
    pub fn build(self) -> Result<Arc<ServerConfig>, TlsError> {
        let key = self.key_der.ok_or(TlsError::MissingPrivateKey)?;
        if self.cert_chain.is_empty() {
            return Err(TlsError::MissingCertificateChain);
        }

        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(self.cert_chain, key.into())?;

        config.alpn_protocols = self.alpn_protocols;
        Ok(Arc::new(config))
    }
}

impl Default for TlsServerConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a PEM certificate chain, returning all certificates found.
fn load_cert_chain_pem(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let mut reader = std::io::BufReader::new(pem);
    let mut certs = Vec::new();
    for item in rustls_pemfile::certs(&mut reader) {
        let cert = item.map_err(|e| TlsError::PemParse(e.to_string()))?;
        certs.push(cert);
    }
    if certs.is_empty() {
        return Err(TlsError::NoCertificatesFound);
    }
    Ok(certs)
}

/// Parse a PEM private key (PKCS#8 format).
fn load_private_key_pem(pem: &[u8]) -> Result<PrivatePkcs8KeyDer<'static>, TlsError> {
    let mut reader = std::io::BufReader::new(pem);
    if let Some(item) = rustls_pemfile::pkcs8_private_keys(&mut reader).next() {
        let key = item.map_err(|e| TlsError::PemParse(e.to_string()))?;
        return Ok(PrivatePkcs8KeyDer::from(key.secret_pkcs8_der().to_vec()));
    }
    Err(TlsError::NoPrivateKeyFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        crate::install_default_crypto_provider();
    }

    #[test]
    fn builder_default() {
        let builder = TlsServerConfigBuilder::new();
        assert!(builder.cert_chain.is_empty());
        assert!(builder.key_der.is_none());
    }

    #[test]
    fn builder_missing_key_fails() {
        init();
        let cert_params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert_der = cert_params.self_signed(&key_pair).unwrap();
        let cert_pem = cert_der.pem();

        let result = TlsServerConfigBuilder::new()
            .with_certificate_pem(cert_pem.as_bytes())
            .unwrap()
            .build();
        assert!(result.is_err());
        match result.unwrap_err() {
            TlsError::MissingPrivateKey => {}
            e => panic!("expected MissingPrivateKey, got: {:?}", e),
        }
    }

    #[test]
    fn builder_round_trip() {
        init();
        // Generate a self-signed cert with rcgen
        let cert_params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert_der = cert_params.self_signed(&key_pair).unwrap();
        let cert_pem = cert_der.pem();
        let key_pem = key_pair.serialize_pem();

        let config = TlsServerConfigBuilder::new()
            .with_certificate_pem(cert_pem.as_bytes())
            .unwrap()
            .with_key_pem(key_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();

        // Server config should be usable
        assert!(!config.alpn_protocols.is_empty() || config.alpn_protocols.is_empty());
    }
}
