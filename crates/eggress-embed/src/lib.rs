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

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

pub use error::EggressError;

/// Parsed and validated eggress configuration.
///
/// Construct via [`EggressConfig::from_toml_str`] or [`EggressConfig::from_toml_file`].
#[derive(Clone)]
pub struct EggressConfig {
    source_toml: String,
}

impl EggressConfig {
    /// Parse a TOML configuration string.
    pub fn from_toml_str(input: &str) -> Result<Self, EggressError> {
        let config: eggress_config::model::ConfigFile =
            toml::from_str(input).map_err(|e| EggressError::Config(e.to_string()))?;

        if let Some(version) = config.version {
            if version != 1 {
                return Err(EggressError::Config(format!(
                    "unsupported config version: {version}"
                )));
            }
        }

        eggress_config::validate::validate_config(&config).map_err(|errors| {
            let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            EggressError::Config(messages.join("; "))
        })?;

        let _inner = eggress_config::compile::compile_config(&config)
            .map_err(|e| EggressError::Config(e.to_string()))?;

        Ok(Self {
            source_toml: input.to_string(),
        })
    }

    /// Load and validate a TOML configuration file.
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, EggressError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .map_err(|e| EggressError::Config(format!("failed to read {path:?}: {e}")))?;
        Self::from_toml_str(&contents)
    }

    /// Return the original TOML source text.
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
    pub async fn start(self) -> Result<EggressHandle, EggressError> {
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let config_path = write_temp_config(&self.config)?;
        let config_path_clone = config_path.clone();

        let join = tokio::task::spawn_blocking(move || -> Result<
            (
                Arc<eggress_runtime::RuntimeState>,
                tokio_util::sync::CancellationToken,
            ),
            EggressError,
        > {
            let mut sup = eggress_runtime::ServiceSupervisor::start(&config_path_clone)
                .map_err(|e| EggressError::Startup(e.to_string()))?;

            let state = sup.state().clone();
            let token = sup.shutdown_token();

            let run_result = std::thread::Builder::new()
                .name("eggress-embed-rt".into())
                .spawn(move || sup.run())
                .map_err(|e| EggressError::Startup(e.to_string()))?;

            // Wait for readiness or failure
            let started = std::time::Instant::now();
            let timeout = Duration::from_secs(30);
            loop {
                if state.readiness.load(Ordering::Acquire) {
                    let _ = ready_tx.send(Ok((state.clone(), token.clone())));
                    break;
                }
                if started.elapsed() > timeout {
                    token.cancel();
                    let _ = ready_tx.send(Err(EggressError::Startup(
                        "readiness timeout".to_string(),
                    )));
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }

            // Wait for the run thread to finish (shutdown)
            match run_result.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::debug!(%e, "runtime exited with error"),
                Err(_) => tracing::debug!("runtime thread panicked"),
            }

            // Clean up temp config file
            let _ = std::fs::remove_file(&config_path_clone);

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
            _config_path: Some(config_path),
            _runtime_task: Some(join),
            reload_mutex: std::sync::Mutex::new(()),
        })
    }

    /// Start the service with a dedicated runtime thread (blocking).
    ///
    /// This spawns a background thread that creates a Tokio runtime and runs
    /// the proxy. Blocks until readiness is achieved or startup fails.
    /// Returns a handle that owns the runtime thread.
    pub fn start_blocking(self) -> Result<EggressHandle, EggressError> {
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let config_path = write_temp_config(&self.config)?;
        let config_path_clone = config_path.clone();

        let _thread_handle = std::thread::Builder::new()
            .name("eggress-embed-rt".into())
            .spawn(move || {
                let mut sup = match eggress_runtime::ServiceSupervisor::start(&config_path_clone) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(EggressError::Startup(e.to_string())));
                        return;
                    }
                };

                let state = sup.state().clone();
                let token = sup.shutdown_token();

                let run_handle = std::thread::Builder::new()
                    .name("eggress-embed-run".into())
                    .spawn(move || {
                        if let Err(e) = sup.run() {
                            tracing::error!("supervisor exited with error: {e}");
                        }
                    });

                let run_handle = match run_handle {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = ready_tx.send(Err(EggressError::Startup(e.to_string())));
                        return;
                    }
                };

                // Wait for readiness
                let started = std::time::Instant::now();
                let timeout = Duration::from_secs(30);
                loop {
                    if state.readiness.load(Ordering::Acquire) {
                        let _ = ready_tx.send(Ok((state, token, run_handle, config_path_clone)));
                        break;
                    }
                    if started.elapsed() > timeout {
                        // On timeout, cancel and clean up immediately
                        token.cancel();
                        match run_handle.join() {
                            Ok(()) => {}
                            Err(_) => tracing::debug!("runtime thread panicked"),
                        }
                        let _ = std::fs::remove_file(&config_path_clone);
                        let _ =
                            ready_tx.send(Err(EggressError::Startup("readiness timeout".into())));
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
            .map_err(|e| EggressError::Startup(e.to_string()))?;

        let (state, token, run_handle, config_path) = ready_rx
            .recv()
            .map_err(|_| EggressError::Startup("startup channel dropped".into()))??;

        Ok(EggressHandle {
            state,
            token: Some(token),
            _run_handle: Some(run_handle),
            _config_path: Some(config_path),
            _runtime_task: None,
            reload_mutex: std::sync::Mutex::new(()),
        })
    }
}

