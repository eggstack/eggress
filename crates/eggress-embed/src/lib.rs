//! # eggress-embed
//!
//! Stable Rust embed API for starting and controlling an eggress proxy in-process.
//!
//! This crate wraps the internal runtime, config, and server infrastructure behind
//! a minimal, binding-friendly surface. Python bindings (PyO3) in later phases will
//! wrap this API.
//!
//! ## Quick start (blocking)
//!
//! ```no_run
//! use eggress_embed::{EggressService, EggressConfig};
//!
//! let config = EggressConfig::from_toml_str(r#"
//!     version = 1
//!     [[listeners]]
//!     name = "socks"
//!     bind = "127.0.0.1:0"
//!     protocols = ["socks5"]
//! "#).unwrap();
//!
//! let handle = EggressService::new(config).start_blocking().unwrap();
//! let addrs = handle.bound_addresses();
//! println!("listening on {:?}", addrs);
//! handle.shutdown_blocking().unwrap();
//! ```
//!
//! ## Quick start (async)
//!
//! ```no_run
//! # tokio_test::block_on(async {
//! use eggress_embed::{EggressService, EggressConfig};
//!
//! let config = EggressConfig::from_toml_str(r#"
//!     version = 1
//!     [[listeners]]
//!     name = "http"
//!     bind = "127.0.0.1:0"
//!     protocols = ["http"]
//! "#).unwrap();
//!
//! let handle = EggressService::new(config).start().await.unwrap();
//! let status = handle.status();
//! println!("generation: {}", status.generation);
//! handle.shutdown().await.unwrap();
//! # });
//! ```

mod error;
pub mod outbound;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

pub use error::EggressError;

/// Parsed and validated eggress configuration.
///
/// Construct via [`EggressConfig::from_toml_str`] or [`EggressConfig::from_toml_file`].
///
/// The validated compiled [`eggress_config::compile::RuntimeConfig`] is the
/// canonical startup handoff; the retained TOML source is ancillary state kept
/// only for round-trip display (`source_toml`/`to_redacted_toml`) and must not
/// be required to start the service.
#[derive(Clone)]
pub struct EggressConfig {
    compiled: eggress_config::compile::RuntimeConfig,
    source_toml: String,
}

/// Parse, version-check, validate, and compile TOML exactly once.
///
/// Single shared boundary for all TOML-string entry points
/// (`EggressConfig::from_toml_str`, `OutboundConnector::from_toml` /
/// `validate_outbound_config`, `EggressHandle::reload_toml_str`). Callers map
/// the message-only error to their own `EggressError` variant
/// (`Config` vs `Reload`) so reload failures still record metrics.
pub(crate) fn parse_validate_compile(
    input: &str,
) -> Result<eggress_config::compile::RuntimeConfig, String> {
    let config: eggress_config::model::ConfigFile =
        toml::from_str(input).map_err(|e| e.to_string())?;

    if let Some(version) = config.version {
        if version != 1 {
            return Err(format!("unsupported config version: {version}"));
        }
    }

    eggress_config::validate::validate_config(&config).map_err(|errors| {
        let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        messages.join("; ")
    })?;

    eggress_config::compile::compile_config(&config).map_err(|e| e.to_string())
}

impl EggressConfig {
    /// Parse a TOML configuration string.
    ///
    /// Validation and compilation happen exactly once via the shared
    /// [`parse_validate_compile`] boundary; the resulting compiled runtime
    /// configuration is stored for in-memory supervisor startup with no
    /// filesystem round trip.
    pub fn from_toml_str(input: &str) -> Result<Self, EggressError> {
        let compiled = parse_validate_compile(input).map_err(EggressError::Config)?;

        Ok(Self {
            compiled,
            source_toml: input.to_string(),
        })
    }

    /// Construct from an already-compiled runtime configuration.
    ///
    /// `source_toml` is ancillary display state (may be empty when the config
    /// originated natively, e.g. pproxy direct compilation). Startup uses
    /// only `compiled`.
    pub fn from_compiled(
        compiled: eggress_config::compile::RuntimeConfig,
        source_toml: String,
    ) -> Self {
        Self {
            compiled,
            source_toml,
        }
    }

