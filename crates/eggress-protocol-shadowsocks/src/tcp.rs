use std::sync::Arc;

use tokio::io::AsyncWriteExt;

use crate::address::{decode_address, encode_address};
use crate::error::ShadowsocksError;
use crate::method::CipherMethod;
use crate::metrics::ShadowsocksMetrics;
use crate::tcp_stream::ShadowsocksAeadStream;
use eggress_core::{BoxStream, TargetAddr};

/// Maximum size for a single Shadowsocks data frame (payload length).
pub const MAX_FRAME_SIZE: usize = 65535;

/// Send a Shadowsocks TCP CONNECT request and return the upgraded stream.
///
/// Sends the salt + encrypted address header (using the standard AEAD chunk
/// format: length block + payload block), then wraps the stream with a
/// bidirectional AEAD stream adapter for encrypting subsequent data.
///
/// Wire format after salt:
///   AEAD(len_u16_be, nonce=0)  — 18 bytes (encrypted address length)
///   AEAD(address, nonce=1)     — variable (encrypted address payload)
pub async fn shadowsocks_connect(
    mut stream: BoxStream,
    target: &TargetAddr,
    method: CipherMethod,
    password: &str,
    metrics: Option<Arc<ShadowsocksMetrics>>,
) -> Result<BoxStream, ShadowsocksError> {
    use rand::RngCore;
    let mut salt = vec![0u8; method.salt_size()];
    rand::thread_rng().fill_bytes(&mut salt);

    let subkey = method.derive_key(password.as_bytes(), &salt)?;

    let address = encode_address(target)?;

    // Encrypt address header using the standard chunk format (same as data chunks).
    // Nonce 0 for the length block, nonce 1 for the payload block.
    let addr_nonce = vec![0u8; method.nonce_size()];
    let addr_wire = crate::aead::encrypt_chunk_standard(method, &subkey, &addr_nonce, &address)?;

    let mut payload = Vec::with_capacity(salt.len() + addr_wire.len());
    payload.extend_from_slice(&salt);
    payload.extend_from_slice(&addr_wire);

    stream.write_all(&payload).await?;
    stream.flush().await?;

    if let Some(m) = metrics.as_ref() {
        m.record_tcp_upstream_session();
        m.record_tcp_flow_open();
    }

    // Data chunks start at nonce 2 (nonce 0+1 were used for the address header).
    Ok(Box::new(ShadowsocksAeadStream::new(stream, method, subkey)))
}

