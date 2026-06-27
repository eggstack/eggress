use std::io::Write;
use std::sync::atomic::Ordering;

use eggress_core::TargetHost;
use eggress_protocol_shadowsocks::udp::{decode_udp_packet, encode_udp_packet};
use eggress_protocol_shadowsocks::CipherMethod;
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

async fn socks5_udp_associate(stream: &mut tokio::net::TcpStream) -> std::io::Result<[u8; 10]> {
    stream.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await?;
    assert_eq!(resp, [0x05, 0x00]);

    stream
        .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
        .await?;
    stream.write_all(&0u16.to_be_bytes()).await?;

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await?;
    Ok(reply)
}

fn ipv4_socks5_packet(target: [u8; 4], port: u16, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00, 0x01];
    pkt.extend_from_slice(&target);
    pkt.extend_from_slice(&port.to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

async fn start_udp_echo() -> std::net::SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 65535];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let _ = socket.send_to(&buf[..n], peer).await;
        }
    });
    addr
}

async fn start_shadowsocks_udp_echo(method: CipherMethod, password: &str) -> std::net::SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    let password = password.as_bytes().to_vec();
    tokio::spawn(async move {
        let mut buf = [0u8; 65535];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            if let Ok((target, payload)) = decode_udp_packet(method, &password, &buf[..n]) {
                let target_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
                let target_ip = match &target.host {
                    TargetHost::Ip(ip) => *ip,
                    _ => continue,
                };
                let target_addr = std::net::SocketAddr::new(target_ip, target.port);
                if target_socket.send_to(&payload, target_addr).await.is_err() {
                    continue;
                }
                let mut resp_buf = [0u8; 65535];
                if let Ok(Ok((n, _))) = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    target_socket.recv_from(&mut resp_buf),
                )
                .await
                {
                    let resp_salt = vec![0xABu8; method.salt_size()];
                    if let Ok(response) =
                        encode_udp_packet(method, &password, &target, &resp_buf[..n], &resp_salt)
                    {
                        let _ = socket.send_to(&response, peer).await;
                    }
                }
            }
        }
    });
    addr
}

async fn wait_ready(state: &eggress_runtime::RuntimeState) {
    for _ in 0..100 {
        if state.readiness.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("timeout waiting for readiness");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_udp_upstream_routes_udp_echo() {
    eggress_transport_tls::install_default_crypto_provider();

    let echo_addr = start_udp_echo().await;
    let ss_addr = start_shadowsocks_udp_echo(CipherMethod::Aes256Gcm, "test-secret").await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "ss-proxy"
uri = "shadowsocks://aes-256-gcm:test-secret@127.0.0.1:{ss_port}"

[[upstream_groups]]
id = "main"
members = ["ss-proxy"]

[[rules]]
id = "route-all"
upstream_group = "main"

[rules.match]
all = [
  {{ transport = "udp" }}
]
"#,
        ss_port = ss_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00, "udp associate should succeed");

    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = format!("127.0.0.1:{relay_port}");

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(&relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"ss-udp-hello");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout waiting for response")
    .expect("recv");

    let resp = eggress_udp::codec::decode_packet(
        &recv_buf[..n],
        &eggress_udp::limits::UdpLimits::default(),
    )
    .unwrap();
    assert_eq!(resp.payload, b"ss-udp-hello");

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_udp_wrong_password_drops() {
    eggress_transport_tls::install_default_crypto_provider();

    let echo_addr = start_udp_echo().await;
    let ss_addr = start_shadowsocks_udp_echo(CipherMethod::Aes256Gcm, "correct-password").await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "ss-proxy"
uri = "shadowsocks://aes-256-gcm:wrong-password@127.0.0.1:{ss_port}"

[[upstream_groups]]
id = "main"
members = ["ss-proxy"]

[[rules]]
id = "route-all"
upstream_group = "main"

[rules.match]
all = [
  {{ transport = "udp" }}
]
"#,
        ss_port = ss_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00, "udp associate should succeed");

    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = format!("127.0.0.1:{relay_port}");

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(&relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"wrong-pw-test");
    client_socket.send(&pkt).await.unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut buf = [0u8; 65535];
        client_socket.recv(&mut buf).await
    })
    .await;

    match result {
        Ok(Ok(_)) => {
            panic!("expected no response with wrong password, but got data back");
        }
        Ok(Err(_)) => {}
        Err(_) => {}
    }

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_udp_unsupported_method_rejected() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    eggress_transport_tls::install_default_crypto_provider();

    let echo_addr = start_udp_echo().await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "ss-proxy"
uri = "shadowsocks://rc4-md5:pass@127.0.0.1:{port}"

[[upstream_groups]]
id = "main"
members = ["ss-proxy"]

[[rules]]
id = "route-all"
upstream_group = "main"

