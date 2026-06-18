use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::Socks5Error;
use crate::socks5::server::{SocksAddr, ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6};

/// SOCKS5 authentication methods.
const AUTH_NONE: u8 = 0x00;
const AUTH_USERNAME_PASSWORD: u8 = 0x02;

/// Username/password auth version.
const AUTH_VERSION: u8 = 0x01;

/// Maximum length for username or password in SOCKS5 auth.
const MAX_CRED_LEN: usize = 255;

/// Connect to a target through a SOCKS5 proxy.
///
/// This performs the full SOCKS5 handshake:
/// 1. Method negotiation
/// 2. Username/password authentication (if configured)
/// 3. CONNECT request
/// 4. Reply parsing
///
/// # Arguments
/// * `stream` - The bidirectional stream to the SOCKS5 proxy.
/// * `target` - The target address to connect to.
/// * `auth` - Optional (username, password) for authentication.
///
/// # Returns
/// The stream after successful connection, ready for data transfer.
pub async fn socks5_connect<RW: AsyncRead + AsyncWrite + Unpin>(
    mut stream: RW,
    target: &SocksAddr,
    auth: Option<(&str, &str)>,
) -> Result<RW, Socks5Error> {
    // Step 1: Method negotiation
    let mut methods = vec![AUTH_NONE];
    if auth.is_some() {
        methods.push(AUTH_USERNAME_PASSWORD);
    }

    // Send: version=5, nmethods, methods
    stream.write_all(&[0x05, methods.len() as u8]).await?;
    stream.write_all(&methods).await?;
    stream.flush().await?;

    // Read: version=5, selected_method
    let version = stream.read_u8().await?;
    if version != 0x05 {
        return Err(Socks5Error::UnsupportedVersion(version));
    }

    let selected_method = stream.read_u8().await?;

    match selected_method {
        AUTH_NONE => {}
        AUTH_USERNAME_PASSWORD => {
            if let Some((username, password)) = auth {
                // Send username/password auth
                send_auth_request(&mut stream, username, password).await?;
                read_auth_response(&mut stream).await?;
            } else {
                return Err(Socks5Error::UnsupportedAuthMethod(selected_method));
            }
        }
        0xFF => return Err(Socks5Error::MethodNegotiationFailed),
        other => return Err(Socks5Error::UnsupportedAuthMethod(other)),
    }

    // Step 2: Send CONNECT request
    send_connect_request(&mut stream, target).await?;

    // Step 3: Read reply
    read_connect_reply(&mut stream).await?;

    Ok(stream)
}

