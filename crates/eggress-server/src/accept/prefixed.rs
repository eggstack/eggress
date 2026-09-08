//! Prefixed stream: replay handshake bytes, then delegate.
//!
//! Lets protocol detection peek at the first bytes without consuming
//! them from the live connection.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite};

use eggress_core::BoxStream;

/// A stream that returns `prefix` bytes first, then delegates to `inner`.
pub(crate) struct PrefixedStream {
    prefix: std::io::Cursor<Vec<u8>>,
    inner: BoxStream,
}

impl PrefixedStream {
    pub(crate) fn new(prefix: Vec<u8>, inner: BoxStream) -> Self {
        Self {
            prefix: std::io::Cursor::new(prefix),
            inner,
        }
    }
}

impl AsyncRead for PrefixedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let pos = self.prefix.position() as usize;
        let len = self.prefix.get_ref().len();
        if pos < len {
            let remaining = &self.prefix.get_ref()[pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.prefix.set_position((pos + to_copy) as u64);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedStream {
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

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
