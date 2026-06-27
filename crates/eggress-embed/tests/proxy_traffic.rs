use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Perform an HTTP CONNECT handshake to connect to target.
fn http_connect(proxy_addr: std::net::SocketAddr, target: std::net::SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(proxy_addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    let connect_req = format!(
        "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n\r\n",
        target.ip(),
        target.port(),
        target.ip(),
        target.port()
    );
    stream.write_all(connect_req.as_bytes()).unwrap();

    let mut response = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut buf).unwrap();
        response.extend_from_slice(&buf[..n]);
        if response.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let response_str = String::from_utf8_lossy(&response);
    assert!(
        response_str.starts_with("HTTP/1.1 200"),
        "HTTP CONNECT should succeed, got: {response_str}"
    );

    stream
}

/// Start a simple TCP echo server.
fn start_echo_server() -> std::net::SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
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
    addr
}

/// Perform a SOCKS5 handshake to connect to target.
fn socks5_connect(proxy_addr: std::net::SocketAddr, target: std::net::SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(proxy_addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    // Method negotiation: NO AUTH
    stream.write_all(&[0x05, 0x01, 0x00]).unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).unwrap();
    assert_eq!(resp, [0x05, 0x00]);

    // CONNECT request
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
fn socks5_proxy_tcp_echo() {
    let echo_addr = start_echo_server();

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

    let msg = b"hello from embed API";
    stream.write_all(msg).unwrap();

    let mut response = vec![0u8; msg.len()];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(&response, msg);

    handle.shutdown_blocking().unwrap();
}

#[test]
fn http_connect_proxy_tcp_echo() {
    let echo_addr = start_echo_server();

    let config = eggress_embed::EggressConfig::from_toml_str(
        r#"
version = 1

[[listeners]]
name = "proxy"
bind = "127.0.0.1:0"
protocols = ["http"]
"#,
    )
    .unwrap();

    let handle = eggress_embed::EggressService::new(config)
        .start_blocking()
        .unwrap();

    let proxy_addr = handle.bound_addresses().listener("proxy").unwrap();

    let mut stream = http_connect(proxy_addr, echo_addr);

    let msg = b"hello from http connect";
    stream.write_all(msg).unwrap();

    let mut response = vec![0u8; msg.len()];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(&response, msg);

    handle.shutdown_blocking().unwrap();
}

#[test]
fn bound_address_port_zero_is_discoverable() {
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

    let addr = handle.bound_addresses().listener("test").unwrap();
    assert!(addr.port() > 0, "port-0 bind should expose actual port");

    handle.shutdown_blocking().unwrap();
}
