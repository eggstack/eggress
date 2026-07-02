use crate::metrics::ReverseMetrics;
use crate::{redact_auth, relay_bidirectional, server_auth_handshake, ControlState, ProtocolError};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
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
    /// Optional list of allowed external bind addresses. When `Some` and
    /// non-empty, the server rejects bind addresses not in the list.
    /// When `None` or empty, no allowlist enforcement is applied.
    pub allow_bind: Option<Vec<SocketAddr>>,
    /// Maximum number of external listeners per control client. Currently
    /// pproxy supports one external listener per control connection, so
    /// defaults to 1.
    pub max_listeners_per_client: u32,
    /// Maximum concurrent streams per external listener.
    pub max_streams_per_listener: u32,
    /// Maximum number of concurrent external clients queued while waiting
    /// for a control connection. Excess clients are dropped.
    pub max_pending_external: u32,
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
            allow_bind: None,
            max_listeners_per_client: 1,
            max_streams_per_listener: 1024,
            max_pending_external: 1024,
        }
    }
}

impl ReverseServerConfig {
    /// Returns true if the supplied external bind address is allowed by the
    /// configured `allow_bind` policy. When `allow_bind` is `None` or empty,
    /// all addresses are allowed.
    pub fn is_bind_allowed(&self, addr: SocketAddr) -> bool {
        match &self.allow_bind {
            None => true,
            Some(list) if list.is_empty() => true,
            Some(list) => list.iter().any(|allowed| same_bind(allowed, &addr)),
        }
    }

    /// Returns true if the address is loopback (127.0.0.0/8 or ::1).
    pub fn is_loopback(addr: SocketAddr) -> bool {
        match addr.ip() {
            IpAddr::V4(v4) => v4.is_loopback(),
            IpAddr::V6(v6) => v6.is_loopback(),
        }
    }

    /// Validate this configuration. Returns an error if the configuration is
    /// unsafe (e.g. external bind on a non-loopback address without
    /// authentication and without an explicit `allow_bind` allowlist).
    ///
    /// This is a defense-in-depth check: it catches misconfigurations that
    /// would otherwise expose the reverse proxy to unauthenticated network
    /// clients.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(external) = self.external_bind {
            // Non-loopback external bind requires BOTH authentication
            // credentials AND a non-empty `allow_bind` allowlist. This
            // prevents accidentally exposing the reverse proxy to the
            // local network without operator intent.
            if !Self::is_loopback(external) {
                let has_auth = self.auth_username.as_deref().is_some_and(|s| !s.is_empty())
                    && self.auth_password.as_deref().is_some_and(|s| !s.is_empty());
                let has_allowlist = matches!(&self.allow_bind, Some(list) if !list.is_empty());
                if !has_auth {
                    return Err(ProtocolError::ConfigInvalid(format!(
                        "reverse server external_bind={external} is non-loopback but no \
                         authentication is configured; set auth_username/auth_password or \
                         bind to loopback"
                    )));
                }
                if !has_allowlist {
                    return Err(ProtocolError::ConfigInvalid(format!(
                        "reverse server external_bind={external} is non-loopback but \
                         allow_bind is empty; configure an explicit allowlist"
                    )));
                }
            }
        }
        Ok(())
    }
}

fn same_bind(a: &SocketAddr, b: &SocketAddr) -> bool {
    a.port() == b.port()
        && match (a.ip(), b.ip()) {
            (IpAddr::V4(a4), IpAddr::V4(b4)) => a4 == b4,
            (IpAddr::V6(a6), IpAddr::V6(b6)) => a6 == b6,
            _ => false,
        }
}

/// Active state of the reverse server, exposed for tests and admin hooks.
#[derive(Debug, Default)]
pub struct ReverseServerState {
    /// Number of currently-active (accepted, awaiting use) control connections.
    pub active_control: AtomicU32,
    /// Number of currently-active external streams being relayed.
    pub active_streams: AtomicU32,
    /// Number of listeners denied because of allow_bind.
    pub denied_bind: AtomicU32,
    /// Number of streams dropped because max_streams_per_listener was reached.
    pub dropped_stream_limit: AtomicU32,
}

