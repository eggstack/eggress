use std::cmp;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Buf as _, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::aead::{decrypt_chunk, encrypt_chunk};
use crate::method::CipherMethod;
use crate::nonce::NonceCounter;

/// Maximum plaintext payload per AEAD chunk.
///
/// Must leave room for the 2-byte length prefix inside the AEAD ciphertext
/// plus the 16-byte authentication tag, all of which must fit in a u16 on the
/// wire: `2 + payload + 16 <= u16::MAX`.
pub const MAX_CHUNK_PAYLOAD: usize = 65535 - 2 - 16;

/// Internal read state machine.
#[derive(Clone, Copy, Debug)]
enum ReadState {
    /// Waiting for the 2-byte ciphertext-length header.
    Header,
    /// Reading `len` bytes of ciphertext payload.
    Payload { len: usize },
}

/// Bidirectional AEAD stream adapter for Shadowsocks TCP.
///
/// Wraps an `AsyncRead + AsyncWrite` stream and encrypts/decrypts all data
/// using Shadowsocks AEAD chunk framing.
///
/// Wire format per chunk:
/// ```text
/// [2 bytes: ciphertext_length (plaintext, big-endian u16)]
/// [ciphertext_length bytes: AEAD(len_u16 + plaintext, nonce) → ciphertext + 16-byte tag]
/// ```
///
/// Read and write nonces are independent and both start at 1 (nonce 0 was
/// consumed by the address header).
pub struct ShadowsocksAeadStream<S> {
    inner: S,
    method: CipherMethod,
    subkey: Vec<u8>,
    write_nonce: NonceCounter,
    read_nonce: NonceCounter,
    read_plain: BytesMut,
    read_buf: BytesMut,
    read_state: ReadState,
    write_buf: BytesMut,
}

