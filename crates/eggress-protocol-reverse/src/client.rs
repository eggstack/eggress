use crate::metrics::ReverseMetrics;
use crate::{client_auth_handshake, relay_bidirectional_with_timeout, ControlState, ProtocolError};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
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
    /// Idle read timeout on the control channel in milliseconds. 0 = no
    /// timeout.
    pub read_timeout_ms: u64,
    /// Grace period for drain on shutdown, in milliseconds.
    pub drain_grace_ms: u64,
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
            read_timeout_ms: 60_000,
            drain_grace_ms: 5_000,
        }
    }
}

/// Result of resolving where to send a relayed stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetResolution {
    /// Connect to the given host:port.
    Connect { host: String, port: u16 },
    /// Reject the stream and close the control channel.
    Reject { reason: String },
}

/// Trait for resolving the target of a relayed reverse stream.
///
/// The default implementation (used when no resolver is attached) returns
/// the configured `default_target_host`/`default_target_port`, or rejects the
/// stream if no default is configured. Production deployments inject a
/// resolver that consults the route engine.
pub trait TargetResolver: Send + Sync {
    fn resolve(&self) -> TargetResolution;
}

/// Default resolver: uses the configured default target, or rejects.
pub struct DefaultTargetResolver {
    pub host: Option<String>,
    pub port: Option<u16>,
}

impl DefaultTargetResolver {
    pub fn new(host: Option<String>, port: Option<u16>) -> Self {
        Self { host, port }
    }
}

