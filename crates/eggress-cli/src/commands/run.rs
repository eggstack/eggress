//! Native proxy startup: CLI listeners, router construction, signal
//! handling, and connection drain.
//!
//! Listener sockets are bound before any accept task spawns so a bind
//! failure (address in use, permission denied) fails closed with
//! [`eggress_cli::EXIT_BIND_FAILURE`] instead of leaving a partially
//! listening process running.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eggress_cli::{
    EXIT_BIND_FAILURE, EXIT_CLI_PARSE_ERROR, EXIT_CONFIG_VALIDATION, EXIT_RUNTIME_FAILURE,
    EXIT_SIGINT, EXIT_SIGTERM, EXIT_SUCCESS,
};
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_routing::{RouteActionSpec, RouteService, Router, SharedRoutingService};
use eggress_server::ConnectionConfig;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::cli::{Cli, CliContext};

static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(1);
static ACTIVE_CONNECTIONS: AtomicU64 = AtomicU64::new(0);
/// Notified when an in-flight connection finishes so the shutdown drain loop
/// can react without polling the counter on a 100 ms timer.
static ACTIVE_CONNECTIONS_DRAIN: std::sync::LazyLock<tokio::sync::Notify> =
    std::sync::LazyLock::new(tokio::sync::Notify::new);

/// Start the native proxy from top-level `-l`/`-r` flags or `--config`.
/// Returns the process exit code.
pub async fn handle_native_startup(args: Cli, ctx: CliContext) -> i32 {
    if args.config.is_some() && (!args.listeners.is_empty() || !args.upstreams.is_empty()) {
        eprintln!("--config mode is incompatible with -l and -r flags. Use one or the other.");
        return EXIT_CLI_PARSE_ERROR;
    }

    if let Some(ref config_path) = ctx.config {
        crate::logging::init_logging(ctx.log_format);
        match eggress_runtime::ServiceSupervisor::start(config_path) {
            Ok(mut supervisor) => {
                if let Err(e) = supervisor.run() {
                    eprintln!("runtime error: {e}");
                    return EXIT_RUNTIME_FAILURE;
                }
            }
            Err(e) => {
                eprintln!("runtime error: {e}");
                return EXIT_RUNTIME_FAILURE;
            }
        }
        return EXIT_SUCCESS;
    }

    crate::logging::init_logging(ctx.log_format);
    let cancel_token = CancellationToken::new();

    let router = match build_router_from_cli(&args) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return EXIT_CONFIG_VALIDATION;
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
                eprintln!(
                    "invalid listener URI '{}': {e}",
                    eggress_uri::redact_proxy_uri(uri)
                );
                return EXIT_CLI_PARSE_ERROR;
            }
        }
    }

    // Bind every listener before spawning any accept loop. A bind failure
    // exits instead of serving a subset of the requested listeners.
    let mut bound = Vec::new();
    for (uri, spec) in &listener_specs {
        let config = TcpListenerConfig {
            bind_addr: spec.bind_addr,
            protocols: spec.protocols.clone(),
            auth_required: false,
            handshake_timeout: Duration::from_secs(30),
            connection_limit: 1024,
        };
        match TcpListener::new(&config, cancel_token.clone()).await {
            Ok(listener) => bound.push((
                uri.clone(),
                listener,
                spec.protocols.clone(),
                spec.auth.clone(),
            )),
            Err(e) => {
                eprintln!(
                    "failed to bind listener '{}': {e}",
                    eggress_uri::redact_proxy_uri(uri)
                );
                return EXIT_BIND_FAILURE;
            }
        }
    }

    let mut handles = Vec::new();

    for (uri, listener, protocols, auth) in bound {
        let routing = routing_service.clone();
        let metrics = metrics.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = serve_listener(listener, protocols, routing, auth, metrics).await {
                tracing::error!(
                    "listener '{}' error: {e}",
                    eggress_uri::redact_proxy_uri(&uri)
                );
            }
        });
        handles.push(handle);
    }

    tracing::info!("eggress started, {} listener(s)", listener_specs.len());

    let mut shutdown_handles = handles;

    let shutdown_exit_code: i32;

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

            let exit;
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        tracing::info!("shutdown signal received");
                        token.cancel();
                        exit = EXIT_SIGINT;
                        break;
                    }
                    _ = async { sigterm.as_mut().ok()?.recv().await }, if sigterm.is_ok() => {
                        tracing::info!("shutdown signal received");
                        token.cancel();
                        exit = EXIT_SIGTERM;
                        break;
                    }
                    _ = async { sighup.as_mut().ok()?.recv().await }, if sighup.is_ok() => {
                        tracing::warn!("SIGHUP received but no config file specified in compatibility mode, ignoring");
                    }
                }
            }
            shutdown_exit_code = exit;
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("shutdown signal received");
            token.cancel();
            shutdown_exit_code = EXIT_SIGINT;
        }
    }

    tracing::info!("draining active connections");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let notified = ACTIVE_CONNECTIONS_DRAIN.notified();
        let active = ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
        if active == 0 {
            tracing::info!("all connections drained");
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(active, "drain timeout reached, forcing shutdown");
            break;
        }
        // Wake immediately when a connection completes so the drain doesn't
        // have to wait out a full tick to notice progress.
        notified.await;
    }

    for h in shutdown_handles.drain(..) {
        let _ = h.await;
    }

    tracing::info!("eggress stopped");
    shutdown_exit_code
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

    // Central typed conversion: exhaustive over `ProtocolSpec`, with
    // upstream-only transports failing explicitly (see
    // `ProtocolId::from_protocol_spec`).
    let mut protocols: Vec<eggress_core::ProtocolId> =
        Vec::with_capacity(first_hop.protocols.len());
    for p in &first_hop.protocols {
        let id = eggress_core::ProtocolId::from_protocol_spec(*p).map_err(|e| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                e.to_string(),
            )) as Box<dyn std::error::Error + Send + Sync>
        })?;
        protocols.push(id);
    }

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

