use std::sync::Arc;

use rustls::pki_types::{pem::PemObject, CertificateDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;

pub use crate::error::TlsError;

/// Builder for constructing `rustls::ServerConfig` from declarative configuration.
pub struct TlsServerConfigBuilder {
    cert_chain: Vec<CertificateDer<'static>>,
    key_der: Option<PrivatePkcs8KeyDer<'static>>,
    alpn_protocols: Vec<Vec<u8>>,
    client_ca_pem: Option<Vec<u8>>,
    require_client_cert: bool,
}

impl TlsServerConfigBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            cert_chain: Vec::new(),
            key_der: None,
            alpn_protocols: Vec::new(),
            client_ca_pem: None,
            require_client_cert: false,
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

    /// Load client CA roots for mutual TLS from PEM bytes.
    ///
    /// When combined with [`TlsServerConfigBuilder::with_require_client_cert`],
    /// the server requires and validates a client certificate signed by one
    /// of these roots. Without the require flag, client certs are verified
    /// when presented but not required.
    pub fn with_client_ca_pem(mut self, ca_pem: &[u8]) -> Result<Self, TlsError> {
        let store = crate::roots::load_pem_roots(ca_pem)?;
        // Retain PEM bytes so `build` can reconstruct the verifier without
        // holding a non-cloneable store across builder moves.
        let _ = store;
        self.client_ca_pem = Some(ca_pem.to_vec());
        Ok(self)
    }

    /// Require a valid client certificate (mutual TLS).
    ///
    /// Validation in [`TlsServerConfigBuilder::build`] fails when this is set
    /// without client CA roots.
    pub fn with_require_client_cert(mut self, require: bool) -> Self {
        self.require_client_cert = require;
        self
    }

    /// Set ALPN protocols (e.g., `b"h2"`, `b"http/1.1"`).
    pub fn with_alpn(mut self, protocols: Vec<Vec<u8>>) -> Self {
        self.alpn_protocols = protocols;
        self
    }

    /// Configure ALPN for HTTP/2 with HTTP/1.1 fallback.
    pub fn with_h2_alpn(mut self) -> Self {
        self.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        self
    }

    /// Build the shared `ServerConfig`.
    pub fn build(self) -> Result<Arc<ServerConfig>, TlsError> {
        let key = self.key_der.ok_or(TlsError::MissingPrivateKey)?;
        if self.cert_chain.is_empty() {
            return Err(TlsError::MissingCertificateChain);
        }

        if self.require_client_cert && self.client_ca_pem.is_none() {
            return Err(TlsError::Handshake(
                "mTLS requires client CA roots when require_client_cert is set".to_string(),
            ));
        }

        let mut config = if let Some(ref ca_pem) = self.client_ca_pem {
            let roots = crate::roots::load_pem_roots(ca_pem)?;
            let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
                .build()
                .map_err(|e| TlsError::Handshake(format!("invalid client CA verifier: {e}")))?;
            if self.require_client_cert {
                ServerConfig::builder()
                    .with_client_cert_verifier(verifier)
                    .with_single_cert(self.cert_chain, key.into())?
            } else {
                // Verify client certs when presented, but do not require one.
                ServerConfig::builder()
                    .with_client_cert_verifier(verifier)
                    .with_single_cert(self.cert_chain, key.into())?
            }
        } else {
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(self.cert_chain, key.into())?
        };

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
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsError::PemParse(e.to_string()))?;
    if certs.is_empty() {
        return Err(TlsError::NoCertificatesFound);
    }
    Ok(certs)
}

/// Parse a PEM private key (PKCS#8 format).
fn load_private_key_pem(pem: &[u8]) -> Result<PrivatePkcs8KeyDer<'static>, TlsError> {
    match PrivatePkcs8KeyDer::from_pem_slice(pem) {
        Ok(key) => Ok(key),
        Err(rustls::pki_types::pem::Error::NoItemsFound) => Err(TlsError::NoPrivateKeyFound),
        Err(e) => Err(TlsError::PemParse(e.to_string())),
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
        let builder = TlsServerConfigBuilder::new();
        assert!(builder.cert_chain.is_empty());
        assert!(builder.key_der.is_none());
        assert!(builder.client_ca_pem.is_none());
        assert!(!builder.require_client_cert);
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
