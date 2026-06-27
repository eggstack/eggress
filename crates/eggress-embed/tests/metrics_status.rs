use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Perform a SOCKS5 handshake to connect to target.
fn socks5_connect(proxy_addr: std::net::SocketAddr, target: std::net::SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(proxy_addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    stream.write_all(&[0x05, 0x01, 0x00]).unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    let octets = match target.ip() {
        std::net::IpAddr::V4(v4) => v4.octets(),
        _ => panic!("only IPv4"),
    };
    let port = target.port().to_be_bytes();
    stream
        .write_all(&[
            0x05, 0x01, 0x00, 0x01, octets[0], octets[1], octets[2], octets[3],
        ])
        .unwrap();
    stream.write_all(&port).unwrap();

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).unwrap();
    assert_eq!(reply[1], 0x00, "SOCKS5 connect failed: {:#04x}", reply[1]);

    stream
}

#[test]
fn metrics_text_contains_prometheus_counters() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "proxy"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#,
    )
    .unwrap();

    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let metrics = handle.metrics_text().unwrap();
    assert!(
        metrics.contains("eggress_connections_total"),
        "metrics should contain connections_total"
    );
    assert!(
        metrics.contains("eggress_config_generation"),
        "metrics should contain config_generation"
    );

    handle.shutdown_blocking().unwrap();
}

#[test]
fn status_reports_generation_and_readiness() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#,
    )
    .unwrap();

    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let status = handle.status();
    assert!(status.readiness);
    assert_eq!(status.generation, 0);
    assert_eq!(status.active_connections, 0);
    assert!(status.uptime_secs < 60);
    assert_eq!(status.listener_count, 1);
    assert_eq!(status.udp_associations_active, 0);
    assert_eq!(status.upstream_count, 0);

    // ListenerStatus details
    assert_eq!(status.listeners.len(), 1);
    let ls = &status.listeners[0];
    assert_eq!(ls.name, "test");
    assert!(ls.local_addr.port() > 0);
    assert_eq!(ls.protocols, vec!["socks5".to_string()]);
    assert!(!ls.udp_enabled);

    handle.shutdown_blocking().unwrap();
}

#[test]
fn status_with_multiple_listeners() {
    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#,
    )
    .unwrap();

    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let status = handle.status();
    assert_eq!(status.listener_count, 2);
    assert_eq!(status.listeners.len(), 2);
    assert_eq!(status.listeners[0].name, "http-in");
    assert_eq!(status.listeners[0].protocols, vec!["http".to_string()]);
    assert_eq!(status.listeners[1].name, "socks-in");
    assert_eq!(status.listeners[1].protocols, vec!["socks5".to_string()]);

    handle.shutdown_blocking().unwrap();
}

#[test]
fn metrics_after_proxy_session() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let echo_addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    let n = match stream.read(&mut buf) {
                        Ok(0) => return,
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    if stream.write_all(&buf[..n]).is_err() {
                        return;
                    }
                }
            });
        }
    });

    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "proxy"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#,
    )
    .unwrap();

    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let proxy_addr = handle.bound_addresses().listener("proxy").unwrap();

    let mut stream = socks5_connect(proxy_addr, echo_addr);
    stream.write_all(b"test data").unwrap();
    let mut response = [0u8; 9];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"test data");

    std::thread::sleep(Duration::from_millis(50));

    let metrics = handle.metrics_text().unwrap();
    assert!(
        metrics.contains("eggress_connections_total"),
        "metrics should track connections"
    );

    handle.shutdown_blocking().unwrap();
}
