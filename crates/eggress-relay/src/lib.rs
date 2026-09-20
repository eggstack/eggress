//! Generic asynchronous bidirectional stream relay.
//!
//! `eggress-relay` copies bytes between two Tokio-compatible duplex streams
//! with explicit half-close semantics, directional byte accounting, and
//! directional I/O errors. It knows nothing about proxy protocols, routing,
//! TLS, URIs, configuration, listeners, metrics, or pproxy compatibility.
//!
//! For a full proxy service (listeners, routing, upstream chains), use
//! `eggress-embed`. Use this crate when you only need to shuttle bytes
//! between two already-connected streams.
//!
//! ```rust
//! use std::num::NonZeroUsize;
//! use std::time::Duration;
//!
//! use eggress_relay::{relay_with_options, HalfClosePolicy, RelayOptions};
//!
//! # #[tokio::main]
//! # async fn main() -> std::io::Result<()> {
//! let (client, server) = tokio::io::duplex(65536);
//! let options = RelayOptions {
//!     buffer_size: NonZeroUsize::new(65536).unwrap(),
//!     half_close: HalfClosePolicy::DrainFor(Duration::from_secs(1)),
//! };
//! // Drive the relay in the background; `client`/`server` halves below are
//! // the endpoints your application reads and writes.
//! let (relay_end_a, mut app_a) = tokio::io::duplex(65536);
//! let (relay_end_b, mut app_b) = tokio::io::duplex(65536);
//! let relay_task = tokio::spawn(relay_with_options(relay_end_a, relay_end_b, options));
//! # let _ = (client, server, app_a, app_b, relay_task);
//! # Ok(())
//! # }
//! ```

use std::fmt;
use std::num::NonZeroUsize;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Default per-direction copy buffer: 64 KiB, matching historical Eggress behavior.
pub const DEFAULT_BUFFER_SIZE: usize = 64 * 1024;

/// Post-half-close policy applied after one direction reaches EOF cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HalfClosePolicy {
    /// After one direction reaches EOF, wait for the opposite direction to
    /// finish without an additional relay-level deadline.
    ///
    /// This is the protocol-neutral default: a client may legitimately
    /// half-close its request while an upstream takes an unbounded amount of
    /// time to produce its response.
    Drain,
    /// After one direction reaches EOF, allow the opposite direction at most
    /// this duration to finish, then stop and report [`RelayTermination::DrainTimedOut`].
    ///
    /// Use this when a peer that never answers a FIN must not hold a relay
    /// slot indefinitely.
    DrainFor(Duration),
}

/// Options controlling a relay invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayOptions {
    /// Per-direction copy buffer size in bytes. Must be non-zero.
    pub buffer_size: NonZeroUsize,
    /// What to do after the first direction reaches EOF cleanly.
    pub half_close: HalfClosePolicy,
}

impl Default for RelayOptions {
    fn default() -> Self {
        Self {
            buffer_size: NonZeroUsize::new(DEFAULT_BUFFER_SIZE)
                .expect("default relay buffer is non-zero"),
            half_close: HalfClosePolicy::Drain,
        }
    }
}

/// Error returned when a validated [`RelayOptions`] cannot be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidRelayOptions {
    requested_buffer_size: usize,
}

impl fmt::Display for InvalidRelayOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid relay options: buffer_size must be non-zero, got {}",
            self.requested_buffer_size
        )
    }
}

impl std::error::Error for InvalidRelayOptions {}

impl RelayOptions {
    /// Build options from explicit fields without validation beyond the types.
    pub fn new(buffer_size: NonZeroUsize, half_close: HalfClosePolicy) -> Self {
        Self {
            buffer_size,
            half_close,
        }
    }

    /// Build options from a raw byte count, rejecting zero explicitly instead
    /// of silently repairing it.
    pub fn try_new(
        buffer_size: usize,
        half_close: HalfClosePolicy,
    ) -> Result<Self, InvalidRelayOptions> {
        let buffer_size = NonZeroUsize::new(buffer_size).ok_or(InvalidRelayOptions {
            requested_buffer_size: buffer_size,
        })?;
        Ok(Self {
            buffer_size,
            half_close,
        })
    }