    /// Borrow the canonical compiled runtime configuration.
    pub fn compiled(&self) -> &eggress_config::compile::RuntimeConfig {
        &self.compiled
    }

    /// Consume into the canonical compiled runtime configuration.
    pub fn into_compiled(self) -> eggress_config::compile::RuntimeConfig {
        self.compiled
    }

    /// Load and validate a TOML configuration file.
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, EggressError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .map_err(|e| EggressError::Config(format!("failed to read {path:?}: {e}")))?;
        Self::from_toml_str(&contents)
    }

    /// Return the original TOML source text.
    ///
    /// For configs built natively via [`EggressConfig::from_compiled`] this
    /// may be empty; startup never depends on it.
    pub fn source_toml(&self) -> &str {
        &self.source_toml
    }

    /// Return the TOML source with credentials redacted.
    ///
    /// Listener auth passwords and upstream URI credentials are replaced with
    /// `****` / `****:****@` placeholders. The result is suitable for logging
    /// or display without leaking secrets.
    pub fn to_redacted_toml(&self) -> Result<String, EggressError> {
        let mut value: toml::Value =
            toml::from_str(&self.source_toml).map_err(|e| EggressError::Config(e.to_string()))?;

        redact_toml_value(&mut value);

        toml::to_string_pretty(&value).map_err(|e| EggressError::Internal(e.to_string()))
    }
}

/// Pre-start service builder.
///
/// Created from a validated config. Call [`.start()`](EggressService::start) (async) or
/// [`.start_blocking()`](EggressService::start_blocking) to launch the proxy and obtain a handle.
pub struct EggressService {
    config: EggressConfig,
}

impl EggressService {
    /// Create a new service from a validated config.
    pub fn new(config: EggressConfig) -> Self {
        Self { config }
    }

    /// Convenience: parse TOML and create a service.
    pub fn from_toml_str(input: &str) -> Result<Self, EggressError> {
        EggressConfig::from_toml_str(input).map(Self::new)
    }

    /// Convenience: load file and create a service.
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, EggressError> {
        EggressConfig::from_toml_file(path).map(Self::new)
    }

    /// Start the service using a caller-provided Tokio runtime context.
    ///
    /// The caller must be inside a Tokio runtime. The service binds listeners,
    /// starts health probes, and enters the event loop on a background task.
    /// Returns once readiness is achieved or startup fails.
    ///
    /// Startup consumes the validated in-memory compiled configuration
    /// directly via `ServiceSupervisor::start_from_config`; no temporary
    /// config file round trip is performed.
    pub async fn start(self) -> Result<EggressHandle, EggressError> {
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let rt_config = self.config.into_compiled();

        let join = tokio::task::spawn_blocking(move || -> Result<
            (
                Arc<eggress_runtime::RuntimeState>,
                tokio_util::sync::CancellationToken,
            ),
            EggressError,
        > {
            let (state, token, run_handle) = startup_in_memory(rt_config, None)?;
            let _ = ready_tx.send(Ok((state.clone(), token.clone())));

            // Wait for the run thread to finish (shutdown)
            match run_handle.join() {
                Ok(()) => {}
                Err(_) => tracing::debug!("runtime thread panicked"),
            }

            Ok((state, token))
        });

        let (state, token) = ready_rx
            .await
            .map_err(|_| EggressError::Startup("startup channel dropped".into()))??;

        let join = tokio::task::spawn(async move {
            match join.await {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(e)) => Err(EggressError::Startup(format!("startup failed: {e}"))),
                Err(e) => Err(EggressError::Startup(format!("startup task panicked: {e}"))),
            }
        });

