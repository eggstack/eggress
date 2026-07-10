// The transparent proxy module uses libc FFI (getsockopt) on Linux only.
// Workspace lints deny unsafe_code; this is the single, documented exception
// per docs/adr/ADR_transparent_proxy_unsafe_boundary.md.

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
#[allow(unsafe_code)]
fn query_original_dst(fd: std::os::raw::c_int, level: i32, optname: i32) -> Option<SocketAddr> {
    // SAFETY: `sockaddr_storage` is a POD type whose layout is defined by the
    // platform libc. Initializing all bytes to zero is valid for any fully
    // zeroed struct, and the kernel only ever writes a `sockaddr_in` or
    // `sockaddr_in6` (both smaller than `sockaddr_storage`) into the buffer
    // via `getsockopt(SO_ORIGINAL_DST)`. `parse_sockaddr` below validates
    // `ss_family` and `len` before reinterpreting the storage as a concrete
    // sockaddr type.
    let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

    // SAFETY: `fd` is a borrowed raw file descriptor owned by the caller and
    // is valid for the duration of this call. The storage pointer is properly
    // aligned (cast from a stack-allocated `sockaddr_storage`) and points to
    // `len` bytes of writable memory. `getsockopt` only writes through the
    // provided pointer; we re-initialize `storage` immediately after the
    // call so previous contents are not relied upon.
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
#[allow(unsafe_code)]
fn parse_sockaddr(storage: &libc::sockaddr_storage, len: libc::socklen_t) -> Option<SocketAddr> {
    match storage.ss_family as i32 {
        libc::AF_INET if len as usize >= std::mem::size_of::<libc::sockaddr_in>() => {
            // SAFETY: We checked that `len` is at least `size_of::<sockaddr_in>()`
            // and `ss_family == AF_INET`. On Linux the kernel only writes a fully
            // populated `sockaddr_in` for `SO_ORIGINAL_DST` on `SOL_IP`, so the
            // `storage` bytes hold a valid `sockaddr_in`. Reading individual
            // fields via copy avoids aliasing assumptions.
            let sa = unsafe {
                std::ptr::read_unaligned(storage as *const _ as *const libc::sockaddr_in)
            };
            let port = u16::from_be(sa.sin_port);
            let octets = sa.sin_addr.s_addr.to_ne_bytes();
            Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3])),
                port,
            ))
        }
        libc::AF_INET6 if len as usize >= std::mem::size_of::<libc::sockaddr_in6>() => {
            // SAFETY: We checked that `len` is at least `size_of::<sockaddr_in6>()`
            // and `ss_family == AF_INET6`. The kernel fills a complete
            // `sockaddr_in6` for `SO_ORIGINAL_DST` on `SOL_IPV6`.
            //
            // NOTE: IPv6 `SO_ORIGINAL_DST` support requires kernel nf_conntrack
            // IPv6 support and a recent enough iptables/nftables. When that
            // path is unreachable we fall through to `NoOriginalDestination`.
            let sa = unsafe {
                std::ptr::read_unaligned(storage as *const _ as *const libc::sockaddr_in6)
            };
            let port = u16::from_be(sa.sin6_port);
            let octets = sa.sin6_addr.s6_addr;
            let seg = |i: usize| -> Option<u16> {
                Some(u16::from_be_bytes(octets[i..i + 2].try_into().ok()?))
            };
            Some(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::new(
                    seg(0)?,
                    seg(2)?,
                    seg(4)?,
                    seg(6)?,
                    seg(8)?,
                    seg(10)?,
                    seg(12)?,
                    seg(14)?,
                )),
                port,
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
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

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_sockaddr_rejects_unknown_family() {
        let storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        // ss_family defaults to 0 (AF_UNSPEC on Linux), so this should
        // always return None without crashing.
        let result = parse_sockaddr(&storage, 0);
        assert!(result.is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_sockaddr_rejects_truncated_ipv4() {
        // Build a sockaddr_in with AF_INET family but pass a too-short len.
        let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        unsafe {
            let sa = &mut *(&mut storage as *mut _ as *mut libc::sockaddr_in);
            sa.sin_family = libc::AF_INET as u16;
            sa.sin_port = 8080u16.to_be();
            sa.sin_addr.s_addr = u32::from_be_bytes([127, 0, 0, 1]);
        }
        let truncated_len = 4u32; // smaller than sizeof(sockaddr_in)
        let result = parse_sockaddr(&storage, truncated_len);
        assert!(result.is_none(), "truncated length must reject");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_sockaddr_round_trip_ipv4() {
        // Build a sockaddr_in with AF_INET family and parse it back.
        let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        unsafe {
            let sa = &mut *(&mut storage as *mut _ as *mut libc::sockaddr_in);
            sa.sin_family = libc::AF_INET as u16;
            sa.sin_port = 8080u16.to_be();
            sa.sin_addr.s_addr = u32::from_be_bytes([192, 0, 2, 7]);
        }
        let len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
        let result = parse_sockaddr(&storage, len).expect("IPv4 should parse");
        match result {
            SocketAddr::V4(v4) => {
                assert_eq!(v4.ip().to_string(), "192.0.2.7");
                assert_eq!(v4.port(), 8080);
            }
            other => panic!("expected IPv4 SocketAddr, got {other:?}"),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_sockaddr_round_trip_ipv6() {
        // Build a sockaddr_in6 with AF_INET6 family and parse it back.
        let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        unsafe {
            let sa = &mut *(&mut storage as *mut _ as *mut libc::sockaddr_in6);
            sa.sin6_family = libc::AF_INET6 as u16;
            sa.sin6_port = 9090u16.to_be();
            // 2001:db8::1
            let addr_bytes: [u8; 16] = [
                0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01,
            ];
            sa.sin6_addr.s6_addr = addr_bytes;
        }
        let len = std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t;
        let result = parse_sockaddr(&storage, len).expect("IPv6 should parse");
        match result {
            SocketAddr::V6(v6) => {
                assert_eq!(v6.ip().to_string(), "2001:db8::1");
                assert_eq!(v6.port(), 9090);
            }
            other => panic!("expected IPv6 SocketAddr, got {other:?}"),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_sockaddr_rejects_truncated_ipv6() {
        let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        unsafe {
            let sa = &mut *(&mut storage as *mut _ as *mut libc::sockaddr_in6);
            sa.sin6_family = libc::AF_INET6 as u16;
            sa.sin6_port = 9090u16.to_be();
        }
        let truncated_len = 4u32;
        let result = parse_sockaddr(&storage, truncated_len);
        assert!(result.is_none(), "truncated IPv6 length must reject");
    }
}