/// Send a username/password authentication request.
async fn send_auth_request<W: AsyncWrite + Unpin>(
    writer: &mut W,
    username: &str,
    password: &str,
) -> Result<(), Socks5Error> {
    if username.len() > MAX_CRED_LEN || password.len() > MAX_CRED_LEN {
        return Err(Socks5Error::CredentialsTooLong);
    }

    // version=1, ulen, username, plen, password
    writer
        .write_all(&[AUTH_VERSION, username.len() as u8])
        .await?;
    writer.write_all(username.as_bytes()).await?;
    writer.write_all(&[password.len() as u8]).await?;
    writer.write_all(password.as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}

/// Read a username/password authentication response.
async fn read_auth_response<R: AsyncRead + Unpin>(reader: &mut R) -> Result<(), Socks5Error> {
    let version = reader.read_u8().await?;
    if version != AUTH_VERSION {
        return Err(Socks5Error::UnsupportedVersion(version));
    }

    let status = reader.read_u8().await?;
    if status != 0x00 {
        return Err(Socks5Error::AuthFailed);
    }

    Ok(())
}

/// Send a CONNECT request with the target address.
async fn send_connect_request<W: AsyncWrite + Unpin>(
    writer: &mut W,
    target: &SocksAddr,
) -> Result<(), Socks5Error> {
    // version=5, cmd=1 (CONNECT), rsv=0
    writer.write_all(&[0x05, 0x01, 0x00]).await?;

    match target {
        SocksAddr::IPv4(addr, port) => {
            writer.write_all(&[ATYP_IPV4]).await?;
            writer.write_all(addr).await?;
            writer.write_all(&port.to_be_bytes()).await?;
        }
        SocksAddr::Domain(domain, port) => {
            if domain.len() > MAX_CRED_LEN {
                return Err(Socks5Error::AddressTooLong);
            }
            writer.write_all(&[ATYP_DOMAIN, domain.len() as u8]).await?;
            writer.write_all(domain.as_bytes()).await?;
            writer.write_all(&port.to_be_bytes()).await?;
        }
        SocksAddr::IPv6(addr, port) => {
            writer.write_all(&[ATYP_IPV6]).await?;
            writer.write_all(addr).await?;
            writer.write_all(&port.to_be_bytes()).await?;
        }
    }

    writer.flush().await?;
    Ok(())
}

/// Read and validate a CONNECT reply from the server.
async fn read_connect_reply<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<SocksAddr, Socks5Error> {
    let version = reader.read_u8().await?;
    if version != 0x05 {
        return Err(Socks5Error::UnsupportedVersion(version));
    }

    let rep = reader.read_u8().await?;
    let _rsv = reader.read_u8().await?;

    // Read bind address (we don't use it, but need to consume the bytes)
    let atyp = reader.read_u8().await?;

    match atyp {
        ATYP_IPV4 => {
            let mut addr = [0u8; 4];
            reader.read_exact(&mut addr).await?;
            let port = reader.read_u16().await?;
            if rep != 0x00 {
                return Err(Socks5Error::ConnectionFailed(format!(
                    "SOCKS5 server returned error: {rep:#04x}"
                )));
            }
            Ok(SocksAddr::IPv4(addr, port))
        }
        ATYP_DOMAIN => {
            let len = reader.read_u8().await? as usize;
            let mut domain = vec![0u8; len];
            reader.read_exact(&mut domain).await?;
            let _port = reader.read_u16().await?;
            if rep != 0x00 {
                return Err(Socks5Error::ConnectionFailed(format!(
                    "SOCKS5 server returned error: {rep:#04x}"
                )));
            }
            let domain = String::from_utf8(domain).map_err(|e| {
                Socks5Error::MalformedMessage(format!("invalid domain in reply: {e}"))
            })?;
            Ok(SocksAddr::Domain(domain, _port))
        }
        ATYP_IPV6 => {
            let mut addr = [0u8; 16];
            reader.read_exact(&mut addr).await?;
            let port = reader.read_u16().await?;
            if rep != 0x00 {
                return Err(Socks5Error::ConnectionFailed(format!(
                    "SOCKS5 server returned error: {rep:#04x}"
                )));
            }
            Ok(SocksAddr::IPv6(addr, port))
        }
        _ => Err(Socks5Error::UnsupportedAddressType(atyp)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn test_send_auth_request() {
        let (mut client, mut server) = duplex(1024);

        send_auth_request(&mut client, "user", "pass")
            .await
            .unwrap();

        let mut buf = vec![0u8; 11]; // 1 + 1 + 4 + 1 + 4 = 11
        server.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf[0], AUTH_VERSION);
        assert_eq!(buf[1], 4); // ulen
        assert_eq!(&buf[2..6], b"user");
        assert_eq!(buf[6], 4); // plen
        assert_eq!(&buf[7..11], b"pass");
    }

    #[tokio::test]
    async fn test_send_auth_request_too_long() {
        let (mut client, _server) = duplex(2048);

        let long_username = "a".repeat(256);
        let result = send_auth_request(&mut client, &long_username, "pass").await;
        assert!(matches!(result, Err(Socks5Error::CredentialsTooLong)));
    }

    #[tokio::test]
    async fn test_read_auth_response_success() {
        let (mut client, mut server) = duplex(1024);

        server.write_all(&[AUTH_VERSION, 0x00]).await.unwrap();

        let result = read_auth_response(&mut client).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_auth_response_failure() {
        let (mut client, mut server) = duplex(1024);

        server.write_all(&[AUTH_VERSION, 0x01]).await.unwrap();

        let result = read_auth_response(&mut client).await;
        assert!(matches!(result, Err(Socks5Error::AuthFailed)));
    }

    #[tokio::test]
    async fn test_send_connect_request_ipv4() {
        let (mut client, mut server) = duplex(1024);

        let target = SocksAddr::IPv4([10, 0, 0, 1], 443);
        send_connect_request(&mut client, &target).await.unwrap();

        let mut buf = vec![0u8; 10];
        server.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, [0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1, 0x01, 0xBB]); // port 443 = 0x01BB
    }

    #[tokio::test]
    async fn test_send_connect_request_domain() {
        let (mut client, mut server) = duplex(1024);

        let target = SocksAddr::Domain("example.com".to_string(), 80);
        send_connect_request(&mut client, &target).await.unwrap();

        let mut buf = vec![0u8; 3 + 1 + 1 + 11 + 2]; // header(3) + atyp + len + domain + port
        server.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf[0], 0x05);
        assert_eq!(buf[1], 0x01); // CONNECT
        assert_eq!(buf[2], 0x00); // reserved
        assert_eq!(buf[3], 0x03); // domain
        assert_eq!(buf[4], 11); // domain len
        assert_eq!(&buf[5..16], b"example.com");
        assert_eq!(&buf[16..18], &[0x00, 0x50]); // port 80 = 0x0050
    }

    #[tokio::test]
    async fn test_send_connect_request_domain_too_long() {
        let (mut client, _server) = duplex(1024);

        let long_domain = "a".repeat(256);
        let target = SocksAddr::Domain(long_domain, 80);
        let result = send_connect_request(&mut client, &target).await;
        assert!(matches!(result, Err(Socks5Error::AddressTooLong)));
    }

    #[tokio::test]
    async fn test_read_connect_reply_success() {
        let (mut client, mut server) = duplex(1024);

        // Reply: version=5, rep=0 (success), rsv=0, atyp=1 (IPv4), addr, port
        server
            .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0])
            .await
            .unwrap();
        server.write_all(&0u16.to_be_bytes()).await.unwrap();

        let result = read_connect_reply(&mut client).await.unwrap();
        assert_eq!(result, SocksAddr::IPv4([0, 0, 0, 0], 0));
    }

    #[tokio::test]
    async fn test_read_connect_reply_failure() {
        let (mut client, mut server) = duplex(1024);

        // Reply: version=5, rep=1 (general failure), rsv=0, atyp=1
        server
            .write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0])
            .await
            .unwrap();
        server.write_all(&0u16.to_be_bytes()).await.unwrap();

        let result = read_connect_reply(&mut client).await;
        assert!(matches!(result, Err(Socks5Error::ConnectionFailed(_))));
    }

    #[tokio::test]
    async fn test_read_connect_reply_domain() {
        let (mut client, mut server) = duplex(1024);

        let domain = "example.com";
        let mut reply = vec![0x05, 0x00, 0x00, 0x03, domain.len() as u8];
        reply.extend_from_slice(domain.as_bytes());
        reply.extend_from_slice(&443u16.to_be_bytes());

        server.write_all(&reply).await.unwrap();

        let result = read_connect_reply(&mut client).await.unwrap();
        assert_eq!(result, SocksAddr::Domain("example.com".to_string(), 443));
    }

    #[tokio::test]
    async fn test_read_connect_reply_ipv6() {
        let (mut client, mut server) = duplex(1024);

        let ipv6_addr = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let mut reply = vec![0x05, 0x00, 0x00, 0x04];
        reply.extend_from_slice(&ipv6_addr);
        reply.extend_from_slice(&443u16.to_be_bytes());

        server.write_all(&reply).await.unwrap();

        let result = read_connect_reply(&mut client).await.unwrap();
        assert_eq!(result, SocksAddr::IPv6(ipv6_addr, 443));
    }

    #[tokio::test]
    async fn test_read_connect_reply_unsupported_atyp() {
        let (mut client, mut server) = duplex(1024);

        // Reply with unsupported address type
        server.write_all(&[0x05, 0x00, 0x00, 0x05]).await.unwrap();

        let result = read_connect_reply(&mut client).await;
        assert!(matches!(
            result,
            Err(Socks5Error::UnsupportedAddressType(0x05))
        ));
    }

    #[tokio::test]
    async fn test_read_connect_reply_bad_version() {
        let (mut client, mut server) = duplex(1024);

        // Reply with wrong version
        server
            .write_all(&[0x04, 0x00, 0x00, 0x01, 0, 0, 0, 0])
            .await
            .unwrap();
        server.write_all(&0u16.to_be_bytes()).await.unwrap();

        let result = read_connect_reply(&mut client).await;
        assert!(matches!(result, Err(Socks5Error::UnsupportedVersion(0x04))));
    }
}