        Ok(EggressHandle {
            state,
            token: Some(token),
            _run_handle: None,
            _config_path: None,
            _runtime_task: Some(join),
            reload_mutex: std::sync::Mutex::new(()),
        })
    }

    /// Start the service with a dedicated runtime thread (blocking).
    ///
    /// This spawns the supervisor run loop on a background thread and blocks
    /// until readiness is achieved or startup fails. Returns a handle that
    /// owns the runtime thread.
    ///
    /// Startup consumes the validated in-memory compiled configuration
    /// directly; no temporary config file is written.
    pub fn start_blocking(self) -> Result<EggressHandle, EggressError> {
        let rt_config = self.config.into_compiled();
        let (state, token, run_handle) = startup_in_memory(rt_config, None)?;
        Ok(EggressHandle {
            state,
            token: Some(token),
            _run_handle: Some(run_handle),
            _config_path: None,
            _runtime_task: None,
            reload_mutex: std::sync::Mutex::new(()),
        })
    }

    /// Start a compatibility service from the validated in-memory config.
    ///
    /// This variant is used by the Python `pproxy` entry point so runtime
    /// compatibility hooks such as `--auth` reuse and `--sys` reach the
    /// runtime without going through a temporary config file or the native
    /// defaults. `-d`/`-v` log policy is resolved by the caller via
    /// `default_log_level()`/`init_pproxy_logging` and never enters the
    /// supervisor. Native and compatibility startup share
    /// [`startup_in_memory`]; only the hooks differ (`None` vs `Some`).
    #[cfg(feature = "pproxy-compat")]
    pub fn start_blocking_with_compatibility_options(
        self,
        hooks: eggress_runtime::CompatibilityRuntimeHooks,
    ) -> Result<EggressHandle, EggressError> {
        let rt_config = self.config.into_compiled();
        let (state, token, run_handle) = startup_in_memory(rt_config, Some(hooks))?;
        Ok(EggressHandle {
            state,
            token: Some(token),
            _run_handle: Some(run_handle),
            _config_path: None,
            _runtime_task: None,
            reload_mutex: std::sync::Mutex::new(()),
        })
    }
}

/// Shared in-memory startup used by native and compatibility paths.
///
/// Creates the supervisor from an already-compiled [`eggress_config::compile::RuntimeConfig`]
/// with optional compatibility hooks (`None` for native, `Some` for pproxy
/// compatibility), spawns the blocking `run()` thread, and waits for
/// readiness. No config file is read or written.
fn startup_in_memory(
    rt_config: eggress_config::compile::RuntimeConfig,
    hooks: Option<eggress_runtime::CompatibilityRuntimeHooks>,
) -> Result<
    (
        Arc<eggress_runtime::RuntimeState>,
        tokio_util::sync::CancellationToken,
        std::thread::JoinHandle<()>,
    ),
    EggressError,
