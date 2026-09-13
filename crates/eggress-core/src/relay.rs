use std::num::NonZeroUsize;
use std::time::Duration;

use crate::BoxStream;

/// Legacy Eggress copy buffer: 64 KiB per direction.
const LEGACY_BUFFER_SIZE: usize = 64 * 1024;

/// Legacy Eggress post-half-close drain: one second.
///
/// Without a bound, a half-closing peer whose FIN is never echoed by the
/// upstream would hold the relay (and its connection slot) indefinitely.
/// The generic `eggress-relay` engine defaults to an unbounded drain; this
/// facade explicitly opts into the historical bounded behavior so existing
/// server outcomes are unchanged.
const LEGACY_HALF_CLOSE_DRAIN: Duration = Duration::from_secs(1);

/// Reason the relay terminated.
///
/// `ClientClosed`/`ServerClosed` report which side hung up first when the
/// relay completed without an I/O failure; `Error` means at least one
/// direction failed. `BothClosed` is retained for API compatibility.
///
/// Note: when the bounded legacy drain expires, the facade reports the
/// first-closed side (`ClientClosed`/`ServerClosed`), matching historical
/// behavior. The richer `eggress-relay` API distinguishes that case as
/// `DrainTimedOut`; this facade intentionally collapses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    ClientClosed,
    ServerClosed,
    BothClosed,
    Error,
}

/// Result of a relay operation.
#[derive(Debug)]
pub struct RelayResult {
    pub bytes_upstream: u64,
    pub bytes_downstream: u64,
    pub termination_reason: TerminationReason,
}

fn legacy_options() -> eggress_relay::RelayOptions {
    eggress_relay::RelayOptions {
        buffer_size: NonZeroUsize::new(LEGACY_BUFFER_SIZE)
            .expect("legacy relay buffer is non-zero"),
        half_close: eggress_relay::HalfClosePolicy::DrainFor(LEGACY_HALF_CLOSE_DRAIN),
    }
}

fn map_termination(termination: eggress_relay::RelayTermination) -> TerminationReason {
    use eggress_relay::{RelaySide, RelayTermination};
    match termination {
        RelayTermination::ClientClosed => TerminationReason::ClientClosed,
        RelayTermination::ServerClosed => TerminationReason::ServerClosed,
        RelayTermination::DrainTimedOut { first_closed } => match first_closed {
            RelaySide::Client => TerminationReason::ClientClosed,
            RelaySide::Server => TerminationReason::ServerClosed,
        },
    }
}

