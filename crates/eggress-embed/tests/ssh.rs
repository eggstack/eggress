#![cfg(all(feature = "ssh", feature = "pproxy-compat"))]

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

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
    async fn start() -> io::Result<Option<Self>> {
        let sshd = command_path("sshd");
        let ssh_keygen = command_path("ssh-keygen");
        let missing_tools = [
            ("sshd", sshd.is_none()),
            ("ssh-keygen", ssh_keygen.is_none()),
        ]
        .into_iter()
        .filter_map(|(command, missing)| missing.then_some(command))
        .collect::<Vec<_>>();
        if !missing_tools.is_empty() {
            let message = format!(
                "OpenSSH test tools unavailable: {}",
                missing_tools.join(", ")
            );
            if require_openssh_tests() {
                return Err(io::Error::new(io::ErrorKind::NotFound, message));
            }
            eprintln!("skipping embed SSH test: {message}");
            return Ok(None);
        }
        let sshd = sshd.expect("checked above");
        let ssh_keygen = ssh_keygen.expect("checked above");

        let dir = tempfile::tempdir()?;
        let host_key = dir.path().join("host_key");
        let private_key = dir.path().join("client_key");
        run_checked(
            Command::new(&ssh_keygen)
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&host_key),
        )?;
        run_checked(
            Command::new(&ssh_keygen)
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&private_key),
        )?;

        let authorized_keys = dir.path().join("authorized_keys");
        std::fs::copy(private_key.with_extension("pub"), &authorized_keys)?;
        let port = ephemeral_port().await?;
        let user = std::env::var("USER")
            .ok()
            .filter(|user| !user.is_empty())
            .ok_or_else(|| io::Error::other("USER is not set for the OpenSSH fixture"))?;
        let config = dir.path().join("sshd_config");
        let config_text = format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\nPasswordAuthentication yes\nKbdInteractiveAuthentication no\nChallengeResponseAuthentication no\nUsePAM no\nPermitRootLogin yes\nPubkeyAuthentication yes\nAllowTcpForwarding yes\nAllowStreamLocalForwarding yes\nGatewayPorts no\nStrictModes no\nUseDNS no\nLogLevel QUIET\n",
            host_key.display(),
            authorized_keys.display()
        );
        std::fs::write(&config, config_text)?;

        let child = Command::new(&sshd)
            .args(["-D", "-e", "-f"])
            .arg(&config)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let fixture = Self {
            _dir: dir,
            child,
            addr: SocketAddr::from(([127, 0, 0, 1], port)),
            user,
            private_key,
        };
        wait_for_port(fixture.addr).await?;
        Ok(Some(fixture))
    }

    fn key_uri(&self) -> String {
        format!(
            "ssh://{}::{}@{}:{}",
            self.user,
            self.private_key.display(),
            self.addr.ip(),
            self.addr.port()
        )
    }

    fn toml(&self) -> String {
        format!(
            "version = 1\n\n[[upstreams]]\nid = \"ssh\"\nuri = \"{}\"\n",
            self.key_uri()
        )
    }
}

fn require_openssh_tests() -> bool {
    std::env::var_os("EGRESS_REQUIRE_OPENSSH_TESTS").is_some_and(|value| value == "1")
}

impl Drop for OpenSsh {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn command_path(command: &str) -> Option<PathBuf> {
    let output = Command::new("sh")
        .args(["-c", &format!("command -v {command}")])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout);
    let path = path.lines().next()?.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
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

async fn wait_for_port(addr: SocketAddr) -> io::Result<()> {
    // Wait for the SSH banner, not just TCP acceptance, so the first real
    // handshake does not reset while sshd is still starting under load.
    for _ in 0..100 {
        if let Ok(mut stream) = tokio::net::TcpStream::connect(addr).await {
            let mut banner = [0u8; 128];
            match tokio::time::timeout(
                Duration::from_millis(200),
                tokio::io::AsyncReadExt::read(&mut stream, &mut banner),
            )
            .await
            {
                Ok(Ok(count)) if count > 0 => {
                    if String::from_utf8_lossy(&banner[..count]).contains("SSH-") {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "OpenSSH fixture did not accept connections",
    ))
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

async fn start_ssh_observing_proxy(
    backend: SocketAddr,
    handshakes: std::sync::Arc<std::sync::Mutex<Vec<SocketAddr>>>,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut client, peer)) = listener.accept().await else {
                return;
            };
            let handshakes = handshakes.clone();
            tokio::spawn(async move {
                let banner = tokio::time::timeout(Duration::from_secs(5), async {
                    let mut banner = Vec::new();
                    loop {
                        let byte = client.read_u8().await?;
                        banner.push(byte);
                        if byte == b'\n' {
                            return Ok::<_, io::Error>(banner);
                        }
                        if banner.len() > 255 {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "SSH identification line exceeded 255 bytes",
                            ));
                        }
                    }
                })
                .await;
                let Ok(Ok(banner)) = banner else {
                    return;
                };
                handshakes.lock().unwrap().push(peer);
                let Ok(mut upstream) = tokio::net::TcpStream::connect(backend).await else {
                    return;
                };
                if upstream.write_all(&banner).await.is_err() {
                    return;
                }
                let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
            });
        }
    });
    (addr, task)
}

