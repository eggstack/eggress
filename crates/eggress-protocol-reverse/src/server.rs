use crate::{read_frame, write_frame, Frame, FrameType, ProtocolError, MAX_FRAME_SIZE};
use bytes::BytesMut;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Configuration for a reverse listener (the side that accepts control connections).
#[derive(Debug, Clone)]
pub struct ReverseServerConfig {
    /// Address to bind the control listener on.
    pub control_bind: SocketAddr,
    /// Optional username for authentication.
    pub auth_username: Option<String>,
    /// Optional password for authentication.
    pub auth_password: Option<String>,
    /// Maximum concurrent streams per control client.
    pub max_streams: u32,
    /// Heartbeat interval in milliseconds.
    pub heartbeat_interval_ms: u64,
    /// Read timeout in milliseconds (for idle streams).
    pub read_timeout_ms: u64,
}

impl Default for ReverseServerConfig {
    fn default() -> Self {
        Self {
            control_bind: "127.0.0.1:0".parse().unwrap(),
            auth_username: None,
            auth_password: None,
            max_streams: 256,
            heartbeat_interval_ms: 30_000,
            read_timeout_ms: 300_000,
        }
    }
}

/// The reverse proxy server (acceptor side).
///
/// Accepts control connections from reverse clients, binds listeners on their
/// behalf, and dispatches accepted connections back through the control channel.
pub struct ReverseServer {
    config: ReverseServerConfig,
    cancel: CancellationToken,
}

impl ReverseServer {
    pub fn new(config: ReverseServerConfig) -> Self {
        Self {
            config,
            cancel: CancellationToken::new(),
        }
    }

    /// Get a cancel token that can be used to shut down the server.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Start the reverse server, accepting control connections.
    pub async fn run(self) -> Result<(), ProtocolError> {
        let listener = TcpListener::bind(&self.config.control_bind).await?;
        let local_addr = listener.local_addr()?;
        info!(addr = %local_addr, "reverse server listening for control connections");

        let config = Arc::new(self.config);
        let cancel = self.cancel.clone();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, peer_addr)) => {
                            let config = config.clone();
                            let cancel_child = cancel.child_token();
                            tokio::spawn(async move {
                                if let Err(e) = handle_control_connection(
                                    stream,
                                    peer_addr,
                                    config,
                                    cancel_child,
                                ).await {
                                    warn!(peer = %peer_addr, error = %e, "control connection terminated");
                                }
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "failed to accept control connection");
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    info!("reverse server shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Shut down the reverse server.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

/// Handle a single control connection from a reverse client.
async fn handle_control_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    config: Arc<ReverseServerConfig>,
    cancel: CancellationToken,
) -> Result<(), ProtocolError> {
    info!(peer = %peer_addr, "new control connection");

    // Authenticate if configured
    if config.auth_username.is_some() {
        let frame = read_frame(&mut stream, &mut BytesMut::with_capacity(1024)).await?;
        if frame.frame_type == FrameType::Auth {
            let (user, pass) = crate::decode_auth_payload(&frame.payload)?;
            if Some(&user) == config.auth_username.as_ref()
                && Some(&pass) == config.auth_password.as_ref()
            {
                write_frame(&mut stream, &Frame::auth_ok()).await?;
            } else {
                write_frame(&mut stream, &Frame::auth_fail()).await?;
                return Err(ProtocolError::AuthFailed);
            }
        } else {
            write_frame(&mut stream, &Frame::auth_fail()).await?;
            return Err(ProtocolError::AuthRequired);
        }
    }

    info!(peer = %peer_addr, "control connection authenticated");

    // Main loop: read frames from control client
    let mut buf = BytesMut::with_capacity(MAX_FRAME_SIZE);
    let mut stream_ids: Vec<u32> = Vec::new();

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
                                if stream_ids.len() >= config.max_streams as usize {
                                    warn!(peer = %peer_addr, stream_id, "max streams reached");
                                    write_frame(&mut stream, &Frame::stream_reset(stream_id)).await?;
                                    continue;
                                }

                                // Connect to target
                                let target_addr = format!("{}:{}", host, port);
                                match tokio::net::TcpStream::connect(&target_addr).await {
                                    Ok(target_stream) => {
                                        write_frame(&mut stream, &Frame::stream_opened(stream_id)).await?;
                                        stream_ids.push(stream_id);

                                        let (_stop_tx, stop_rx) = oneshot::channel();
                                        let stream_id_for_relay = stream_id;
                                        // Split the control stream for the relay
                                        let (ctrl_read, ctrl_write) = tokio::io::split(stream);
                                        // We need to reconstruct stream for the next loop iteration
                                        // This is the fundamental issue with the single-connection design
                                        // In pproxy's model, each +in connection is a single session,
                                        // so we don't actually need multiplexing.
                                        // For now, handle single-stream case.
                                        tokio::spawn(async move {
                                            run_stream_relay(
                                                ctrl_read,
                                                ctrl_write,
                                                target_stream,
                                                stream_id_for_relay,
                                                stop_rx,
                                            )
                                            .await;
                                        });
                                        // Since we split the stream, we can't continue the loop
                                        // In the real pproxy model, each control connection handles one stream
                                        return Ok(());
                                    }
                                    Err(e) => {
                                        warn!(peer = %peer_addr, stream_id, target = %target_addr, error = %e, "failed to connect to target");
                                        write_frame(&mut stream, &Frame::stream_reset(stream_id)).await?;
                                    }
                                }
                            }
                            FrameType::StreamClose => {
                                debug!(peer = %peer_addr, stream_id = frame.stream_id, "stream close");
                                break;
                            }
                            FrameType::Ping => {
                                write_frame(&mut stream, &Frame::pong()).await?;
                            }
                            FrameType::Pong => {}
                            _ => {
                                warn!(peer = %peer_addr, frame_type = ?frame.frame_type, "unexpected frame");
                            }
                        }
                    }
                    Err(e) => {
                        warn!(peer = %peer_addr, error = %e, "control connection read error");
                        break;
                    }
                }
            }
            _ = cancel.cancelled() => {
                info!(peer = %peer_addr, "control connection cancelled");
                break;
            }
        }
    }

    info!(peer = %peer_addr, "control connection closed");
    Ok(())
}

/// Run a relay between the control channel stream and a target connection.
async fn run_stream_relay(
    mut ctrl_read: tokio::io::ReadHalf<TcpStream>,
    mut ctrl_write: tokio::io::WriteHalf<TcpStream>,
    target_stream: TcpStream,
    stream_id: u32,
    stop_rx: oneshot::Receiver<()>,
) {
    let (mut target_read, mut target_write) = tokio::io::split(target_stream);

    let ctrl_to_target = async {
        let mut buf = [0u8; 8192];
        loop {
            match ctrl_read.read(&mut buf).await {
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
                    if ctrl_write.write_all(&buf[..n]).await.is_err() {
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
        _ = stop_rx => {}
    }

    debug!(stream_id, "stream relay ended");
}

use tokio::io::{AsyncReadExt, AsyncWriteExt};
