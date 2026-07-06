use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use eggress_core::BoxStream;
use eggress_core::{ClientIdentity, ProtocolId, TargetAddr, TargetHost};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Authentication policy for inbound connections.
#[derive(Clone)]
pub enum InboundAuthentication {
    None,
    UsernamePassword { username: String, password: String },
}

impl fmt::Debug for InboundAuthentication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InboundAuthentication::None => write!(f, "InboundAuthentication::None"),
            InboundAuthentication::UsernamePassword { .. } => {
                write!(f, "InboundAuthentication::UsernamePassword {{ .. }}")
            }
        }
    }
}

impl fmt::Display for InboundAuthentication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InboundAuthentication::None => write!(f, "none"),
            InboundAuthentication::UsernamePassword { .. } => write!(f, "username/password"),
        }
    }
}

/// Error type for accept operations.
#[derive(Debug, thiserror::Error)]
pub enum AcceptError {
    #[error("protocol error")]
    Protocol(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("authentication failed")]
    AuthenticationFailed,
}

impl From<Box<dyn std::error::Error + Send + Sync>> for AcceptError {
    fn from(e: Box<dyn std::error::Error + Send + Sync>) -> Self {
        AcceptError::Protocol(e)
    }
}

/// The result of accepting an inbound connection.
pub enum AcceptedSession {
    Tunnel(PendingTunnel),
    HttpForward(PendingHttpForward),
    UdpAssociate(PendingUdpAssociate),
}

/// A pending tunnel connection (HTTP CONNECT, SOCKS4, SOCKS5).
/// Success reply has NOT been sent yet.
pub struct PendingTunnel {
    pub target: TargetAddr,
    pub client: BoxStream,
    pub protocol: TunnelProtocol,
    pub reply_context: ReplyContext,
    pub identity: ClientIdentity,
}

/// A pending HTTP forward-proxy request.
pub struct PendingHttpForward {
    pub target: TargetAddr,
    pub client: BoxStream,
    pub request: eggress_protocol_http::forward::ForwardRequest,
    pub identity: ClientIdentity,
}

/// A pending SOCKS5 UDP ASSOCIATE session.
pub struct PendingUdpAssociate {
    pub client: BoxStream,
    pub protocol: TunnelProtocol,
    pub identity: ClientIdentity,
    pub client_hint: Option<TargetAddr>,
}

/// Which tunnel protocol was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelProtocol {
    HttpConnect,
    Socks4,
    Socks5,
    Shadowsocks,
    Trojan,
}

/// Information needed to send a protocol-specific reply later.
pub enum ReplyContext {
    Http,
    Socks4,
    Socks5,
    Shadowsocks,
    Trojan,
}

/// Configuration for Shadowsocks inbound listener.
#[derive(Clone)]
pub struct InboundShadowsocksConfig {
    pub method: String,
    pub password: String,
}

/// Configuration for Trojan inbound listener.
#[derive(Clone)]
pub struct InboundTrojanConfig {
    pub password: String,
}

/// A stream that returns `prefix` bytes first, then delegates to `inner`.
struct PrefixedStream {
    prefix: std::io::Cursor<Vec<u8>>,
    inner: BoxStream,
}

impl PrefixedStream {
    fn new(prefix: Vec<u8>, inner: BoxStream) -> Self {
        Self {
            prefix: std::io::Cursor::new(prefix),
            inner,
        }
    }
}

