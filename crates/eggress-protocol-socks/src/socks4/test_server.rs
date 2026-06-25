use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::error::Socks4Error;
use super::server::{read_socks4_request, write_socks4_reply, Socks4Status};

/// Behavior mode of the synthetic SOCKS4 server.
#[derive(Debug, Clone)]
pub enum TestServerMode {
    /// Returns CD=90 (Granted).
    Success,
    /// Returns CD=91 (Failed/Rejected).
    Rejected,
    /// Returns CD=92 (Identd unavailable).
    NoIdent,
    /// Returns CD=93 (Different user ID).
    DifferentUser,
    /// Returns 8 bytes of garbage instead of a valid reply.
    MalformedResponse,
    /// Returns a valid header but an unknown status code (e.g. CD=99).
    UnknownStatus,
    /// Sleeps before responding to simulate a slow server.
    SlowResponse,
    /// Accepts SOCKS4a domain requests and returns Granted.
    DomainSuccess,
    /// Disconnects without sending a reply.
    NoReply,
}

/// Handle to a running synthetic SOCKS4 server.
pub struct TestServerHandle {
    pub addr: SocketAddr,
}

impl TestServerHandle {
    /// Spawn the listener loop. Returns when the server is ready to accept.
    pub async fn spawn(mode: TestServerMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            Self::run(listener, mode).await;
        });

        Self { addr }
    }

    async fn run(listener: TcpListener, mode: TestServerMode) {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };

            let mode = mode.clone();
            tokio::spawn(async move {
                let _ = Self::handle_connection(&mut stream, &mode).await;
            });
        }
    }

    async fn handle_connection(
        stream: &mut tokio::net::TcpStream,
        mode: &TestServerMode,
    ) -> Result<(), Socks4Error> {
        let _request = read_socks4_request(stream).await?;
        let bound: SocketAddr = "127.0.0.1:0".parse().unwrap();

        match mode {
            TestServerMode::Success => {
                write_socks4_reply(stream, Socks4Status::Granted, bound).await?;
            }
            TestServerMode::Rejected => {
                write_socks4_reply(stream, Socks4Status::Failed, bound).await?;
            }
            TestServerMode::NoIdent => {
                write_socks4_reply(stream, Socks4Status::FailedNoIdent, bound).await?;
            }
            TestServerMode::DifferentUser => {
                write_socks4_reply(stream, Socks4Status::FailedDifferentUser, bound).await?;
            }
            TestServerMode::MalformedResponse => {
                stream
                    .write_all(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE])
                    .await?;
                stream.flush().await?;
            }
            TestServerMode::UnknownStatus => {
                // Valid SOCKS4 header format but with an unrecognized status code.
                let reply: [u8; 8] = [0x00, 99, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
                stream.write_all(&reply).await?;
                stream.flush().await?;
            }
            TestServerMode::SlowResponse => {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                write_socks4_reply(stream, Socks4Status::Granted, bound).await?;
            }
            TestServerMode::DomainSuccess => {
                write_socks4_reply(stream, Socks4Status::Granted, bound).await?;
            }
            TestServerMode::NoReply => {
                // Drop without sending anything.
                return Ok(());
            }
        }

        // For success modes, relay data until EOF.
        match mode {
            TestServerMode::Success | TestServerMode::DomainSuccess => {
                let mut buf = [0u8; 1024];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let _ = stream.write_all(&buf[..n]).await;
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}