> {
    let mut supervisor = match hooks {
        Some(hooks) => eggress_runtime::ServiceSupervisor::start_from_config_with_compatibility(
            rt_config, None, hooks,
        ),
        None => eggress_runtime::ServiceSupervisor::start_from_config(rt_config, None),
    }
    .map_err(|error| EggressError::Startup(error.to_string()))?;

    let state = supervisor.state().clone();
    let token = supervisor.shutdown_token();

    let run_handle = std::thread::Builder::new()
        .name("eggress-embed-run".into())
        .spawn(move || {
            if let Err(error) = supervisor.run() {
                tracing::error!("supervisor exited with error: {error}");
            }
        })
        .map_err(|error| EggressError::Startup(error.to_string()))?;

    let started = std::time::Instant::now();
    let timeout = Duration::from_secs(30);
    loop {
        if state.readiness.load(Ordering::Acquire) {
            return Ok((state, token, run_handle));
        }
        if started.elapsed() > timeout {
            token.cancel();
            let _ = run_handle.join();
            return Err(EggressError::Startup("readiness timeout".to_string()));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Handle to a running eggress service.
///
/// Provides access to bound addresses, status, metrics, reload, and shutdown.
/// Dropping the handle cancels the shutdown token, initiating graceful shutdown.
///
/// # Thread ownership
///
/// The handle owns exactly one of two mutually exclusive thread models.
/// Both paths share [`startup_in_memory`]; only compatibility hooks differ
/// (`None` native vs `Some` compatibility).
///
/// **Async path** (`start()`):
/// - A Tokio blocking-pool thread runs in-memory startup and then blocks on
///   the run thread join for the lifetime of the service.
/// - A dedicated OS thread (`"eggress-embed-run"`) owns `ServiceSupervisor::run()`.
/// - `_runtime_task` wraps the blocking task's JoinHandle as a Tokio task.
///
/// **Blocking path** (`start_blocking()`):
/// - Startup runs in the caller thread; a single OS thread
///   (`"eggress-embed-run"`) owns `ServiceSupervisor::run()`.
/// - `_run_handle` holds that thread's JoinHandle directly.
///
/// No temporary config file is created; the supervisor starts from the
/// in-memory compiled `RuntimeConfig` and SIGHUP reload is disabled
/// (`config_path=None`).
///
/// # Drop behavior
///
/// Dropping the handle cancels the shutdown token and performs a best-effort
/// join: the blocking path joins the run thread directly; the async path
/// creates a throwaway Tokio runtime and awaits the task with a 5-second
/// timeout. Explicit `shutdown()` or `shutdown_blocking()` is preferred to
/// guarantee orderly teardown.
pub struct EggressHandle {
    state: Arc<eggress_runtime::RuntimeState>,
    token: Option<tokio_util::sync::CancellationToken>,
    _run_handle: Option<std::thread::JoinHandle<()>>,
    _config_path: Option<String>,
    _runtime_task: Option<tokio::task::JoinHandle<Result<(), EggressError>>>,
    reload_mutex: std::sync::Mutex<()>,
}

impl EggressHandle {
    /// Get the addresses the service is listening on.
    pub fn bound_addresses(&self) -> BoundAddresses {
        let addrs = self
            .state
            .listener_addrs
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let admin = self
            .state
            .admin_local_addr
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let snap = self.state.snapshot.load();
        let listeners: Vec<ListenerAddress> = snap
            .listeners
            .iter()
            .enumerate()
            .map(|(idx, lcfg)| ListenerAddress {
                name: lcfg.name.clone(),
                addr: listener_addr_or_configured(&addrs, idx, &lcfg.bind),
            })
            .collect();
        BoundAddresses {
            listeners,
            admin: *admin,
        }
    }

    /// Get the current service status.
    pub fn status(&self) -> ServiceStatus {
        let snap = self.state.snapshot.load();
        let addrs = self
            .state
            .listener_addrs
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let listeners: Vec<ListenerStatus> = snap
            .listeners
            .iter()
            .enumerate()
            .map(|(idx, lcfg)| ListenerStatus {
                name: lcfg.name.clone(),
                bind: lcfg.bind.clone(),
                local_addr: listener_addr_or_configured(&addrs, idx, &lcfg.bind),
                protocols: lcfg.protocols.iter().map(|p| format!("{p}")).collect(),
                udp_enabled: lcfg.udp.as_ref().is_some_and(|u| u.enabled),
            })
            .collect();

        let udp_active = self
            .state
            .udp_metrics
            .associations_active
            .load(Ordering::Relaxed);

        ServiceStatus {
            generation: snap.generation,
            readiness: self.state.readiness.load(Ordering::Relaxed),
            active_connections: self.state.active_connections.load(Ordering::Relaxed),
            uptime_secs: self.state.start_time.elapsed().as_secs(),
            listener_count: snap.listeners.len(),
            listeners,
            udp_associations_active: udp_active,
            upstream_count: snap.upstreams.len(),
        }
    }

    /// Render Prometheus metrics text.
    pub fn metrics_text(&self) -> Result<String, EggressError> {
        Ok(self.state.runtime_metrics.render_prometheus())
    }

    /// Reload configuration from a TOML string.
    ///
    /// Returns the outcome of the reload attempt. On success, the generation
    /// is incremented. On rejection, the old configuration remains active.
    ///
    /// File and embed reload share the canonical
    /// [`eggress_runtime::RuntimeState::apply_compiled_config`] transaction;
    /// this entry point differs only in how the new configuration is obtained
    /// (string parse/validate/compile). Metrics, admin publication, health
    /// restart, and pool invalidation are owned by that transaction.
    pub fn reload_toml_str(&self, input: &str) -> Result<ReloadOutcome, EggressError> {
        let _guard = self
            .reload_mutex
            .lock()
            .map_err(|_| EggressError::Reload("concurrent reload in progress".to_string()))?;

        // Parse/validate/compile once via the shared boundary. Failures
        // before the canonical transaction must still record a failed reload
        // so file and embed metrics agree.
        let new_rt_config = parse_validate_compile(input).map_err(|message| {
            self.state.runtime_metrics.record_reload(false);
            EggressError::Reload(message)
        })?;

        // `_guard` is held; call the canonical transaction directly to avoid
        // re-locking `reload_mutex` via `reload_compiled`.
        match self.state.apply_compiled_config(&new_rt_config) {
            eggress_runtime::ReloadResult::Applied {
                generation,
                upstreams,
            } => Ok(ReloadOutcome::Applied {
                generation,
                upstreams,
            }),
            eggress_runtime::ReloadResult::Rejected { reason } => Err(EggressError::Reload(reason)),
            eggress_runtime::ReloadResult::Failed { error } => Err(EggressError::Reload(error)),
        }
    }

    /// Apply an already-compiled runtime configuration.
    ///
    /// Thin wrapper over the canonical reload transaction for native
    /// (non-TOML) producers such as direct pproxy compilation. Parse
    /// failures cannot occur here; classification/snapshot failures are
    /// reported by the canonical transaction and mapped to reload errors.
    pub fn reload_compiled(
        &self,
        new_config: &eggress_config::compile::RuntimeConfig,
    ) -> Result<ReloadOutcome, EggressError> {
        let _guard = self
            .reload_mutex
            .lock()
            .map_err(|_| EggressError::Reload("concurrent reload in progress".to_string()))?;
        match self.state.apply_compiled_config(new_config) {
            eggress_runtime::ReloadResult::Applied {
                generation,
                upstreams,
            } => Ok(ReloadOutcome::Applied {
                generation,
                upstreams,
            }),
            eggress_runtime::ReloadResult::Rejected { reason } => Err(EggressError::Reload(reason)),
            eggress_runtime::ReloadResult::Failed { error } => Err(EggressError::Reload(error)),
        }
    }

    /// Reload configuration from a file.
    pub fn reload_toml_file(&self, path: impl AsRef<Path>) -> Result<ReloadOutcome, EggressError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .map_err(|e| EggressError::Reload(format!("failed to read {path:?}: {e}")))?;
        self.reload_toml_str(&contents)
    }

    /// Cancel the runtime's shutdown token without joining the supervisor.
    ///
    /// Best-effort teardown for finalizers that must not synchronously join
    /// the service thread: listeners and background tasks stop in the
    /// background while the handle itself may be abandoned.
    pub fn cancel(&self) {
        if let Some(token) = self.token.as_ref() {
            token.cancel();
        }
    }

    /// Cancel the runtime without joining it.
    ///
    /// This is intended for finalizers that must abandon the handle after
    /// cancellation. No temporary config file exists (in-memory startup), so
    /// only the shutdown token is cancelled.
    pub fn cancel_and_cleanup(&mut self) {
        self.cancel();
        let _ = self._config_path.take();
    }

    /// Initiate graceful shutdown.
    pub async fn shutdown(mut self) -> Result<(), EggressError> {
        if let Some(token) = self.token.take() {
            token.cancel();
        }
        if let Some(task) = self._runtime_task.take() {
            let _ = task.await;
        }
        if let Some(jh) = self._run_handle.take() {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = jh.join();
            })
            .await;
        }
        let _ = self._config_path.take();
        Ok(())
    }

    /// Initiate graceful shutdown (blocking).
    pub fn shutdown_blocking(mut self) -> Result<(), EggressError> {
        if let Some(token) = self.token.take() {
            token.cancel();
        }
        if let Some(jh) = self._run_handle.take() {
            let _ = jh.join();
        }
        if let Some(task) = self._runtime_task.take() {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| EggressError::Shutdown(e.to_string()))?;
            rt.block_on(async {
                let _ = task.await;
            });
        }
        let _ = self._config_path.take();
        Ok(())
    }
}