[rules.match]
all = [
  {{ transport = "udp" }}
]
"#,
        port = echo_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    // Connect via SOCKS5
    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    // SOCKS5 handshake
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    // SOCKS5 UDP ASSOCIATE - request UDP relay
    stream
        .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .unwrap();
    let mut assoc_resp = [0u8; 10];
    stream.read_exact(&mut assoc_resp).await.unwrap();
    assert_eq!(assoc_resp[1], 0x00, "UDP ASSOCIATE should succeed");

    // Parse the relay address from the response
    let relay_ip =
        std::net::Ipv4Addr::new(assoc_resp[4], assoc_resp[5], assoc_resp[6], assoc_resp[7]);
    let relay_port = u16::from_be_bytes([assoc_resp[8], assoc_resp[9]]);
    let relay_addr = std::net::SocketAddr::new(std::net::IpAddr::V4(relay_ip), relay_port);

    // Send a UDP packet through the relay with unsupported method
    // The packet should be dropped because rc4-md5 is not a valid AEAD method
    let client_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pkt = ipv4_socks5_packet([127, 0, 0, 1], 9090, b"unsupported method test");
    client_socket.send_to(&pkt, relay_addr).await.unwrap();

    // Should not receive a response (packet dropped)
    let result = tokio::time::timeout(std::time::Duration::from_millis(500), async {
        let mut buf = [0u8; 65535];
        client_socket.recv(&mut buf).await
    })
    .await;

    // Timeout = no response = packet was dropped (correct behavior for unsupported method)
    assert!(
        result.is_err(),
        "unsupported method should cause packet to be dropped"
    );

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_udp_metrics_increment() {
    eggress_transport_tls::install_default_crypto_provider();

    let echo_addr = start_udp_echo().await;
    let ss_addr = start_shadowsocks_udp_echo(CipherMethod::Aes256Gcm, "test-secret").await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[[upstreams]]
id = "ss-proxy"
uri = "shadowsocks://aes-256-gcm:test-secret@127.0.0.1:{ss_port}"

[[upstream_groups]]
id = "main"
members = ["ss-proxy"]

[[rules]]
id = "route-all"
upstream_group = "main"

[rules.match]
all = [
  {{ transport = "udp" }}
]

[admin]
bind = "127.0.0.1:0"
enabled = true
"#,
        ss_port = ss_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let admin_addr = {
        let mut addr = None;
        for _ in 0..100 {
            if let Some(a) = *state.admin_local_addr.lock().unwrap() {
                addr = Some(a.to_string());
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        addr.expect("admin should have bound within 5s")
    };

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00, "udp associate should succeed");

    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = format!("127.0.0.1:{relay_port}");

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(&relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"metrics-test");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout waiting for response")
    .expect("recv");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let metrics_body = {
        let mut stream = tokio::net::TcpStream::connect(&admin_addr)
            .await
            .expect("connect to admin");
        let req = b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
        tokio::io::AsyncWriteExt::write_all(&mut stream, req)
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::flush(&mut stream).await.unwrap();

        let mut resp = Vec::new();
        let read_result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut buf = [0u8; 8192];
            loop {
                match tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
                    Ok(0) => break,
                    Ok(n) => resp.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
        })
        .await;
        assert!(read_result.is_ok(), "timeout reading metrics");
        String::from_utf8_lossy(&resp).to_string()
    };

    assert!(
        metrics_body.contains("eggress_udp_upstream_packets_up_total"),
        "metrics should expose UDP upstream packets up counter"
    );

    assert!(
        metrics_body.contains("eggress_udp_upstream_packets_down_total"),
        "metrics should expose UDP upstream packets down counter"
    );

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadowsocks_udp_target_flow_idle_cleanup() {
    eggress_transport_tls::install_default_crypto_provider();

    let echo_addr = start_udp_echo().await;
    let ss_addr = start_shadowsocks_udp_echo(CipherMethod::Aes256Gcm, "test-secret").await;

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
udp_enabled = true

[listeners.udp]
target_idle_timeout = "150ms"

[[upstreams]]
id = "ss-proxy"
uri = "shadowsocks://aes-256-gcm:test-secret@127.0.0.1:{ss_port}"

[[upstream_groups]]
id = "main"
members = ["ss-proxy"]

[[rules]]
id = "route-all"
upstream_group = "main"

[rules.match]
all = [
  {{ transport = "udp" }}
]
"#,
        ss_port = ss_addr.port()
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    wait_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        addrs[0]
    };

    let mut stream = tokio::net::TcpStream::connect(listener_addr)
        .await
        .expect("connect");

    let reply = socks5_udp_associate(&mut stream)
        .await
        .expect("udp associate");
    assert_eq!(reply[1], 0x00, "udp associate should succeed");

    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let relay_addr = format!("127.0.0.1:{relay_port}");

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(&relay_addr).await.unwrap();

    let pkt = ipv4_socks5_packet([127, 0, 0, 1], echo_addr.port(), b"idle-timeout-test");
    client_socket.send(&pkt).await.unwrap();

    let mut recv_buf = [0u8; 65535];
    let n = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        client_socket.recv(&mut recv_buf).await
    })
    .await
    .expect("timeout waiting for response")
    .expect("recv");

    let resp = eggress_udp::codec::decode_packet(
        &recv_buf[..n],
        &eggress_udp::limits::UdpLimits::default(),
    )
    .unwrap();
    assert_eq!(resp.payload, b"idle-timeout-test");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let flows_after_send = state
        .udp_metrics
        .target_flows_active
        .load(Ordering::Relaxed);
    assert_eq!(flows_after_send, 1, "should have one active target flow");

    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let flows_after_idle = state
        .udp_metrics
        .target_flows_active
        .load(Ordering::Relaxed);
    assert_eq!(
        flows_after_idle, 0,
        "target flow should be evicted after idle timeout"
    );

    drop(stream);
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), jh).await;
    assert!(result.is_ok(), "shutdown should complete within timeout");
}
