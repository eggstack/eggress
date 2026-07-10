use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

async fn wait_ready(state: &eggress_runtime::RuntimeState) {
    for _ in 0..200 {
        if state.readiness.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("supervisor did not become ready");
}

async fn start_tcp_echo() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
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
    addr
}

async fn start_udp_echo() -> std::net::SocketAddr {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 65535];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let _ = socket.send_to(&buf[..n], peer).await;
        }
    });
    addr
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

// ---------------------------------------------------------------------------
// Test 1: TCP relay performance smoke — 50 concurrent SOCKS5 sessions
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn performance_tcp_relay_smoke() {
    let echo_addr = start_tcp_echo().await;

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
        addrs[0].unwrap()
    };

    let concurrency = 50;
    let start = std::time::Instant::now();
    let mut handles = Vec::with_capacity(concurrency);

    for i in 0..concurrency {
        let addr = listener_addr;
        let echo = echo_addr;
        let handle = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

            // SOCKS5 handshake (no auth)
            stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut resp = [0u8; 2];
            stream.read_exact(&mut resp).await.unwrap();
            assert_eq!(resp, [0x05, 0x00]);

            // CONNECT to echo server
            let ip_octets = match echo.ip() {
                std::net::IpAddr::V4(ip) => ip.octets(),
                _ => panic!("expected IPv4"),
            };
            stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
            stream.write_all(&ip_octets).await.unwrap();
            stream.write_all(&echo.port().to_be_bytes()).await.unwrap();

            let mut reply = [0u8; 10];
            stream.read_exact(&mut reply).await.unwrap();
            assert_eq!(reply[1], 0x00, "SOCKS5 CONNECT failed");

            // Send payload and verify echo
            let payload = format!("payload-{i}");
            stream.write_all(payload.as_bytes()).await.unwrap();
            let mut buf = [0u8; 4096];
            let n = tokio::time::timeout(Duration::from_secs(5), async {
                stream.read(&mut buf).await
            })
            .await
            .expect("read timeout")
            .expect("read error");
            assert_eq!(&buf[..n], payload.as_bytes());

            drop(stream);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "50 concurrent TCP sessions should complete in under 5s, took {elapsed:?}"
    );

    token.cancel();
    let _ = jh.await;
}

// ---------------------------------------------------------------------------
// Test 2: UDP relay performance smoke — 100 datagrams through standalone relay
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn performance_udp_relay_smoke() {
    use eggress_udp::codec::decode_packet;
    use eggress_udp::limits::UdpLimits;
    use eggress_udp::metrics::UdpMetrics;
    use eggress_udp::standalone::{standalone_udp_relay, StandaloneUdpConfig};
    use tokio::net::UdpSocket;
    use tokio_util::sync::CancellationToken;

    let echo_addr = start_udp_echo().await;

    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();
    let metrics = Arc::new(UdpMetrics::new());
    let routing: Arc<dyn eggress_routing::RouteService> = Arc::new(eggress_routing::Router::new(
        vec![],
        eggress_routing::RouteActionSpec::Direct,
    ));
    let config = StandaloneUdpConfig {
        routing,
        udp_metrics: metrics.clone(),
        limits: UdpLimits::default(),
        listener: "test-perf-udp".to_string(),
        generation: 1,
        allow_private_egress: true,
    };
    let cancel = CancellationToken::new();
    let relay_cancel = cancel.clone();
    let relay_sock = relay_socket.clone();
    let handle =
        tokio::spawn(async move { standalone_udp_relay(relay_sock, config, relay_cancel).await });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(relay_addr).await.unwrap();

    let num_datagrams = 100;
    for i in 0..num_datagrams {
        let payload = format!("datagram-{i}");
        let ip_octets = match echo_addr.ip() {
            std::net::IpAddr::V4(ip) => ip.octets(),
            _ => panic!("expected IPv4"),
        };
        let mut pkt = vec![0x00, 0x00, 0x00, 0x01];
        pkt.extend_from_slice(&ip_octets);
        pkt.extend_from_slice(&echo_addr.port().to_be_bytes());
        pkt.extend_from_slice(payload.as_bytes());
        client_socket.send(&pkt).await.unwrap();

        let mut recv_buf = [0u8; 65535];
        let n = tokio::time::timeout(Duration::from_secs(2), async {
            client_socket.recv(&mut recv_buf).await
        })
        .await
        .unwrap_or_else(|_| panic!("recv timeout on datagram {i}"))
        .unwrap_or_else(|e| panic!("recv error on datagram {i}: {e}"));

        let resp = decode_packet(&recv_buf[..n], &UdpLimits::default())
            .unwrap_or_else(|e| panic!("decode error on datagram {i}: {e}"));
        assert_eq!(
            resp.payload,
            payload.as_bytes(),
            "payload mismatch on datagram {i}"
        );
    }

    cancel.cancel();
    handle.await.unwrap().unwrap();
}

