use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_core::TargetHost;
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

    #[command(subcommand)]
    command: Option<SubCommand>,
}

#[derive(Subcommand, Debug)]
enum SubCommand {
    Route(RouteExplain),
    Upstream(UpstreamCommand),
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
    let router = match &args.config {
        Some(path) => match eggress_config::compile::load_and_compile(path) {
            Ok(rt) => match build_router_from_config(&rt) {
                Ok(r) => r,
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
        None => Router::new(vec![], RouteActionSpec::Direct),
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
        print_explanation(&explanation);
    }
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

    let mut results = Vec::new();

    for upstream in &upstreams {
        let chain = &upstream.chain;
        let first_hop = &chain.hops[0];
        let host = &first_hop.endpoint.host;
        let port = first_hop.endpoint.port;

        let result = runtime.block_on(test_upstream(host, port, timeout));
        results.push(UpstreamTestResult {
            id: upstream.id.clone(),
            host: host.clone(),
            port,
            target: target.to_string(),
            ..result
        });
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

#[derive(serde::Serialize)]
struct UpstreamTestResult {
    id: String,
    host: String,
    port: u16,
    target: String,
    reachable: bool,
    latency_ms: Option<u64>,
    error: Option<String>,
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
            reachable: true,
            latency_ms: Some(elapsed),
            error: None,
        },
        Ok(Err(e)) => UpstreamTestResult {
            id: String::new(),
            host: host.to_string(),
            port,
            target: String::new(),
            reachable: false,
            latency_ms: None,
            error: Some(e.to_string()),
        },
        Err(_) => UpstreamTestResult {
            id: String::new(),
            host: host.to_string(),
            port,
            target: String::new(),
            reachable: false,
            latency_ms: None,
            error: Some("connection timed out".to_string()),
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

fn print_explanation(explanation: &eggress_routing::RouteExplanation) {
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
    println!("Config generation: {}", explanation.generation);
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

    Ok(match &upstream_chain {
        Some(spec) => {
            let upstream = Arc::new(eggress_routing::upstream::UpstreamRuntime::new(
                1u64,
                spec.clone(),
            ));
            let group_id = eggress_routing::UpstreamGroupId(Arc::from("cli-group"));
            let group = eggress_routing::upstream::UpstreamGroup {
                id: group_id.clone(),
                scheduler: eggress_routing::scheduler::SchedulerKind::FirstAvailable,
                members: Arc::from([upstream]),
                fallback: eggress_routing::upstream::GroupFallback::Direct,
            };
            Router::with_groups(
                vec![],
                RouteActionSpec::UpstreamGroup(group_id.clone()),
                vec![(group_id, group)],
            )
        }
        None => Router::new(vec![], RouteActionSpec::Direct),
    })
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

    init_logging(&args.log_format);
    let cancel_token = CancellationToken::new();

    let config_path = args.config.clone();

    let router = match &config_path {
        Some(path) => match eggress_config::compile::load_and_compile(path) {
            Ok(rt) => match build_router_from_config(&rt) {
                Ok(r) => r,
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
        None => match build_router_from_cli(&args) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
    };

    let routing_service = Arc::new(SharedRoutingService::new(router));

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
        let bind_addr = spec.bind_addr;
        let protocols = spec.protocols.clone();
        let auth = spec.auth.clone();
        let uri = uri.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = run_listener(bind_addr, protocols, routing, auth, cancel).await {
                tracing::error!("listener '{uri}' error: {e}");
            }
        });
        handles.push(handle);
    }

    tracing::info!("eggress started, {} listener(s)", listener_specs.len());

    let mut shutdown_handles = handles;

    {
        let token = cancel_token.clone();
        let config_path = config_path.clone();
        let routing_service = routing_service.clone();

        #[cfg(unix)]
        {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("failed to register SIGTERM handler");
            let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
                .expect("failed to register SIGHUP handler");

            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        tracing::info!("shutdown signal received");
                        token.cancel();
                        break;
                    }
                    _ = sigterm.recv() => {
                        tracing::info!("shutdown signal received");
                        token.cancel();
                        break;
                    }
                    _ = sighup.recv() => {
                        if let Some(ref path) = config_path {
                            tracing::info!("reload signal received, reloading config from {path}");
                            match eggress_config::compile::load_and_compile(path) {
                                Ok(rt) => match build_router_from_config(&rt) {
                                    Ok(new_router) => {
                                        routing_service.swap(new_router);
                                        tracing::info!(
                                            generation = routing_service.generation(),
                                            "config reloaded successfully"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::error!("reload failed (router build): {e}");
                                    }
                                },
                                Err(e) => {
                                    tracing::error!("reload failed (config load): {e}");
                                }
                            }
                        } else {
                            tracing::warn!("SIGHUP received but no config file specified, ignoring");
                        }
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

    let shutdown_grace = config_path
        .as_ref()
        .and_then(|path| eggress_config::compile::load_and_compile(path).ok())
        .map(|rt| rt.process.shutdown_grace)
        .unwrap_or(Duration::from_secs(30));

    tracing::info!("draining active connections (timeout: {shutdown_grace:?})");

    let deadline = tokio::time::Instant::now() + shutdown_grace;
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
    let upstreams: Vec<(
        eggress_routing::UpstreamGroupId,
        eggress_routing::upstream::UpstreamRuntime,
    )> = rt
        .upstreams
        .iter()
        .map(|u| {
            let id = eggress_routing::UpstreamGroupId(Arc::from(u.id.as_str()));
            let runtime =
                eggress_routing::upstream::UpstreamRuntime::new(u.id.len() as u64, u.chain.clone());
            (id, runtime)
        })
        .collect();

    let upstream_map: std::collections::HashMap<
        String,
        Arc<eggress_routing::upstream::UpstreamRuntime>,
    > = upstreams
        .into_iter()
        .map(|(id, runtime)| (id.0.to_string(), Arc::new(runtime)))
        .collect();

    let groups: Vec<(
        eggress_routing::UpstreamGroupId,
        eggress_routing::upstream::UpstreamGroup,
    )> = rt
        .groups
        .iter()
        .filter_map(|g| {
            let members: Vec<Arc<eggress_routing::upstream::UpstreamRuntime>> = g
                .members
                .iter()
                .filter_map(|m| upstream_map.get(m).cloned())
                .collect();

            if members.is_empty() {
                return None;
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

            Some((
                g.id.clone(),
                eggress_routing::upstream::UpstreamGroup {
                    id: g.id.clone(),
                    scheduler: g.scheduler,
                    members: Arc::from(members),
                    fallback,
                },
            ))
        })
        .collect();

    let group_ids: std::collections::HashSet<_> = groups.iter().map(|(id, _)| id.clone()).collect();

    let rules: Vec<eggress_routing::CompiledRule> = rt
        .rules
        .iter()
        .filter_map(|r| {
            let action = match &r.action {
                eggress_routing::RouteActionSpec::Direct => {
                    eggress_routing::RouteActionSpec::Direct
                }
                eggress_routing::RouteActionSpec::UpstreamGroup(gid) => {
                    if group_ids.contains(gid) {
                        eggress_routing::RouteActionSpec::UpstreamGroup(gid.clone())
                    } else {
                        return None;
                    }
                }
                eggress_routing::RouteActionSpec::Reject(reason) => {
                    eggress_routing::RouteActionSpec::Reject(reason.clone())
                }
            };
            Some(eggress_routing::CompiledRule {
                id: r.id.clone(),
                matcher: r.matcher.clone(),
                action,
            })
        })
        .collect();

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

        ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);

        tokio::spawn(async move {
            let started = std::time::Instant::now();
            let config = ConnectionConfig {
                routing: routing as Arc<dyn RouteService>,
                handshake_timeout: Duration::from_secs(30),
                connect_timeout: Duration::from_secs(30),
                protocols: conn_protocols,
                authentication: conn_auth,
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
                    handshake_timeout: Duration::from_secs(5),
                    connect_timeout: Duration::from_secs(10),
                    protocols: Arc::from([eggress_core::ProtocolId::Http]),
                    authentication: eggress_server::accept::InboundAuthentication::None,
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
            reachable: true,
            latency_ms: Some(15),
            error: None,
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("\"reachable\": true"));
        assert!(json.contains("\"latency_ms\": 15"));
        assert!(!json.contains("secret"));
    }
}
