use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::BoxStream;

const DEFAULT_MAX_BUFFER: usize = 8 * 1024;

/// A wrapper around a `BoxStream` that provides bounded sniff buffering for
/// protocol detection. All bytes read during detection are preserved in an
/// internal buffer. After detection completes, subsequent reads are served
/// directly from the underlying stream.
pub struct ReplayStream {
    inner: BoxStream,
    buffer: Vec<u8>,
    read_pos: usize,
    sniffing: bool,
    max_buffer: usize,
}

impl std::fmt::Debug for ReplayStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayStream")
            .field("buffer_len", &self.buffer.len())
            .field("read_pos", &self.read_pos)
            .field("sniffing", &self.sniffing)
            .field("max_buffer", &self.max_buffer)
            .finish()
    }
}

impl ReplayStream {
    /// Creates a new `ReplayStream` with the default 8 KiB buffer size.
    pub fn new(stream: BoxStream) -> Self {
        Self {
            inner: stream,
            buffer: Vec::new(),
            read_pos: 0,
            sniffing: true,
            max_buffer: DEFAULT_MAX_BUFFER,
        }
    }

    /// Creates a new `ReplayStream` with a custom maximum buffer size.
    pub fn with_max_buffer(stream: BoxStream, max_buffer: usize) -> Self {
        Self {
            inner: stream,
            buffer: Vec::new(),
            read_pos: 0,
            sniffing: true,
            max_buffer,
        }
    }

    /// Returns a reference to the bytes that have been sniffed so far.
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// Consumes this `ReplayStream` and returns the underlying stream.
    ///
    /// All buffered bytes have already been returned to callers during
    /// detection reads, so the underlying stream is positioned right after
    /// the sniffed prefix.
    pub fn into_inner(self) -> BoxStream {
        self.inner
    }

    /// Disables sniffing mode. After this call, reads are served directly
    /// from the underlying stream. The buffer retains the sniffed bytes for
    /// inspection.
    pub fn finish_sniff(&mut self) {
        self.sniffing = false;
    }

    /// Returns the number of bytes remaining in the sniff buffer that have
    /// not yet been returned to a caller.
    pub fn buffered_remaining(&self) -> usize {
        self.buffer.len().saturating_sub(self.read_pos)
    }
}

impl AsyncRead for ReplayStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.sniffing {
            // Serve from buffer first if there are unconsumed bytes.
            let remaining = self.buffer.len().saturating_sub(self.read_pos);
            if remaining > 0 {
                let to_copy = remaining.min(buf.remaining());
                buf.put_slice(&self.buffer[self.read_pos..self.read_pos + to_copy]);
                self.read_pos += to_copy;
                return Poll::Ready(Ok(()));
            }

            // Buffer exhausted while sniffing: read more from the underlying
            // stream via a temporary buffer, then copy into our internal buffer.
            if self.buffer.len() >= self.max_buffer {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "sniff buffer full",
                )));
            }

            let space = self.max_buffer - self.buffer.len();
            let read_size = space.min(buf.remaining());
            let mut temp = vec![0u8; read_size];
            let mut temp_buf = ReadBuf::new(&mut temp);

            match Pin::new(&mut self.inner).poll_read(cx, &mut temp_buf) {
                Poll::Ready(Ok(())) => {
                    let filled = temp_buf.filled().len();
                    if filled == 0 {
                        // Underlying stream closed; return EOF.
                        return Poll::Ready(Ok(()));
                    }
                    self.buffer.extend_from_slice(&temp[..filled]);
                    let to_copy = filled.min(buf.remaining());
                    buf.put_slice(&self.buffer[self.read_pos..self.read_pos + to_copy]);
                    self.read_pos += to_copy;
                    Poll::Ready(Ok(()))
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
        } else {
            // Not sniffing: delegate directly to the underlying stream.
            Pin::new(&mut self.inner).poll_read(cx, buf)
        }
    }
}

