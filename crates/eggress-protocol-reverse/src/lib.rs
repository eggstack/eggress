use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub mod client;
pub mod metrics;
pub mod server;

/// Handshake response: accept.
pub const HANDSHAKE_ACCEPT: u8 = 0x01;

/// Handshake response: reject.
pub const HANDSHAKE_REJECT: u8 = 0x00;

/// Errors specific to the reverse protocol.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("authentication failed")]
    AuthFailed,
    #[error("authentication required")]
    AuthRequired,
    #[error("connection closed")]
    ConnectionClosed,
    #[error("bind address {0} is not in the allow_bind allowlist")]
    BindDenied(std::net::SocketAddr),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// State of a reverse control channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlState {
    Disconnected,
    Connecting,
    Authenticating,
    Ready,
    Draining,
    Closed,
}

/// Write auth credentials as raw bytes to a stream.
///
/// pproxy format: raw `user:pass` string bytes.
pub async fn write_auth(
    stream: &mut TcpStream,
    username: &str,
    password: &str,
) -> Result<(), ProtocolError> {
    let auth = format!("{}:{}", username, password);
    stream.write_all(auth.as_bytes()).await?;
    Ok(())
}

/// Read and validate the 1-byte handshake response.
pub async fn read_handshake(stream: &mut TcpStream) -> Result<(), ProtocolError> {
    let mut buf = [0u8; 1];
    stream.read_exact(&mut buf).await?;
    if buf[0] == HANDSHAKE_REJECT {
        return Err(ProtocolError::AuthFailed);
    }
    Ok(())
}

/// Write the 1-byte handshake response (accept).
pub async fn write_handshake_accept(stream: &mut TcpStream) -> Result<(), ProtocolError> {
    stream.write_all(&[HANDSHAKE_ACCEPT]).await?;
    Ok(())
}

/// Write the 1-byte handshake response (reject).
pub async fn write_handshake_reject(stream: &mut TcpStream) -> Result<(), ProtocolError> {
    stream.write_all(&[HANDSHAKE_REJECT]).await?;
    Ok(())
}

/// Perform the client-side auth handshake: send credentials, read response.
pub async fn client_auth_handshake(
    stream: &mut TcpStream,
    username: &str,
    password: &str,
) -> Result<(), ProtocolError> {
    write_auth(stream, username, password).await?;
    read_handshake(stream).await
}

/// Perform the server-side auth handshake: read credentials, validate, respond.
///
/// Returns the redacted auth representation `user:****` (never the password)
/// so callers can log it without leaking credentials. The full raw bytes are
/// only retained for the duration of the auth phase and then dropped.
pub async fn server_auth_handshake(
    stream: &mut TcpStream,
    expected_user: Option<&str>,
    expected_pass: Option<&str>,
) -> Result<String, ProtocolError> {
    // Read auth bytes (raw user:pass string)
    let mut auth_buf = BytesMut::with_capacity(1024);
    if auth_buf.capacity() < 1024 {
        auth_buf.reserve(1024);
    }
    let n = stream.read_buf(&mut auth_buf).await?;
    if n == 0 {
        return Err(ProtocolError::ConnectionClosed);
    }

    let auth_str = String::from_utf8_lossy(&auth_buf).to_string();

    // Validate if credentials are configured
    if let (Some(exp_user), Some(exp_pass)) = (expected_user, expected_pass) {
        let (user, pass) = parse_auth_str(&auth_str);
        if user != exp_user || pass != exp_pass {
            write_handshake_reject(stream).await?;
            return Err(ProtocolError::AuthFailed);
        }
    }

    write_handshake_accept(stream).await?;
    Ok(redact_auth(&auth_str))
}

/// Build a redacted form of an auth string suitable for logging.
///
/// Replaces the password with `****` while preserving the username. If the
/// string contains no `:`, the entire content is replaced with `****`.
pub fn redact_auth(auth: &str) -> String {
    let (user, _) = parse_auth_str(auth);
    if user.is_empty() && auth.is_empty() {
        return String::new();
    }
    if !auth.contains(':') {
        return "****".to_string();
    }
    format!("{}:****", user)
}

/// Parse a `user:pass` auth string.
fn parse_auth_str(auth: &str) -> (&str, &str) {
    match auth.find(':') {
        Some(idx) => (&auth[..idx], &auth[idx + 1..]),
        None => (auth, ""),
    }
}