impl<S: AsyncRead + AsyncWrite + Unpin> ShadowsocksAeadStream<S> {
    pub fn new(inner: S, method: CipherMethod, subkey: Vec<u8>) -> Self {
        let nonce_size = method.nonce_size();
        Self {
            inner,
            method,
            subkey,
            write_nonce: NonceCounter::starting_at(nonce_size, 1),
            read_nonce: NonceCounter::starting_at(nonce_size, 1),
            read_plain: BytesMut::new(),
            read_buf: BytesMut::new(),
            read_state: ReadState::Header,
            write_buf: BytesMut::new(),
        }
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

/// Read bytes from `inner` into `buf` until `buf.len() >= target`.
///
/// Handles partial reads across `poll_read` calls. On `Pending`, already-read
/// bytes are appended to `buf` so they survive across poll invocations.
///
/// Returns `Poll::Ready(Ok(true))` if the target was reached,
/// `Poll::Ready(Ok(false))` on clean EOF (zero bytes read from inner),
/// or `Poll::Ready(Err(..))` on error / premature EOF.
fn read_until<S: AsyncRead + Unpin>(
    inner: &mut S,
    cx: &mut Context<'_>,
    buf: &mut BytesMut,
    target: usize,
) -> Poll<io::Result<bool>> {
    while buf.len() < target {
        let start = buf.len();
        buf.resize(target, 0);
        let mut rbuf = ReadBuf::new(&mut buf[start..target]);
        match Pin::new(&mut *inner).poll_read(cx, &mut rbuf) {
            Poll::Ready(Ok(())) => {
                let n = rbuf.filled().len();
                if n == 0 {
                    // Clean EOF — no more data from the inner stream.
                    buf.truncate(start);
                    return Poll::Ready(Ok(false));
                }
                // The bytes are already in `buf` through the slice. Just
                // update the length to reflect what was actually filled.
                buf.truncate(start + n);
            }
            Poll::Ready(Err(e)) => {
                buf.truncate(start);
                return Poll::Ready(Err(e));
            }
            Poll::Pending => {
                buf.truncate(start);
                return Poll::Pending;
            }
        }
    }
    Poll::Ready(Ok(true))
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for ShadowsocksAeadStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        // Drain any previously-buffered plaintext first.
        if !this.read_plain.is_empty() {
            let n = cmp::min(this.read_plain.len(), buf.remaining());
            buf.put_slice(&this.read_plain.split_to(n));
            return Poll::Ready(Ok(()));
        }

        // Drive the read state machine until we produce plaintext or stall.
        loop {
            let state = this.read_state;
            match state {
                ReadState::Header => {
                    // Read the 2-byte ciphertext-length header.
                    match read_until(&mut this.inner, cx, &mut this.read_buf, 2) {
                        Poll::Ready(Ok(true)) => {}
                        Poll::Ready(Ok(false)) => {
                            // Clean EOF — no more data from the inner stream.
                            this.read_buf.clear();
                            return Poll::Ready(Ok(()));
                        }
                        Poll::Ready(Err(e)) => {
                            this.read_buf.clear();
                            return Poll::Ready(Err(e));
                        }
                        Poll::Pending => return Poll::Pending,
                    }

                    let ct_len = u16::from_be_bytes([this.read_buf[0], this.read_buf[1]]) as usize;
                    this.read_buf.clear();

                    // A zero-length frame signals end-of-stream.
                    if ct_len == 0 {
                        return Poll::Ready(Ok(()));
                    }

                    this.read_state = ReadState::Payload { len: ct_len };
                }
                ReadState::Payload { len } => {
                    // Read `len` bytes of ciphertext.
                    match read_until(&mut this.inner, cx, &mut this.read_buf, len) {
                        Poll::Ready(Ok(true)) => {}
                        Poll::Ready(Ok(false)) | Poll::Ready(Err(_)) => {
                            this.read_buf.clear();
                            this.read_state = ReadState::Header;
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "unexpected EOF in payload",
                            )));
                        }
                        Poll::Pending => return Poll::Pending,
                    }

                    // Decrypt the ciphertext (which contains len_u16 + plaintext inside).
                    let nonce = this.read_nonce.current();
                    let plaintext =
                        decrypt_chunk(this.method, &this.subkey, &nonce, &this.read_buf)
                            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    this.read_nonce.advance().map_err(io::Error::other)?;

                    this.read_buf.clear();
                    this.read_state = ReadState::Header;
                    this.read_plain.extend_from_slice(&plaintext);

                    // We produced plaintext — drain it and return.
                    let n = cmp::min(this.read_plain.len(), buf.remaining());
                    buf.put_slice(&this.read_plain.split_to(n));
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for ShadowsocksAeadStream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();

        // Flush any leftover ciphertext buffered from a previous call.
        while !this.write_buf.is_empty() {
            match Pin::new(&mut this.inner).poll_write(cx, &this.write_buf) {
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "zero-byte write",
                    )));
                }
                Poll::Ready(Ok(n)) => this.write_buf.advance(n),
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        // Encrypt at most MAX_CHUNK_PAYLOAD bytes of plaintext.
        let chunk_size = cmp::min(buf.len(), MAX_CHUNK_PAYLOAD);
        if chunk_size == 0 {
            return Poll::Ready(Ok(0));
        }

        let nonce = this.write_nonce.current();
        let ct = encrypt_chunk(this.method, &this.subkey, &nonce, &buf[..chunk_size])
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        this.write_nonce.advance().map_err(io::Error::other)?;

        // Wire frame: [2-byte ciphertext_length] [ciphertext]
        let ct_len = ct.len() as u16;
        this.write_buf.extend_from_slice(&ct_len.to_be_bytes());
        this.write_buf.extend_from_slice(&ct);

