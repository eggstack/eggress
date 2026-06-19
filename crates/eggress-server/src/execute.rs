use crate::accept::{AcceptedSession, PendingHttpForward, PendingTunnel};
use crate::error::SessionOpenError;
use crate::reply;
use crate::ConnectionConfig;
use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::connector::{Connector, DirectConnector};
use eggress_core::relay::relay;
use eggress_core::BoxStream;
use eggress_core::{TargetAddr, TargetHost};
use eggress_routing::{RouteRequest, SelectedRoute};
use tokio::io::AsyncWriteExt;

pub struct SessionReport {
    pub protocol: Option<String>,
    pub target: Option<String>,
    pub route: String,
    pub bytes_upstream: u64,
    pub bytes_downstream: u64,
    pub outcome: SessionOutcome,
    pub failure: Option<FailureCategory>,
    pub rule_id: Option<String>,
    pub upstream_group: Option<String>,
    pub upstream_id: Option<String>,
    pub selection_reason: Option<eggress_routing::SelectionReason>,
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
    RouteHop,
    UpstreamAuthentication,
    PolicyDenied,
    Relay,
    Cancelled,
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
            rule_id: None,
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
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
            rule_id: None,
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
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
            failure: Some(FailureCategory::Cancelled),
            rule_id: None,
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
        }
    }

    pub fn rejected(protocol: Option<String>, target: Option<String>, rule_id: String) -> Self {
        SessionReport {
            protocol,
            target,
            route: "reject".to_string(),
            bytes_upstream: 0,
            bytes_downstream: 0,
            outcome: SessionOutcome::RouteFailed,
            failure: Some(FailureCategory::PolicyDenied),
            rule_id: Some(rule_id),
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
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
            SessionOpenError::Hop { .. } => FailureCategory::RouteHop,
            SessionOpenError::PolicyDenied => FailureCategory::PolicyDenied,
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
    match session {
        AcceptedSession::Tunnel(pending) => {
            let protocol = Some(match pending.protocol {
                crate::accept::TunnelProtocol::HttpConnect => "http".to_string(),
                crate::accept::TunnelProtocol::Socks4 => "socks4".to_string(),
                crate::accept::TunnelProtocol::Socks5 => "socks5".to_string(),
            });
            let target = Some(pending.target.to_string());
            execute_tunnel(pending, config, protocol, target).await
        }
        AcceptedSession::HttpForward(pending) => {
            let target = Some(pending.target.to_string());
            execute_http_forward(pending, config, target).await
        }
    }
}

fn route_description(selected: &SelectedRoute) -> String {
    match selected {
        SelectedRoute::Direct {
            selection_reason, ..
        } => match selection_reason {
            eggress_routing::SelectionReason::DirectFallback => "direct(fallback)".to_string(),
            _ => "direct".to_string(),
        },
        SelectedRoute::Upstream {
            upstream, group, ..
        } => format!("upstream({}/{})", group.0, upstream),
    }
}

fn route_metadata(
    selected: &SelectedRoute,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<eggress_routing::SelectionReason>,
) {
    match selected {
        SelectedRoute::Direct {
            decision,
            selection_reason,
        } => {
            let rule_id = match decision {
                eggress_routing::RouteDecision::Direct { rule, .. }
                | eggress_routing::RouteDecision::UpstreamGroup { rule, .. }
                | eggress_routing::RouteDecision::Reject { rule, .. } => rule.0.to_string(),
            };
            (Some(rule_id), None, None, Some(*selection_reason))
        }
        SelectedRoute::Upstream {
            decision,
            group,
            upstream,
            selection_reason,
            ..
        } => {
            let rule_id = match decision {
                eggress_routing::RouteDecision::Direct { rule, .. }
                | eggress_routing::RouteDecision::UpstreamGroup { rule, .. }
                | eggress_routing::RouteDecision::Reject { rule, .. } => rule.0.to_string(),
            };
            (
                Some(rule_id),
                Some(group.0.to_string()),
                Some(upstream.to_string()),
                Some(*selection_reason),
            )
        }
    }
}

struct OpenedRoute {
    stream: BoxStream,
    active_lease: Option<eggress_routing::lease::ActiveLease>,
    route_description: String,
    rule_id: Option<String>,
    upstream_group: Option<String>,
    upstream_id: Option<String>,
    selection_reason: Option<eggress_routing::SelectionReason>,
}

async fn open_route(
    config: &ConnectionConfig,
    request: &RouteRequest<'_>,
) -> Result<OpenedRoute, SessionOpenError> {
    let selected = config.routing.route(request).map_err(|e| match e {
        eggress_routing::RouteError::Rejected { .. } => SessionOpenError::PolicyDenied,
        eggress_routing::RouteError::NoEligibleUpstream(_)
        | eggress_routing::RouteError::UnknownGroup(_) => SessionOpenError::PolicyDenied,
    })?;

    let route = route_description(&selected);
    let (rule_id, upstream_group, upstream_id, selection_reason) = route_metadata(&selected);

    if let Some(metrics) = &config.metrics {
        let rule_str = rule_id.as_deref().unwrap_or("default");
        let action_str = match &selected {
            SelectedRoute::Direct { .. } => "direct",
            SelectedRoute::Upstream { .. } => "upstream",
        };
        metrics.record_route_decision(rule_str, action_str, "selected");
    }

    let result = tokio::time::timeout(config.connect_timeout, async {
        match selected {
            SelectedRoute::Direct { .. } => {
                let stream = DirectConnector.connect(request.target).await?;
                Ok::<_, SessionOpenError>((stream, None))
            }
            SelectedRoute::Upstream {
                chain,
                pending_lease,
                ..
            } => {
                let executor = build_chain_executor();
                let stream = executor.execute(&chain.hops, request.target).await?;
                let active_lease = pending_lease.established();
                Ok::<_, SessionOpenError>((stream, Some(active_lease)))
            }
        }
    })
    .await;

    match result {
        Ok(Ok((stream, active_lease))) => Ok(OpenedRoute {
            stream,
            active_lease,
            route_description: route,
            rule_id,
            upstream_group,
            upstream_id,
            selection_reason,
        }),
        Ok(Err(e)) => Err(e),
        Err(_timeout) => Err(SessionOpenError::Timeout),
    }
}

/// Execute a tunnel session: open route, send success/failure, relay.
async fn execute_tunnel(
    mut pending: PendingTunnel,
    config: &ConnectionConfig,
    protocol: Option<String>,
    target: Option<String>,
) -> SessionReport {
    tracing::info!("connecting to {}", pending.target);

    let request = RouteRequest {
        target: &pending.target,
        source: config.context.source,
        listener: &config.context.listener,
        inbound_protocol: match pending.protocol {
            crate::accept::TunnelProtocol::HttpConnect => eggress_core::ProtocolId::Http,
            crate::accept::TunnelProtocol::Socks4 => eggress_core::ProtocolId::Socks4,
            crate::accept::TunnelProtocol::Socks5 => eggress_core::ProtocolId::Socks5,
        },
        identity: &pending.identity,
    };

    match open_route(config, &request).await {
        Ok(opened) => {
            let route = opened.route_description;
            let rule_id = opened.rule_id;
            let upstream_group = opened.upstream_group;
            let upstream_id = opened.upstream_id;
            let selection_reason = opened.selection_reason;
            let _active_lease = opened.active_lease;
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
                    rule_id,
                    upstream_group,
                    upstream_id,
                    selection_reason,
                };
            }
            let result = relay(pending.client, opened.stream).await;
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
                    rule_id,
                    upstream_group,
                    upstream_id,
                    selection_reason,
                },
                _ => SessionReport {
                    protocol,
                    target,
                    route,
                    bytes_upstream: result.bytes_upstream,
                    bytes_downstream: result.bytes_downstream,
                    outcome: SessionOutcome::Completed,
                    failure: None,
                    rule_id,
                    upstream_group,
                    upstream_id,
                    selection_reason,
                },
            }
        }
        Err(SessionOpenError::PolicyDenied) => {
            let _ = reply::send_tunnel_failure(&mut pending, &SessionOpenError::PolicyDenied).await;
            SessionReport::rejected(protocol, target, "reject".to_string())
        }
        Err(error) => {
            let _ = reply::send_tunnel_failure(&mut pending, &error).await;
            SessionReport::open_failed(error, protocol, target, "error".to_string())
        }
    }
}

