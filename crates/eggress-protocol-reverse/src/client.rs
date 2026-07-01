use crate::metrics::ReverseMetrics;
use crate::{client_auth_handshake, relay_bidirectional, ProtocolError};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Configuration for a reverse proxy control client.
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
    /// Default target host (used when no target is specified in proxy request).
    pub default_target_host: Option<String>,
    /// Default target port.
    pub default_target_port: Option<u16>,
}

impl Default for ReverseClientConfig {
    fn default() -> Self {
        Self {
            server_addr: "127.0.0.1:0".parse().unwrap(),
            auth_username: None,
            auth_password: None,
            reconnect_initial_ms: 1_000,
            reconnect_max_ms: 30_000,
            default_target_host: None,
            default_target_port: None,
        }
    }
}

/// A reverse proxy control client.
///
/// Connects to a reverse server, authenticates, and services incoming proxy
/// requests by connecting to local targets and relaying data.
///
/// In pproxy's backward model, each control connection carries exactly one
/// proxy session. When the session ends, the client reconnects.
pub struct ReverseClient {
    config: ReverseClientConfig,
    cancel: CancellationToken,
    metrics: Option<Arc<ReverseMetrics>>,
}

impl ReverseClient {
    pub fn new(config: ReverseClientConfig) -> Self {
        Self {
            config,
            cancel: CancellationToken::new(),
            metrics: None,
        }
    }

    /// Attach metrics to this client instance.
    pub fn set_metrics(&mut self, metrics: Arc<ReverseMetrics>) {
        self.metrics = Some(metrics);
    }

    /// Get a cancel token for external shutdown.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Run the reverse client with automatic reconnection.
    pub async fn run(&self) -> Result<(), ProtocolError> {
        let mut backoff_ms = self.config.reconnect_initial_ms;

        loop {
            match self.run_session().await {
                Ok(()) => {
                    // Normal session end (external client disconnected)
                    // Reset backoff and reconnect immediately
                    backoff_ms = self.config.reconnect_initial_ms;
                    if self.cancel.is_cancelled() {
                        break;
                    }
                    debug!("session ended, reconnecting immediately");
                }
                Err(e) => {
                    if self.cancel.is_cancelled() {
                        break;
                    }
                    if let Some(ref m) = self.metrics {
                        m.record_reconnect();
                    }
                    warn!(error = %e, backoff_ms, "session failed, reconnecting");
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(self.config.reconnect_max_ms);
                }
            }
        }

        info!("reverse client shut down");
        Ok(())
    }

    /// Run a single session with the server.
    async fn run_session(&self) -> Result<(), ProtocolError> {
        let stream = TcpStream::connect(&self.config.server_addr).await?;
        info!(server = %self.config.server_addr, "connected to reverse server");

        // Authenticate
        let stream = if let Some(ref username) = self.config.auth_username {
            let password = self.config.auth_password.as_deref().unwrap_or("");
            let mut s = stream;
            client_auth_handshake(&mut s, username, password).await?;
            info!("authentication successful");
            s
        } else {
            // No auth: just read the handshake response
            let mut s = stream;
            crate::read_handshake(&mut s).await?;
            s
        };

        if let Some(ref m) = self.metrics {
            m.record_stream_opened();
        }

        // At this point, the control channel is established.
        // In pproxy's model, the acceptor relays an external client's
        // connection through this control channel as raw TCP. The server
        // runs relay_bidirectional between the external client and this
        // control stream. We connect to the configured default target and
        // relay bidirectionally between control and target.

        if let (Some(ref host), Some(port)) = (
            &self.config.default_target_host,
            self.config.default_target_port,
        ) {
            let target_addr = format!("{}:{}", host, port);
            match TcpStream::connect(&target_addr).await {
                Ok(target_stream) => {
                    info!(target = %target_addr, "connected to target, relaying");
                    relay_bidirectional(stream, target_stream).await?;
                }
                Err(e) => {
                    warn!(target = %target_addr, error = %e, "failed to connect to target");
                    if let Some(ref m) = self.metrics {
                        m.record_error(&format!("target connect failed: {e}"));
                    }
                    return Err(ProtocolError::Io(e));
                }
            }
        } else {
            // No default target configured; drain the control connection
            debug!("no default target configured, holding control connection open");
            let mut stream = stream;
            let mut buf = [0u8; 8192];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }

        if let Some(ref m) = self.metrics {
            m.record_stream_closed(0);
        }

        Ok(())
    }

    /// Shut down the reverse client.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}
