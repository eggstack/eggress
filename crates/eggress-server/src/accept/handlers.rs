//! Protocol-specific accept handlers: SOCKS5, SOCKS4/4a, HTTP.
//!
//! Called by the `accept` entry points after detection; each handler
//! performs its handshake and returns an `AcceptedSession` without
//! opening outbound connections.

use std::net::IpAddr;

use eggress_core::BoxStream;
use eggress_core::{ClientIdentity, TargetAddr, TargetHost};

use crate::auth::parse_basic_auth;

use super::forward::{
    parse_connect_request, parse_header_line_str, read_http_head, socks_addr_to_target,
    write_proxy_auth_required,
};
use super::prefixed::PrefixedStream;
use super::{
    auth_credentials, cached_identity, record_authenticated, AcceptError, AcceptedSession,
    InboundAuthentication, PendingHttpForward, PendingTunnel, PendingUdpAssociate, ReplyContext,
    TunnelProtocol,
};

pub(crate) async fn accept_socks5(
    stream: BoxStream,
    auth: &InboundAuthentication,
    peer_ip: Option<IpAddr>,
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

    let cached = cached_identity(auth, peer_ip);
    let selected_method = match (auth_credentials(auth), cached.is_some()) {
        (None, _) | (Some(_), true) if methods.contains(&AUTH_NONE) => AUTH_NONE,
        (Some(_), _) if methods.contains(&AUTH_USERNAME_PASSWORD) => AUTH_USERNAME_PASSWORD,
        (None, _) => AUTH_NO_ACCEPTABLE,
        (Some(_), _) => AUTH_NO_ACCEPTABLE,
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
    let mut identity = cached.unwrap_or(ClientIdentity::Anonymous);
    if selected_method == AUTH_USERNAME_PASSWORD {
        let (username, password, _) = match auth_credentials(auth) {
            Some(creds) => creds,
            None => return Err(AcceptError::AuthenticationFailed),
        };
        match read_auth_request(&mut reader, username, password).await {
            Ok(client_username) => {
                identity = ClientIdentity::Username(client_username);
                record_authenticated(auth, peer_ip, &identity);
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

pub(crate) async fn accept_socks4(
    stream: BoxStream,
    auth: &InboundAuthentication,
    peer_ip: Option<IpAddr>,
) -> Result<AcceptedSession, AcceptError> {
    use eggress_protocol_socks::socks4::server::read_socks4_request;

    let (mut reader, writer) = tokio::io::split(stream);
    let request = read_socks4_request(&mut reader)
        .await
        .map_err(|e| AcceptError::Protocol(Box::new(e)))?;
    let target = if let Some(ref domain) = request.domain {
        TargetAddr {
            host: TargetHost::Domain(domain.clone()),
            port: request.port,
        }
    } else {
        TargetAddr {
            host: TargetHost::Ip(request.addr.ip()),
            port: request.addr.port(),
        }
    };
    let cached = cached_identity(auth, peer_ip);
    if cached.is_none() {
        if let Some((username, _, _)) = auth_credentials(auth) {
            use subtle::ConstantTimeEq;
            let user_ok: bool = request.user_id.as_bytes().ct_eq(username.as_bytes()).into();
            if !user_ok {
                return Err(AcceptError::AuthenticationFailed);
            }
        }
    }
    let identity = cached.unwrap_or({
        if request.user_id.is_empty() {
            ClientIdentity::Anonymous
        } else {
            ClientIdentity::Opaque(request.user_id)
        }
    });
    if matches!(
        identity,
        ClientIdentity::Opaque(_) | ClientIdentity::Username(_)
    ) {
        record_authenticated(auth, peer_ip, &identity);
    }
    let stream: BoxStream = Box::new(tokio::io::join(reader, writer));

    Ok(AcceptedSession::Tunnel(PendingTunnel {
        target,
        client: stream,
        protocol: TunnelProtocol::Socks4,
        reply_context: ReplyContext::Socks4,
        identity,
    }))
}

pub(crate) async fn accept_http(
    stream: BoxStream,
    auth: &InboundAuthentication,
    peer_ip: Option<IpAddr>,
) -> Result<AcceptedSession, AcceptError> {
    // Keep the reader buffered so parsing a request head does not issue one
    // underlying read per byte. Any prefetched body remains in the reader.
    let mut stream = tokio::io::BufReader::new(stream);
    let head_buf = read_http_head(&mut stream).await?;

    let method = {
        let request_line = String::from_utf8_lossy(&head_buf);
        request_line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    };

    if method == "connect" {
        let request = parse_connect_request(&head_buf, &mut stream, auth, peer_ip).await?;
        Ok(AcceptedSession::Tunnel(PendingTunnel {
            target: request.target,
            client: Box::new(stream),
            protocol: TunnelProtocol::HttpConnect,
            reply_context: ReplyContext::Http,
            identity: request.identity,
        }))
    } else {
        // Parse Proxy-Authorization from the raw head
        let head_str = String::from_utf8_lossy(&head_buf);
        let cached = cached_identity(auth, peer_ip);
        let proxy_auth = if cached.is_some() {
            None
        } else if let Some((username, password, _)) = auth_credentials(auth) {
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
                        let _ = write_proxy_auth_required(&mut stream).await;
                        return Err(AcceptError::AuthenticationFailed);
                    }
                    Some((user, pass))
                }
                None => {
                    let _ = write_proxy_auth_required(&mut stream).await;
                    return Err(AcceptError::AuthenticationFailed);
                }
            }
        } else {
            None
        };
        let identity = cached.unwrap_or_else(|| match &proxy_auth {
            Some((user, _)) => ClientIdentity::Username(user.clone()),
            None => ClientIdentity::Anonymous,
        });
        if matches!(identity, ClientIdentity::Username(_)) {
            record_authenticated(auth, peer_ip, &identity);
        }
        let _ = proxy_auth; // Auth already validated above

        // Reconstruct stream for forward_request
        let stream: BoxStream = Box::new(PrefixedStream::new(head_buf, Box::new(stream)));

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
