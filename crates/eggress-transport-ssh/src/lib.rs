//! Optional SSH client transport used by pproxy-compatible upstream chains.
//!
//! The crate intentionally exposes only stream/channel operations.  It does
//! not expose remote command, SFTP, agent-forwarding, or SSH-server APIs.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use eggress_core::BoxStream;
use tokio::sync::{mpsc, Mutex};

/// A boxed stream suitable for SSH-over-SSH transport setup.
pub type SshStream = BoxStream;

/// Errors returned by the optional SSH transport.
#[derive(Debug, thiserror::Error)]
pub enum SshTransportError {
    #[error("SSH transport requires a username")]
    MissingUsername,

    #[error("SSH authentication failed")]
    AuthenticationFailed,

    #[error("SSH private key path is empty")]
    EmptyPrivateKeyPath,

    #[error("SSH private key could not be loaded: {0}")]
    PrivateKey(String),

    #[error("SSH connection failed: {0}")]
    Connection(String),

    #[error("SSH channel failed: {0}")]
    Channel(String),

    #[error("SSH target port {0} is out of range")]
    TargetPort(u16),
}

/// SSH authentication selected by pproxy's `login:password` convention.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum SshAuth {
    Password(String),
    PrivateKey(String),
}

impl std::fmt::Debug for SshAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password(_) => f.write_str("Password(****)"),
            Self::PrivateKey(_) => f.write_str("PrivateKey(****)"),
        }
    }
}

/// A redacted key for a reusable SSH session.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SshSessionKey {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
    pub hop_index: usize,
}

impl std::fmt::Debug for SshSessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshSessionKey")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth", &self.auth)
            .field("hop_index", &self.hop_index)
            .finish()
    }
}

#[derive(Clone)]
struct CompatClient {
    forwarded_channels: Option<mpsc::UnboundedSender<russh::Channel<russh::client::Msg>>>,
}

impl CompatClient {
    fn cached() -> Self {
        Self {
            forwarded_channels: None,
        }
    }

    fn remote_forward(
        forwarded_channels: mpsc::UnboundedSender<russh::Channel<russh::client::Msg>>,
    ) -> Self {
        Self {
            forwarded_channels: Some(forwarded_channels),
        }
    }
}

impl russh::client::Handler for CompatClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        // pproxy 2.7.9 passes known_hosts=None. This behavior is isolated to
        // the compatibility transport and is intentionally not a native API.
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<russh::client::Msg>,
        _connected_address: &str,
        _connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: russh::client::ChannelOpenHandle,
        _session: &mut russh::client::Session,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        let forwarded_channels = self.forwarded_channels.clone();
        async move {
            if let Some(forwarded_channels) = forwarded_channels {
                if forwarded_channels.send(channel).is_ok() {
                    reply.accept().await;
                    return Ok(());
                }
            }
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            Ok(())
        }
    }
}

type SessionHandle = russh::client::Handle<CompatClient>;

/// A server-side TCP forward requested through an SSH upstream.
///
/// Each accepted channel is an already-connected stream whose peer is the
/// client that reached the forwarded port on the SSH server. Keeping this
/// value alive keeps the SSH session and remote forward alive.
pub struct SshRemoteForward {
    handle: Arc<SessionHandle>,
    address: String,
    port: u16,
    channels: mpsc::UnboundedReceiver<russh::Channel<russh::client::Msg>>,
}

impl SshRemoteForward {
    /// The address requested from the SSH server.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The port assigned by the SSH server.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Wait for the next incoming connection at the remote forward.
    pub async fn accept(&mut self) -> Option<SshStream> {
        self.channels
            .recv()
            .await
            .map(|channel| Box::new(channel.into_stream()) as SshStream)
    }

    /// Cancel the remote forward while retaining the underlying session.
    pub async fn cancel(&self) -> Result<(), SshTransportError> {
        self.handle
            .cancel_tcpip_forward(self.address.clone(), u32::from(self.port))
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))
    }
}

/// Reusable SSH sessions for one Eggress service lifetime.
///
/// The cache is deliberately explicit so shutdown can drop all session
/// handles. A closed handle is discarded and recreated on the next channel
/// request.
#[derive(Clone, Default)]
pub struct SshSessionCache {
    sessions: Arc<Mutex<HashMap<SshSessionKey, Arc<SessionHandle>>>>,
}

