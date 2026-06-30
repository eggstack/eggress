use crate::{read_frame, write_frame, Frame, FrameType, ProtocolError, MAX_FRAME_SIZE};
use bytes::BytesMut;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Configuration for a reverse control client.
#[derive(Debug, Clone)]
pub struct ReverseClientConfig {
    /// Address of the reverse server to connect to.
    pub server_addr: SocketAddr,
    /// Optional username for authentication.
    pub auth_username: Option<String>,
    /// Optional password for authentication.
    pub auth_password: Option<String>,
    /// Reconnect backoff initial delay in milliseconds.
    pub reconnect_initial_ms: u64,
    /// Reconnect backoff max delay in milliseconds.
    pub reconnect_max_ms: u64,
    /// Heartbeat interval in milliseconds.
    pub heartbeat_interval_ms: u64,
}

impl Default for ReverseClientConfig {
    fn default() -> Self {
        Self {
            server_addr: "127.0.0.1:0".parse().unwrap(),
            auth_username: None,
            auth_password: None,
            reconnect_initial_ms: 1_000,
            reconnect_max_ms: 30_000,
            heartbeat_interval_ms: 30_000,
        }
    }
}

/// A reverse proxy control client.
///
/// Connects to a reverse server, authenticates, and services incoming stream-open
/// requests by connecting to local targets and relaying data.
pub struct ReverseClient {
    config: ReverseClientConfig,
    cancel: CancellationToken,
}

impl ReverseClient {
    pub fn new(config: ReverseClientConfig) -> Self {
        Self {
            config,
            cancel: CancellationToken::new(),
        }
    }

    /// Run the reverse client with automatic reconnection.
    pub async fn run(&self) -> Result<(), ProtocolError> {
        let mut backoff_ms = self.config.reconnect_initial_ms;

        loop {
            match self.run_session().await {
                Ok(()) => {
                    info!("reverse client session ended normally");
                    break;
                }
                Err(e) => {
                    if self.cancel.is_cancelled() {
                        break;
                    }
                    warn!(error = %e, backoff_ms, "reverse client session failed, reconnecting");
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(self.config.reconnect_max_ms);
                }
            }
        }

        Ok(())
    }

    /// Run a single session with the server.
    async fn run_session(&self) -> Result<(), ProtocolError> {
        let mut stream = TcpStream::connect(&self.config.server_addr).await?;
        info!(server = %self.config.server_addr, "connected to reverse server");

        // Authenticate if configured
        if let Some(ref username) = self.config.auth_username {
            let password = self.config.auth_password.as_deref().unwrap_or("");
            write_frame(&mut stream, &Frame::auth(username, password)).await?;

            let resp = read_frame(&mut stream, &mut BytesMut::with_capacity(256)).await?;
            match resp.frame_type {
                FrameType::AuthOk => {
                    info!("authentication successful");
                }
                FrameType::AuthFail => {
                    return Err(ProtocolError::AuthFailed);
                }
                _ => {
                    return Err(ProtocolError::MalformedFrame);
                }
            }
        }

        // Main loop: read frames from server
        let mut buf = BytesMut::with_capacity(MAX_FRAME_SIZE);
        let cancel = self.cancel.clone();

        loop {
            tokio::select! {
                result = read_frame(&mut stream, &mut buf) => {
                    match result {
                        Ok(frame) => {
                            match frame.frame_type {
                                FrameType::OpenStream => {
                                    let (host, port) =
                                        crate::decode_open_stream_payload(&frame.payload)?;
                                    let stream_id = frame.stream_id;
                                    info!(stream_id, target = format!("{}:{}", host, port), "opening stream to target");

                                    // Connect to local target
                                    let target_addr = format!("{}:{}", host, port);
                                    match tokio::net::TcpStream::connect(&target_addr).await {
                                        Ok(target_stream) => {
                                            write_frame(&mut stream, &Frame::stream_opened(stream_id)).await?;

                                            let cancel_child = cancel.child_token();
                                            // Split stream for relay
                                            let (mut reader, mut writer) = tokio::io::split(stream);
                                            let (mut target_read, mut target_write) = tokio::io::split(target_stream);

                                            let ctrl_to_target = async {
                                                let mut buf = [0u8; 8192];
                                                loop {
                                                    match reader.read(&mut buf).await {
                                                        Ok(0) => break,
                                                        Ok(n) => {
                                                            if target_write.write_all(&buf[..n]).await.is_err() {
                                                                break;
                                                            }
                                                        }
                                                        Err(_) => break,
                                                    }
                                                }
                                            };

                                            let target_to_ctrl = async {
                                                let mut buf = [0u8; 8192];
                                                loop {
                                                    match target_read.read(&mut buf).await {
                                                        Ok(0) => break,
                                                        Ok(n) => {
                                                            if writer.write_all(&buf[..n]).await.is_err() {
                                                                break;
                                                            }
                                                        }
                                                        Err(_) => break,
                                                    }
                                                }
                                            };

                                            tokio::select! {
                                                _ = ctrl_to_target => {}
                                                _ = target_to_ctrl => {}
                                                _ = cancel_child.cancelled() => {}
                                            }

                                            debug!(stream_id, "stream relay ended");
                                            return Ok(());
                                        }
                                        Err(e) => {
                                            warn!(stream_id, target = %target_addr, error = %e, "failed to connect to target");
                                            write_frame(&mut stream, &Frame::stream_reset(stream_id)).await?;
                                        }
                                    }
                                }
                                FrameType::StreamData => {
                                    debug!(stream_id = frame.stream_id, data_len = frame.payload.len(), "received stream data");
                                }
                                FrameType::StreamClose => {
                                    debug!(stream_id = frame.stream_id, "stream closed by server");
                                }
                                FrameType::StreamReset => {
                                    warn!(stream_id = frame.stream_id, "stream reset by server");
                                }
                                FrameType::Ping => {
                                    write_frame(&mut stream, &Frame::pong()).await?;
                                }
                                _ => {
                                    warn!(frame_type = ?frame.frame_type, "unexpected frame from server");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "connection read error");
                            break;
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    info!("reverse client shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Shut down the reverse client.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

use tokio::io::{AsyncReadExt, AsyncWriteExt};
