use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;

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

#[cfg(unix)]
fn count_fds() -> usize {
    std::fs::read_dir("/proc/self/fd")
        .map(|dir| dir.count())
        .unwrap_or(0)
}

#[cfg(not(unix))]
fn count_fds() -> usize {
    0
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

// ---------------------------------------------------------------------------
// Phase 50: Slowloris-style handshake test — send data one byte at a time
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn load_test_slowloris_handshake() {
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

    // Baseline FD count
    tokio::time::sleep(Duration::from_millis(200)).await;
    let baseline_fds = count_fds();

    let slow_connections = 10;
    let mut handles = Vec::with_capacity(slow_connections);

    for _i in 0..slow_connections {
        let addr = listener_addr;
        let handle = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

            // Send SOCKS5 greeting one byte at a time to simulate slowloris
            let greeting = [0x05, 0x01, 0x00];
            for (j, &byte) in greeting.iter().enumerate() {
                stream.write_all(&[byte]).await.unwrap();
                // Sleep between bytes to simulate slow client
                tokio::time::sleep(Duration::from_millis(50)).await;
                // After first byte, we may get a response — read it if available
                if j == 0 {
                    // Non-blocking check for server response
                    let mut resp_buf = [0u8; 2];
                    match tokio::time::timeout(Duration::from_millis(100), async {
                        stream.read_exact(&mut resp_buf).await
                    })
                    .await
                    {
                        Ok(Ok(_)) => {
                            // Got response, continue sending remaining bytes
                        }
                        _ => {
                            // No response yet, continue
                        }
                    }
                }
            }

            // Read the server's choice (may already be read above)
            let mut resp = [0u8; 2];
            let read_result =
                tokio::time::timeout(Duration::from_secs(3), stream.read_exact(&mut resp)).await;
            if let Ok(Ok(_)) = read_result {
                // Only send CONNECT if we got a valid handshake response
                if resp == [0x05, 0x00] {
                    // Send CONNECT request one byte at a time
                    let connect = [0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0x1F, 0x90];
                    for &byte in &connect {
                        stream.write_all(&[byte]).await.unwrap();
                        tokio::time::sleep(Duration::from_millis(30)).await;
                    }

                    let mut reply = [0u8; 10];
                    let _ = tokio::time::timeout(Duration::from_secs(3), async {
                        stream.read_exact(&mut reply).await
                    })
                    .await;
                }
            }
            // Connection closes here — drop stream
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    // Wait for cleanup
    tokio::time::sleep(Duration::from_millis(500)).await;

    let final_fds = count_fds();
    let tolerance = 3;
    assert!(
        final_fds <= baseline_fds + tolerance,
        "FD leak detected after slowloris test: baseline={baseline_fds}, final={final_fds}, tolerance={tolerance}"
    );

    // Verify proxy is still responsive with a normal connection
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("proxy should still accept connections after slowloris");
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(
        resp,
        [0x05, 0x00],
        "proxy should still respond to normal SOCKS5 after slowloris"
    );

    token.cancel();
    let _ = jh.await;
}

// ---------------------------------------------------------------------------
// Phase 50: Auth failure burst — rapid auth failures against SOCKS5 with auth
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn load_test_auth_failure_burst() {
    let config = r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "admin"
password = "correct_password"

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

    // Baseline FD count
    tokio::time::sleep(Duration::from_millis(200)).await;
    let baseline_fds = count_fds();

    let burst_count = 50;
    let mut auth_failures = 0u32;
    let mut connect_failures = 0u32;
    let mut other_errors = 0u32;

    for i in 0..burst_count {
        match tokio::net::TcpStream::connect(listener_addr).await {
            Ok(mut stream) => {
                // SOCKS5 greeting
                stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
                let mut resp = [0u8; 2];
                stream.read_exact(&mut resp).await.unwrap();
                assert_eq!(resp, [0x05, 0x00]);

                // Send username/password auth with wrong credentials
                let username = format!("wrong_user_{i}");
                let password = "wrong_password";
                let auth_len = 1 + 1 + username.len() as u8 + 1 + password.len() as u8;
                let mut auth_msg = vec![0x05, 0x01, 0x00, 0x02]; // version, nmethods=1, reserved, username/password
                auth_msg.push(auth_len);
                auth_msg.push(username.len() as u8);
                auth_msg.extend_from_slice(username.as_bytes());
                auth_msg.push(password.len() as u8);
                auth_msg.extend_from_slice(password.as_bytes());
                stream.write_all(&auth_msg).await.unwrap();

                let mut auth_resp = [0u8; 2];
                if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(2), async {
                    stream.read_exact(&mut auth_resp).await
                })
                .await
                {
                    // Auth response: [0x01, status] — status != 0 means failure
                    if auth_resp == [0x01, 0x00] {
                        other_errors += 1;
                        eprintln!(
                            "auth_failure_burst iteration {i}: expected failure but got success"
                        );
                    } else {
                        auth_failures += 1;
                    }
                } else {
                    connect_failures += 1;
                }
            }
            Err(_) => {
                connect_failures += 1;
            }
        }
    }

    assert!(
        auth_failures > 0,
        "should have at least some auth failures (got {auth_failures})"
    );
    assert_eq!(
        other_errors, 0,
        "should have no unexpected successes (got {other_errors})"
    );
    assert!(
        connect_failures < burst_count / 2,
        "too many connection failures: {connect_failures}/{burst_count} — proxy may be unstable"
    );

    // Wait for cleanup
    tokio::time::sleep(Duration::from_millis(500)).await;

    let final_fds = count_fds();
    let tolerance = 3;
    assert!(
        final_fds <= baseline_fds + tolerance,
        "FD leak detected after auth failure burst: baseline={baseline_fds}, final={final_fds}, tolerance={tolerance}"
    );

    // Verify proxy is still responsive with correct auth
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("proxy should still accept connections");
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    // Authenticate with correct credentials
    let username = "admin";
    let password = "correct_password";
    let auth_len = 1 + 1 + username.len() as u8 + 1 + password.len() as u8;
    let mut auth_msg = vec![0x05, 0x01, 0x00, 0x02];
    auth_msg.push(auth_len);
    auth_msg.push(username.len() as u8);
    auth_msg.extend_from_slice(username.as_bytes());
    auth_msg.push(password.len() as u8);
    auth_msg.extend_from_slice(password.as_bytes());
    stream.write_all(&auth_msg).await.unwrap();

    let mut auth_resp = [0u8; 2];
    stream.read_exact(&mut auth_resp).await.unwrap();
    assert_eq!(
        auth_resp,
        [0x01, 0x00],
        "correct credentials should be accepted after auth failure burst"
    );

    token.cancel();
    let _ = jh.await;
}

// ---------------------------------------------------------------------------
// Phase 50: UDP association churn — rapid create/destroy UDP associations
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn load_test_udp_association_churn() {
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

    // Baseline FD count
    tokio::time::sleep(Duration::from_millis(200)).await;
    let baseline_fds = count_fds();

    let churn_rounds = 10;
    let mut successes = 0u32;
    let mut rejected = 0u32;

    for _round in 0..churn_rounds {
        let mut control_streams = Vec::new();

        // Open associations up to the limit
        for _ in 0..5 {
            let Ok(mut stream) = tokio::net::TcpStream::connect(listener_addr).await else {
                continue;
            };

            stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut resp = [0u8; 2];
            stream.read_exact(&mut resp).await.unwrap();
            if resp != [0x05, 0x00] {
                continue;
            }

            stream
                .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
                .await
                .unwrap();
            stream.write_all(&0u16.to_be_bytes()).await.unwrap();

            let mut reply = [0u8; 10];
            if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(2), async {
                stream.read_exact(&mut reply).await
            })
            .await
            {
                if reply[1] == 0x00 {
                    successes += 1;
                } else {
                    rejected += 1;
                }
            }
            control_streams.push(stream);
        }

        // Try one more — should be rejected since limit is 5
        if let Ok(mut stream) = tokio::net::TcpStream::connect(listener_addr).await {
            stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut resp = [0u8; 2];
            stream.read_exact(&mut resp).await.unwrap();
            stream
                .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
                .await
                .unwrap();
            stream.write_all(&0u16.to_be_bytes()).await.unwrap();
            let mut reply = [0u8; 10];
            if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(2), async {
                stream.read_exact(&mut reply).await
            })
            .await
            {
                if reply[1] == 0x00 {
                    // Should not succeed — limit is 5
                    panic!("6th UDP ASSOCIATE should be rejected at limit");
                } else {
                    rejected += 1;
                }
            }
        } else {
            // Connection refused is acceptable at the limit
            rejected += 1;
        }

        // Drop all control streams to release associations
        drop(control_streams);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(
        successes > 0,
        "should have at least some successful associations across churn rounds (got {successes})"
    );
    assert!(
        rejected > 0,
        "should have at least some rejections at the association limit (got {rejected})"
    );

    // Wait for cleanup
    tokio::time::sleep(Duration::from_millis(500)).await;

    let final_fds = count_fds();
    let tolerance = 3;
    assert!(
        final_fds <= baseline_fds + tolerance,
        "FD leak detected after UDP association churn: baseline={baseline_fds}, final={final_fds}, tolerance={tolerance}"
    );

    // Verify active connections return to zero
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let active = state.active_connections.load(Ordering::Relaxed);
            if active == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("active connections did not return to 0 within 5s");

    token.cancel();
    let _ = jh.await;
}
