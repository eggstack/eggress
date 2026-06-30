use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::{BoxStream, TargetAddr, TargetHost};
use eggress_routing::{RouteActionSpec, RouteService, Router, SharedRoutingService};
use eggress_server::ConnectionConfig;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{fmt, EnvFilter};

static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(1);
static ACTIVE_CONNECTIONS: AtomicU64 = AtomicU64::new(0);

#[derive(Parser, Debug)]
#[command(name = "eggress", version, about = "A multi-protocol TCP proxy")]
struct Cli {
    #[arg(short = 'l', long = "listen", value_name = "URI")]
    listeners: Vec<String>,

    #[arg(short = 'r', long = "remote", value_name = "URI")]
    upstreams: Vec<String>,

    #[arg(long = "log-format", value_name = "FORMAT", default_value = "pretty")]
    log_format: String,

    #[arg(long = "config", value_name = "PATH")]
    config: Option<String>,

    #[arg(long = "rules-file", value_name = "PATH")]
    rules_file: Option<String>,

    #[command(subcommand)]
    command: Option<SubCommand>,
}

#[derive(Subcommand, Debug)]
enum SubCommand {
    Route(RouteExplain),
    Upstream(UpstreamCommand),
    Pproxy(PproxyCommand),
}

#[derive(Parser, Debug)]
struct UpstreamCommand {
    #[command(subcommand)]
    action: UpstreamAction,
}

#[derive(Subcommand, Debug)]
enum UpstreamAction {
    Test(UpstreamTest),
}

#[derive(Parser, Debug)]
struct UpstreamTest {
    #[arg(short, long, value_name = "ID")]
    id: Option<String>,

    #[arg(short, long, value_name = "HOST:PORT")]
    target: Option<String>,

    #[arg(short, long)]
    config: Option<String>,

    #[arg(long, default_value = "5")]
    timeout: u64,

    #[arg(long, default_value = "proxy")]
    mode: String,

    #[arg(long)]
    json: bool,
}

#[derive(Parser, Debug)]
struct RouteExplain {
    target: String,

    #[arg(short = 'c', long = "config")]
    config: Option<String>,

    #[arg(long)]
    listener: Option<String>,

    #[arg(long)]
    protocol: Option<String>,

    #[arg(long)]
    json: bool,

    #[arg(long, value_name = "URL")]
    admin: Option<String>,
}

#[derive(Parser, Debug)]
struct PproxyCommand {
    #[command(subcommand)]
    action: PproxyAction,
}

#[derive(Subcommand, Debug)]
enum PproxyAction {
    /// Translate pproxy arguments to Eggress TOML
    Translate(PproxyTranslate),
    /// Check pproxy arguments and report parity tier
    Check(PproxyCheck),
    /// Translate and run pproxy-style arguments
    Run(PproxyRun),
}

#[derive(Parser, Debug)]
struct PproxyTranslate {
    /// pproxy-style arguments (after --)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,

    /// Add explanatory comments to generated TOML
    #[arg(long)]
    annotate: bool,
}

#[derive(Parser, Debug)]
struct PproxyCheck {
    /// pproxy-style arguments (after --)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Parser, Debug)]
struct PproxyRun {
    /// pproxy-style arguments (after --)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,

