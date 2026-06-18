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
            eggress_core::ConnectError::DnsResolution(msg) => {
                SessionOpenError::Other(format!("DNS: {msg}"))
            }
            eggress_core::ConnectError::TlsHandshake(msg) => {
                SessionOpenError::Other(format!("TLS: {msg}"))
            }
            eggress_core::ConnectError::Io(io) => SessionOpenError::Other(io.to_string()),
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
            } => SessionOpenError::Hop {
                hop: hop_index,
                source: Box::new(SessionOpenError::Other(source.to_string())),
            },
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