/// Server-side accept: read the salt, decrypt the address header (sent as a
/// standard AEAD chunk: length block + payload block), and return the wrapped
/// AEAD stream plus the target address.
///
/// Wire format after salt:
///   AEAD(len_u16_be, nonce=0)  — 18 bytes (encrypted address length)
///   AEAD(address, nonce=1)     — variable (encrypted address payload)
pub async fn shadowsocks_accept(
    mut stream: BoxStream,
    password: &str,
    method: CipherMethod,
    metrics: Option<Arc<ShadowsocksMetrics>>,
) -> Result<(BoxStream, TargetAddr), ShadowsocksError> {
    use crate::aead::aead_decrypt_raw;
    use tokio::io::AsyncReadExt;

    let mut salt = vec![0u8; method.salt_size()];
    stream.read_exact(&mut salt).await?;

    let subkey = method.derive_key(password.as_bytes(), &salt)?;
    let tag_size = method.tag_size();
    let len_block_size = 2 + tag_size; // 18 bytes

    // Read the 18-byte encrypted length block (nonce 0).
    let mut len_block = vec![0u8; len_block_size];
    stream.read_exact(&mut len_block).await?;

    let len_nonce = vec![0u8; method.nonce_size()];
    let len_plaintext = aead_decrypt_raw(method, &subkey, &len_nonce, &len_block).map_err(|e| {
        if let Some(m) = metrics.as_ref() {
            m.record_tcp_decrypt_failure();
        }
        ShadowsocksError::DecryptionFailed(e.to_string())
    })?;

    if len_plaintext.len() != 2 {
        if let Some(m) = metrics.as_ref() {
            m.record_tcp_frame_parse_failure();
        }
        return Err(ShadowsocksError::DecryptionFailed(
            "invalid length block plaintext".into(),
        ));
    }
    let addr_len = u16::from_be_bytes([len_plaintext[0], len_plaintext[1]]) as usize;

    // Read the address payload block (nonce 1).
    let mut addr_block = vec![0u8; addr_len + tag_size];
    stream.read_exact(&mut addr_block).await?;

    let payload_nonce_bytes = {
        let mut n = vec![0u8; method.nonce_size()];
        n[0] = 1; // little-endian nonce 1
        n
    };
    let address_plaintext = aead_decrypt_raw(method, &subkey, &payload_nonce_bytes, &addr_block)
        .map_err(|e| {
            if let Some(m) = metrics.as_ref() {
                m.record_tcp_decrypt_failure();
            }
            ShadowsocksError::DecryptionFailed(e.to_string())
        })?;

    let (target_addr, _consumed) = decode_address(&address_plaintext)?;

    if let Some(m) = metrics.as_ref() {
        m.record_tcp_session_accepted();
        m.record_tcp_flow_open();
    }

    // Data chunks start at nonce 2 (nonces 0+1 consumed by address header).
    Ok((
        Box::new(ShadowsocksAeadStream::new(stream, method, subkey)),
        target_addr,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::TargetHost;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_shadowsocks_connect_sends_payload() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let password = "test-password";
        let method = CipherMethod::Aes256Gcm;

        // Server: accept, read salt, decrypt address header, read AEAD data
        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (mut ss_stream, target) = shadowsocks_accept(boxed, password, method, None)
                .await
                .unwrap();

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

        let mut conn = shadowsocks_connect(boxed, &target, method, password, None)
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
                let (mut ss_stream, _target) = shadowsocks_accept(boxed, password, method, None)
                    .await
                    .unwrap();

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

            let mut conn = shadowsocks_connect(boxed, &target, method, password, None)
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let password = "test-password";
        let method = CipherMethod::Aes256Gcm;

        // Server: accept, read salt, decrypt header, wrap with AEAD stream, read data
        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (mut ss_stream, _target) = shadowsocks_accept(boxed, password, method, None)
                .await
                .unwrap();

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

        let mut conn = shadowsocks_connect(boxed, &target, method, password, None)
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let password = "roundtrip-password";
        let method = CipherMethod::ChaCha20IetfPoly1305;

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let (mut ss_stream, target) = shadowsocks_accept(boxed, password, method, None)
                .await
                .unwrap();

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

        let mut conn = shadowsocks_connect(boxed, &target, method, password, None)
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
                let (mut ss_stream, target) = shadowsocks_accept(boxed, password, method, None)
                    .await
                    .unwrap();

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

            let mut conn = shadowsocks_connect(boxed, &target, method, password, None)
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

    #[tokio::test]
    #[ignore = "requires ssserver binary"]
    async fn test_shadowsocks_connect_to_real_ssserver() {
        use std::process::Command;

        // Start TCP echo server
        let echo_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo_listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = echo_listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if stream.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        });

        // Find a free port for ssserver
        let ss_port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };

        // Start ssserver
        let mut child = Command::new("ssserver")
            .args([
                "-s",
                &format!("127.0.0.1:{ss_port}"),
                "-m",
                "aes-256-gcm",
                "-k",
                "testpass",
                "-v",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to start ssserver");

        // Wait for ssserver
        let mut ready = false;
        for _ in 0..50 {
            if tokio::net::TcpStream::connect(format!("127.0.0.1:{ss_port}"))
                .await
                .is_ok()
            {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(ready, "ssserver failed to start on port {ss_port}");

        // Connect to ssserver using our Shadowsocks client
        let stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{ss_port}"))
            .await
            .unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Ip(echo_addr.ip()),
            port: echo_addr.port(),
        };
        let method = CipherMethod::Aes256Gcm;

        let mut conn = shadowsocks_connect(boxed, &target, method, "testpass", None)
            .await
            .unwrap();

        // Write data and read response
        conn.write_all(b"hello").await.unwrap();
        conn.flush().await.unwrap();

        // Read with timeout — ssserver may not close the connection
        let mut buf = vec![0u8; 1024];
        let n = tokio::time::timeout(Duration::from_secs(2), conn.read(&mut buf))
            .await
            .expect("read timed out")
            .expect("read error");
        buf.truncate(n);
        assert_eq!(buf, b"hello");

        child.kill().ok();
        child.wait().ok();
    }
}