impl Drop for EggressHandle {
    /// Cancel the shutdown token and best-effort join the supervisor.
    ///
    /// This is a fallback for callers who do not call `shutdown()` explicitly.
    /// The async path creates a throwaway Tokio runtime to await the task with
    /// a 5-second timeout; if the timeout expires, the task is abandoned.
    /// Prefer explicit `shutdown()` or `shutdown_blocking()` for guaranteed
    /// orderly teardown.
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            token.cancel();
        }
        if let Some(jh) = self._run_handle.take() {
            let _ = jh.join();
        }
        if let Some(task) = self._runtime_task.take() {
            let rt = tokio::runtime::Runtime::new().ok();
            if let Some(rt) = rt {
                rt.block_on(async {
                    let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
                });
            }
        }
        let _ = self._config_path.take();
    }
}

fn listener_addr_or_configured(
    bound_addrs: &[Option<SocketAddr>],
    idx: usize,
    configured_bind: &str,
) -> SocketAddr {
    bound_addrs
        .get(idx)
        .and_then(|a| *a)
        .or_else(|| configured_bind.parse().ok())
        .unwrap_or_else(default_listener_addr)
}

fn default_listener_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
}

/// Addresses the service is listening on.
#[derive(Debug, Clone)]
pub struct BoundAddresses {
    /// TCP listener addresses.
    pub listeners: Vec<ListenerAddress>,
    /// Admin server address (if enabled).
    pub admin: Option<std::net::SocketAddr>,
}

