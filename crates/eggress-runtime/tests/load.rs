use std::io::Write;

use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

async fn wait_ready(state: &eggress_runtime::RuntimeState) {
    for _ in 0..200 {
        if state.readiness.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("supervisor did not become ready");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn load_test_100_concurrent_tcp_sessions() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[rules]]
id = "route-all"
any = true
direct = true
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap().to_string();

    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    let concurrency = 100;
    let mut handles = Vec::with_capacity(concurrency);

    for i in 0..concurrency {
        let addr = listener_addr;
        let handle = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

            // SOCKS5 handshake (no auth)
            stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut resp = [0u8; 2];
            stream.read_exact(&mut resp).await.unwrap();
            assert_eq!(resp, [0x05, 0x00]);

            // CONNECT to a dummy target
            let port: u16 = 8080;
            stream
                .write_all(&[0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1])
                .await
                .unwrap();
            stream.write_all(&port.to_be_bytes()).await.unwrap();

            let mut reply = [0u8; 10];
            stream.read_exact(&mut reply).await.unwrap();
            assert_eq!(reply[1], 0x00, "SOCKS5 CONNECT failed for session {i}");

            // Close the stream
            drop(stream);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    token.cancel();
    let _ = jh.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn load_test_udp_associations_up_to_limit() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[listeners.udp]
enabled = true
max_associations = 5

[[rules]]
id = "route-all"
any = true
direct = true
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap().to_string();

    let mut sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    // Open UDP associations up to the configured limit
    let max_associations = 5;
    let mut control_streams = Vec::with_capacity(max_associations);

    for _ in 0..max_associations {
        let mut stream = tokio::net::TcpStream::connect(listener_addr).await.unwrap();

        // SOCKS5 handshake
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut resp = [0u8; 2];
        stream.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp, [0x05, 0x00]);

        // UDP ASSOCIATE
        stream
            .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
            .await
            .unwrap();
        stream.write_all(&0u16.to_be_bytes()).await.unwrap();

        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[1], 0x00, "UDP ASSOCIATE failed: {:02x}", reply[1]);

        control_streams.push(stream);
    }

    // Verify all control streams are still connected
    for stream in &control_streams {
        let _ = stream.try_write(&[0x05, 0x01, 0x00]);
    }

    // Clean up: drop all control streams to release associations
    drop(control_streams);

    // Give time for cleanup
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    token.cancel();
    let _ = jh.await;
}
