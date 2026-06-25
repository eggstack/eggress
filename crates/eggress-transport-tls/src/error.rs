/// Errors from TLS operations.
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("TLS handshake failed: {0}")]
    Handshake(String),

    #[error("PEM parsing error: {0}")]
    PemParse(String),

    #[error("no certificates found in PEM data")]
    NoCertificatesFound,

    #[error("no private key found in PEM data")]
    NoPrivateKeyFound,

    #[error("private key is required but not provided")]
    MissingPrivateKey,

    #[error("certificate chain is required but not provided")]
    MissingCertificateChain,

    #[error("root certificate store error: {0}")]
    RootStore(String),

    #[error("invalid server name: {0}")]
    InvalidServerName(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<rustls::Error> for TlsError {
    fn from(e: rustls::Error) -> Self {
        TlsError::Handshake(e.to_string())
    }
}
