use crate::accept::{AcceptedSession, PendingHttpForward, PendingTunnel};
use crate::error::SessionOpenError;
use crate::reply;
use crate::{ConnectionConfig, RouteConfig};
use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::connector::{Connector, DirectConnector};
use eggress_core::relay::relay;
use eggress_core::BoxStream;
use eggress_core::{TargetAddr, TargetHost};
use tokio::io::AsyncWriteExt;

/// Report from a completed session.
pub struct SessionReport {
    pub protocol: Option<String>,
    pub target: Option<String>,
    pub route: String,
    pub bytes_upstream: u64,
    pub bytes_downstream: u64,
    pub outcome: SessionOutcome,
    pub failure: Option<FailureCategory>,
}

/// Outcome of a session.
#[derive(Debug)]
pub enum SessionOutcome {
    Completed,
    ClientProtocolError,
    AuthenticationFailed,
    HandshakeTimedOut,
    RouteFailed,
    RelayFailed,
    Cancelled,
}

/// Specific failure category for structured diagnostics and metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    Protocol,
    Authentication,
    HandshakeTimeout,
    Dns,
    ConnectionRefused,
    NetworkUnreachable,
    HostUnreachable,
    RouteTimeout,
    UpstreamAuthentication,
    Relay,
    Internal,
}

impl SessionReport {
    pub fn open_failed(
        error: SessionOpenError,
        protocol: Option<String>,
        target: Option<String>,
        route: String,
    ) -> Self {
        SessionReport {
            protocol,
            target,
            route,
            bytes_upstream: 0,
            bytes_downstream: 0,
            outcome: SessionOutcome::RouteFailed,
            failure: Some(FailureCategory::from(&error)),
        }
    }

    pub fn completed(
        protocol: Option<String>,
        target: Option<String>,
        route: String,
        bytes_upstream: u64,
        bytes_downstream: u64,
    ) -> Self {
        SessionReport {
            protocol,
            target,
            route,
            bytes_upstream,
            bytes_downstream,
            outcome: SessionOutcome::Completed,
            failure: None,
        }
    }

    pub fn cancelled(protocol: Option<String>, target: Option<String>, route: String) -> Self {
        SessionReport {
            protocol,
            target,
            route,
            bytes_upstream: 0,
            bytes_downstream: 0,
            outcome: SessionOutcome::Cancelled,
            failure: None,
        }
    }
}

impl From<&SessionOpenError> for FailureCategory {
    fn from(error: &SessionOpenError) -> Self {
        match error {
            SessionOpenError::Dns => FailureCategory::Dns,
            SessionOpenError::Refused => FailureCategory::ConnectionRefused,
            SessionOpenError::NetworkUnreachable => FailureCategory::NetworkUnreachable,
            SessionOpenError::HostUnreachable => FailureCategory::HostUnreachable,
            SessionOpenError::Timeout => FailureCategory::RouteTimeout,
            SessionOpenError::UpstreamAuthentication => FailureCategory::UpstreamAuthentication,
            SessionOpenError::Hop { .. } => FailureCategory::Relay,
            SessionOpenError::PolicyDenied => FailureCategory::Internal,
            SessionOpenError::Other(_) => FailureCategory::Relay,
        }
    }
}

impl FailureCategory {
    pub fn from_io_error(error: &std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::ConnectionRefused => FailureCategory::ConnectionRefused,
            std::io::ErrorKind::ConnectionReset => FailureCategory::Relay,
            std::io::ErrorKind::TimedOut => FailureCategory::Relay,
            _ => FailureCategory::Relay,
        }
    }
}

/// Execute a session from an accepted connection.
pub async fn execute(session: AcceptedSession, config: &ConnectionConfig) -> SessionReport {
    let route_str = match &config.route {
        RouteConfig::Direct => "direct".to_string(),
        RouteConfig::Chain(spec) => format!("chain({})", spec.hops.len()),
    };

    match session {
        AcceptedSession::Tunnel(pending) => {
            let protocol = Some(match pending.protocol {
                crate::accept::TunnelProtocol::HttpConnect => "http".to_string(),
                crate::accept::TunnelProtocol::Socks4 => "socks4".to_string(),
                crate::accept::TunnelProtocol::Socks5 => "socks5".to_string(),
            });
            let target = Some(pending.target.to_string());
            execute_tunnel(pending, config, protocol, target, route_str).await
        }
        AcceptedSession::HttpForward(pending) => {
            let target = Some(pending.target.to_string());
            execute_http_forward(pending, config, target, route_str).await
        }
    }
}

/// Open a route to the target using the configured route.
async fn open_route(
    route: &RouteConfig,
    target: &TargetAddr,
) -> Result<BoxStream, SessionOpenError> {
    match route {
        RouteConfig::Direct => DirectConnector.connect(target).await.map_err(Into::into),
        RouteConfig::Chain(spec) => {
            let executor = build_chain_executor();
            executor
                .execute(&spec.hops, target)
                .await
                .map_err(Into::into)
        }
    }
}

