use tokio::io::AsyncWriteExt;

use crate::accept::{PendingTunnel, ReplyContext, TunnelProtocol};
use crate::error::SessionOpenError;
use eggress_protocol_socks::socks5::server::SocksAddr;

/// Send a tunnel success reply after route is established.
pub async fn send_tunnel_success(
    pending: &mut PendingTunnel,
    _bound_addr: Option<std::net::SocketAddr>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match (&pending.protocol, &pending.reply_context) {
        (TunnelProtocol::HttpConnect, ReplyContext::Http) => {
            pending
                .client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await?;
        }
        (TunnelProtocol::Socks4, ReplyContext::Socks4) => {
            eggress_protocol_socks::socks4::server::write_socks4_reply(
                &mut pending.client,
                eggress_protocol_socks::socks4::server::Socks4Status::Granted,
                "0.0.0.0:0".parse().unwrap(),
            )
            .await?;
        }
        (TunnelProtocol::Socks5, ReplyContext::Socks5) => {
            let bind_addr = SocksAddr::IPv4([0, 0, 0, 0], 0);
            eggress_protocol_socks::socks5::server::send_connect_reply(
                &mut pending.client,
                0x00,
                &bind_addr,
            )
            .await?;
        }
        _ => {
            return Err("mismatched protocol and reply context".into());
        }
    }
    Ok(())
}

/// Send a tunnel failure reply when route opening fails.
pub async fn send_tunnel_failure(
    pending: &mut PendingTunnel,
    error: &SessionOpenError,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match (&pending.protocol, &pending.reply_context) {
        (TunnelProtocol::HttpConnect, ReplyContext::Http) => {
            let status = http_failure_status(error);
            pending.client.write_all(status).await?;
        }
        (TunnelProtocol::Socks4, ReplyContext::Socks4) => {
            eggress_protocol_socks::socks4::server::write_socks4_reply(
                &mut pending.client,
                eggress_protocol_socks::socks4::server::Socks4Status::Failed,
                "0.0.0.0:0".parse().unwrap(),
            )
            .await?;
        }
        (TunnelProtocol::Socks5, ReplyContext::Socks5) => {
            let rep = socks5_failure_rep(error);
            let bind_addr = SocksAddr::IPv4([0, 0, 0, 0], 0);
            eggress_protocol_socks::socks5::server::send_connect_reply(
                &mut pending.client,
                rep,
                &bind_addr,
            )
            .await?;
        }
        _ => {
            return Err("mismatched protocol and reply context".into());
        }
    }
    Ok(())
}

/// Send an HTTP forward-proxy failure reply.
pub async fn send_http_forward_failure(
    client: &mut eggress_core::BoxStream,
    error: &SessionOpenError,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let status = http_failure_status(error);
    client.write_all(status).await?;
    Ok(())
}

fn http_failure_status(error: &SessionOpenError) -> &'static [u8] {
    match error {
        SessionOpenError::Timeout => b"HTTP/1.1 504 Gateway Timeout\r\nConnection: close\r\n\r\n",
        SessionOpenError::PolicyDenied => b"HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n",
        _ => b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n",
    }
}

fn socks5_failure_rep(error: &SessionOpenError) -> u8 {
    match error {
        SessionOpenError::Timeout => 0x06,
        SessionOpenError::PolicyDenied => 0x02,
        SessionOpenError::NetworkUnreachable => 0x03,
        SessionOpenError::HostUnreachable | SessionOpenError::Dns => 0x04,
        SessionOpenError::Refused => 0x05,
        _ => 0x01,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accept::{PendingTunnel, ReplyContext, TunnelProtocol};
    use eggress_core::{TargetAddr, TargetHost};
    use tokio::io::AsyncReadExt;

    fn make_pending(
        protocol: TunnelProtocol,
        reply_context: ReplyContext,
    ) -> (PendingTunnel, tokio::io::DuplexStream) {
        let (client_stream, server_stream) = tokio::io::duplex(1024);
        let pending = PendingTunnel {
            target: TargetAddr {
                host: TargetHost::Domain("example.com".into()),
                port: 443,
            },
            client: Box::new(client_stream),
            protocol,
            reply_context,
            identity: eggress_core::ClientIdentity::Anonymous,
        };
        (pending, server_stream)
    }

    #[tokio::test]
    async fn test_send_tunnel_success_http() {
        let (mut pending, mut server) =
            make_pending(TunnelProtocol::HttpConnect, ReplyContext::Http);
        send_tunnel_success(&mut pending, None).await.unwrap();

        let mut response = vec![0u8; 1024];
        let n = server.read(&mut response).await.unwrap();
        let s = String::from_utf8_lossy(&response[..n]);
        assert!(s.contains("200"));
    }

    #[tokio::test]
    async fn test_send_tunnel_success_socks4() {
        let (mut pending, mut server) = make_pending(TunnelProtocol::Socks4, ReplyContext::Socks4);
        send_tunnel_success(&mut pending, None).await.unwrap();

        let mut response = [0u8; 8];
        server.read_exact(&mut response).await.unwrap();
        assert_eq!(response[0], 0x00);
        assert_eq!(response[1], 90); // granted
    }

    #[tokio::test]
    async fn test_send_tunnel_success_socks5() {
        let (mut pending, mut server) = make_pending(TunnelProtocol::Socks5, ReplyContext::Socks5);
        send_tunnel_success(&mut pending, None).await.unwrap();

        let mut response = [0u8; 10];
        server.read_exact(&mut response).await.unwrap();
        assert_eq!(response[0], 0x05);
        assert_eq!(response[1], 0x00); // success
    }

    #[tokio::test]
    async fn test_send_tunnel_failure_http_timeout() {
        let (mut pending, mut server) =
            make_pending(TunnelProtocol::HttpConnect, ReplyContext::Http);
        send_tunnel_failure(&mut pending, &SessionOpenError::Timeout)
            .await
            .unwrap();

        let mut response = vec![0u8; 1024];
        let n = server.read(&mut response).await.unwrap();
        let s = String::from_utf8_lossy(&response[..n]);
        assert!(s.contains("504"));
    }

    #[tokio::test]
    async fn test_send_tunnel_failure_socks5_refused() {
        let (mut pending, mut server) = make_pending(TunnelProtocol::Socks5, ReplyContext::Socks5);
        send_tunnel_failure(&mut pending, &SessionOpenError::Refused)
            .await
            .unwrap();

        let mut response = [0u8; 10];
        server.read_exact(&mut response).await.unwrap();
        assert_eq!(response[0], 0x05);
        assert_eq!(response[1], 0x05); // connection refused
    }

    #[tokio::test]
    async fn test_send_http_forward_failure() {
        let (client_stream, mut server) = tokio::io::duplex(1024);
        let mut client: eggress_core::BoxStream = Box::new(client_stream);
        send_http_forward_failure(&mut client, &SessionOpenError::Refused)
            .await
            .unwrap();

        let mut response = vec![0u8; 1024];
        let n = server.read(&mut response).await.unwrap();
        let s = String::from_utf8_lossy(&response[..n]);
        assert!(s.contains("502"));
    }
}
