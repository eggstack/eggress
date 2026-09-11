use std::time::{Duration, Instant};

// Single exit-code owner: `eggress-pproxy-compat::exit_codes` defines the
// numeric process contract. This crate re-exports it so both binaries and
// every nested command agree on one mapping instead of maintaining
// overlapping constant tables.
#[cfg(feature = "pproxy-compat")]
pub use eggress_pproxy_compat::exit_codes::exit_code_name;
#[cfg(feature = "pproxy-compat")]
pub use eggress_pproxy_compat::exit_codes::{
    ProcessExit, EXIT_BIND_FAILURE, EXIT_CLI_PARSE_ERROR, EXIT_CONFIG_VALIDATION,
    EXIT_EXTERNAL_DEPENDENCY, EXIT_PLATFORM_MISSING, EXIT_RUNTIME_FAILURE, EXIT_SIGINT,
    EXIT_SIGTERM, EXIT_SUCCESS, EXIT_UNSUPPORTED_FEATURE,
};

// Without `pproxy-compat` there is no shared owner available, so the same
// numeric contract is mirrored here. The values must match
// `eggress-pproxy-compat::exit_codes` exactly; `exit_code_mirror_matches_owner`
// (run with the feature enabled) pins that invariant.
#[cfg(not(feature = "pproxy-compat"))]
pub const EXIT_SUCCESS: i32 = 0;
#[cfg(not(feature = "pproxy-compat"))]
pub const EXIT_RUNTIME_FAILURE: i32 = 1;
#[cfg(not(feature = "pproxy-compat"))]
pub const EXIT_CLI_PARSE_ERROR: i32 = 2;
#[cfg(not(feature = "pproxy-compat"))]
pub const EXIT_CONFIG_VALIDATION: i32 = 3;
#[cfg(not(feature = "pproxy-compat"))]
pub const EXIT_BIND_FAILURE: i32 = 4;
#[cfg(not(feature = "pproxy-compat"))]
pub const EXIT_UNSUPPORTED_FEATURE: i32 = 5;
#[cfg(not(feature = "pproxy-compat"))]
pub const EXIT_PLATFORM_MISSING: i32 = 6;
#[cfg(not(feature = "pproxy-compat"))]
pub const EXIT_EXTERNAL_DEPENDENCY: i32 = 7;
#[cfg(not(feature = "pproxy-compat"))]
pub const EXIT_SIGINT: i32 = 130;
#[cfg(not(feature = "pproxy-compat"))]
pub const EXIT_SIGTERM: i32 = 143;

/// Map a runtime startup/serve error to its process exit code.
///
/// Listener and admin bind failures are [`EXIT_BIND_FAILURE`]; all other
/// runtime errors are [`EXIT_RUNTIME_FAILURE`]. Shared by native startup
/// and the compatibility execution facade so every entry point reports
/// bind failures with the documented code instead of a generic runtime
/// failure.
pub fn runtime_error_exit_code(error: &eggress_runtime::RuntimeError) -> i32 {
    match error {
        eggress_runtime::RuntimeError::ListenerBind { .. }
        | eggress_runtime::RuntimeError::AdminBind { .. } => EXIT_BIND_FAILURE,
        _ => EXIT_RUNTIME_FAILURE,
    }
}

/// Apply the optional Linux pproxy daemon transition after compatibility
/// parsing and configuration validation. Re-exec keeps the transition safe
/// under the workspace's `unsafe_code = "deny"` policy and leaves signal,
/// listener, and system-proxy rollback ownership with the child process.
/// Why a `--daemon` transition failed.
#[cfg(feature = "pproxy-daemon")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonizeError {
    /// `--daemon` was requested on a platform without daemon support.
    /// Maps to [`EXIT_PLATFORM_MISSING`].
    UnsupportedPlatform,
    /// The Linux re-exec could not be started. Maps to
    /// [`EXIT_RUNTIME_FAILURE`].
    Spawn(String),
}

