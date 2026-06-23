use std::net::SocketAddr;

pub async fn start_udp_echo_server() -> SocketAddr {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Socks5TestMode {
    NoAuth,
    UsernamePassword { username: String, password: String },
    EchoWithCredentials { username: String, password: String },
    AuthFailure,
    AssociateFailure { reply_code: u8 },
    Echo,
}

pub struct Socks5TestServerConfig {
    pub mode: Socks5TestMode,
    pub relay_addr: Option<SocketAddr>,
}

pub struct Socks5UdpTestServer {
    pub tcp_addr: SocketAddr,
    pub udp_relay_addr: SocketAddr,
    pub received: tokio::sync::mpsc::Receiver<Vec<u8>>,
}

impl Socks5UdpTestServer {
    pub async fn start(config: Socks5TestServerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let tcp_addr = tcp_listener.local_addr()?;

        let udp_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
        let udp_relay_addr = config.relay_addr.unwrap_or(udp_socket.local_addr()?);

        let (tx, rx) = tokio::sync::mpsc::channel(64);

        let mode = config.mode;
        tokio::spawn(async move {
            loop {
                let (stream, _peer) = match tcp_listener.accept().await {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let mode = mode.clone();
                let udp_socket = match tokio::net::UdpSocket::bind("127.0.0.1:0").await {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let tx = tx.clone();

                tokio::spawn(async move {
                    let _ = handle_socks5_connection(stream, udp_socket, &mode, tx).await;
                });
            }
        });

        Ok(Self {
            tcp_addr,
            udp_relay_addr,
            received: rx,
        })
    }
}

async fn handle_socks5_connection(
    mut stream: tokio::net::TcpStream,
    udp_socket: tokio::net::UdpSocket,
    mode: &Socks5TestMode,
    tx: tokio::sync::mpsc::Sender<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let version = stream.read_u8().await?;
    if version != 0x05 {
        return Ok(());
    }
    let nmethods = stream.read_u8().await?;
    let mut methods = vec![0u8; nmethods as usize];
    stream.read_exact(&mut methods).await?;

    match mode {
        Socks5TestMode::NoAuth | Socks5TestMode::Echo => {
            if !methods.contains(&0x00) {
                stream.write_all(&[0x05, 0xFF]).await?;
                return Ok(());
            }
            stream.write_all(&[0x05, 0x00]).await?;
        }
        Socks5TestMode::UsernamePassword { .. }
        | Socks5TestMode::EchoWithCredentials { .. }
        | Socks5TestMode::AuthFailure => {
            if !methods.contains(&0x02) {
                stream.write_all(&[0x05, 0xFF]).await?;
                return Ok(());
            }
            stream.write_all(&[0x05, 0x02]).await?;
        }
        Socks5TestMode::AssociateFailure { .. } => {
            if !methods.contains(&0x00) {
                stream.write_all(&[0x05, 0xFF]).await?;
                return Ok(());
            }
            stream.write_all(&[0x05, 0x00]).await?;
        }
    }

    match mode {
        Socks5TestMode::UsernamePassword {
            username: expected_user,
            password: expected_pass,
        }
        | Socks5TestMode::EchoWithCredentials {
            username: expected_user,
            password: expected_pass,
        } => {
            let auth_version = stream.read_u8().await?;
            if auth_version != 0x01 {
                return Ok(());
            }
            let ulen = stream.read_u8().await? as usize;
            let mut username = vec![0u8; ulen];
            stream.read_exact(&mut username).await?;
            let plen = stream.read_u8().await? as usize;
            let mut password = vec![0u8; plen];
            stream.read_exact(&mut password).await?;

            if username == expected_user.as_bytes() && password == expected_pass.as_bytes() {
                stream.write_all(&[0x01, 0x00]).await?;
            } else {
                stream.write_all(&[0x01, 0x01]).await?;
                return Ok(());
            }
        }
        Socks5TestMode::AuthFailure => {
            let auth_version = stream.read_u8().await?;
            if auth_version != 0x01 {
                return Ok(());
            }
            let ulen = stream.read_u8().await? as usize;
            let mut username = vec![0u8; ulen];
            stream.read_exact(&mut username).await?;
            let plen = stream.read_u8().await? as usize;
            let mut password = vec![0u8; plen];
            stream.read_exact(&mut password).await?;
            stream.write_all(&[0x01, 0x01]).await?;
            return Ok(());
        }
        _ => {}
    }

    let cmd_version = stream.read_u8().await?;
    if cmd_version != 0x05 {
        return Ok(());
    }
    let cmd = stream.read_u8().await?;
    let _rsv = stream.read_u8().await?;
    let atyp = stream.read_u8().await?;
    match atyp {
        0x01 => {
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).await?;
            let _port = stream.read_u16().await?;
        }
        0x03 => {
            let len = stream.read_u8().await? as usize;
            let mut domain = vec![0u8; len];
            stream.read_exact(&mut domain).await?;
            let _port = stream.read_u16().await?;
        }
        0x04 => {
            let mut addr = [0u8; 16];
            stream.read_exact(&mut addr).await?;
            let _port = stream.read_u16().await?;
        }
        _ => return Ok(()),
    }

    if cmd == 0x03 {
        if let Socks5TestMode::AssociateFailure { reply_code } = mode {
            let mut reply = vec![0x05, *reply_code, 0x00];
            reply.push(0x01);
            reply.extend_from_slice(&[0, 0, 0, 0]);
            reply.extend_from_slice(&0u16.to_be_bytes());
            stream.write_all(&reply).await?;
            return Ok(());
        }

        let relay = udp_socket.local_addr()?;
        let relay_ip = relay.ip();
        let relay_port = relay.port();
        let mut reply = vec![0x05, 0x00, 0x00];
        match relay_ip {
            std::net::IpAddr::V4(ip) => {
                reply.push(0x01);
                reply.extend_from_slice(&ip.octets());
            }
            std::net::IpAddr::V6(ip) => {
                reply.push(0x04);
                reply.extend_from_slice(&ip.octets());
            }
        }
        reply.extend_from_slice(&relay_port.to_be_bytes());
        stream.write_all(&reply).await?;

        if matches!(
            mode,
            Socks5TestMode::Echo | Socks5TestMode::EchoWithCredentials { .. }
        ) {
            let udp_socket = std::sync::Arc::new(udp_socket);
            let udp_socket_clone = udp_socket.clone();
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 65535];
                while let Ok((n, peer)) = udp_socket_clone.recv_from(&mut buf).await {
                    if n < 10 {
                        continue;
                    }
                    let payload = buf[10..n].to_vec();
                    let _ = tx_clone.send(payload).await;
                    let _ = udp_socket_clone.send_to(&buf[..n], peer).await;
                }
            });
        }
    } else {
        let mut reply = vec![0x05, 0x00, 0x00, 0x01];
        reply.extend_from_slice(&[0, 0, 0, 0]);
        reply.extend_from_slice(&0u16.to_be_bytes());
        stream.write_all(&reply).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn udp_echo_server_responds() {
        let addr = start_udp_echo_server().await;
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        socket.connect(addr).await.unwrap();
        socket.send(b"ping").await.unwrap();
        let mut buf = [0u8; 65535];
        let n = socket.recv(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"ping");
    }

    #[tokio::test]
    async fn socks5_test_server_no_auth() {
        let server = Socks5UdpTestServer::start(Socks5TestServerConfig {
            mode: Socks5TestMode::NoAuth,
            relay_addr: None,
        })
        .await
        .unwrap();

        let mut stream = tokio::net::TcpStream::connect(server.tcp_addr)
            .await
            .unwrap();

        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();

        let mut method_reply = [0u8; 2];
        stream.read_exact(&mut method_reply).await.unwrap();
        assert_eq!(method_reply, [0x05, 0x00]);

        stream
            .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
            .await
            .unwrap();
        stream.write_all(&0u16.to_be_bytes()).await.unwrap();

        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x00);
    }

    #[tokio::test]
    async fn socks5_test_server_auth_failure() {
        let server = Socks5UdpTestServer::start(Socks5TestServerConfig {
            mode: Socks5TestMode::AuthFailure,
            relay_addr: None,
        })
        .await
        .unwrap();

        let mut stream = tokio::net::TcpStream::connect(server.tcp_addr)
            .await
            .unwrap();

        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        stream.write_all(&[0x05, 0x01, 0x02]).await.unwrap();

        let mut method_reply = [0u8; 2];
        stream.read_exact(&mut method_reply).await.unwrap();
        assert_eq!(method_reply, [0x05, 0x02]);

        stream
            .write_all(&[
                0x01, 0x04, b'u', b's', b'e', b'r', 0x04, b'p', b'a', b's', b's',
            ])
            .await
            .unwrap();

        let mut auth_reply = [0u8; 2];
        stream.read_exact(&mut auth_reply).await.unwrap();
        assert_eq!(auth_reply, [0x01, 0x01]);
    }
}