impl AsyncRead for PrefixedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let pos = self.prefix.position() as usize;
        let len = self.prefix.get_ref().len();
        if pos < len {
            let remaining = &self.prefix.get_ref()[pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.prefix.set_position((pos + to_copy) as u64);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Read the first byte from the stream, detect the protocol, perform the
/// handshake parsing, and return an `AcceptedSession` **without** opening
/// any outbound connection or sending any success/failure reply.
pub async fn accept(
    client: BoxStream,
    protocols: &[ProtocolId],
    auth: &InboundAuthentication,
    shadowsocks_config: Option<&InboundShadowsocksConfig>,
    shadowsocks_metrics: Option<&std::sync::Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>>,
    trojan_config: Option<&InboundTrojanConfig>,
) -> Result<AcceptedSession, AcceptError> {
    #[inline]
    fn shadows_metrics(
        m: Option<&std::sync::Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>>,
    ) -> Option<std::sync::Arc<eggress_protocol_shadowsocks::ShadowsocksMetrics>> {
        m.cloned()
    }
    let mut stream = client;
    let mut first_byte = [0u8; 1];
    stream
        .read_exact(&mut first_byte)
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

    let has_socks5 = protocols.contains(&ProtocolId::Socks5);
    let has_socks4 = protocols.contains(&ProtocolId::Socks4);
    let has_http = protocols.contains(&ProtocolId::Http);

    // Check SOCKS5
    if first_byte[0] == 0x05 && has_socks5 {
        tracing::trace!(
            "detected protocol: socks5 (first_byte={:#04x})",
            first_byte[0]
        );
        let stream: BoxStream = Box::new(PrefixedStream::new(first_byte.to_vec(), stream));
        return accept_socks5(stream, auth).await;
    }

    // Check SOCKS4
    if first_byte[0] == 0x04 && has_socks4 {
        tracing::trace!(
            "detected protocol: socks4 (first_byte={:#04x})",
            first_byte[0]
        );
        let stream: BoxStream = Box::new(PrefixedStream::new(first_byte.to_vec(), stream));
        return accept_socks4(stream).await;
    }

    // Try HTTP detection if HTTP is allowed
    if has_http {
        // Read more bytes to detect the HTTP method
        let mut prefix = vec![first_byte[0]];
        let mut buf = [0u8; 32];
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
        prefix.extend_from_slice(&buf[..n]);

        match detect_http_method(&prefix) {
            DetectResult::Match => {
                tracing::trace!(
                    "detected protocol: http (prefix={:?})",
                    &prefix[..prefix.len().min(16)]
                );
                let stream: BoxStream = Box::new(PrefixedStream::new(prefix, stream));
                return accept_http(stream, auth).await;
            }
            DetectResult::NeedMore => {
                // Read more bytes
                let mut more = [0u8; 32];
                let n = stream
                    .read(&mut more)
                    .await
                    .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
                prefix.extend_from_slice(&more[..n]);
                match detect_http_method(&prefix) {
                    DetectResult::Match => {
                        tracing::trace!(
                            "detected protocol: http (prefix={:?})",
                            &prefix[..prefix.len().min(16)]
                        );
                        let stream: BoxStream = Box::new(PrefixedStream::new(prefix, stream));
                        return accept_http(stream, auth).await;
                    }
                    DetectResult::NoMatch => {
                        return Err(AcceptError::Protocol(
                            "no matching protocol for listener".into(),
                        ));
                    }
                    DetectResult::NeedMore => {
                        return Err(AcceptError::Protocol(
                            "no matching protocol for listener".into(),
                        ));
                    }
                }
            }
            DetectResult::NoMatch => {
                return Err(AcceptError::Protocol(
                    "no matching protocol for listener".into(),
                ));
            }
        }
    }

    // Check if Shadowsocks is the only protocol (auto-detection not possible)
    if protocols.len() == 1 && protocols.contains(&ProtocolId::Shadowsocks) {
        if let Some(ss_config) = shadowsocks_config {
            let method =
                eggress_protocol_shadowsocks::CipherMethod::parse_method(&ss_config.method)
                    .map_err(|e| {
                        if let Some(m) = shadowsocks_metrics {
                            m.record_tcp_unsupported_method_reject();
                        }
                        AcceptError::Protocol(Box::new(e))
                    })?;

            let stream: BoxStream = Box::new(PrefixedStream::new(first_byte.to_vec(), stream));
            let (ss_stream, target_addr) = eggress_protocol_shadowsocks::tcp::shadowsocks_accept(
                stream,
                &ss_config.password,
                method,
                shadows_metrics(shadowsocks_metrics),
            )
            .await
            .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

            return Ok(AcceptedSession::Tunnel(PendingTunnel {
                target: target_addr,
                client: ss_stream,
                protocol: TunnelProtocol::Shadowsocks,
                reply_context: ReplyContext::Shadowsocks,
                identity: ClientIdentity::Anonymous,
            }));
        }
        return Err(AcceptError::Protocol(
            "shadowsocks listener requires shadowsocks config".into(),
        ));
    }

    // Check if Trojan is the only protocol (TLS termination already happened upstream)
    if protocols.len() == 1 && protocols.contains(&ProtocolId::Trojan) {
        if let Some(trojan_cfg) = trojan_config {
            let (trojan_stream, result) =
                eggress_protocol_trojan::trojan_accept(stream, &trojan_cfg.password)
                    .await
                    .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

            return Ok(AcceptedSession::Tunnel(PendingTunnel {
                target: result.target,
                client: trojan_stream,
                protocol: TunnelProtocol::Trojan,
                reply_context: ReplyContext::Trojan,
                identity: ClientIdentity::Anonymous,
            }));
        }
        return Err(AcceptError::Protocol(
            "trojan listener requires trojan config".into(),
        ));
    }

    Err(AcceptError::Protocol(
        "no matching protocol for listener".into(),
    ))
}

enum DetectResult {
    Match,
    NeedMore,
    NoMatch,
}

fn detect_http_method(prefix: &[u8]) -> DetectResult {
    // Look for a space in the prefix to find the end of the method token
    if let Some(space_pos) = prefix.iter().position(|&b| b == b' ') {
        let method_token = &prefix[..space_pos];
        if method_token.is_empty() || method_token.len() > 16 {
            return DetectResult::NoMatch;
        }
        // Check if all bytes are valid HTTP method characters:
        // uppercase ASCII letters, lowercase ASCII letters, or hyphens
        let is_valid_method = method_token
            .iter()
            .all(|&b| b.is_ascii_uppercase() || b == b'-' || b.is_ascii_lowercase());
        if is_valid_method {
            DetectResult::Match
        } else {
            DetectResult::NoMatch
        }
    } else {
        // No space found yet - check if what we have so far looks like a valid method prefix
        if prefix.len() > 16 {
            return DetectResult::NoMatch;
        }
        // Check if all bytes so far are valid method characters
        let is_valid_prefix = prefix
            .iter()
            .all(|&b| b.is_ascii_uppercase() || b == b'-' || b.is_ascii_lowercase());
        if is_valid_prefix {
            DetectResult::NeedMore
        } else {
            DetectResult::NoMatch
        }
    }
}

async fn accept_socks5(
    stream: BoxStream,
    auth: &InboundAuthentication,
) -> Result<AcceptedSession, AcceptError> {
    use eggress_protocol_socks::socks5::server::{
        read_auth_request, read_method_negotiation, read_socks5_request, send_auth_response,
        send_connect_reply, Socks5Command, CMD_BIND, REP_COMMAND_NOT_SUPPORTED,
    };

    let (mut reader, mut writer) = tokio::io::split(stream);
    let methods = read_method_negotiation(&mut reader)
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

    // Determine method selection based on auth policy
    const AUTH_NONE: u8 = 0x00;
    const AUTH_USERNAME_PASSWORD: u8 = 0x02;
    const AUTH_NO_ACCEPTABLE: u8 = 0xFF;

    let selected_method = match auth {
        InboundAuthentication::UsernamePassword { .. } => {
            if methods.contains(&AUTH_USERNAME_PASSWORD) {
                AUTH_USERNAME_PASSWORD
            } else {
                AUTH_NO_ACCEPTABLE
            }
        }
        InboundAuthentication::None => {
            if methods.contains(&AUTH_NONE) {
                AUTH_NONE
            } else {
                AUTH_NO_ACCEPTABLE
            }
        }
    };

    // Send method selection
    use tokio::io::AsyncWriteExt;
    writer
        .write_all(&[0x05, selected_method])
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
    writer
        .flush()
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

    if selected_method == AUTH_NO_ACCEPTABLE {
        return Err(AcceptError::Protocol(Box::new(
            eggress_protocol_socks::error::Socks5Error::MethodNegotiationFailed,
        )));
    }

    // Handle auth if required
    let mut identity = ClientIdentity::Anonymous;
    if let InboundAuthentication::UsernamePassword { username, password } = auth {
        match read_auth_request(&mut reader, password).await {
            Ok(client_username) => {
                if client_username != *username {
                    let _ = send_auth_response(&mut writer, false).await;
                    return Err(AcceptError::AuthenticationFailed);
                }
                identity = ClientIdentity::Username(client_username);
                send_auth_response(&mut writer, true)
                    .await
                    .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
            }
            Err(_) => {
                let _ = send_auth_response(&mut writer, false).await;
                return Err(AcceptError::AuthenticationFailed);
            }
        }
    }

    let (command, socks_addr) = read_socks5_request(&mut reader)
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

    match command {
        Socks5Command::Connect => {
            let target = socks_addr_to_target(&socks_addr);
            let stream: BoxStream = Box::new(tokio::io::join(reader, writer));

            Ok(AcceptedSession::Tunnel(PendingTunnel {
                target,
                client: stream,
                protocol: TunnelProtocol::Socks5,
                reply_context: ReplyContext::Socks5,
                identity,
            }))
        }
        Socks5Command::UdpAssociate => {
            let client_hint = Some(socks_addr_to_target(&socks_addr));
            let stream: BoxStream = Box::new(tokio::io::join(reader, writer));

            Ok(AcceptedSession::UdpAssociate(PendingUdpAssociate {
                client: stream,
                protocol: TunnelProtocol::Socks5,
                identity,
                client_hint,
            }))
        }
        Socks5Command::Bind => {
            let _ = send_connect_reply(&mut writer, REP_COMMAND_NOT_SUPPORTED, &socks_addr).await;
            Err(AcceptError::Protocol(Box::new(
                eggress_protocol_socks::error::Socks5Error::UnsupportedCommand(CMD_BIND),
            )))
        }
    }
}

async fn accept_socks4(stream: BoxStream) -> Result<AcceptedSession, AcceptError> {
    use eggress_protocol_socks::socks4::server::read_socks4_request;

    let (mut reader, writer) = tokio::io::split(stream);
    let request = read_socks4_request(&mut reader)
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
    let target = TargetAddr {
        host: TargetHost::Ip(request.addr.ip()),
        port: request.addr.port(),
    };
    let identity = if request.user_id.is_empty() {
        ClientIdentity::Anonymous
    } else {
        ClientIdentity::Opaque(request.user_id)
    };
    let stream: BoxStream = Box::new(tokio::io::join(reader, writer));

    Ok(AcceptedSession::Tunnel(PendingTunnel {
        target,
        client: stream,
        protocol: TunnelProtocol::Socks4,
        reply_context: ReplyContext::Socks4,
        identity,
    }))
}

async fn accept_http(
    mut stream: BoxStream,
    auth: &InboundAuthentication,
) -> Result<AcceptedSession, AcceptError> {
    // Read the request line to determine method
    let mut head_buf = Vec::with_capacity(256);
    let mut temp = [0u8; 1];

    loop {
        if head_buf.len() >= MAX_HEAD_SIZE {
            return Err(AcceptError::Protocol(
                eggress_protocol_http::HttpError::HeaderTooLarge.into(),
            ));
        }
        let n = stream
            .read(&mut temp)
            .await
            .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
        if n == 0 {
            return Err(AcceptError::Protocol(
                eggress_protocol_http::HttpError::MalformedRequest("unexpected EOF".into()).into(),
            ));
        }
        head_buf.push(temp[0]);
        if head_buf.len() >= 2 && &head_buf[head_buf.len() - 2..] == b"\r\n" {
            break;
        }
    }

    let method = {
        let request_line = String::from_utf8_lossy(&head_buf);
        request_line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    };

    // Reconstruct stream with the request line bytes prepended
    let mut stream: BoxStream = Box::new(PrefixedStream::new(head_buf, stream));

    if method == "connect" {
        let request = read_connect_request_from_stream(&mut stream, auth).await?;
        Ok(AcceptedSession::Tunnel(PendingTunnel {
            target: request.target,
            client: stream,
            protocol: TunnelProtocol::HttpConnect,
            reply_context: ReplyContext::Http,
            identity: request.identity,
        }))
    } else {
        // Read the complete head to extract Proxy-Authorization before forward_request strips it
        let mut head_buf = Vec::with_capacity(1024);
        let mut temp = [0u8; 1];
        let mut header_count = 0;

        loop {
            if head_buf.len() >= MAX_HEAD_SIZE {
                return Err(AcceptError::Protocol(
                    eggress_protocol_http::HttpError::HeaderTooLarge.into(),
                ));
            }

            let n = stream
                .read(&mut temp)
                .await
                .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
            if n == 0 {
                return Err(AcceptError::Protocol(
                    eggress_protocol_http::HttpError::MalformedRequest(
                        "unexpected EOF reading request".into(),
                    )
                    .into(),
                ));
            }

            head_buf.push(temp[0]);

            if head_buf.len() >= 4 {
                let len = head_buf.len();
                if &head_buf[len - 4..] == b"\r\n\r\n" {
                    break;
                }
                if head_buf.len() >= 2 && &head_buf[len - 2..] == b"\r\n" {
                    header_count += 1;
                    if header_count > MAX_HEADER_LINES {
                        return Err(AcceptError::Protocol(
                            eggress_protocol_http::HttpError::TooManyHeaders.into(),
                        ));
                    }
                }
            }
        }

        // Parse Proxy-Authorization from the raw head
        let head_str = String::from_utf8_lossy(&head_buf);
        let proxy_auth = if let InboundAuthentication::UsernamePassword { username, password } =
            auth
        {
            let mut found_auth = None;
            for line in head_str.split("\r\n") {
                if let Some((name, value)) = parse_header_line_str(line) {
                    if name.eq_ignore_ascii_case("Proxy-Authorization") {
                        found_auth = parse_basic_auth(&value);
                        break;
                    }
                }
            }
            match found_auth {
                Some((user, pass)) => {
                    use subtle::ConstantTimeEq;
                    let user_ok: bool = user.as_bytes().ct_eq(username.as_bytes()).into();
                    let pass_ok: bool = pass.as_bytes().ct_eq(password.as_bytes()).into();
                    if !user_ok || !pass_ok {
                        // Reconstruct stream and send 407
                        let mut stream: BoxStream = Box::new(PrefixedStream::new(head_buf, stream));
                        let _ = write_proxy_auth_required(&mut stream).await;
                        return Err(AcceptError::AuthenticationFailed);
                    }
                    Some((user, pass))
                }
                None => {
                    let mut stream: BoxStream = Box::new(PrefixedStream::new(head_buf, stream));
                    let _ = write_proxy_auth_required(&mut stream).await;
                    return Err(AcceptError::AuthenticationFailed);
                }
            }
        } else {
            None
        };
        let identity = match &proxy_auth {
            Some((user, _)) => ClientIdentity::Username(user.clone()),
            None => ClientIdentity::Anonymous,
        };
        let _ = proxy_auth; // Auth already validated above

        // Reconstruct stream for forward_request
        let stream: BoxStream = Box::new(PrefixedStream::new(head_buf, stream));

        let (request, client_stream) = eggress_protocol_http::forward_request(stream)
            .await
            .map_err(|e| AcceptError::Protocol(Box::new(e)))?;

        let target = request.target.clone();
        Ok(AcceptedSession::HttpForward(PendingHttpForward {
            target,
            client: client_stream,
            request,
            identity,
        }))
    }
}

struct ConnectRequest {
    target: TargetAddr,
    identity: ClientIdentity,
}

async fn read_connect_request_from_stream(
    stream: &mut BoxStream,
    auth: &InboundAuthentication,
) -> Result<ConnectRequest, AcceptError> {
    let mut head_buf = Vec::with_capacity(1024);
    let mut temp = [0u8; 1];
    let mut header_count = 0;

    loop {
        if head_buf.len() >= MAX_HEAD_SIZE {
            return Err(AcceptError::Protocol(
                eggress_protocol_http::HttpError::HeaderTooLarge.into(),
            ));
        }

        let n = stream
            .read(&mut temp)
            .await
            .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
        if n == 0 {
            return Err(AcceptError::Protocol(
                eggress_protocol_http::HttpError::MalformedRequest(
                    "unexpected EOF reading request".into(),
                )
                .into(),
            ));
        }

        head_buf.push(temp[0]);

        if head_buf.len() >= 4 {
            let len = head_buf.len();
            if &head_buf[len - 4..] == b"\r\n\r\n" {
                break;
            }
            if head_buf.len() >= 2 && &head_buf[len - 2..] == b"\r\n" {
                header_count += 1;
                if header_count > MAX_HEADER_LINES {
                    return Err(AcceptError::Protocol(
                        eggress_protocol_http::HttpError::TooManyHeaders.into(),
                    ));
                }
            }
        }
    }

    let head_str = String::from_utf8_lossy(&head_buf);
    let mut lines = head_str.split("\r\n");

    let request_line = lines.next().ok_or_else(|| {
        AcceptError::Protocol(
            eggress_protocol_http::HttpError::MalformedRequest("empty request".into()).into(),
        )
    })?;

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(AcceptError::Protocol(
            eggress_protocol_http::HttpError::MalformedRequest(format!(
                "expected 3 parts in request line, got {}",
                parts.len()
            ))
            .into(),
        ));
    }

    let authority = parts[1];
    let target = parse_authority(authority)?;

    // Parse Proxy-Authorization header
    let mut proxy_auth = None;
    let mut parsed_username: Option<String> = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = parse_header_line_str(line) {
            if name.eq_ignore_ascii_case("Proxy-Authorization") {
                proxy_auth = parse_basic_auth(&value);
                if let Some((user, _)) = &proxy_auth {
                    parsed_username = Some(user.clone());
                }
            }
        }
    }

    // Validate auth if required
    if let InboundAuthentication::UsernamePassword { username, password } = auth {
        match proxy_auth {
            Some((user, pass)) => {
                use subtle::ConstantTimeEq;
                let user_ok: bool = user.as_bytes().ct_eq(username.as_bytes()).into();
                let pass_ok: bool = pass.as_bytes().ct_eq(password.as_bytes()).into();
                if !user_ok || !pass_ok {
                    let _ = write_proxy_auth_required(stream).await;
                    return Err(AcceptError::AuthenticationFailed);
                }
            }
            None => {
                let _ = write_proxy_auth_required(stream).await;
                return Err(AcceptError::AuthenticationFailed);
            }
        }
    }

    let identity = match parsed_username {
        Some(user) => ClientIdentity::Username(user),
        None => ClientIdentity::Anonymous,
    };

    Ok(ConnectRequest { target, identity })
}