impl SshSessionCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a direct TCP channel, reusing a live authenticated session.
    pub async fn open_tcp_channel(
        &self,
        key: SshSessionKey,
        transport: SshStream,
        target_host: &str,
        target_port: u16,
    ) -> Result<SshStream, SshTransportError> {
        if target_port == 0 {
            return Err(SshTransportError::TargetPort(target_port));
        }
        let session = self.get_or_connect(key.clone(), transport).await?;
        let channel = session
            .channel_open_direct_tcpip(target_host, u32::from(target_port), "127.0.0.1", 0)
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        Ok(Box::new(channel.into_stream()))
    }

    /// Open a Unix-domain channel on the SSH server.
    pub async fn open_unix_channel(
        &self,
        key: SshSessionKey,
        transport: SshStream,
        socket_path: &str,
    ) -> Result<SshStream, SshTransportError> {
        if socket_path.is_empty() {
            return Err(SshTransportError::Channel(
                "Unix target path is empty".to_string(),
            ));
        }
        let session = self.get_or_connect(key, transport).await?;
        let channel = session
            .channel_open_direct_streamlocal(socket_path)
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        Ok(Box::new(channel.into_stream()))
    }

    /// Request a TCP remote forward on the SSH server.
    ///
    /// This is the transport primitive used by pproxy's SSH listener form;
    /// callers own the local listener/tunnel policy and must keep the returned
    /// forward alive while accepting channels.
    pub async fn start_remote_tcp_forward(
        &self,
        key: SshSessionKey,
        transport: SshStream,
        address: &str,
        port: u16,
    ) -> Result<SshRemoteForward, SshTransportError> {
        if !matches!(address, "127.0.0.1" | "::1" | "localhost") {
            tracing::warn!(
                address,
                port,
                "SSH compatibility remote forwarding exposes a non-loopback bind"
            );
        }
        let (sender, channels) = mpsc::unbounded_channel();
        let session = Arc::new(
            connect_authenticated(key, transport, CompatClient::remote_forward(sender)).await?,
        );
        let assigned_port = session
            .tcpip_forward(address, u32::from(port))
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        let assigned_port = u16::try_from(assigned_port)
            .map_err(|_| SshTransportError::Channel("SSH remote port is out of range".into()))?;
        Ok(SshRemoteForward {
            handle: session,
            address: address.to_string(),
            port: assigned_port,
            channels,
        })
    }

    async fn get_or_connect(
        &self,
        key: SshSessionKey,
        transport: SshStream,
    ) -> Result<Arc<SessionHandle>, SshTransportError> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(&key) {
            if !session.is_closed() {
                drop(transport);
                return Ok(Arc::clone(session));
            }
        }

        sessions.remove(&key);
        tracing::warn!(
            host = %key.host,
            port = key.port,
            "SSH compatibility transport disables host-key verification"
        );
        let config = russh::client::Config {
            keepalive_interval: Some(Duration::from_secs(60)),
            keepalive_max: 3,
            ..Default::default()
        };
        let session = Arc::new(
            connect_authenticated_with_config(
                key.clone(),
                transport,
                CompatClient::cached(),
                config,
            )
            .await?,
        );
        sessions.insert(key, Arc::clone(&session));
        Ok(session)
    }

    /// Drop all cached session handles. Existing channels drain according to
    /// the normal Eggress connection cancellation path.
    pub async fn shutdown(&self) {
        self.sessions.lock().await.clear();
    }

    /// Remove one cached session so the next channel request reconnects.
    pub async fn invalidate(&self, key: &SshSessionKey) {
        self.sessions.lock().await.remove(key);
    }
}

async fn connect_authenticated(
    key: SshSessionKey,
    transport: SshStream,
    client: CompatClient,
) -> Result<SessionHandle, SshTransportError> {
    let config = russh::client::Config {
        keepalive_interval: Some(Duration::from_secs(60)),
        keepalive_max: 3,
        ..Default::default()
    };
    connect_authenticated_with_config(key, transport, client, config).await
}

async fn connect_authenticated_with_config(
    key: SshSessionKey,
    transport: SshStream,
    client: CompatClient,
    config: russh::client::Config,
) -> Result<SessionHandle, SshTransportError> {
    let mut session = russh::client::connect_stream(Arc::new(config), transport, client)
        .await
        .map_err(|error| SshTransportError::Connection(error.to_string()))?;

    let auth_result = match &key.auth {
        SshAuth::Password(password) => session
            .authenticate_password(key.username.clone(), password.clone())
            .await
            .map_err(|error| SshTransportError::Connection(error.to_string()))?,
        SshAuth::PrivateKey(path) => {
            let private_key = russh::keys::load_secret_key(path, None).map_err(|_| {
                SshTransportError::PrivateKey("could not load configured private key".into())
            })?;
            let private_key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(private_key), None);
            session
                .authenticate_publickey(key.username.clone(), private_key)
                .await
                .map_err(|error| SshTransportError::Connection(error.to_string()))?
        }
    };
    if !auth_result.success() {
        return Err(SshTransportError::AuthenticationFailed);
    }
    Ok(session)
}
