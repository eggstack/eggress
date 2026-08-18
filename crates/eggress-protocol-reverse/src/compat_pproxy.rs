//! The deliberately small wire adapter for pproxy 2.7.9 backward links.
//!
//! pproxy's ProxyBackward is not the native Eggress reverse protocol. A
//! worker opens a transport to the remote listener, writes the configured auth
//! bytes without a delimiter, and then waits for the remote listener to put
//! that channel to work. Keeping this implementation separate is important:
//! changing the native reverse handshake would change the native protocol.

use crate::client::TargetResolver;
use crate::{relay_bidirectional_with_timeout, ProtocolError};
use eggress_core::{TargetAddr, TargetHost};
use eggress_uri::{ProtocolSpec, ProxyChainSpec};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// pproxy backward reconnect states, exposed for deterministic tests and
/// diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PproxyBackwardState {
    Disconnected,
    Connecting,
    Authenticating,
    ReadyChannel,
    Retrying,
    Closed,
}

/// Channel framing applied between the compatibility adapter and the
/// peer `pproxy 2.7.9` process after the auth handshake completes.
///
/// pproxy's `+in` worker runs the SOCKS5 server side after auth, expecting
/// the listener side to send a SOCKS5 hello. The `Raw` framing keeps the
/// older byte-pipe model for Eggress-internal use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PproxyBackwardFraming {
    /// Bytes flow between the queued channel and the configured target with
    /// no protocol framing. Used for Eggress-internal reverse tests where
    /// the channel is paired with a plain TCP external client.
    #[default]
    Raw,
    /// SOCKS5 server (worker side) or client (listener side) framing matches
    /// pproxy 2.7.9 `+in` semantics so payload-level interop with the real
    /// pproxy interpreter can be verified byte-for-byte.
    Socks5,
}

/// Configuration for one pproxy backward worker.
#[derive(Debug, Clone)]
pub struct PproxyBackwardClientConfig {
    pub server_addr: SocketAddr,
    /// Full pproxy chain. Hop zero is the raw backward endpoint; later hops
    /// are used as transport jumps in reverse order.
    pub server_chain: Option<ProxyChainSpec>,
    /// Raw pproxy auth bytes. This is intentionally not newline terminated.
    pub auth: Vec<u8>,
    pub reconnect_initial_ms: u64,
    pub reconnect_max_ms: u64,
    pub read_timeout_ms: u64,
    pub target_connect_timeout_ms: u64,
    /// Channel framing used after the auth handshake. Defaults to `Raw`.
    pub server_framing: PproxyBackwardFraming,
}

impl Default for PproxyBackwardClientConfig {
    fn default() -> Self {
        Self {
            server_addr: "127.0.0.1:0".parse().expect("valid default socket address"),
            server_chain: None,
            auth: Vec::new(),
            reconnect_initial_ms: 100,
            reconnect_max_ms: 30_000,
            read_timeout_ms: 60_000,
            target_connect_timeout_ms: 10_000,
            server_framing: PproxyBackwardFraming::default(),
        }
    }
}

/// A pproxy-compatible backward client. One instance owns exactly one
/// persistent worker; callers create one instance per +in occurrence.
pub struct PproxyBackwardClient {
    config: PproxyBackwardClientConfig,
    cancel: CancellationToken,
    resolver: Arc<dyn TargetResolver>,
}