fn parse_authority(
    authority: &str,
) -> Result<TargetAddr, Box<dyn std::error::Error + Send + Sync>> {
    if authority.starts_with('[') {
        let bracket_end = authority.find(']').ok_or_else(|| {
            eggress_protocol_http::HttpError::TargetParseError(
                "unclosed bracket in IPv6 address".into(),
            )
        })?;

        let ip_str = &authority[1..bracket_end];
        let ip: std::net::IpAddr = ip_str.parse().map_err(|e| {
            eggress_protocol_http::HttpError::TargetParseError(format!("invalid IPv6 address: {e}"))
        })?;

        let port_str = authority.get(bracket_end + 2..).ok_or_else(|| {
            eggress_protocol_http::HttpError::TargetParseError("missing port".into())
        })?;

        if !authority
            .as_bytes()
            .get(bracket_end + 1)
            .is_some_and(|&b| b == b':')
        {
            return Err(eggress_protocol_http::HttpError::TargetParseError(
                "expected ':' between IPv6 address and port".into(),
            )
            .into());
        }

        let port: u16 = port_str.parse().map_err(|e| {
            eggress_protocol_http::HttpError::TargetParseError(format!("invalid port: {e}"))
        })?;

        return Ok(TargetAddr {
            host: TargetHost::Ip(ip),
            port,
        });
    }

    let colon_pos = authority.rfind(':').ok_or_else(|| {
        eggress_protocol_http::HttpError::TargetParseError("missing port in authority".into())
    })?;

    let host_str = &authority[..colon_pos];
    let port_str = &authority[colon_pos + 1..];

    let port: u16 = port_str.parse().map_err(|e| {
        eggress_protocol_http::HttpError::TargetParseError(format!("invalid port: {e}"))
    })?;

    if let Ok(ip) = host_str.parse::<std::net::IpAddr>() {
        return Ok(TargetAddr {
            host: TargetHost::Ip(ip),
            port,
        });
    }

    if host_str.is_empty() {
        return Err(eggress_protocol_http::HttpError::TargetParseError("empty host".into()).into());
    }

    Ok(TargetAddr {
        host: TargetHost::Domain(host_str.to_string()),
        port,
    })
}