    /// Convenience for the bounded-drain shape (`DrainFor`).
    pub fn bounded(buffer_size: NonZeroUsize, drain: Duration) -> Self {
        Self {
            buffer_size,
            half_close: HalfClosePolicy::DrainFor(drain),
        }
    }
}

/// Which endpoint closed first when a drain timeout expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelaySide {
    Client,
    Server,
}

impl fmt::Display for RelaySide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelaySide::Client => write!(f, "client"),
            RelaySide::Server => write!(f, "server"),
        }
    }
}

/// How a successful relay terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayTermination {
    /// The client-to-server direction reached EOF first and the opposite
    /// direction subsequently completed cleanly.
    ClientClosed,
    /// The server-to-client direction reached EOF first and the opposite
    /// direction subsequently completed cleanly.
    ServerClosed,
    /// The first direction closed cleanly but the opposite direction did not
    /// finish within the configured bounded drain.
    DrainTimedOut { first_closed: RelaySide },
}

impl fmt::Display for RelayTermination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelayTermination::ClientClosed => write!(f, "client closed first"),
            RelayTermination::ServerClosed => write!(f, "server closed first"),
            RelayTermination::DrainTimedOut { first_closed } => {
                write!(f, "drain timed out after {first_closed} closed first")
            }
        }
    }
}

/// Successful relay report with directional byte counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayReport {
    /// Bytes copied client -> server.
    pub bytes_upstream: u64,
    /// Bytes copied server -> client.
    pub bytes_downstream: u64,
    /// How the relay terminated.
    pub termination: RelayTermination,
}

/// Which copy direction failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayDirection {
    /// Client -> server.
    Upstream,
    /// Server -> client.
    Downstream,
}

impl fmt::Display for RelayDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelayDirection::Upstream => write!(f, "upstream (client -> server)"),
            RelayDirection::Downstream => write!(f, "downstream (server -> client)"),
        }
    }
}

/// Directional relay failure. Byte counts record progress before the failure.
#[derive(Debug)]
pub struct RelayFailure {
    /// Which direction produced the I/O error.
    pub direction: RelayDirection,
    /// The underlying I/O error.
    pub source: std::io::Error,
    /// Bytes copied client -> server before the failure.
    pub bytes_upstream: u64,
    /// Bytes copied server -> client before the failure.
    pub bytes_downstream: u64,
}

impl fmt::Display for RelayFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "relay {} failed: {}", self.direction, self.source)
    }
}

impl std::error::Error for RelayFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

struct DirectionState {
    buffer: Vec<u8>,
    read_len: usize,
    write_pos: usize,
    bytes: u64,
    eof: bool,
    shutdown_complete: bool,
}

impl DirectionState {
    fn new(buffer_size: usize) -> Self {
        Self {
            buffer: vec![0; buffer_size],
            read_len: 0,
            write_pos: 0,
            bytes: 0,
            eof: false,
            shutdown_complete: false,
        }
    }
}

enum DirectionPoll {
    Progress,
    Complete,
    Failed(std::io::Error),
}

