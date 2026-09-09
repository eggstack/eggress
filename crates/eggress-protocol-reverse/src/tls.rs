//! TLS/mTLS configuration for native reverse control channels.
//!
//! Wraps the existing `eggress-transport-tls` builders so reverse control
//! traffic can be protected by Rustls without reverse-specific cryptography.
//! Plaintext remains available when no TLS config is present; TLS is applied
//! before reverse framing/authentication so credentials never cross in
//! plaintext when configured.

use std::sync::Arc;

/// TLS material for a native reverse server (control listener).
#[derive(Clone)]
pub struct ReverseServerTlsConfig {
    /// Server certificate chain PEM bytes.
    pub cert_pem: Vec<u8>,
    /// Server private key PEM bytes (PKCS#8).
    pub key_pem: Vec<u8>,
    /// Optional client CA roots PEM for mutual TLS.
    pub client_ca_pem: Option<Vec<u8>>,
    /// Require and validate a client certificate.
    pub require_client_cert: bool,
}

impl std::fmt::Debug for ReverseServerTlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print key material; presence flags only.
        f.debug_struct("ReverseServerTlsConfig")
            .field("has_cert", &!self.cert_pem.is_empty())
            .field("has_key", &!self.key_pem.is_empty())
            .field("has_client_ca", &self.client_ca_pem.is_some())
            .field("require_client_cert", &self.require_client_cert)
            .finish()
    }
}

impl Drop for ReverseServerTlsConfig {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.key_pem.zeroize();
        if let Some(ref mut ca) = self.client_ca_pem {
            ca.zeroize();
        }
    }
}

impl ReverseServerTlsConfig {
    /// Validate impossible combinations before runtime startup.
    pub fn validate(&self) -> Result<(), crate::ProtocolError> {
        if self.cert_pem.is_empty() {
            return Err(crate::ProtocolError::ConfigInvalid(
                "reverse server TLS requires a certificate".to_string(),
            ));
        }
        if self.key_pem.is_empty() {
            return Err(crate::ProtocolError::ConfigInvalid(
                "reverse server TLS requires a private key".to_string(),
            ));
        }
        if self.require_client_cert && self.client_ca_pem.is_none() {
            return Err(crate::ProtocolError::ConfigInvalid(
                "reverse server mTLS requires client CA roots when require_client_cert is set"
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// Build a shared rustls `ServerConfig` via the shared TLS transport.
    pub fn build_server_config(&self) -> Result<Arc<rustls::ServerConfig>, crate::ProtocolError> {
        self.validate()?;
        let mut builder = eggress_transport_tls::TlsServerConfigBuilder::new()
            .with_certificate_pem(&self.cert_pem)
            .map_err(|e| crate::ProtocolError::Tls(format!("invalid server certificate: {e}")))?
            .with_key_pem(&self.key_pem)
            .map_err(|e| crate::ProtocolError::Tls(format!("invalid server key: {e}")))?;
        if let Some(ref ca_pem) = self.client_ca_pem {
            builder = builder
                .with_client_ca_pem(ca_pem)
                .map_err(|e| crate::ProtocolError::Tls(format!("invalid client CA: {e}")))?;
        }
        if self.require_client_cert {
            builder = builder.with_require_client_cert(true);
        } else if self.client_ca_pem.is_some() {
            // Verify client certs when presented, but do not require one.
            // The builder treats any configured client CA as verify-when-present
            // unless require is set; no extra flag needed beyond the CA.
        }
        builder
            .build()
            .map_err(|e| crate::ProtocolError::Tls(format!("invalid server TLS config: {e}")))
    }
}

/// TLS material for a native reverse client (control dialer).
#[derive(Clone)]
pub struct ReverseClientTlsConfig {
    /// Optional custom CA roots PEM. When `None`, system roots are used,
    /// consistent with existing upstream TLS policy.
    pub ca_pem: Option<Vec<u8>>,
    /// SNI / server name for verification (required; server_addr is an IP
    /// literal so SNI cannot be derived safely).
    pub server_name: String,
    /// Optional client certificate PEM for mutual TLS.
    pub client_cert_pem: Option<Vec<u8>>,
    /// Optional client private key PEM for mutual TLS.
    pub client_key_pem: Option<Vec<u8>>,
}

impl std::fmt::Debug for ReverseClientTlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReverseClientTlsConfig")
            .field("has_ca", &self.ca_pem.is_some())
            .field("server_name", &self.server_name)
            .field("has_client_cert", &self.client_cert_pem.is_some())
            .field("has_client_key", &self.client_key_pem.is_some())
            .finish()
    }
}

impl Drop for ReverseClientTlsConfig {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        if let Some(ref mut key) = self.client_key_pem {
            key.zeroize();
        }
    }
}

impl ReverseClientTlsConfig {
    /// Validate impossible combinations before runtime startup.
    pub fn validate(&self) -> Result<(), crate::ProtocolError> {
        if self.server_name.is_empty() {
            return Err(crate::ProtocolError::ConfigInvalid(
                "reverse client TLS requires a server_name for SNI/verification".to_string(),
            ));
        }
        // rustls ServerName parsing is the authority for validity; fail here
        // rather than during the first reconnect attempt.
        let _ =
            rustls::pki_types::ServerName::try_from(self.server_name.clone()).map_err(|_| {
                crate::ProtocolError::ConfigInvalid(format!(
                    "reverse client TLS has an invalid server_name '{}'",
                    self.server_name
                ))
            })?;
        if self.client_cert_pem.is_some() != self.client_key_pem.is_some() {
            return Err(crate::ProtocolError::ConfigInvalid(
                "reverse client mTLS requires both client certificate and key".to_string(),
            ));
        }
        Ok(())
    }