fn ssh_executor(
    sessions: std::sync::Arc<eggress_transport_ssh::SshSessionCache>,
) -> eggress_core::chain::ChainExecutor {
    eggress_embed::outbound::build_chain_executor_with_options(
        eggress_embed::outbound::OutboundExecutorOptions::new().with_ssh_sessions(sessions),
    )
}

fn ssh_chain_uri(fixture: &OpenSsh, endpoint: SocketAddr) -> String {
    format!(
        "ssh://{}::{}@{}:{}",
        fixture.user,
        fixture.private_key.display(),
        endpoint.ip(),
        endpoint.port()
    )
}

async fn execute_ssh_chain(
    executor: &eggress_core::chain::ChainExecutor,
    uri: &str,
    target: SocketAddr,
) -> Result<eggress_core::BoxStream, eggress_core::chain::ChainError> {
    let chain = eggress_uri::parse_proxy_chain(uri).unwrap();
    let target = format!("{}:{}", target.ip(), target.port())
        .parse()
        .unwrap();
    executor.execute(&chain.hops, &target).await
}

#[tokio::test]
async fn ssh_hop_zero_default_policy_reuses_session() {
    let Some(fixture) = OpenSsh::start()
        .await
        .expect("OpenSSH fixture setup failed")
    else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let handshakes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (proxy_addr, proxy_task) =
        start_ssh_observing_proxy(fixture.addr, handshakes.clone()).await;
    let sessions = std::sync::Arc::new(eggress_transport_ssh::SshSessionCache::new_compatibility());
    let executor = ssh_executor(sessions.clone());
    let uri = ssh_chain_uri(&fixture, proxy_addr);

    for _ in 0..2 {
        let mut stream = execute_ssh_chain(&executor, &uri, echo_addr).await.unwrap();
        stream.write_all(b"cached-ssh").await.unwrap();
        let mut response = [0; 10];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"cached-ssh");
        drop(stream);
    }

    assert_eq!(handshakes.lock().unwrap().len(), 1);
    sessions.shutdown().await;
    proxy_task.abort();
    echo_task.abort();
}

#[tokio::test]
async fn ssh_hop_zero_local_bind_uses_fresh_session() {
    let Some(fixture) = OpenSsh::start()
        .await
        .expect("OpenSSH fixture setup failed")
    else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let handshakes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (proxy_addr, proxy_task) =
        start_ssh_observing_proxy(fixture.addr, handshakes.clone()).await;
    let sessions = std::sync::Arc::new(eggress_transport_ssh::SshSessionCache::new_compatibility());
    let executor = ssh_executor(sessions.clone());
    let uri = ssh_chain_uri(&fixture, proxy_addr);

    let mut ordinary_stream = execute_ssh_chain(&executor, &uri, echo_addr).await.unwrap();
    ordinary_stream.write_all(b"ordinary-ssh").await.unwrap();
    let mut response = [0; 12];
    ordinary_stream.read_exact(&mut response).await.unwrap();
    assert_eq!(&response, b"ordinary-ssh");
    drop(ordinary_stream);

    let mut bound_stream = None;
    let mut requested_port = 0;
    for _ in 0..8 {
        requested_port = ephemeral_port().await.unwrap();
        let bound_uri = format!("{uri}@127.0.0.1:{requested_port}");
        if let Ok(stream) = execute_ssh_chain(&executor, &bound_uri, echo_addr).await {
            bound_stream = Some(stream);
            break;
        }
    }
    let mut bound_stream = bound_stream.expect("a reserved loopback source port should bind");
    bound_stream.write_all(b"bound-ssh").await.unwrap();
    let mut response = [0; 9];
    bound_stream.read_exact(&mut response).await.unwrap();
    assert_eq!(&response, b"bound-ssh");
    drop(bound_stream);

    let observed = handshakes.lock().unwrap();
    assert_eq!(
        observed.len(),
        2,
        "local-bind must establish a fresh SSH session"
    );
    assert!(
        observed.iter().any(|peer| peer.port() == requested_port),
        "SSH proxy must observe the explicit source port {requested_port}"
    );
    drop(observed);
    sessions.shutdown().await;
    proxy_task.abort();
    echo_task.abort();
}