#[cfg(feature = "pproxy-daemon")]
impl DaemonizeError {
    /// Numeric exit code for this failure.
    pub fn exit_code(&self) -> i32 {
        match self {
            DaemonizeError::UnsupportedPlatform => EXIT_PLATFORM_MISSING,
            DaemonizeError::Spawn(_) => EXIT_RUNTIME_FAILURE,
        }
    }
}

#[cfg(feature = "pproxy-daemon")]
impl std::fmt::Display for DaemonizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonizeError::UnsupportedPlatform => {
                f.write_str("--daemon compatibility is only available on Linux")
            }
            DaemonizeError::Spawn(reason) => write!(f, "{reason}"),
        }
    }
}

#[cfg(feature = "pproxy-daemon")]
pub fn maybe_daemonize(requested: bool) -> Result<(), DaemonizeError> {
    const CHILD_MARKER: &str = "EGGRESS_PPROXY_DAEMON_CHILD";
    if !requested || std::env::var_os(CHILD_MARKER).is_some() {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let executable = std::env::current_exe().map_err(|error| {
            DaemonizeError::Spawn(format!("cannot resolve executable for --daemon: {error}"))
        })?;
        std::process::Command::new(executable)
            .args(std::env::args_os().skip(1))
            .env(CHILD_MARKER, "1")
            .current_dir("/")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| {
                DaemonizeError::Spawn(format!("cannot start --daemon child: {error}"))
            })?;
        std::process::exit(0);
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(DaemonizeError::UnsupportedPlatform)
    }
}

#[cfg(feature = "pproxy-compat")]
pub mod pproxy_exec;

use eggress_core::chain::ChainExecutor;
use eggress_core::{TargetAddr, TargetHost};

#[derive(serde::Serialize)]
pub struct UpstreamTestResult {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub target: String,
    pub mode: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    pub failure: Option<String>,
    pub failed_hop: Option<usize>,
}

/// Parse the URL-shaped `pproxy --test` value into the target address used by
/// the shared native upstream tester. The regular `eggress upstream test`
/// command continues to accept its existing `host:port` form.
pub fn parse_pproxy_test_target(value: &str) -> Result<TargetAddr, String> {
    if let Ok(target) = value.parse::<TargetAddr>() {
        return Ok(target);
    }

    let uri: http::Uri = value
        .parse()
        .map_err(|e| format!("invalid test URL '{value}': {e}"))?;
    let scheme = uri
        .scheme_str()
        .ok_or_else(|| format!("invalid test URL '{value}': missing scheme"))?;
    if !matches!(scheme, "http" | "https") {
        return Err(format!(
            "invalid test URL '{value}': unsupported scheme '{scheme}'"
        ));
    }
    let authority = uri
        .authority()
        .ok_or_else(|| format!("invalid test URL '{value}': missing host"))?;
    let host = authority.host();
    if host.is_empty() {
        return Err(format!("invalid test URL '{value}': missing host"));
    }
    let port = authority
        .port_u16()
        .unwrap_or_else(|| if scheme == "https" { 443 } else { 80 });
    let host = if host.contains(':') {
        TargetHost::Ip(
            host.parse()
                .map_err(|e| format!("invalid test URL '{value}': invalid IPv6 host: {e}"))?,
        )
    } else if let Ok(ip) = host.parse() {
        TargetHost::Ip(ip)
    } else {
        TargetHost::Domain(host.to_string())
    };
    Ok(TargetAddr { host, port })
}

/// Run upstream tests against a compiled config and return the exit code.
///
/// This is the shared implementation used by both `eggress upstream test`
/// and `pproxy --test`. It accepts typed inputs, not CLI argv.
pub fn run_upstream_test(
    rt: &eggress_config::compile::RuntimeConfig,
    target: Option<&str>,
    timeout: Duration,
    json_output: bool,
) -> i32 {
    run_upstream_test_with_mode(rt, target, "proxy", timeout, json_output)
}