async fn serve_listener(
    listener: TcpListener,
    protocols: Vec<eggress_core::ProtocolId>,
    routing: Arc<SharedRoutingService>,
    authentication: eggress_server::accept::InboundAuthentication,
    metrics: Arc<dyn eggress_server::SessionMetrics>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let local_addr = listener.local_addr()?;
    tracing::info!("listening on {local_addr}");

    let proto_slice: Arc<[eggress_core::ProtocolId]> = protocols.into();

    // Back off when accept() keeps failing (e.g. fd exhaustion) so the loop
    // does not tight-spin while the system is resource-starved.
    const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(100);

    loop {
        let conn = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                if eggress_core::listener::is_listener_cancelled(&e) {
                    break;
                }
                match e.kind() {
                    std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::ConnectionAborted => {}
                    _ => tokio::time::sleep(ACCEPT_ERROR_BACKOFF).await,
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
                trojan: None,
                fixed_target: None,
                local_bind: None,
                #[cfg(feature = "ssh")]
                ssh_sessions: None,
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
            ACTIVE_CONNECTIONS_DRAIN.notify_one();

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
                    trojan: None,
                    fixed_target: None,
                    local_bind: None,
                    #[cfg(feature = "ssh")]
                    ssh_sessions: None,
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

    #[test]
    fn listener_uri_redaction_hides_password() {
        // Regression for credential leak in listener parse/runtime error logs:
        // both sites must render through redact_proxy_uri.
        let uri = "socks5://secret_user:super_secret_password_123@127.0.0.1:1080";
        let redacted = eggress_uri::redact_proxy_uri(uri);
        assert!(
            !redacted.contains("super_secret_password_123"),
            "redacted listener URI leaked password: {redacted}"
        );
        assert!(
            !redacted.contains("secret_user"),
            "redacted listener URI leaked username: {redacted}"
        );
        assert!(redacted.contains("127.0.0.1:1080"));
    }
}