/// Relay data bidirectionally between two TCP streams.
///
/// Takes ownership of both streams and relays until either side closes.
pub async fn relay_bidirectional(
    stream_a: TcpStream,
    stream_b: TcpStream,
) -> Result<(), ProtocolError> {
    let (mut a_read, mut a_write) = tokio::io::split(stream_a);
    let (mut b_read, mut b_write) = tokio::io::split(stream_b);

    let a_to_b = async {
        let mut buf = [0u8; 8192];
        loop {
            match a_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if b_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    };

    let b_to_a = async {
        let mut buf = [0u8; 8192];
        loop {
            match b_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if a_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    };

    tokio::select! {
        _ = a_to_b => {}
        _ = b_to_a => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_auth_str_normal() {
        let (user, pass) = parse_auth_str("user:pass");
        assert_eq!(user, "user");
        assert_eq!(pass, "pass");
    }

    #[test]
    fn parse_auth_str_empty() {
        let (user, pass) = parse_auth_str("");
        assert_eq!(user, "");
        assert_eq!(pass, "");
    }

    #[test]
    fn parse_auth_str_no_colon() {
        let (user, pass) = parse_auth_str("nocolon");
        assert_eq!(user, "nocolon");
        assert_eq!(pass, "");
    }

    #[test]
    fn parse_auth_str_multiple_colons() {
        let (user, pass) = parse_auth_str("user:pass:extra");
        assert_eq!(user, "user");
        assert_eq!(pass, "pass:extra");
    }

    #[test]
    fn redact_auth_basic() {
        assert_eq!(redact_auth("user:pass"), "user:****");
    }

    #[test]
    fn redact_auth_no_colon() {
        assert_eq!(redact_auth("opaque"), "****");
    }

    #[test]
    fn redact_auth_empty() {
        assert_eq!(redact_auth(""), "");
    }

    #[test]
    fn redact_auth_password_contains_colon() {
        // Multiple colons: only the first separates user/pass
        assert_eq!(redact_auth("user:p:a:s:s"), "user:****");
    }

    #[test]
    fn redact_auth_does_not_leak_password() {
        let s = redact_auth("user:supersecret123");
        assert!(!s.contains("supersecret"));
        assert!(!s.contains("secret"));
    }

    #[test]
    fn handshake_constants() {
        assert_eq!(HANDSHAKE_ACCEPT, 0x01);
        assert_eq!(HANDSHAKE_REJECT, 0x00);
    }

    #[test]
    fn control_state_variants() {
        let state = ControlState::Disconnected;
        assert_eq!(state, ControlState::Disconnected);

        let state = ControlState::Connecting;
        assert_eq!(state, ControlState::Connecting);

        let state = ControlState::Authenticating;
        assert_eq!(state, ControlState::Authenticating);

        let state = ControlState::Ready;
        assert_eq!(state, ControlState::Ready);

        let state = ControlState::Draining;
        assert_eq!(state, ControlState::Draining);

        let state = ControlState::Closed;
        assert_eq!(state, ControlState::Closed);
    }

    #[tokio::test]
    async fn auth_handshake_success() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let result = server_auth_handshake(&mut stream, Some("user"), Some("pass")).await;
            assert!(result.is_ok());
            // Returned form is redacted to avoid leaking the password
            assert_eq!(result.unwrap(), "user:****");
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let result = client_auth_handshake(&mut stream, "user", "pass").await;
        assert!(result.is_ok());

        server.await.unwrap();
    }

    #[tokio::test]
    async fn auth_handshake_failure() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let result = server_auth_handshake(&mut stream, Some("user"), Some("pass")).await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let result = client_auth_handshake(&mut stream, "user", "wrong").await;
        assert!(result.is_err());

        server.await.unwrap();
    }

    #[tokio::test]
    async fn auth_no_credentials_configured() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let result = server_auth_handshake(&mut stream, None, None).await;
            assert!(result.is_ok());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let result = client_auth_handshake(&mut stream, "", "").await;
        assert!(result.is_ok());

        server.await.unwrap();
    }

    #[tokio::test]
    async fn relay_bidirectional_data() {
        let listener_a = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_a = listener_a.local_addr().unwrap();
        let listener_b = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_b = listener_b.local_addr().unwrap();

        // Spawn connector for side A
        let conn_a =
            tokio::spawn(async move { tokio::net::TcpStream::connect(addr_a).await.unwrap() });

        // Spawn connector for side B
        let conn_b =
            tokio::spawn(async move { tokio::net::TcpStream::connect(addr_b).await.unwrap() });

        let (stream_a, _) = listener_a.accept().await.unwrap();
        let (stream_b, _) = listener_b.accept().await.unwrap();

        // Spawn relay
        let relay_handle = tokio::spawn(async move {
            let _ = relay_bidirectional(stream_a, stream_b).await;
        });

        let mut conn_a = conn_a.await.unwrap();
        let mut conn_b = conn_b.await.unwrap();

        // Write from A to B
        tokio::io::AsyncWriteExt::write_all(&mut conn_a, b"hello from A")
            .await
            .unwrap();

        // Read from B
        let mut buf = [0u8; 1024];
        let n = tokio::io::AsyncReadExt::read(&mut conn_b, &mut buf)
            .await
            .unwrap();
        assert_eq!(&buf[..n], b"hello from A");

        // Write from B to A
        tokio::io::AsyncWriteExt::write_all(&mut conn_b, b"hello from B")
            .await
            .unwrap();

        // Read from A
        let n = tokio::io::AsyncReadExt::read(&mut conn_a, &mut buf)
            .await
            .unwrap();
        assert_eq!(&buf[..n], b"hello from B");

        // Clean up
        drop(conn_a);
        drop(conn_b);
        let _ = relay_handle.await;
    }
}