/// Run upstream tests with an explicit mode ("proxy" or "tcp").
///
/// Invalid modes fail closed with [`EXIT_CLI_PARSE_ERROR`] instead of
/// silently falling back to either probe path. Normal callers pass a value
/// from the typed `UpstreamTestMode` CLI enum (`proxy`/`tcp`), so this is
/// defense-in-depth at the library boundary (including programmatic and
/// Python-facing callers).
pub fn run_upstream_test_with_mode(
    rt: &eggress_config::compile::RuntimeConfig,
    target: Option<&str>,
    mode: &str,
    timeout: Duration,
    json_output: bool,
) -> i32 {
    let is_proxy_mode = match mode {
        "proxy" => true,
        "tcp" => false,
        other => {
            eprintln!("invalid upstream test mode '{other}': expected 'proxy' or 'tcp'");
            return EXIT_CLI_PARSE_ERROR;
        }
    };
    let target = match target {
        Some(t) => match t.parse::<TargetAddr>() {
            Ok(addr) => addr,
            Err(e) => {
                eprintln!("invalid target: {e}");
                return EXIT_CLI_PARSE_ERROR;
            }
        },
        None => TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        },
    };

    let target_string = target.to_string();
    let mut results = Vec::new();

    for upstream in &rt.upstreams {
        let chain = &upstream.chain;
        let first_hop = &chain.hops[0];
        let host = &first_hop.endpoint.host;
        let port = first_hop.endpoint.port;

        let result = if is_proxy_mode {
            let hops = chain.hops.clone();
            let target_for_closure = target.clone();
            let (reachable, latency_ms, error) = run_async_test(move || {
                let target = target_for_closure.clone();
                let hops = hops.clone();
                Box::pin(async move {
                    let executor = build_test_chain_executor();
                    test_upstream_proxy(&executor, &hops, &target, timeout).await
                })
            });
            UpstreamTestResult {
                id: upstream.id.clone(),
                host: host.clone(),
                port,
                target: target_string.clone(),
                mode: "proxy".to_string(),
                reachable,
                latency_ms,
                error,
                failure: None,
                failed_hop: None,
            }
        } else {
            let host_owned = host.clone();
            let target_result = run_async_test(move || {
                let host = host_owned.clone();
                Box::pin(async move { test_upstream_tcp(&host, port, timeout).await })
            });
            UpstreamTestResult {
                id: upstream.id.clone(),
                host: host.clone(),
                port,
                target: target_string.clone(),
                ..target_result
            }
        };
        results.push(result);
    }

    if results.is_empty() {
        eprintln!("no upstreams found matching criteria");
        return EXIT_CONFIG_VALIDATION;
    }

    if json_output {
        match serde_json::to_string_pretty(&results) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("failed to serialize results: {e}");
                return EXIT_RUNTIME_FAILURE;
            }
        }
    } else {
        for result in &results {
            print_upstream_test_result(result);
        }
    }

    if results.iter().any(|r| r.reachable) {
        EXIT_SUCCESS
    } else {
        EXIT_RUNTIME_FAILURE
    }
}

pub fn print_upstream_test_result(result: &UpstreamTestResult) {
    let status = if result.reachable {
        "reachable"
    } else {
        "unreachable"
    };
    let latency = result
        .latency_ms
        .map(|ms| format!("{}ms", ms))
        .unwrap_or_else(|| "n/a".to_string());
    let error = result
        .error
        .as_deref()
        .map(|e| format!(" ({e})"))
        .unwrap_or_default();

    println!(
        "{} {}:{} [{}] latency={}{}",
        result.id, result.host, result.port, status, latency, error
    );
}

pub fn run_async_test<F, T>(make_future: F) -> T
where
    F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>> + Send + 'static,
    T: Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::Builder::new()
            .name("eggress-cli-test".to_string())
            .spawn(move || -> T {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build tokio runtime for cli test");
                rt.block_on(make_future())
            })
            .expect("failed to spawn cli test thread")
            .join()
            .expect("cli test thread panicked")
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime for cli test");
        rt.block_on(make_future())
    }
}

