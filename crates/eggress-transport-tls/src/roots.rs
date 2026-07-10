use rustls::pki_types::{pem::PemObject, CertificateDer};
use rustls::RootCertStore;

use crate::error::TlsError;

/// Load system root certificates via webpki-roots.
pub fn load_system_roots() -> Result<RootCertStore, TlsError> {
    let mut store = RootCertStore::empty();
    store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().map(|r| r.to_owned()));
    Ok(store)
}

/// Load CA certificates from PEM bytes into a `RootCertStore`.
pub fn load_pem_roots(pem: &[u8]) -> Result<RootCertStore, TlsError> {
    let mut store = RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(pem) {
        let cert = cert.map_err(|e| TlsError::PemParse(e.to_string()))?;
        store
            .add(cert)
            .map_err(|e| TlsError::RootStore(e.to_string()))?;
    }
    Ok(store)
}

/// Load CA certificates from PEM bytes as raw `CertificateDer` values.
pub fn load_pem_certs(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsError::PemParse(e.to_string()))?;
    Ok(certs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        crate::install_default_crypto_provider();
    }

    #[test]
    fn system_roots_not_empty() {
        init();
        let store = load_system_roots().unwrap();
        assert!(!store.is_empty());
    }

    #[test]
    fn load_pem_roots_round_trip() {
        init();
        // Generate a self-signed cert, convert to DER, load as PEM root
        let cert_params = rcgen::CertificateParams::new(vec!["test-ca".to_string()]).unwrap();
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert_der = cert_params.self_signed(&key_pair).unwrap();
        let cert_pem = cert_der.pem();

        let store = load_pem_roots(cert_pem.as_bytes()).unwrap();
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn load_pem_roots_invalid_data() {
        let result = load_pem_roots(b"not a valid PEM");
        // Should not fail — just returns empty store (no certs found)
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn load_pem_certs_round_trip() {
        let cert_params = rcgen::CertificateParams::new(vec!["test-ca".to_string()]).unwrap();
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert_der = cert_params.self_signed(&key_pair).unwrap();
        let cert_pem = cert_der.pem();

        let certs = load_pem_certs(cert_pem.as_bytes()).unwrap();
        assert_eq!(certs.len(), 1);
    }
}