/// Execute a tunnel session: open route, send success/failure, relay.
async fn execute_tunnel(
    mut pending: PendingTunnel,
    config: &ConnectionConfig,
    protocol: Option<String>,
    target: Option<String>,
    route: String,
) -> SessionReport {
    tracing::info!("connecting to {}", pending.target);

    match open_route(&config.route, &pending.target).await {
        Ok(upstream) => {
            if let Err(e) = reply::send_tunnel_success(&mut pending, None).await {
                tracing::debug!("failed to send success reply: {e}");
                return SessionReport {
                    protocol,
                    target,
                    route,
                    bytes_upstream: 0,
                    bytes_downstream: 0,
                    outcome: SessionOutcome::ClientProtocolError,
                    failure: Some(FailureCategory::Protocol),
                };
            }
            let result = relay(pending.client, upstream).await;
            tracing::debug!(
                "relay complete: upstream={}B downstream={}B reason={:?}",
                result.bytes_upstream,
                result.bytes_downstream,
                result.termination_reason
            );
            match result.termination_reason {
                eggress_core::relay::TerminationReason::Error => SessionReport {
                    protocol,
                    target,
                    route,
                    bytes_upstream: result.bytes_upstream,
                    bytes_downstream: result.bytes_downstream,
                    outcome: SessionOutcome::RelayFailed,
                    failure: Some(FailureCategory::Relay),
                },
                _ => SessionReport {
                    protocol,
                    target,
                    route,
                    bytes_upstream: result.bytes_upstream,
                    bytes_downstream: result.bytes_downstream,
                    outcome: SessionOutcome::Completed,
                    failure: None,
                },
            }
        }
        Err(error) => {
            let _ = reply::send_tunnel_failure(&mut pending, &error).await;
            SessionReport::open_failed(error, protocol, target, route)
        }
    }
}

/// Execute an HTTP forward-proxy session.
async fn execute_http_forward(
    mut pending: PendingHttpForward,
    config: &ConnectionConfig,
    target: Option<String>,
    route: String,
) -> SessionReport {
    tracing::info!("forward proxy to {}", pending.target);

    match open_route(&config.route, &pending.target).await {
        Ok(mut upstream) => {
            // Build origin-form request and send to upstream
            let origin_req = eggress_protocol_http::build_origin_request(&pending.request);
            let head_bytes = origin_req.len() as u64;

            if let Err(e) = upstream.write_all(origin_req.as_bytes()).await {
                let _ = reply::send_http_forward_failure(
                    &mut pending.client,
                    &SessionOpenError::Other(e.to_string()),
                )
                .await;
                return SessionReport::open_failed(
                    SessionOpenError::Other(e.to_string()),
                    None,
                    target,
                    route,
                );
            }
            if let Err(e) = upstream.flush().await {
                let _ = reply::send_http_forward_failure(
                    &mut pending.client,
                    &SessionOpenError::Other(e.to_string()),
                )
                .await;
                return SessionReport::open_failed(
                    SessionOpenError::Other(e.to_string()),
                    None,
                    target.clone(),
                    route.clone(),
                );
            }

            // Forward body if present
            let body_report = match eggress_protocol_http::copy_request_body(
                &mut pending.client,
                &mut upstream,
                pending.body_kind,
                &eggress_protocol_http::BodyCopyLimits::default(),
            )
            .await
            {
                Ok(report) => report,
                Err(_e) => {
                    return SessionReport {
                        protocol: None,
                        target,
                        route,
                        bytes_upstream: 0,
                        bytes_downstream: 0,
                        outcome: SessionOutcome::ClientProtocolError,
                        failure: Some(FailureCategory::Protocol),
                    };
                }
            };

            let bytes_upstream = head_bytes + body_report.wire_bytes;

            // Forward the upstream response back to the client
            let response_report =
                match eggress_protocol_http::forward_response(&mut upstream, &mut pending.client)
                    .await
                {
                    Ok(report) => report,
                    Err(_e) => {
                        return SessionReport {
                            protocol: None,
                            target,
                            route,
                            bytes_upstream,
                            bytes_downstream: 0,
                            outcome: SessionOutcome::RelayFailed,
                            failure: Some(FailureCategory::Relay),
                        };
                    }
                };

            SessionReport {
                protocol: None,
                target,
                route,
                bytes_upstream,
                bytes_downstream: response_report.bytes_forwarded,
                outcome: SessionOutcome::Completed,
                failure: None,
            }
        }
        Err(error) => {
            let _ = reply::send_http_forward_failure(&mut pending.client, &error).await;
            SessionReport::open_failed(error, None, target, route)
        }
    }
}

type HandshakeFuture<'a> = std::pin::Pin<
    Box<
        dyn std::future::Future<
                Output = Result<BoxStream, Box<dyn std::error::Error + Send + Sync>>,
            > + Send
            + 'a,
    >,
>;

fn build_chain_executor() -> ChainExecutor {
    let handlers: Vec<Box<dyn HopHandler>> = vec![
        Box::new(HttpHopHandler),
        Box::new(Socks5HopHandler),
        Box::new(Socks4HopHandler),
    ];
    ChainExecutor::new(handlers)
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
        credentials: Option<&'a eggress_uri::CredentialSpec>,
    ) -> HandshakeFuture<'a> {
        let auth = credentials.map(|c| (c.username.as_str(), c.password.as_str()));
        Box::pin(async move {
            eggress_protocol_http::http_connect(stream, target, auth)
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
        credentials: Option<&'a eggress_uri::CredentialSpec>,
    ) -> HandshakeFuture<'a> {
        let socks_addr = target_to_socks_addr(target);
        let auth = credentials.map(|c| (c.username.as_str(), c.password.as_str()));
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
        credentials: Option<&'a eggress_uri::CredentialSpec>,
    ) -> HandshakeFuture<'a> {
        let user_id = credentials.map(|c| c.username.as_str());
        Box::pin(async move {
            eggress_protocol_socks::socks4_connect(stream, target, user_id)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

fn target_to_socks_addr(target: &TargetAddr) -> eggress_protocol_socks::socks5::server::SocksAddr {
    use eggress_protocol_socks::socks5::server::SocksAddr;
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => SocksAddr::IPv4(ip.octets(), target.port),
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => SocksAddr::IPv6(ip.octets(), target.port),
        TargetHost::Domain(d) => SocksAddr::Domain(d.clone(), target.port),
    }
}
