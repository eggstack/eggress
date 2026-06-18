use std::time::Duration;

use tokio::io::AsyncReadExt;

use crate::detect::{DetectResult, ProtocolDetector};
use crate::replay::ReplayStream;
use crate::{BoxStream, ProtocolId};

const DEFAULT_MAX_SNIFF: usize = 8 * 1024;

/// Errors that can occur during protocol dispatch.
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("handshake timeout")]
    Timeout,
    #[error("sniff buffer full ({0} bytes) with no protocol match")]
    BufferOverflow(usize),
    #[error("no protocol matched the connection")]
    NoMatch,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Dispatches a connection to the appropriate protocol handler based on
/// sniffed initial bytes.
pub struct ProtocolDispatcher {
    detectors: Vec<Box<dyn ProtocolDetector>>,
    max_sniff: usize,
    handshake_timeout: Duration,
}

impl ProtocolDispatcher {
    /// Creates a new `ProtocolDispatcher`.
    ///
    /// # Arguments
    /// * `detectors` - Ordered list of protocol detectors. The first match wins.
    /// * `max_sniff` - Maximum number of bytes to buffer for protocol detection.
    /// * `handshake_timeout` - Maximum time to spend on protocol detection.
    pub fn new(
        detectors: Vec<Box<dyn ProtocolDetector>>,
        max_sniff: usize,
        handshake_timeout: Duration,
    ) -> Self {
        Self {
            detectors,
            max_sniff,
            handshake_timeout,
        }
    }

    /// Creates a new `ProtocolDispatcher` with default sniff buffer size.
    pub fn with_defaults(
        detectors: Vec<Box<dyn ProtocolDetector>>,
        handshake_timeout: Duration,
    ) -> Self {
        Self::new(detectors, DEFAULT_MAX_SNIFF, handshake_timeout)
    }

    /// Returns the list of registered protocol IDs.
    pub fn protocol_ids(&self) -> Vec<ProtocolId> {
        self.detectors.iter().map(|d| d.id()).collect()
    }

