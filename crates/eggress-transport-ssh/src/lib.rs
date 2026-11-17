//! Optional SSH client transport used by pproxy-compatible upstream chains.
//!
//! The crate intentionally exposes only stream/channel operations.  It does
//! not expose remote command, SFTP, agent-forwarding, or SSH-server APIs.

use std::collections::HashMap;
use std::path::PathBuf;
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
///
/// `policy` scopes pooled sessions to one host-key verification policy so
/// a session authenticated under one policy (e.g. insecure compatibility)
/// can never be reused by a caller expecting another.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SshSessionKey {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
    pub hop_index: usize,
    pub policy: SshHostKeyPolicy,
}

impl std::fmt::Debug for SshSessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshSessionKey")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth", &self.auth)
            .field("hop_index", &self.hop_index)
            .field("policy", &self.policy)
            .finish()
    }
}

#[derive(Clone)]
struct CompatClient {
    forwarded_channels: Option<mpsc::Sender<russh::Channel<russh::client::Msg>>>,
    host: String,
    port: u16,
    host_key_policy: SshHostKeyPolicy,
}

impl CompatClient {
    fn cached(key: &SshSessionKey, host_key_policy: SshHostKeyPolicy) -> Self {
        Self {
            forwarded_channels: None,
            host: key.host.clone(),
            port: key.port,
            host_key_policy,
        }
    }

    fn remote_forward(
        forwarded_channels: mpsc::Sender<russh::Channel<russh::client::Msg>>,
        key: &SshSessionKey,
        host_key_policy: SshHostKeyPolicy,
    ) -> Self {
        Self {
            forwarded_channels: Some(forwarded_channels),
            host: key.host.clone(),
            port: key.port,
            host_key_policy,
        }
    }
}

/// Policy used to verify the SSH server's host key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SshHostKeyPolicy {
    /// Verify against the user's default OpenSSH known_hosts file.
    KnownHosts,
    /// Verify against an explicitly selected known_hosts file.
    KnownHostsFile(PathBuf),
    /// Disable verification for the explicitly selected pproxy compatibility path.
    InsecureCompatibility,
}

/// Decide whether a plain server host key is acceptable under `policy`.
///
/// This is the whole host-key trust decision, kept out of the `russh` callback
/// so it can be unit-tested without a live session. A lookup error (unreadable
/// or malformed known_hosts, or a changed key) is logged and rejected; it never
/// degrades into `InsecureCompatibility`.
fn verify_host_key(
    host: &str,
    port: u16,
    policy: &SshHostKeyPolicy,
    server_public_key: &russh::keys::PublicKey,
) -> bool {
    match policy {
        SshHostKeyPolicy::KnownHosts => {
            match russh::keys::check_known_hosts(host, port, server_public_key) {
                Ok(accepted) => accepted,
                Err(error) => {
                    // A missing or unreadable known_hosts file must not be
                    // indistinguishable from a key mismatch.
                    tracing::warn!(
                        host = %host,
                        port = port,
                        %error,
                        "known_hosts lookup failed; rejecting host key"
                    );
                    false
                }
            }
        }
        SshHostKeyPolicy::KnownHostsFile(path) => {
            match russh::keys::check_known_hosts_path(host, port, server_public_key, path) {
                Ok(accepted) => accepted,
                Err(error) => {
                    tracing::warn!(
                        host = %host,
                        port = port,
                        known_hosts = %path.display(),
                        %error,
                        "known_hosts lookup failed; rejecting host key"
                    );
                    false
                }
            }
        }
        SshHostKeyPolicy::InsecureCompatibility => true,
    }
}