async fn test_upstream_proxy(
    executor: &ChainExecutor,
    chain: &[eggress_uri::ProxyHopSpec],
    target: &TargetAddr,
    timeout: Duration,
) -> (bool, Option<u64>, Option<String>) {
    let start = Instant::now();

    match tokio::time::timeout(timeout, executor.execute(chain, target)).await {
        Ok(Ok(_stream)) => {
            let elapsed = start.elapsed().as_millis() as u64;
            (true, Some(elapsed), None)
        }
        Ok(Err(e)) => (false, None, Some(e.to_string())),
        Err(_) => (false, None, Some("connection timed out".to_string())),
    }
}

/// Build the upstream-test chain executor from the production connector
/// registry (`eggress-server::build_chain_executor`), not a CLI-local
/// reimplementation.
///
/// Diagnostic coverage therefore tracks live connection support: adding a
/// production-supported upstream protocol never requires a separate
/// registration in CLI test code. Both `eggress upstream test --mode proxy`
/// and `pproxy --test` execute through this path.
pub fn build_test_chain_executor() -> ChainExecutor {
    // `None` arguments are polymorphic over the feature-gated parameter
    // types (`Option<Arc<ShadowsocksMetrics>>` vs `Option<()>`), so this
    // call tracks the production signature without mirroring its cfg gates.
    // The `ssh` cfg matches the workspace feature-forwarding chain
    // (`eggress-cli/ssh` -> `eggress-runtime/ssh` -> `eggress-server/ssh`)
    // under Cargo feature unification.
    #[cfg(feature = "ssh")]
    {
        eggress_server::build_chain_executor(None, None, None)
    }
    #[cfg(not(feature = "ssh"))]
    {
        eggress_server::build_chain_executor(None, None)
    }
}

pub async fn test_upstream_tcp(host: &str, port: u16, timeout: Duration) -> UpstreamTestResult {
    let addr = format!("{}:{}", host, port);
    let start = Instant::now();

    let result = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await;

    let elapsed = start.elapsed().as_millis() as u64;

    match result {
        Ok(Ok(_stream)) => UpstreamTestResult {
            id: String::new(),
            host: host.to_string(),
            port,
            target: String::new(),
            mode: "tcp".to_string(),
            reachable: true,
            latency_ms: Some(elapsed),
            error: None,
            failure: None,
            failed_hop: None,
        },
        Ok(Err(e)) => UpstreamTestResult {
            id: String::new(),
            host: host.to_string(),
            port,
            target: String::new(),
            mode: "tcp".to_string(),
            reachable: false,
            latency_ms: None,
            error: Some(e.to_string()),
            failure: None,
            failed_hop: None,
        },
        Err(_) => UpstreamTestResult {
            id: String::new(),
            host: host.to_string(),
            port,
            target: String::new(),
            mode: "tcp".to_string(),
            reachable: false,
            latency_ms: None,
            error: Some("connection timed out".to_string()),
            failure: None,
            failed_hop: None,
        },
    }
}

#[cfg(all(test, feature = "pproxy-compat"))]
mod exit_code_mirror_tests {
    /// The non-`pproxy-compat` constant mirror in this crate must stay
    /// numerically identical to the single owner in
    /// `eggress-pproxy-compat::exit_codes`.
    #[test]
    fn exit_code_mirror_matches_owner() {
        use eggress_pproxy_compat::exit_codes as owner;
        assert_eq!(super::EXIT_SUCCESS, owner::EXIT_SUCCESS);
        assert_eq!(super::EXIT_RUNTIME_FAILURE, owner::EXIT_RUNTIME_FAILURE);
        assert_eq!(super::EXIT_CLI_PARSE_ERROR, owner::EXIT_CLI_PARSE_ERROR);
        assert_eq!(super::EXIT_CONFIG_VALIDATION, owner::EXIT_CONFIG_VALIDATION);
        assert_eq!(super::EXIT_BIND_FAILURE, owner::EXIT_BIND_FAILURE);
        assert_eq!(
            super::EXIT_UNSUPPORTED_FEATURE,
            owner::EXIT_UNSUPPORTED_FEATURE
        );
        assert_eq!(super::EXIT_PLATFORM_MISSING, owner::EXIT_PLATFORM_MISSING);
        assert_eq!(
            super::EXIT_EXTERNAL_DEPENDENCY,
            owner::EXIT_EXTERNAL_DEPENDENCY
        );
        assert_eq!(super::EXIT_SIGINT, owner::EXIT_SIGINT);
        assert_eq!(super::EXIT_SIGTERM, owner::EXIT_SIGTERM);
    }
}