/// Poll one direction while retaining both complete streams in the parent
/// future. The bounded step budget prevents a stream that is always ready from
/// monopolising a poll and starving the opposite direction.
fn poll_direction<R, W>(
    mut reader: std::pin::Pin<&mut R>,
    mut writer: std::pin::Pin<&mut W>,
    state: &mut DirectionState,
    cx: &mut std::task::Context<'_>,
) -> std::task::Poll<DirectionPoll>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut made_progress = false;

    for _ in 0..16 {
        if state.write_pos < state.read_len {
            match writer
                .as_mut()
                .poll_write(cx, &state.buffer[state.write_pos..state.read_len])
            {
                std::task::Poll::Ready(Ok(0)) => {
                    return std::task::Poll::Ready(DirectionPoll::Failed(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "relay write made no progress",
                    )));
                }
                std::task::Poll::Ready(Ok(written)) => {
                    state.write_pos += written;
                    state.bytes += written as u64;
                    made_progress = true;
                    continue;
                }
                std::task::Poll::Ready(Err(error)) => {
                    return std::task::Poll::Ready(DirectionPoll::Failed(error));
                }
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        }

        if state.read_len != 0 {
            state.read_len = 0;
            state.write_pos = 0;
        }

        if state.eof {
            if state.shutdown_complete {
                return std::task::Poll::Ready(DirectionPoll::Complete);
            }
            match writer.as_mut().poll_shutdown(cx) {
                std::task::Poll::Ready(Ok(())) => {
                    state.shutdown_complete = true;
                    return std::task::Poll::Ready(DirectionPoll::Complete);
                }
                std::task::Poll::Ready(Err(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                    ) =>
                {
                    state.shutdown_complete = true;
                    return std::task::Poll::Ready(DirectionPoll::Complete);
                }
                std::task::Poll::Ready(Err(error)) => {
                    return std::task::Poll::Ready(DirectionPoll::Failed(error));
                }
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        }

        let mut read_buf = ReadBuf::new(&mut state.buffer);
        match reader.as_mut().poll_read(cx, &mut read_buf) {
            std::task::Poll::Ready(Ok(())) => {
                state.read_len = read_buf.filled().len();
                if state.read_len == 0 {
                    state.eof = true;
                } else {
                    made_progress = true;
                }
                continue;
            }
            std::task::Poll::Ready(Err(error)) => {
                return std::task::Poll::Ready(DirectionPoll::Failed(error));
            }
            std::task::Poll::Pending => return std::task::Poll::Pending,
        }
    }

    if made_progress {
        cx.waker().wake_by_ref();
        std::task::Poll::Ready(DirectionPoll::Progress)
    } else {
        std::task::Poll::Pending
    }
}

struct RelayFuture<C, S> {
    client: C,
    server: S,
    upstream: DirectionState,
    downstream: DirectionState,
    half_close: HalfClosePolicy,
    first_closed: Option<RelaySide>,
    drain_deadline: Option<std::pin::Pin<Box<tokio::time::Sleep>>>,
}

impl<C, S> RelayFuture<C, S> {
    fn new(client: C, server: S, options: RelayOptions) -> Self {
        let buffer_size = options.buffer_size.get();
        Self {
            client,
            server,
            upstream: DirectionState::new(buffer_size),
            downstream: DirectionState::new(buffer_size),
            half_close: options.half_close,
            first_closed: None,
            drain_deadline: None,
        }
    }

    fn start_drain(&mut self, first_closed: RelaySide) {
        self.first_closed = Some(first_closed);
        if let HalfClosePolicy::DrainFor(duration) = self.half_close {
            self.drain_deadline = Some(Box::pin(tokio::time::sleep(duration)));
        }
    }
}

impl<C, S> std::future::Future for RelayFuture<C, S>
where
    C: AsyncRead + AsyncWrite + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
{
    type Output = Result<RelayReport, RelayFailure>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();

        if this.first_closed.is_none() {
            match poll_direction(
                std::pin::Pin::new(&mut this.client),
                std::pin::Pin::new(&mut this.server),
                &mut this.upstream,
                cx,
            ) {
                std::task::Poll::Ready(DirectionPoll::Complete) => {
                    this.start_drain(RelaySide::Client);
                }
                std::task::Poll::Ready(DirectionPoll::Failed(source)) => {
                    return std::task::Poll::Ready(Err(RelayFailure {
                        direction: RelayDirection::Upstream,
                        source,
                        bytes_upstream: this.upstream.bytes,
                        bytes_downstream: this.downstream.bytes,
                    }));
                }
                std::task::Poll::Ready(DirectionPoll::Progress) | std::task::Poll::Pending => {}
            }

            if this.first_closed.is_none() {
                match poll_direction(
                    std::pin::Pin::new(&mut this.server),
                    std::pin::Pin::new(&mut this.client),
                    &mut this.downstream,
                    cx,
                ) {
                    std::task::Poll::Ready(DirectionPoll::Complete) => {
                        this.start_drain(RelaySide::Server);
                    }
                    std::task::Poll::Ready(DirectionPoll::Failed(source)) => {
                        return std::task::Poll::Ready(Err(RelayFailure {
                            direction: RelayDirection::Downstream,
                            source,
                            bytes_upstream: this.upstream.bytes,
                            bytes_downstream: this.downstream.bytes,
                        }));
                    }
                    std::task::Poll::Ready(DirectionPoll::Progress) | std::task::Poll::Pending => {}
                }
            }

            if this.first_closed.is_none() {
                return std::task::Poll::Pending;
            }
        }

        if let Some(deadline) = this.drain_deadline.as_mut() {
            if std::future::Future::poll(deadline.as_mut(), cx).is_ready() {
                return std::task::Poll::Ready(Ok(RelayReport {
                    bytes_upstream: this.upstream.bytes,
                    bytes_downstream: this.downstream.bytes,
                    termination: RelayTermination::DrainTimedOut {
                        first_closed: this.first_closed.expect("drain has a first close"),
                    },
                }));
            }
        }

        let first_closed = this.first_closed.expect("drain has a first close");
        let remaining = match first_closed {
            RelaySide::Client => poll_direction(
                std::pin::Pin::new(&mut this.server),
                std::pin::Pin::new(&mut this.client),
                &mut this.downstream,
                cx,
            ),
            RelaySide::Server => poll_direction(
                std::pin::Pin::new(&mut this.client),
                std::pin::Pin::new(&mut this.server),
                &mut this.upstream,
                cx,
            ),
        };

        match remaining {
            std::task::Poll::Ready(DirectionPoll::Complete) => {
                std::task::Poll::Ready(Ok(RelayReport {
                    bytes_upstream: this.upstream.bytes,
                    bytes_downstream: this.downstream.bytes,
                    termination: match first_closed {
                        RelaySide::Client => RelayTermination::ClientClosed,
                        RelaySide::Server => RelayTermination::ServerClosed,
                    },
                }))
            }
            std::task::Poll::Ready(DirectionPoll::Failed(source)) => {
                std::task::Poll::Ready(Err(RelayFailure {
                    direction: match first_closed {
                        RelaySide::Client => RelayDirection::Downstream,
                        RelaySide::Server => RelayDirection::Upstream,
                    },
                    source,
                    bytes_upstream: this.upstream.bytes,
                    bytes_downstream: this.downstream.bytes,
                }))
            }
            std::task::Poll::Ready(DirectionPoll::Progress) | std::task::Poll::Pending => {
                std::task::Poll::Pending
            }
        }
    }
}

