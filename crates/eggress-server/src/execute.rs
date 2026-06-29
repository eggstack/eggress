use crate::accept::{AcceptedSession, PendingHttpForward, PendingTunnel, PendingUdpAssociate};
use crate::error::SessionOpenError;
use crate::reply;
use crate::ConnectionConfig;
use eggress_core::chain::{ChainExecutor, HopHandler};
use eggress_core::connector::{Connector, DirectConnector};
use eggress_core::relay::relay;
use eggress_core::BoxStream;
use eggress_core::{TargetAddr, TargetHost};
use eggress_routing::{RouteRequest, SelectedRoute};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        AcceptedSession::UdpAssociate(pending) => execute_udp_associate(pending, config).await,
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

fn upstream_protocol_label(chain: &eggress_uri::ProxyChainSpec) -> &'static str {
    chain
        .hops
        .first()
        .and_then(|h| h.protocols.first())
        .map(|p| match p {
            eggress_uri::ProtocolSpec::Http => "http",
            eggress_uri::ProtocolSpec::Socks4 => "socks4",
            eggress_uri::ProtocolSpec::Socks5 => "socks5",
            eggress_uri::ProtocolSpec::Shadowsocks => "shadowsocks",
            eggress_uri::ProtocolSpec::Trojan => "trojan",
        })
        .unwrap_or("unknown")
}

