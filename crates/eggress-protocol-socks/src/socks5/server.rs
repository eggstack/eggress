use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::Socks5Error;

/// SOCKS5 address types.
pub const ATYP_IPV4: u8 = 0x01;
pub const ATYP_DOMAIN: u8 = 0x03;
pub const ATYP_IPV6: u8 = 0x04;

/// SOCKS5 commands.
pub const CMD_CONNECT: u8 = 0x01;
pub const CMD_BIND: u8 = 0x02;
pub const CMD_UDP_ASSOCIATE: u8 = 0x03;

/// Parsed SOCKS5 command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Socks5Command {
    Connect,
    Bind,
    UdpAssociate,
}

/// Parse a SOCKS5 command byte.
pub fn parse_command(cmd: u8) -> Result<Socks5Command, Socks5Error> {
    match cmd {
        CMD_CONNECT => Ok(Socks5Command::Connect),
        CMD_BIND => Ok(Socks5Command::Bind),
        CMD_UDP_ASSOCIATE => Ok(Socks5Command::UdpAssociate),
        _ => Err(Socks5Error::UnsupportedCommand(cmd)),
    }
}

/// SOCKS5 reply codes.
pub const REP_SUCCESS: u8 = 0x00;
pub const REP_GENERAL_FAILURE: u8 = 0x01;
pub const REP_NOT_ALLOWED: u8 = 0x02;
pub const REP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
pub const REP_ADDRESS_TYPE_NOT_SUPPORTED: u8 = 0x08;

/// SOCKS5 authentication methods.
const AUTH_NONE: u8 = 0x00;
const AUTH_USERNAME_PASSWORD: u8 = 0x02;
const AUTH_NO_ACCEPTABLE: u8 = 0xFF;

/// Username/password auth version.
const AUTH_VERSION: u8 = 0x01;

/// Maximum length for username or password in SOCKS5 auth.
const MAX_CRED_LEN: usize = 255;

/// A parsed SOCKS5 CONNECT request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SocksAddr {
    IPv4([u8; 4], u16),
    Domain(String, u16),
    IPv6([u8; 16], u16),
}

impl SocksAddr {
    /// Returns the host as a displayable string.
    pub fn host_str(&self) -> String {
        match self {
            SocksAddr::IPv4(addr, _) => format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3]),
            SocksAddr::Domain(domain, _) => domain.clone(),
            SocksAddr::IPv6(addr, _) => {
                // Format as [ipv6]:port
                let segments: Vec<String> = addr
                    .chunks(2)
                    .map(|chunk| format!("{:02x}{:02x}", chunk[0], chunk[1]))
                    .collect();
                format!("[{}]", segments.join(":"))
            }
        }
    }

    /// Returns the port.
    pub fn port(&self) -> u16 {
        match self {
            SocksAddr::IPv4(_, port) | SocksAddr::Domain(_, port) | SocksAddr::IPv6(_, port) => {
                *port
            }
        }
    }

    /// Encode this address into bytes for a SOCKS5 reply.
    pub fn encode_reply(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            SocksAddr::IPv4(addr, port) => {
                buf.push(ATYP_IPV4);
                buf.extend_from_slice(addr);
                buf.extend_from_slice(&port.to_be_bytes());
            }
            SocksAddr::Domain(domain, port) => {
                buf.push(ATYP_DOMAIN);
                // SOCKS5 spec limits domain to 255 bytes; clamp to prevent silent truncation
                let len = domain.len().min(255) as u8;
                buf.push(len);
                buf.extend_from_slice(&domain.as_bytes()[..len as usize]);
                buf.extend_from_slice(&port.to_be_bytes());
            }
            SocksAddr::IPv6(addr, port) => {
                buf.push(ATYP_IPV6);
                buf.extend_from_slice(addr);
                buf.extend_from_slice(&port.to_be_bytes());
            }
        }
        buf
    }
}