#[tokio::test]
async fn openssh_nested_ssh_does_not_cross_reuse_prefixes() {
    let Some(prefix_a) = OpenSsh::start()
        .await
        .expect("first OpenSSH prefix setup failed")
    else {
        return;
    };
    let Some(prefix_b) = OpenSsh::start()
        .await
        .expect("second OpenSSH prefix setup failed")
    else {
        return;
    };
    let Some(nested) = OpenSsh::start()
        .await
        .expect("nested OpenSSH endpoint setup failed")
    else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let handshakes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (proxy_addr, proxy_task) = start_ssh_observing_proxy(nested.addr, handshakes.clone()).await;
    let sessions = std::sync::Arc::new(eggress_transport_ssh::SshSessionCache::new_compatibility());
    let executor = ssh_executor(sessions.clone());

    let chain_a = format!(
        "{}__{}",
        ssh_chain_uri(&prefix_a, prefix_a.addr),
        ssh_chain_uri(&nested, proxy_addr)
    );
    let chain_b = format!(
        "{}__{}",
        ssh_chain_uri(&prefix_b, prefix_b.addr),
        ssh_chain_uri(&nested, proxy_addr)
    );
    for chain in [&chain_a, &chain_b] {
        let mut stream = execute_ssh_chain(&executor, chain, echo_addr)
            .await
            .unwrap();
        stream.write_all(b"nested-prefix").await.unwrap();
        let mut response = [0; 13];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"nested-prefix");
        drop(stream);
    }

    assert_eq!(
        handshakes.lock().unwrap().len(),
        2,
        "each selected SSH prefix must carry a fresh nested SSH handshake"
    );
    sessions.shutdown().await;
    proxy_task.abort();
    echo_task.abort();
}

#[tokio::test]
async fn pproxy_connector_ssh_transports_bytes() {
    let Some(fixture) = OpenSsh::start()
        .await
        .expect("OpenSSH fixture setup failed")
    else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let connector =
        eggress_embed::outbound::OutboundConnector::from_pproxy_uri(&fixture.key_uri()).unwrap();

    // Retry transient transport resets while sshd finishes starting. This
    // path uses a valid key, so any error here is a startup race or a real
    // bug; retrying a real bug just delays the same failure.
    let (mut stream, info) = {
        let mut attempts = 0;
        loop {
            attempts += 1;
            match connector
                .connect_tcp(&echo_addr.ip().to_string(), echo_addr.port())
                .await
            {
                Ok(established) => break established,
                Err(_) if attempts < 50 => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Err(error) => panic!("embed SSH connect failed: {error:?}"),
            }
        }
    };
    assert_eq!(info.hop_count, 1);
    stream.write_all(b"embed-ssh-echo").await.unwrap();
    let mut received = [0; 14];
    stream.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"embed-ssh-echo");

    echo_task.abort();
}

#[tokio::test]
async fn pproxy_connector_ssh_auth_failure_is_fail_closed_and_redacted() {
    let Some(fixture) = OpenSsh::start()
        .await
        .expect("OpenSSH fixture setup failed")
    else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let secret = "embed-invalid-ssh-password";
    let uri = format!(
        "ssh://{}:{}@{}:{}",
        fixture.user,
        secret,
        fixture.addr.ip(),
        fixture.addr.port()
    );
    let connector = eggress_embed::outbound::OutboundConnector::from_pproxy_uri(&uri).unwrap();
    let error = match connector
        .connect_tcp(&echo_addr.ip().to_string(), echo_addr.port())
        .await
    {
        Ok(_) => panic!("invalid SSH credentials must not fall back to direct egress"),
        Err(error) => error,
    };
    let rendered = format!("{error:?} {error}");
    assert!(
        !rendered.contains(secret),
        "SSH password leaked: {rendered}"
    );
    echo_task.abort();
}

#[tokio::test]
async fn native_toml_connector_rejects_untrusted_ssh_host_key() {
    let Some(fixture) = OpenSsh::start()
        .await
        .expect("OpenSSH fixture setup failed")
    else {
        return;
    };
    let (echo_addr, echo_task) = start_echo().await;
    let connector = eggress_embed::outbound::OutboundConnector::from_toml(&fixture.toml()).unwrap();
    let error = match connector
        .connect_tcp(&echo_addr.ip().to_string(), echo_addr.port())
        .await
    {
        Ok(_) => panic!("native SSH must verify the host key"),
        Err(error) => error,
    };
    let rendered = format!("{error:?} {error}");
    assert!(
        rendered.contains("SSH"),
        "unexpected native SSH error: {rendered}"
    );
    echo_task.abort();
}
