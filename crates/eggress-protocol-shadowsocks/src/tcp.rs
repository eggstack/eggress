use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::address::{decode_address, encode_address};
use crate::aead::aead_encrypt_raw;
use crate::error::ShadowsocksError;
use crate::method::CipherMethod;
use crate::tcp_stream::ShadowsocksAeadStream;
use eggress_core::{BoxStream, TargetAddr};

/// Maximum size for a single Shadowsocks data frame (payload length).
pub const MAX_FRAME_SIZE: usize = 65535;

/// Send a Shadowsocks TCP CONNECT request and return the upgraded stream.
///
/// Sends the salt + encrypted address header, then wraps the stream with
/// a bidirectional AEAD stream adapter for encrypting subsequent data.
pub async fn shadowsocks_connect(
    mut stream: BoxStream,
    target: &TargetAddr,
    method: CipherMethod,
    password: &str,
) -> Result<BoxStream, ShadowsocksError> {
    use rand::RngCore;
    let mut salt = vec![0u8; method.salt_size()];
    rand::thread_rng().fill_bytes(&mut salt);

    let subkey = method.derive_key(password.as_bytes(), &salt)?;

    let address = encode_address(target)?;
    let nonce_bytes = vec![0u8; method.nonce_size()];
    let encrypted_addr = aead_encrypt_raw(method, &subkey, &nonce_bytes, &address)?;

    let mut payload = Vec::with_capacity(salt.len() + encrypted_addr.len());
    payload.extend_from_slice(&salt);
    payload.extend_from_slice(&encrypted_addr);

    stream.write_all(&payload).await?;
    stream.flush().await?;

    Ok(Box::new(ShadowsocksAeadStream::new(stream, method, subkey)))
}

/// Server-side accept: read the salt, decrypt the address header, and return
/// the wrapped AEAD stream plus the target address.
///
/// Reads the 16-byte salt, then progressively reads ciphertext bytes and
/// attempts AEAD decryption to find the correct address header boundary.
/// Any bytes read beyond the address header ciphertext are prepended to the
/// returned stream so they can be consumed by the AEAD stream adapter.
pub async fn shadowsocks_accept(
    mut stream: BoxStream,
    password: &str,
    method: CipherMethod,
) -> Result<(BoxStream, TargetAddr), ShadowsocksError> {
    use crate::aead::aead_decrypt_raw;

    let mut salt = vec![0u8; method.salt_size()];
    stream.read_exact(&mut salt).await?;

    let subkey = method.derive_key(password.as_bytes(), &salt)?;

    let nonce_bytes = vec![0u8; method.nonce_size()];
    let tag_size = method.tag_size();

    // The address header plaintext is at most:
    //   ATYP(1) + domain_len(1) + max_domain(255) + port(2) = 259 bytes
    // So ciphertext is at most 259 + tag_size = 275 bytes.
    // Read up to 512 bytes to be safe.
    let mut buf = vec![0u8; 512];
    let mut buf_len = 0;

    // Progressive read + decrypt: try increasing ciphertext lengths until
    // AEAD decryption succeeds. Minimum address is IPv4: 7 bytes + 16 tag = 23.
    loop {
        if buf_len >= buf.len() {
            return Err(ShadowsocksError::DecryptionFailed(
                "address header too large".into(),
            ));
        }
        let n = stream.read(&mut buf[buf_len..]).await?;
        if n == 0 {
            return Err(ShadowsocksError::DecryptionFailed(
                "unexpected EOF reading address header".into(),
            ));
        }
        buf_len += n;

        // Try decrypting with increasing ciphertext lengths.
        // Minimum plaintext: ATYP(1) + len(1) + domain(1) + port(2) = 5 → min ciphertext = 5 + tag_size
        let min_ct = 5 + tag_size;
        for ct_len in min_ct..=buf_len {
            if let Ok(plaintext) = aead_decrypt_raw(method, &subkey, &nonce_bytes, &buf[..ct_len]) {
                let (target_addr, _consumed) = decode_address(&plaintext)?;
                let extra = buf[ct_len..buf_len].to_vec();
                let stream: BoxStream = if extra.is_empty() {
                    stream
                } else {
                    Box::new(PrependReader::new(stream, extra))
                };
                return Ok((
                    Box::new(ShadowsocksAeadStream::new(stream, method, subkey)),
                    target_addr,
                ));
            }
        }
    }
}

/// A reader that prepends already-read bytes before reading from the inner stream.
struct PrependReader<S> {
    inner: S,
    prefix: Vec<u8>,
}

impl<S: tokio::io::AsyncRead + Unpin> PrependReader<S> {
    fn new(inner: S, prefix: Vec<u8>) -> Self {
        Self { inner, prefix }
    }
}

impl<S: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for PrependReader<S> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if !this.prefix.is_empty() {
            let n = std::cmp::min(this.prefix.len(), buf.remaining());
            buf.put_slice(&this.prefix[..n]);
            this.prefix.drain(..n);
            return std::task::Poll::Ready(Ok(()));
        }
        let pinned = std::pin::Pin::new(&mut this.inner);
        pinned.poll_read(cx, buf)
    }
}

impl<S: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for PrependReader<S> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::TargetHost;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_shadowsocks_connect_sends_payload() {
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let password = "test-password";
        let method = CipherMethod::Aes256Gcm;

        // Server: accept, read salt, decrypt address header, read AEAD data
        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (mut ss_stream, target) =
                shadowsocks_accept(boxed, password, method).await.unwrap();

