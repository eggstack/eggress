use std::time::{Duration, Instant};

/// Apply the optional Linux pproxy daemon transition after compatibility
/// parsing and configuration validation. Re-exec keeps the transition safe
/// under the workspace's `unsafe_code = "deny"` policy and leaves signal,
/// listener, and system-proxy rollback ownership with the child process.
#[cfg(feature = "pproxy-daemon")]
pub fn maybe_daemonize(requested: bool) -> Result<(), String> {
    const CHILD_MARKER: &str = "EGGRESS_PPROXY_DAEMON_CHILD";
    if !requested || std::env::var_os(CHILD_MARKER).is_some() {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let executable = std::env::current_exe()
            .map_err(|error| format!("cannot resolve executable for --daemon: {error}"))?;
        std::process::Command::new(executable)
            .args(std::env::args_os().skip(1))
            .env(CHILD_MARKER, "1")
            .current_dir("/")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("cannot start --daemon child: {error}"))?;
        std::process::exit(0);
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("--daemon compatibility is only available on Linux".to_string())
    }
}

use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::{BoxStream, TargetAddr, TargetHost};

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
pub fn run_upstream_test_with_mode(
    rt: &eggress_config::compile::RuntimeConfig,
    target: Option<&str>,
    mode: &str,
    timeout: Duration,
    json_output: bool,
) -> i32 {
    let target = match target {
        Some(t) => match t.parse::<TargetAddr>() {
            Ok(addr) => addr,
            Err(e) => {
                eprintln!("invalid target: {e}");
                return 2;
            }
        },
        None => TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        },
    };

    let target_string = target.to_string();
    let is_proxy_mode = mode == "proxy";
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
        return 3;
    }

    if json_output {
        match serde_json::to_string_pretty(&results) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("failed to serialize results: {e}");
                return 1;
            }
        }
    } else {
        for result in &results {
            print_upstream_test_result(result);
        }
    }

    if results.iter().any(|r| r.reachable) {
        0
    } else {
        1
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

struct HttpHopHandler;

impl HopHandler for HttpHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Http
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<BoxStream, Box<dyn std::error::Error + Send + Sync>>,
                > + Send
                + 'a,
        >,
    > {
        let auth = hop
            .credentials
            .as_ref()
            .map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            eggress_protocol_http::http_connect(stream, target, auth, &Default::default())
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

struct Socks5HopHandler;

impl HopHandler for Socks5HopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Socks5
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<BoxStream, Box<dyn std::error::Error + Send + Sync>>,
                > + Send
                + 'a,
        >,
    > {
        let socks_addr = target_to_socks_addr(target);
        let auth = hop
            .credentials
            .as_ref()
            .map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            eggress_protocol_socks::socks5::client::socks5_connect(stream, &socks_addr, auth)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

struct Socks4HopHandler;

impl HopHandler for Socks4HopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Socks4
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
        _hop_index: usize,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<BoxStream, Box<dyn std::error::Error + Send + Sync>>,
                > + Send
                + 'a,
        >,
    > {
        let user_id = hop.credentials.as_ref().map(|c| c.username.as_str());
        Box::pin(async move {
            eggress_protocol_socks::socks4_connect(stream, target, user_id)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

pub fn build_test_chain_executor() -> ChainExecutor {
    let handlers: Vec<Box<dyn HopHandler>> = vec![
        Box::new(HttpHopHandler),
        Box::new(Socks5HopHandler),
        Box::new(Socks4HopHandler),
    ];
    ChainExecutor::new(handlers)
}

fn target_to_socks_addr(target: &TargetAddr) -> eggress_protocol_socks::socks5::server::SocksAddr {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => SocksAddr::IPv4(ip.octets(), target.port),
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => SocksAddr::IPv6(ip.octets(), target.port),
        TargetHost::Domain(d) => SocksAddr::Domain(d.clone(), target.port),
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
