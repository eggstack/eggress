//! Dedicated listener handlers for multiplexed and upgraded transports.

use eggress_core::{BoxStream, ClientIdentity, TargetAddr};
use subtle::ConstantTimeEq;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::accept::{
    auth_credentials, cached_identity, record_authenticated, AcceptedSession, PendingTunnel,
    ReplyContext, TunnelProtocol,
};
use crate::auth::parse_basic_auth;
use crate::ConnectionConfig;

struct H2StreamAdapter {
    reader: eggress_protocol_http::H2StreamRead,
    writer: eggress_protocol_http::H2StreamWrite,
}

impl AsyncRead for H2StreamAdapter {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl AsyncWrite for H2StreamAdapter {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.writer).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.writer).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}

/// Serve an HTTP/2 prior-knowledge or TLS/ALPN listener. Each CONNECT stream
/// is routed independently while the parent connection continues accepting
/// unrelated streams. Stream tasks are spawned onto `tasks` and cancelled via
/// `cancel` so shutdown coordination can track and drain live tunnels.
pub async fn serve_h2_connection(
    client: BoxStream,
    config: ConnectionConfig,
    tasks: &tokio_util::task::TaskTracker,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    let peer_ip = config.context.source.map(|peer| peer.ip());
    let mut connection = h2::server::handshake(client)
        .await
        .map_err(|error| format!("H2 handshake failed: {error}"))?;

    while let Some(result) = connection.accept().await {
        let (request, mut response) = match result {
            Ok(pair) => pair,
            Err(error) => {
                tracing::warn!(%error, "H2 accept error");
                continue;
            }
        };
        if request.method() != http::Method::CONNECT {
            response.send_reset(h2::Reason::PROTOCOL_ERROR);
            continue;
        }

        let target = match h2_target(request.uri()) {
            Ok(target) => target,
            Err(_) => {
                let reply = http::Response::builder().status(400).body(()).unwrap();
                if let Err(error) = response.send_response(reply, true) {
                    tracing::warn!(%error, "H2 send 400 response failed");
                }
                continue;
            }
        };

        let cached = cached_identity(&config.authentication, peer_ip);
        let authenticated = if cached.is_some() {
            true
        } else if let Some((username, password, _)) = auth_credentials(&config.authentication) {
            matches!(
                request
                    .headers()
                    .get(http::header::PROXY_AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .and_then(parse_basic_auth),
                Some((user, pass))
                    if (user.as_bytes().ct_eq(username.as_bytes())
                        & pass.as_bytes().ct_eq(password.as_bytes()))
                        .unwrap_u8()
                        == 1
            )
        } else {
            true
        };

        if !authenticated {
            let reply = http::Response::builder()
                .status(407)
                .header(http::header::PROXY_AUTHENTICATE, "Basic realm=\"eggress\"")
                .body(())
                .unwrap();
            if let Err(error) = response.send_response(reply, true) {
                tracing::warn!(%error, "H2 send 407 response failed");
            }
            continue;
        }

        let identity = cached.unwrap_or_else(|| {
            let identity = request
                .headers()
                .get(http::header::PROXY_AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_basic_auth)
                .map(|(user, _)| ClientIdentity::Username(user))
                .unwrap_or(ClientIdentity::Anonymous);
            record_authenticated(&config.authentication, peer_ip, &identity);
            identity
        });

        let send_stream = match response.send_response(
            http::Response::builder().status(200).body(()).unwrap(),
            false,
        ) {
            Ok(stream) => stream,
            Err(error) => {
                tracing::warn!(%error, "H2 send 200 response failed");
                continue;
            }
        };
        let client_stream: BoxStream = Box::new(H2StreamAdapter {
            reader: eggress_protocol_http::H2StreamRead::new(request.into_body()),
            writer: eggress_protocol_http::H2StreamWrite::new(send_stream),
        });
        let stream_config = config.clone();
        let stream_cancel = cancel.child_token();
        let pending = PendingTunnel {
            target,
            client: client_stream,
            protocol: TunnelProtocol::Http2,
            reply_context: ReplyContext::Http2,
            identity,
        };
        let target_for_cancel = pending.target.to_string();
        tasks.spawn(async move {
            if let Some(metrics) = &stream_config.metrics {
                metrics.record_session_start();
            }
            let report = tokio::select! {
                _ = stream_cancel.cancelled() => {
                    crate::execute::SessionReport::cancelled(
                        Some("h2".to_string()),
                        Some(target_for_cancel),
                        "h2-cancelled".to_string(),
                    )
                }
                result = crate::execute::execute(AcceptedSession::Tunnel(pending), &stream_config) => {
                    result
                }
            };
            if let Some(metrics) = &stream_config.metrics {
                metrics.record_session(&report);
            }
        });
    }

    Ok(())
}

/// Serve a WebSocket listener. WebSocket listeners use a fixed target because
/// the upgrade request itself carries no proxy CONNECT authority.
pub async fn serve_websocket_connection(
    client: BoxStream,
    config: ConnectionConfig,
    fixed_target: TargetAddr,
) -> Result<(), String> {
    let peer_ip = config.context.source.map(|peer| peer.ip());
    let cached = cached_identity(&config.authentication, peer_ip);
    let credentials = if cached.is_some() {
        None
    } else {
        auth_credentials(&config.authentication).map(|(user, pass, _)| (user, pass))
    };
    let (client, authenticated_user) =
        eggress_protocol_websocket::accept_upgrade_with_auth(client, credentials)
            .await
            .map_err(|error| error.to_string())?;
    let identity = cached.unwrap_or_else(|| {
        let identity = authenticated_user
            .map(ClientIdentity::Username)
            .unwrap_or(ClientIdentity::Anonymous);
        record_authenticated(&config.authentication, peer_ip, &identity);
        identity
    });
    let pending = PendingTunnel {
        target: fixed_target.clone(),
        client,
        protocol: TunnelProtocol::WebSocket,
        reply_context: ReplyContext::WebSocket,
        identity,
    };
    if let Some(metrics) = &config.metrics {
        metrics.record_session_start();
    }
    let report = crate::execute::execute(AcceptedSession::Tunnel(pending), &config).await;
    if let Some(metrics) = &config.metrics {
        metrics.record_session(&report);
    }
    Ok(())
}

fn h2_target(uri: &http::Uri) -> Result<TargetAddr, String> {
    let authority = uri
        .authority()
        .map(|authority| authority.as_str().to_string())
        .or_else(|| (!uri.path().is_empty()).then(|| uri.path().to_string()))
        .ok_or_else(|| "missing H2 CONNECT authority".to_string())?;
    if authority.contains(':') {
        authority.parse()
    } else {
        format!("{authority}:443").parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::sync::Arc;
    use std::time::Duration;

    use eggress_routing::{RouteActionSpec, RouteService, Router};
    use futures_util::{SinkExt, StreamExt};

    fn config(peer: std::net::SocketAddr, protocol: eggress_core::ProtocolId) -> ConnectionConfig {
        ConnectionConfig {
            routing: Arc::new(Router::new(vec![], RouteActionSpec::Direct))
                as Arc<dyn RouteService>,
            context: crate::ConnectionContext {
                source: Some(peer),
                listener: "advanced-test".to_string(),
                generation: 0,
            },
            handshake_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(5),
            protocols: Arc::from([protocol]),
            authentication: crate::accept::InboundAuthentication::None,
            metrics: None,
            udp: None,
            tls_client_config: None,
            shadowsocks: None,
            shadowsocks_metrics: Some(Arc::new(
                eggress_protocol_shadowsocks::ShadowsocksMetrics::new(),
            )),
            trojan: None,
            fixed_target: None,
            local_bind: None,
        }
    }

    #[tokio::test]
    async fn h2_listener_routes_connect_stream_to_local_target() {
        let (echo_addr, echo_task) = eggress_testkit::start_echo_server().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_addr = listener.local_addr().unwrap();
        let tasks = tokio_util::task::TaskTracker::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let server_tasks = tasks.clone();
        let server = tokio::spawn(async move {
            let (stream, peer) = listener.accept().await.unwrap();
            let result = serve_h2_connection(
                Box::new(stream),
                config(peer, eggress_core::ProtocolId::Http2),
                &server_tasks,
                cancel,
            )
            .await;
            tasks.close();
            result
        });

        let stream = tokio::net::TcpStream::connect(listener_addr).await.unwrap();
        let (mut sender, connection) = h2::client::handshake(stream).await.unwrap();
        let driver = tokio::spawn(connection);
        let request = http::Request::builder()
            .method(http::Method::CONNECT)
            .uri(echo_addr.to_string())
            .body(())
            .unwrap();
        let (response, mut send) = sender.send_request(request, false).unwrap();
        let response = match tokio::time::timeout(Duration::from_secs(5), response).await {
            Ok(response) => response.unwrap(),
            Err(_) => {
                let server_done = server.is_finished();
                server.abort();
                panic!(
                    "H2 response timed out (client driver done: {}, server done: {server_done})",
                    driver.is_finished(),
                );
            }
        };
        assert_eq!(response.status(), http::StatusCode::OK);
        send.send_data(Bytes::from_static(b"h2 listener"), true)
            .unwrap();

        let mut body = response.into_body();
        let mut received = Vec::new();
        while let Some(chunk) = body.data().await {
            received.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(received, b"h2 listener");
        drop(sender);
        driver.abort();
        server.abort();
        echo_task.abort();
    }

    #[tokio::test]
    async fn websocket_listener_routes_binary_to_local_target() {
        let (echo_addr, echo_task) = eggress_testkit::start_echo_server().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_addr = listener.local_addr().unwrap();
        let target = echo_addr.to_string().parse().unwrap();
        let server = tokio::spawn(async move {
            let (stream, peer) = listener.accept().await.unwrap();
            serve_websocket_connection(
                Box::new(stream),
                config(peer, eggress_core::ProtocolId::WebSocket),
                target,
            )
            .await
        });

        let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{listener_addr}"))
            .await
            .unwrap();
        client
            .send(tokio_tungstenite::tungstenite::Message::Binary(
                b"websocket listener".to_vec().into(),
            ))
            .await
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(&response.into_data()[..], b"websocket listener");
        let _ = client.close(None).await;
        server.abort();
        echo_task.abort();
    }
}