            assert_eq!(target.host, TargetHost::Domain("example.com".to_string()));
            assert_eq!(target.port, 443);

            let mut buf = vec![0u8; 4096];
            let n = ss_stream.read(&mut buf).await.unwrap();
            buf.truncate(n);
            buf
        });

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };

        let mut conn = shadowsocks_connect(boxed, &target, method, password)
            .await
            .unwrap();

        // Write through the AEAD stream — server should receive decrypted data
        conn.write_all(b"hello shadowsocks").await.unwrap();
        conn.flush().await.unwrap();

        let received = server_jh.await.unwrap();
        assert_eq!(received, b"hello shadowsocks");
    }

    #[tokio::test]
    async fn test_shadowsocks_connect_all_methods() {
        use tokio::io::AsyncWriteExt;

        let methods = [
            CipherMethod::Aes128Gcm,
            CipherMethod::Aes256Gcm,
            CipherMethod::ChaCha20IetfPoly1305,
        ];

        for method in methods {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();

            let password = "method-password";

            let server_jh = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let boxed: BoxStream = Box::new(stream);
                let (mut ss_stream, _target) =
                    shadowsocks_accept(boxed, password, method).await.unwrap();

                let mut buf = vec![0u8; 4096];
                let n = ss_stream.read(&mut buf).await.unwrap();
                buf.truncate(n);
                ss_stream.write_all(&buf).await.unwrap();
                ss_stream.flush().await.unwrap();
            });

            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let target = TargetAddr {
                host: TargetHost::Ip("93.184.216.34".parse().unwrap()),
                port: 80,
            };

            let mut conn = shadowsocks_connect(boxed, &target, method, password)
                .await
                .unwrap();

            conn.write_all(b"ping").await.unwrap();
            conn.flush().await.unwrap();

            let mut response = vec![0u8; 64];
            let n = conn.read(&mut response).await.unwrap();
            response.truncate(n);
            assert_eq!(response, b"ping", "method {} failed", method);

            server_jh.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_shadowsocks_connect_returns_aead_stream() {
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let password = "test-password";
        let method = CipherMethod::Aes256Gcm;

        // Server: accept, read salt, decrypt header, wrap with AEAD stream, read data
        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (mut ss_stream, _target) =
                shadowsocks_accept(boxed, password, method).await.unwrap();

            let mut buf = vec![0u8; 4096];
            let n = ss_stream.read(&mut buf).await.unwrap();
            buf.truncate(n);
            buf
        });

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };

        let mut conn = shadowsocks_connect(boxed, &target, method, password)
            .await
            .unwrap();

        // Write plaintext through the AEAD stream — it should be encrypted on the wire
        conn.write_all(b"hello world").await.unwrap();
        conn.flush().await.unwrap();

        let received = server_jh.await.unwrap();
        assert_eq!(received, b"hello world");
    }

    #[tokio::test]
    async fn test_shadowsocks_connect_and_accept_roundtrip() {
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let password = "roundtrip-password";
        let method = CipherMethod::ChaCha20IetfPoly1305;

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (mut ss_stream, target) =
                shadowsocks_accept(boxed, password, method).await.unwrap();

            // Verify the target address was decoded correctly
            assert_eq!(target.host, TargetHost::Ip("10.0.0.1".parse().unwrap()));
            assert_eq!(target.port, 8080);

            // Echo data back through the AEAD stream
            let mut buf = vec![0u8; 4096];
            let n = ss_stream.read(&mut buf).await.unwrap();
            buf.truncate(n);
            ss_stream.write_all(&buf).await.unwrap();
            ss_stream.flush().await.unwrap();
            buf
        });

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip("10.0.0.1".parse().unwrap()),
            port: 8080,
        };

        let mut conn = shadowsocks_connect(boxed, &target, method, password)
            .await
            .unwrap();

        // Send data, expect it echoed back encrypted
        conn.write_all(b"ping").await.unwrap();
        conn.flush().await.unwrap();

        let mut response = vec![0u8; 64];
        let n = conn.read(&mut response).await.unwrap();
        response.truncate(n);

        assert_eq!(response, b"ping");

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_all_methods_connect_accept() {
        let methods = [
            CipherMethod::Aes128Gcm,
            CipherMethod::Aes256Gcm,
            CipherMethod::ChaCha20IetfPoly1305,
        ];

        for method in methods {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();

            let password = "method-test-password";

            let server_jh = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let boxed: BoxStream = Box::new(stream);
                let (mut ss_stream, target) =
                    shadowsocks_accept(boxed, password, method).await.unwrap();

                assert_eq!(target.host, TargetHost::Domain("example.com".to_string()));
                assert_eq!(target.port, 443);

                let mut buf = vec![0u8; 4096];
                let n = ss_stream.read(&mut buf).await.unwrap();
                buf.truncate(n);
                ss_stream.write_all(&buf).await.unwrap();
                ss_stream.flush().await.unwrap();
            });

            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let target = TargetAddr {
                host: TargetHost::Domain("example.com".to_string()),
                port: 443,
            };

            let mut conn = shadowsocks_connect(boxed, &target, method, password)
                .await
                .unwrap();

            conn.write_all(b"test-data").await.unwrap();
            conn.flush().await.unwrap();

            let mut response = vec![0u8; 64];
            let n = conn.read(&mut response).await.unwrap();
            response.truncate(n);
            assert_eq!(response, b"test-data", "method {} failed", method);

            server_jh.await.unwrap();
        }
    }
}