impl BoundAddresses {
    /// Look up a listener by name.
    pub fn listener(&self, name: &str) -> Option<std::net::SocketAddr> {
        self.listeners
            .iter()
            .find(|l| l.name == name)
            .map(|l| l.addr)
    }
}

/// A single listener's bound address.
#[derive(Debug, Clone)]
pub struct ListenerAddress {
    /// Listener name from config.
    pub name: String,
    /// Bound socket address.
    pub addr: std::net::SocketAddr,
}

/// Detailed status of a single listener.
#[derive(Debug, Clone)]
pub struct ListenerStatus {
    /// Listener name from config.
    pub name: String,
    /// Configured bind address.
    pub bind: String,
    /// Actual bound socket address (reflects port-0 resolution).
    pub local_addr: std::net::SocketAddr,
    /// Protocols served by this listener.
    pub protocols: Vec<String>,
    /// Whether UDP relay is enabled on this listener.
    pub udp_enabled: bool,
}

/// Current service status.
#[derive(Debug, Clone)]
pub struct ServiceStatus {
    /// Current configuration generation (increments on reload).
    pub generation: u64,
    /// Whether the service is ready to accept connections.
    pub readiness: bool,
    /// Number of active connections.
    pub active_connections: u64,
    /// Uptime in seconds since the service started.
    pub uptime_secs: u64,
    /// Number of configured listeners.
    pub listener_count: usize,
    /// Detailed status for each listener.
    pub listeners: Vec<ListenerStatus>,
    /// Number of active UDP associations.
    pub udp_associations_active: u64,
    /// Number of configured upstreams.
    pub upstream_count: usize,
}

/// Outcome of a configuration reload attempt.
#[derive(Debug)]
pub enum ReloadOutcome {
    /// Reload was applied successfully.
    Applied {
        /// New generation number.
        generation: u64,
        /// Number of upstreams in the new config.
        upstreams: usize,
    },
}

/// Well-known keys that hold raw secrets and must always be redacted.
const REDACTED_SECRET_KEYS: &[&str] = &[
    "password",
    "password_env",
    "secret",
    "secret_ref",
    "token",
    "api_key",
    "apikey",
    "credentials",
];

/// Redact credential fields in a dynamic TOML value tree.
///
/// Walks the tree generically rather than only enumerating known paths:
/// - Any string whose key matches a known credential-bearing name is
///   replaced with `****`.
/// - Any string containing `://` is passed through the canonical tolerant
///   redactor [`eggress_uri::redact_proxy_uri`], which strips `user:pass@`
///   and `user@` userinfo for any scheme. The scheme check is deliberately
///   generic (presence of `://`) so credential-bearing URIs using schemes
///   added after this code was written cannot leak. Strings without
///   userinfo are returned unchanged.
fn redact_toml_value(value: &mut toml::Value) {
    redact_toml_value_inner(value);
}