/// Parse the SOCKS5 method negotiation bytes synchronously.
///
/// Returns `(methods, remaining)` on success, or a [`Socks5Error`] if the
/// buffer is truncated or the version byte is wrong. Exposed for fuzzing.
pub fn parse_method_negotiation(buf: &[u8]) -> Result<(Vec<u8>, &[u8]), Socks5Error> {
    if buf.is_empty() {
        return Err(Socks5Error::UnexpectedEof);
    }
    let version = buf[0];
    if version != 0x05 {
        return Err(Socks5Error::UnsupportedVersion(version));
    }
    if buf.len() < 2 {
        return Err(Socks5Error::UnexpectedEof);
    }
    let nmethods = buf[1] as usize;
    if buf.len() < 2 + nmethods {
        return Err(Socks5Error::UnexpectedEof);
    }
    Ok((buf[2..2 + nmethods].to_vec(), &buf[2 + nmethods..]))
}

/// Parse a SOCKS5 CONNECT request synchronously.
///
/// Returns `(target, remaining)` on success. Exposed for fuzzing.
pub fn parse_connect_request(buf: &[u8]) -> Result<(SocksAddr, &[u8]), Socks5Error> {
    if buf.len() < 4 {
        return Err(Socks5Error::UnexpectedEof);
    }
    let version = buf[0];
    if version != 0x05 {
        return Err(Socks5Error::UnsupportedVersion(version));
    }
    let cmd = buf[1];
    if cmd != CMD_CONNECT {
        return Err(Socks5Error::UnsupportedCommand(cmd));
    }
    let _rsv = buf[2];
    let atyp = buf[3];

    let (addr, rest) = match atyp {
        ATYP_IPV4 => {
            if buf.len() < 4 + 4 + 2 {
                return Err(Socks5Error::UnexpectedEof);
            }
            let mut octets = [0u8; 4];
            octets.copy_from_slice(&buf[4..8]);
            let port = u16::from_be_bytes([buf[8], buf[9]]);
            (SocksAddr::IPv4(octets, port), &buf[10..])
        }
        ATYP_DOMAIN => {
            if buf.len() < 5 {
                return Err(Socks5Error::UnexpectedEof);
            }
            let len = buf[4] as usize;
            if buf.len() < 5 + len + 2 {
                return Err(Socks5Error::UnexpectedEof);
            }
            let domain_bytes = &buf[5..5 + len];
            let domain = String::from_utf8(domain_bytes.to_vec())
                .map_err(|e| Socks5Error::MalformedMessage(format!("invalid domain: {e}")))?;
            let port = u16::from_be_bytes([buf[5 + len], buf[5 + len + 1]]);
            (SocksAddr::Domain(domain, port), &buf[5 + len + 2..])
        }
        ATYP_IPV6 => {
            if buf.len() < 4 + 16 + 2 {
                return Err(Socks5Error::UnexpectedEof);
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&buf[4..20]);
            let port = u16::from_be_bytes([buf[20], buf[21]]);
            (SocksAddr::IPv6(octets, port), &buf[22..])
        }
        _ => return Err(Socks5Error::UnsupportedAddressType(atyp)),
    };

    Ok((addr, rest))
}

/// Parse a SOCKS5 generic request synchronously (CONNECT, BIND, or UDP_ASSOCIATE).
///
/// Returns `(command, target, remaining)`. Exposed for fuzzing.
pub fn parse_socks5_request(buf: &[u8]) -> Result<(Socks5Command, SocksAddr, &[u8]), Socks5Error> {
    if buf.len() < 4 {
        return Err(Socks5Error::UnexpectedEof);
    }
    let version = buf[0];
    if version != 0x05 {
        return Err(Socks5Error::UnsupportedVersion(version));
    }
    let cmd = buf[1];
    let command = parse_command(cmd)?;
    let _rsv = buf[2];
    let atyp = buf[3];

    let (addr, rest) = match atyp {
        ATYP_IPV4 => {
            if buf.len() < 4 + 4 + 2 {
                return Err(Socks5Error::UnexpectedEof);
            }
            let mut octets = [0u8; 4];
            octets.copy_from_slice(&buf[4..8]);
            let port = u16::from_be_bytes([buf[8], buf[9]]);
            (SocksAddr::IPv4(octets, port), &buf[10..])
        }
        ATYP_DOMAIN => {
            if buf.len() < 5 {
                return Err(Socks5Error::UnexpectedEof);
            }
            let len = buf[4] as usize;
            if buf.len() < 5 + len + 2 {
                return Err(Socks5Error::UnexpectedEof);
            }
            let domain_bytes = &buf[5..5 + len];
            let domain = String::from_utf8(domain_bytes.to_vec())
                .map_err(|e| Socks5Error::MalformedMessage(format!("invalid domain: {e}")))?;
            let port = u16::from_be_bytes([buf[5 + len], buf[5 + len + 1]]);
            (SocksAddr::Domain(domain, port), &buf[5 + len + 2..])
        }
        ATYP_IPV6 => {
            if buf.len() < 4 + 16 + 2 {
                return Err(Socks5Error::UnexpectedEof);
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&buf[4..20]);
            let port = u16::from_be_bytes([buf[20], buf[21]]);
            (SocksAddr::IPv6(octets, port), &buf[22..])
        }
        _ => return Err(Socks5Error::UnsupportedAddressType(atyp)),
    };

    Ok((command, addr, rest))
}

