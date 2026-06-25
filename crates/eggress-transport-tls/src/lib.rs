pub mod client;
pub mod error;
pub mod roots;
pub mod server;
pub mod transport;

pub use client::TlsClientConfigBuilder;
pub use error::TlsError;
pub use roots::{load_pem_certs, load_pem_roots, load_system_roots};
pub use server::TlsServerConfigBuilder;
pub use transport::{tls_accept, tls_connect};

/// Install the ring crypto provider if not already installed.
/// Safe to call multiple times — only the first call takes effect.
pub fn install_default_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(test)]
pub(crate) fn self_signed_cert() -> (String, String) {
    let cert_params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let cert_der = cert_params.self_signed(&key_pair).unwrap();
    (cert_der.pem(), key_pair.serialize_pem())
}