#[cfg(test)]
mod production_registry_tests {
    use super::*;

    /// Closed mode domain fails closed at the library boundary instead of
    /// silently falling back to either probe path. The CLI parser already
    /// rejects typos; this pins the defense-in-depth for programmatic and
    /// Python-facing callers that bypass Clap.
    #[test]
    fn invalid_upstream_test_mode_fails_closed() {
        let (rt, _) = eggress_config::validate_and_compile_toml_with_warnings(
            "version = 1\n[[listeners]]\nname = \"http-in\"\nbind = \"127.0.0.1:0\"\nprotocols = [\"http\"]\n",
        )
        .expect("minimal config must compile");
        let code = run_upstream_test_with_mode(&rt, None, "socks", Duration::from_secs(1), false);
        assert_eq!(code, EXIT_CLI_PARSE_ERROR);
    }

    /// Bind failures map to the documented bind exit code on every entry
    /// point (native `--config` startup and the compat facade share this
    /// helper); all other runtime errors stay generic runtime failures.
    #[test]
    fn runtime_bind_errors_map_to_bind_failure() {
        let io = |kind: std::io::ErrorKind| std::io::Error::new(kind, "test bind");
        assert_eq!(
            runtime_error_exit_code(&eggress_runtime::RuntimeError::ListenerBind {
                addr: "127.0.0.1:8080".to_string(),
                source: io(std::io::ErrorKind::AddrInUse),
            }),
            EXIT_BIND_FAILURE
        );
        assert_eq!(
            runtime_error_exit_code(&eggress_runtime::RuntimeError::AdminBind {
                addr: "127.0.0.1:9090".to_string(),
                source: io(std::io::ErrorKind::PermissionDenied),
            }),
            EXIT_BIND_FAILURE
        );
        assert_eq!(
            runtime_error_exit_code(&eggress_runtime::RuntimeError::Other("boom".to_string())),
            EXIT_RUNTIME_FAILURE
        );
        assert_eq!(
            runtime_error_exit_code(&eggress_runtime::RuntimeError::Config("bad".to_string())),
            EXIT_RUNTIME_FAILURE
        );
    }

    /// Registry parity proof: Shadowsocks AEAD upstreams are production
    /// paths, but the old CLI-local test registry (HTTP/SOCKS4/SOCKS5 only)
    /// reported them as "no handler". The production registry must accept
    /// the hop and fail only on connection, never on handler lookup.
    #[test]
    fn test_chain_executor_covers_production_shadowsocks() {
        let executor = build_test_chain_executor();
        let spec = eggress_uri::parse_proxy_chain("ss://aes-128-gcm:testpass@127.0.0.1:18388")
            .expect("shadowsocks test URI must parse");
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        };
        let outcome = run_async_test(move || {
            let hops = spec.hops.clone();
            let target = target.clone();
            Box::pin(async move {
                match executor.execute(&hops, &target).await {
                    Ok(_) => "reachable".to_string(),
                    Err(e) => e.to_string(),
                }
            })
        });
        assert!(
            !outcome.contains("no handler"),
            "production registry must cover Shadowsocks (got: {outcome})"
        );
    }
}
