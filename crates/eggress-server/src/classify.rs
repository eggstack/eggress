//! Shared typed classification for connection and handshake failures.
//!
//! This module owns the single internal classifier for boxed handshake errors
//! used by both the server-side [`crate::SessionOpenError`] conversion and
//! the embed `OutboundConnector` detailed surface. Classification is purely
//! type-based (downcasting concrete built-in error types); it never inspects
//! message strings.
//!
//! The returned [`ClassifiedKind`] is intentionally narrow and
//! protocol-neutral. It is exposed as `#[doc(hidden)]` for Rust visibility
//! between `eggress-server` and `eggress-embed` and is not a second end-user
//! API: embedding consumers should use `eggress-embed::OutboundConnectError`.

/// Protocol-neutral failure category produced by the shared classifier.
///
/// Mirrors the categories needed by both `SessionOpenError` mapping and the
/// embed detailed error. `Tls`, `Protocol`, `Policy`, and `Other` preserve
/// the distinction between transport security, proxy protocol, local policy,
/// and unclassified failures without leaking protocol-specific types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum ClassifiedKind {
    Timeout,
    Dns,
    Refused,
    NetworkUnreachable,
    HostUnreachable,
    Auth,
    Tls,
    Protocol,
    Policy,
    Other,
}

/// Classify a [`std::io::ErrorKind`] without inspecting message strings.
///
/// Only stable kinds with clear network semantics are mapped; everything
/// else becomes [`ClassifiedKind::Other`] rather than guessing.
#[doc(hidden)]
pub fn classify_io_kind(kind: std::io::ErrorKind) -> ClassifiedKind {
    match kind {
        std::io::ErrorKind::ConnectionRefused => ClassifiedKind::Refused,
        std::io::ErrorKind::TimedOut => ClassifiedKind::Timeout,
        std::io::ErrorKind::NetworkUnreachable => ClassifiedKind::NetworkUnreachable,
        std::io::ErrorKind::HostUnreachable => ClassifiedKind::HostUnreachable,
        _ => ClassifiedKind::Other,
    }
}

/// Classify a direct [`eggress_core::ConnectError`] without formatting.
#[doc(hidden)]
pub fn classify_connect_error(error: &eggress_core::ConnectError) -> ClassifiedKind {
    match error {
        eggress_core::ConnectError::ConnectionRefused => ClassifiedKind::Refused,
        eggress_core::ConnectError::Timeout => ClassifiedKind::Timeout,
        eggress_core::ConnectError::DnsResolution(_) => ClassifiedKind::Dns,
        eggress_core::ConnectError::TlsHandshake(_) => ClassifiedKind::Tls,
        eggress_core::ConnectError::ReservedTarget(_) => ClassifiedKind::Policy,
        eggress_core::ConnectError::Io(io) => classify_io_kind(io.kind()),
    }
}

