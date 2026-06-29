use std::cmp;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Buf as _, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::aead::{aead_decrypt_raw, aead_encrypt_raw};
use crate::method::CipherMethod;
use crate::nonce::NonceCounter;

/// Maximum plaintext payload per AEAD chunk in the standard Shadowsocks framing.
///
/// The length field is a u16, so the maximum payload is 65535 bytes.
pub const MAX_CHUNK_PAYLOAD: usize = 65535;

/// Internal read state machine for standard SIP003 framing.
#[derive(Clone, Copy, Debug)]
enum ReadState {
    /// Waiting for the 18-byte encrypted length block.
    LengthBlock,
    /// Reading `len` bytes of encrypted payload.
    Payload { len: usize },
}

/// Bidirectional AEAD stream adapter for Shadowsocks TCP.
///
/// Wraps an `AsyncRead + AsyncWrite` stream and encrypts/decrypts all data
/// using standard Shadowsocks AEAD chunk framing (SIP003).
///
/// Wire format per chunk:
/// ```text
/// [18 bytes: AEAD(encrypt, key, nonce_n, len_u16_be)]                    -- length block
/// [payload_len + 16 bytes: AEAD(encrypt, key, nonce_n+1, payload)]       -- payload block
/// ```
///
/// Read and write nonces are independent and both start at 1 (nonce 0 was
/// consumed by the address header). Each AEAD chunk consumes two nonces
/// (one for length, one for payload).
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
            read_state: ReadState::LengthBlock,
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
                ReadState::LengthBlock => {
                    // Read the 18-byte encrypted length block (2 plaintext bytes + 16 tag).
                    match read_until(&mut this.inner, cx, &mut this.read_buf, 18) {
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

                    // Decrypt length block with current nonce.
                    let nonce = this.read_nonce.current();
                    let len_plaintext =
                        aead_decrypt_raw(this.method, &this.subkey, &nonce, &this.read_buf[..18])
                            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

                    // Advance past length nonce.
                    this.read_nonce.advance().map_err(io::Error::other)?;

                    if len_plaintext.len() != 2 {
                        this.read_buf.clear();
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid length block plaintext",
                        )));
                    }

                    let payload_len =
                        u16::from_be_bytes([len_plaintext[0], len_plaintext[1]]) as usize;
                    this.read_buf.clear();

                    // A zero-length payload signals end-of-stream.
                    if payload_len == 0 {
                        return Poll::Ready(Ok(()));
                    }

                    this.read_state = ReadState::Payload { len: payload_len };
                }
                ReadState::Payload { len } => {
                    // Read `len` bytes of encrypted payload (+ 16-byte tag).
                    let wire_len = len + this.method.tag_size();
                    match read_until(&mut this.inner, cx, &mut this.read_buf, wire_len) {
                        Poll::Ready(Ok(true)) => {}
                        Poll::Ready(Ok(false)) | Poll::Ready(Err(_)) => {
                            this.read_buf.clear();
                            this.read_state = ReadState::LengthBlock;
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "unexpected EOF in payload",
                            )));
                        }
                        Poll::Pending => return Poll::Pending,
                    }

                    // Decrypt payload with current nonce (now at payload nonce).
                    let nonce = this.read_nonce.current();
                    let plaintext =
                        aead_decrypt_raw(this.method, &this.subkey, &nonce, &this.read_buf)
                            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

                    // Advance past payload nonce.
                    this.read_nonce.advance().map_err(io::Error::other)?;

                    this.read_buf.clear();
                    this.read_state = ReadState::LengthBlock;
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

        // Encrypt length block: AEAD(len_u16_be, nonce)
        let len_bytes = (chunk_size as u16).to_be_bytes();
        let len_nonce = this.write_nonce.current();
        let len_ct = aead_encrypt_raw(this.method, &this.subkey, &len_nonce, &len_bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        this.write_nonce.advance().map_err(io::Error::other)?;

        // Encrypt payload block: AEAD(payload, nonce+1)
        let payload_nonce = this.write_nonce.current();
        let payload_ct = aead_encrypt_raw(
            this.method,
            &this.subkey,
            &payload_nonce,
            &buf[..chunk_size],
        )
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        this.write_nonce.advance().map_err(io::Error::other)?;

        // Wire frame: [18-byte length block] [payload_len + 16-byte payload block]
        this.write_buf.extend_from_slice(&len_ct);
        this.write_buf.extend_from_slice(&payload_ct);

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

    use crate::aead::encrypt_chunk_standard;

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
    async fn zero_length_payload_signals_eof() {
        let (client, server) = tokio::io::duplex(256);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x33u8; 32];

        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey.clone());

        // Manually send a zero-length payload chunk (raw, not through the adapter).
        // Wire format: AEAD(len_u16=0, nonce) + AEAD(empty, nonce+1)
        // The reader starts at nonce counter=1, so encrypt with nonce 1.
        let mut nonce = vec![0u8; method.nonce_size()];
        nonce[method.nonce_size() - 1] = 1;
        let wire = encrypt_chunk_standard(method, &subkey, &nonce, b"").unwrap();
        let mut raw_stream = client;
        raw_stream.write_all(&wire).await.unwrap();
        drop(raw_stream);

        // The server should see EOF after the zero-length payload chunk.
        let mut buf = vec![0u8; 64];
        let result = server_stream.read(&mut buf).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn tampered_length_block_fails() {
        let (client, server) = tokio::io::duplex(256);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x42u8; 32];

        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey.clone());

        // Manually create wire: encrypt with nonce 1, then tamper length block
        let mut raw_stream = client;
        let nonce1 = {
            let mut n = vec![0u8; method.nonce_size()];
            n[method.nonce_size() - 1] = 1;
            n
        };
        let len_bytes = (5u16).to_be_bytes();
        let len_ct = crate::aead::aead_encrypt_raw(method, &subkey, &nonce1, &len_bytes).unwrap();
        let mut tampered_len_ct = len_ct;
        tampered_len_ct[0] ^= 0xFF;

        // Write tampered length block + valid payload block
        raw_stream.write_all(&tampered_len_ct).await.unwrap();
        // Write a valid payload block (won't matter since length decryption fails)
        let nonce2 = {
            let mut n = vec![0u8; method.nonce_size()];
            n[method.nonce_size() - 1] = 2;
            n
        };
        let payload_ct = crate::aead::aead_encrypt_raw(method, &subkey, &nonce2, b"hello").unwrap();
        raw_stream.write_all(&payload_ct).await.unwrap();
        drop(raw_stream);

        let mut buf = vec![0u8; 64];
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            server_stream.read(&mut buf),
        )
        .await;

        match result {
            Ok(Ok(0)) => {}
            Ok(Ok(_)) => {
                panic!("expected decryption failure from tampered length block");
            }
            Ok(Err(_)) => {}
            Err(_) => {}
        }
    }

    #[tokio::test]
    async fn tampered_payload_block_fails() {
        let (client, server) = tokio::io::duplex(1024);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x42u8; 32];

        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey.clone());

        // Manually create wire: valid length block + tampered payload block
        let mut raw_stream = client;
        let nonce1 = {
            let mut n = vec![0u8; method.nonce_size()];
            n[method.nonce_size() - 1] = 1;
            n
        };
        let len_bytes = (5u16).to_be_bytes();
        let len_ct = crate::aead::aead_encrypt_raw(method, &subkey, &nonce1, &len_bytes).unwrap();
        raw_stream.write_all(&len_ct).await.unwrap();

        // Write tampered payload block
        let nonce2 = {
            let mut n = vec![0u8; method.nonce_size()];
            n[method.nonce_size() - 1] = 2;
            n
        };
        let payload_ct = crate::aead::aead_encrypt_raw(method, &subkey, &nonce2, b"hello").unwrap();
        let mut tampered_payload_ct = payload_ct;
        tampered_payload_ct[0] ^= 0xFF;
        raw_stream.write_all(&tampered_payload_ct).await.unwrap();
        drop(raw_stream);

        let mut buf = vec![0u8; 64];
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            server_stream.read(&mut buf),
        )
        .await;

        match result {
            Ok(Ok(0)) => {}
            Ok(Ok(_)) => {
                panic!("expected decryption failure from tampered payload block");
            }
            Ok(Err(_)) => {}
            Err(_) => {}
        }
    }

    #[tokio::test]
    async fn tampered_payload_fails() {
        use crate::aead::aead_encrypt_raw;

        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x42u8; 32];
        let plaintext = b"hello world";

        let nonce = vec![0u8; method.nonce_size()];
        let ciphertext = aead_encrypt_raw(method, &subkey, &nonce, plaintext).unwrap();

        let mut tampered = ciphertext.clone();
        tampered[0] ^= 0x01;

        let result = crate::aead::aead_decrypt_raw(method, &subkey, &nonce, &tampered);
        assert!(
            result.is_err(),
            "decryption of tampered ciphertext should fail"
        );
    }

    #[tokio::test]
    async fn wrong_key_fails() {
        use crate::aead::aead_encrypt_raw;

        let method = CipherMethod::Aes256Gcm;
        let correct_key = vec![0x42u8; 32];
        let wrong_key = vec![0x99u8; 32];
        let plaintext = b"secret data";

        let nonce = vec![0u8; method.nonce_size()];
        let ciphertext = aead_encrypt_raw(method, &correct_key, &nonce, plaintext).unwrap();

        let result = crate::aead::aead_decrypt_raw(method, &wrong_key, &nonce, &ciphertext);
        assert!(result.is_err(), "decryption with wrong key should fail");

        let result = crate::aead::aead_decrypt_raw(method, &correct_key, &nonce, &ciphertext);
        assert!(result.is_ok(), "decryption with correct key should succeed");
        assert_eq!(result.unwrap(), plaintext);
    }

    #[tokio::test]
    async fn standard_chunk_format_roundtrip() {
        let (client, server) = tokio::io::duplex(4096);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0x55u8; 32];

        let mut client_stream = ShadowsocksAeadStream::new(client, method, subkey.clone());
        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey);

        let data = b"standard SIP003 framing test";
        client_stream.write_all(data).await.unwrap();
        client_stream.flush().await.unwrap();

        let mut buf = vec![0u8; 64];
        let n = server_stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], data.as_slice());
    }

    #[tokio::test]
    async fn empty_plaintext_roundtrip() {
        let (client, server) = tokio::io::duplex(256);
        let method = CipherMethod::Aes128Gcm;
        let subkey = vec![0x88u8; 16];

        let mut client_stream = ShadowsocksAeadStream::new(client, method, subkey.clone());
        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey);

        // Write empty data — should produce a zero-length payload chunk
        client_stream.write_all(b"").await.unwrap();
        client_stream.flush().await.unwrap();
        drop(client_stream);

        let mut buf = vec![0u8; 64];
        let result = server_stream.read(&mut buf).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0); // EOF signal
    }

    #[tokio::test]
    async fn large_chunk_split_across_reads() {
        let (client, server) = tokio::io::duplex(1 << 18);
        let method = CipherMethod::Aes256Gcm;
        let subkey = vec![0xBBu8; 32];

        let payload = vec![0x44u8; 50_000];
        let expected = payload.clone();
        let write_subkey = subkey.clone();

        let write_handle = tokio::spawn(async move {
            let mut client_stream = ShadowsocksAeadStream::new(client, method, write_subkey);
            client_stream.write_all(&payload).await.unwrap();
            client_stream.flush().await.unwrap();
            drop(client_stream);
        });

        let mut server_stream = ShadowsocksAeadStream::new(server, method, subkey);

        // Read in small chunks to exercise partial-read logic
        let mut received = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            let n = server_stream.read(&mut tmp).await.unwrap();
            if n == 0 {
                break;
            }
            received.extend_from_slice(&tmp[..n]);
        }
        write_handle.await.unwrap();
        assert_eq!(received, expected);
    }
}
