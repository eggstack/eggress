//! Optional QUIC stream transport used by the pproxy compatibility path.
//!
//! The transport deliberately exposes streams and connection lifecycle
//! operations, rather than Quinn types, to the rest of Eggress. HTTP/3 uses
//! the opaque [`QuicConnection`] adapter when it needs to drive a session.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use eggress_core::BoxStream;
use quinn::crypto::rustls::{
    QuicClientConfig as QuinnTlsClientConfig, QuicServerConfig as QuinnTlsServerConfig,
};
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig};
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_MAX_STREAMS: u32 = 1024;

/// Upper bound on concurrently live per-connection tasks spawned by
/// [`QuicListener::run`]. Each accepted connection holds one permit for its
/// whole lifetime, so a flood of connections cannot multiply tasks without
/// limit.
const MAX_CONCURRENT_CONNECTION_TASKS: usize = 1024;

/// Transport errors with stable, redacted messages.
#[derive(Debug, thiserror::Error)]
pub enum QuicError {
    #[error("QUIC endpoint error: {0}")]
    Endpoint(String),
    #[error("QUIC connection error: {0}")]
    Connection(String),
    #[error("QUIC stream error: {0}")]
    Stream(String),
    #[error("QUIC TLS configuration error: {0}")]
    Tls(String),
    #[error("QUIC address resolution failed: {0}")]
    Resolve(String),
    #[error("QUIC server certificate and key are required")]
    MissingCertificate,
}

/// Client-side QUIC policy.
#[derive(Debug, Clone)]
pub struct QuicClientConfig {
    /// Server name used for TLS SNI and certificate verification.
    pub server_name: String,
    /// Compatibility-only certificate bypass. Native callers should leave this false.
    pub insecure: bool,
    /// QUIC idle timeout.
    pub idle_timeout: Duration,
    /// Maximum number of concurrent bidirectional streams advertised to the peer.
    pub max_concurrent_streams: u32,
    /// TLS ALPN values. HTTP/3 callers set this to `h3`; raw QUIC leaves it empty.
    pub alpn_protocols: Vec<Vec<u8>>,
}

impl Default for QuicClientConfig {
    fn default() -> Self {
        Self {
            server_name: String::new(),
            insecure: false,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            max_concurrent_streams: DEFAULT_MAX_STREAMS,
            alpn_protocols: Vec::new(),
        }
    }
}

/// Server-side QUIC policy and certificate material.
#[derive(Debug, Clone)]
pub struct QuicServerConfig {
    pub certificate_pem: Vec<u8>,
    pub private_key_pem: Vec<u8>,
    pub idle_timeout: Duration,
    pub max_concurrent_streams: u32,
    /// TLS ALPN values. HTTP/3 listeners set this to `h3`; raw QUIC leaves it empty.
    pub alpn_protocols: Vec<Vec<u8>>,
}

impl QuicServerConfig {
    fn server_config(&self) -> Result<ServerConfig, QuicError> {
        if self.certificate_pem.is_empty() || self.private_key_pem.is_empty() {
            return Err(QuicError::MissingCertificate);
        }
        let certificates: Vec<CertificateDer<'static>> =
            CertificateDer::pem_slice_iter(&self.certificate_pem)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| QuicError::Tls(e.to_string()))?;
        let key = PrivateKeyDer::from_pem_slice(&self.private_key_pem)
            .map_err(|e| QuicError::Tls(e.to_string()))?;
        let mut tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certificates, key)
            .map_err(|e| QuicError::Tls(e.to_string()))?;
        tls.alpn_protocols = self.alpn_protocols.clone();
        let crypto = QuinnTlsServerConfig::try_from(Arc::new(tls))
            .map_err(|e| QuicError::Tls(e.to_string()))?;
        let mut config = ServerConfig::with_crypto(Arc::new(crypto));
        let transport = Arc::get_mut(&mut config.transport).expect("new transport config");
        transport.max_concurrent_bidi_streams(self.max_concurrent_streams.into());
        transport.max_idle_timeout(Some(
            self.idle_timeout
                .try_into()
                .map_err(|_| QuicError::Tls("idle timeout is too large".to_string()))?,
        ));
        Ok(config)
    }
}

/// A bidirectional QUIC stream adapted to Eggress's boxed stream boundary.
pub struct QuicStream {
    inner: tokio::io::Join<quinn::RecvStream, quinn::SendStream>,
}

impl QuicStream {
    fn new(recv: quinn::RecvStream, send: quinn::SendStream) -> Self {
        Self {
            inner: tokio::io::join(recv, send),
        }
    }
}

impl tokio::io::AsyncRead for QuicStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for QuicStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Opaque established connection handle for HTTP/3 integration.
#[derive(Clone)]
pub struct QuicConnection {
    inner: Connection,
}

impl QuicConnection {
    /// Return the remote peer address.
    pub fn remote_address(&self) -> SocketAddr {
        self.inner.remote_address()
    }

    /// Close the connection and cancel all outstanding streams.
    pub fn close(&self, reason: &str) {
        self.inner.close(0u32.into(), reason.as_bytes());
    }