        // Best-effort flush of the newly-buffered ciphertext.
        while !this.write_buf.is_empty() {
            match Pin::new(&mut this.inner).poll_write(cx, &this.write_buf) {
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "zero-byte write",
                    )));
                }
                Poll::Ready(Ok(n)) => this.write_buf.advance(n),
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => break,
            }
        }

        Poll::Ready(Ok(chunk_size))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        while !this.write_buf.is_empty() {
            match Pin::new(&mut this.inner).poll_write(cx, &this.write_buf) {
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "zero-byte write",
                    )));
                }
                Poll::Ready(Ok(n)) => this.write_buf.advance(n),
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        // Flush any remaining buffered ciphertext before shutting down.
        while !this.write_buf.is_empty() {
            match Pin::new(&mut this.inner).poll_write(cx, &this.write_buf) {
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "zero-byte write",
                    )));
                }
                Poll::Ready(Ok(n)) => this.write_buf.advance(n),
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn roundtrip_small_data() {
        let (client, server) = tokio::io::duplex(4096);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x42u8; 32];

        let mut client_stream = ShadowsocksAeadStream::new(client, method, subkey.clone());
        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey);

        // Write from client
        client_stream.write_all(b"hello").await.unwrap();
        client_stream.flush().await.unwrap();

        // Read from server
        let mut buf = vec![0u8; 64];
        let n = server_stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello");
    }

    #[tokio::test]
    async fn roundtrip_large_data() {
        let (client, server) = tokio::io::duplex(1 << 16);
        let method = CipherMethod::ChaCha20IetfPoly1305;
        let subkey = vec![0xABu8; 32];

        let payload = vec![0xCDu8; 100_000];
        let expected = payload.clone();
        let write_subkey = subkey.clone();

        let write_handle = tokio::spawn(async move {
            let mut client_stream = ShadowsocksAeadStream::new(client, method, write_subkey);
            client_stream.write_all(&payload).await.unwrap();
            client_stream.flush().await.unwrap();
        });

        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey);

        let mut received = Vec::new();
        server_stream.read_to_end(&mut received).await.unwrap();
        write_handle.await.unwrap();
        assert_eq!(received, expected);
    }

    #[tokio::test]
    async fn bidirectional_communication() {
        let (c1, s1) = tokio::io::duplex(4096);
        let (c2, s2) = tokio::io::duplex(4096);
        let method = CipherMethod::Aes128Gcm;
        let subkey = vec![0x11u8; 16];

        let mut client_a = ShadowsocksAeadStream::new(c1, method, subkey.clone());
        let mut server_a = ShadowsocksAeadStream::new(s1, method, subkey.clone());
        let mut client_b = ShadowsocksAeadStream::new(c2, method, subkey.clone());
        let mut server_b = ShadowsocksAeadStream::new(s2, method, subkey);

        // Client A -> Server A -> Client B -> Server B
        client_a.write_all(b"ping").await.unwrap();
        client_a.flush().await.unwrap();

        let mut buf = vec![0u8; 64];
        let n = server_a.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"ping");

        client_b.write_all(b"pong").await.unwrap();
        client_b.flush().await.unwrap();

        let n = server_b.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"pong");
    }

    #[tokio::test]
    async fn empty_read_on_eof() {
        let (client, server) = tokio::io::duplex(256);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x55u8; 32];

        let client_stream = ShadowsocksAeadStream::new(client, method, subkey.clone());
        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey);

        // Drop client to signal EOF
        drop(client_stream);

        let mut buf = vec![0u8; 64];
        let result = server_stream.read(&mut buf).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0); // EOF
    }

    #[tokio::test]
    async fn write_buffer_flushed_on_flush() {
        let (client, server) = tokio::io::duplex(4096);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x99u8; 32];

        let mut client_stream = ShadowsocksAeadStream::new(client, method, subkey.clone());
        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey);

        client_stream.write_all(b"data").await.unwrap();
        client_stream.flush().await.unwrap();

        let mut buf = vec![0u8; 64];
        let n = server_stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"data");
    }

    #[tokio::test]
    async fn multiple_chunks() {
        let (client, server) = tokio::io::duplex(8192);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x77u8; 32];

        let mut client_stream = ShadowsocksAeadStream::new(client, method, subkey.clone());
        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey);

        // Send multiple small writes; each becomes its own AEAD chunk.
        for i in 0..10 {
            let msg = format!("msg-{i}");
            client_stream.write_all(msg.as_bytes()).await.unwrap();
        }
        client_stream.flush().await.unwrap();
        drop(client_stream);

        let mut received = Vec::new();
        server_stream.read_to_end(&mut received).await.unwrap();

        let mut expected = Vec::new();
        for i in 0..10 {
            expected.extend_from_slice(format!("msg-{i}").as_bytes());
        }
        assert_eq!(received, expected);
    }

    #[tokio::test]
    async fn into_inner_returns_original_stream() {
        let (client, _server) = tokio::io::duplex(256);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x01u8; 32];

        let stream = ShadowsocksAeadStream::new(client, method, subkey);
        let _ = stream.into_inner();
    }

    #[tokio::test]
    async fn zero_length_frame_signals_eof() {
        let (client, server) = tokio::io::duplex(256);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x33u8; 32];

        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey.clone());

        // Manually send a zero-length frame (raw, not through the adapter).
        drop(ShadowsocksAeadStream::new(client, method, subkey));

        // The server should see EOF after the zero-length frame.
        let mut buf = vec![0u8; 64];
        let result = server_stream.read(&mut buf).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }
}