impl russh::client::Handler for CompatClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // `russh` 0.65 reports certificate host keys separately from plain
        // host keys. Eggress has never accepted SSH certificate host keys: the
        // pre-0.65 graph could not even parse one, so the handshake failed
        // before a policy decision. Rejecting here keeps that fail-closed
        // behaviour instead of projecting a certificate onto a plain key, and
        // it applies to every policy including insecure compatibility.
        let server_public_key = match server_public_key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => key,
            russh::keys::PublicKeyOrCertificate::Certificate(_) => {
                tracing::warn!(
                    host = %self.host,
                    port = self.port,
                    "SSH server presented a certificate host key; certificate host keys are not supported"
                );
                return Ok(false);
            }
        };
        Ok(verify_host_key(
            &self.host,
            self.port,
            &self.host_key_policy,
            server_public_key,
        ))
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
                if forwarded_channels.try_send(channel).is_ok() {
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
    channels: mpsc::Receiver<russh::Channel<russh::client::Msg>>,
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
#[derive(Clone)]
pub struct SshSessionCache {
    sessions: Arc<Mutex<HashMap<SshSessionKey, Arc<SessionHandle>>>>,
    host_key_policy: SshHostKeyPolicy,
}

impl Default for SshSessionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SshSessionCache {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            host_key_policy: SshHostKeyPolicy::KnownHosts,
        }
    }

    /// Host-key verification policy scoping this cache's pooled sessions.
    pub fn policy(&self) -> SshHostKeyPolicy {
        self.host_key_policy.clone()
    }

    /// Construct the explicitly opted-in pproxy compatibility cache.
    ///
    /// Only available with the `pproxy-compat` feature so the insecure
    /// host-key policy cannot be selected accidentally.
    #[cfg(feature = "pproxy-compat")]
    pub fn new_compatibility() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            host_key_policy: SshHostKeyPolicy::InsecureCompatibility,
        }
    }

    /// Construct a cache using a caller-selected known_hosts file.
    pub fn with_known_hosts(path: PathBuf) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            host_key_policy: SshHostKeyPolicy::KnownHostsFile(path),
        }
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

    /// Open a TCP channel using only the supplied route stream. This session is
    /// never inserted into or looked up in the shared cache.
    pub async fn open_tcp_channel_fresh(
        &self,
        key: SshSessionKey,
        transport: SshStream,
        target_host: &str,
        target_port: u16,
    ) -> Result<SshStream, SshTransportError> {
        if target_port == 0 {
            return Err(SshTransportError::TargetPort(target_port));
        }
        let session =
            Arc::new(connect_fresh(key.clone(), transport, self.host_key_policy.clone()).await?);
        let channel = session
            .channel_open_direct_tcpip(target_host, u32::from(target_port), "127.0.0.1", 0)
            .await
            .map_err(|e| SshTransportError::Channel(e.to_string()))?;
        Ok(Box::new(SessionStream {
            inner: Box::new(channel.into_stream()),
            _session: session,
        }))
    }

    /// Open a Unix channel over a fresh SSH session on the supplied route.
    pub async fn open_unix_channel_fresh(
        &self,
        key: SshSessionKey,
        transport: SshStream,
        socket_path: &str,
    ) -> Result<SshStream, SshTransportError> {
        if socket_path.is_empty() {
            return Err(SshTransportError::Channel(
                "Unix target path is empty".into(),
            ));
        }
        let session =
            Arc::new(connect_fresh(key.clone(), transport, self.host_key_policy.clone()).await?);
        let channel = session
            .channel_open_direct_streamlocal(socket_path)
            .await
            .map_err(|e| SshTransportError::Channel(e.to_string()))?;
        Ok(Box::new(SessionStream {
            inner: Box::new(channel.into_stream()),
            _session: session,
        }))
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
        let (sender, channels) = mpsc::channel(128);
        let session = Arc::new(
            connect_authenticated(
                key.clone(),
                transport,
                CompatClient::remote_forward(sender, &key, self.host_key_policy.clone()),
            )
            .await?,
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
        {
            let mut sessions = self.sessions.lock().await;
            if let Some(session) = sessions.get(&key) {
                if !session.is_closed() {
                    drop(transport);
                    return Ok(Arc::clone(session));
                }
            }
            sessions.remove(&key);
        }

        if matches!(
            &self.host_key_policy,
            SshHostKeyPolicy::InsecureCompatibility
        ) {
            tracing::warn!(
                host = %key.host,
                port = key.port,
                "SSH compatibility transport disables host-key verification"
            );
        }
        let config = russh::client::Config {
            keepalive_interval: Some(Duration::from_secs(60)),
            keepalive_max: 3,
            ..Default::default()
        };
        let session = Arc::new(
            connect_authenticated_with_config(
                key.clone(),
                transport,
                CompatClient::cached(&key, self.host_key_policy.clone()),
                config,
            )
            .await?,
        );

        let mut sessions = self.sessions.lock().await;
        Ok(sessions
            .entry(key)
            .or_insert_with(|| Arc::clone(&session))
            .clone())
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

async fn connect_fresh(
    key: SshSessionKey,
    transport: SshStream,
    policy: SshHostKeyPolicy,
) -> Result<SessionHandle, SshTransportError> {
    if matches!(&policy, SshHostKeyPolicy::InsecureCompatibility) {
        tracing::warn!(host = %key.host, port = key.port, "SSH compatibility transport disables host-key verification");
    }
    connect_authenticated(key.clone(), transport, CompatClient::cached(&key, policy)).await
}

struct SessionStream {
    inner: SshStream,
    _session: Arc<SessionHandle>,
}
impl tokio::io::AsyncRead for SessionStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
impl tokio::io::AsyncWrite for SessionStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Public test vectors. They are host *public* keys and authorize nothing.
    const HOST_KEY_A: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBwetC7SGNq9kJMmuSzhc5wn7U9GiYrQdsOSemUfu8rK";
    const HOST_KEY_B: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG13LBHUOjmMmz1H4YO3A0sMYlX2X9Q8BSrN/4wR6TZZ";

    fn public_key(openssh: &str) -> russh::keys::PublicKey {
        russh::keys::PublicKey::from_openssh(openssh).expect("static test key parses")
    }

    fn known_hosts_entry(host: &str, port: u16, key: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("known_hosts");
        std::fs::write(&path, format!("[{host}]:{port} {key}\n")).expect("write known_hosts");
        (dir, path)
    }

    fn session_key(policy: SshHostKeyPolicy) -> SshSessionKey {
        SshSessionKey {
            host: "ssh.example".to_string(),
            port: 2222,
            username: "operator".to_string(),
            auth: SshAuth::Password("hunter2".to_string()),
            hop_index: 0,
            policy,
        }
    }

    #[test]
    fn ssh_auth_debug_never_shows_secrets() {
        let password = SshAuth::Password("hunter2".to_string());
        let private_key = SshAuth::PrivateKey("/home/operator/.ssh/id_ed25519".to_string());
        for (auth, secret) in [
            (&password, "hunter2"),
            (&private_key, "/home/operator/.ssh/id_ed25519"),
        ] {
            let rendered = format!("{auth:?}");
            assert!(rendered.contains("****"), "{rendered}");
            assert!(!rendered.contains(secret), "{rendered}");
        }
    }

    #[test]
    fn session_key_debug_never_shows_secrets() {
        let rendered = format!("{:?}", session_key(SshHostKeyPolicy::InsecureCompatibility));
        assert!(rendered.contains("****"), "{rendered}");
        assert!(!rendered.contains("hunter2"), "{rendered}");
    }

    #[test]
    fn session_key_is_scoped_by_host_key_policy() {
        let known = session_key(SshHostKeyPolicy::KnownHosts);
        let insecure = session_key(SshHostKeyPolicy::InsecureCompatibility);
        assert_ne!(known, insecure);
        let keys = HashSet::from([known, insecure]);
        assert_eq!(
            keys.len(),
            2,
            "a session authenticated under one policy must not collide with another"
        );
    }

    #[test]
    fn known_hosts_file_accepts_matching_key_and_rejects_mismatch() {
        let (_dir, path) = known_hosts_entry("127.0.0.1", 2222, HOST_KEY_A);
        let policy = SshHostKeyPolicy::KnownHostsFile(path);
        assert!(verify_host_key(
            "127.0.0.1",
            2222,
            &policy,
            &public_key(HOST_KEY_A)
        ));
        assert!(
            !verify_host_key("127.0.0.1", 2222, &policy, &public_key(HOST_KEY_B)),
            "a changed host key must be rejected"
        );
    }

    #[test]
    fn missing_known_hosts_file_rejects_host_key() {
        let dir = tempfile::tempdir().expect("temp dir");
        let policy = SshHostKeyPolicy::KnownHostsFile(dir.path().join("absent_known_hosts"));
        assert!(
            !verify_host_key("127.0.0.1", 2222, &policy, &public_key(HOST_KEY_A)),
            "an absent known_hosts file must fail closed"
        );
    }

    #[test]
    fn default_known_hosts_policy_rejects_unrecorded_host() {
        // A reserved `.invalid` name cannot be recorded in a real known_hosts
        // file, so the default policy is exercised without reading a match.
        let policy = SshHostKeyPolicy::KnownHosts;
        assert!(!verify_host_key(
            "eggress-ssh-policy.invalid",
            65022,
            &policy,
            &public_key(HOST_KEY_A)
        ));
    }

    #[test]
    fn insecure_compatibility_accepts_any_plain_key() {
        let policy = SshHostKeyPolicy::InsecureCompatibility;
        for key in [HOST_KEY_A, HOST_KEY_B] {
            assert!(verify_host_key(
                "127.0.0.1",
                2222,
                &policy,
                &public_key(key)
            ));
        }
    }

    #[test]
    fn cache_policy_accessors_match_their_constructors() {
        assert_eq!(
            SshSessionCache::new().policy(),
            SshHostKeyPolicy::KnownHosts
        );
        let path = PathBuf::from("known_hosts");
        assert_eq!(
            SshSessionCache::with_known_hosts(path.clone()).policy(),
            SshHostKeyPolicy::KnownHostsFile(path)
        );
    }

    #[cfg(feature = "pproxy-compat")]
    #[test]
    fn compatibility_cache_constructs_the_insecure_policy() {
        assert_eq!(
            SshSessionCache::new_compatibility().policy(),
            SshHostKeyPolicy::InsecureCompatibility
        );
    }
}