    /// Dispatches a connection by sniffing its initial bytes and matching
    /// against registered protocol detectors.
    ///
    /// Returns the matched protocol ID and a `ReplayStream` positioned at the
    /// start of the connection (all sniffed bytes are preserved in the buffer
    /// and will be replayed to the protocol handler on first read).
    pub async fn dispatch(
        &self,
        stream: BoxStream,
    ) -> Result<(ProtocolId, ReplayStream), DispatchError> {
        let mut replay = ReplayStream::with_max_buffer(stream, self.max_sniff);

        let mut read_buf = vec![0u8; 4096];
        let mut total_read: usize = 0;

        let result = tokio::time::timeout(self.handshake_timeout, async {
            loop {
                // Check buffer overflow
                if total_read >= self.max_sniff {
                    return Err(DispatchError::BufferOverflow(self.max_sniff));
                }

                // Read more data from the stream
                let to_read = (self.max_sniff - total_read).min(read_buf.len());
                let n = replay
                    .read(&mut read_buf[..to_read])
                    .await
                    .map_err(|e| DispatchError::Io(std::io::Error::new(e.kind(), e.to_string())))?;

                if n == 0 {
                    // Stream closed before we could determine the protocol.
                    break;
                }

                total_read += n;
                let prefix = &replay.buffer()[..total_read];

                // Try each detector in order
                let mut need_more_min = None;
                for detector in &self.detectors {
                    match detector.detect(prefix) {
                        DetectResult::Match { confidence: _ } => {
                            replay.finish_sniff();
                            return Ok((detector.id(), replay));
                        }
                        DetectResult::NeedMore { minimum } => {
                            if need_more_min.map_or(true, |m| minimum < m) {
                                need_more_min = Some(minimum);
                            }
                        }
                        DetectResult::NoMatch => {}
                    }
                }

                // If any detector needs more and we haven't hit the buffer
                // limit, continue reading.
                if need_more_min.is_some() {
                    if total_read < self.max_sniff {
                        continue;
                    }
                    // Buffer full but some detector still needs more data.
                    return Err(DispatchError::BufferOverflow(self.max_sniff));
                }

                // No detector needs more data and none matched.
                break;
            }

            // No protocol matched.
            Err(DispatchError::NoMatch)
        });

        match result.await {
            Ok(result) => result,
            Err(_elapsed) => Err(DispatchError::Timeout),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::PrefixDetector;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn make_dispatcher(handshake_timeout: Duration) -> ProtocolDispatcher {
        let detectors: Vec<Box<dyn ProtocolDetector>> = vec![
            Box::new(PrefixDetector::new(ProtocolId::Http, b"GET ".to_vec())),
            Box::new(PrefixDetector::new(ProtocolId::Socks5, b"\x05".to_vec())),
            Box::new(PrefixDetector::new(ProtocolId::Http, b"SSH-".to_vec())),
        ];
        ProtocolDispatcher::with_defaults(detectors, handshake_timeout)
    }

    #[tokio::test]
    async fn test_dispatch_http() {
        let dispatcher = make_dispatcher(Duration::from_secs(5));
        let (mut tx, rx) = tokio::io::duplex(1024);

        tx.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();

        let (proto, mut replay) = dispatcher.dispatch(Box::new(rx)).await.unwrap();
        assert_eq!(proto, ProtocolId::Http);

        // The replay stream should contain the full sniffed data
        assert_eq!(
            replay.buffer(),
            b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n"
        );

        // Reads after sniff should come from the underlying stream
        tx.write_all(b"more data").await.unwrap();
        tx.shutdown().await.unwrap();

        let mut buf = [0u8; 1024];
        let n = replay.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"more data");
    }

    #[tokio::test]
    async fn test_dispatch_socks5() {
        let dispatcher = make_dispatcher(Duration::from_secs(5));
        let (mut tx, rx) = tokio::io::duplex(1024);

        tx.write_all(b"\x05\x01\x00").await.unwrap();

        let (proto, _) = dispatcher.dispatch(Box::new(rx)).await.unwrap();
        assert_eq!(proto, ProtocolId::Socks5);
    }

    #[tokio::test]
    async fn test_dispatch_ssh() {
        let dispatcher = make_dispatcher(Duration::from_secs(5));
        let (mut tx, rx) = tokio::io::duplex(1024);

        tx.write_all(b"SSH-2.0-OpenSSH_8.9\r\n").await.unwrap();

        let (proto, _) = dispatcher.dispatch(Box::new(rx)).await.unwrap();
        assert_eq!(proto, ProtocolId::Http);
    }

    #[tokio::test]
    async fn test_dispatch_no_match() {
        let dispatcher = make_dispatcher(Duration::from_secs(5));
        let (tx, rx) = tokio::io::duplex(1024);

        let jh = tokio::spawn(async move {
            let mut stream = tx;
            stream.write_all(b"\xFF\xFE\xFD").await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let result = dispatcher.dispatch(Box::new(rx)).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            DispatchError::NoMatch => {}
            e => panic!("expected NoMatch, got {:?}", e),
        }
        jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_dispatch_timeout() {
        let dispatcher = make_dispatcher(Duration::from_millis(50));
        let (_tx, rx) = tokio::io::duplex(1024);

        // Never send data — should time out
        let result = dispatcher.dispatch(Box::new(rx)).await;
        assert!(matches!(result, Err(DispatchError::Timeout)));
    }

    #[tokio::test]
    async fn test_dispatch_buffer_overflow() {
        let detectors: Vec<Box<dyn ProtocolDetector>> = vec![Box::new(PrefixDetector::new(
            ProtocolId::Http,
            b"NEVER_MATCH_ANYTHING_HERE_FOREVER".to_vec(),
        ))];
        let dispatcher = ProtocolDispatcher::new(
            detectors,
            16, // very small buffer
            Duration::from_secs(5),
        );

        let (tx, rx) = tokio::io::duplex(1024);

        let jh = tokio::spawn(async move {
            let mut stream = tx;
            stream.write_all(b"AAAA_BBBB_CCCC_DDDD_EEEE").await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let result = dispatcher.dispatch(Box::new(rx)).await;
        assert!(matches!(result, Err(DispatchError::BufferOverflow(16))));
        jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_dispatch_ordered_detection() {
        // Both detectors would match \x05, but HTTP is listed first
        let detectors: Vec<Box<dyn ProtocolDetector>> = vec![
            Box::new(PrefixDetector::new(ProtocolId::Http, b"\x05".to_vec())),
            Box::new(PrefixDetector::new(ProtocolId::Socks5, b"\x05".to_vec())),
        ];
        let dispatcher = ProtocolDispatcher::with_defaults(detectors, Duration::from_secs(5));

        let (mut tx, rx) = tokio::io::duplex(1024);
        tx.write_all(b"\x05").await.unwrap();

        let (proto, _) = dispatcher.dispatch(Box::new(rx)).await.unwrap();
        // First detector wins
        assert_eq!(proto, ProtocolId::Http);
    }

    #[tokio::test]
    async fn test_dispatch_fragmented_detection() {
        let dispatcher = make_dispatcher(Duration::from_secs(5));
        let (tx, rx) = tokio::io::duplex(1024);

        let jh = tokio::spawn(async move {
            let mut stream = tx;
            stream.write_all(b"GE").await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            stream.write_all(b"T /").await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let (proto, _) = dispatcher.dispatch(Box::new(rx)).await.unwrap();
        assert_eq!(proto, ProtocolId::Http);

        jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_dispatch_unknown_input_closes() {
        let dispatcher = make_dispatcher(Duration::from_secs(5));
        let (tx, rx) = tokio::io::duplex(1024);

        // Send nothing then close
        drop(tx);

        let result = dispatcher.dispatch(Box::new(rx)).await;
        // Should get NoMatch since stream closed before any detector matched
        assert!(matches!(result, Err(DispatchError::NoMatch)));
    }

    #[tokio::test]
    async fn test_dispatch_stream_closed_mid_detection() {
        let dispatcher = make_dispatcher(Duration::from_secs(5));
        let (mut tx, rx) = tokio::io::duplex(1024);

        // Send a partial prefix then close
        tx.write_all(b"GE").await.unwrap();
        drop(tx);

        let result = dispatcher.dispatch(Box::new(rx)).await;
        assert!(matches!(result, Err(DispatchError::NoMatch)));
    }

    #[tokio::test]
    async fn test_dispatch_protocol_ids() {
        let dispatcher = make_dispatcher(Duration::from_secs(5));
        let ids = dispatcher.protocol_ids();
        assert_eq!(
            ids,
            vec![ProtocolId::Http, ProtocolId::Socks5, ProtocolId::Http]
        );
    }
}