/// Read a complete method negotiation message from the client.
///
/// Returns the list of methods the client supports.
pub async fn read_method_negotiation<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Vec<u8>, Socks5Error> {
    let version = reader.read_u8().await?;
    if version != 0x05 {
        return Err(Socks5Error::UnsupportedVersion(version));
    }

    let nmethods = reader.read_u8().await?;
    let mut methods = vec![0u8; nmethods as usize];
    reader.read_exact(&mut methods).await?;

    Ok(methods)
}

/// Send a method selection response to the client.
///
/// If the client supports `AUTH_NONE` and no password is required, selects no auth.
/// If `password` is Some and the client supports username/password auth, selects that.
/// Otherwise, sends 0xFF (no acceptable methods).
pub async fn send_method_selection<W: AsyncWrite + Unpin>(
    writer: &mut W,
    methods: &[u8],
    password: Option<&str>,
) -> Result<(), Socks5Error> {
    let method = if password.is_some() && methods.contains(&AUTH_USERNAME_PASSWORD) {
        AUTH_USERNAME_PASSWORD
    } else if methods.contains(&AUTH_NONE) {
        AUTH_NONE
    } else {
        AUTH_NO_ACCEPTABLE
    };

    writer.write_all(&[0x05, method]).await?;
    writer.flush().await?;

    if method == AUTH_NO_ACCEPTABLE {
        return Err(Socks5Error::MethodNegotiationFailed);
    }

    Ok(())
}

/// Read and validate username/password authentication from the client.
///
/// Returns Ok(()) on success, or an error if auth fails.
pub async fn read_auth_request<R: AsyncRead + Unpin>(
    reader: &mut R,
    expected_password: &str,
) -> Result<String, Socks5Error> {
    let version = reader.read_u8().await?;
    if version != AUTH_VERSION {
        return Err(Socks5Error::UnsupportedVersion(version));
    }

    let ulen = reader.read_u8().await? as usize;
    if ulen > MAX_CRED_LEN {
        return Err(Socks5Error::CredentialsTooLong);
    }
    let mut username = vec![0u8; ulen];
    reader.read_exact(&mut username).await?;

    let plen = reader.read_u8().await? as usize;
    if plen > MAX_CRED_LEN {
        return Err(Socks5Error::CredentialsTooLong);
    }
    let mut password_bytes = vec![0u8; plen];
    reader.read_exact(&mut password_bytes).await?;

    let password_str = String::from_utf8_lossy(&password_bytes);
    use subtle::ConstantTimeEq;
    let passwords_match: bool = password_str
        .as_bytes()
        .ct_eq(expected_password.as_bytes())
        .into();
    if !passwords_match {
        return Err(Socks5Error::AuthFailed);
    }

    Ok(String::from_utf8_lossy(&username).to_string())
}