fn socks_addr_to_target(addr: &eggress_protocol_socks::socks5::server::SocksAddr) -> TargetAddr {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    match addr {
        SocksAddr::IPv4(octets, port) => TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V4((*octets).into())),
            port: *port,
        },
        SocksAddr::IPv6(octets, port) => TargetAddr {
            host: TargetHost::Ip(std::net::IpAddr::V6((*octets).into())),
            port: *port,
        },
        SocksAddr::Domain(domain, port) => TargetAddr {
            host: TargetHost::Domain(domain.clone()),
            port: *port,
        },
    }
}

/// Maximum size for the HTTP request head (request line + headers).
const MAX_HEAD_SIZE: usize = 32 * 1024;

/// Maximum number of header lines.
const MAX_HEADER_LINES: usize = 128;

/// Parse a header line into (name, value).
fn parse_header_line_str(line: &str) -> Option<(String, String)> {
    let colon_pos = line.find(':')?;
    let name = line[..colon_pos].trim().to_string();
    let value = line[colon_pos + 1..].trim().to_string();
    Some((name, value))
}

/// Simple base64 decoder.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let input = input.trim_end_matches('=');
    let input_bytes = input.as_bytes();

    let mut result = Vec::with_capacity(input_bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for &byte in input_bytes {
        let val = TABLE.iter().position(|&b| b == byte)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
        }
    }

    Some(result)
}