fn redact_toml_value_inner(value: &mut toml::Value) {
    match value {
        toml::Value::Table(table) => {
            for (key, val) in table.iter_mut() {
                let lkey = key.to_ascii_lowercase();
                if REDACTED_SECRET_KEYS.iter().any(|k| lkey == *k) {
                    if let toml::Value::String(_) = val {
                        *val = toml::Value::String("****".to_string());
                        continue;
                    }
                }
                redact_toml_value_inner(val);
            }
        }
        toml::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_toml_value_inner(item);
            }
        }
        toml::Value::String(s) if s.contains("://") => {
            *s = eggress_uri::redact_proxy_uri(s);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::{default_listener_addr, listener_addr_or_configured};

    #[test]
    fn listener_addr_prefers_bound_address() {
        let bound: SocketAddr = "127.0.0.1:1234".parse().unwrap();

        assert_eq!(
            listener_addr_or_configured(&[Some(bound)], 0, "127.0.0.1:5678"),
            bound
        );
    }

    #[test]
    fn listener_addr_falls_back_to_configured_bind() {
        let configured: SocketAddr = "127.0.0.1:5678".parse().unwrap();

        assert_eq!(
            listener_addr_or_configured(&[], 0, "127.0.0.1:5678"),
            configured
        );
    }

    #[test]
    fn listener_addr_uses_default_for_invalid_configured_bind() {
        assert_eq!(
            listener_addr_or_configured(&[], 0, "not an address"),
            default_listener_addr()
        );
    }

    fn temp_embed_files() -> Vec<std::path::PathBuf> {
        std::fs::read_dir(std::env::temp_dir())
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with("eggress-embed-"))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn from_toml_str_validates_once_and_compiles_in_memory() {
        let input = r#"
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
        let config = super::EggressConfig::from_toml_str(input).unwrap();
        assert_eq!(config.source_toml(), input);
        assert_eq!(config.compiled().listeners.len(), 1);
        assert_eq!(config.compiled().listeners[0].name, "socks");
    }

    #[test]
    fn blocking_start_succeeds_from_in_memory_config_without_tempfile() {
        let before = temp_embed_files();
        let config = super::EggressConfig::from_toml_str(
            r#"
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#,
        )
        .unwrap();
        let handle = super::EggressService::new(config).start_blocking().unwrap();
        assert!(handle._config_path.is_none());
        let status = handle.status();
        assert_eq!(status.listener_count, 1);
        assert!(!handle.bound_addresses().listeners.is_empty());
        let after = temp_embed_files();
        assert_eq!(
            before, after,
            "in-memory startup must not create eggress-embed-*.toml temp files"
        );
        handle.shutdown_blocking().unwrap();
    }

    #[test]
    fn startup_does_not_require_writable_temp_dir() {
        // Startup must not depend on writing a temp TOML file: even when the
        // temp directory already contains no writable assumption, startup
        // creates no new temp file. This replaces the old owner-only temp
        // file permission test; eliminating the temp file removes plaintext
        // credential persistence entirely.
        let before = temp_embed_files();
        let config = super::EggressConfig::from_toml_str("version = 1").unwrap();
        let handle = super::EggressService::new(config).start_blocking().unwrap();
        assert!(handle._config_path.is_none());
        assert_eq!(before, temp_embed_files());
        handle.shutdown_blocking().unwrap();
    }

    #[test]
    fn cancel_and_cleanup_without_tempfile_is_idempotent() {
        let config = super::EggressConfig::from_toml_str("version = 1").unwrap();
        let mut handle = super::EggressService::new(config).start_blocking().unwrap();
        assert!(handle._config_path.is_none());
        handle.cancel_and_cleanup();
        handle.cancel_and_cleanup();
    }

    #[tokio::test]
    async fn async_start_succeeds_from_in_memory_config() {
        let config = super::EggressConfig::from_toml_str(
            r#"
version = 1

[[listeners]]
name = "http"
bind = "127.0.0.1:0"
protocols = ["http"]
"#,
        )
        .unwrap();
        let handle = super::EggressService::new(config).start().await.unwrap();
        assert_eq!(handle.status().listener_count, 1);
        handle.shutdown().await.unwrap();
    }
}
