use std::fmt;
use std::net::SocketAddr;
#[cfg(target_os = "linux")]
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use tokio::net::TcpStream;

/// Errors that can occur when retrieving the original destination of a transparent connection.
#[derive(Debug)]
pub enum TransparentError {
    /// An I/O error occurred while querying the socket.
    Io(std::io::Error),
    /// The current platform does not support transparent proxying.
    UnsupportedPlatform,
    /// The socket has no original destination (not a redirected connection).
    NoOriginalDestination,
    /// Elevated privileges (e.g., CAP_NET_ADMIN or root) are required.
    PrivilegeRequired,
}

impl fmt::Display for TransparentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransparentError::Io(e) => write!(f, "I/O error: {e}"),
            TransparentError::UnsupportedPlatform => {
                write!(f, "transparent proxy is not supported on this platform")
            }
            TransparentError::NoOriginalDestination => {
                write!(f, "no original destination found for this connection")
            }
            TransparentError::PrivilegeRequired => {
                write!(f, "privilege required for transparent proxy")
            }
        }
    }
}

impl std::error::Error for TransparentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TransparentError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TransparentError {
    fn from(e: std::io::Error) -> Self {
        TransparentError::Io(e)
    }
}

/// Transparent TCP listener wrapper.
///
/// Wraps a `TcpListener` with transparent proxy capabilities, allowing
/// retrieval of the original destination for connections intercepted by
/// iptables/nftables REDIRECT or TPROXY rules.
pub struct TransparentListener {
    inner: tokio::net::TcpListener,
}

impl TransparentListener {
    /// Wraps an existing `TcpListener` with transparent proxy capabilities.
    pub fn new(listener: tokio::net::TcpListener) -> Self {
        Self { inner: listener }
    }

    /// Binds a new transparent listener to the given address.
    pub async fn bind(addr: &str) -> Result<Self, std::io::Error> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        Ok(Self::new(listener))
    }

    /// Accepts an incoming connection and retrieves its original destination.
    ///
    /// Returns the accepted stream along with the original destination address
    /// as reported by the kernel.
    pub async fn accept(&self) -> Result<(TcpStream, SocketAddr), TransparentError> {
        let (stream, _peer) = self.inner.accept().await?;
        let original_dst = get_original_destination(&stream)?;
        Ok((stream, original_dst))
    }

    /// Returns the local address this listener is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.inner.local_addr()
    }

    /// Returns a reference to the underlying `TcpListener`.
    pub fn inner(&self) -> &tokio::net::TcpListener {
        &self.inner
    }
}

/// Retrieves the original destination of a TCP connection intercepted by
/// transparent proxy rules (e.g., iptables REDIRECT, nftables).
///
/// On Linux, this queries `SO_ORIGINAL_DST` via `getsockopt`. IPv4 is
/// attempted first; if that fails, IPv6 is tried as a fallback.
///
/// On non-Linux platforms, returns `TransparentError::UnsupportedPlatform`.
pub fn get_original_destination(stream: &TcpStream) -> Result<SocketAddr, TransparentError> {
    get_original_destination_impl(stream)
}

#[cfg(target_os = "linux")]
fn get_original_destination_impl(stream: &TcpStream) -> Result<SocketAddr, TransparentError> {
    use std::os::unix::io::AsRawFd;

    let fd = stream.as_raw_fd();

    // Try IPv4 first: SOL_IP (0), SO_ORIGINAL_DST (80)
    if let Some(addr) = query_original_dst(fd, libc::SOL_IP, libc::SO_ORIGINAL_DST) {
        return Ok(addr);
    }

    // Try IPv6 fallback: SOL_IPV6 (41), SO_ORIGINAL_DST (80)
    if let Some(addr) = query_original_dst(fd, libc::SOL_IPV6, libc::SO_ORIGINAL_DST) {
        return Ok(addr);
    }

    Err(TransparentError::NoOriginalDestination)
}

#[cfg(not(target_os = "linux"))]
fn get_original_destination_impl(_stream: &TcpStream) -> Result<SocketAddr, TransparentError> {
    Err(TransparentError::UnsupportedPlatform)
}