/// Classify a boxed handshake source by concrete type.
///
/// Covers the built-in errors used by normal outbound chains. Feature-gated
/// arms are compiled only when the corresponding protocol transport is
/// available so minimal builds do not pull in optional protocols solely for
/// error downcasting. Unknown or unproven failures become
/// [`ClassifiedKind::Protocol`] or [`ClassifiedKind::Other`]; no
/// message-string heuristics are used.
#[doc(hidden)]
pub fn classify_handshake_source(
    source: &(dyn std::error::Error + Send + Sync + 'static),
) -> ClassifiedKind {
    // Direct I/O boxed without a protocol wrapper.
    if let Some(io) = source.downcast_ref::<std::io::Error>() {
        return classify_io_kind(io.kind());
    }
    // A boxed ConnectError (defensive; normally ConnectFailed, not Handshake).
    if let Some(connect) = source.downcast_ref::<eggress_core::ConnectError>() {
        return classify_connect_error(connect);
    }
    // Core handshake wrapper, if ever boxed.
    if let Some(handshake) = source.downcast_ref::<eggress_core::chain::HandshakeError>() {
        return match handshake {
            eggress_core::chain::HandshakeError::Io(io) => classify_io_kind(io.kind()),
            eggress_core::chain::HandshakeError::ConnectionRefused => ClassifiedKind::Refused,
            eggress_core::chain::HandshakeError::AuthFailed => ClassifiedKind::Auth,
            eggress_core::chain::HandshakeError::Protocol(_) => ClassifiedKind::Protocol,
            eggress_core::chain::HandshakeError::Other(_) => ClassifiedKind::Other,
        };
    }
    // HTTP CONNECT (client): auth, refusal, gateway timeout are explicit.
    // BadGateway and other status/parse failures are Protocol, never a local
    // TCP refusal.
    if let Some(http) = source.downcast_ref::<eggress_protocol_http::HttpError>() {
        return match http {
            eggress_protocol_http::HttpError::AuthRequired
            | eggress_protocol_http::HttpError::AuthFailed => ClassifiedKind::Auth,
            eggress_protocol_http::HttpError::ConnectionRefused => ClassifiedKind::Refused,
            eggress_protocol_http::HttpError::GatewayTimeout => ClassifiedKind::Timeout,
            eggress_protocol_http::HttpError::BadGateway => ClassifiedKind::Protocol,
            eggress_protocol_http::HttpError::Io(io) => classify_io_kind(io.kind()),
            eggress_protocol_http::HttpError::UnexpectedStatus(_) => ClassifiedKind::Protocol,
            _ => ClassifiedKind::Protocol,
        };
    }
    // H2 CONNECT client wrapper: unwrap nested HTTP or I/O where typed.
    if let Some(h2) = source.downcast_ref::<eggress_protocol_http::H2ConnectError>() {
        return match h2 {
            eggress_protocol_http::H2ConnectError::Io(io) => classify_io_kind(io.kind()),
            eggress_protocol_http::H2ConnectError::Http(inner) => {
                // Reuse the HTTP mapping above via a synthetic reference.
                // Match explicitly to avoid re-boxing.
                match inner {
                    eggress_protocol_http::HttpError::AuthRequired
                    | eggress_protocol_http::HttpError::AuthFailed => ClassifiedKind::Auth,
                    eggress_protocol_http::HttpError::ConnectionRefused => ClassifiedKind::Refused,
                    eggress_protocol_http::HttpError::GatewayTimeout => ClassifiedKind::Timeout,
                    eggress_protocol_http::HttpError::BadGateway => ClassifiedKind::Protocol,
                    eggress_protocol_http::HttpError::Io(io) => classify_io_kind(io.kind()),
                    eggress_protocol_http::HttpError::UnexpectedStatus(_) => {
                        ClassifiedKind::Protocol
                    }
                    _ => ClassifiedKind::Protocol,
                }
            }
            eggress_protocol_http::H2ConnectError::DnsRebinding(_) => ClassifiedKind::Policy,
            eggress_protocol_http::H2ConnectError::H2(_) => ClassifiedKind::Protocol,
            eggress_protocol_http::H2ConnectError::PoolExhausted => ClassifiedKind::Other,
        };
    }
    // SOCKS5 client: auth and typed refusal are explicit. Generic
    // ConnectionFailed (other REP codes) stays Protocol rather than
    // pretending every proxy-reported failure is a TCP refusal.
    if let Some(socks5) = source.downcast_ref::<eggress_protocol_socks::Socks5Error>() {
        return match socks5 {
            eggress_protocol_socks::Socks5Error::AuthFailed => ClassifiedKind::Auth,
            eggress_protocol_socks::Socks5Error::ConnectionRefused => ClassifiedKind::Refused,
            eggress_protocol_socks::Socks5Error::Io(io) => classify_io_kind(io.kind()),
            eggress_protocol_socks::Socks5Error::ConnectionFailed(_) => ClassifiedKind::Protocol,
            _ => ClassifiedKind::Protocol,
        };
    }
    // SOCKS4/4a client: 91 (Failed) is the proxy-reported target failure;
    // 92/93 are identd identity failures.
    if let Some(socks4) = source.downcast_ref::<eggress_protocol_socks::Socks4Error>() {
        return match socks4 {
            eggress_protocol_socks::Socks4Error::ConnectionRefused
            | eggress_protocol_socks::Socks4Error::ConnectionFailed => ClassifiedKind::Refused,
            eggress_protocol_socks::Socks4Error::FailedNoIdent
            | eggress_protocol_socks::Socks4Error::FailedDifferentUser => ClassifiedKind::Auth,
            eggress_protocol_socks::Socks4Error::Io(io) => classify_io_kind(io.kind()),
            _ => ClassifiedKind::Protocol,
        };
    }
    // TLS transport wrapper (explicit `+tls` hop stage).
    if source
        .downcast_ref::<eggress_transport_tls::TlsError>()
        .is_some()
    {
        return ClassifiedKind::Tls;
    }
    #[cfg(feature = "extended")]
    {
        if let Some(trojan) = source.downcast_ref::<eggress_protocol_trojan::TrojanError>() {
            return match trojan {
                eggress_protocol_trojan::TrojanError::AuthFailed => ClassifiedKind::Auth,
                eggress_protocol_trojan::TrojanError::ConnectionRefused => ClassifiedKind::Refused,
                eggress_protocol_trojan::TrojanError::Tls(_) => ClassifiedKind::Tls,
                eggress_protocol_trojan::TrojanError::Io(io) => classify_io_kind(io.kind()),
                eggress_protocol_trojan::TrojanError::Protocol(_) => ClassifiedKind::Protocol,
            };
        }
        if let Some(ss) = source.downcast_ref::<eggress_protocol_shadowsocks::ShadowsocksError>() {
            return match ss {
                eggress_protocol_shadowsocks::ShadowsocksError::Io(io) => {
                    classify_io_kind(io.kind())
                }
                _ => ClassifiedKind::Protocol,
            };
        }
        if let Some(ws) = source.downcast_ref::<eggress_protocol_websocket::error::WebSocketError>()
        {
            return match ws {
                eggress_protocol_websocket::error::WebSocketError::Io(io) => {
                    classify_io_kind(io.kind())
                }
                _ => ClassifiedKind::Protocol,
            };
        }
    }
    #[cfg(feature = "ssh")]
    {
        if let Some(ssh) = source.downcast_ref::<eggress_transport_ssh::SshTransportError>() {
            return match ssh {
                eggress_transport_ssh::SshTransportError::AuthenticationFailed => {
                    ClassifiedKind::Auth
                }
                eggress_transport_ssh::SshTransportError::MissingUsername
                | eggress_transport_ssh::SshTransportError::EmptyPrivateKeyPath
                | eggress_transport_ssh::SshTransportError::PrivateKey(_)
                | eggress_transport_ssh::SshTransportError::TargetPort(_) => ClassifiedKind::Policy,
                // Connection/Channel carry string details only; do not infer
                // timeout/refusal from message text.
                eggress_transport_ssh::SshTransportError::Connection(_)
                | eggress_transport_ssh::SshTransportError::Channel(_) => ClassifiedKind::Other,
            };
        }
    }
    #[cfg(feature = "quic")]
    {
        if let Some(quic) = source.downcast_ref::<eggress_transport_quic::QuicError>() {
            return match quic {
                eggress_transport_quic::QuicError::Tls(_) => ClassifiedKind::Tls,
                eggress_transport_quic::QuicError::Resolve(_) => ClassifiedKind::Dns,
                eggress_transport_quic::QuicError::MissingCertificate => ClassifiedKind::Policy,
                eggress_transport_quic::QuicError::Endpoint(_)
                | eggress_transport_quic::QuicError::Connection(_)
                | eggress_transport_quic::QuicError::Stream(_) => ClassifiedKind::Protocol,
            };
        }
        if let Some(h3) = source.downcast_ref::<eggress_protocol_h3::H3Error>() {
            return match h3 {
                eggress_protocol_h3::H3Error::Quic(inner) => match inner {
                    eggress_transport_quic::QuicError::Tls(_) => ClassifiedKind::Tls,
                    eggress_transport_quic::QuicError::Resolve(_) => ClassifiedKind::Dns,
                    eggress_transport_quic::QuicError::MissingCertificate => ClassifiedKind::Policy,
                    _ => ClassifiedKind::Protocol,
                },
                _ => ClassifiedKind::Protocol,
            };
        }
    }
    ClassifiedKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_kinds_classify_without_strings() {
        assert_eq!(
            classify_io_kind(std::io::ErrorKind::ConnectionRefused),
            ClassifiedKind::Refused
        );
        assert_eq!(
            classify_io_kind(std::io::ErrorKind::TimedOut),
            ClassifiedKind::Timeout
        );
        assert_eq!(
            classify_io_kind(std::io::ErrorKind::NetworkUnreachable),
            ClassifiedKind::NetworkUnreachable
        );
        assert_eq!(
            classify_io_kind(std::io::ErrorKind::HostUnreachable),
            ClassifiedKind::HostUnreachable
        );
        assert_eq!(
            classify_io_kind(std::io::ErrorKind::ConnectionReset),
            ClassifiedKind::Other
        );
    }

    #[test]
    fn connect_error_maps_typed() {
        assert_eq!(
            classify_connect_error(&eggress_core::ConnectError::ConnectionRefused),
            ClassifiedKind::Refused
        );
        assert_eq!(
            classify_connect_error(&eggress_core::ConnectError::Timeout),
            ClassifiedKind::Timeout
        );
        assert_eq!(
            classify_connect_error(&eggress_core::ConnectError::DnsResolution("x".into())),
            ClassifiedKind::Dns
        );
        assert_eq!(
            classify_connect_error(&eggress_core::ConnectError::TlsHandshake("x".into())),
            ClassifiedKind::Tls
        );
        assert_eq!(
            classify_connect_error(&eggress_core::ConnectError::ReservedTarget(
                "127.0.0.1".parse().unwrap()
            )),
            ClassifiedKind::Policy
        );
    }

    #[test]
    fn http_handshake_maps_typed() {
        let auth = eggress_protocol_http::HttpError::AuthFailed;
        let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(auth);
        assert_eq!(classify_handshake_source(&*boxed), ClassifiedKind::Auth);

        let timeout = eggress_protocol_http::HttpError::GatewayTimeout;
        let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(timeout);
        assert_eq!(classify_handshake_source(&*boxed), ClassifiedKind::Timeout);

        let bad_gateway = eggress_protocol_http::HttpError::BadGateway;
        let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(bad_gateway);
        assert_eq!(classify_handshake_source(&*boxed), ClassifiedKind::Protocol);
    }

    #[test]
    fn socks5_handshake_maps_typed() {
        let auth = eggress_protocol_socks::Socks5Error::AuthFailed;
        let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(auth);
        assert_eq!(classify_handshake_source(&*boxed), ClassifiedKind::Auth);

        let refused = eggress_protocol_socks::Socks5Error::ConnectionRefused;
        let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(refused);
        assert_eq!(classify_handshake_source(&*boxed), ClassifiedKind::Refused);
    }

    #[test]
    fn unknown_boxed_source_is_other() {
        #[derive(Debug)]
        struct Synthetic;
        impl std::fmt::Display for Synthetic {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("synthetic handshake failure")
            }
        }
        impl std::error::Error for Synthetic {}
        let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(Synthetic);
        assert_eq!(classify_handshake_source(&*boxed), ClassifiedKind::Other);
    }
}