// ---------------------------------------------------------------------------
// Test 3: Resource leak — file descriptor cleanup after SOCKS5 sessions
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resource_leak_fd_cleanup() {
    let echo_addr = start_tcp_echo().await;

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
        addrs[0].unwrap()
    };

    // Allow baseline to stabilize
    tokio::time::sleep(Duration::from_millis(200)).await;
    let baseline_fds = count_fds();

    let sessions = 20;
    for _ in 0..sessions {
        let mut stream = tokio::net::TcpStream::connect(listener_addr).await.unwrap();

        // SOCKS5 handshake
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut resp = [0u8; 2];
        stream.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp, [0x05, 0x00]);

        // CONNECT to echo server
        let ip_octets = match echo_addr.ip() {
            std::net::IpAddr::V4(ip) => ip.octets(),
            _ => panic!("expected IPv4"),
        };
        stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
        stream.write_all(&ip_octets).await.unwrap();
        stream
            .write_all(&echo_addr.port().to_be_bytes())
            .await
            .unwrap();

        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[1], 0x00, "SOCKS5 CONNECT failed");

        // Send and verify a round-trip to confirm relay is active
        stream.write_all(b"fd-test").await.unwrap();
        let mut buf = [0u8; 4096];
        let n = tokio::time::timeout(Duration::from_secs(2), async {
            stream.read(&mut buf).await
        })
        .await
        .expect("read timeout")
        .expect("read error");
        assert_eq!(&buf[..n], b"fd-test");

        drop(stream);
    }

    // Wait for cleanup
    tokio::time::sleep(Duration::from_millis(500)).await;

    let final_fds = count_fds();

    // On Unix, allow ±2 tolerance for FD counting noise
    let tolerance = 2;
    assert!(
        final_fds <= baseline_fds + tolerance,
        "FD leak detected: baseline={baseline_fds}, final={final_fds}, tolerance={tolerance}"
    );

    token.cancel();
    let _ = jh.await;
}

// ---------------------------------------------------------------------------
// Test 4: Resource leak — task cleanup and clean shutdown
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resource_leak_task_cleanup() {
    let echo_addr = start_tcp_echo().await;

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
        addrs[0].unwrap()
    };

    // Open 20 SOCKS5 sessions concurrently, each doing CONNECT + echo
    let sessions = 20;
    let mut handles = Vec::with_capacity(sessions);

    for _ in 0..sessions {
        let addr = listener_addr;
        let echo = echo_addr;
        let handle = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

            // SOCKS5 handshake
            stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut resp = [0u8; 2];
            stream.read_exact(&mut resp).await.unwrap();
            assert_eq!(resp, [0x05, 0x00]);

            // CONNECT to echo server
            let ip_octets = match echo.ip() {
                std::net::IpAddr::V4(ip) => ip.octets(),
                _ => panic!("expected IPv4"),
            };
            stream.write_all(&[0x05, 0x01, 0x00, 0x01]).await.unwrap();
            stream.write_all(&ip_octets).await.unwrap();
            stream.write_all(&echo.port().to_be_bytes()).await.unwrap();

            let mut reply = [0u8; 10];
            stream.read_exact(&mut reply).await.unwrap();
            assert_eq!(reply[1], 0x00, "SOCKS5 CONNECT failed");

            // Verify relay is active with an echo round-trip
            stream.write_all(b"task-test").await.unwrap();
            let mut buf = [0u8; 4096];
            let n = tokio::time::timeout(Duration::from_secs(2), async {
                stream.read(&mut buf).await
            })
            .await
            .expect("read timeout")
            .expect("read error");
            assert_eq!(&buf[..n], b"task-test");

            // Close session
            drop(stream);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

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

    assert_eq!(
        state.active_connections.load(Ordering::Relaxed),
        0,
        "active connections should be 0 after all sessions closed"
    );

    // Shutdown cleanly and verify no panics or timeouts
    let start = std::time::Instant::now();
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(10), jh).await;
    let elapsed = start.elapsed();

    assert!(
        result.is_ok(),
        "supervisor join handle timed out during shutdown"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "shutdown took too long: {elapsed:?}"
    );
}
