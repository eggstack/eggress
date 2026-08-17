use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use eggress_core::BoxStream;
use eggress_transport_ssh::{SshAuth, SshSessionCache, SshSessionKey, SshTransportError};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct OpenSsh {
    _dir: TempDir,
    child: Child,
    addr: SocketAddr,
    user: String,
    private_key: PathBuf,
}

impl OpenSsh {
    async fn start() -> Option<Self> {
        if !command_available("sshd") || !command_available("ssh-keygen") {
            eprintln!("skipping OpenSSH transport test: sshd/ssh-keygen unavailable");
            return None;
        }

        let dir = tempfile::tempdir().ok()?;
        let host_key = dir.path().join("host_key");
        let private_key = dir.path().join("client_key");
        run_checked(
            Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&host_key),
        )
        .ok()?;
        run_checked(
            Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&private_key),
        )
        .ok()?;

        let authorized_keys = dir.path().join("authorized_keys");
        std::fs::copy(private_key.with_extension("pub"), &authorized_keys).ok()?;
        let port = ephemeral_port().await.ok()?;
        let user = std::env::var("USER").ok().filter(|user| !user.is_empty())?;
        let config = dir.path().join("sshd_config");
        let config_text = format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\nPasswordAuthentication yes\nKbdInteractiveAuthentication no\nChallengeResponseAuthentication no\nUsePAM no\nPermitRootLogin yes\nPubkeyAuthentication yes\nAllowTcpForwarding yes\nAllowStreamLocalForwarding yes\nGatewayPorts no\nStrictModes no\nUseDNS no\nLogLevel QUIET\n",
            host_key.display(),
            authorized_keys.display()
        );
        std::fs::write(&config, config_text).ok()?;

        let child = Command::new("sshd")
            .args(["-D", "-e", "-f"])
            .arg(&config)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let fixture = Self {
            _dir: dir,
            child,
            addr: SocketAddr::from(([127, 0, 0, 1], port)),
            user,
            private_key,
        };
        if !wait_for_port(fixture.addr).await {
            return None;
        }
        Some(fixture)
    }

    fn key(&self, hop_index: usize) -> SshSessionKey {
        SshSessionKey {
            host: self.addr.ip().to_string(),
            port: self.addr.port(),
            username: self.user.clone(),
            auth: SshAuth::PrivateKey(self.private_key.display().to_string()),
            hop_index,
        }
    }

    fn password_key(&self, password: &str) -> SshSessionKey {
        SshSessionKey {
            host: self.addr.ip().to_string(),
            port: self.addr.port(),
            username: self.user.clone(),
            auth: SshAuth::Password(password.to_string()),
            hop_index: 0,
        }
    }
}

impl Drop for OpenSsh {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn command_available(command: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {command} >/dev/null 2>&1")])
        .status()
        .is_ok_and(|status| status.success())
}

fn run_checked(command: &mut Command) -> io::Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("fixture command failed"))
    }
}

async fn ephemeral_port() -> io::Result<u16> {
    Ok(tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await?
        .local_addr()?
        .port())
}

async fn wait_for_port(addr: SocketAddr) -> bool {
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

async fn start_echo() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = [0u8; 4096];
                while let Ok(count) = stream.read(&mut buffer).await {
                    if count == 0 || stream.write_all(&buffer[..count]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    (addr, task)
}

async fn open_channel(
    cache: &SshSessionCache,
    fixture: &OpenSsh,
    target: SocketAddr,
    hop_index: usize,
) -> Result<BoxStream, SshTransportError> {
    let stream: BoxStream = Box::new(tokio::net::TcpStream::connect(fixture.addr).await.unwrap());
    cache
        .open_tcp_channel(
            fixture.key(hop_index),
            stream,
            &target.ip().to_string(),
            target.port(),
        )
        .await
}

#[tokio::test]
async fn openssh_public_key_auth_and_direct_tcpip_echo() {
    let Some(fixture) = OpenSsh::start().await else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let cache = SshSessionCache::new();
    assert!(!format!("{:?}", fixture.key(0)).contains(&fixture.private_key.display().to_string()));
    let mut channel = open_channel(&cache, &fixture, echo_addr, 0).await.unwrap();
    channel.write_all(b"ssh-echo").await.unwrap();
    let mut received = [0; 8];
    channel.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"ssh-echo");
    cache.shutdown().await;
    echo_task.abort();
}

#[tokio::test]
async fn openssh_password_failure_is_redacted_and_reconnect_is_explicit() {
    let Some(fixture) = OpenSsh::start().await else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let cache = SshSessionCache::new();
    let error = match open_channel_with_key(
        &cache,
        fixture.password_key("definitely-not-the-password"),
        fixture.addr,
        echo_addr,
    )
    .await
    {
        Ok(_) => panic!("invalid password unexpectedly authenticated"),
        Err(error) => error,
    };
    assert!(matches!(error, SshTransportError::AuthenticationFailed));
    assert!(!error.to_string().contains("definitely-not-the-password"));

    let key = fixture.key(0);
    let mut channel = open_channel(&cache, &fixture, echo_addr, 0).await.unwrap();
    channel.write_all(b"before").await.unwrap();
    let mut received = [0; 6];
    channel.read_exact(&mut received).await.unwrap();
    cache.invalidate(&key).await;
    let mut reconnected = open_channel(&cache, &fixture, echo_addr, 0).await.unwrap();
    reconnected.write_all(b"after").await.unwrap();
    let mut received = [0; 5];
    reconnected.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"after");
    cache.shutdown().await;
    let mut after_shutdown = open_channel(&cache, &fixture, echo_addr, 0).await.unwrap();
    after_shutdown.write_all(b"fresh").await.unwrap();
    let mut received = [0; 5];
    after_shutdown.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"fresh");
    cache.shutdown().await;
    echo_task.abort();
}