/// Relay with default options (64 KiB buffers, unbounded post-half-close drain).
pub async fn relay<C, S>(client: C, server: S) -> Result<RelayReport, RelayFailure>
where
    C: AsyncRead + AsyncWrite + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
{
    relay_with_options(client, server, RelayOptions::default()).await
}

/// Relay with explicit options.
///
/// The implementation runs both copy directions in the caller's task (no
/// `tokio::spawn`): the two direction futures are polled concurrently, and the
/// unfinished direction is dropped when an error or a bounded-drain deadline
/// occurs. Cancelling the returned future drops both directions with it; no
/// detached relay task can outlive it.
pub async fn relay_with_options<C, S>(
    client: C,
    server: S,
    options: RelayOptions,
) -> Result<RelayReport, RelayFailure>
where
    C: AsyncRead + AsyncWrite + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
{
    RelayFuture::new(client, server, options).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_options(half_close: HalfClosePolicy) -> RelayOptions {
        RelayOptions {
            buffer_size: NonZeroUsize::new(8192).unwrap(),
            half_close,
        }
    }

    async fn relay_pair(
        client_payload: &[u8],
        server_payload: &[u8],
        options: RelayOptions,
    ) -> (RelayReport, Vec<u8>, Vec<u8>) {
        let client_payload = client_payload.to_vec();
        let server_payload = server_payload.to_vec();
        let (client_end, relay_client) = tokio::io::duplex(65536);
        let (relay_server, server_end) = tokio::io::duplex(65536);
        let (mut client_end, mut server_end) = (client_end, server_end);

        let relay_task = tokio::spawn(relay_with_options(relay_client, relay_server, options));

        let client_len = client_payload.len();
        let client_write = tokio::spawn(async move {
            client_end.write_all(&client_payload).await.unwrap();
            client_end.shutdown().await.unwrap();
            let mut out = Vec::new();
            client_end.read_to_end(&mut out).await.unwrap();
            out
        });
        let server_write = tokio::spawn(async move {
            let mut buf = vec![0u8; client_len];
            if client_len > 0 {
                server_end.read_exact(&mut buf).await.unwrap();
            }
            server_end.write_all(&server_payload).await.unwrap();
            server_end.shutdown().await.unwrap();
            buf
        });

        let report = relay_task
            .await
            .unwrap()
            .unwrap_or_else(|failure| panic!("relay failed unexpectedly: {failure}"));
        let client_received = client_write.await.unwrap();
        let server_received = server_write.await.unwrap();
        (report, client_received, server_received)
    }

    #[tokio::test]
    async fn bidirectional_transfer_reports_exact_counts_client_first() {
        let (report, client_received, server_received) = relay_pair(
            b"hello upstream",
            b"hello downstream",
            test_options(HalfClosePolicy::Drain),
        )
        .await;
        assert_eq!(server_received, b"hello upstream");
        assert_eq!(client_received, b"hello downstream");
        assert_eq!(report.bytes_upstream, 14);
        assert_eq!(report.bytes_downstream, 16);
        assert_eq!(report.termination, RelayTermination::ClientClosed);
    }

    #[tokio::test]
    async fn server_first_close_reports_server_closed() {
        let (relay_client, mut client_peer) = tokio::io::duplex(65536);
        let (mut server_peer, relay_server) = tokio::io::duplex(65536);

        let relay_task = tokio::spawn(relay_with_options(
            relay_client,
            relay_server,
            test_options(HalfClosePolicy::Drain),
        ));

        server_peer.write_all(b"early").await.unwrap();
        server_peer.shutdown().await.unwrap();

        let mut buf = [0u8; 5];
        client_peer.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"early");
        client_peer.shutdown().await.unwrap();

        let report = tokio::time::timeout(Duration::from_secs(5), relay_task)
            .await
            .expect("relay should finish")
            .unwrap()
            .unwrap();
        assert_eq!(report.bytes_downstream, 5);
        assert_eq!(report.termination, RelayTermination::ServerClosed);
    }

    #[tokio::test]
    async fn unbounded_drain_delivers_slow_response_after_client_half_close() {
        let (client_end, relay_client) = tokio::io::duplex(65536);
        let (relay_server, server_end) = tokio::io::duplex(65536);
        let (mut client_end, mut server_end) = (client_end, server_end);

        let relay_task = tokio::spawn(relay_with_options(
            relay_client,
            relay_server,
            RelayOptions::default(),
        ));

        client_end.write_all(b"request").await.unwrap();
        client_end.shutdown().await.unwrap();

        let mut request = [0u8; 7];
        server_end.read_exact(&mut request).await.unwrap();
        assert_eq!(&request, b"request");
        // Longer than the legacy one-second Eggress drain would allow if it
        // were (incorrectly) applied to the generic default. Kept to a few
        // hundred milliseconds of wall-clock time.
        tokio::time::sleep(Duration::from_millis(300)).await;
        server_end.write_all(b"slow response").await.unwrap();
        server_end.shutdown().await.unwrap();

        let report = tokio::time::timeout(Duration::from_secs(5), relay_task)
            .await
            .expect("unbounded drain should deliver the slow response")
            .unwrap()
            .unwrap();
        assert_eq!(report.bytes_upstream, 7);
        assert_eq!(report.bytes_downstream, 13);
        assert_eq!(report.termination, RelayTermination::ClientClosed);

        let mut response = Vec::new();
        client_end.read_to_end(&mut response).await.unwrap();
        assert_eq!(response, b"slow response");
    }

    #[tokio::test]
    async fn bounded_drain_contrast_times_out_on_slow_response() {
        let (client_end, relay_client) = tokio::io::duplex(65536);
        let (relay_server, server_end) = tokio::io::duplex(65536);
        let (mut client_end, mut server_end) = (client_end, server_end);

        let relay_task = tokio::spawn(relay_with_options(
            relay_client,
            relay_server,
            RelayOptions::bounded(NonZeroUsize::new(8192).unwrap(), Duration::from_millis(50)),
        ));

        client_end.write_all(b"request").await.unwrap();
        client_end.shutdown().await.unwrap();

        // Hold the response past the 50ms bounded drain.
        let mut request = [0u8; 7];
        server_end.read_exact(&mut request).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let _ = server_end.write_all(b"too late").await;
        let _ = server_end.shutdown().await;

        let report = tokio::time::timeout(Duration::from_secs(5), relay_task)
            .await
            .expect("bounded drain should finish")
            .unwrap()
            .unwrap();
        assert_eq!(report.bytes_upstream, 7);
        assert_eq!(
            report.termination,
            RelayTermination::DrainTimedOut {
                first_closed: RelaySide::Client
            }
        );
        drop(client_end);
    }

    #[tokio::test]
    async fn bounded_drain_timeout_reports_first_closed_side_and_counts() {
        let (mut pending_client, relay_client) = tokio::io::duplex(1024);
        let (_relay_server, _server_peer) = tokio::io::duplex(1024);
        // Keep the server side open but silent: the client half-closes, the
        // server never answers, and the bounded drain must expire.
        let relay_task = tokio::spawn(relay_with_options(
            relay_client,
            _relay_server,
            RelayOptions::bounded(NonZeroUsize::new(1024).unwrap(), Duration::from_millis(30)),
        ));
        pending_client.write_all(b"ping").await.unwrap();
        pending_client.shutdown().await.unwrap();
        let report = tokio::time::timeout(Duration::from_secs(5), relay_task)
            .await
            .expect("bounded drain must not hang")
            .unwrap()
            .unwrap();
        assert_eq!(report.bytes_upstream, 4);
        assert_eq!(report.bytes_downstream, 0);
        assert_eq!(
            report.termination,
            RelayTermination::DrainTimedOut {
                first_closed: RelaySide::Client
            }
        );
    }

    struct ScriptedStream {
        read_data: Vec<u8>,
        read_pos: usize,
        fail_kind: Option<std::io::ErrorKind>,
        written: Vec<u8>,
    }

    impl ScriptedStream {
        fn new(read_data: Vec<u8>, fail_kind: Option<std::io::ErrorKind>) -> Self {
            Self {
                read_data,
                read_pos: 0,
                fail_kind,
                written: Vec::new(),
            }
        }
    }

    impl AsyncRead for ScriptedStream {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            if self.read_pos < self.read_data.len() {
                let n = std::cmp::min(buf.remaining(), self.read_data.len() - self.read_pos);
                buf.put_slice(&self.read_data[self.read_pos..self.read_pos + n]);
                self.read_pos += n;
                Poll::Ready(Ok(()))
            } else if let Some(kind) = self.fail_kind {
                Poll::Ready(Err(std::io::Error::new(kind, "scripted failure")))
            } else {
                Poll::Ready(Ok(()))
            }
        }
    }

    impl AsyncWrite for ScriptedStream {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            self.written.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn upstream_failure_reports_direction_kind_and_counts() {
        let client = ScriptedStream::new(
            b"partial".to_vec(),
            Some(std::io::ErrorKind::ConnectionReset),
        );
        let server = ScriptedStream::new(Vec::new(), None);
        let failure = relay_with_options(client, server, test_options(HalfClosePolicy::Drain))
            .await
            .expect_err("scripted upstream read must fail");
        assert_eq!(failure.direction, RelayDirection::Upstream);
        assert_eq!(failure.source.kind(), std::io::ErrorKind::ConnectionReset);
        assert_eq!(failure.bytes_upstream, 7);
        assert_eq!(failure.bytes_downstream, 0);
    }

    #[tokio::test]
    async fn downstream_failure_reports_direction_kind() {
        let client = ScriptedStream::new(Vec::new(), None);
        let server = ScriptedStream::new(b"reply".to_vec(), Some(std::io::ErrorKind::BrokenPipe));
        let failure = relay_with_options(client, server, test_options(HalfClosePolicy::Drain))
            .await
            .expect_err("scripted downstream read must fail");
        assert_eq!(failure.direction, RelayDirection::Downstream);
        assert_eq!(failure.source.kind(), std::io::ErrorKind::BrokenPipe);
        assert_eq!(failure.bytes_downstream, 5);
    }

    #[tokio::test]
    async fn tiny_buffer_preserves_content_and_counts() {
        let payload: Vec<u8> = (0..32_768u32).map(|i| (i % 251) as u8).collect();
        let expected = payload.clone();
        let (client_end, relay_client) = tokio::io::duplex(65536);
        let (relay_server, server_end) = tokio::io::duplex(65536);
        let (mut client_end, mut server_end) = (client_end, server_end);

        let relay_task = tokio::spawn(relay_with_options(
            relay_client,
            relay_server,
            RelayOptions {
                buffer_size: NonZeroUsize::new(7).unwrap(),
                half_close: HalfClosePolicy::Drain,
            },
        ));

        client_end.write_all(&payload).await.unwrap();
        client_end.shutdown().await.unwrap();

        let mut received = Vec::new();
        server_end.read_to_end(&mut received).await.unwrap();
        assert_eq!(received, expected);
        server_end.shutdown().await.unwrap();

        let report = tokio::time::timeout(Duration::from_secs(10), relay_task)
            .await
            .expect("tiny-buffer relay should finish")
            .unwrap()
            .unwrap();
        assert_eq!(report.bytes_upstream, 32768);
        assert_eq!(report.bytes_downstream, 0);

        let mut tail = Vec::new();
        client_end.read_to_end(&mut tail).await.unwrap();
        assert!(tail.is_empty());
    }

    struct DropCounted<S> {
        inner: S,
        drops: Arc<AtomicUsize>,
    }

    impl<S: AsyncRead + Unpin> AsyncRead for DropCounted<S> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.inner).poll_read(cx, buf)
        }
    }

    impl<S: AsyncWrite + Unpin> AsyncWrite for DropCounted<S> {
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

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.inner).poll_shutdown(cx)
        }
    }

    impl<S> Drop for DropCounted<S> {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn dropping_relay_releases_both_streams() {
        let drops = Arc::new(AtomicUsize::new(0));
        let (a_end, a_relay) = tokio::io::duplex(1024);
        let (b_relay, b_end) = tokio::io::duplex(1024);
        // Hold both peer ends open so the relay stays pending until dropped.
        let _peer_a = a_end;
        let _peer_b = b_end;

        let counted_a = DropCounted {
            inner: a_relay,
            drops: Arc::clone(&drops),
        };
        let counted_b = DropCounted {
            inner: b_relay,
            drops: Arc::clone(&drops),
        };

        let relay_future =
            relay_with_options(counted_a, counted_b, test_options(HalfClosePolicy::Drain));
        let mut relay_future = Box::pin(relay_future);
        // Poll once so the relay owns both streams, then drop without any
        // endpoint ever closing.
        tokio::select! {
            _ = &mut relay_future => panic!("relay should stay pending"),
            _ = tokio::time::sleep(Duration::from_millis(20)) => {},
        }
        drop(relay_future);
        // Both wrapped relay halves must be released with the outer future;
        // the engine spawns no child task that could retain them.
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(drops.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn zero_buffer_size_is_rejected() {
        assert!(RelayOptions::try_new(0, HalfClosePolicy::Drain).is_err());
        assert!(RelayOptions::try_new(1, HalfClosePolicy::Drain).is_ok());
    }

    #[test]
    fn default_is_unbounded_sixty_four_kib() {
        let options = RelayOptions::default();
        assert_eq!(options.buffer_size.get(), 64 * 1024);
        assert_eq!(options.half_close, HalfClosePolicy::Drain);
    }
}