/// Handle to a running eggress service.
///
/// Provides access to bound addresses, status, metrics, reload, and shutdown.
/// Dropping the handle cancels the shutdown token, initiating graceful shutdown.
///
/// # Thread ownership
///
/// The handle owns exactly one of two mutually exclusive thread models:
///
/// **Async path** (`start()`):
/// - A Tokio blocking-pool thread runs the startup sequence and then blocks on
///   `run_result.join()` for the lifetime of the service.
/// - A dedicated OS thread (`"eggress-embed-rt"`) owns `ServiceSupervisor::run()`.
/// - `_runtime_task` wraps the blocking task's JoinHandle as a Tokio task.
///
/// **Blocking path** (`start_blocking()`):
/// - An outer OS thread (`"eggress-embed-rt"`) handles startup, sends results
///   through a channel, and terminates.
/// - An inner OS thread (`"eggress-embed-run"`) owns `ServiceSupervisor::run()`.
/// - `_run_handle` holds the inner thread's JoinHandle directly.
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
        Ok(self.state.metrics.render_prometheus())
    }

    /// Reload configuration from a TOML string.
    ///
    /// Returns the outcome of the reload attempt. On success, the generation
    /// is incremented. On rejection, the old configuration remains active.
    pub fn reload_toml_str(&self, input: &str) -> Result<ReloadOutcome, EggressError> {
        let _guard = self
            .reload_mutex
            .lock()
            .map_err(|_| EggressError::Reload("concurrent reload in progress".to_string()))?;

        // Parse and validate the new config
        let config: eggress_config::model::ConfigFile =
            toml::from_str(input).map_err(|e| EggressError::Reload(e.to_string()))?;

        if let Some(version) = config.version {
            if version != 1 {
                return Err(EggressError::Reload(format!(
                    "unsupported config version: {version}"
                )));
            }
        }

        eggress_config::validate::validate_config(&config).map_err(|errors| {
            let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            EggressError::Reload(messages.join("; "))
        })?;

        let new_rt_config = eggress_config::compile::compile_config(&config)
            .map_err(|e| EggressError::Reload(e.to_string()))?;

        // Classify reload
        let prev_snapshot = self.state.snapshot.load();
        let old_listeners = &prev_snapshot.listeners;
        let new_listeners = &new_rt_config.listeners;

        if old_listeners.len() != new_listeners.len() {
            return Err(EggressError::Reload(format!(
                "listener count changed ({} -> {}); restart required",
                old_listeners.len(),
                new_listeners.len()
            )));
        }

        for (old, new) in old_listeners.iter().zip(new_listeners.iter()) {
            if old.name != new.name {
                return Err(EggressError::Reload(format!(
                    "listener name changed ('{}' -> '{}'); restart required",
                    old.name, new.name
                )));
            }
            if old.bind != new.bind {
                return Err(EggressError::Reload(format!(
                    "listener bind changed for '{}'; restart required",
                    old.name
                )));
            }
        }

        let prev_ref: Option<&eggress_runtime::CompiledRuntimeSnapshot> = Some(&prev_snapshot);
        let new_snapshot =
            eggress_runtime::snapshot::compile_runtime_snapshot(&new_rt_config, prev_ref)
                .map_err(|e| EggressError::Reload(format!("snapshot build: {e}")))?;

        let gen = new_snapshot.generation;
        let upstreams = new_snapshot.upstreams.len();

        // Snapshot must be published before the router swap. Readers that observe
        // the new generation via `snapshot.load()` pull the router from that
        // same snapshot Arc, so any reader seeing the new generation also sees
        // the router that belongs to it.
        let new_snapshot = Arc::new(new_snapshot);
        self.state.snapshot.store(new_snapshot.clone());
        self.state.routing.swap_arc(new_snapshot.router.clone());

        self.state.metrics.set_config_generation(gen);
        self.state.metrics.record_reload(true);

        Ok(ReloadOutcome::Applied {
            generation: gen,
            upstreams,
        })
    }

    /// Reload configuration from a file.
    pub fn reload_toml_file(&self, path: impl AsRef<Path>) -> Result<ReloadOutcome, EggressError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .map_err(|e| EggressError::Reload(format!("failed to read {path:?}: {e}")))?;
        self.reload_toml_str(&contents)
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
        if let Some(path) = self._config_path.take() {
            let _ = std::fs::remove_file(&path);
        }
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
        if let Some(path) = self._config_path.take() {
            let _ = std::fs::remove_file(&path);
        }
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
        if let Some(path) = self._config_path.take() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn listener_addr_or_configured(
    bound_addrs: &[SocketAddr],
    idx: usize,
    configured_bind: &str,
) -> SocketAddr {
    bound_addrs
        .get(idx)
        .copied()
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
/// - Any string that looks like a proxy URI (`scheme://...`) is passed
///   through [`redact_uri`] so `user:pass@` and `user@` authorities are
///   stripped. This covers `upstreams[].uri`, per-hop credentials, PAC
///   fields, and any future field that embeds a proxy URI.
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
        toml::Value::String(s) if looks_like_proxy_uri(s) => {
            *s = redact_uri(s);
        }
        _ => {}
    }
}

