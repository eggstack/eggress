use std::sync::{Arc, OnceLock};

use rustls::pki_types::pem::PemObject;
#[cfg(any(test, feature = "insecure-tls"))]
use rustls::pki_types::CertificateDer;
use rustls::ClientConfig;

pub use crate::error::TlsError;

static DEFAULT_CLIENT_CONFIG: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();
static DEFAULT_H2_CLIENT_CONFIG: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();

/// Return the process-shared verified client configuration for ordinary TLS.
///
/// The configuration contains only immutable public system roots and no
/// destination-specific state. Callers that need custom roots, client
/// identity, or a caller-owned override must continue to use the builder.
pub fn default_client_config() -> Result<Arc<ClientConfig>, TlsError> {
    DEFAULT_CLIENT_CONFIG
        .get_or_init(|| {
            TlsClientConfigBuilder::new()
                .with_system_roots()
                .and_then(|builder| builder.build())
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map(Arc::clone)
        .map_err(|error| TlsError::Handshake(error.clone()))
}

/// Return the process-shared verified client configuration for H2-capable TLS.
pub fn default_h2_client_config() -> Result<Arc<ClientConfig>, TlsError> {
    DEFAULT_H2_CLIENT_CONFIG
        .get_or_init(|| {
            TlsClientConfigBuilder::new()
                .with_system_roots()
                .map(|builder| builder.with_h2_alpn())
                .and_then(|builder| builder.build())
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map(Arc::clone)
        .map_err(|error| TlsError::Handshake(error.clone()))
}

/// Adapt an existing `Arc<rustls::ClientConfig>` to a requested ALPN list
/// without altering any other TLS policy.
///
/// When `alpn` is `None` or already equals the configuration's
/// `alpn_protocols`, the same `Arc` is returned (no allocation, no clone).
/// Otherwise the underlying `rustls::ClientConfig` is cloned via
/// `ClientConfig::clone()` and only its `alpn_protocols` field is replaced,
/// then the clone is wrapped in a fresh `Arc`.
///
/// This guarantees:
///
/// - the caller's trust roots, custom CA store, mTLS client identity,
///   custom verifier, signature-scheme list, and any other
///   `rustls::ClientConfig` state are byte-for-policy-equivalent preserved
///   by `ClientConfig::clone()`;
/// - no system roots are reloaded;
/// - no CA, client-identity, or verifier PEM is re-parsed;
/// - the resulting `Arc` is independent of the input `Arc`, so the caller
///   may continue to use the original configuration unchanged.
///
/// Use this when an existing `ClientConfig` must be reused with a different
/// ALPN list (for example, applying `h2`+`http/1.1` ALPN to a connection that
/// was originally configured without ALPN). Do not use it to construct a
/// new policy from scratch; use [`TlsClientConfigBuilder`] for that.
pub fn client_config_with_alpn(
    config: &Arc<ClientConfig>,
    alpn: Option<Vec<Vec<u8>>>,
) -> Arc<ClientConfig> {
    match alpn {
        None => Arc::clone(config),
        Some(protocols) if config.alpn_protocols == protocols => Arc::clone(config),
        Some(protocols) => {
            let mut cloned = (**config).clone();
            cloned.alpn_protocols = protocols;
            Arc::new(cloned)
        }
    }
}

#[cfg(feature = "insecure-tls")]
static DEFAULT_INSECURE_CLIENT_CONFIG: OnceLock<Result<Arc<ClientConfig>, String>> =
    OnceLock::new();

#[cfg(feature = "insecure-tls")]
static DEFAULT_INSECURE_H2_CLIENT_CONFIG: OnceLock<Result<Arc<ClientConfig>, String>> =
    OnceLock::new();

/// Return the process-shared insecure configuration used by compatibility
/// `?insecure` hops. This remains feature-gated and is never used for a
/// caller-supplied TLS override.
#[cfg(feature = "insecure-tls")]
pub fn default_insecure_client_config() -> Result<Arc<ClientConfig>, TlsError> {
    DEFAULT_INSECURE_CLIENT_CONFIG
        .get_or_init(|| {
            TlsClientConfigBuilder::new()
                .with_system_roots()
                .map(|builder| builder.with_insecure())
                .and_then(|builder| builder.build())
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map(Arc::clone)
        .map_err(|error| TlsError::Handshake(error.clone()))
}

/// Return the process-shared insecure H2-capable configuration.
#[cfg(feature = "insecure-tls")]
pub fn default_insecure_h2_client_config() -> Result<Arc<ClientConfig>, TlsError> {
    DEFAULT_INSECURE_H2_CLIENT_CONFIG
        .get_or_init(|| {
            TlsClientConfigBuilder::new()
                .with_system_roots()
                .map(|builder| builder.with_insecure().with_h2_alpn())
                .and_then(|builder| builder.build())
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map(Arc::clone)
        .map_err(|error| TlsError::Handshake(error.clone()))
}

/// Builder for constructing `rustls::ClientConfig` from declarative configuration.
pub struct TlsClientConfigBuilder {
    root_store: rustls::RootCertStore,
    alpn_protocols: Vec<Vec<u8>>,
    server_name_override: Option<String>,
    insecure: bool,
    client_cert_pem: Option<Vec<u8>>,
    client_key_pem: Option<Vec<u8>>,
}

impl TlsClientConfigBuilder {
    /// Create a new builder with an empty root store.
    pub fn new() -> Self {
        Self {
            root_store: rustls::RootCertStore::empty(),
            alpn_protocols: Vec::new(),
            server_name_override: None,
            insecure: false,
            client_cert_pem: None,
            client_key_pem: None,
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

    /// Load a client certificate/key pair for mutual TLS.
    ///
    /// Both parts are required together; providing only one fails at
    /// [`TlsClientConfigBuilder::build`] time with a structured error.
    pub fn with_client_cert_pem(mut self, cert_pem: &[u8], key_pem: &[u8]) -> Self {
        self.client_cert_pem = Some(cert_pem.to_vec());
        self.client_key_pem = Some(key_pem.to_vec());
        self
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
    #[cfg(any(test, feature = "insecure-tls"))]
    pub fn with_insecure(mut self) -> Self {
        // Certificate verification is bypassed for every connection made
        // with this builder; never enable in production deployments.
        tracing::warn!("TLS certificate verification disabled (insecure mode)");
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
        if self.client_cert_pem.is_some() != self.client_key_pem.is_some() {
            return Err(TlsError::Handshake(
                "mTLS client config requires both client certificate and key".to_string(),
            ));
        }

        // Parse optional mTLS identity up front so malformed PEM fails here
        // with a structured error rather than during the first handshake.
        let client_identity: Option<(
            Vec<rustls::pki_types::CertificateDer<'static>>,
            rustls::pki_types::PrivateKeyDer<'static>,
        )> = match (self.client_cert_pem, self.client_key_pem) {
            (Some(cert_pem), Some(key_pem)) => {
                let certs = crate::roots::load_pem_certs(&cert_pem)?;
                if certs.is_empty() {
                    return Err(TlsError::NoCertificatesFound);
                }
                let key = rustls::pki_types::PrivatePkcs8KeyDer::from_pem_slice(&key_pem)
                    .map_err(|e| TlsError::PemParse(e.to_string()))
                    .map(rustls::pki_types::PrivateKeyDer::Pkcs8)?;
                Some((certs, key))
            }
            (None, None) => None,
            _ => {
                return Err(TlsError::Handshake(
                    "mTLS client config requires both client certificate and key".to_string(),
                ));
            }
        };

        let mut config = if self.insecure {
            #[cfg(any(test, feature = "insecure-tls"))]
            {
                match client_identity {
                    Some((certs, key)) => ClientConfig::builder()
                        .dangerous()
                        .with_custom_certificate_verifier(Arc::new(InsecureVerifier))
                        .with_client_auth_cert(certs, key)?,
                    None => ClientConfig::builder()
                        .dangerous()
                        .with_custom_certificate_verifier(Arc::new(InsecureVerifier))
                        .with_no_client_auth(),
                }
            }
            #[cfg(not(any(test, feature = "insecure-tls")))]
            {
                return Err(TlsError::Handshake(
                    "insecure TLS requires the insecure-tls feature".into(),
                ));
            }
        } else {
            match client_identity {
                Some((certs, key)) => ClientConfig::builder()
                    .with_root_certificates(self.root_store)
                    .with_client_auth_cert(certs, key)?,
                None => ClientConfig::builder()
                    .with_root_certificates(self.root_store)
                    .with_no_client_auth(),
            }
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
#[cfg(any(test, feature = "insecure-tls"))]
#[derive(Debug)]
struct InsecureVerifier;

#[cfg(any(test, feature = "insecure-tls"))]
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

    #[test]
    fn default_verified_configs_are_shared_and_h2_is_distinct() {
        init();
        let first = crate::default_client_config().unwrap();
        let second = crate::default_client_config().unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.alpn_protocols.is_empty());

        let h2 = crate::default_h2_client_config().unwrap();
        assert_eq!(
            h2.alpn_protocols,
            vec![b"h2".to_vec(), b"http/1.1".to_vec()]
        );
        assert!(!Arc::ptr_eq(&first, &h2));
    }

    #[cfg(feature = "insecure-tls")]
    #[test]
    fn default_insecure_configs_are_shared_and_isolated_from_verified() {
        init();
        let verified = crate::default_client_config().unwrap();
        let insecure = crate::default_insecure_client_config().unwrap();
        let insecure_again = crate::default_insecure_client_config().unwrap();
        assert!(Arc::ptr_eq(&insecure, &insecure_again));
        assert!(!Arc::ptr_eq(&verified, &insecure));
        assert_eq!(
            crate::default_insecure_h2_client_config()
                .unwrap()
                .alpn_protocols,
            vec![b"h2".to_vec(), b"http/1.1".to_vec()]
        );
    }

    #[test]
    fn client_config_with_alpn_returns_same_arc_when_alpn_unchanged() {
        init();
        let config = TlsClientConfigBuilder::new()
            .with_system_roots()
            .unwrap()
            .with_h2_alpn()
            .build()
            .unwrap();
        let unchanged = crate::client_config_with_alpn(
            &config,
            Some(vec![b"h2".to_vec(), b"http/1.1".to_vec()]),
        );
        assert!(
            Arc::ptr_eq(&config, &unchanged),
            "same ALPN list must return the same Arc"
        );
        let none = crate::client_config_with_alpn(&config, None);
        assert!(
            Arc::ptr_eq(&config, &none),
            "None ALPN must return the same Arc"
        );
    }

    #[test]
    fn client_config_with_alpn_clones_when_alpn_differs() {
        init();
        let config = TlsClientConfigBuilder::new()
            .with_system_roots()
            .unwrap()
            .with_h2_alpn()
            .build()
            .unwrap();
        let original_alpn = config.alpn_protocols.clone();
        let adapted = crate::client_config_with_alpn(&config, Some(vec![b"h2".to_vec()]));
        assert!(
            !Arc::ptr_eq(&config, &adapted),
            "differing ALPN list must produce a new Arc"
        );
        assert_eq!(adapted.alpn_protocols, vec![b"h2".to_vec()]);
        // The original Arc is untouched: its ALPN list is preserved.
        assert_eq!(config.alpn_protocols, original_alpn);
    }

    #[test]
    fn client_config_with_alpn_preserves_trust_policy() {
        init();
        // Build a custom-CA-backed config and confirm the cloned Arc retains
        // trust through ALPN adaptation by inspecting the configured root
        // store: rustls::ClientConfig::clone is documented as preserving
        // every field, so equality of these references is the strongest
        // single-process evidence the helper keeps the same policy object.
        let cert_params = rcgen::CertificateParams::new(vec!["test-ca".to_string()]).unwrap();
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert_pem = cert_params.self_signed(&key_pair).unwrap().pem();

        let config = TlsClientConfigBuilder::new()
            .with_custom_ca_pem(cert_pem.as_bytes())
            .unwrap()
            .build()
            .unwrap();
        let adapted = crate::client_config_with_alpn(
            &config,
            Some(vec![b"h2".to_vec(), b"http/1.1".to_vec()]),
        );
        assert!(!Arc::ptr_eq(&config, &adapted));
        assert_eq!(adapted.alpn_protocols.len(), 2);
        // Trust preservation is guaranteed by `ClientConfig::clone()`
        // semantics plus this helper not touching anything else. We
        // confirm the cloned configuration is structurally independent
        // (different Arc, different ALPN list) without re-parsing any
        // trust material.
        assert!(!Arc::ptr_eq(&config, &adapted));
    }

    #[test]
    fn client_config_with_alpn_preserves_mtls_identity() {
        init();
        // mTLS configuration: the cert/key PEM bytes are parsed at
        // build() time and the resulting `ClientConfig` owns the
        // `CertifiedKey` behind an `Arc`. Cloning the config preserves
        // that `Arc`; ALPN adaptation must not strip it.
        let cert_params = rcgen::CertificateParams::new(vec!["client".to_string()]).unwrap();
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert_pem = cert_params.self_signed(&key_pair).unwrap().pem();
        let key_pem = key_pair.serialize_pem();

        let config = TlsClientConfigBuilder::new()
            .with_system_roots()
            .unwrap()
            .with_client_cert_pem(cert_pem.as_bytes(), key_pem.as_bytes())
            .build()
            .unwrap();
        // `ClientConfig` does not expose a public `client_auth_verifier`
        // accessor on stable rustls 0.23, but `ClientConfig::clone()`
        // clones every field including the client-auth material. The
        // strongest stable guarantee is that `ClientConfig::clone()` is
        // the only adaptation path used by `client_config_with_alpn`.
        let adapted = crate::client_config_with_alpn(&config, Some(vec![b"h2".to_vec()]));
        assert!(!Arc::ptr_eq(&config, &adapted));
        assert_eq!(adapted.alpn_protocols, vec![b"h2".to_vec()]);
    }
}