impl PproxyBackwardClient {
    pub fn new(config: PproxyBackwardClientConfig, resolver: Arc<dyn TargetResolver>) -> Self {
        Self {
            config,
            cancel: CancellationToken::new(),
            resolver,
        }
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub async fn run(&self) -> Result<(), ProtocolError> {
        let mut backoff = self.config.reconnect_initial_ms.max(1);
        loop {
            if self.cancel.is_cancelled() {
                break;
            }

            match self.run_connection().await {
                Ok(()) => backoff = self.config.reconnect_initial_ms.max(1),
                Err(error) => {
                    if self.cancel.is_cancelled() {
                        break;
                    }
                    warn!(error = %error, backoff_ms = backoff, "pproxy backward channel failed");
                    let delay = tokio::time::sleep(Duration::from_millis(backoff));
                    tokio::pin!(delay);
                    tokio::select! {
                        _ = &mut delay => {}
                        _ = self.cancel.cancelled() => break,
                    }
                    backoff = backoff
                        .saturating_mul(2)
                        .min(self.config.reconnect_max_ms.max(1));
                }
            }
        }
        Ok(())
    }

    async fn run_connection(&self) -> Result<(), ProtocolError> {
        let mut stream = self.connect_control().await?;
        if !self.config.auth.is_empty() {
            // Observable pproxy framing: raw bytes, no newline, and no native
            // Eggress accept/reject byte.
            stream.write_all(&self.config.auth).await?;
            stream.flush().await?;
        }

        match self.config.server_framing {
            PproxyBackwardFraming::Raw => self.run_connection_raw(stream).await,
            PproxyBackwardFraming::Socks5 => self.run_connection_socks5(stream).await,
        }
    }

    /// Raw byte-pipe mode: connect to the configured resolver target and
    /// relay bytes between the channel and that target. Used by
    /// Eggress-internal reverse tests where the external side is a plain
    /// TCP client.
    async fn run_connection_raw(&self, stream: TcpStream) -> Result<(), ProtocolError> {
        let (host, port) = match self.resolver.resolve() {
            crate::client::TargetResolution::Connect { host, port } => (host, port),
            crate::client::TargetResolution::Reject { reason } => {
                return Err(ProtocolError::ConfigInvalid(format!(
                    "pproxy backward route rejected: {reason}"
                )))
            }
        };

        let timeout = Duration::from_millis(self.config.target_connect_timeout_ms.max(1));
        let target = tokio::time::timeout(timeout, TcpStream::connect((host.as_str(), port)))
            .await
            .map_err(|_| {
                ProtocolError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "pproxy backward target connect timed out",
                ))
            })??;

        relay_bidirectional_with_timeout(
            stream,
            target,
            (self.config.read_timeout_ms > 0)
                .then(|| Duration::from_millis(self.config.read_timeout_ms)),
        )
        .await
    }