fn failure_reason_label(error: &SessionOpenError) -> &'static str {
    match error {
        SessionOpenError::Dns => "dns",
        SessionOpenError::Refused => "connection_refused",
        SessionOpenError::NetworkUnreachable => "network_unreachable",
        SessionOpenError::HostUnreachable => "host_unreachable",
        SessionOpenError::Timeout => "timeout",
        SessionOpenError::UpstreamAuthentication => "auth_failed",
        SessionOpenError::PolicyDenied => "policy_denied",
        SessionOpenError::Hop { .. } => "handshake",
        SessionOpenError::Other(_) => "io",
    }
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

    let upstream_protocol = match &selected {
        SelectedRoute::Upstream { chain, .. } => Some(upstream_protocol_label(chain)),
        SelectedRoute::Direct { .. } => None,
    };

    let tls_override = config.tls_client_config.as_ref();

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
                let executor = build_chain_executor(tls_override);
                let stream = executor.execute(&chain.hops, request.target).await?;
                let active_lease = pending_lease.established();
                Ok::<_, SessionOpenError>((stream, Some(active_lease)))
            }
        }
    })
    .await;

    match result {
        Ok(Ok((stream, active_lease))) => {
            if let (Some(metrics), Some(protocol)) = (&config.metrics, upstream_protocol) {
                metrics.record_upstream_open(protocol, "success");
            }
            Ok(OpenedRoute {
                stream,
                active_lease,
                route_description: route,
                rule_id,
                upstream_group,
                upstream_id,
                selection_reason,
            })
        }
        Ok(Err(e)) => {
            if let (Some(metrics), Some(protocol)) = (&config.metrics, upstream_protocol) {
                metrics.record_upstream_failure(protocol, failure_reason_label(&e));
            }
            Err(e)
        }
        Err(_timeout) => {
            if let Some(metrics) = &config.metrics {
                if let Some(protocol) = upstream_protocol {
                    metrics.record_upstream_failure(protocol, "timeout");
                }
            }
            Err(SessionOpenError::Timeout)
        }
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
        transport: eggress_routing::TransportKind::Tcp,
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

/// Execute an HTTP forward-proxy session with persistent connection support.
///
/// Loops over requests on the client connection, forwarding each to the
/// appropriate upstream. Supports HTTP/1.1 keep-alive semantics: the
/// connection persists until the client sends `Connection: close` or the
/// upstream signals close.
#[allow(unused_assignments)]
async fn execute_http_forward(
    pending: PendingHttpForward,
    config: &ConnectionConfig,
    _target: Option<String>,
) -> SessionReport {
    tracing::info!("forward proxy to {}", pending.target);

    let mut client = pending.client;
    let mut total_bytes_upstream: u64 = 0;
    let mut total_bytes_downstream: u64 = 0;
    let mut last_target: Option<String> = None;
    let mut last_rule_id: Option<String> = None;
    let mut last_upstream_group: Option<String> = None;
    let mut last_upstream_id: Option<String> = None;
    let mut last_selection_reason: Option<eggress_routing::SelectionReason> = None;
    let mut last_route = String::new();

    // Process the first request (already parsed in pending)
    let mut request = pending.request;
    let mut client_close = request.connection_close;

    loop {
        let target_addr = request.target.clone();
        last_target = Some(target_addr.to_string());

        let route_request = RouteRequest {
            target: &target_addr,
            source: config.context.source,
            listener: &config.context.listener,
            inbound_protocol: eggress_core::ProtocolId::Http,
            identity: &pending.identity,
            transport: eggress_routing::TransportKind::Tcp,
        };

        match open_route(config, &route_request).await {
            Ok(mut opened) => {
                last_route = opened.route_description;
                last_rule_id = opened.rule_id;
                last_upstream_group = opened.upstream_group;
                last_upstream_id = opened.upstream_id;
                last_selection_reason = opened.selection_reason;
                let _active_lease = opened.active_lease;

                let origin_req = eggress_protocol_http::build_origin_request(&request);
                let head_bytes = origin_req.len() as u64;

                if let Err(e) = opened.stream.write_all(origin_req.as_bytes()).await {
                    let _ = reply::send_http_forward_failure(
                        &mut client,
                        &SessionOpenError::Other(e.to_string()),
                    )
                    .await;
                    return SessionReport {
                        protocol: None,
                        target: last_target,
                        route: last_route,
                        bytes_upstream: total_bytes_upstream + head_bytes,
                        bytes_downstream: total_bytes_downstream,
                        outcome: SessionOutcome::RelayFailed,
                        failure: Some(FailureCategory::Relay),
                        rule_id: last_rule_id,
                        upstream_group: last_upstream_group,
                        upstream_id: last_upstream_id,
                        selection_reason: last_selection_reason,
                    };
                }
                if let Err(e) = opened.stream.flush().await {
                    let _ = reply::send_http_forward_failure(
                        &mut client,
                        &SessionOpenError::Other(e.to_string()),
                    )
                    .await;
                    return SessionReport {
                        protocol: None,
                        target: last_target,
                        route: last_route,
                        bytes_upstream: total_bytes_upstream + head_bytes,
                        bytes_downstream: total_bytes_downstream,
                        outcome: SessionOutcome::RelayFailed,
                        failure: Some(FailureCategory::Relay),
                        rule_id: last_rule_id,
                        upstream_group: last_upstream_group,
                        upstream_id: last_upstream_id,
                        selection_reason: last_selection_reason,
                    };
                }

                let body_report = match eggress_protocol_http::copy_request_body(
                    &mut client,
                    &mut opened.stream,
                    request.body_kind(),
                    &eggress_protocol_http::BodyCopyLimits::default(),
                )
                .await
                {
                    Ok(report) => report,
                    Err(_e) => {
                        return SessionReport {
                            protocol: None,
                            target: last_target,
                            route: last_route,
                            bytes_upstream: total_bytes_upstream + head_bytes,
                            bytes_downstream: total_bytes_downstream,
                            outcome: SessionOutcome::ClientProtocolError,
                            failure: Some(FailureCategory::Protocol),
                            rule_id: last_rule_id,
                            upstream_group: last_upstream_group,
                            upstream_id: last_upstream_id,
                            selection_reason: last_selection_reason,
                        };
                    }
                };

                total_bytes_upstream += head_bytes + body_report.wire_bytes;

                let forward_result =
                    match eggress_protocol_http::forward_response(&mut opened.stream, &mut client)
                        .await
                    {
                        Ok(result) => result,
                        Err(_e) => {
                            return SessionReport {
                                protocol: None,
                                target: last_target,
                                route: last_route,
                                bytes_upstream: total_bytes_upstream,
                                bytes_downstream: total_bytes_downstream,
                                outcome: SessionOutcome::RelayFailed,
                                failure: Some(FailureCategory::Relay),
                                rule_id: last_rule_id,
                                upstream_group: last_upstream_group,
                                upstream_id: last_upstream_id,
                                selection_reason: last_selection_reason,
                            };
                        }
                    };

                total_bytes_downstream += forward_result.report.bytes_forwarded;

                // Determine whether to continue the session
                let should_close = client_close
                    || forward_result.client_should_close
                    || !forward_result.upstream_alive;

                if should_close {
                    break;
                }

                // Read the next request from the client
                match eggress_protocol_http::forward_request_stream(&mut client).await {
                    Ok(next_request) => {
                        client_close = next_request.connection_close;
                        request = next_request;
                    }
                    Err(eggress_protocol_http::HttpError::Io(ref e))
                        if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                    {
                        // Client closed the connection
                        break;
                    }
                    Err(_) => {
                        // Malformed next request — close with error
                        break;
                    }
                }
            }
            Err(SessionOpenError::PolicyDenied) => {
                let _ =
                    reply::send_http_forward_failure(&mut client, &SessionOpenError::PolicyDenied)
                        .await;
                return SessionReport::rejected(None, last_target, "reject".to_string());
            }
            Err(error) => {
                let _ = reply::send_http_forward_failure(&mut client, &error).await;
                return SessionReport::open_failed(error, None, last_target, "error".to_string());
            }
        }
    }

    SessionReport {
        protocol: None,
        target: last_target,
        route: last_route,
        bytes_upstream: total_bytes_upstream,
        bytes_downstream: total_bytes_downstream,
        outcome: SessionOutcome::Completed,
        failure: None,
        rule_id: last_rule_id,
        upstream_group: last_upstream_group,
        upstream_id: last_upstream_id,
        selection_reason: last_selection_reason,
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

async fn execute_udp_associate(
    pending: PendingUdpAssociate,
    config: &ConnectionConfig,
) -> SessionReport {
    let protocol = Some("socks5".to_string());

    let udp_service = match &config.udp {
        Some(svc) if svc.is_enabled() => svc,
        _ => {
            tracing::debug!("UDP ASSOCIATE rejected: UDP service not available");
            let mut stream = pending.client;
            let target = pending.client_hint.unwrap_or(TargetAddr {
                host: TargetHost::Ip(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
                port: 0,
            });
            let socks_addr = target_to_socks_addr(&target);
            let _ = eggress_protocol_socks::socks5::server::send_connect_reply(
                &mut stream,
                eggress_protocol_socks::socks5::server::REP_NOT_ALLOWED,
                &socks_addr,
            )
            .await;
            return SessionReport {
                protocol,
                target: None,
                route: "udp_associate_disabled".to_string(),
                bytes_upstream: 0,
                bytes_downstream: 0,
                outcome: SessionOutcome::RouteFailed,
                failure: Some(FailureCategory::Protocol),
                rule_id: None,
                upstream_group: None,
                upstream_id: None,
                selection_reason: None,
            };
        }
    };

    let client_tcp_peer = config
        .context
        .source
        .unwrap_or_else(|| "127.0.0.1:0".parse().unwrap());

    let gen = config.context.generation;

    let handle = match tokio::time::timeout(
        config.connect_timeout,
        udp_service.create_association(
            &config.context.listener,
            client_tcp_peer,
            pending.identity.clone(),
            gen,
        ),
    )
    .await
    {
        Ok(Ok(handle)) => handle,
        Ok(Err(e)) => {
            tracing::debug!("UDP ASSOCIATE failed: {e}");
            let mut stream = pending.client;
            let target = pending.client_hint.unwrap_or(TargetAddr {
                host: TargetHost::Ip(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
                port: 0,
            });
            let socks_addr = target_to_socks_addr(&target);
            let _ = eggress_protocol_socks::socks5::server::send_connect_reply(
                &mut stream,
                eggress_protocol_socks::socks5::server::REP_GENERAL_FAILURE,
                &socks_addr,
            )
            .await;
            return SessionReport {
                protocol,
                target: None,
                route: "udp_associate_failed".to_string(),
                bytes_upstream: 0,
                bytes_downstream: 0,
                outcome: SessionOutcome::RouteFailed,
                failure: Some(FailureCategory::Protocol),
                rule_id: None,
                upstream_group: None,
                upstream_id: None,
                selection_reason: None,
            };
        }
        Err(_) => {
            tracing::debug!("UDP ASSOCIATE failed: timeout");
            let mut stream = pending.client;
            let target = pending.client_hint.unwrap_or(TargetAddr {
                host: TargetHost::Ip(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
                port: 0,
            });
            let socks_addr = target_to_socks_addr(&target);
            let _ = eggress_protocol_socks::socks5::server::send_connect_reply(
                &mut stream,
                eggress_protocol_socks::socks5::server::REP_GENERAL_FAILURE,
                &socks_addr,
            )
            .await;
            return SessionReport {
                protocol,
                target: None,
                route: "udp_associate_timeout".to_string(),
                bytes_upstream: 0,
                bytes_downstream: 0,
                outcome: SessionOutcome::HandshakeTimedOut,
                failure: Some(FailureCategory::RouteTimeout),
                rule_id: None,
                upstream_group: None,
                upstream_id: None,
                selection_reason: None,
            };
        }
    };

    let relay_ip = handle.relay_addr.ip();
    let relay_port = handle.relay_addr.port();
    let socks_addr = match relay_ip {
        std::net::IpAddr::V4(ip) => {
            eggress_protocol_socks::socks5::server::SocksAddr::IPv4(ip.octets(), relay_port)
        }
        std::net::IpAddr::V6(ip) => {
            eggress_protocol_socks::socks5::server::SocksAddr::IPv6(ip.octets(), relay_port)
        }
    };

    let mut stream = pending.client;
    if let Err(e) =
        eggress_protocol_socks::socks5::server::send_udp_associate_reply(&mut stream, &socks_addr)
            .await
    {
        tracing::debug!("failed to send UDP ASSOCIATE reply: {e}");
        handle.cancel.cancel();
        return SessionReport {
            protocol,
            target: None,
            route: "udp_associate_reply_failed".to_string(),
            bytes_upstream: 0,
            bytes_downstream: 0,
            outcome: SessionOutcome::ClientProtocolError,
            failure: Some(FailureCategory::Protocol),
            rule_id: None,
            upstream_group: None,
            upstream_id: None,
            selection_reason: None,
        };
    }

    tracing::info!(
        association_id = ?handle.id,
        relay_addr = %handle.relay_addr,
        "UDP ASSOCIATE established, keeping TCP control connection alive"
    );

    let mut buf = [0u8; 1];
    tokio::select! {
        result = stream.read_exact(&mut buf) => {
            match result {
                Ok(_) => {
                    tracing::debug!(
                        association_id = ?handle.id,
                        "TCP control connection closed by client"
                    );
                }
                Err(_) => {
                    tracing::debug!(
                        association_id = ?handle.id,
                        "TCP control connection read failed"
                    );
                }
            }
        }
        _ = handle.cancel.cancelled() => {
            tracing::debug!(
                association_id = ?handle.id,
                "UDP association cancelled"
            );
        }
    }

    handle.cancel.cancel();

    SessionReport {
        protocol,
        target: None,
        route: "udp_associate".to_string(),
        bytes_upstream: 0,
        bytes_downstream: 0,
        outcome: SessionOutcome::Completed,
        failure: None,
        rule_id: None,
        upstream_group: None,
        upstream_id: None,
        selection_reason: None,
    }
}

fn build_chain_executor(
    tls_override: Option<&std::sync::Arc<rustls::ClientConfig>>,
) -> ChainExecutor {
    // Build shared TLS client config for upstream hops
    let shared_tls_config = match tls_override {
        Some(config) => Some(config.clone()),
        None => {
            let builder = eggress_transport_tls::TlsClientConfigBuilder::new();
            match builder.with_system_roots().and_then(|b| b.build()) {
                Ok(config) => Some(config),
                Err(e) => {
                    tracing::warn!("failed to build shared TLS config: {e}");
                    None
                }
            }
        }
    };

    let shared_tls_config_arc = shared_tls_config.clone();

    let handlers: Vec<Box<dyn HopHandler>> = vec![
        Box::new(HttpHopHandler),
        Box::new(Socks5HopHandler),
        Box::new(Socks4HopHandler),
        Box::new(ShadowsocksHopHandler),
        Box::new(TrojanHopHandler {
            tls_config: shared_tls_config_arc,
        }),
    ];

    // Set up TLS wrapper using system roots by default, or the override if provided
    let tls_wrapper_override = tls_override.cloned();
    let tls_wrapper: eggress_core::chain::TlsWrapper = Box::new(move |stream, server_name| {
        let config_override = tls_wrapper_override.clone();
        Box::pin(async move {
            let config = match config_override {
                Some(c) => c,
                None => {
                    let builder = eggress_transport_tls::TlsClientConfigBuilder::new();
                    builder
                        .with_system_roots()
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                            Box::new(e) as _
                        })?
                        .build()
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                            Box::new(e) as _
                        })?
                }
            };
            eggress_transport_tls::tls_connect(stream, config, &server_name)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) as _ })
        })
    });

    ChainExecutor::new(handlers)
        .with_tls_wrapper(tls_wrapper)
        .with_shared_tls_config(shared_tls_config)
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
    ) -> HandshakeFuture<'a> {
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
    ) -> HandshakeFuture<'a> {
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
    ) -> HandshakeFuture<'a> {
        let user_id = hop.credentials.as_ref().map(|c| c.username.as_str());
        Box::pin(async move {
            eggress_protocol_socks::socks4_connect(stream, target, user_id)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

struct ShadowsocksHopHandler;

impl HopHandler for ShadowsocksHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Shadowsocks
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
    ) -> HandshakeFuture<'a> {
        Box::pin(async move {
            let creds = hop.credentials.as_ref().ok_or_else(|| {
                Box::new(eggress_protocol_shadowsocks::ShadowsocksError::Other(
                    "shadowsocks requires credentials (method:password)".to_string(),
                )) as Box<dyn std::error::Error + Send + Sync>
            })?;

            let method = eggress_protocol_shadowsocks::CipherMethod::parse_method(&creds.username)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

            eggress_protocol_shadowsocks::shadowsocks_connect(
                stream,
                target,
                method,
                &creds.password,
            )
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

struct TrojanHopHandler {
    tls_config: Option<std::sync::Arc<rustls::ClientConfig>>,
}

impl HopHandler for TrojanHopHandler {
    fn protocol(&self) -> eggress_uri::ProtocolSpec {
        eggress_uri::ProtocolSpec::Trojan
    }

    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a eggress_uri::ProxyHopSpec,
    ) -> HandshakeFuture<'a> {
        let tls_config = self.tls_config.clone();
        let password = hop.credentials.as_ref().map(|c| c.password.clone());
        let server_name = hop
            .server_name
            .clone()
            .unwrap_or_else(|| hop.endpoint.host.clone());
        Box::pin(async move {
            let password = password.ok_or_else(|| {
                Box::new(eggress_protocol_trojan::TrojanError::Protocol(
                    "trojan requires credentials (password)".to_string(),
                )) as Box<dyn std::error::Error + Send + Sync>
            })?;

            eggress_protocol_trojan::trojan_connect(
                stream,
                target,
                &password,
                &server_name,
                tls_config,
            )
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