/// Queries `getsockopt` for the original destination on a raw file descriptor.
///
/// Uses `libc::getsockopt` directly. The `SO_ORIGINAL_DST` option returns
/// a `sockaddr_in` (IPv4) or `sockaddr_in6` (IPv6) structure.
#[cfg(target_os = "linux")]
fn query_original_dst(fd: std::os::raw::c_int, level: i32, optname: i32) -> Option<SocketAddr> {
    // sockaddr_storage is large enough for both sockaddr_in and sockaddr_in6.
    let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

    let ret = unsafe {
        libc::getsockopt(
            fd,
            level,
            optname,
            &mut storage as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };

    if ret != 0 {
        return None;
    }

    parse_sockaddr(&storage, len)
}

/// Parses a `sockaddr_storage` into a `SocketAddr`.
#[cfg(target_os = "linux")]
fn parse_sockaddr(storage: &libc::sockaddr_storage, len: libc::socklen_t) -> Option<SocketAddr> {
    match storage.ss_family as i32 {
        libc::AF_INET if len as usize >= std::mem::size_of::<libc::sockaddr_in>() => {
            let sa = unsafe { *((storage as *const _) as *const libc::sockaddr_in) };
            let port = u16::from_be_bytes(sa.sin_port.to_ne_bytes());
            let octets = sa.sin_addr.s_addr.to_ne_bytes();
            Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3])),
                port,
            ))
        }
        libc::AF_INET6 if len as usize >= std::mem::size_of::<libc::sockaddr_in6>() => {
            let sa = unsafe { *((storage as *const _) as *const libc::sockaddr_in6) };
            let port = u16::from_be_bytes(sa.sin6_port.to_ne_bytes());
            let flowinfo = u32::from_be_bytes(sa.sin6_flowinfo.to_ne_bytes());
            let scope_id = sa.sin6_scope_id;
            let seg0 = u16::from_be_bytes(sa.sin6_addr.s6_addr[0..2].try_into().ok()?);
            let seg1 = u16::from_be_bytes(sa.sin6_addr.s6_addr[2..4].try_into().ok()?);
            let seg2 = u16::from_be_bytes(sa.sin6_addr.s6_addr[4..6].try_into().ok()?);
            let seg3 = u16::from_be_bytes(sa.sin6_addr.s6_addr[6..8].try_into().ok()?);
            let seg4 = u16::from_be_bytes(sa.sin6_addr.s6_addr[8..10].try_into().ok()?);
            let seg5 = u16::from_be_bytes(sa.sin6_addr.s6_addr[10..12].try_into().ok()?);
            let seg6 = u16::from_be_bytes(sa.sin6_addr.s6_addr[12..14].try_into().ok()?);
            let seg7 = u16::from_be_bytes(sa.sin6_addr.s6_addr[14..16].try_into().ok()?);
            Some(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::new(
                    seg0, seg1, seg2, seg3, seg4, seg5, seg6, seg7,
                )),
                port,
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transparent_error_display() {
        let err = TransparentError::UnsupportedPlatform;
        assert_eq!(
            err.to_string(),
            "transparent proxy is not supported on this platform"
        );

        let err = TransparentError::NoOriginalDestination;
        assert_eq!(
            err.to_string(),
            "no original destination found for this connection"
        );

        let err = TransparentError::PrivilegeRequired;
        assert_eq!(err.to_string(), "privilege required for transparent proxy");
    }

    #[test]
    fn test_transparent_error_is_error() {
        fn assert_error<T: std::error::Error>() {}
        assert_error::<TransparentError>();
    }

    #[test]
    fn test_transparent_error_from_io() {
        let io_err = std::io::Error::other("test");
        let err: TransparentError = io_err.into();
        assert!(matches!(err, TransparentError::Io(_)));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_get_original_destination_unsupported_without_redirect() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let stream = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let result = get_original_destination(&stream);
        assert!(
            matches!(result, Err(TransparentError::NoOriginalDestination)),
            "expected NoOriginalDestination without iptables redirect, got: {:?}",
            result
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn test_get_original_destination_unsupported_platform() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let stream = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let result = get_original_destination(&stream);
        assert!(
            matches!(result, Err(TransparentError::UnsupportedPlatform)),
            "expected UnsupportedPlatform on non-Linux, got: {:?}",
            result
        );
    }
}
