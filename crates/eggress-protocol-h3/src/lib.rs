//! HTTP/3 CONNECT protocol adapters over the optional QUIC transport.

use std::sync::Arc;

use base64::Engine;
use bytes::{Buf, Bytes};
use eggress_core::BoxStream;
use eggress_transport_quic::{QuicClient, QuicConnection, QuicError};
use http::{Request, Response, StatusCode};
use subtle::ConstantTimeEq;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// HTTP/3 CONNECT errors.
#[derive(Debug, thiserror::Error)]
pub enum H3Error {
    #[error("QUIC transport error: {0}")]
    Quic(#[from] QuicError),
    #[error("HTTP/3 connection error: {0}")]
    Connection(String),
    #[error("HTTP/3 stream error: {0}")]
    Stream(String),
    #[error("HTTP/3 CONNECT rejected with status {0}")]
    Rejected(StatusCode),
    #[error("HTTP/3 request authority is missing or invalid")]
    InvalidAuthority,
    #[error("HTTP/3 request is not CONNECT")]
    InvalidMethod,
}

/// Details of an accepted H3 proxy request.
#[derive(Debug, Clone)]
pub struct H3Request {
    pub authority: String,
    pub headers: http::HeaderMap,
}

impl H3Request {
    /// Parse the CONNECT authority into an Eggress target.
    pub fn target(&self) -> Result<eggress_core::TargetAddr, H3Error> {
        self.authority
            .parse()
            .map_err(|_| H3Error::InvalidAuthority)
    }
}

/// An H3 client session sharing one QUIC connection across concurrent requests.
pub struct H3Client {
    quic: Arc<QuicClient>,
    session: Mutex<Option<Arc<H3Session>>>,
    authorization: Option<(String, String)>,
}

struct H3Session {
    sender: h3::client::SendRequest<h3_quinn::OpenStreams, Bytes>,
}

impl H3Client {
    pub fn new(quic: Arc<QuicClient>, authorization: Option<(String, String)>) -> Self {
        Self {
            quic,
            session: Mutex::new(None),
            authorization,
        }
    }

    async fn session(&self) -> Result<Arc<H3Session>, H3Error> {
        let mut guard = self.session.lock().await;
        if let Some(session) = &*guard {
            return Ok(session.clone());
        }
        let connection = self.quic.get_connection().await?;
        let (mut driver, sender) = h3::client::new(connection.into_h3())
            .await
            .map_err(|e| H3Error::Connection(e.to_string()))?;
        tokio::spawn(async move {
            let _ = driver.wait_idle().await;
        });
        let session = Arc::new(H3Session { sender });
        *guard = Some(session.clone());
        Ok(session)
    }

    /// Open a multiplexed HTTP/3 CONNECT stream.
    pub async fn connect(&self, target: &eggress_core::TargetAddr) -> Result<BoxStream, H3Error> {
        let session = self.session().await?;
        let authority = target.to_string();
        let mut request = Request::builder()
            .method(http::Method::CONNECT)
            .uri(format!("https://{authority}/"));
        if let Some((username, password)) = &self.authorization {
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
            request = request.header(
                http::header::PROXY_AUTHORIZATION,
                format!("Basic {encoded}"),
            );
        }
        let request = request
            .body(())
            .map_err(|e| H3Error::Connection(e.to_string()))?;
        let mut stream = session
            .sender
            .clone()
            .send_request(request)
            .await
            .map_err(|e| H3Error::Stream(e.to_string()))?;
        stream
            .finish()
            .await
            .map_err(|e| H3Error::Stream(e.to_string()))?;
        let response = stream
            .recv_response()
            .await
            .map_err(|e| H3Error::Stream(e.to_string()))?;
        if response.status() != StatusCode::OK {
            return Err(H3Error::Rejected(response.status()));
        }
        let (send, recv) = stream.split();
        Ok(bridge_client_stream(send, recv))
    }

    pub async fn close(&self) {
        self.session.lock().await.take();
        self.quic.close();
    }
}

fn h3_response(status: StatusCode) -> Response<()> {
    let mut response = Response::new(());
    *response.status_mut() = status;
    response
}

fn h3_auth_required_response() -> Response<()> {
    let mut response = h3_response(StatusCode::PROXY_AUTHENTICATION_REQUIRED);
    response.headers_mut().insert(
        http::header::PROXY_AUTHENTICATE,
        http::HeaderValue::from_static("Basic realm=\"eggress\""),
    );
    response
}

/// Serve all H3 CONNECT requests on one established QUIC connection.
pub async fn serve_connection<F, Fut>(
    connection: QuicConnection,
    cancel: CancellationToken,
    authorization: Option<(String, String)>,
    handler: F,
) -> Result<(), H3Error>
where
    F: Fn(H3Request, BoxStream, std::net::SocketAddr) -> Fut + Send + Sync + Clone + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let peer = connection.remote_address();
    let mut h3_connection = h3::server::builder()
        .build(connection.into_h3())
        .await
        .map_err(|e| H3Error::Connection(e.to_string()))?;
    loop {
        let resolver = tokio::select! {
            resolver = h3_connection.accept() => resolver,
            _ = cancel.cancelled() => break,
        }
        .map_err(|e| H3Error::Connection(e.to_string()))?;
        let Some(resolver) = resolver else { break };
        let (request, mut stream) = resolver
            .resolve_request()
            .await
            .map_err(|e| H3Error::Stream(e.to_string()))?;
        if request.method() != http::Method::CONNECT {
            let _ = stream
                .send_response(h3_response(StatusCode::METHOD_NOT_ALLOWED))
                .await;
            let _ = stream.finish().await;
            continue;
        }
        let authority = request
            .uri()
            .authority()
            .map(|authority| authority.as_str().to_string())
            .ok_or(H3Error::InvalidAuthority)?;
        let request = H3Request {
            authority,
            headers: request.headers().clone(),
        };
        if let Some((username, password)) = &authorization {
            let valid = request
                .headers
                .get(http::header::PROXY_AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_basic_authorization)
                .is_some_and(|(user, pass)| {
                    (user.as_bytes().ct_eq(username.as_bytes())
                        & pass.as_bytes().ct_eq(password.as_bytes()))
                    .unwrap_u8()
                        == 1
                });
            if !valid {
                let _ = stream.send_response(h3_auth_required_response()).await;
                let _ = stream.finish().await;
                continue;
            }
        }
        stream
            .send_response(h3_response(StatusCode::OK))
            .await
            .map_err(|e| H3Error::Stream(e.to_string()))?;
        let (send, recv) = stream.split();
        let local = bridge_server_stream(send, recv);
        let handler = handler.clone();
        tokio::spawn(async move {
            handler(request, local, peer).await;
        });
    }
    Ok(())
}