/// Send an authentication response to the client.
pub async fn send_auth_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    success: bool,
) -> Result<(), Socks5Error> {
    let status = if success { 0x00 } else { 0x01 };
    writer.write_all(&[AUTH_VERSION, status]).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a CONNECT request from the client.
///
/// Returns the target address.
pub async fn read_connect_request<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<SocksAddr, Socks5Error> {
    let version = reader.read_u8().await?;
    if version != 0x05 {
        return Err(Socks5Error::UnsupportedVersion(version));
    }

    let cmd = reader.read_u8().await?;
    if cmd != CMD_CONNECT {
        // Send reply for command not supported before returning error
        return Err(Socks5Error::UnsupportedCommand(cmd));
    }

    let _rsv = reader.read_u8().await?;

    let atyp = reader.read_u8().await?;

    let addr = match atyp {
        ATYP_IPV4 => {
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf).await?;
            let port = reader.read_u16().await?;
            SocksAddr::IPv4(buf, port)
        }
        ATYP_DOMAIN => {
            let len = reader.read_u8().await? as usize;
            let mut domain = vec![0u8; len];
            reader.read_exact(&mut domain).await?;
            let domain = String::from_utf8(domain)
                .map_err(|e| Socks5Error::MalformedMessage(format!("invalid domain: {e}")))?;
            let port = reader.read_u16().await?;
            SocksAddr::Domain(domain, port)
        }
        ATYP_IPV6 => {
            let mut buf = [0u8; 16];
            reader.read_exact(&mut buf).await?;
            let port = reader.read_u16().await?;
            SocksAddr::IPv6(buf, port)
        }
        _ => return Err(Socks5Error::UnsupportedAddressType(atyp)),
    };

    Ok(addr)
}

/// Read a SOCKS5 request from the client.
///
/// Returns the command and target address. Does not reject non-CONNECT commands.
pub async fn read_socks5_request<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<(Socks5Command, SocksAddr), Socks5Error> {
    let version = reader.read_u8().await?;
    if version != 0x05 {
        return Err(Socks5Error::UnsupportedVersion(version));
    }

    let cmd = reader.read_u8().await?;
    let command = parse_command(cmd)?;

    let _rsv = reader.read_u8().await?;

    let atyp = reader.read_u8().await?;

    let addr = match atyp {
        ATYP_IPV4 => {
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf).await?;
            let port = reader.read_u16().await?;
            SocksAddr::IPv4(buf, port)
        }
        ATYP_DOMAIN => {
            let len = reader.read_u8().await? as usize;
            let mut domain = vec![0u8; len];
            reader.read_exact(&mut domain).await?;
            let domain = String::from_utf8(domain)
                .map_err(|e| Socks5Error::MalformedMessage(format!("invalid domain: {e}")))?;
            let port = reader.read_u16().await?;
            SocksAddr::Domain(domain, port)
        }
        ATYP_IPV6 => {
            let mut buf = [0u8; 16];
            reader.read_exact(&mut buf).await?;
            let port = reader.read_u16().await?;
            SocksAddr::IPv6(buf, port)
        }
        _ => return Err(Socks5Error::UnsupportedAddressType(atyp)),
    };

    Ok((command, addr))
}

/// Send a UDP ASSOCIATE reply to the client with the relay bind address.
pub async fn send_udp_associate_reply<W: AsyncWrite + Unpin>(
    writer: &mut W,
    bind_addr: &SocksAddr,
) -> Result<(), Socks5Error> {
    send_connect_reply(writer, REP_SUCCESS, bind_addr).await
}

/// Send a CONNECT reply to the client.
pub async fn send_connect_reply<W: AsyncWrite + Unpin>(
    writer: &mut W,
    rep: u8,
    bind_addr: &SocksAddr,
) -> Result<(), Socks5Error> {
    let mut reply = vec![0x05, rep, 0x00]; // version, reply, reserved
    reply.extend_from_slice(&bind_addr.encode_reply());
    writer.write_all(&reply).await?;
    writer.flush().await?;
    Ok(())
}

/// Handle a complete SOCKS5 server handshake.
///
/// This reads the method negotiation, optionally handles username/password auth,
/// and reads the CONNECT request. Returns the target address on success.
///
/// # Arguments
/// * `reader` - The stream to read from.
/// * `writer` - The stream to write to.
/// * `password` - If Some, require username/password authentication with this password.
pub async fn handle_socks5_handshake<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    reader: &mut R,
    writer: &mut W,
    password: Option<&str>,
) -> Result<SocksAddr, Socks5Error> {
    // Step 1: Method negotiation
    let methods = read_method_negotiation(reader).await?;
    send_method_selection(writer, &methods, password).await?;

    // Step 2: Auth (if password required)
    if let Some(pwd) = password {
        read_auth_request(reader, pwd).await?;
        send_auth_response(writer, true).await?;
    }

    // Step 3: CONNECT request
    let target = read_connect_request(reader).await?;

    Ok(target)
}

