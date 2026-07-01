use crate::metrics::ReverseMetrics;
use crate::{relay_bidirectional, server_auth_handshake, ProtocolError};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Configuration for a reverse proxy server (acceptor side).
///
/// The server accepts control connections from remote clients and dispatches
/// externally-accepted connections back through the control channel.
#[derive(Debug, Clone)]
pub struct ReverseServerConfig {
    /// Address to bind the control listener on.
    pub control_bind: SocketAddr,
    /// Address to bind the external listener on (for clients to connect to).
    pub external_bind: Option<SocketAddr>,
    /// Optional username for authentication.
    pub auth_username: Option<String>,
    /// Optional password for authentication.
    pub auth_password: Option<String>,
    /// Maximum concurrent control connections.
    pub max_control_connections: u32,
    /// Read timeout in milliseconds (for idle control connections).
    pub read_timeout_ms: u64,
}

impl Default for ReverseServerConfig {
    fn default() -> Self {
        Self {
            control_bind: "127.0.0.1:0".parse().unwrap(),
            external_bind: None,
            auth_username: None,
            auth_password: None,
            max_control_connections: 256,
            read_timeout_ms: 300_000,
        }
    }
}

/// The reverse proxy server (acceptor side).
///
/// Accepts control connections from reverse clients and external clients,
/// relaying traffic between them. Each control connection carries exactly
/// one proxy session (matching pproxy's backward model).
pub struct ReverseServer {
    config: ReverseServerConfig,
    cancel: CancellationToken,
    metrics: Option<Arc<ReverseMetrics>>,
}

impl ReverseServer {
    pub fn new(config: ReverseServerConfig) -> Self {
        Self {
            config,
            cancel: CancellationToken::new(),
            metrics: None,
        }
    }

    /// Attach metrics to this server instance.
    pub fn set_metrics(&mut self, metrics: Arc<ReverseMetrics>) {
        self.metrics = Some(metrics);
    }

    /// Get a cancel token for external shutdown.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Start the reverse server.
    pub async fn run(self) -> Result<(), ProtocolError> {
        let control_listener = TcpListener::bind(&self.config.control_bind).await?;
        let control_addr = control_listener.local_addr()?;
        info!(addr = %control_addr, "reverse server listening for control connections");

        // Optionally bind external listener
        let external_listener = if let Some(external_bind) = self.config.external_bind {
            let listener = TcpListener::bind(external_bind).await?;
            let addr = listener.local_addr()?;
            info!(addr = %addr, "reverse server listening for external clients");
            Some(listener)
        } else {
            None
        };

        let config = Arc::new(self.config);
        let cancel = self.cancel.clone();

        // Channel for available control connections
        let (control_tx, control_rx) = mpsc::unbounded_channel::<TcpStream>();

        // Spawn control connection acceptor
        let config_clone = config.clone();
        let cancel_clone = cancel.clone();
        let control_tx_clone = control_tx.clone();
        let metrics_clone = self.metrics.clone();
        tokio::spawn(async move {
            Self::accept_control_connections(
                control_listener,
                config_clone,
                cancel_clone,
                control_tx_clone,
                metrics_clone,
            )
            .await;
        });

        // Spawn external client acceptor
        if let Some(external_listener) = external_listener {
            let config_clone = config.clone();
            let cancel_clone = cancel.clone();
            let metrics_clone = self.metrics.clone();
            tokio::spawn(async move {
                Self::accept_external_clients(
                    external_listener,
                    config_clone,
                    cancel_clone,
                    control_rx,
                    metrics_clone,
                )
                .await;
            });
        }

        // Wait for shutdown
        cancel.cancelled().await;
        info!("reverse server shutting down");

        Ok(())
    }

    /// Accept control connections, authenticate, and add to available pool.
    async fn accept_control_connections(
        listener: TcpListener,
        config: Arc<ReverseServerConfig>,
        cancel: CancellationToken,
        control_tx: mpsc::UnboundedSender<TcpStream>,
        metrics: Option<Arc<ReverseMetrics>>,
    ) {
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, peer_addr)) => {
                            let config = config.clone();
                            let control_tx = control_tx.clone();
                            let metrics = metrics.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_control_connection(
                                    stream,
                                    peer_addr,
                                    config,
                                    control_tx,
                                    metrics.as_deref(),
                                ).await {
                                    debug!(peer = %peer_addr, error = %e, "control connection handler error");
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
                    break;
                }
            }
        }
    }

    /// Handle a single control connection: authenticate and add to pool.
    async fn handle_control_connection(
        mut stream: TcpStream,
        peer_addr: SocketAddr,
        config: Arc<ReverseServerConfig>,
        control_tx: mpsc::UnboundedSender<TcpStream>,
        metrics: Option<&ReverseMetrics>,
    ) -> Result<(), ProtocolError> {
        info!(peer = %peer_addr, "new control connection");

        // Authenticate if configured
        if config.auth_username.is_some() || config.auth_password.is_some() {
            let result = server_auth_handshake(
                &mut stream,
                config.auth_username.as_deref(),
                config.auth_password.as_deref(),
            )
            .await;

            match result {
                Ok(_) => {
                    info!(peer = %peer_addr, "control connection authenticated");
                    if let Some(m) = metrics {
                        m.record_control_accepted(peer_addr);
                    }
                }
                Err(e) => {
                    warn!(peer = %peer_addr, error = %e, "control connection auth failed");
                    if let Some(m) = metrics {
                        m.record_control_rejected(peer_addr, &e.to_string());
                    }
                    return Err(e);
                }
            }
        } else {
            // No auth configured: send accept handshake
            crate::write_handshake_accept(&mut stream).await?;
            info!(peer = %peer_addr, "control connection accepted (no auth)");
            if let Some(m) = metrics {
                m.record_control_accepted(peer_addr);
            }
        }

        // Add to pool of available control connections
        // After this, the stream is owned by the channel receiver (relay task)
        if control_tx.send(stream).is_err() {
            warn!(peer = %peer_addr, "control channel closed, cannot add to pool");
        }

        // Handler returns immediately; the stream is now owned by the relay task
        Ok(())
    }

    /// Accept external clients and relay them through available control connections.
    async fn accept_external_clients(
        listener: TcpListener,
        _config: Arc<ReverseServerConfig>,
        cancel: CancellationToken,
        mut control_rx: mpsc::UnboundedReceiver<TcpStream>,
        metrics: Option<Arc<ReverseMetrics>>,
    ) {
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((external_stream, peer_addr)) => {
                            // Get an available control connection
                            match control_rx.recv().await {
                                Some(control_stream) => {
                                    let metrics = metrics.clone();
                                    tokio::spawn(async move {
                                        info!(peer = %peer_addr, "relaying external client through control connection");
                                        if let Some(m) = metrics.as_deref() {
                                            m.record_stream_opened();
                                        }
                                        if let Err(e) = relay_bidirectional(
                                            external_stream,
                                            control_stream,
                                        ).await {
                                            debug!(peer = %peer_addr, error = %e, "relay ended");
                                        }
                                        if let Some(m) = metrics.as_deref() {
                                            m.record_stream_closed(0);
                                        }
                                        debug!(peer = %peer_addr, "relay finished");
                                    });
                                }
                                None => {
                                    warn!(peer = %peer_addr, "no control connections available, rejecting external client");
                                    drop(external_stream);
                                }
                            }
                        }
                        Err(e) => {
                            error!(error = %e, "failed to accept external client");
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    break;
                }
            }
        }
    }

    /// Shut down the reverse server.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}