    /// Open a raw bidirectional stream.
    pub async fn open_stream(&self) -> Result<BoxStream, QuicError> {
        let (send, recv) = self
            .inner
            .open_bi()
            .await
            .map_err(|e| QuicError::Stream(e.to_string()))?;
        Ok(Box::new(QuicStream::new(recv, send)))
    }

    /// Accept the next raw bidirectional stream.
    pub async fn accept_stream(&self) -> Result<BoxStream, QuicError> {
        let (send, recv) = self
            .inner
            .accept_bi()
            .await
            .map_err(|e| QuicError::Stream(e.to_string()))?;
        Ok(Box::new(QuicStream::new(recv, send)))
    }

    /// Adapt this connection for the HTTP/3 crate without exposing Quinn to
    /// callers that only need the protocol transport boundary.
    pub fn into_h3(self) -> h3_quinn::Connection {
        h3_quinn::Connection::new(self.inner)
    }
}

/// Reusable client connection cache keyed by one configured remote endpoint.
pub struct QuicClient {
    endpoint: Endpoint,
    remote: SocketAddr,
    config: ClientConfig,
    server_name: String,
    connection: Mutex<Option<QuicConnection>>,
}

impl QuicClient {
    /// Resolve and construct a client. The endpoint is bound to an ephemeral UDP port.
    pub async fn connect(
        host: &str,
        port: u16,
        config: QuicClientConfig,
    ) -> Result<Arc<Self>, QuicError> {
        let remote = tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| QuicError::Resolve(e.to_string()))?
            .next()
            .ok_or_else(|| QuicError::Resolve("no addresses found".to_string()))?;
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().expect("valid ephemeral address"))
            .map_err(|e| QuicError::Endpoint(e.to_string()))?;
        let client_config = if config.insecure {
            #[cfg(feature = "insecure-quic")]
            {
                let mut tls = rustls::ClientConfig::builder()
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(InsecureVerifier))
                    .with_no_client_auth();
                tls.alpn_protocols = config.alpn_protocols.clone();
                let crypto = QuinnTlsClientConfig::try_from(Arc::new(tls))
                    .map_err(|e| QuicError::Tls(e.to_string()))?;
                ClientConfig::new(Arc::new(crypto))
            }
            #[cfg(not(feature = "insecure-quic"))]
            {
                return Err(QuicError::Tls(
                    "insecure QUIC requires the insecure-quic feature".to_string(),
                ));
            }
        } else {
            use rustls_platform_verifier::ConfigVerifierExt;
            let mut tls = rustls::ClientConfig::with_platform_verifier()
                .map_err(|e| QuicError::Tls(e.to_string()))?;
            tls.alpn_protocols = config.alpn_protocols.clone();
            let crypto = QuinnTlsClientConfig::try_from(Arc::new(tls))
                .map_err(|e| QuicError::Tls(e.to_string()))?;
            ClientConfig::new(Arc::new(crypto))
        };
        endpoint.set_default_client_config(client_config.clone());
        Ok(Arc::new(Self {
            endpoint,
            remote,
            config: client_config,
            server_name: if config.server_name.is_empty() {
                host.to_string()
            } else {
                config.server_name
            },
            connection: Mutex::new(None),
        }))
    }

    async fn connection(&self) -> Result<QuicConnection, QuicError> {
        let mut guard = self.connection.lock().await;
        if let Some(connection) = &*guard {
            return Ok(connection.clone());
        }
        let connecting = self
            .endpoint
            .connect_with(self.config.clone(), self.remote, &self.server_name)
            .map_err(|e| QuicError::Connection(e.to_string()))?;
        let connection = connecting
            .await
            .map_err(|e| QuicError::Connection(e.to_string()))?;
        let connection = QuicConnection { inner: connection };
        *guard = Some(connection.clone());
        Ok(connection)
    }

    /// Open a raw stream, reconnecting once after a terminated cached connection.
    pub async fn open_stream(&self) -> Result<BoxStream, QuicError> {
        let connection = self.connection().await?;
        match connection.open_stream().await {
            Ok(stream) => Ok(stream),
            Err(err) => {
                tracing::debug!(first_error = %err, "quic stream open failed, reconnecting");
                self.connection.lock().await.take();
                self.connection().await?.open_stream().await
            }
        }
    }

    /// Obtain the cached connection for a protocol session.
    pub async fn get_connection(&self) -> Result<QuicConnection, QuicError> {
        self.connection().await
    }

    /// Drop the cached connection so the next use reconnects.
    ///
    /// Protocol sessions call this when they detect a dead connection, mirroring
    /// the recovery path of [`QuicClient::open_stream`].
    pub async fn reset_connection(&self) {
        self.connection.lock().await.take();
    }

    /// Stop the endpoint and all cached connections.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }
}

/// A QUIC listener accepting independent bidirectional streams.
pub struct QuicListener {
    endpoint: Endpoint,
}