fn parse_basic_authorization(value: &str) -> Option<(String, String)> {
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())?;
    let (username, password) = decoded.split_once(':')?;
    Some((username.to_string(), password.to_string()))
}

fn bridge_client_stream<S, R>(
    mut send: h3::client::RequestStream<S, Bytes>,
    mut recv: h3::client::RequestStream<R, Bytes>,
) -> BoxStream
where
    S: h3::quic::SendStream<Bytes> + Send + 'static,
    R: h3::quic::RecvStream + Send + 'static,
{
    let (application, peer) = tokio::io::duplex(64 * 1024);
    let (mut application_reader, application_writer) = tokio::io::split(application);
    let (peer_reader, mut peer_writer) = tokio::io::split(peer);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            match application_reader.read(&mut buf).await {
                Ok(0) => {
                    let _ = send.finish().await;
                    break;
                }
                Ok(n) => {
                    if send
                        .send_data(Bytes::copy_from_slice(&buf[..n]))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    tokio::spawn(async move {
        while let Ok(Some(mut data)) = recv.recv_data().await {
            let bytes = data.copy_to_bytes(data.remaining());
            if peer_writer.write_all(&bytes).await.is_err() {
                return;
            }
        }
        let _ = peer_writer.shutdown().await;
    });
    Box::new(tokio::io::join(peer_reader, application_writer))
}

fn bridge_server_stream<S, R>(
    mut send: h3::server::RequestStream<S, Bytes>,
    mut recv: h3::server::RequestStream<R, Bytes>,
) -> BoxStream
where
    S: h3::quic::SendStream<Bytes> + Send + 'static,
    R: h3::quic::RecvStream + Send + 'static,
{
    let (application, peer) = tokio::io::duplex(64 * 1024);
    let (mut application_reader, application_writer) = tokio::io::split(application);
    let (peer_reader, mut peer_writer) = tokio::io::split(peer);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            match application_reader.read(&mut buf).await {
                Ok(0) => {
                    let _ = send.finish().await;
                    break;
                }
                Ok(n) => {
                    if send
                        .send_data(Bytes::copy_from_slice(&buf[..n]))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    tokio::spawn(async move {
        while let Ok(Some(mut data)) = recv.recv_data().await {
            let bytes = data.copy_to_bytes(data.remaining());
            if peer_writer.write_all(&bytes).await.is_err() {
                return;
            }
        }
        let _ = peer_writer.shutdown().await;
    });
    Box::new(tokio::io::join(peer_reader, application_writer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_transport_quic::{QuicClient, QuicClientConfig, QuicListener, QuicServerConfig};
    use rcgen::{CertificateParams, KeyPair};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_util::sync::CancellationToken;

    #[test]
    fn basic_authorization_is_deterministic() {
        assert_eq!(
            base64::engine::general_purpose::STANDARD.encode("user:pass"),
            "dXNlcjpwYXNz"
        );
    }

    #[tokio::test]
    async fn h3_connect_stream_round_trips_over_quic() {
        let params = CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        let key = KeyPair::generate().unwrap();
        let certificate = params.self_signed(&key).unwrap();
        let listener = QuicListener::bind(
            "127.0.0.1:0".parse().unwrap(),
            QuicServerConfig {
                certificate_pem: certificate.pem().into_bytes(),
                private_key_pem: key.serialize_pem().into_bytes(),
                idle_timeout: Duration::from_secs(60),
                max_concurrent_streams: 16,
                alpn_protocols: vec![b"h3".to_vec()],
            },
        )
        .await
        .unwrap();
        let cancel = CancellationToken::new();
        let server = listener.clone();
        let server_cancel = cancel.clone();
        let server_task = tokio::spawn(async move {
            let connection = server
                .accept_connection(&server_cancel)
                .await
                .unwrap()
                .unwrap();
            serve_connection(
                connection,
                server_cancel,
                None,
                |_, mut stream, _| async move {
                    let mut data = [0u8; 5];
                    stream.read_exact(&mut data).await.unwrap();
                    stream.write_all(&data).await.unwrap();
                },
            )
            .await
            .unwrap();
        });

        let address = listener.local_addr().unwrap();
        let client = QuicClient::connect(
            "127.0.0.1",
            address.port(),
            QuicClientConfig {
                insecure: true,
                alpn_protocols: vec![b"h3".to_vec()],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let mut stream = H3Client::new(client, None)
            .connect(&"example.com:443".parse().unwrap())
            .await
            .unwrap();
        stream.write_all(b"hello").await.unwrap();
        let mut output = [0u8; 5];
        stream.read_exact(&mut output).await.unwrap();
        assert_eq!(&output, b"hello");

        cancel.cancel();
        server_task.await.unwrap();
    }
}