#[tokio::test]
async fn openssh_password_auth_and_direct_tcpip_echo_when_configured() {
    let Some(password) = std::env::var("EGRESS_SSH_TEST_PASSWORD")
        .ok()
        .filter(|password| !password.is_empty())
    else {
        eprintln!("skipping SSH password-auth success test: EGRESS_SSH_TEST_PASSWORD is unset");
        return;
    };
    let Some(fixture) = OpenSsh::start().await else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let cache = SshSessionCache::new();
    let mut channel = open_channel_with_key(
        &cache,
        fixture.password_key(&password),
        fixture.addr,
        echo_addr,
    )
    .await
    .unwrap();
    channel.write_all(b"password").await.unwrap();
    let mut received = [0; 8];
    channel.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"password");
    cache.shutdown().await;
    echo_task.abort();
}

async fn open_channel_with_key(
    cache: &SshSessionCache,
    key: SshSessionKey,
    ssh_addr: SocketAddr,
    target: SocketAddr,
) -> Result<BoxStream, SshTransportError> {
    let stream: BoxStream = Box::new(tokio::net::TcpStream::connect(ssh_addr).await.unwrap());
    cache
        .open_tcp_channel(key, stream, &target.ip().to_string(), target.port())
        .await
}

#[tokio::test]
async fn openssh_supports_concurrent_channels_over_one_cached_session() {
    let Some(fixture) = OpenSsh::start().await else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let cache = std::sync::Arc::new(SshSessionCache::new());
    let mut tasks = Vec::new();
    for index in 0..8u8 {
        let cache = cache.clone();
        let fixture_key = fixture.key(0);
        tasks.push(tokio::spawn(async move {
            let stream: BoxStream = Box::new(
                tokio::net::TcpStream::connect(("127.0.0.1", fixture_key.port))
                    .await
                    .unwrap(),
            );
            let mut channel = cache
                .open_tcp_channel(fixture_key, stream, "127.0.0.1", echo_addr.port())
                .await
                .unwrap();
            channel.write_all(&[index]).await.unwrap();
            let mut received = [0; 1];
            channel.read_exact(&mut received).await.unwrap();
            received[0]
        }));
    }
    for (index, task) in tasks.into_iter().enumerate() {
        assert_eq!(task.await.unwrap(), index as u8);
    }
    cache.shutdown().await;
    echo_task.abort();
}

#[tokio::test]
async fn openssh_chain_tunnels_second_ssh_hop_through_first() {
    let Some(first) = OpenSsh::start().await else {
        return;
    };
    let Some(second) = OpenSsh::start().await else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let cache = SshSessionCache::new();

    let first_transport: BoxStream =
        Box::new(tokio::net::TcpStream::connect(first.addr).await.unwrap());
    let through_first = cache
        .open_tcp_channel(
            first.key(0),
            first_transport,
            &second.addr.ip().to_string(),
            second.addr.port(),
        )
        .await
        .unwrap();
    let mut through_second = cache
        .open_tcp_channel(
            second.key(1),
            through_first,
            &echo_addr.ip().to_string(),
            echo_addr.port(),
        )
        .await
        .unwrap();
    through_second.write_all(b"two-hop").await.unwrap();
    let mut received = [0; 7];
    through_second.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"two-hop");
    cache.shutdown().await;
    echo_task.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn openssh_forwards_to_remote_unix_socket() {
    let Some(fixture) = OpenSsh::start().await else {
        return;
    };
    let socket_path = fixture._dir.path().join("target.sock");
    let listener = tokio::net::UnixListener::bind(&socket_path).unwrap();
    let echo_task = tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buffer = [0u8; 128];
        if let Ok(count) = stream.read(&mut buffer).await {
            let _ = stream.write_all(&buffer[..count]).await;
        }
    });
    let stream: BoxStream = Box::new(tokio::net::TcpStream::connect(fixture.addr).await.unwrap());
    let cache = SshSessionCache::new();
    let mut channel = cache
        .open_unix_channel(fixture.key(0), stream, &socket_path.display().to_string())
        .await
        .unwrap();
    channel.write_all(b"unix").await.unwrap();
    let mut received = [0; 4];
    channel.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"unix");
    cache.shutdown().await;
    echo_task.abort();
}

#[tokio::test]
async fn openssh_remote_tcp_forward_accepts_incoming_channel() {
    let Some(fixture) = OpenSsh::start().await else {
        return;
    };
    let cache = SshSessionCache::new();
    let stream: BoxStream = Box::new(tokio::net::TcpStream::connect(fixture.addr).await.unwrap());
    let mut forward = cache
        .start_remote_tcp_forward(fixture.key(0), stream, "127.0.0.1", 0)
        .await
        .unwrap();
    let mut incoming = tokio::net::TcpStream::connect(("127.0.0.1", forward.port()))
        .await
        .unwrap();
    let mut channel = forward.accept().await.expect("forwarded channel");
    incoming.write_all(b"remote-forward").await.unwrap();
    let mut received = [0; 14];
    channel.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"remote-forward");
    channel.write_all(&received).await.unwrap();
    let mut echoed = [0; 14];
    incoming.read_exact(&mut echoed).await.unwrap();
    assert_eq!(&echoed, b"remote-forward");
    forward.cancel().await.unwrap();
    cache.shutdown().await;
}
