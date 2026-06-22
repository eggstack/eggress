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
}
