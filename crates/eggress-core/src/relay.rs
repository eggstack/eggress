use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::task::{AbortHandle, JoinSet};

use crate::BoxStream;

/// Time to wait for the opposite direction to drain naturally after one side
/// completes. Without this, a half-closing peer whose FIN is never echoed by
/// the upstream causes the relay to block forever on the other side's read.
const RELAY_HALF_CLOSE_DRAIN: Duration = Duration::from_secs(1);

/// Upper bound for a forced abort to take effect after the drain timeout.
const RELAY_ABORT_GRACE: Duration = Duration::from_secs(1);

/// Reason the relay terminated.
///
/// `ClientClosed`/`ServerClosed` report which side hung up first when both
/// directions completed cleanly; `Error` means at least one direction failed.
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

/// Which relay direction a spawned task was copying.
#[derive(Debug, Clone, Copy)]
enum Direction {
    /// Client → server (upstream).
    Upstream,
    /// Server → client (downstream).
    Downstream,
}

async fn copy_direction<R, W>(reader: &mut R, writer: &mut W, counter: &AtomicU64) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            if let Err(error) = writer.shutdown().await {
                if !matches!(
                    error.kind(),
                    io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                ) {
                    return Err(error);
                }
            }
            return Ok(());
        }
        writer.write_all(&buf[..n]).await?;
        counter.fetch_add(n as u64, Ordering::Relaxed);
    }
}

fn termination_reason(
    first_closed: Option<TerminationReason>,
    had_error: bool,
    drain_timed_out: bool,
) -> TerminationReason {
    if had_error {
        TerminationReason::Error
    } else if drain_timed_out {
        first_closed.unwrap_or(TerminationReason::Error)
    } else {
        first_closed.unwrap_or(TerminationReason::BothClosed)
    }
}

/// Relay data bidirectionally between two streams.
///
/// When one side closes its write half, the other side's write half is shut down
/// (half-close semantics). Both directions must complete before returning.
pub async fn relay(client: BoxStream, server: BoxStream) -> RelayResult {
    let (mut client_read, mut client_write) = io::split(client);
    let (mut server_read, mut server_write) = io::split(server);

    let bytes_upstream = Arc::new(AtomicU64::new(0));
    let bytes_downstream = Arc::new(AtomicU64::new(0));
    let mut tasks = JoinSet::new();

    let upstream_counter = Arc::clone(&bytes_upstream);
    let upstream_abort: AbortHandle = tasks.spawn(async move {
        let result = copy_direction(&mut client_read, &mut server_write, &upstream_counter).await;
        (Direction::Upstream, result)
    });

    let downstream_counter = Arc::clone(&bytes_downstream);
    let downstream_abort: AbortHandle = tasks.spawn(async move {
        let result = copy_direction(&mut server_read, &mut client_write, &downstream_counter).await;
        (Direction::Downstream, result)
    });

    let mut had_error = false;
    // Records the first direction to finish cleanly, so diagnostics can tell
    // which side hung up first.
    let mut first_closed: Option<TerminationReason> = None;
    // When half-close leaves one side's reader stuck on a peer that never
    // answers the FIN we sent, the relay aborts the surviving direction after
    // a short drain window so the connection does not leak.
    let mut pending_abort: Option<AbortHandle> = None;
    let mut drain_timed_out = false;

    match tasks.join_next().await {
        Some(Ok((direction, Ok(())))) => {
            let reason = match direction {
                Direction::Upstream => {
                    pending_abort = Some(downstream_abort.clone());
                    TerminationReason::ClientClosed
                }
                Direction::Downstream => {
                    pending_abort = Some(upstream_abort.clone());
                    TerminationReason::ServerClosed
                }
            };
            first_closed = Some(reason);
        }
        Some(Ok((direction, Err(error)))) => {
            tracing::debug!(%error, ?direction, "relay direction failed");
            had_error = true;
            upstream_abort.abort();
            downstream_abort.abort();
        }
        Some(Err(error)) => {
            tracing::debug!(%error, "relay direction task failed");
            had_error = true;
            upstream_abort.abort();
            downstream_abort.abort();
        }
        None => {}
    }

    if let Some(abort) = pending_abort.as_ref() {
        match tokio::time::timeout(RELAY_HALF_CLOSE_DRAIN, tasks.join_next()).await {
            Ok(Some(Ok((_, Ok(()))))) => {}
            Ok(Some(Ok((direction, Err(error))))) => {
                tracing::debug!(%error, ?direction, "relay direction failed during drain");
                had_error = true;
            }
            Ok(Some(Err(error))) => {
                tracing::debug!(%error, "relay direction task failed during drain");
                had_error = true;
            }
            Ok(None) => {}
            Err(_) => {
                drain_timed_out = true;
                abort.abort();
                let _ = tokio::time::timeout(RELAY_ABORT_GRACE, tasks.join_next()).await;
            }
        }
    }

    if !drain_timed_out {
        while let Some(outcome) = tasks.join_next().await {
            match outcome {
                Ok((direction, Err(error))) => {
                    tracing::debug!(%error, ?direction, "relay direction failed");
                    had_error = true;
                }
                Err(error) => {
                    tracing::debug!(%error, "relay direction task failed");
                    had_error = true;
                }
                Ok((_, Ok(()))) => {}
            }
        }
    }

    let termination_reason = termination_reason(first_closed, had_error, drain_timed_out);

    RelayResult {
        bytes_upstream: bytes_upstream.load(Ordering::Relaxed),
        bytes_downstream: bytes_downstream.load(Ordering::Relaxed),
        termination_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn relay_error_during_drain_takes_precedence_over_close_reason() {
        assert_eq!(
            termination_reason(Some(TerminationReason::ClientClosed), true, true),
            TerminationReason::Error
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