    /// Build a shared rustls `ClientConfig` via the shared TLS transport.
    ///
    /// The returned config is immutable and cheap to clone (`Arc`); reconnect
    /// loops should reuse it rather than rebuilding per attempt.
    pub fn build_client_config(&self) -> Result<Arc<rustls::ClientConfig>, crate::ProtocolError> {
        self.validate()?;
        let mut builder = eggress_transport_tls::TlsClientConfigBuilder::new();
        builder = match self.ca_pem.as_deref() {
            Some(ca_pem) => builder
                .with_custom_ca_pem(ca_pem)
                .map_err(|e| crate::ProtocolError::Tls(format!("invalid client CA: {e}")))?,
            None => builder.with_system_roots().map_err(|e| {
                crate::ProtocolError::Tls(format!("TLS system roots unavailable: {e}"))
            })?,
        };
        if let (Some(cert_pem), Some(key_pem)) =
            (self.client_cert_pem.as_ref(), self.client_key_pem.as_ref())
        {
            builder = builder.with_client_cert_pem(cert_pem, key_pem);
        }
        builder
            .build()
            .map_err(|e| crate::ProtocolError::Tls(format!("invalid client TLS config: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_crypto() {
        eggress_transport_tls::install_default_crypto_provider();
    }

    fn cert_for(names: Vec<String>) -> (String, String) {
        let params = rcgen::CertificateParams::new(names).unwrap();
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        (cert.pem(), key.serialize_pem())
    }

    #[test]
    fn server_tls_debug_redacts_key_material() {
        let (cert, key) = cert_for(vec!["localhost".to_string()]);
        let cfg = ReverseServerTlsConfig {
            cert_pem: cert.into_bytes(),
            key_pem: key.into_bytes(),
            client_ca_pem: None,
            require_client_cert: false,
        };
        let rendered = format!("{cfg:?}");
        assert!(!rendered.contains("BEGIN PRIVATE KEY"));
        assert!(!rendered.contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn server_tls_require_without_ca_rejected() {
        let (cert, key) = cert_for(vec!["localhost".to_string()]);
        let cfg = ReverseServerTlsConfig {
            cert_pem: cert.into_bytes(),
            key_pem: key.into_bytes(),
            client_ca_pem: None,
            require_client_cert: true,
        };
        assert!(cfg.validate().is_err());
        assert!(cfg.build_server_config().is_err());
    }

    #[test]
    fn server_tls_malformed_pem_rejected() {
        let cfg = ReverseServerTlsConfig {
            cert_pem: b"not pem".to_vec(),
            key_pem: b"not pem".to_vec(),
            client_ca_pem: None,
            require_client_cert: false,
        };
        assert!(cfg.build_server_config().is_err());
    }

    #[test]
    fn client_tls_cert_without_key_rejected() {
        let (cert, _) = cert_for(vec!["localhost".to_string()]);
        let cfg = ReverseClientTlsConfig {
            ca_pem: None,
            server_name: "localhost".to_string(),
            client_cert_pem: Some(cert.into_bytes()),
            client_key_pem: None,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn client_tls_missing_server_name_rejected() {
        let cfg = ReverseClientTlsConfig {
            ca_pem: None,
            server_name: String::new(),
            client_cert_pem: None,
            client_key_pem: None,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn client_tls_debug_redacts_key_material() {
        init_crypto();
        let (cert, key) = cert_for(vec!["localhost".to_string()]);
        let cfg = ReverseClientTlsConfig {
            ca_pem: Some(cert.clone().into_bytes()),
            server_name: "localhost".to_string(),
            client_cert_pem: Some(cert.into_bytes()),
            client_key_pem: Some(key.into_bytes()),
        };
        let rendered = format!("{cfg:?}");
        assert!(!rendered.contains("BEGIN PRIVATE KEY"));
    }
}
