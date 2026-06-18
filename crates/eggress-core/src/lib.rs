use std::fmt;
use std::net::IpAddr;

use tokio::io::{AsyncRead, AsyncWrite};

pub mod chain;
pub mod connector;
pub mod detect;
pub mod dispatch;
pub mod listener;
pub mod relay;
pub mod replay;

/// A unique identifier for a protocol handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolId {
    Http,
    Socks4,
    Socks5,
}

impl fmt::Display for ProtocolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolId::Http => write!(f, "http"),
            ProtocolId::Socks4 => write!(f, "socks4"),
            ProtocolId::Socks5 => write!(f, "socks5"),
        }
    }
}

/// A unique identifier for a listener.
pub type ListenerId = u64;

/// A unique identifier for an upstream proxy.
pub type UpstreamId = u64;

/// The host of a target server, either an IP address or a domain name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TargetHost {
    Ip(IpAddr),
    Domain(String),
}

impl fmt::Display for TargetHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetHost::Ip(ip) => write!(f, "{}", ip),
            TargetHost::Domain(domain) => write!(f, "{}", domain),
        }
    }
}

/// The address of a target server.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TargetAddr {
    pub host: TargetHost,
    pub port: u16,
}

impl fmt::Display for TargetAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

impl std::str::FromStr for TargetAddr {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(idx) = s.rfind(':') {
            let host_part = &s[..idx];
            let port_part = &s[idx + 1..];
            let port: u16 = port_part
                .parse()
                .map_err(|e| format!("invalid port '{port_part}': {e}"))?;
            let host = if let Ok(ip) = host_part.parse::<IpAddr>() {
                TargetHost::Ip(ip)
            } else {
                TargetHost::Domain(host_part.to_string())
            };
            Ok(TargetAddr { host, port })
        } else {
            Err(format!("invalid target format: {s}"))
        }
    }
}

/// Client identity information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientIdentity {
    Anonymous,
    Username(String),
    Opaque(String),
}

/// Context for a proxy session.
#[derive(Debug, Clone)]
pub struct SessionContext {
    pub session_id: u64,
    pub client_identity: ClientIdentity,
    pub target_addr: TargetAddr,
}

/// Action to take for a routed connection.
#[derive(Debug, Clone)]
pub enum RouteAction {
    Direct,
    Upstream(UpstreamId),
    Reject(RejectReason),
}

/// Reason for rejecting a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    UnsupportedProtocol,
    AuthRequired,
    AccessDenied,
    Blocked,
    InternalError,
}

impl fmt::Display for RejectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RejectReason::UnsupportedProtocol => write!(f, "unsupported protocol"),
            RejectReason::AuthRequired => write!(f, "authentication required"),
            RejectReason::AccessDenied => write!(f, "access denied"),
            RejectReason::Blocked => write!(f, "target address blocked"),
            RejectReason::InternalError => write!(f, "internal error"),
        }
    }
}

/// A trait that combines AsyncRead and AsyncWrite for bidirectional streams.
pub trait AsyncStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncStream for T {}

/// A type alias for a boxed async stream.
pub type BoxStream = Box<dyn AsyncStream>;

/// Error types for connection operations.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("connection refused")]
    ConnectionRefused,
    #[error("connection timed out")]
    Timeout,
    #[error("DNS resolution failed: {0}")]
    DnsResolution(String),
    #[error("TLS handshake failed: {0}")]
    TlsHandshake(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Error types for protocol operations.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("malformed message")]
    MalformedMessage,
    #[error("unsupported version")]
    UnsupportedVersion,
    #[error("method not supported")]
    MethodNotSupported,
    #[error("address type not supported")]
    AddressTypeNotSupported,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Error types for authentication operations.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("authentication method not supported")]
    MethodNotSupported,
    #[error("authentication required")]
    Required,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Error types for relay operations.
#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("connection closed")]
    ConnectionClosed,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_host_display() {
        let ip_host = TargetHost::Ip("127.0.0.1".parse().unwrap());
        assert_eq!(ip_host.to_string(), "127.0.0.1");

        let domain_host = TargetHost::Domain("example.com".to_string());
        assert_eq!(domain_host.to_string(), "example.com");
    }

    #[test]
    fn test_target_addr_display() {
        let addr = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 8080,
        };
        assert_eq!(addr.to_string(), "example.com:8080");
    }

    #[test]
    fn test_reject_reason_display() {
        assert_eq!(
            RejectReason::UnsupportedProtocol.to_string(),
            "unsupported protocol"
        );
        assert_eq!(
            RejectReason::AuthRequired.to_string(),
            "authentication required"
        );
    }
}