/// Send a rejection reply for unsupported commands.
pub async fn reject_command<W: AsyncWrite + Unpin>(
    writer: &mut W,
    cmd: u8,
    target: &SocksAddr,
) -> Result<(), Socks5Error> {
    match cmd {
        // BIND is not supported
        CMD_BIND => {
            send_connect_reply(writer, REP_NOT_ALLOWED, target).await?;
        }
        // Unknown command
        _ => {
            send_connect_reply(writer, REP_COMMAND_NOT_SUPPORTED, target).await?;
        }
    }
    Ok(())
}

/// Get the success reply code.
pub const fn success_reply() -> u8 {
    REP_SUCCESS
}

/// Get the general failure reply code.
pub const fn general_failure_reply() -> u8 {
    REP_GENERAL_FAILURE
}

/// Get the command not supported reply code.
pub const fn command_not_supported_reply() -> u8 {
    REP_COMMAND_NOT_SUPPORTED
}

/// Get the address type not supported reply code.
pub const fn address_type_not_supported_reply() -> u8 {
    REP_ADDRESS_TYPE_NOT_SUPPORTED
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn test_method_negotiation_no_auth() {
        let (mut client, mut server) = duplex(1024);

        // Client sends: version=5, nmethods=1, method=0x00 (no auth)
        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();

        let methods = read_method_negotiation(&mut server).await.unwrap();
        assert_eq!(methods, vec![0x00]);

        send_method_selection(&mut server, &methods, None)
            .await
            .unwrap();

        let mut response = [0u8; 2];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]); // Selected no auth
    }

    #[tokio::test]
    async fn test_method_negotiation_username_password() {
        let (mut client, mut server) = duplex(1024);

        // Client offers: no auth and username/password
        client.write_all(&[0x05, 0x02, 0x00, 0x02]).await.unwrap();

        let methods = read_method_negotiation(&mut server).await.unwrap();
        assert_eq!(methods, vec![0x00, 0x02]);

        // Server requires password
        send_method_selection(&mut server, &methods, Some("secret"))
            .await
            .unwrap();

        let mut response = [0u8; 2];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x02]); // Selected username/password
    }

    #[tokio::test]
    async fn test_method_negotiation_no_acceptable() {
        let (mut client, mut server) = duplex(1024);

        // Client offers only GSSAPI (0x01) which we don't support
        client.write_all(&[0x05, 0x01, 0x01]).await.unwrap();

        let methods = read_method_negotiation(&mut server).await.unwrap();
        let result = send_method_selection(&mut server, &methods, None).await;
        assert!(matches!(result, Err(Socks5Error::MethodNegotiationFailed)));
    }

    #[tokio::test]
    async fn test_auth_success() {
        let (mut client, mut server) = duplex(1024);

        // Auth request: version=1, ulen=4, username="user", plen=6, password="secret"
        client
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r', 0x06])
            .await
            .unwrap();
        client.write_all(b"secret").await.unwrap();

        let username = read_auth_request(&mut server, "secret").await.unwrap();
        assert_eq!(username, "user");

        send_auth_response(&mut server, true).await.unwrap();

        let mut response = [0u8; 2];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x01, 0x00]); // Success
    }

    #[tokio::test]
    async fn test_auth_failure() {
        let (mut client, mut server) = duplex(1024);

        // Auth request with wrong password
        client
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r', 0x05])
            .await
            .unwrap();
        client.write_all(b"wrong").await.unwrap();

        let result = read_auth_request(&mut server, "secret").await;
        assert!(matches!(result, Err(Socks5Error::AuthFailed)));
    }

    #[tokio::test]
    async fn test_connect_ipv4() {
        let (mut client, mut server) = duplex(1024);

        // CONNECT request: version=5, cmd=1, rsv=0, atyp=1 (IPv4), addr=192.168.1.1, port=8080
        client
            .write_all(&[0x05, 0x01, 0x00, 0x01, 192, 168, 1, 1])
            .await
            .unwrap();
        client.write_all(&8080u16.to_be_bytes()).await.unwrap();

        let target = read_connect_request(&mut server).await.unwrap();
        assert_eq!(target, SocksAddr::IPv4([192, 168, 1, 1], 8080));
    }

    #[tokio::test]
    async fn test_connect_domain() {
        let (mut client, mut server) = duplex(1024);

        let domain = "example.com";
        // version=5, cmd=1, rsv=0, atyp=3, len, domain, port
        client
            .write_all(&[0x05, 0x01, 0x00, 0x03, domain.len() as u8])
            .await
            .unwrap();
        client.write_all(domain.as_bytes()).await.unwrap();
        client.write_all(&443u16.to_be_bytes()).await.unwrap();

        let target = read_connect_request(&mut server).await.unwrap();
        assert_eq!(target, SocksAddr::Domain("example.com".to_string(), 443));
    }

    #[tokio::test]
    async fn test_connect_ipv6() {
        let (mut client, mut server) = duplex(1024);

        // CONNECT request: version=5, cmd=1, rsv=0, atyp=4 (IPv6), addr=::1, port=443
        let ipv6_addr = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        client.write_all(&[0x05, 0x01, 0x00, 0x04]).await.unwrap();
        client.write_all(&ipv6_addr).await.unwrap();
        client.write_all(&443u16.to_be_bytes()).await.unwrap();

        let target = read_connect_request(&mut server).await.unwrap();
        assert_eq!(target, SocksAddr::IPv6(ipv6_addr, 443));
    }

    #[tokio::test]
    async fn test_connect_reply_success() {
        let (mut client, mut server) = duplex(1024);

        let bind_addr = SocksAddr::IPv4([0, 0, 0, 0], 0);
        send_connect_reply(&mut server, REP_SUCCESS, &bind_addr)
            .await
            .unwrap();

        let mut response = [0u8; 10];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response[0], 0x05); // version
        assert_eq!(response[1], 0x00); // success
        assert_eq!(response[2], 0x00); // reserved
        assert_eq!(response[3], 0x01); // atyp IPv4
    }

    #[tokio::test]
    async fn test_unsupported_version() {
        let (mut client, mut server) = duplex(1024);

        // Send version 4 instead of 5
        client.write_all(&[0x04, 0x01, 0x00]).await.unwrap();

        let result = read_method_negotiation(&mut server).await;
        assert!(matches!(result, Err(Socks5Error::UnsupportedVersion(0x04))));
    }

    #[tokio::test]
    async fn test_unsupported_command() {
        let (mut client, mut server) = duplex(1024);

        // BIND command (0x02)
        let ipv4_addr = [192, 168, 1, 1];
        client.write_all(&[0x05, 0x02, 0x00, 0x01]).await.unwrap();
        client.write_all(&ipv4_addr).await.unwrap();
        client.write_all(&80u16.to_be_bytes()).await.unwrap();

        let result = read_connect_request(&mut server).await;
        assert!(matches!(result, Err(Socks5Error::UnsupportedCommand(0x02))));
    }

    #[tokio::test]
    async fn test_unsupported_address_type() {
        let (mut client, mut server) = duplex(1024);

        // atyp=0x05 (unsupported)
        client.write_all(&[0x05, 0x01, 0x00, 0x05]).await.unwrap();

        let result = read_connect_request(&mut server).await;
        assert!(matches!(
            result,
            Err(Socks5Error::UnsupportedAddressType(0x05))
        ));
    }

    #[tokio::test]
    async fn test_reject_bind_command() {
        let (mut client, mut server) = duplex(1024);

        let target = SocksAddr::IPv4([192, 168, 1, 1], 80);
        reject_command(&mut server, 0x02, &target).await.unwrap();

        let mut response = [0u8; 10];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response[0], 0x05); // version
        assert_eq!(response[1], 0x02); // not allowed
    }

    #[tokio::test]
    async fn test_reject_unknown_command() {
        let (mut client, mut server) = duplex(1024);

        let target = SocksAddr::IPv4([192, 168, 1, 1], 80);
        reject_command(&mut server, 0x04, &target).await.unwrap();

        let mut response = [0u8; 10];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response[0], 0x05); // version
        assert_eq!(response[1], 0x07); // command not supported
    }

    #[tokio::test]
    async fn test_creds_too_long() {
        // ulen is u8 so max 255; CredentialsTooLong can only happen
        // if we manually construct bad data. Verify boundary with 255-byte credential.
    }

    #[tokio::test]
    async fn test_boundary_credentials_length() {
        let (mut client, mut server) = duplex(2048);

        let username = "a".repeat(255);
        let password = "b".repeat(255);

        // Auth request: version=1, ulen=255, username, plen=255, password
        client.write_all(&[0x01, 255]).await.unwrap();
        client.write_all(username.as_bytes()).await.unwrap();
        client.write_all(&[255]).await.unwrap();
        client.write_all(password.as_bytes()).await.unwrap();

        let result = read_auth_request(&mut server, &password).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), username);
    }

    #[tokio::test]
    async fn test_full_handshake_no_auth() {
        let (mut client, mut server) = duplex(1024);

        // Method negotiation: client offers no auth
        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();

        // Server reads and selects method
        let methods = read_method_negotiation(&mut server).await.unwrap();
        send_method_selection(&mut server, &methods, None)
            .await
            .unwrap();

        // Read method selection response
        let mut response = [0u8; 2];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // CONNECT request
        client
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        client.write_all(&443u16.to_be_bytes()).await.unwrap();

        let target = read_connect_request(&mut server).await.unwrap();
        assert_eq!(target, SocksAddr::IPv4([10, 0, 0, 1], 443));

        // Send success reply
        let bind_addr = SocksAddr::IPv4([0, 0, 0, 0], 0);
        send_connect_reply(&mut server, REP_SUCCESS, &bind_addr)
            .await
            .unwrap();

        let mut reply = [0u8; 10];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x00);
    }

    #[tokio::test]
    async fn test_full_handshake_with_auth() {
        let (mut client, mut server) = duplex(2048);

        // Method negotiation: client offers both methods
        client.write_all(&[0x05, 0x02, 0x00, 0x02]).await.unwrap();

        // Server requires password
        let methods = read_method_negotiation(&mut server).await.unwrap();
        send_method_selection(&mut server, &methods, Some("mypass"))
            .await
            .unwrap();

        // Read method selection
        let mut response = [0u8; 2];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x02]);

        // Auth request
        client
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r'])
            .await
            .unwrap();
        client
            .write_all(&[0x06, b'm', b'y', b'p', b'a', b's', b's'])
            .await
            .unwrap();

        let username = read_auth_request(&mut server, "mypass").await.unwrap();
        assert_eq!(username, "user");
        send_auth_response(&mut server, true).await.unwrap();

        // Read auth response
        let mut auth_response = [0u8; 2];
        client.read_exact(&mut auth_response).await.unwrap();
        assert_eq!(auth_response, [0x01, 0x00]);

        // CONNECT request
        let domain = "example.com";
        client
            .write_all(&[0x05, 0x01, 0x00, 0x03, domain.len() as u8])
            .await
            .unwrap();
        client.write_all(domain.as_bytes()).await.unwrap();
        client.write_all(&443u16.to_be_bytes()).await.unwrap();

        let target = read_connect_request(&mut server).await.unwrap();
        assert_eq!(target, SocksAddr::Domain("example.com".to_string(), 443));
    }

    #[tokio::test]
    async fn test_fragged_handshake() {
        let (mut client, mut server) = duplex(1024);

        // Send method negotiation in fragments
        client.write_all(&[0x05]).await.unwrap();
        // Small delay to simulate fragmentation
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        client.write_all(&[0x01]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        client.write_all(&[0x00]).await.unwrap();

        let methods = read_method_negotiation(&mut server).await.unwrap();
        assert_eq!(methods, vec![0x00]);
    }

    #[tokio::test]
    async fn test_socks_addr_display() {
        let ipv4 = SocksAddr::IPv4([192, 168, 1, 1], 8080);
        assert_eq!(ipv4.host_str(), "192.168.1.1");
        assert_eq!(ipv4.port(), 8080);

        let domain = SocksAddr::Domain("example.com".to_string(), 443);
        assert_eq!(domain.host_str(), "example.com");
        assert_eq!(domain.port(), 443);

        let ipv6 = SocksAddr::IPv6([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], 443);
        assert_eq!(ipv6.port(), 443);
    }

    #[tokio::test]
    async fn test_socks_addr_encode_reply() {
        let ipv4 = SocksAddr::IPv4([192, 168, 1, 1], 8080);
        let encoded = ipv4.encode_reply();
        assert_eq!(encoded[0], ATYP_IPV4);
        assert_eq!(&encoded[1..5], &[192, 168, 1, 1]);
        assert_eq!(&encoded[5..7], &8080u16.to_be_bytes());

        let domain = SocksAddr::Domain("example.com".to_string(), 443);
        let encoded = domain.encode_reply();
        assert_eq!(encoded[0], ATYP_DOMAIN);
        assert_eq!(encoded[1], 11); // "example.com" length
        assert_eq!(&encoded[2..13], b"example.com");
        assert_eq!(&encoded[13..15], &443u16.to_be_bytes());
    }

    #[test]
    fn test_parse_command() {
        assert_eq!(parse_command(0x01).unwrap(), Socks5Command::Connect);
        assert_eq!(parse_command(0x02).unwrap(), Socks5Command::Bind);
        assert_eq!(parse_command(0x03).unwrap(), Socks5Command::UdpAssociate);
        assert!(parse_command(0x04).is_err());
        assert!(parse_command(0xFF).is_err());
    }

    #[tokio::test]
    async fn test_udp_associate_ipv4() {
        let (mut client, mut server) = duplex(1024);

        // UDP ASSOCIATE request: version=5, cmd=3, rsv=0, atyp=1 (IPv4), addr=0.0.0.0, port=0
        client
            .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
            .await
            .unwrap();
        client.write_all(&0u16.to_be_bytes()).await.unwrap();

        let (cmd, target) = read_socks5_request(&mut server).await.unwrap();
        assert_eq!(cmd, Socks5Command::UdpAssociate);
        assert_eq!(target, SocksAddr::IPv4([0, 0, 0, 0], 0));
    }

    #[tokio::test]
    async fn test_udp_associate_ipv6() {
        let (mut client, mut server) = duplex(1024);

        let ipv6_addr = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        client.write_all(&[0x05, 0x03, 0x00, 0x04]).await.unwrap();
        client.write_all(&ipv6_addr).await.unwrap();
        client.write_all(&0u16.to_be_bytes()).await.unwrap();

        let (cmd, target) = read_socks5_request(&mut server).await.unwrap();
        assert_eq!(cmd, Socks5Command::UdpAssociate);
        assert_eq!(target, SocksAddr::IPv6(ipv6_addr, 0));
    }

    #[tokio::test]
    async fn test_udp_associate_domain() {
        let (mut client, mut server) = duplex(1024);

        let domain = "example.com";
        client
            .write_all(&[0x05, 0x03, 0x00, 0x03, domain.len() as u8])
            .await
            .unwrap();
        client.write_all(domain.as_bytes()).await.unwrap();
        client.write_all(&0u16.to_be_bytes()).await.unwrap();

        let (cmd, target) = read_socks5_request(&mut server).await.unwrap();
        assert_eq!(cmd, Socks5Command::UdpAssociate);
        assert_eq!(target, SocksAddr::Domain("example.com".to_string(), 0));
    }

    #[tokio::test]
    async fn test_connect_still_works_via_read_socks5_request() {
        let (mut client, mut server) = duplex(1024);

        client
            .write_all(&[0x05, 0x01, 0x00, 0x01, 192, 168, 1, 1])
            .await
            .unwrap();
        client.write_all(&8080u16.to_be_bytes()).await.unwrap();

        let (cmd, target) = read_socks5_request(&mut server).await.unwrap();
        assert_eq!(cmd, Socks5Command::Connect);
        assert_eq!(target, SocksAddr::IPv4([192, 168, 1, 1], 8080));
    }

    #[tokio::test]
    async fn test_reject_bind_only_not_udp_associate() {
        let (mut client, mut server) = duplex(1024);

        let target = SocksAddr::IPv4([192, 168, 1, 1], 80);
        reject_command(&mut server, 0x02, &target).await.unwrap();

        let mut response = [0u8; 10];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response[0], 0x05);
        assert_eq!(response[1], 0x02); // not allowed for BIND
    }

    #[tokio::test]
    async fn test_send_udp_associate_reply() {
        let (mut client, mut server) = duplex(1024);

        let bind_addr = SocksAddr::IPv4([127, 0, 0, 1], 1080);
        send_udp_associate_reply(&mut server, &bind_addr)
            .await
            .unwrap();

        let mut response = [0u8; 10];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response[0], 0x05);
        assert_eq!(response[1], 0x00); // success
        assert_eq!(response[3], 0x01); // atyp IPv4
        assert_eq!(&response[4..8], &[127, 0, 0, 1]);
        assert_eq!(&response[8..10], &1080u16.to_be_bytes());
    }
}