/// Parse Basic authentication from a Proxy-Authorization header value.
fn parse_basic_auth(value: &str) -> Option<(String, String)> {
    let value = value.trim();
    if !value.starts_with("Basic ") {
        return None;
    }

    let encoded = &value[6..];
    let decoded = base64_decode(encoded)?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    let colon_pos = decoded_str.find(':')?;
    let username = decoded_str[..colon_pos].to_string();
    let password = decoded_str[colon_pos + 1..].to_string();
    Some((username, password))
}

/// Write a 407 Proxy Authentication Required response.
async fn write_proxy_auth_required(stream: &mut BoxStream) -> Result<(), std::io::Error> {
    let response = b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"eggress\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(response).await?;
    stream.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_accept_socks5() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_socks4() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks4);
                    assert_eq!(pending.target.port, 80);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(&[0x04, 0x01, 0x00, 0x50, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&[0x00]).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_http_connect() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_http_forward() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.target.port, 80);
                    assert_eq!(pending.request.method, "GET");
                }
                _ => panic!("expected http forward"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET http://example.com/index.html HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_on_http_only_listener() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_on_http_only_listener_rejected() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_on_socks5_only_listener_rejected() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_on_mixed_listener_accepted() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_on_mixed_listener_accepted() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_random_binary_prefix_rejected() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send random binary prefix that isn't 0x04 or 0x05 and not valid HTTP
        stream.write_all(&[0x00, 0x01, 0x02, 0x03]).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_tls_client_hello_not_interpreted_as_http() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // TLS ClientHello starts with 0x16, 0x03, which isn't valid HTTP method
        stream
            .write_all(&[0x16, 0x03, 0x01, 0x00, 0x05])
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    // === Authentication tests ===

    #[tokio::test]
    async fn test_socks5_auth_correct_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "secret".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed, &all_protocols, &auth, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Client offers both no-auth and username/password
        stream.write_all(&[0x05, 0x02, 0x00, 0x02]).await.unwrap();
        // Server selects username/password (0x02)
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x02]);

        // Send auth: version=1, ulen=4, "user", plen=6, "secret"
        stream
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r', 0x06])
            .await
            .unwrap();
        stream.write_all(b"secret").await.unwrap();
        // Read auth response (success)
        let mut auth_resp = [0u8; 2];
        stream.read_exact(&mut auth_resp).await.unwrap();
        assert_eq!(auth_resp, [0x01, 0x00]);

        // Send CONNECT request
        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_auth_wrong_password() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "secret".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x02, 0x00, 0x02]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x02]);

        // Send auth with wrong password
        stream
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r', 0x05])
            .await
            .unwrap();
        stream.write_all(b"wrong").await.unwrap();
        // Read auth response (failure)
        let mut auth_resp = [0u8; 2];
        stream.read_exact(&mut auth_resp).await.unwrap();
        assert_eq!(auth_resp, [0x01, 0x01]);

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_auth_no_auth_client_rejected() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "secret".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Client only offers no-auth
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        // Server should send 0xFF (no acceptable methods)
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0xFF]);

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_connect_auth_correct_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed, &all_protocols, &auth, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // "user:pass" base64 encoded is "dXNlcjpwYXNz"
        stream
            .write_all(
                b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nProxy-Authorization: Basic dXNlcjpwYXNz\r\n\r\n",
            )
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_connect_auth_missing_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();

        // Read 407 response
        let mut response = vec![0u8; 512];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("407"),
            "expected 407, got: {response_str}"
        );
        assert!(
            response_str.contains("Proxy-Authenticate"),
            "expected Proxy-Authenticate header"
        );

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_connect_auth_wrong_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // "user:wrong" base64 encoded is "dXNlcjp3cm9uZw=="
        stream
            .write_all(
                b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nProxy-Authorization: Basic dXNlcjp3cm9uZw==\r\n\r\n",
            )
            .await
            .unwrap();

        let mut response = vec![0u8; 512];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("407"),
            "expected 407, got: {response_str}"
        );

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_connect_auth_malformed_base64() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nProxy-Authorization: Basic !!!invalid!!!\r\n\r\n",
            )
            .await
            .unwrap();

        let mut response = vec![0u8; 512];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("407"),
            "expected 407, got: {response_str}"
        );

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_forward_auth_correct_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed, &all_protocols, &auth, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.target.port, 80);
                    assert_eq!(pending.request.method, "GET");
                    // Proxy-Authorization should be stripped
                    assert!(!pending
                        .request
                        .headers
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case("Proxy-Authorization")));
                }
                _ => panic!("expected http forward"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\nProxy-Authorization: Basic dXNlcjpwYXNz\r\n\r\n",
            )
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_forward_auth_missing_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        let mut response = vec![0u8; 512];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("407"),
            "expected 407, got: {response_str}"
        );

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_forward_auth_wrong_credentials() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(boxed, &all_protocols, &auth, None, None, None).await;
            assert!(matches!(result, Err(AcceptError::AuthenticationFailed)));
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // "user:wrong" base64 encoded is "dXNlcjp3cm9uZw=="
        stream
            .write_all(
                b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\nProxy-Authorization: Basic dXNlcjp3cm9uZw==\r\n\r\n",
            )
            .await
            .unwrap();

        let mut response = vec![0u8; 512];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("407"),
            "expected 407, got: {response_str}"
        );

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_udp_associate_returns_pending() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::UdpAssociate(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(
                        pending.client_hint,
                        Some(TargetAddr {
                            host: TargetHost::Ip(std::net::IpAddr::V4(std::net::Ipv4Addr::new(
                                0, 0, 0, 0
                            ))),
                            port: 0,
                        })
                    );
                }
                _ => panic!("expected UdpAssociate"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // UDP ASSOCIATE (cmd=0x03), target 0.0.0.0:0
        stream
            .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
            .await
            .unwrap();
        stream.write_all(&0u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_bind_rejected() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // BIND (cmd=0x02)
        stream
            .write_all(&[0x05, 0x02, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&80u16.to_be_bytes()).await.unwrap();

        // Server sends rejection reply (RFC 1928 0x07 command not supported)
        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x07); // command not supported

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_connect_still_works() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // CONNECT request
        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_socks5_udp_associate_with_auth() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let auth = InboundAuthentication::UsernamePassword {
            username: "user".to_string(),
            password: "secret".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(boxed, &all_protocols, &auth, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::UdpAssociate(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(
                        pending.identity,
                        ClientIdentity::Username("user".to_string())
                    );
                }
                _ => panic!("expected UdpAssociate"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0x05, 0x02, 0x00, 0x02]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x02]);

        // Auth
        stream
            .write_all(&[0x01, 0x04, b'u', b's', b'e', b'r', 0x06])
            .await
            .unwrap();
        stream.write_all(b"secret").await.unwrap();
        let mut auth_resp = [0u8; 2];
        stream.read_exact(&mut auth_resp).await.unwrap();
        assert_eq!(auth_resp, [0x01, 0x00]);

        // UDP ASSOCIATE
        stream
            .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0])
            .await
            .unwrap();
        stream.write_all(&0u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    // === Mixed-protocol listener robustness tests ===

    #[tokio::test]
    async fn test_fragmented_first_byte_http() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.request.method, "GET");
                }
                _ => panic!("expected http forward"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send HTTP GET fragmented into individual bytes
        stream.write_all(b"G").await.unwrap();
        stream.write_all(b"E").await.unwrap();
        stream.write_all(b"T").await.unwrap();
        stream
            .write_all(b" http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_garbage_bytes_rejected() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&[0xAA, 0xBB, 0xCC, 0xDD]).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_slow_socks5_detection() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send first byte (version) then delay
        stream.write_all(&[0x05]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // Send rest of method negotiation
        stream.write_all(&[0x01, 0x00]).await.unwrap();
        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // Send CONNECT request
        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_connect_and_socks5_same_listener() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // First connection: HTTP CONNECT
        let client_jh1 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
                .await
                .unwrap();
        });

        let (stream1, _) = listener.accept().await.unwrap();
        let p = protocols.clone();
        let server_jh1 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream1);
            let session = accept(boxed, &p, &InboundAuthentication::None, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::HttpConnect);
                }
                _ => panic!("expected tunnel"),
            }
        });

        client_jh1.await.unwrap();
        server_jh1.await.unwrap();

        // Second connection: SOCKS5
        let client_jh2 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut response = [0u8; 2];
            stream.read_exact(&mut response).await.unwrap();
            assert_eq!(response, [0x05, 0x00]);

            stream
                .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
                .await
                .unwrap();
            stream.write_all(&443u16.to_be_bytes()).await.unwrap();
        });

        let (stream2, _) = listener.accept().await.unwrap();
        let server_jh2 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream2);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        client_jh2.await.unwrap();
        server_jh2.await.unwrap();
    }

    #[tokio::test]
    async fn test_http_forward_and_socks4_same_listener() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http, ProtocolId::Socks4];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // First connection: HTTP forward
        let client_jh1 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
                .await
                .unwrap();
        });

        let (stream1, _) = listener.accept().await.unwrap();
        let p = protocols.clone();
        let server_jh1 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream1);
            let session = accept(boxed, &p, &InboundAuthentication::None, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.request.method, "GET");
                    assert_eq!(pending.target.port, 80);
                }
                _ => panic!("expected http forward"),
            }
        });

        client_jh1.await.unwrap();
        server_jh1.await.unwrap();

        // Second connection: SOCKS4
        let client_jh2 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            // SOCKS4 CONNECT: version=0x04, cmd=0x01, port=443, addr=0.0.0.1, userid=0
            stream.write_all(&[0x04, 0x01]).await.unwrap();
            stream.write_all(&443u16.to_be_bytes()).await.unwrap();
            stream.write_all(&[10, 0, 0, 1]).await.unwrap();
            stream.write_all(&[0x00]).await.unwrap();
        });

        let (stream2, _) = listener.accept().await.unwrap();
        let server_jh2 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream2);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks4);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        client_jh2.await.unwrap();
        server_jh2.await.unwrap();
    }

    #[tokio::test]
    async fn test_fragmented_socks5_handshake() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let session = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send version byte separately from method negotiation
        stream.write_all(&[0x05]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        stream.write_all(&[0x01, 0x00]).await.unwrap();

        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, [0x05, 0x00]);

        // Now send CONNECT request, also fragmented
        stream
            .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
            .await
            .unwrap();
        stream.write_all(&443u16.to_be_bytes()).await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_malformed_http_request_rejected() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Send a partial HTTP request that never completes headers
        stream
            .write_all(b"GET http://example.com HTTP/1.1\r\n")
            .await
            .unwrap();
        // Never send the final \r\n to end headers, then close the connection
        stream.shutdown().await.unwrap();

        server_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_empty_connection_closed() {
        let all_protocols: Vec<ProtocolId> =
            vec![ProtocolId::Http, ProtocolId::Socks4, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let boxed: BoxStream = Box::new(stream);
            let result = accept(
                boxed,
                &all_protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await;
            assert!(result.is_err());
        });

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Close immediately without sending anything
        drop(stream);

        server_jh.await.unwrap();
    }

    /// Mixed-protocol listener with auth: HTTP with auth and SOCKS5 with auth
    /// on the same listener. Both connections should be detected correctly
    /// when correct credentials are provided.
    #[tokio::test]
    async fn test_mixed_protocols_with_auth_detection() {
        let protocols: Vec<ProtocolId> = vec![ProtocolId::Http, ProtocolId::Socks5];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // First connection: HTTP forward (non-CONNECT) without auth —
        // protocol detection still works, auth is checked in serve_connection.
        let client_jh1 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
                .await
                .unwrap();
        });

        let (stream1, _) = listener.accept().await.unwrap();
        let p = protocols.clone();
        let server_jh1 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream1);
            let session = accept(boxed, &p, &InboundAuthentication::None, None, None, None)
                .await
                .unwrap();
            match session {
                AcceptedSession::HttpForward(pending) => {
                    assert_eq!(pending.request.method, "GET");
                }
                _ => panic!("expected http forward"),
            }
        });

        client_jh1.await.unwrap();
        server_jh1.await.unwrap();

        // Second connection: SOCKS5 without auth — detected correctly.
        let client_jh2 = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut response = [0u8; 2];
            stream.read_exact(&mut response).await.unwrap();
            assert_eq!(response, [0x05, 0x00]);
            stream
                .write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1])
                .await
                .unwrap();
            stream.write_all(&443u16.to_be_bytes()).await.unwrap();
        });

        let (stream2, _) = listener.accept().await.unwrap();
        let server_jh2 = tokio::spawn(async move {
            let boxed: BoxStream = Box::new(stream2);
            let session = accept(
                boxed,
                &protocols,
                &InboundAuthentication::None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            match session {
                AcceptedSession::Tunnel(pending) => {
                    assert_eq!(pending.protocol, TunnelProtocol::Socks5);
                    assert_eq!(pending.target.port, 443);
                }
                _ => panic!("expected tunnel"),
            }
        });

        client_jh2.await.unwrap();
        server_jh2.await.unwrap();
    }
}