/// Execute an HTTP forward-proxy session.
async fn execute_http_forward(
    mut pending: PendingHttpForward,
    config: &ConnectionConfig,
    target: Option<String>,
) -> SessionReport {
    tracing::info!("forward proxy to {}", pending.target);

    let request = RouteRequest {
        target: &pending.target,
        source: config.context.source,
        listener: &config.context.listener,
        inbound_protocol: eggress_core::ProtocolId::Http,
        identity: &pending.identity,
    };

    match open_route(config, &request).await {
        Ok(mut opened) => {
            let route = opened.route_description;
            let rule_id = opened.rule_id;
            let upstream_group = opened.upstream_group;
            let upstream_id = opened.upstream_id;
            let selection_reason = opened.selection_reason;
            let _active_lease = opened.active_lease;
            let origin_req = eggress_protocol_http::build_origin_request(&pending.request);
            let head_bytes = origin_req.len() as u64;

            if let Err(e) = opened.stream.write_all(origin_req.as_bytes()).await {
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
            if let Err(e) = opened.stream.flush().await {
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

            let body_report = match eggress_protocol_http::copy_request_body(
                &mut pending.client,
                &mut opened.stream,
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
                        rule_id,
                        upstream_group,
                        upstream_id,
                        selection_reason,
                    };
                }
            };

            let bytes_upstream = head_bytes + body_report.wire_bytes;

            let response_report = match eggress_protocol_http::forward_response(
                &mut opened.stream,
                &mut pending.client,
            )
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
                        rule_id,
                        upstream_group,
                        upstream_id,
                        selection_reason,
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
                rule_id,
                upstream_group,
                upstream_id,
                selection_reason,
            }
        }
        Err(SessionOpenError::PolicyDenied) => {
            let _ = reply::send_http_forward_failure(
                &mut pending.client,
                &SessionOpenError::PolicyDenied,
            )
            .await;
            SessionReport::rejected(None, target, "reject".to_string())
        }
        Err(error) => {
            let _ = reply::send_http_forward_failure(&mut pending.client, &error).await;
            SessionReport::open_failed(error, None, target, "error".to_string())
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