/// Heuristic: a string is treated as a proxy URI if it starts with
/// `scheme://` where `scheme` is one of the eggress-supported schemes.
fn looks_like_proxy_uri(s: &str) -> bool {
    let Some(colon) = s.find("://") else {
        return false;
    };
    let scheme = &s[..colon];
    matches!(
        scheme,
        "socks5"
            | "socks4"
            | "http"
            | "https"
            | "ss"
            | "trojan"
            | "h2"
            | "ws"
            | "wss"
            | "raw"
            | "tunnel"
            | "redir"
            | "unix"
    )
}

/// Redact credentials embedded in a proxy URI.
///
/// Transforms `proto://user:pass@host:port` into `proto://****:****@host:port`.
/// Also redacts username-only authorities (`proto://user@host:port`) so that
/// bare usernames never leak into diagnostic or `redacted_*` output.
/// If no `userinfo` is present, the URI is returned unchanged.
fn redact_uri(uri: &str) -> String {
    if let Some(scheme_end) = uri.find("://") {
        let rest = &uri[scheme_end + 3..];
        if let Some(at_pos) = rest.find('@') {
            // Any authority between `://` and `@` is treated as credentials
            // and replaced with `****:****`. This covers both `user:pass@` and
            // username-only `user@` forms, plus any other characters that
            // appeared between them.
            let authority_after = &rest[at_pos + 1..];
            return format!("{}://****:****@{}", &uri[..scheme_end], authority_after);
        }
    }
    uri.to_string()
}

/// Write config to a temporary file for the supervisor.
fn write_temp_config(config: &EggressConfig) -> Result<String, EggressError> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let file_name = format!("eggress-embed-{}-{id}.toml", std::process::id());
    let path = dir.join(&file_name);
    std::fs::write(&path, &config.source_toml)
        .map_err(|e| EggressError::Config(format!("failed to write temp config: {e}")))?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::{default_listener_addr, listener_addr_or_configured};

    #[test]
    fn listener_addr_prefers_bound_address() {
        let bound: SocketAddr = "127.0.0.1:1234".parse().unwrap();

        assert_eq!(
            listener_addr_or_configured(&[bound], 0, "127.0.0.1:5678"),
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
}