impl AsyncWrite for ReplayStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_replay_stream_buffers_during_sniff() {
        let (mut tx, rx) = tokio::io::duplex(1024);
        let replay = ReplayStream::new(Box::new(rx));
        let mut replay = Box::pin(replay);

        tx.write_all(b"hello").await.unwrap();
        tx.shutdown().await.unwrap();

        let mut buf = [0u8; 1024];
        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello");
        assert_eq!(replay.buffer(), b"hello");
    }

    #[tokio::test]
    async fn test_replay_stream_preserves_all_bytes() {
        let (mut tx, rx) = tokio::io::duplex(1024);
        let mut replay = ReplayStream::new(Box::new(rx));

        tx.write_all(b"abcdef").await.unwrap();

        // Read 3 bytes at a time
        let mut buf = [0u8; 3];
        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"abc");

        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"def");

        assert_eq!(replay.buffer(), b"abcdef");
    }

    #[tokio::test]
    async fn test_replay_stream_into_inner_after_partial_read() {
        let (mut tx, rx) = tokio::io::duplex(1024);
        let mut replay = ReplayStream::new(Box::new(rx));

        tx.write_all(b"abcdefghij").await.unwrap();

        // Read only 5 bytes — the ReplayStream reads 5 from inner into its
        // buffer (limited by caller's buf size), then returns 5 to caller.
        let mut buf = [0u8; 5];
        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"abcde");
        assert_eq!(replay.buffer(), b"abcde");

        // Drop tx so the inner stream sees EOF after remaining bytes.
        drop(tx);

        // into_inner returns the underlying stream, which still has the
        // remaining 5 bytes unread.
        let mut inner = replay.into_inner();
        let mut remaining = Vec::new();
        inner.read_to_end(&mut remaining).await.unwrap();
        assert_eq!(&remaining[..], b"fghij");
    }

    #[tokio::test]
    async fn test_replay_stream_delegates_writes() {
        let (rx, mut tx) = tokio::io::duplex(1024);
        let mut replay = ReplayStream::new(Box::new(rx));

        replay.write_all(b"test").await.unwrap();

        let mut buf = [0u8; 4];
        tx.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"test");
    }

    #[tokio::test]
    async fn test_replay_stream_finish_sniff_delegates_to_inner() {
        let (mut tx, rx) = tokio::io::duplex(1024);
        let mut replay = ReplayStream::new(Box::new(rx));

        tx.write_all(b"hello").await.unwrap();

        let mut buf = [0u8; 1024];
        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello");

        replay.finish_sniff();
        assert!(!replay.sniffing);

        tx.write_all(b"world").await.unwrap();

        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"world");
    }

    #[tokio::test]
    async fn test_replay_stream_custom_max_buffer() {
        let (tx, rx) = tokio::io::duplex(1024);
        let mut replay = ReplayStream::with_max_buffer(Box::new(rx), 4);

        // Write 6 bytes
        let write_jh = tokio::spawn(async move {
            let mut stream = tx;
            stream.write_all(b"abcdef").await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let mut buf = [0u8; 4];
        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"abcd");

        // Buffer is now full (6 bytes > max_buffer of 4), next read should error
        let result = replay.read(&mut buf).await;
        assert!(result.is_err());

        write_jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_replay_stream_empty_read() {
        let (tx, rx) = tokio::io::duplex(1024);
        let mut replay = ReplayStream::new(Box::new(rx));

        // Close the write half
        drop(tx);

        let mut buf = [0u8; 1024];
        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_replay_stream_reads_after_sniff_continue_from_inner() {
        let (mut tx, rx) = tokio::io::duplex(1024);
        let mut replay = ReplayStream::new(Box::new(rx));

        tx.write_all(b"first").await.unwrap();

        // Read all data (triggers read from inner into buffer)
        let mut buf = [0u8; 1024];
        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"first");
        assert_eq!(replay.buffer(), b"first");
        assert_eq!(replay.buffered_remaining(), 0);

        // Finish sniff - subsequent reads go to inner
        replay.finish_sniff();

        tx.write_all(b"second").await.unwrap();
        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"second");
    }
}