/// Relay data bidirectionally between two streams.
///
/// Compatibility facade over [`eggress_relay::relay_with_options`] preserving
/// historical Eggress behavior: 64 KiB buffers, a one-second bounded
/// post-half-close drain, and collapsed `TerminationReason::Error` for any
/// directional I/O failure (details are debug-logged).
///
/// When one side closes its write half, the other side's write half is shut
/// down (half-close semantics). Both directions must complete — or the drain
/// must expire — before returning.
pub async fn relay(client: BoxStream, server: BoxStream) -> RelayResult {
    match eggress_relay::relay_with_options(client, server, legacy_options()).await {
        Ok(report) => RelayResult {
            bytes_upstream: report.bytes_upstream,
            bytes_downstream: report.bytes_downstream,
            termination_reason: map_termination(report.termination),
        },
        Err(failure) => {
            tracing::debug!(
                error = %failure.source,
                direction = %failure.direction,
                bytes_upstream = failure.bytes_upstream,
                bytes_downstream = failure.bytes_downstream,
                "relay direction failed"
            );
            RelayResult {
                bytes_upstream: failure.bytes_upstream,
                bytes_downstream: failure.bytes_downstream,
                termination_reason: TerminationReason::Error,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    struct FailingReadStream {
        kind: io::ErrorKind,
    }

    impl AsyncRead for FailingReadStream {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::new(self.kind, "injected relay failure")))
        }
    }

    impl AsyncWrite for FailingReadStream {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn legacy_facade_preserves_bounded_drain_options() {
        let options = legacy_options();
        assert_eq!(options.buffer_size.get(), 64 * 1024);
        assert_eq!(
            options.half_close,
            eggress_relay::HalfClosePolicy::DrainFor(std::time::Duration::from_secs(1))
        );
    }

    #[test]
    fn drain_timeout_collapses_to_first_closed_side() {
        use eggress_relay::{RelaySide, RelayTermination};
        assert_eq!(
            map_termination(RelayTermination::ClientClosed),
            TerminationReason::ClientClosed
        );
        assert_eq!(
            map_termination(RelayTermination::ServerClosed),
            TerminationReason::ServerClosed
        );
        assert_eq!(
            map_termination(RelayTermination::DrainTimedOut {
                first_closed: RelaySide::Client
            }),
            TerminationReason::ClientClosed
        );
        assert_eq!(
            map_termination(RelayTermination::DrainTimedOut {
                first_closed: RelaySide::Server
            }),
            TerminationReason::ServerClosed
        );
    }

    #[tokio::test]
    async fn test_relay_echo() {
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo.local_addr().unwrap();

        let jh = tokio::spawn(async move {
            let (stream, _) = echo.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                loop {
                    let n = reader.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    writer.write_all(&buf[..n]).await.unwrap();
                }
            });
        });

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (client_stream, _) = proxy_listener.accept().await.unwrap();
            let server_stream = tokio::net::TcpStream::connect(echo_addr).await.unwrap();
            relay(Box::new(client_stream), Box::new(server_stream)).await
        });

        let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"hello relay").await.unwrap();
        client.shutdown().await.unwrap();

        let mut buf = String::new();
        client.read_to_string(&mut buf).await.unwrap();
        assert_eq!(buf, "hello relay");

        let result = proxy_jh.await.unwrap();
        assert_eq!(result.bytes_upstream, 11);
        assert_eq!(result.bytes_downstream, 11);

        jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_relay_half_close() {
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo.local_addr().unwrap();

        let jh = tokio::spawn(async move {
            let (mut stream, _) = echo.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await.unwrap();
            stream.write_all(&buf[..n]).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (client_stream, _) = proxy_listener.accept().await.unwrap();
            let server_stream = tokio::net::TcpStream::connect(echo_addr).await.unwrap();
            relay(Box::new(client_stream), Box::new(server_stream)).await
        });

        let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"data").await.unwrap();
        client.shutdown().await.unwrap();

        let mut buf = [0u8; 4];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"data");

        let result = proxy_jh.await.unwrap();
        assert_eq!(result.bytes_upstream, 4);
        assert_eq!(result.bytes_downstream, 4);
        // The client closed its write half first.
        assert_eq!(result.termination_reason, TerminationReason::ClientClosed);

        jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_relay_half_close_server_hangs() {
        // The upstream reads the client payload and then never writes or closes.
        // Without the half-close drain + abort, the relay would block forever
        // on the downstream reader waiting for an EOF that never arrives.
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();

        let upstream_jh = tokio::spawn(async move {
            let (mut stream, _) = upstream.accept().await.unwrap();
            let mut buf = [0u8; 64];
            let _ = stream.read(&mut buf).await.unwrap();
            std::future::pending::<()>().await;
        });

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (client_stream, _) = proxy_listener.accept().await.unwrap();
            let server_stream = tokio::net::TcpStream::connect(upstream_addr).await.unwrap();
            relay(Box::new(client_stream), Box::new(server_stream)).await
        });

        let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"data").await.unwrap();
        client.shutdown().await.unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_secs(5), proxy_jh)
            .await
            .expect("relay should not block forever on a hanging upstream")
            .unwrap();
        assert_eq!(result.bytes_upstream, 4);
        assert_eq!(result.bytes_downstream, 0);
        assert_eq!(result.termination_reason, TerminationReason::ClientClosed);

        upstream_jh.abort();
    }

    #[tokio::test]
    async fn test_relay_server_half_close_first() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();

        let upstream_jh = tokio::spawn(async move {
            let (mut stream, _) = upstream.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await.unwrap();
            stream.write_all(&buf[..n]).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (client_stream, _) = proxy_listener.accept().await.unwrap();
            let server_stream = tokio::net::TcpStream::connect(upstream_addr).await.unwrap();
            relay(Box::new(client_stream), Box::new(server_stream)).await
        });

        let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"data").await.unwrap();

        let mut buf = [0u8; 4];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"data");

        let result = tokio::time::timeout(std::time::Duration::from_secs(5), proxy_jh)
            .await
            .expect("server-first close should complete")
            .unwrap();
        assert_eq!(result.bytes_upstream, 4);
        assert_eq!(result.bytes_downstream, 4);
        assert_eq!(result.termination_reason, TerminationReason::ServerClosed);

        upstream_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_relay_io_error_maps_to_error() {
        let (client_side, _peer) = tokio::io::duplex(1024);
        let failing: BoxStream = Box::new(FailingReadStream {
            kind: io::ErrorKind::ConnectionReset,
        });
        let result = relay(Box::new(client_side), failing).await;
        assert_eq!(result.termination_reason, TerminationReason::Error);
    }

    #[tokio::test]
    async fn test_relay_cancellation() {
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo.local_addr().unwrap();

        let jh = tokio::spawn(async move {
            let (stream, _) = echo.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                loop {
                    let n = reader.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    writer.write_all(&buf[..n]).await.unwrap();
                }
            });
        });

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let proxy_jh = tokio::spawn(async move {
            let (client_stream, _) = proxy_listener.accept().await.unwrap();
            let server_stream = tokio::net::TcpStream::connect(echo_addr).await.unwrap();
            relay(Box::new(client_stream), Box::new(server_stream)).await
        });

        let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"data").await.unwrap();
        drop(client);

        let result = proxy_jh.await.unwrap();
        assert!(result.bytes_upstream > 0 || result.bytes_downstream > 0);

        jh.await.unwrap();
    }
}