impl ReverseServerState {
    /// Snapshot of the counters for admin/log display.
    pub fn snapshot(&self) -> ReverseServerStateSnapshot {
        ReverseServerStateSnapshot {
            active_control: self.active_control.load(Ordering::Relaxed),
            active_streams: self.active_streams.load(Ordering::Relaxed),
            denied_bind: self.denied_bind.load(Ordering::Relaxed),
            dropped_stream_limit: self.dropped_stream_limit.load(Ordering::Relaxed),
        }
    }
}

/// Plain-data snapshot of [`ReverseServerState`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReverseServerStateSnapshot {
    pub active_control: u32,
    pub active_streams: u32,
    pub denied_bind: u32,
    pub dropped_stream_limit: u32,
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
    state: Arc<ReverseServerState>,
}

impl ReverseServer {
    pub fn new(config: ReverseServerConfig) -> Self {
        Self {
            config,
            cancel: CancellationToken::new(),
            metrics: None,
            state: Arc::new(ReverseServerState::default()),
        }
    }

    /// Attach metrics to this server instance.
    pub fn set_metrics(&mut self, metrics: Arc<ReverseMetrics>) {
        self.metrics = Some(metrics);
    }

    /// Get a handle to the active server state.
    pub fn state_handle(&self) -> Arc<ReverseServerState> {
        self.state.clone()
    }

    /// Get a cancel token for external shutdown.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Validate the configured bind address against `allow_bind` before
    /// binding. Returns the resolved listener or an error.
    async fn bind_external_listener(
        config: &ReverseServerConfig,
        state: &ReverseServerState,
    ) -> Result<Option<TcpListener>, ProtocolError> {
        let external_bind = match config.external_bind {
            Some(addr) => addr,
            None => return Ok(None),
        };
        if !config.is_bind_allowed(external_bind) {
            state.denied_bind.fetch_add(1, Ordering::Relaxed);
            return Err(ProtocolError::BindDenied(external_bind));
        }
        let listener = TcpListener::bind(external_bind).await?;
        let addr = listener.local_addr()?;
        info!(addr = %addr, "reverse server listening for external clients");
        Ok(Some(listener))
    }

    /// Start the reverse server.
    pub async fn run(self) -> Result<(), ProtocolError> {
        // Defense-in-depth validation: catch unsafe configurations (e.g.
        // non-loopback external_bind without auth or allow_bind allowlist)
        // before binding any sockets.
        self.config.validate()?;

        // Enforce the allow_bind policy up-front so misconfiguration is loud.
        if let Some(external_bind) = self.config.external_bind {
            if !self.config.is_bind_allowed(external_bind) {
                self.state.denied_bind.fetch_add(1, Ordering::Relaxed);
                return Err(ProtocolError::BindDenied(external_bind));
            }
        }

        let control_listener = TcpListener::bind(&self.config.control_bind).await?;
        let control_addr = control_listener.local_addr()?;
        info!(addr = %control_addr, "reverse server listening for control connections");

        let external_listener = Self::bind_external_listener(&self.config, &self.state).await?;

        let config = Arc::new(self.config);
        let cancel = self.cancel.clone();
        let state = self.state.clone();
        let metrics = self.metrics.clone();

        // Channel for available control connections
        let (control_tx, control_rx) = mpsc::unbounded_channel::<ControlStream>();

        // Spawn control connection acceptor
        let config_clone = config.clone();
        let cancel_clone = cancel.clone();
        let control_tx_clone = control_tx.clone();
        let metrics_clone = metrics.clone();
        let state_clone = state.clone();
        tokio::spawn(async move {
            Self::accept_control_connections(
                control_listener,
                config_clone,
                cancel_clone,
                control_tx_clone,
                metrics_clone,
                state_clone,
            )
            .await;
        });

        // Spawn external client acceptor
        if let Some(external_listener) = external_listener {
            let config_clone = config.clone();
            let cancel_clone = cancel.clone();
            let metrics_clone = metrics.clone();
            let state_clone = state.clone();
            tokio::spawn(async move {
                Self::accept_external_clients(
                    external_listener,
                    config_clone,
                    cancel_clone,
                    control_rx,
                    metrics_clone,
                    state_clone,
                )
                .await;
            });
        } else {
            // No external listener: drain the control channel so the
            // counter accurately reflects connections that have not yet
            // been paired with an external client. Each received stream
            // is closed and the active_control counter is decremented.
            let state_clone = state.clone();
            tokio::spawn(async move {
                let mut control_rx = control_rx;
                while let Some(ctrl) = control_rx.recv().await {
                    debug!(
                        control_peer = %ctrl.peer_addr,
                        "dropping control connection: no external listener"
                    );
                    drop(ctrl.stream);
                    state_clone.active_control.fetch_sub(1, Ordering::Relaxed);
                }
            });
        }

        // Wait for shutdown
        cancel.cancelled().await;
        let drain_start = Instant::now();
        info!("reverse server shutting down, draining active streams");

        // Wait briefly for active streams to finish. We do not have a direct
        // count of in-flight relay tasks, but a short bounded sleep gives them
        // a chance to exit cleanly.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let drain_ms = drain_start.elapsed().as_millis() as u64;
        if let Some(ref m) = metrics {
            m.record_drain(drain_ms);
        }
        info!(drain_ms, "reverse server drain complete");
        Ok(())
    }