    #[arg(long = "log-format", value_name = "FORMAT", default_value = "pretty")]
    log_format: String,
}

fn init_logging(format: &str) {
    let builder = fmt().with_env_filter(
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
    );

    match format {
        "json" => builder.json().init(),
        "compact" => builder.compact().init(),
        _ => builder.compact().init(),
    }
}

fn handle_route_explain(args: &RouteExplain) {
    if let Some(ref admin_url) = args.admin {
        handle_route_explain_remote(args, admin_url);
        return;
    }

    let (router, is_online) = match &args.config {
        Some(path) => match eggress_config::compile::load_and_compile(path) {
            Ok(rt) => match build_router_from_config(&rt) {
                Ok(r) => (r, true),
                Err(e) => {
                    eprintln!("failed to build router from config: {e}");
                    std::process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("failed to load config: {e}");
                std::process::exit(1);
            }
        },
        None => (Router::new(vec![], RouteActionSpec::Direct), false),
    };

    let target: eggress_core::TargetAddr = match args.target.parse() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let protocol = match args.protocol.as_deref() {
        Some("http") => eggress_core::ProtocolId::Http,
        Some("socks4") => eggress_core::ProtocolId::Socks4,
        Some("socks5") => eggress_core::ProtocolId::Socks5,
        Some(p) => {
            eprintln!("unknown protocol: {p}");
            std::process::exit(1);
        }
        None => eggress_core::ProtocolId::Http,
    };

    let listener = args.listener.as_deref().unwrap_or("cli");

    let request = eggress_routing::RouteRequest {
        target: &target,
        source: None,
        listener,
        inbound_protocol: protocol,
        identity: &eggress_core::ClientIdentity::Anonymous,
        transport: eggress_routing::TransportKind::Tcp,
    };

    let explanation = router.explain(&request, 0);

    if args.json {
        match serde_json::to_string_pretty(&explanation) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("failed to serialize explanation: {e}");
                std::process::exit(1);
            }
        }
    } else {
        print_explanation(&explanation, is_online);
    }
}

fn handle_route_explain_remote(args: &RouteExplain, admin_url: &str) {
    let target: eggress_core::TargetAddr = match args.target.parse() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let protocol = match args.protocol.as_deref() {
        Some("http") => "http",
        Some("socks4") => "socks4",
        Some("socks5") => "socks5",
        Some(p) => {
            eprintln!("unknown protocol: {p}");
            std::process::exit(1);
        }
        None => "http",
    };

    let listener = args.listener.as_deref().unwrap_or("default");

    let body = serde_json::json!({
        "target": target.to_string(),
        "listener": listener,
        "protocol": protocol,
    });

    let base = admin_url.trim_end_matches('/');
    let url = format!("{base}/-/route-explain");

    let (host, port, path) = parse_admin_url(&url);

    let body_str = body.to_string();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body_str}",
        body_str.len(),
    );

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime.block_on(async {
        let addr = format!("{host}:{port}");
        let mut stream = match tokio::net::TcpStream::connect(&addr).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("failed to connect to admin at {addr}: {e}");
                std::process::exit(1);
            }
        };

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        if let Err(e) = stream.write_all(request.as_bytes()).await {
            eprintln!("failed to send request: {e}");
            std::process::exit(1);
        }
        let _ = stream.shutdown().await;

        let mut response = Vec::new();
        loop {
            let mut buf = [0u8; 4096];
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => response.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        String::from_utf8_lossy(&response).to_string()
    });

    let body_start = result.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    let body = &result[body_start..];

    let status_line = result.lines().next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    if status != 200 {
        eprintln!("admin returned {status}: {body}");
        std::process::exit(1);
    }

    if args.json {
        println!("{body}");
    } else {
        match serde_json::from_str::<eggress_routing::RouteExplanation>(body) {
            Ok(explanation) => print_explanation(&explanation, true),
            Err(e) => {
                eprintln!("failed to parse response: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn parse_admin_url(url: &str) -> (String, u16, String) {
    let without_proto = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    let (host_port, path) = match without_proto.find('/') {
        Some(i) => (&without_proto[..i], &without_proto[i..]),
        None => (without_proto, "/"),
    };
    let (host, port) = match host_port.rfind(':') {
        Some(i) => (
            host_port[..i].to_string(),
            host_port[i + 1..].parse::<u16>().unwrap_or(9090),
        ),
        None => (host_port.to_string(), 9090),
    };
    (host, port, path.to_string())
}

fn handle_upstream_test(args: &UpstreamTest) {
    let rt = match &args.config {
        Some(path) => match eggress_config::compile::load_and_compile(path) {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("failed to load config: {e}");
                std::process::exit(1);
            }
        },
        None => {
            eprintln!("--config is required for upstream test");
            std::process::exit(1);
        }
    };

    let upstreams: Vec<_> = if let Some(ref id) = args.id {
        rt.upstreams.iter().filter(|u| &u.id == id).collect()
    } else {
        rt.upstreams.iter().collect()
    };

    if upstreams.is_empty() {
        eprintln!("no upstreams found matching criteria");
        std::process::exit(1);
    }

    let target = match &args.target {
        Some(t) => match t.parse::<eggress_core::TargetAddr>() {
            Ok(addr) => addr,
            Err(e) => {
                eprintln!("invalid target: {e}");
                std::process::exit(1);
            }
        },
        None => eggress_core::TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 443,
        },
    };

    let timeout = Duration::from_secs(args.timeout);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let is_proxy_mode = args.mode == "proxy";

    let mut results = Vec::new();

    for upstream in &upstreams {
        let chain = &upstream.chain;
        let first_hop = &chain.hops[0];
        let host = &first_hop.endpoint.host;
        let port = first_hop.endpoint.port;

        let result = if is_proxy_mode {
            let executor = build_test_chain_executor();
            let (reachable, latency_ms, error) = runtime.block_on(test_upstream_proxy(
                &executor,
                &chain.hops,
                &target,
                timeout,
            ));
            UpstreamTestResult {
                id: upstream.id.clone(),
                host: host.clone(),
                port,
                target: target.to_string(),
                mode: "proxy".to_string(),
                reachable,
                latency_ms,
                error,
                failure: None,
                failed_hop: None,
            }
        } else {
            let result = runtime.block_on(test_upstream(host, port, timeout));
            UpstreamTestResult {
                id: upstream.id.clone(),
                host: host.clone(),
                port,
                target: target.to_string(),
                ..result
            }
        };
        results.push(result);
    }

    if args.json {
        match serde_json::to_string_pretty(&results) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("failed to serialize results: {e}");
                std::process::exit(1);
            }
        }
    } else {
        for result in &results {
            print_upstream_test_result(result);
        }
    }
}

