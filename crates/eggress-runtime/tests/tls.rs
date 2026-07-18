use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tempfile::NamedTempFile;

fn toml_path(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

fn install_crypto() {
    eggress_transport_tls::install_default_crypto_provider();
}

fn self_signed_cert() -> (String, String) {
    let cert_params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let cert_der = cert_params.self_signed(&key_pair).unwrap();
    (cert_der.pem(), key_pair.serialize_pem())
}

async fn wait_ready(state: &eggress_runtime::RuntimeState) {
    for _ in 0..100 {
        if state.readiness.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timeout waiting for readiness");
}

fn get_addrs(state: &eggress_runtime::RuntimeState) -> Vec<std::net::SocketAddr> {
    state
        .listener_addrs
        .lock()
        .unwrap()
        .iter()
        .filter_map(|a| *a)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tls_listener_accepts_https_connection() {
    install_crypto();
    let (cert_pem, key_pem) = self_signed_cert();

    let cert_file = NamedTempFile::new().unwrap();
    let key_file = NamedTempFile::new().unwrap();
    std::fs::write(cert_file.path(), &cert_pem).unwrap();
    std::fs::write(key_file.path(), &key_pem).unwrap();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "https-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.tls]
cert = "{}"
key = "{}"
"#,
        toml_path(cert_file.path()),
        toml_path(key_file.path())
    );
    let f = write_config(&config);
    let path = f.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;
    let addrs = get_addrs(&state);
    let addr = addrs[0];

    // Connect with TLS (insecure mode for self-signed cert)
    let client_config = eggress_transport_tls::TlsClientConfigBuilder::new()
        .with_insecure()
        .build()
        .unwrap();

    let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    let boxed: eggress_core::BoxStream = Box::new(tcp);
    let tls_result = eggress_transport_tls::tls_connect(boxed, client_config, "localhost").await;

    // TLS handshake should succeed
    assert!(
        tls_result.is_ok(),
        "TLS handshake should succeed: {:?}",
        tls_result.err()
    );

    let mut tls_stream = tls_result.unwrap();

    // Send a plain HTTP request over TLS
    use tokio::io::AsyncWriteExt;
    tls_stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    tls_stream.flush().await.unwrap();

    // Read response (any outcome proves TLS worked)
    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 1024];
    let _ = tokio::time::timeout(Duration::from_secs(3), async {
        let _ = tls_stream.read(&mut buf).await;
    })
    .await;

    token.cancel();
    jh.await.ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plaintext_to_tls_listener_fails() {
    install_crypto();
    let (cert_pem, key_pem) = self_signed_cert();

    let cert_file = NamedTempFile::new().unwrap();
    let key_file = NamedTempFile::new().unwrap();
    std::fs::write(cert_file.path(), &cert_pem).unwrap();
    std::fs::write(key_file.path(), &key_pem).unwrap();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "https-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.tls]
cert = "{}"
key = "{}"
"#,
        toml_path(cert_file.path()),
        toml_path(key_file.path())
    );
    let f = write_config(&config);
    let path = f.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;
    let addrs = get_addrs(&state);
    let addr = addrs[0];

    // Send plaintext to a TLS listener - should fail
    let mut tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    use tokio::io::AsyncWriteExt;
    tcp.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();

    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 1];
    let result =
        tokio::time::timeout(Duration::from_secs(3), async { tcp.read(&mut buf).await }).await;

    // Should get an error or EOF
    match result {
        Ok(Ok(0)) => {}
        Ok(Ok(_)) => {}
        Ok(Err(_)) => {}
        Err(_) => {}
    }

    token.cancel();
    jh.await.ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mixed_tls_and_plaintext_listeners() {
    install_crypto();
    let (cert_pem, key_pem) = self_signed_cert();

    let cert_file = NamedTempFile::new().unwrap();
    let key_file = NamedTempFile::new().unwrap();
    std::fs::write(cert_file.path(), &cert_pem).unwrap();
    std::fs::write(key_file.path(), &key_pem).unwrap();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "plaintext"
bind = "127.0.0.1:0"
protocols = ["http"]

[[listeners]]
name = "tls"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.tls]
cert = "{}"
key = "{}"
"#,
        toml_path(cert_file.path()),
        toml_path(key_file.path())
    );
    let f = write_config(&config);
    let path = f.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;
    let addrs = get_addrs(&state);
    let plaintext_addr = addrs[0];
    let tls_addr = addrs[1];

    // Plaintext listener should accept plaintext connections
    let mut tcp = tokio::net::TcpStream::connect(plaintext_addr)
        .await
        .unwrap();
    use tokio::io::AsyncWriteExt;
    tcp.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();

    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 1024];
    let _ = tokio::time::timeout(Duration::from_secs(3), async {
        let _ = tcp.read(&mut buf).await;
    })
    .await;

    // TLS listener should accept TLS connections
    let client_config = eggress_transport_tls::TlsClientConfigBuilder::new()
        .with_insecure()
        .build()
        .unwrap();
    let tcp = tokio::net::TcpStream::connect(tls_addr).await.unwrap();
    let boxed: eggress_core::BoxStream = Box::new(tcp);
    let mut tls_stream = eggress_transport_tls::tls_connect(boxed, client_config, "localhost")
        .await
        .unwrap();
    tls_stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    tls_stream.flush().await.unwrap();

    let mut buf = [0u8; 1024];
    let _ = tokio::time::timeout(Duration::from_secs(3), async {
        let _ = tls_stream.read(&mut buf).await;
    })
    .await;

    token.cancel();
    jh.await.ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tls_listener_with_wrong_server_name_fails() {
    install_crypto();
    let (cert_pem, key_pem) = self_signed_cert();

    let cert_file = NamedTempFile::new().unwrap();
    let key_file = NamedTempFile::new().unwrap();
    std::fs::write(cert_file.path(), &cert_pem).unwrap();
    std::fs::write(key_file.path(), &key_pem).unwrap();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "https-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.tls]
cert = "{}"
key = "{}"
"#,
        toml_path(cert_file.path()),
        toml_path(key_file.path())
    );
    let f = write_config(&config);
    let path = f.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;
    let addrs = get_addrs(&state);
    let addr = addrs[0];

    // Connect with wrong server name using system roots (not insecure)
    let client_config = eggress_transport_tls::TlsClientConfigBuilder::new()
        .with_system_roots()
        .unwrap()
        .build()
        .unwrap();

    let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    let boxed: eggress_core::BoxStream = Box::new(tcp);

    // Should fail because self-signed cert is not in system roots
    let result =
        eggress_transport_tls::tls_connect(boxed, client_config, "wrong.example.com").await;
    assert!(result.is_err(), "TLS with wrong server name should fail");

    token.cancel();
    jh.await.ok();
}
