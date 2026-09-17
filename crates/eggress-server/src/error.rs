#[derive(Debug, thiserror::Error)]
pub enum SessionOpenError {
    #[error("connection timed out")]
    Timeout,
    #[error("connection refused")]
    Refused,
    #[error("network unreachable")]
    NetworkUnreachable,
    #[error("host unreachable")]
    HostUnreachable,
    #[error("DNS resolution failed")]
    Dns,
    #[error("upstream authentication failed")]
    UpstreamAuthentication,
    #[error("request rejected by policy")]
    PolicyDenied,
    #[error("no eligible upstream available (transient)")]
    UpstreamUnavailable,
    #[error("route failed at hop {hop}")]
    Hop {
        hop: usize,
        source: Box<SessionOpenError>,
    },
    #[error("other connection error: {0}")]
    Other(String),
}

impl From<eggress_core::ConnectError> for SessionOpenError {
    fn from(e: eggress_core::ConnectError) -> Self {
        match e {
            eggress_core::ConnectError::ConnectionRefused => SessionOpenError::Refused,
            eggress_core::ConnectError::Timeout => SessionOpenError::Timeout,
            eggress_core::ConnectError::DnsResolution(_) => SessionOpenError::Dns,
            eggress_core::ConnectError::TlsHandshake(_) => {
                SessionOpenError::Other("TLS handshake failed".into())
            }
            eggress_core::ConnectError::Io(io) => {
                match crate::classify::classify_io_kind(io.kind()) {
                    crate::classify::ClassifiedKind::Refused => SessionOpenError::Refused,
                    crate::classify::ClassifiedKind::Timeout => SessionOpenError::Timeout,
                    crate::classify::ClassifiedKind::NetworkUnreachable => {
                        SessionOpenError::NetworkUnreachable
                    }
                    crate::classify::ClassifiedKind::HostUnreachable => {
                        SessionOpenError::HostUnreachable
                    }
                    _ => SessionOpenError::Other(io.to_string()),
                }
            }
            eggress_core::ConnectError::ReservedTarget(addr) => {
                SessionOpenError::Other(format!("reserved target: {addr}"))
            }
        }
    }
}

impl From<eggress_core::chain::ChainError> for SessionOpenError {
    fn from(e: eggress_core::chain::ChainError) -> Self {
        match e {
            eggress_core::chain::ChainError::ConnectFailed {
                hop_index, source, ..
            } => SessionOpenError::Hop {
                hop: hop_index,
                source: Box::new(SessionOpenError::from(source)),
            },
            eggress_core::chain::ChainError::HandshakeFailed {
                hop_index, source, ..
            } => {
                // Reuse the shared handshake classifier so built-in
                // HTTP/SOCKS (and other typed) failures keep their
                // structured category instead of flattening to a string.
                let kind = crate::classify::classify_handshake_source(&*source);
                let inner = match kind {
                    crate::classify::ClassifiedKind::Auth => {
                        SessionOpenError::UpstreamAuthentication
                    }
                    crate::classify::ClassifiedKind::Refused => SessionOpenError::Refused,
                    crate::classify::ClassifiedKind::Timeout => SessionOpenError::Timeout,
                    crate::classify::ClassifiedKind::Dns => SessionOpenError::Dns,
                    crate::classify::ClassifiedKind::NetworkUnreachable => {
                        SessionOpenError::NetworkUnreachable
                    }
                    crate::classify::ClassifiedKind::HostUnreachable => {
                        SessionOpenError::HostUnreachable
                    }
                    crate::classify::ClassifiedKind::Policy => SessionOpenError::PolicyDenied,
                    crate::classify::ClassifiedKind::Tls
                    | crate::classify::ClassifiedKind::Protocol
                    | crate::classify::ClassifiedKind::Other => {
                        SessionOpenError::Other(source.to_string())
                    }
                };
                SessionOpenError::Hop {
                    hop: hop_index,
                    source: Box::new(inner),
                }
            }
            eggress_core::chain::ChainError::EmptyChain => {
                SessionOpenError::Other("empty chain".into())
            }
            eggress_core::chain::ChainError::InvalidChain { reason } => {
                SessionOpenError::Other(format!("invalid chain: {reason}"))
            }
        }
    }
}

impl From<eggress_protocol_http::HttpError> for SessionOpenError {
    fn from(e: eggress_protocol_http::HttpError) -> Self {
        match e {
            eggress_protocol_http::HttpError::AuthRequired
            | eggress_protocol_http::HttpError::AuthFailed => {
                SessionOpenError::UpstreamAuthentication
            }
            eggress_protocol_http::HttpError::ConnectionRefused => SessionOpenError::Refused,
            eggress_protocol_http::HttpError::GatewayTimeout => SessionOpenError::Timeout,
            other => SessionOpenError::Other(other.to_string()),
        }
    }
}

impl From<eggress_protocol_socks::Socks5Error> for SessionOpenError {
    fn from(e: eggress_protocol_socks::Socks5Error) -> Self {
        match e {
            eggress_protocol_socks::Socks5Error::ConnectionRefused => SessionOpenError::Refused,
            eggress_protocol_socks::Socks5Error::AuthFailed => {
                SessionOpenError::UpstreamAuthentication
            }
            other => SessionOpenError::Other(other.to_string()),
        }
    }
}