fn handle_pproxy_translate(args: &PproxyTranslate) {
    let pproxy_args = match eggress_pproxy_compat::PproxyArgs::parse(&args.args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let output = match eggress_pproxy_compat::translate_pproxy_args(&pproxy_args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    if output.has_unsupported() {
        for u in &output.unsupported {
            eprintln!("warning: {u}");
        }
        eprintln!("\nGenerated TOML may be incomplete due to unsupported features.");
    }

    for w in &output.warnings {
        eprintln!("warning: {w}");
    }

    if args.annotate {
        println!("# Generated by eggress pproxy translate");
        println!("# pproxy arguments: {}", args.args.join(" "));
        if !output.warnings.is_empty() || !output.unsupported.is_empty() {
            println!("#");
            for w in &output.warnings {
                println!("# {w}");
            }
            for u in &output.unsupported {
                println!("# {u}");
            }
        }
        println!();
    }

    print!("{}", output.toml);

    if output.has_unsupported() {
        std::process::exit(1);
    }
}

fn handle_pproxy_check(args: &PproxyCheck) {
    let pproxy_args = match eggress_pproxy_compat::PproxyArgs::parse(&args.args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let output = match eggress_pproxy_compat::translate_pproxy_args(&pproxy_args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    println!("pproxy compatibility check");
    println!("=========================");

    let local_uris = pproxy_args.parse_local_uris();
    let remote_uris = pproxy_args.parse_remote_uris();

    match local_uris {
        Ok(uris) => {
            for uri in &uris {
                println!(
                    "  local:  {} -> scheme={}",
                    uri.redacted_display(),
                    uri.scheme
                );
            }
        }
        Err(e) => eprintln!("  local:  error: {e}"),
    }

    match remote_uris {
        Ok(uris) => {
            for uri in &uris {
                println!(
                    "  remote: {} -> scheme={}",
                    uri.redacted_display(),
                    uri.scheme
                );
            }
        }
        Err(e) => eprintln!("  remote: error: {e}"),
    }

    if output.warnings.is_empty() && output.unsupported.is_empty() {
        println!("\nparity tier: Compatible");
    } else if output.unsupported.is_empty() {
        println!("\nparity tier: Supported (with warnings)");
    } else {
        println!("\nparity tier: Partial");
    }

    if !output.warnings.is_empty() {
        println!("\nwarnings:");
        for w in &output.warnings {
            println!("  {w}");
        }
    }

    if !output.unsupported.is_empty() {
        println!("\nunsupported:");
        for u in &output.unsupported {
            println!("  {u}");
        }
    }
}

fn handle_pproxy_run(args: &PproxyRun) {
    let pproxy_args = match eggress_pproxy_compat::PproxyArgs::parse(&args.args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let output = match eggress_pproxy_compat::translate_pproxy_args(&pproxy_args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    if output.has_unsupported() {
        for u in &output.unsupported {
            eprintln!("warning: {u}");
        }
        eprintln!("\nSome features are unsupported. Service may not behave as expected.");
    }

    for w in &output.warnings {
        eprintln!("warning: {w}");
    }

    // Write generated TOML to a temp file and start supervisor
    let tmp_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("failed to create temp directory: {e}");
            std::process::exit(1);
        }
    };
    let config_path = tmp_dir.path().join("pproxy-compat.toml");
    if let Err(e) = std::fs::write(&config_path, &output.toml) {
        eprintln!("failed to write config: {e}");
        std::process::exit(1);
    }

    tracing::info!("starting eggress with pproxy-compatible config");

    match eggress_runtime::ServiceSupervisor::start(config_path.to_str().unwrap_or_default()) {
        Ok(mut supervisor) => {
            if let Err(e) = supervisor.run() {
                eprintln!("runtime error: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("runtime error: {e}");
            std::process::exit(1);
        }
    }
}

#[derive(serde::Serialize)]
struct UpstreamTestResult {
    id: String,
    host: String,
    port: u16,
    target: String,
    mode: String,
    reachable: bool,
    latency_ms: Option<u64>,
    error: Option<String>,
    failure: Option<String>,
    failed_hop: Option<usize>,
}

fn build_test_chain_executor() -> ChainExecutor {
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

async fn test_upstream(host: &str, port: u16, timeout: Duration) -> UpstreamTestResult {
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

fn print_upstream_test_result(result: &UpstreamTestResult) {
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

fn print_explanation(explanation: &eggress_routing::RouteExplanation, is_online: bool) {
    println!("Target: {}", explanation.target);
    println!("Listener: {}", explanation.listener);
    println!("Protocol: {}", explanation.protocol);
    if let Some(ref rule) = explanation.matched_rule {
        println!("Matched rule: {rule}");
    }
    println!("Action: {}", explanation.action);
    if let Some(ref group) = explanation.upstream_group {
        println!("Upstream group: {group}");
    }
    if let Some(ref scheduler) = explanation.scheduler {
        println!("Scheduler: {scheduler}");
    }
    if !explanation.eligible_upstreams.is_empty() {
        println!("Eligible upstreams:");
        for u in &explanation.eligible_upstreams {
            println!(
                "  {}  {}  active={}  in_flight={}",
                u.id, u.health, u.active, u.in_flight
            );
        }
    }
    if let Some(ref upstream) = explanation.selected_upstream {
        println!("Selected upstream: {upstream}");
    }
    if let Some(ref chain) = explanation.chain {
        println!("Chain: {chain}");
    }
    if is_online {
        println!("Config generation: {}", explanation.generation);
    } else {
        println!("Mode: offline");
        println!("Generation: not-live");
    }
}

fn build_router_from_cli(args: &Cli) -> Result<Router, Box<dyn std::error::Error + Send + Sync>> {
    let upstream_chain: Option<eggress_uri::ProxyChainSpec> = if args.upstreams.is_empty() {
        None
    } else {
        let combined = args.upstreams.join("__");
        match eggress_uri::parse_proxy_chain(&combined) {
            Ok(spec) => Some(spec),
            Err(e) => return Err(format!("invalid upstream URI: {e}").into()),
        }
    };

    let mut rules: Vec<eggress_routing::CompiledRule> = Vec::new();
    let mut default_action = RouteActionSpec::Direct;
    let mut groups = Vec::new();

    if let Some(ref spec) = upstream_chain {
        let upstream = Arc::new(eggress_routing::upstream::UpstreamRuntime::new(
            eggress_core::UpstreamId::new("cli-upstream"),
            spec.clone(),
        ));
        let group_id = eggress_routing::UpstreamGroupId(Arc::from("cli-group"));
        let group = eggress_routing::upstream::UpstreamGroup::new(
            group_id.clone(),
            eggress_routing::scheduler::SchedulerKind::FirstAvailable,
            Arc::from([upstream]),
            eggress_routing::upstream::GroupFallback::Direct,
        );
        default_action = RouteActionSpec::UpstreamGroup(group_id.clone());
        groups.push((group_id, group));
    }

    if let Some(ref rules_file_path) = args.rules_file {
        let content = std::fs::read_to_string(rules_file_path)
            .map_err(|e| format!("failed to read rules file '{}': {}", rules_file_path, e))?;
        let compat_rules = eggress_routing::CompatRegexRule::parse_file(&content)
            .map_err(|e| format!("failed to parse rules file '{}': {}", rules_file_path, e))?;
        for (idx, compat) in compat_rules.into_iter().enumerate() {
            rules.push(eggress_routing::CompiledRule {
                id: eggress_routing::RuleId(Arc::from(format!("rules-file-{}", idx + 1).as_str())),
                matcher: eggress_routing::MatchExpr::HostRegex(compat.pattern),
                action: default_action.clone(),
            });
        }
    }

    Ok(Router::with_groups(rules, default_action, groups))
}

struct ListenerSpec {
    bind_addr: SocketAddr,
    protocols: Vec<eggress_core::ProtocolId>,
    auth: eggress_server::accept::InboundAuthentication,
}

fn parse_listener_uri(uri: &str) -> Result<ListenerSpec, Box<dyn std::error::Error + Send + Sync>> {
    let spec = eggress_uri::parse_proxy_chain(uri)?;
    let first_hop = &spec.hops[0];
    let bind_addr: SocketAddr =
        format!("{}:{}", first_hop.endpoint.host, first_hop.endpoint.port).parse()?;

    let protocols: Vec<eggress_core::ProtocolId> = first_hop
        .protocols
        .iter()
        .map(|p| match p {
            eggress_uri::ProtocolSpec::Http => eggress_core::ProtocolId::Http,
            eggress_uri::ProtocolSpec::Socks4 => eggress_core::ProtocolId::Socks4,
            eggress_uri::ProtocolSpec::Socks5 => eggress_core::ProtocolId::Socks5,
            eggress_uri::ProtocolSpec::Shadowsocks => eggress_core::ProtocolId::Shadowsocks,
            eggress_uri::ProtocolSpec::Trojan => eggress_core::ProtocolId::Trojan,
        })
        .collect();

    let auth = match &first_hop.credentials {
        Some(credentials) => eggress_server::accept::InboundAuthentication::UsernamePassword {
            username: credentials.username.clone(),
            password: credentials.password.clone(),
        },
        None => eggress_server::accept::InboundAuthentication::None,
    };

    Ok(ListenerSpec {
        bind_addr,
        protocols,
        auth,
    })
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    if let Some(SubCommand::Route(explain_args)) = args.command {
        handle_route_explain(&explain_args);
        return;
    }

    if let Some(SubCommand::Upstream(upstream_cmd)) = args.command {
        match upstream_cmd.action {
            UpstreamAction::Test(test_args) => {
                handle_upstream_test(&test_args);
                return;
            }
        }
    }

    if let Some(SubCommand::Pproxy(pproxy_cmd)) = args.command {
        match pproxy_cmd.action {
            PproxyAction::Translate(translate_args) => {
                handle_pproxy_translate(&translate_args);
                return;
            }
            PproxyAction::Check(check_args) => {
                handle_pproxy_check(&check_args);
                return;
            }
            PproxyAction::Run(run_args) => {
                init_logging(&run_args.log_format);
                handle_pproxy_run(&run_args);
                return;
            }
        }
    }

    if args.config.is_some() && (!args.listeners.is_empty() || !args.upstreams.is_empty()) {
        eprintln!("--config mode is incompatible with -l and -r flags. Use one or the other.");
        std::process::exit(1);
    }

    if let Some(ref config_path) = args.config {
        init_logging(&args.log_format);
        match eggress_runtime::ServiceSupervisor::start(config_path) {
            Ok(mut supervisor) => {
                if let Err(e) = supervisor.run() {
                    eprintln!("runtime error: {e}");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("runtime error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    init_logging(&args.log_format);
    let cancel_token = CancellationToken::new();

    let router = match build_router_from_cli(&args) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let routing_service = Arc::new(SharedRoutingService::new(router));

    let metrics = Arc::new(eggress_metrics::MetricsRegistry::new());

    let listener_uris: Vec<String> = if args.listeners.is_empty() {
        vec!["http://127.0.0.1:8080".to_string()]
    } else {
        args.listeners
    };

    let mut listener_specs = Vec::new();
    for uri in &listener_uris {
        match parse_listener_uri(uri) {
            Ok(spec) => listener_specs.push((uri.clone(), spec)),
            Err(e) => {
                eprintln!("invalid listener URI '{uri}': {e}");
                std::process::exit(1);
            }
        }
    }

    let mut handles = Vec::new();

    for (uri, spec) in &listener_specs {
        let cancel = cancel_token.clone();
        let routing = routing_service.clone();
        let metrics = metrics.clone();
        let bind_addr = spec.bind_addr;
        let protocols = spec.protocols.clone();
        let auth = spec.auth.clone();
        let uri = uri.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = run_listener(bind_addr, protocols, routing, auth, metrics, cancel).await
            {
                tracing::error!("listener '{uri}' error: {e}");
            }
        });
        handles.push(handle);
    }

    tracing::info!("eggress started, {} listener(s)", listener_specs.len());

    let mut shutdown_handles = handles;

    {
        let token = cancel_token.clone();

        #[cfg(unix)]
        {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
            let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup());

            if let Err(ref e) = sigterm {
                tracing::warn!("failed to register SIGTERM handler: {e}");
            }
            if let Err(ref e) = sighup {
                tracing::warn!("failed to register SIGHUP handler: {e}");
            }

            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        tracing::info!("shutdown signal received");
                        token.cancel();
                        break;
                    }
                    _ = async { sigterm.as_mut().ok()?.recv().await }, if sigterm.is_ok() => {
                        tracing::info!("shutdown signal received");
                        token.cancel();
                        break;
                    }
                    _ = async { sighup.as_mut().ok()?.recv().await }, if sighup.is_ok() => {
                        tracing::warn!("SIGHUP received but no config file specified in compatibility mode, ignoring");
                    }
                }
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("shutdown signal received");
            token.cancel();
        }
    }

    tracing::info!("draining active connections");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let active = ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
        if active == 0 {
            tracing::info!("all connections drained");
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(active, "drain timeout reached, forcing shutdown");
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    for h in shutdown_handles.drain(..) {
        let _ = h.await;
    }

    tracing::info!("eggress stopped");
}

fn build_router_from_config(
    rt: &eggress_config::RuntimeConfig,
) -> Result<Router, Box<dyn std::error::Error + Send + Sync>> {
    let mut seen_upstream_ids = std::collections::HashSet::new();
    let mut upstreams = Vec::new();

    for u in &rt.upstreams {
        if !seen_upstream_ids.insert(u.id.clone()) {
            return Err(format!("duplicate upstream ID '{}'", u.id).into());
        }
        let id = eggress_routing::UpstreamGroupId(Arc::from(u.id.as_str()));
        let runtime = eggress_routing::upstream::UpstreamRuntime::new(
            eggress_core::UpstreamId::new(u.id.clone()),
            u.chain.clone(),
        );
        upstreams.push((id, runtime));
    }

    let upstream_map: std::collections::HashMap<
        String,
        Arc<eggress_routing::upstream::UpstreamRuntime>,
    > = upstreams
        .into_iter()
        .map(|(id, runtime)| (id.0.to_string(), Arc::new(runtime)))
        .collect();

    let mut seen_group_ids = std::collections::HashSet::new();
    let mut groups = Vec::new();

    for g in &rt.groups {
        if !seen_group_ids.insert(g.id.clone()) {
            return Err(format!("duplicate group ID '{}'", g.id).into());
        }
        let mut members = Vec::new();
        for m in &g.members {
            let member = upstream_map
                .get(m)
                .ok_or_else(|| format!("group '{}' references unknown upstream '{}'", g.id, m))?;
            members.push(member.clone());
        }
        if members.is_empty() {
            return Err(format!("group '{}' has no valid members", g.id).into());
        }

        let fallback = match g.fallback {
            eggress_config::compile::GroupFallback::Reject => {
                eggress_routing::upstream::GroupFallback::Reject
            }
            eggress_config::compile::GroupFallback::Direct => {
                eggress_routing::upstream::GroupFallback::Direct
            }
            eggress_config::compile::GroupFallback::UseUnhealthy => {
                eggress_routing::upstream::GroupFallback::UseUnhealthy
            }
        };

        groups.push((
            g.id.clone(),
            eggress_routing::upstream::UpstreamGroup::new(
                g.id.clone(),
                g.scheduler,
                Arc::from(members),
                fallback,
            ),
        ));
    }

    let group_ids: std::collections::HashSet<_> = groups.iter().map(|(id, _)| id.clone()).collect();

    let mut rules = Vec::new();
    for r in &rt.rules {
        let action = match &r.action {
            eggress_routing::RouteActionSpec::Direct => eggress_routing::RouteActionSpec::Direct,
            eggress_routing::RouteActionSpec::UpstreamGroup(gid) => {
                if !group_ids.contains(gid) {
                    return Err(
                        format!("rule '{}' references unknown group '{}'", r.id, gid).into(),
                    );
                }
                eggress_routing::RouteActionSpec::UpstreamGroup(gid.clone())
            }
            eggress_routing::RouteActionSpec::Reject(reason) => {
                eggress_routing::RouteActionSpec::Reject(reason.clone())
            }
        };
        rules.push(eggress_routing::CompiledRule {
            id: r.id.clone(),
            matcher: r.matcher.clone(),
            action,
        });
    }

    Ok(Router::with_groups(
        rules,
        rt.default_action.clone(),
        groups,
    ))
}

async fn run_listener(
    bind_addr: SocketAddr,
    protocols: Vec<eggress_core::ProtocolId>,
    routing: Arc<SharedRoutingService>,
    authentication: eggress_server::accept::InboundAuthentication,
    metrics: Arc<dyn eggress_server::SessionMetrics>,
    cancel_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = TcpListenerConfig {
        bind_addr,
        protocols: protocols.clone(),
        auth_required: false,
        handshake_timeout: Duration::from_secs(30),
        connection_limit: 1024,
    };

    let listener = TcpListener::new(&config, cancel_token.clone()).await?;
    let local_addr = listener.local_addr()?;
    tracing::info!("listening on {local_addr}");

    let proto_slice: Arc<[eggress_core::ProtocolId]> = config.protocols.clone().into();

    loop {
        let conn = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                if e.to_string().contains("listener cancelled") {
                    break;
                }
                tracing::error!("accept error: {e}");
                continue;
            }
        };

        let routing = routing.clone();
        let peer = conn.peer_addr;
        let listener = local_addr;
        let conn_id = CONNECTION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let conn_protocols = proto_slice.clone();
        let conn_auth = authentication.clone();
        let conn_metrics = metrics.clone();

        ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);

        tokio::spawn(async move {
            let started = std::time::Instant::now();
            let config = ConnectionConfig {
                routing: routing as Arc<dyn RouteService>,
                context: eggress_server::ConnectionContext {
                    source: Some(peer),
                    listener: listener.to_string(),
                    generation: 0,
                },
                handshake_timeout: Duration::from_secs(30),
                connect_timeout: Duration::from_secs(30),
                protocols: conn_protocols,
                authentication: conn_auth,
                metrics: Some(conn_metrics),
                udp: None,
                tls_client_config: None,
                shadowsocks: None,
                shadowsocks_metrics: None,
            };

            let report = eggress_server::serve_connection(conn.stream, config)
                .instrument(tracing::info_span!(
                    "conn",
                    id = conn_id,
                    peer = %peer,
                    listener = %listener,
                ))
                .await;

            ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);

            tracing::info!(
                protocol = ?report.protocol,
                target = ?report.target,
                route = %report.route,
                outcome = ?report.outcome,
                bytes_upstream = report.bytes_upstream,
                bytes_downstream = report.bytes_downstream,
                duration_ms = started.elapsed().as_millis() as u64,
                "connection completed",
            );
        });
    }

    Ok(())
}

use tracing::Instrument;

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_http_proxy_end_to_end() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        drop(proxy_listener);

        let cancel = CancellationToken::new();
        let config = TcpListenerConfig {
            bind_addr: proxy_addr,
            protocols: vec![eggress_core::ProtocolId::Http],
            auth_required: false,
            handshake_timeout: Duration::from_secs(5),
            connection_limit: 10,
        };
        let listener = TcpListener::new(&config, cancel.clone()).await.unwrap();

        let routing: Arc<SharedRoutingService> = Arc::new(SharedRoutingService::new(Router::new(
            vec![],
            RouteActionSpec::Direct,
        )));

        let proxy_jh = tokio::spawn(async move {
            loop {
                let conn = match listener.accept().await {
                    Ok(c) => c,
                    Err(_) => break,
                };
                let routing = routing.clone();
                let config = ConnectionConfig {
                    routing: routing as Arc<dyn RouteService>,
                    context: eggress_server::ConnectionContext {
                        source: Some(conn.peer_addr),
                        listener: String::new(),
                        generation: 0,
                    },
                    handshake_timeout: Duration::from_secs(5),
                    connect_timeout: Duration::from_secs(10),
                    protocols: Arc::from([eggress_core::ProtocolId::Http]),
                    authentication: eggress_server::accept::InboundAuthentication::None,
                    metrics: None,
                    udp: None,
                    tls_client_config: None,
                    shadowsocks: None,
                    shadowsocks_metrics: None,
                };
                tokio::spawn(async move {
                    let _ = eggress_server::serve_connection(conn.stream, config).await;
                });
            }
        });

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let connect_req = format!(
            "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n\r\n",
            echo_addr.ip(),
            echo_addr.port(),
            echo_addr.ip(),
            echo_addr.port()
        );
        stream.write_all(connect_req.as_bytes()).await.unwrap();

        let mut response = vec![0u8; 1024];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(
            response_str.contains("200"),
            "expected 200, got: {response_str}"
        );

        let header_end = response_str.find("\r\n\r\n").unwrap() + 4;
        let leftover = &response.as_slice()[header_end..n];

        stream.write_all(b"hello proxy").await.unwrap();
        stream.shutdown().await.unwrap();

        let mut buf = Vec::new();
        if !leftover.is_empty() {
            buf.extend_from_slice(leftover);
        }
        stream.read_to_end(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello proxy");

        cancel.cancel();
        let _ = proxy_jh.await;
        echo_jh.abort();
    }

    #[tokio::test]
    async fn test_upstream_test_reachable() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let result = test_upstream(
            &echo_addr.ip().to_string(),
            echo_addr.port(),
            Duration::from_secs(5),
        )
        .await;

        assert!(result.reachable);
        assert!(result.latency_ms.is_some());
        assert!(result.error.is_none());

        echo_jh.abort();
    }

    #[tokio::test]
    async fn test_upstream_test_unreachable() {
        let result = test_upstream("127.0.0.1", 1, Duration::from_secs(1)).await;

        assert!(!result.reachable);
        assert!(result.latency_ms.is_none());
        assert!(result.error.is_some());
    }

    #[test]
    fn test_upstream_test_json_output() {
        let result = UpstreamTestResult {
            id: "test-upstream".to_string(),
            host: "127.0.0.1".to_string(),
            port: 1080,
            target: "example.com:443".to_string(),
            mode: "tcp".to_string(),
            reachable: true,
            latency_ms: Some(15),
            error: None,
            failure: None,
            failed_hop: None,
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("\"reachable\": true"));
        assert!(json.contains("\"latency_ms\": 15"));
        assert!(!json.contains("secret"));
    }
}