impl QuicListener {
    /// Bind a UDP QUIC listener. Certificate material is mandatory.
    pub async fn bind(bind: SocketAddr, config: QuicServerConfig) -> Result<Arc<Self>, QuicError> {
        let server_config = config.server_config()?;
        let endpoint = Endpoint::server(server_config, bind)
            .map_err(|e| QuicError::Endpoint(e.to_string()))?;
        Ok(Arc::new(Self { endpoint }))
    }

    /// Return the actual UDP address, including an OS-assigned port.
    pub fn local_addr(&self) -> Result<SocketAddr, QuicError> {
        self.endpoint
            .local_addr()
            .map_err(|e| QuicError::Endpoint(e.to_string()))
    }

    /// Accept connections and dispatch each bidirectional stream independently.
    pub async fn run<F, Fut>(
        self: Arc<Self>,
        cancel: CancellationToken,
        handler: F,
    ) -> Result<(), QuicError>
    where
        F: Fn(BoxStream, SocketAddr) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTION_TASKS));
        loop {
            let incoming = tokio::select! {
                incoming = self.endpoint.accept() => incoming,
                _ = cancel.cancelled() => break,
            };
            let Some(incoming) = incoming else { break };
            // Bound the number of concurrently live connection tasks. Waiting
            // here applies backpressure to accepts; select on cancellation so
            // shutdown is never delayed by a saturated permit pool.
            let permit = tokio::select! {
                permit = permits.clone().acquire_owned() => permit,
                _ = cancel.cancelled() => break,
            };
            let connection = match incoming.await {
                Ok(connection) => connection,
                Err(error) => {
                    tracing::debug!(%error, "QUIC connection handshake failed");
                    continue;
                }
            };
            let peer = connection.remote_address();
            let handler = handler.clone();
            tokio::spawn(async move {
                let _permit = permit;
                loop {
                    match connection.accept_bi().await {
                        Ok((send, recv)) => {
                            let handler = handler.clone();
                            tokio::spawn(async move {
                                handler(Box::new(QuicStream::new(recv, send)), peer).await;
                            });
                        }
                        Err(error) => {
                            tracing::debug!(%error, %peer, "QUIC connection stream loop ended");
                            break;
                        }
                    }
                }
            });
        }
        self.endpoint.close(0u32.into(), b"shutdown");
        Ok(())
    }

    /// Accept a connection for HTTP/3, leaving stream dispatch to the protocol layer.
    pub async fn accept_connection(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Option<QuicConnection>, QuicError> {
        let incoming = tokio::select! {
            incoming = self.endpoint.accept() => incoming,
            _ = cancel.cancelled() => return Ok(None),
        };
        let Some(incoming) = incoming else {
            return Ok(None);
        };
        let connection = incoming
            .await
            .map_err(|e| QuicError::Connection(e.to_string()))?;
        Ok(Some(QuicConnection { inner: connection }))
    }

    /// Stop accepting new connections.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }
}

#[cfg(feature = "insecure-quic")]
#[derive(Debug)]
struct InsecureVerifier;

#[cfg(feature = "insecure-quic")]
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
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

#[cfg(all(test, feature = "insecure-quic"))]
mod tests {
    use super::*;
    use rcgen::{CertificateParams, KeyPair};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn cert() -> (String, String) {
        let params = CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        let key = KeyPair::generate().unwrap();
        let certificate = params.self_signed(&key).unwrap();
        (certificate.pem(), key.serialize_pem())
    }

    #[tokio::test]
    async fn raw_streams_share_one_connection() {
        let (certificate, key) = cert();
        let listener = QuicListener::bind(
            "127.0.0.1:0".parse().unwrap(),
            QuicServerConfig {
                certificate_pem: certificate.into_bytes(),
                private_key_pem: key.into_bytes(),
                idle_timeout: DEFAULT_IDLE_TIMEOUT,
                max_concurrent_streams: DEFAULT_MAX_STREAMS,
                alpn_protocols: Vec::new(),
            },
        )
        .await
        .unwrap();
        let addr = listener.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let server = listener.clone();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            server
                .run(task_cancel, |mut stream, _peer| async move {
                    let mut data = [0u8; 5];
                    stream.read_exact(&mut data).await.unwrap();
                    stream.write_all(&data).await.unwrap();
                })
                .await
                .unwrap();
        });
        let client = QuicClient::connect(
            "127.0.0.1",
            addr.port(),
            QuicClientConfig {
                insecure: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let mut a = client.open_stream().await.unwrap();
        let mut b = client.open_stream().await.unwrap();
        a.write_all(b"hello").await.unwrap();
        b.write_all(b"world").await.unwrap();
        let mut out = [0u8; 5];
        a.read_exact(&mut out).await.unwrap();
        assert_eq!(&out, b"hello");
        b.read_exact(&mut out).await.unwrap();
        assert_eq!(&out, b"world");
        client
            .get_connection()
            .await
            .unwrap()
            .close("test reconnect");
        let mut c = client.open_stream().await.unwrap();
        c.write_all(b"again").await.unwrap();
        c.read_exact(&mut out).await.unwrap();
        assert_eq!(&out, b"again");
        cancel.cancel();
        task.await.unwrap();
    }
}