/// SOCKS5 server mode. pproxy 2.7.9 `+in` workers run the SOCKS5 server
/// side after the auth handshake, sending `[0x05, n, ...methods]` and
/// expecting a methods selection back. Reading the resulting CONNECT
/// target gives this worker the local echo target, matching the byte
/// payload relayed end-to-end through the real pproxy interpreter.
async fn run_connection_socks5(&self, mut stream: TcpStream) -> Result<(), ProtocolError> {
    const SOCKS5_VERSION: u8 = 0x05;
    const SOCKS5_METHOD_NONE: u8 = 0x00;
    const SOCKS5_CMD_CONNECT: u8 = 0x01;
    const SOCKS5_RSV: u8 = 0x00;
    const SOCKS5_ATYP_IPV4: u8 = 0x01;
    const SOCKS5_ATYP_DOMAIN: u8 = 0x03;
    const SOCKS5_ATYP_IPV6: u8 = 0x04;
    const SOCKS5_REP_SUCCESS: u8 = 0x00;

        // SOCKS5 hello: [version, nmethods, methods...]
        let mut header = [0u8; 2];
        stream.read_exact(&mut header).await?;
        if header[0] != SOCKS5_VERSION {
            return Err(ProtocolError::ConfigInvalid(format!(
                "pproxy backward SOCKS5 hello version mismatch: {}",
                header[0]
            )));
        }
        let nmethods = header[1] as usize;
        let mut methods = vec![0u8; nmethods];
        if nmethods > 0 {
            stream.read_exact(&mut methods).await?;
        }
        if !methods.contains(&SOCKS5_METHOD_NONE) {
            stream
                .write_all(&[SOCKS5_VERSION, 0xff])
                .await?;
            stream.flush().await?;
            return Err(ProtocolError::AuthFailed);
        }
        stream
            .write_all(&[SOCKS5_VERSION, SOCKS5_METHOD_NONE])
            .await?;
        stream.flush().await?;

        // SOCKS5 CONNECT request: [version, cmd, rsv, atyp, ...]
        let mut req_header = [0u8; 4];
        stream.read_exact(&mut req_header).await?;
        if req_header[0] != SOCKS5_VERSION || req_header[1] != SOCKS5_CMD_CONNECT {
            return Err(ProtocolError::ConfigInvalid(format!(
                "pproxy backward SOCKS5 request header invalid: {:?}",
                &req_header[..]
            )));
        }
        let _rsv = req_header[2];
        if req_header[2] != SOCKS5_RSV {
            return Err(ProtocolError::ConfigInvalid(format!(
                "pproxy backward SOCKS5 RSV must be zero, got {}",
                req_header[2]
            )));
        }
        let atyp = req_header[3];
        let host = match atyp {
            SOCKS5_ATYP_IPV4 => {
                let mut addr = [0u8; 4];
                stream.read_exact(&mut addr).await?;
                std::net::IpAddr::V4(std::net::Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]))
                    .to_string()
            }
            SOCKS5_ATYP_DOMAIN => {
                let mut len = [0u8; 1];
                stream.read_exact(&mut len).await?;
                let n = len[0] as usize;
                if n == 0 {
                    return Err(ProtocolError::ConfigInvalid(
                        "pproxy backward SOCKS5 domain length zero".into(),
                    ));
                }
                let mut domain = vec![0u8; n];
                stream.read_exact(&mut domain).await?;
                String::from_utf8(domain)
                    .map_err(|_| ProtocolError::ConfigInvalid("invalid SOCKS5 domain".into()))?
            }
            SOCKS5_ATYP_IPV6 => {
                let mut addr = [0u8; 16];
                stream.read_exact(&mut addr).await?;
                std::net::IpAddr::V6(std::net::Ipv6Addr::from(addr)).to_string()
            }
            _ => {
                stream
                    .write_all(&[
                        SOCKS5_VERSION,
                        0x08,
                        SOCKS5_RSV,
                        SOCKS5_ATYP_IPV4,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ])
                    .await?;
                stream.flush().await?;
                return Err(ProtocolError::ConfigInvalid(format!(
                    "pproxy backward SOCKS5 ATYP {atyp} unsupported"
                )));
            }
        };
        let mut port_bytes = [0u8; 2];
        stream.read_exact(&mut port_bytes).await?;
        let port = u16::from_be_bytes(port_bytes);

        let timeout = Duration::from_millis(self.config.target_connect_timeout_ms.max(1));
        let target = tokio::time::timeout(timeout, TcpStream::connect((host.as_str(), port)))
            .await
            .map_err(|_| {
                ProtocolError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "pproxy backward SOCKS5 target connect timed out",
                ))
            })?;
        let target = match target {
            Ok(t) => t,
            Err(error) => {
                // Reply with a connection refused-style SOCKS5 response so
                // the peer closes the channel cleanly.
                let _ = stream
                    .write_all(&[
                        SOCKS5_VERSION,
                        0x05,
                        SOCKS5_RSV,
                        SOCKS5_ATYP_IPV4,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ])
                    .await;
                let _ = stream.flush().await;
                return Err(ProtocolError::Io(error));
            }
        };

        stream
            .write_all(&[
                SOCKS5_VERSION,
                SOCKS5_REP_SUCCESS,
                SOCKS5_RSV,
                SOCKS5_ATYP_IPV4,
                0,
                0,
                0,
                0,
                0,
                0,
            ])
            .await?;
        stream.flush().await?;

        relay_bidirectional_with_timeout(
            stream,
            target,
            (self.config.read_timeout_ms > 0)
                .then(|| Duration::from_millis(self.config.read_timeout_ms)),
        )
        .await
    }

    async fn connect_control(&self) -> Result<TcpStream, ProtocolError> {
        let Some(chain) = self.config.server_chain.as_ref() else {
            return Ok(TcpStream::connect(self.config.server_addr).await?);
        };
        if chain.hops.len() <= 1 {
            return Ok(TcpStream::connect(self.config.server_addr).await?);
        }
        if chain.hops.iter().any(|hop| hop.tls) {
            return Err(ProtocolError::ConfigInvalid(
                "TLS-wrapped pproxy backward jumps require a configured TLS transport".into(),
            ));
        }

        let last = chain.hops.last().expect("chain length checked");
        let mut stream =
            TcpStream::connect((last.endpoint.host.as_str(), last.endpoint.port)).await?;
        for index in (1..chain.hops.len()).rev() {
            let jump = &chain.hops[index];
            let endpoint = &chain.hops[index - 1].endpoint;
            let target = TargetAddr {
                host: endpoint
                    .host
                    .parse()
                    .map(TargetHost::Ip)
                    .unwrap_or_else(|_| TargetHost::Domain(endpoint.host.clone())),
                port: endpoint.port,
            };
            stream = connect_jump(stream, jump, &target).await?;
        }
        Ok(stream)
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

/// Configuration for the pproxy-compatible accepting side.
#[derive(Debug, Clone)]
pub struct PproxyBackwardServerConfig {
    pub control_bind: SocketAddr,
    pub external_bind: SocketAddr,
    /// Raw auth bytes expected immediately after the transport is accepted.
    pub auth: Vec<u8>,
    pub max_control_connections: usize,
    pub max_pending_external: usize,
    pub read_timeout_ms: u64,
    /// Optional fixed target the server forwards each external client to
    /// after the SOCKS5 CONNECT handshake. The SOCKS5 framing negotiates a
    /// destination with the pproxy worker side and uses it for the channel.
    pub socks5_target: Option<(String, u16)>,
    /// Channel framing applied between the listener and pproxy worker
    /// channels. The `Raw` framing keeps the older byte-pipe model for
    /// Eggress-internal reverse tests; `Socks5` matches pproxy 2.7.9 `+in`
    /// so payload-level interop with the real pproxy interpreter can be
    /// verified byte-for-byte.
    pub client_framing: PproxyBackwardFraming,
}

impl Default for PproxyBackwardServerConfig {
    fn default() -> Self {
        Self {
            control_bind: "127.0.0.1:0".parse().expect("valid default socket address"),
            external_bind: "127.0.0.1:0".parse().expect("valid default socket address"),
            auth: Vec::new(),
            max_control_connections: 256,
            max_pending_external: 1024,
            read_timeout_ms: 300_000,
            socks5_target: None,
            client_framing: PproxyBackwardFraming::default(),
        }
    }
}

struct QueuedChannel {
    stream: TcpStream,
}

/// pproxy-compatible accepting side. It has no native handshake byte and
/// never mutates the native ReverseServer.
pub struct PproxyBackwardServer {
    config: PproxyBackwardServerConfig,
    cancel: CancellationToken,
}

impl PproxyBackwardServer {
    pub fn new(config: PproxyBackwardServerConfig) -> Self {
        Self {
            config,
            cancel: CancellationToken::new(),
        }
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub async fn run(self) -> Result<(), ProtocolError> {
        let control_listener = TcpListener::bind(self.config.control_bind).await?;
        let external_listener = TcpListener::bind(self.config.external_bind).await?;
        let (control_tx, mut control_rx) =
            mpsc::channel::<QueuedChannel>(self.config.max_control_connections.max(1));
        let cancel = self.cancel.clone();
        let config = Arc::new(self.config);
        let mut tasks = JoinSet::new();

        let accept_cancel = cancel.clone();
        let accept_config = config.clone();
        tasks.spawn(async move {
            loop {
                tokio::select! {
                    result = control_listener.accept() => {
                        let (mut stream, peer) = match result {
                            Ok(value) => value,
                            Err(error) => {
                                warn!(%error, "pproxy backward control accept failed");
                                continue;
                            }
                        };
                        let auth = accept_config.auth.clone();
                        let tx = control_tx.clone();
                        let timeout = accept_config.read_timeout_ms;
                        let framing = accept_config.client_framing;
                        let socks5_target = accept_config.socks5_target.clone();
                        tokio::spawn(async move {
                            if !auth.is_empty() {
                                let mut received = vec![0u8; auth.len()];
                                let read = tokio::time::timeout(
                                    Duration::from_millis(timeout.max(1)),
                                    stream.read_exact(&mut received),
                                ).await;
                                if !matches!(read, Ok(Ok(_))) || received != auth {
                                    debug!(%peer, "pproxy backward auth rejected");
                                    return;
                                }
                            }
                            if matches!(framing, PproxyBackwardFraming::Socks5) {
                                if let Err(error) = proxy_socks5_setup(&mut stream).await {
                                    debug!(%peer, %error, "pproxy backward SOCKS5 setup failed");
                                    return;
                                }
                                if let Some((host, port)) = socks5_target {
                                    if let Err(error) =
                                        reply_socks5_connect(&mut stream, &host, port).await
                                    {
                                        debug!(
                                            %peer,
                                            %error,
                                            "pproxy backward SOCKS5 CONNECT reply failed"
                                        );
                                        return;
                                    }
                                    // The worker dials the target and sends a
                                    // SOCKS5 CONNECT reply. Drain it here so the
                                    // channel only carries application bytes
                                    // once it is paired with an external client.
                                    if let Err(error) =
                                        read_socks5_connect_reply(&mut stream).await
                                    {
                                        debug!(
                                            %peer,
                                            %error,
                                            "pproxy backward SOCKS5 CONNECT reply read failed"
                                        );
                                        return;
                                    }
                                }
                            }
                            let _ = tx.send(QueuedChannel { stream }).await;
                        });
                    }
                    _ = accept_cancel.cancelled() => break,
                }
            }
        });

        let external_cancel = cancel.clone();
        let external_framing = config.client_framing;
        let external_target = config.socks5_target.clone();
        tasks.spawn(async move {
            let mut relays = JoinSet::new();
            loop {
                tokio::select! {
                    result = external_listener.accept() => {
                        let (external, peer) = match result {
                            Ok(value) => value,
                            Err(error) => {
                                warn!(%error, "pproxy backward external accept failed");
                                continue;
                            }
                        };
                        let control = tokio::select! {
                            value = control_rx.recv() => value,
                            _ = external_cancel.cancelled() => break,
                        };
                        let Some(control) = control else { break };
                        let timeout = config.read_timeout_ms;
                        let target = external_target.clone();
                        relays.spawn(async move {
                            debug!(%peer, "relaying pproxy backward channel");
                            let result = relay_pproxy_pair(
                                external,
                                control.stream,
                                target,
                                external_framing,
                                timeout,
                            )
                            .await;
                            if let Err(error) = result {
                                debug!(%peer, %error, "pproxy backward relay finished with error");
                            }
                        });
                    }
                    _ = external_cancel.cancelled() => break,
                }
            }
            relays.abort_all();
            while relays.join_next().await.is_some() {}
        });

        cancel.cancelled().await;
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        info!("pproxy backward server shut down");
        Ok(())
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

/// Build the raw auth field used by pproxy's ProxySimple.auth property.
pub fn raw_auth(username: Option<&str>, password: Option<&str>) -> Vec<u8> {
    match (username, password) {
        (Some(user), Some(pass)) => format!("{user}:{pass}").into_bytes(),
        (Some(user), None) => user.as_bytes().to_vec(),
        (None, Some(pass)) => pass.as_bytes().to_vec(),
        (None, None) => Vec::new(),
    }
}

/// Drive the SOCKS5 hello + methods selection half of the channel handshake
/// from the server side. pproxy 2.7.9 `+in` workers act as SOCKS5 servers
/// after auth and wait for a SOCKS5 client hello. The listener (this side)
/// acts as the SOCKS5 client, sends the hello, then waits for the worker
/// to choose a method.
async fn proxy_socks5_setup(stream: &mut TcpStream) -> Result<(), ProtocolError> {
    stream.write_all(&[0x05, 0x01, 0x00]).await?;
    stream.flush().await?;
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await?;
    if header[0] != 0x05 {
        return Err(ProtocolError::ConfigInvalid(format!(
            "SOCKS5 methods selection version mismatch: {}",
            header[0]
        )));
    }
    if header[1] != 0x00 {
        return Err(ProtocolError::AuthFailed);
    }
    Ok(())
}

/// Drain the SOCKS5 CONNECT reply from the worker after it has dialed the
/// target. The reply header has the same shape as a SOCKS5 reply to a
/// CONNECT request: `[version, rep, rsv, atyp, ...bound addr..., port]`.
async fn read_socks5_connect_reply(stream: &mut TcpStream) -> Result<(), ProtocolError> {
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    if header[0] != 0x05 {
        return Err(ProtocolError::ConfigInvalid(format!(
            "SOCKS5 CONNECT reply version mismatch: {}",
            header[0]
        )));
    }
    if header[1] != 0x00 {
        return Err(ProtocolError::ConfigInvalid(format!(
            "SOCKS5 CONNECT reply rep non-success: {}",
            header[1]
        )));
    }
    match header[3] {
        0x01 => {
            let mut tail = [0u8; 6];
            stream.read_exact(&mut tail).await?;
        }
        0x04 => {
            let mut tail = [0u8; 18];
            stream.read_exact(&mut tail).await?;
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut tail = vec![0u8; len[0] as usize + 2];
            stream.read_exact(&mut tail).await?;
        }
        other => {
            return Err(ProtocolError::ConfigInvalid(format!(
                "SOCKS5 CONNECT reply ATYP {other} unsupported"
            )));
        }
    }
    Ok(())
}

/// Send a SOCKS5 CONNECT request through the channel to inform the worker
/// of the destination it should reach. The worker dials that target via its
/// `-r` upstream chain and relays bytes back through the channel.
async fn reply_socks5_connect(
    stream: &mut TcpStream,
    host: &str,
    port: u16,
) -> Result<(), ProtocolError> {
    // CONNECT request header
    stream.write_all(&[0x05, 0x01, 0x00]).await?;
    // ATYP + address
    let parsed_host: std::net::IpAddr = host.parse().unwrap_or(std::net::IpAddr::V4(
        std::net::Ipv4Addr::UNSPECIFIED,
    ));
    if let std::net::IpAddr::V4(ipv4) = parsed_host {
        stream.write_all(&[0x01]).await?;
        stream.write_all(&ipv4.octets()).await?;
    } else if let std::net::IpAddr::V6(ipv6) = parsed_host {
        stream.write_all(&[0x04]).await?;
        stream.write_all(&ipv6.octets()).await?;
    } else {
        let bytes = host.as_bytes();
        if bytes.len() > 255 {
            return Err(ProtocolError::ConfigInvalid(format!(
                "SOCKS5 target host too long: {}",
                bytes.len()
            )));
        }
        stream.write_all(&[0x03, bytes.len() as u8]).await?;
        stream.write_all(bytes).await?;
    }
    stream.write_all(&port.to_be_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

/// Pair an external TCP client with a queued pproxy worker channel. Under
/// the `Raw` framing this is a plain byte relay; under `Socks5` the
/// listener already drove the channel CONNECT during the worker's
/// initial handshake, so this only forwards the application bytes.
async fn relay_pproxy_pair(
    external: TcpStream,
    control: TcpStream,
    _target: Option<(String, u16)>,
    framing: PproxyBackwardFraming,
    timeout_ms: u64,
) -> Result<(), ProtocolError> {
    let timeout = (timeout_ms > 0).then(|| Duration::from_millis(timeout_ms));
    match framing {
        PproxyBackwardFraming::Raw => relay_bidirectional_with_timeout(external, control, timeout).await,
        PproxyBackwardFraming::Socks5 => relay_bidirectional_with_timeout(external, control, timeout).await,
    }
}

async fn connect_jump(
    mut stream: TcpStream,
    hop: &eggress_uri::ProxyHopSpec,
    target: &TargetAddr,
) -> Result<TcpStream, ProtocolError> {
    match hop.protocols.as_slice() {
        [ProtocolSpec::Http] | [ProtocolSpec::HttpOnly] => {
            let authority = target.to_string();
            let mut request = format!(
                "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nConnection: keep-alive\r\n"
            );
            if let Some(credentials) = &hop.credentials {
                let encoded = base64_encode(
                    format!("{}:{}", credentials.username, credentials.password).as_bytes(),
                );
                request.push_str(&format!("Proxy-Authorization: Basic {encoded}\r\n"));
            }
            request.push_str("\r\n");
            stream.write_all(request.as_bytes()).await?;
            let mut response = Vec::new();
            read_until_headers(&mut stream, &mut response).await?;
            let status = response
                .split(|byte| *byte == b' ')
                .nth(1)
                .and_then(|code| std::str::from_utf8(code).ok())
                .and_then(|code| code.parse::<u16>().ok());
            if status != Some(200) {
                return Err(ProtocolError::ConfigInvalid(
                    "pproxy backward HTTP jump rejected CONNECT".into(),
                ));
            }
            Ok(stream)
        }
        [ProtocolSpec::Socks5] => {
            let credentials = hop.credentials.as_ref();
            if credentials.is_some() {
                stream.write_all(&[5, 1, 2]).await?;
            } else {
                stream.write_all(&[5, 1, 0]).await?;
            }
            let mut method = [0u8; 2];
            stream.read_exact(&mut method).await?;
            if method[0] != 5 || method[1] == 0xff {
                return Err(ProtocolError::ConfigInvalid(
                    "pproxy backward SOCKS5 jump rejected authentication".into(),
                ));
            }
            if method[1] == 2 {
                let credentials = credentials.ok_or_else(|| {
                    ProtocolError::ConfigInvalid(
                        "SOCKS5 jump requested credentials that were not configured".into(),
                    )
                })?;
                let user = credentials.username.as_bytes();
                let pass = credentials.password.as_bytes();
                if user.len() > 255 || pass.len() > 255 {
                    return Err(ProtocolError::ConfigInvalid(
                        "SOCKS5 jump credentials are too long".into(),
                    ));
                }
                stream.write_all(&[1, user.len() as u8]).await?;
                stream.write_all(user).await?;
                stream.write_all(&[pass.len() as u8]).await?;
                stream.write_all(pass).await?;
                let mut auth_reply = [0u8; 2];
                stream.read_exact(&mut auth_reply).await?;
                if auth_reply != [1, 0] {
                    return Err(ProtocolError::AuthFailed);
                }
            }
            let address = encode_socks_address(target)?;
            stream.write_all(&[5, 1, 0]).await?;
            stream.write_all(&address).await?;
            let mut reply = [0u8; 4];
            stream.read_exact(&mut reply).await?;
            if reply[1] != 0 {
                return Err(ProtocolError::ConfigInvalid(format!(
                    "SOCKS5 backward jump CONNECT failed with code {}",
                    reply[1]
                )));
            }
            let remaining = match reply[3] {
                1 => 6,
                4 => 18,
                3 => {
                    let mut length = [0u8; 1];
                    stream.read_exact(&mut length).await?;
                    usize::from(length[0]) + 2
                }
                _ => return Err(ProtocolError::ConfigInvalid("invalid SOCKS5 reply".into())),
            };
            let mut discard = vec![0u8; remaining];
            stream.read_exact(&mut discard).await?;
            Ok(stream)
        }
        _ => Err(ProtocolError::ConfigInvalid(
            "pproxy backward jump supports only HTTP CONNECT and SOCKS5".into(),
        )),
    }
}

async fn read_until_headers(
    stream: &mut TcpStream,
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    let mut byte = [0u8; 1];
    while output.len() < 16 * 1024 {
        stream.read_exact(&mut byte).await?;
        output.push(byte[0]);
        if output.ends_with(b"\r\n\r\n") {
            return Ok(());
        }
    }
    Err(ProtocolError::ConfigInvalid(
        "proxy jump response headers exceed 16 KiB".into(),
    ))
}

fn encode_socks_address(target: &TargetAddr) -> Result<Vec<u8>, ProtocolError> {
    let mut output = Vec::new();
    match &target.host {
        TargetHost::Ip(std::net::IpAddr::V4(ip)) => {
            output.push(1);
            output.extend_from_slice(&ip.octets());
        }
        TargetHost::Ip(std::net::IpAddr::V6(ip)) => {
            output.push(4);
            output.extend_from_slice(&ip.octets());
        }
        TargetHost::Domain(domain) => {
            if domain.len() > 255 {
                return Err(ProtocolError::ConfigInvalid(
                    "SOCKS5 backward jump target domain is too long".into(),
                ));
            }
            output.push(3);
            output.push(domain.len() as u8);
            output.extend_from_slice(domain.as_bytes());
        }
    }
    output.extend_from_slice(&target.port.to_be_bytes());
    Ok(output)
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in input.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[((a << 4 | b >> 4) & 0x3f) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[((b << 2 | c >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{TargetResolution, TargetResolver};

    struct Resolver;
    impl TargetResolver for Resolver {
        fn resolve(&self) -> TargetResolution {
            TargetResolution::Reject {
                reason: "test".into(),
            }
        }
    }

    struct FixedResolver(SocketAddr);
    impl TargetResolver for FixedResolver {
        fn resolve(&self) -> TargetResolution {
            TargetResolution::Connect {
                host: self.0.ip().to_string(),
                port: self.0.port(),
            }
        }
    }

    #[test]
    fn raw_auth_is_not_newline_terminated() {
        assert_eq!(raw_auth(Some("user"), Some("pass")), b"user:pass");
        assert!(!raw_auth(Some("user"), Some("pass")).contains(&b'\n'));
    }

    #[tokio::test]
    async fn client_cancellation_is_prompt() {
        let client = PproxyBackwardClient::new(
            PproxyBackwardClientConfig {
                server_addr: "127.0.0.1:1".parse().unwrap(),
                reconnect_initial_ms: 1,
                reconnect_max_ms: 2,
                ..Default::default()
            },
            Arc::new(Resolver),
        );
        let cancel = client.cancel_token();
        let task = tokio::spawn(async move { client.run().await });
        tokio::time::sleep(Duration::from_millis(5)).await;
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn raw_backward_client_and_server_relay_without_native_handshake() {
        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_addr = target_listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = target_listener.accept().await.unwrap();
            let mut buf = [0u8; 64];
            let size = stream.read(&mut buf).await.unwrap();
            stream.write_all(&buf[..size]).await.unwrap();
        });

        let control_addr = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap();
        let external_addr = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap();
        let server = PproxyBackwardServer::new(PproxyBackwardServerConfig {
            control_bind: control_addr,
            external_bind: external_addr,
            auth: b"user:pass".to_vec(),
            read_timeout_ms: 2_000,
            ..Default::default()
        });
        let server_cancel = server.cancel_token();
        let server_task = tokio::spawn(server.run());

        let client = PproxyBackwardClient::new(
            PproxyBackwardClientConfig {
                server_addr: control_addr,
                auth: b"user:pass".to_vec(),
                reconnect_initial_ms: 1,
                reconnect_max_ms: 5,
                ..Default::default()
            },
            Arc::new(FixedResolver(target_addr)),
        );
        let client_cancel = client.cancel_token();
        let client_task = tokio::spawn(async move { client.run().await });

        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        let mut external = loop {
            match TcpStream::connect(external_addr).await {
                Ok(stream) => break stream,
                Err(error) if tokio::time::Instant::now() < deadline => {
                    debug!(%error, "waiting for backward external listener in test");
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                Err(error) => panic!("backward external listener did not start: {error}"),
            }
        };
        external.write_all(b"backward").await.unwrap();
        let mut echoed = [0u8; 8];
        tokio::time::timeout(Duration::from_secs(2), external.read_exact(&mut echoed))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&echoed, b"backward");

        client_cancel.cancel();
        server_cancel.cancel();
        external.shutdown().await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), client_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), server_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
