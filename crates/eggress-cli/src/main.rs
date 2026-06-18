use std::net::SocketAddr;
use std::time::Duration;

use clap::Parser;
use eggress_core::listener::{TcpListener, TcpListenerConfig};
use eggress_server::{ConnectionConfig, RouteConfig};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{fmt, EnvFilter};

static CONNECTION_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Eggress - A multi-protocol TCP proxy
#[derive(Parser, Debug)]
#[command(name = "eggress", version, about = "A multi-protocol TCP proxy")]
struct Cli {
    /// Listener URIs (e.g., "http://0.0.0.0:8080", "socks5://0.0.0.0:1080")
    #[arg(short = 'l', long = "listen", value_name = "URI")]
    listeners: Vec<String>,

    /// Upstream proxy URIs (can be specified multiple times, chain with __)
    #[arg(short = 'r', long = "remote", value_name = "URI")]
    upstreams: Vec<String>,
}

fn init_logging() {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .compact()
        .init();
}

#[tokio::main]
async fn main() {
    init_logging();
    let args = Cli::parse();
    let cancel_token = CancellationToken::new();

    {
        let token = cancel_token.clone();
        tokio::spawn(async move {
            let ctrl_c = tokio::signal::ctrl_c();
            #[cfg(unix)]
            {
                let mut sigterm =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("failed to register SIGTERM handler");
                tokio::select! {
                    _ = ctrl_c => {}
                    _ = sigterm.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                ctrl_c.await.ok();
            }
            tracing::info!("shutdown signal received");
            token.cancel();
        });
    }

    let upstream_chain: Option<eggress_uri::ProxyChainSpec> = if args.upstreams.is_empty() {
        None
    } else {
        let combined = args.upstreams.join("__");
        match eggress_uri::parse_proxy_chain(&combined) {
            Ok(spec) => {
                tracing::info!("upstream chain: {}", eggress_uri::RedactedUri::new(&spec));
                Some(spec)
            }
            Err(e) => {
                eprintln!("invalid upstream URI: {e}");
                std::process::exit(1);
            }
        }
    };

    let listener_uris: Vec<String> = if args.listeners.is_empty() {
        vec!["http://127.0.0.1:8080".to_string()]
    } else {
        args.listeners
    };

    let mut handles = Vec::new();

    for uri in &listener_uris {
        let spec = match eggress_uri::parse_proxy_chain(uri) {
            Ok(spec) => spec,
            Err(e) => {
                eprintln!("invalid listener URI '{uri}': {e}");
                std::process::exit(1);
            }
        };

        let first_hop = &spec.hops[0];
        let bind_addr: SocketAddr =
            format!("{}:{}", first_hop.endpoint.host, first_hop.endpoint.port)
                .parse()
                .unwrap_or_else(|e| {
                    eprintln!("invalid listener bind address '{uri}': {e}");
                    std::process::exit(1);
                });

        let protocols: Vec<eggress_core::ProtocolId> = first_hop
            .protocols
            .iter()
            .map(|p| match p {
                eggress_uri::ProtocolSpec::Http => eggress_core::ProtocolId::Http,
                eggress_uri::ProtocolSpec::Socks4 => eggress_core::ProtocolId::Socks4,
                eggress_uri::ProtocolSpec::Socks5 => eggress_core::ProtocolId::Socks5,
            })
            .collect();

        let cancel = cancel_token.clone();
        let chain = upstream_chain.clone();
        let auth = match &first_hop.credentials {
            Some(credentials) => eggress_server::accept::InboundAuthentication::UsernamePassword {
                username: credentials.username.clone(),
                password: credentials.password.clone(),
            },
            None => eggress_server::accept::InboundAuthentication::None,
        };

        let handle = tokio::spawn(async move {
            if let Err(e) = run_listener(bind_addr, protocols, chain, auth, cancel).await {
                tracing::error!("listener error: {e}");
            }
        });
        handles.push(handle);
    }

    tracing::info!("eggress started, {} listener(s)", listener_uris.len());

    tokio::select! {
        _ = cancel_token.cancelled() => {
            tracing::info!("shutting down");
        }
        _ = async {
            for h in handles {
                let _ = h.await;
            }
        } => {}
    }
}

async fn run_listener(
    bind_addr: SocketAddr,
    protocols: Vec<eggress_core::ProtocolId>,
    upstream_chain: Option<eggress_uri::ProxyChainSpec>,
    authentication: eggress_server::accept::InboundAuthentication,
    cancel_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = TcpListenerConfig {
        bind_addr,
        protocols,
        auth_required: false,
        handshake_timeout: Duration::from_secs(30),
        connection_limit: 1024,
    };

    let listener = TcpListener::new(&config, cancel_token.clone()).await?;
    let local_addr = listener.local_addr()?;
    tracing::info!("listening on {local_addr}");

    let protocols: std::sync::Arc<[eggress_core::ProtocolId]> = config.protocols.clone().into();

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

        let chain = upstream_chain.clone();
        let peer = conn.peer_addr;
        let listener = local_addr;
        let conn_id = CONNECTION_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let conn_protocols = protocols.clone();
        let conn_auth = authentication.clone();

        tokio::spawn(async move {
            let started = std::time::Instant::now();
            let route = match chain {
                Some(spec) => RouteConfig::Chain(spec),
                None => RouteConfig::Direct,
            };
            let config = ConnectionConfig {
                route,
                handshake_timeout: Duration::from_secs(30),
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

        let proxy_jh = tokio::spawn(async move {
            loop {
                let conn = match listener.accept().await {
                    Ok(c) => c,
                    Err(_) => break,
                };
                let route = RouteConfig::Direct;
                let config = ConnectionConfig {
                    route,
                    handshake_timeout: Duration::from_secs(5),
                    protocols: std::sync::Arc::from([eggress_core::ProtocolId::Http]),
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
}