impl TargetResolver for DefaultTargetResolver {
    fn resolve(&self) -> TargetResolution {
        match (&self.host, self.port) {
            (Some(h), Some(p)) => TargetResolution::Connect {
                host: h.clone(),
                port: p,
            },
            _ => TargetResolution::Reject {
                reason: "no default target configured".to_string(),
            },
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
    resolver: Option<Arc<dyn TargetResolver>>,
}

impl ReverseClient {
    pub fn new(config: ReverseClientConfig) -> Self {
        let resolver: Arc<dyn TargetResolver> = Arc::new(DefaultTargetResolver::new(
            config.default_target_host.clone(),
            config.default_target_port,
        ));
        Self {
            config,
            cancel: CancellationToken::new(),
            metrics: None,
            resolver: Some(resolver),
        }
    }

    /// Attach metrics to this client instance.
    pub fn set_metrics(&mut self, metrics: Arc<ReverseMetrics>) {
        self.metrics = Some(metrics);
    }

    /// Replace the target resolver (defaults to `DefaultTargetResolver`).
    pub fn set_resolver(&mut self, resolver: Arc<dyn TargetResolver>) {
        self.resolver = Some(resolver);
    }

    /// Get a cancel token for external shutdown.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Run the reverse client with automatic reconnection.
    pub async fn run(&self) -> Result<(), ProtocolError> {
        let mut backoff_ms = self.config.reconnect_initial_ms;

        loop {
            if self.cancel.is_cancelled() {
                break;
            }
            let session_start = Instant::now();
            match self.run_session().await {
                Ok(()) => {
                    if let Some(ref m) = self.metrics {
                        m.record_state_duration(
                            ControlState::Ready,
                            session_start.elapsed().as_millis() as u64,
                        );
                    }
                    if self.cancel.is_cancelled() {
                        break;
                    }
                    // Normal session end (external client disconnected)
                    // Reset backoff and reconnect immediately
                    backoff_ms = self.config.reconnect_initial_ms;
                    debug!("session ended, reconnecting immediately");
                }
                Err(e) => {
                    if self.cancel.is_cancelled() {
                        break;
                    }
                    if let Some(ref m) = self.metrics {
                        m.record_reconnect();
                        m.record_state_duration(
                            ControlState::Connecting,
                            session_start.elapsed().as_millis() as u64,
                        );
                    }
                    warn!(error = %e, backoff_ms, "session failed, reconnecting");
                    let sleep = tokio::time::sleep(Duration::from_millis(backoff_ms));
                    tokio::select! {
                        _ = sleep => {}
                        _ = self.cancel.cancelled() => break,
                    }
                    backoff_ms = (backoff_ms * 2).min(self.config.reconnect_max_ms);
                }
            }
        }

        // Drain phase: wait briefly for any pending cleanup
        let drain_start = Instant::now();
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Some(ref m) = self.metrics {
            m.record_drain(drain_start.elapsed().as_millis() as u64);
        }
        info!("reverse client shut down");
        Ok(())
    }

    /// Run a single session with the server.
    async fn run_session(&self) -> Result<(), ProtocolError> {
        let connecting_start = Instant::now();
        let stream = TcpStream::connect(&self.config.server_addr).await?;
        if let Some(ref m) = self.metrics {
            m.record_state_duration(
                ControlState::Connecting,
                connecting_start.elapsed().as_millis() as u64,
            );
        }
        info!(
            server = %self.config.server_addr,
            state = ?ControlState::Connecting,
            "connected to reverse server"
        );

        // Authenticate
        let authenticating_start = Instant::now();
        let stream = if let (Some(ref username), Some(ref password)) =
            (&self.config.auth_username, &self.config.auth_password)
        {
            let mut s = stream;
            client_auth_handshake(&mut s, username, password).await?;
            if let Some(ref m) = self.metrics {
                m.record_state_duration(
                    ControlState::Authenticating,
                    authenticating_start.elapsed().as_millis() as u64,
                );
            }
            info!(
                state = ?ControlState::Authenticating,
                "authentication successful"
            );
            s
        } else {
            // No auth: just read the handshake response
            let mut s = stream;
            crate::read_handshake(&mut s).await?;
            if let Some(ref m) = self.metrics {
                m.record_state_duration(
                    ControlState::Authenticating,
                    authenticating_start.elapsed().as_millis() as u64,
                );
            }
            s
        };

        if let Some(ref m) = self.metrics {
            m.record_stream_opened();
        }
        let ready_start = Instant::now();

        // Resolve target via the route engine (or default resolver).
        let resolution =
            self.resolver
                .as_ref()
                .map(|r| r.resolve())
                .unwrap_or(TargetResolution::Reject {
                    reason: "no resolver configured".to_string(),
                });

        let session_result: Result<(), ProtocolError> = match resolution {
            TargetResolution::Connect { host, port } => {
                let target_addr = format!("{}:{}", host, port);
                match TcpStream::connect(&target_addr).await {
                    Ok(target_stream) => {
                        info!(
                            target = %target_addr,
                            state = ?ControlState::Ready,
                            "connected to target, relaying"
                        );
                        relay_bidirectional_with_timeout(
                            stream,
                            target_stream,
                            (self.config.read_timeout_ms > 0)
                                .then(|| Duration::from_millis(self.config.read_timeout_ms)),
                        )
                        .await
                    }
                    Err(e) => {
                        warn!(
                            target = %target_addr,
                            error = %e,
                            "failed to connect to target"
                        );
                        if let Some(ref m) = self.metrics {
                            m.record_error(&format!("target connect failed: {e}"));
                        }
                        Err(ProtocolError::Io(e))
                    }
                }
            }
            TargetResolution::Reject { reason } => {
                warn!(reason = %reason, "route resolution rejected, dropping control channel");
                if let Some(ref m) = self.metrics {
                    m.record_error(&format!("route rejected: {reason}"));
                }
                Err(ProtocolError::AuthFailed)
            }
        };

        if let Some(ref m) = self.metrics {
            m.record_state_duration(
                ControlState::Ready,
                ready_start.elapsed().as_millis() as u64,
            );
            m.record_stream_closed(0);
        }

        session_result
    }

    /// Shut down the reverse client.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedResolver(TargetResolution);
    impl TargetResolver for FixedResolver {
        fn resolve(&self) -> TargetResolution {
            self.0.clone()
        }
    }

    #[test]
    fn default_resolver_returns_configured_target() {
        let r = DefaultTargetResolver::new(Some("127.0.0.1".to_string()), Some(8080));
        assert_eq!(
            r.resolve(),
            TargetResolution::Connect {
                host: "127.0.0.1".to_string(),
                port: 8080,
            }
        );
    }

    #[test]
    fn default_resolver_rejects_when_unset() {
        let r = DefaultTargetResolver::new(None, None);
        match r.resolve() {
            TargetResolution::Reject { .. } => {}
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[test]
    fn default_resolver_rejects_partial() {
        let r = DefaultTargetResolver::new(Some("127.0.0.1".to_string()), None);
        match r.resolve() {
            TargetResolution::Reject { .. } => {}
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[test]
    fn custom_resolver_can_reject() {
        let r: Arc<dyn TargetResolver> = Arc::new(FixedResolver(TargetResolution::Reject {
            reason: "policy".to_string(),
        }));
        match r.resolve() {
            TargetResolution::Reject { reason } => assert_eq!(reason, "policy"),
            other => panic!("expected Reject, got {other:?}"),
        }
    }
}