    /// Accept control connections, authenticate, and add to available pool.
    async fn accept_control_connections(
        listener: TcpListener,
        config: Arc<ReverseServerConfig>,
        cancel: CancellationToken,
        control_tx: mpsc::UnboundedSender<ControlStream>,
        metrics: Option<Arc<ReverseMetrics>>,
        state: Arc<ReverseServerState>,
    ) {
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, peer_addr)) => {
                            // Enforce the per-server control connection cap.
                            if state.active_control.load(Ordering::Relaxed)
                                >= config.max_control_connections
                            {
                                warn!(
                                    peer = %peer_addr,
                                    max = config.max_control_connections,
                                    "rejecting control connection: max reached"
                                );
                                if let Some(ref m) = metrics {
                                    m.record_control_rejected(peer_addr, "max_control_connections");
                                }
                                drop(stream);
                                continue;
                            }

                            let config = config.clone();
                            let control_tx = control_tx.clone();
                            let metrics = metrics.clone();
                            let state = state.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_control_connection(
                                    stream,
                                    peer_addr,
                                    config,
                                    control_tx,
                                    metrics.as_deref(),
                                    state,
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
        control_tx: mpsc::UnboundedSender<ControlStream>,
        metrics: Option<&ReverseMetrics>,
        state: Arc<ReverseServerState>,
    ) -> Result<(), ProtocolError> {
        info!(peer = %peer_addr, state = ?ControlState::Connecting, "new control connection");

        // Authenticate if configured
        let redacted = if config.auth_username.is_some() || config.auth_password.is_some() {
            let authenticating_start = Instant::now();
            let result = server_auth_handshake(
                &mut stream,
                config.auth_username.as_deref(),
                config.auth_password.as_deref(),
            )
            .await;
            let elapsed = authenticating_start.elapsed().as_millis() as u64;

            match result {
                Ok(redacted) => {
                    info!(
                        peer = %peer_addr,
                        auth = %redacted,
                        duration_ms = elapsed,
                        state = ?ControlState::Authenticating,
                        "control connection authenticated"
                    );
                    if let Some(m) = metrics {
                        m.record_control_accepted(peer_addr);
                        m.record_state_duration(ControlState::Authenticating, elapsed);
                    }
                    Some(redacted)
                }
                Err(e) => {
                    warn!(
                        peer = %peer_addr,
                        error = %e,
                        duration_ms = elapsed,
                        state = ?ControlState::Authenticating,
                        "control connection auth failed"
                    );
                    if let Some(m) = metrics {
                        m.record_control_rejected(peer_addr, &e.to_string());
                    }
                    return Err(e);
                }
            }
        } else {
            // No auth configured: send accept handshake
            crate::write_handshake_accept(&mut stream).await?;
            info!(
                peer = %peer_addr,
                state = ?ControlState::Authenticating,
                "control connection accepted (no auth)"
            );
            if let Some(m) = metrics {
                m.record_control_accepted(peer_addr);
            }
            None
        };

        // pproxy model: 1 control connection == 1 external listener. We enforce
        // a configurable cap (default 1) to keep the model but allow operators
        // to relax it.
        let listener_budget = config.max_listeners_per_client.max(1);
        if state.active_control.load(Ordering::Relaxed) > listener_budget {
            warn!(
                peer = %peer_addr,
                active = state.active_control.load(Ordering::Relaxed),
                max_listeners_per_client = listener_budget,
                "exceeded max_listeners_per_client, allowing but logging"
            );
        }

        state.active_control.fetch_add(1, Ordering::Relaxed);

        let ctrl = ControlStream {
            stream,
            peer_addr,
            redacted_auth: redacted,
        };
        if control_tx.send(ctrl).is_err() {
            state.active_control.fetch_sub(1, Ordering::Relaxed);
            warn!(peer = %peer_addr, "control channel closed, cannot add to pool");
        }

        Ok(())
    }

    /// Accept external clients and relay them through available control connections.
    async fn accept_external_clients(
        listener: TcpListener,
        config: Arc<ReverseServerConfig>,
        cancel: CancellationToken,
        mut control_rx: mpsc::UnboundedReceiver<ControlStream>,
        metrics: Option<Arc<ReverseMetrics>>,
        state: Arc<ReverseServerState>,
    ) {
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((external_stream, peer_addr)) => {
                            if state.active_streams.load(Ordering::Relaxed)
                                >= config.max_streams_per_listener
                            {
                                warn!(
                                    peer = %peer_addr,
                                    active = state.active_streams.load(Ordering::Relaxed),
                                    max = config.max_streams_per_listener,
                                    "dropping external client: max_streams_per_listener reached"
                                );
                                state.dropped_stream_limit.fetch_add(1, Ordering::Relaxed);
                                drop(external_stream);
                                continue;
                            }
                            // Get an available control connection
                            match control_rx.recv().await {
                                Some(control) => {
                                    let metrics = metrics.clone();
                                    let state = state.clone();
                                    state.active_streams.fetch_add(1, Ordering::Relaxed);
                                    state.active_control.fetch_sub(1, Ordering::Relaxed);
                                    tokio::spawn(async move {
                                        info!(
                                            peer = %peer_addr,
                                            control_peer = %control.peer_addr,
                                            "relaying external client through control connection"
                                        );
                                        if let Some(m) = metrics.as_deref() {
                                            m.record_stream_opened();
                                            m.record_state_duration(ControlState::Ready, 0);
                                        }
                                        let relay_result = relay_bidirectional(
                                            external_stream,
                                            control.stream,
                                        ).await;
                                        match relay_result {
                                            Ok(()) => {
                                                debug!(peer = %peer_addr, "relay finished cleanly");
                                            }
                                            Err(e) => {
                                                debug!(peer = %peer_addr, error = %e, "relay ended");
                                            }
                                        }
                                        if let Some(m) = metrics.as_deref() {
                                            m.record_stream_closed(0);
                                        }
                                        state.active_streams.fetch_sub(1, Ordering::Relaxed);
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

/// A control stream paired with metadata, used when handing the stream off
/// from the auth phase to the relay phase.
pub struct ControlStream {
    pub stream: TcpStream,
    pub peer_addr: SocketAddr,
    pub redacted_auth: Option<String>,
}

/// Helper that exposes the redacted auth form for tests and admin code.
pub fn format_auth_redacted(auth: &str) -> String {
    redact_auth(auth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_bind_allowed_with_none() {
        let cfg = ReverseServerConfig {
            allow_bind: None,
            ..Default::default()
        };
        assert!(cfg.is_bind_allowed("127.0.0.1:8080".parse().unwrap()));
    }

    #[test]
    fn is_bind_allowed_with_empty() {
        let cfg = ReverseServerConfig {
            allow_bind: Some(vec![]),
            ..Default::default()
        };
        assert!(cfg.is_bind_allowed("127.0.0.1:8080".parse().unwrap()));
    }

    #[test]
    fn is_bind_allowed_match() {
        let cfg = ReverseServerConfig {
            allow_bind: Some(vec!["127.0.0.1:8080".parse().unwrap()]),
            ..Default::default()
        };
        assert!(cfg.is_bind_allowed("127.0.0.1:8080".parse().unwrap()));
    }

    #[test]
    fn is_bind_allowed_mismatch() {
        let cfg = ReverseServerConfig {
            allow_bind: Some(vec!["127.0.0.1:8080".parse().unwrap()]),
            ..Default::default()
        };
        assert!(!cfg.is_bind_allowed("0.0.0.0:8080".parse().unwrap()));
        assert!(!cfg.is_bind_allowed("127.0.0.1:9090".parse().unwrap()));
    }

    #[test]
    fn state_snapshot_round_trip() {
        let s = ReverseServerState::default();
        s.active_control.fetch_add(3, Ordering::Relaxed);
        s.active_streams.fetch_add(2, Ordering::Relaxed);
        s.denied_bind.fetch_add(1, Ordering::Relaxed);
        s.dropped_stream_limit.fetch_add(4, Ordering::Relaxed);
        let snap = s.snapshot();
        assert_eq!(snap.active_control, 3);
        assert_eq!(snap.active_streams, 2);
        assert_eq!(snap.denied_bind, 1);
        assert_eq!(snap.dropped_stream_limit, 4);
    }

    #[test]
    fn format_auth_redacted_basic() {
        assert_eq!(format_auth_redacted("user:pass"), "user:****");
    }

    #[test]
    fn same_bind_v4() {
        let a: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let b: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        assert!(same_bind(&a, &b));
    }

    #[test]
    fn same_bind_different_port() {
        let a: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let b: SocketAddr = "127.0.0.1:9090".parse().unwrap();
        assert!(!same_bind(&a, &b));
    }

    #[test]
    fn validate_loopback_ok() {
        let cfg = ReverseServerConfig {
            control_bind: "127.0.0.1:0".parse().unwrap(),
            external_bind: Some("127.0.0.1:0".parse().unwrap()),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_no_external_bind_ok() {
        let cfg = ReverseServerConfig {
            control_bind: "127.0.0.1:0".parse().unwrap(),
            external_bind: None,
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_non_loopback_without_auth_rejected() {
        let cfg = ReverseServerConfig {
            control_bind: "127.0.0.1:0".parse().unwrap(),
            external_bind: Some("0.0.0.0:9000".parse().unwrap()),
            auth_username: None,
            auth_password: None,
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(
            matches!(err, ProtocolError::ConfigInvalid(_)),
            "got: {err:?}"
        );
    }

    #[test]
    fn validate_non_loopback_with_auth_but_no_allowlist_rejected() {
        let cfg = ReverseServerConfig {
            control_bind: "127.0.0.1:0".parse().unwrap(),
            external_bind: Some("0.0.0.0:9000".parse().unwrap()),
            auth_username: Some("user".to_string()),
            auth_password: Some("pass".to_string()),
            allow_bind: None,
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(
            matches!(err, ProtocolError::ConfigInvalid(_)),
            "got: {err:?}"
        );
    }

    #[test]
    fn validate_non_loopback_with_auth_and_allowlist_ok() {
        let cfg = ReverseServerConfig {
            control_bind: "127.0.0.1:0".parse().unwrap(),
            external_bind: Some("0.0.0.0:9000".parse().unwrap()),
            auth_username: Some("user".to_string()),
            auth_password: Some("pass".to_string()),
            allow_bind: Some(vec!["0.0.0.0:9000".parse().unwrap()]),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_ipv6_loopback_ok() {
        let cfg = ReverseServerConfig {
            control_bind: "127.0.0.1:0".parse().unwrap(),
            external_bind: Some("[::1]:9000".parse().unwrap()),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_ipv6_non_loopback_without_auth_rejected() {
        let cfg = ReverseServerConfig {
            control_bind: "127.0.0.1:0".parse().unwrap(),
            external_bind: Some("[2001:db8::1]:9000".parse().unwrap()),
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(
            matches!(err, ProtocolError::ConfigInvalid(_)),
            "got: {err:?}"
        );
    }
}
