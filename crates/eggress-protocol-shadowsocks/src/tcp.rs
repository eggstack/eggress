use tokio::io::AsyncWriteExt;

use crate::address::encode_address;
use crate::error::ShadowsocksError;
use crate::method::CipherMethod;
use eggress_core::{BoxStream, TargetAddr};

/// Maximum size for a single Shadowsocks data frame (payload length).
pub const MAX_FRAME_SIZE: usize = 65535;

/// Send a Shadowsocks TCP CONNECT request and return the upgraded stream.
///
/// This sends the encrypted target address header. Subsequent reads/writes
/// on the returned stream should be encrypted using the same method/key.
///
/// Note: This implementation sends the encrypted address header but does NOT
/// encrypt subsequent bidirectional data. Full stream encryption requires a
/// wrapping stream adapter which is left for future work.
pub async fn shadowsocks_connect(
    mut stream: BoxStream,
    target: &TargetAddr,
    method: CipherMethod,
    password: &str,
) -> Result<BoxStream, ShadowsocksError> {
    // Generate random salt
    use rand::RngCore;
    let mut salt = vec![0u8; method.salt_size()];
    rand::thread_rng().fill_bytes(&mut salt);

    // Derive subkey from password + salt
    let subkey = method.derive_key(password.as_bytes(), &salt);

    // Encode target address in Shadowsocks format
    let address = encode_address(target);

    // Encrypt the address header
    let nonce_bytes = vec![0u8; method.nonce_size()];
    let encrypted_addr = aead_encrypt_single(method, &subkey, &nonce_bytes, &address)?;

    // Build the initial payload: salt + encrypted_address
    let mut payload = Vec::with_capacity(salt.len() + encrypted_addr.len());
    payload.extend_from_slice(&salt);
    payload.extend_from_slice(&encrypted_addr);

    // Send to upstream
    stream.write_all(&payload).await?;
    stream.flush().await?;

    // TODO: For full Shadowsocks TCP, we need to wrap the stream with
    // encrypt/decrypt adapters. For now, return the stream as-is.
    // The address header has been sent, but subsequent data is unencrypted.

    Ok(stream)
}

/// Single-shot AEAD encryption (no additional data).
fn aead_encrypt_single(
    method: CipherMethod,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    use aes_gcm::{aead::Aead, Aes128Gcm, Aes256Gcm, KeyInit, Nonce};
    use chacha20poly1305::ChaCha20Poly1305;

    let nonce = Nonce::from_slice(nonce);

    match method {
        CipherMethod::Aes128Gcm => {
            let cipher = Aes128Gcm::new_from_slice(key)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))?;
            cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))
        }
        CipherMethod::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))?;
            cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))
        }
        CipherMethod::ChaCha20IetfPoly1305 => {
            let cipher = ChaCha20Poly1305::new_from_slice(key)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))?;
            cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| ShadowsocksError::DecryptionFailed(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::TargetHost;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_shadowsocks_connect_sends_payload() {
        // Start a TCP server that reads and echoes back the initial payload
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap();
            buf.truncate(n);
            // Echo back
            stream.write_all(&buf).await.unwrap();
        });

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };

        let mut conn =
            shadowsocks_connect(boxed, &target, CipherMethod::Aes256Gcm, "test-password")
                .await
                .unwrap();

        // Read back what we sent (salt + encrypted address)
        let mut response = vec![0u8; 4096];
        let n = conn.read(&mut response).await.unwrap();
        response.truncate(n);

        // Verify the response starts with a salt (16 bytes)
        assert!(response.len() > 16);

        // Verify the salt matches what we'd expect (16 bytes of random data)
        // The first 16 bytes are the salt, rest is encrypted address
        let expected_salt_size = CipherMethod::Aes256Gcm.salt_size();
        assert!(response.len() > expected_salt_size);

        server_jh.await.unwrap();
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

            let server_jh = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                stream.write_all(&buf[..n]).await.unwrap();
            });

            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let target = TargetAddr {
                host: TargetHost::Ip("93.184.216.34".parse().unwrap()),
                port: 80,
            };

            let result = shadowsocks_connect(boxed, &target, method, "password").await;
            assert!(result.is_ok(), "method {} failed", method);

            server_jh.await.unwrap();
        }
    }
}
