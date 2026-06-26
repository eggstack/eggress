use std::future::Future;
use std::pin::Pin;

use eggress_uri::{EndpointSpec, ProtocolSpec, ProxyHopSpec};

use crate::connector::{Connector, DirectConnector};
use crate::{BoxStream, ConnectError, TargetAddr, TargetHost};

/// A boxed future that resolves to a handshake result.
type HandshakeFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<BoxStream, Box<dyn std::error::Error + Send + Sync>>>
            + Send
            + 'a,
    >,
>;

/// Errors that can occur during chain execution.
#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    #[error("hop {hop_index}: connection to {endpoint} failed: {source}")]
    ConnectFailed {
        hop_index: usize,
        endpoint: String,
        source: ConnectError,
    },

    #[error("hop {hop_index}: {protocol} handshake failed: {source}")]
    HandshakeFailed {
        hop_index: usize,
        protocol: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("chain is empty, at least one hop is required")]
    EmptyChain,

    #[error("invalid chain: {reason}")]
    InvalidChain { reason: String },
}

/// Error type for protocol handshake operations.
#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("connection refused")]
    ConnectionRefused,

    #[error("authentication failed")]
    AuthFailed,

    #[error("{0}")]
    Other(String),
}

/// Trait for performing protocol-specific handshakes through proxy hops.
///
/// Each protocol (HTTP CONNECT, SOCKS4, SOCKS5) provides an implementation
/// of this trait to handle its specific handshake logic.
///
/// # Dyn Compatibility
///
/// This trait is dyn-compatible and can be used with `Box<dyn HopHandler>`.
pub trait HopHandler: Send + Sync {
    /// Returns the protocol this handler supports.
    fn protocol(&self) -> ProtocolSpec;

    /// Perform the protocol handshake over the given stream.
    ///
    /// The handler should:
    /// 1. Perform the protocol-specific handshake (e.g., HTTP CONNECT, SOCKS5 greeting)
    /// 2. Request connection to the specified target
    /// 3. Return the upgraded stream on success
    fn handshake<'a>(
        &'a self,
        stream: BoxStream,
        target: &'a TargetAddr,
        hop: &'a ProxyHopSpec,
    ) -> HandshakeFuture<'a>;
}

/// A function that wraps a `BoxStream` in TLS for upstream connections.
///
/// Returns the TLS-wrapped stream, or an error if the handshake fails.
pub type TlsWrapper = Box<
    dyn Fn(
            BoxStream,
            String,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<BoxStream, Box<dyn std::error::Error + Send + Sync>>,
                    > + Send,
            >,
        > + Send
        + Sync,
>;

/// Executor for proxy chains.
///
/// Establishes a connection through a series of proxy hops, performing
/// the appropriate protocol handshake at each step.
///
/// # Chain Execution Flow
///
/// ```text
/// Transport to hop 1
/// → TLS wrap (if hop.tls)
/// → protocol handshake requesting hop 2
/// → TLS wrap (if hop.tls)
/// → protocol handshake requesting hop 3
/// → final protocol handshake requesting destination
/// ```
pub struct ChainExecutor {
    direct_connector: DirectConnector,
    handlers: Vec<Box<dyn HopHandler>>,
    tls_wrapper: Option<TlsWrapper>,
    shared_tls_config: Option<std::sync::Arc<rustls::ClientConfig>>,
}

impl ChainExecutor {
    /// Creates a new `ChainExecutor` with the given protocol handlers.
    pub fn new(handlers: Vec<Box<dyn HopHandler>>) -> Self {
        Self {
            direct_connector: DirectConnector,
            handlers,
            tls_wrapper: None,
            shared_tls_config: None,
        }
    }

    /// Set a TLS wrapper for upstream hops with `tls: true`.
    pub fn with_tls_wrapper(mut self, wrapper: TlsWrapper) -> Self {
        self.tls_wrapper = Some(wrapper);
        self
    }

    /// Set a shared TLS client config for protocols that need their own TLS
    /// handshake (e.g., Trojan).
    pub fn with_shared_tls_config(
        mut self,
        config: Option<std::sync::Arc<rustls::ClientConfig>>,
    ) -> Self {
        self.shared_tls_config = config;
        self
    }

    /// Get the shared TLS client config, if set.
    pub fn shared_tls_config(&self) -> Option<&std::sync::Arc<rustls::ClientConfig>> {
        self.shared_tls_config.as_ref()
    }

    /// Execute a proxy chain to connect to the target.
    ///
    /// # Arguments
    /// * `chain` - The ordered list of proxy hops
    /// * `target` - The final destination to connect to
    ///
    /// # Returns
    /// A connected stream ready for data transfer, or a `ChainError`.
    pub async fn execute(
        &self,
        chain: &[ProxyHopSpec],
        target: &TargetAddr,
    ) -> Result<BoxStream, ChainError> {
        if chain.is_empty() {
            return Err(ChainError::EmptyChain);
        }

        self.validate_chain(chain)?;

        // Pre-flight: verify handlers exist for all protocols before connecting
        for (i, hop) in chain.iter().enumerate() {
            find_handler(&self.handlers, &hop.protocols).map_err(|_| ChainError::InvalidChain {
                reason: format!(
                    "hop {i}: no handler for protocols: [{}]",
                    hop.protocols
                        .iter()
                        .map(|p| format!("{p:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })?;
        }

        // Step 1: Connect to the first hop's endpoint
        let first_hop = &chain[0];
        let first_hop_addr = endpoint_to_target_addr(&first_hop.endpoint)?;

        let mut current_stream = self
            .direct_connector
            .connect(&first_hop_addr)
            .await
            .map_err(|e| ChainError::ConnectFailed {
                hop_index: 0,
                endpoint: first_hop_addr.to_string(),
                source: e,
            })?;

        // Step 2: For each hop, perform the protocol handshake
        for (i, hop) in chain.iter().enumerate() {
            // Apply TLS wrapping if configured for this hop
            if hop.tls {
                if let Some(ref tls_wrapper) = self.tls_wrapper {
                    let server_name = hop
                        .server_name
                        .clone()
                        .unwrap_or_else(|| hop.endpoint.host.clone());
                    current_stream =
                        tls_wrapper(current_stream, server_name)
                            .await
                            .map_err(|e| ChainError::HandshakeFailed {
                                hop_index: i,
                                protocol: "tls".to_string(),
                                source: e,
                            })?;
                }
            }

            // Determine the target for this hop's handshake
            let next_target = if i + 1 < chain.len() {
                // Target is the next hop's endpoint
                endpoint_to_target_addr(&chain[i + 1].endpoint)?
            } else {
                // Last hop targets the actual destination
                target.clone()
            };

            let handler = find_handler(&self.handlers, &hop.protocols)?;

            current_stream = handler
                .handshake(current_stream, &next_target, hop)
                .await
                .map_err(|e| ChainError::HandshakeFailed {
                    hop_index: i,
                    protocol: format_protocols(&hop.protocols),
                    source: e,
                })?;
        }

        Ok(current_stream)
    }

    /// Validate the chain configuration.
    fn validate_chain(&self, chain: &[ProxyHopSpec]) -> Result<(), ChainError> {
        for (i, hop) in chain.iter().enumerate() {
            if hop.protocols.is_empty() {
                return Err(ChainError::InvalidChain {
                    reason: format!("hop {i}: no protocols specified"),
                });
            }
            if hop.endpoint.host.is_empty() {
                return Err(ChainError::InvalidChain {
                    reason: format!("hop {i}: empty endpoint host"),
                });
            }
            if hop.endpoint.port == 0 {
                return Err(ChainError::InvalidChain {
                    reason: format!("hop {i}: port cannot be 0"),
                });
            }
        }
        Ok(())
    }
}

/// Convert an `EndpointSpec` to a `TargetAddr`.
fn endpoint_to_target_addr(endpoint: &EndpointSpec) -> Result<TargetAddr, ChainError> {
    let host = if let Ok(ip) = endpoint.host.parse::<std::net::IpAddr>() {
        TargetHost::Ip(ip)
    } else {
        TargetHost::Domain(endpoint.host.clone())
    };
    Ok(TargetAddr {
        host,
        port: endpoint.port,
    })
}

/// Find a handler that supports one of the given protocols.
fn find_handler<'a>(
    handlers: &'a [Box<dyn HopHandler>],
    protocols: &[ProtocolSpec],
) -> Result<&'a dyn HopHandler, ChainError> {
    for handler in handlers {
        if protocols.contains(&handler.protocol()) {
            return Ok(handler.as_ref());
        }
    }
    Err(ChainError::InvalidChain {
        reason: format!(
            "no handler for protocols: [{}]",
            protocols
                .iter()
                .map(|p| format!("{p:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

/// Format a list of protocols as a string.
fn format_protocols(protocols: &[ProtocolSpec]) -> String {
    protocols
        .iter()
        .map(|p| format!("{p:?}"))
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TargetHost;
    use eggress_uri::CredentialSpec;
    use std::sync::Arc;

    /// A mock handler that records the target and returns a successful result.
    struct MockHandler {
        protocol: ProtocolSpec,
        captured_target: std::sync::Arc<std::sync::Mutex<Option<TargetAddr>>>,
    }

    impl MockHandler {
        fn new(
            protocol: ProtocolSpec,
        ) -> (Self, std::sync::Arc<std::sync::Mutex<Option<TargetAddr>>>) {
            let captured_target = std::sync::Arc::new(std::sync::Mutex::new(None));
            let handler = Self {
                protocol,
                captured_target: captured_target.clone(),
            };
            (handler, captured_target)
        }
    }

    impl HopHandler for MockHandler {
        fn protocol(&self) -> ProtocolSpec {
            self.protocol
        }

        fn handshake<'a>(
            &'a self,
            stream: BoxStream,
            target: &'a TargetAddr,
            _hop: &'a ProxyHopSpec,
        ) -> HandshakeFuture<'a> {
            Box::pin(async move {
                *self.captured_target.lock().unwrap() = Some(target.clone());
                Ok(stream)
            })
        }
    }

    /// A mock handler that always fails.
    struct FailingHandler {
        protocol: ProtocolSpec,
        error_message: String,
    }

    impl HopHandler for FailingHandler {
        fn protocol(&self) -> ProtocolSpec {
            self.protocol
        }

        fn handshake<'a>(
            &'a self,
            _stream: BoxStream,
            _target: &'a TargetAddr,
            _hop: &'a ProxyHopSpec,
        ) -> HandshakeFuture<'a> {
            let msg = self.error_message.clone();
            Box::pin(async move { Err(msg.into()) })
        }
    }

    fn make_hop(protocol: ProtocolSpec, host: &str, port: u16) -> ProxyHopSpec {
        ProxyHopSpec {
            protocols: vec![protocol],
            endpoint: EndpointSpec {
                host: host.to_string(),
                port,
            },
            credentials: None,
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        }
    }

    fn make_hop_with_creds(
        protocol: ProtocolSpec,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> ProxyHopSpec {
        ProxyHopSpec {
            protocols: vec![protocol],
            endpoint: EndpointSpec {
                host: host.to_string(),
                port,
            },
            credentials: Some(CredentialSpec {
                username: username.to_string(),
                password: password.to_string(),
            }),
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        }
    }

    fn make_target(domain: &str, port: u16) -> TargetAddr {
        TargetAddr {
            host: TargetHost::Domain(domain.to_string()),
            port,
        }
    }

    fn make_ip_target(ip: std::net::IpAddr, port: u16) -> TargetAddr {
        TargetAddr {
            host: TargetHost::Ip(ip),
            port,
        }
    }

    // ===== Empty/Invalid Chain Tests =====

    #[tokio::test]
    async fn test_empty_chain() {
        let executor = ChainExecutor::new(vec![]);
        let target = make_target("example.com", 80);
        let result = executor.execute(&[], &target).await;
        match result {
            Err(e) => {
                assert!(matches!(e, ChainError::EmptyChain));
                assert_eq!(
                    e.to_string(),
                    "chain is empty, at least one hop is required"
                );
            }
            Ok(_) => panic!("expected EmptyChain error"),
        }
    }

    #[tokio::test]
    async fn test_hop_no_protocols() {
        let hop = ProxyHopSpec {
            protocols: vec![],
            endpoint: EndpointSpec {
                host: "127.0.0.1".to_string(),
                port: 8080,
            },
            credentials: None,
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        };
        let executor = ChainExecutor::new(vec![]);
        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;
        match result {
            Err(ChainError::InvalidChain { reason }) => {
                assert!(reason.contains("no protocols specified"));
            }
            _ => panic!("expected InvalidChain error"),
        }
    }

    #[tokio::test]
    async fn test_hop_empty_host() {
        let hop = make_hop(ProtocolSpec::Http, "", 8080);
        let executor = ChainExecutor::new(vec![]);
        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;
        match result {
            Err(ChainError::InvalidChain { reason }) => {
                assert!(reason.contains("empty endpoint host"));
            }
            _ => panic!("expected InvalidChain error"),
        }
    }

    #[tokio::test]
    async fn test_hop_zero_port() {
        let hop = ProxyHopSpec {
            protocols: vec![ProtocolSpec::Http],
            endpoint: EndpointSpec {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            credentials: None,
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        };
        let executor = ChainExecutor::new(vec![]);
        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;
        match result {
            Err(ChainError::InvalidChain { reason }) => {
                assert!(reason.contains("port cannot be 0"));
            }
            _ => panic!("expected InvalidChain error"),
        }
    }

    // ===== Missing Handler Tests =====

    #[tokio::test]
    async fn test_no_handler_for_protocol() {
        let executor = ChainExecutor::new(vec![]);
        let hop = make_hop(ProtocolSpec::Http, "127.0.0.1", 8080);
        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;
        match result {
            Err(ChainError::InvalidChain { reason }) => {
                assert!(reason.contains("no handler for protocols"));
            }
            _ => panic!("expected InvalidChain error"),
        }
    }

    // ===== Connection Failure Tests =====

    #[tokio::test]
    async fn test_connect_failed() {
        let (handler, _) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(handler)]);
        let hop = make_hop(ProtocolSpec::Http, "127.0.0.1", 1);
        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;
        match result {
            Err(ChainError::ConnectFailed {
                hop_index, source, ..
            }) => {
                assert_eq!(hop_index, 0);
                assert!(matches!(source, ConnectError::Io(_)));
            }
            Err(e) => panic!("expected ConnectFailed, got: {e}"),
            Ok(_) => panic!("expected error"),
        }
    }

    // ===== Handshake Failure Tests =====

    #[tokio::test]
    async fn test_handshake_failed() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let failing_handler: Box<dyn HopHandler> = Box::new(FailingHandler {
            protocol: ProtocolSpec::Http,
            error_message: "handshake timeout".to_string(),
        });

        let executor = ChainExecutor::new(vec![failing_handler]);
        let hop = make_hop(ProtocolSpec::Http, &addr.ip().to_string(), addr.port());
        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;

        match result {
            Err(ChainError::HandshakeFailed {
                hop_index,
                protocol,
                source,
            }) => {
                assert_eq!(hop_index, 0);
                assert_eq!(protocol, "Http");
                assert_eq!(source.to_string(), "handshake timeout");
            }
            Err(e) => panic!("expected HandshakeFailed, got: {e}"),
            Ok(_) => panic!("expected error"),
        }

        server_jh.abort();
    }

    // ===== Domain Name Preservation Tests =====

    #[tokio::test]
    async fn test_domain_preserved_for_single_hop() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler, captured) = MockHandler::new(ProtocolSpec::Socks5);
        let executor = ChainExecutor::new(vec![Box::new(handler)]);

        let hop = make_hop(ProtocolSpec::Socks5, &addr.ip().to_string(), addr.port());
        let target = make_target("example.com", 443);
        let result = executor.execute(&[hop], &target).await;

        assert!(result.is_ok());

        let captured_target = captured.lock().unwrap().take().unwrap();
        assert_eq!(
            captured_target.host,
            TargetHost::Domain("example.com".to_string())
        );
        assert_eq!(captured_target.port, 443);

        server_jh.abort();
    }

    #[tokio::test]
    async fn test_domain_preserved_through_two_hops() {
        let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr1 = listener1.local_addr().unwrap();

        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();

        let server_jh1 = tokio::spawn(async move {
            let (_stream, _) = listener1.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let server_jh2 = tokio::spawn(async move {
            let (_stream, _) = listener2.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler1, captured1) = MockHandler::new(ProtocolSpec::Socks5);
        let (handler2, captured2) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(handler1), Box::new(handler2)]);

        let hop1 = make_hop(ProtocolSpec::Socks5, &addr1.ip().to_string(), addr1.port());
        let hop2 = make_hop(ProtocolSpec::Http, &addr2.ip().to_string(), addr2.port());
        let target = make_target("example.com", 443);
        let result = executor.execute(&[hop1, hop2], &target).await;

        assert!(result.is_ok());

        let target1 = captured1.lock().unwrap().take().unwrap();
        assert_eq!(target1, make_ip_target(addr2.ip(), addr2.port()));

        let target2 = captured2.lock().unwrap().take().unwrap();
        assert_eq!(target2.host, TargetHost::Domain("example.com".to_string()));
        assert_eq!(target2.port, 443);

        server_jh1.abort();
        server_jh2.abort();
    }

    // ===== Multi-hop Chain Tests =====

    #[tokio::test]
    async fn test_single_hop_chain() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler, captured) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(handler)]);

        let hop = make_hop(ProtocolSpec::Http, &addr.ip().to_string(), addr.port());
        let target = make_target("destination.example.com", 443);
        let result = executor.execute(&[hop], &target).await;

        assert!(result.is_ok());

        let captured_target = captured.lock().unwrap().take().unwrap();
        assert_eq!(
            captured_target.host,
            TargetHost::Domain("destination.example.com".to_string())
        );
        assert_eq!(captured_target.port, 443);

        server_jh.abort();
    }

    #[tokio::test]
    async fn test_two_hop_chain() {
        let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr1 = listener1.local_addr().unwrap();

        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();

        let server_jh1 = tokio::spawn(async move {
            let (_stream, _) = listener1.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let server_jh2 = tokio::spawn(async move {
            let (_stream, _) = listener2.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler1, captured1) = MockHandler::new(ProtocolSpec::Socks5);
        let (handler2, captured2) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(handler1), Box::new(handler2)]);

        let hop1 = make_hop(ProtocolSpec::Socks5, &addr1.ip().to_string(), addr1.port());
        let hop2 = make_hop(ProtocolSpec::Http, &addr2.ip().to_string(), addr2.port());
        let target = make_target("final.example.com", 443);
        let result = executor.execute(&[hop1, hop2], &target).await;

        assert!(result.is_ok());

        let target1 = captured1.lock().unwrap().take().unwrap();
        assert_eq!(target1.host, TargetHost::Ip(addr2.ip()));
        assert_eq!(target1.port, addr2.port());

        let target2 = captured2.lock().unwrap().take().unwrap();
        assert_eq!(
            target2.host,
            TargetHost::Domain("final.example.com".to_string())
        );
        assert_eq!(target2.port, 443);

        server_jh1.abort();
        server_jh2.abort();
    }

    #[tokio::test]
    async fn test_three_hop_chain() {
        let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr1 = listener1.local_addr().unwrap();

        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();

        let listener3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr3 = listener3.local_addr().unwrap();

        let server_jh1 = tokio::spawn(async move {
            let (_stream, _) = listener1.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let server_jh2 = tokio::spawn(async move {
            let (_stream, _) = listener2.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let server_jh3 = tokio::spawn(async move {
            let (_stream, _) = listener3.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler1, captured1) = MockHandler::new(ProtocolSpec::Socks5);
        let (handler2, captured2) = MockHandler::new(ProtocolSpec::Http);
        let (handler3, captured3) = MockHandler::new(ProtocolSpec::Socks4);
        let executor = ChainExecutor::new(vec![
            Box::new(handler1),
            Box::new(handler2),
            Box::new(handler3),
        ]);

        let hop1 = make_hop(ProtocolSpec::Socks5, &addr1.ip().to_string(), addr1.port());
        let hop2 = make_hop(ProtocolSpec::Http, &addr2.ip().to_string(), addr2.port());
        let hop3 = make_hop(ProtocolSpec::Socks4, &addr3.ip().to_string(), addr3.port());
        let target = make_target("final.example.com", 8080);
        let result = executor.execute(&[hop1, hop2, hop3], &target).await;

        assert!(result.is_ok());

        let target1 = captured1.lock().unwrap().take().unwrap();
        assert_eq!(target1.host, TargetHost::Ip(addr2.ip()));
        assert_eq!(target1.port, addr2.port());

        let target2 = captured2.lock().unwrap().take().unwrap();
        assert_eq!(target2.host, TargetHost::Ip(addr3.ip()));
        assert_eq!(target2.port, addr3.port());

        let target3 = captured3.lock().unwrap().take().unwrap();
        assert_eq!(
            target3.host,
            TargetHost::Domain("final.example.com".to_string())
        );
        assert_eq!(target3.port, 8080);

        server_jh1.abort();
        server_jh2.abort();
        server_jh3.abort();
    }

    // ===== Credential Passing Tests =====

    #[tokio::test]
    async fn test_credentials_passed_to_handler() {
        use std::sync::Arc;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        struct CapturingHandler {
            protocol: ProtocolSpec,
            captured_creds: Arc<std::sync::Mutex<Option<CredentialSpec>>>,
        }

        impl HopHandler for CapturingHandler {
            fn protocol(&self) -> ProtocolSpec {
                self.protocol
            }

            fn handshake<'a>(
                &'a self,
                stream: BoxStream,
                _target: &'a TargetAddr,
                hop: &'a ProxyHopSpec,
            ) -> HandshakeFuture<'a> {
                Box::pin(async move {
                    if let Some(creds) = hop.credentials.as_ref() {
                        *self.captured_creds.lock().unwrap() = Some(creds.clone());
                    }
                    Ok(stream)
                })
            }
        }

        let captured_creds = Arc::new(std::sync::Mutex::new(None));
        let handler: Box<dyn HopHandler> = Box::new(CapturingHandler {
            protocol: ProtocolSpec::Http,
            captured_creds: captured_creds.clone(),
        });

        let executor = ChainExecutor::new(vec![handler]);

        let hop = make_hop_with_creds(
            ProtocolSpec::Http,
            &addr.ip().to_string(),
            addr.port(),
            "testuser",
            "testpass",
        );
        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;

        assert!(result.is_ok());

        let creds = captured_creds.lock().unwrap().take().unwrap();
        assert_eq!(creds.username, "testuser");
        assert_eq!(creds.password, "testpass");

        server_jh.abort();
    }

    // ===== Handler Selection Tests =====

    #[tokio::test]
    async fn test_handler_selection_first_matching() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler, _) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(handler)]);

        let hop = ProxyHopSpec {
            protocols: vec![ProtocolSpec::Http, ProtocolSpec::Socks5],
            endpoint: EndpointSpec {
                host: addr.ip().to_string(),
                port: addr.port(),
            },
            credentials: None,
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        };
        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;

        assert!(result.is_ok());

        server_jh.abort();
    }

    // ===== Error Chain Index Tests =====

    #[tokio::test]
    async fn test_error_identifies_failing_hop() {
        let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr1 = listener1.local_addr().unwrap();

        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();

        let server_jh1 = tokio::spawn(async move {
            let (_stream, _) = listener1.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let server_jh2 = tokio::spawn(async move {
            let (_stream, _) = listener2.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (good_handler, _) = MockHandler::new(ProtocolSpec::Socks5);
        let bad_handler: Box<dyn HopHandler> = Box::new(FailingHandler {
            protocol: ProtocolSpec::Http,
            error_message: "proxy refused connection".to_string(),
        });

        let executor = ChainExecutor::new(vec![Box::new(good_handler), bad_handler]);

        let hop1 = make_hop(ProtocolSpec::Socks5, &addr1.ip().to_string(), addr1.port());
        let hop2 = make_hop(ProtocolSpec::Http, &addr2.ip().to_string(), addr2.port());
        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop1, hop2], &target).await;

        match result {
            Err(ChainError::HandshakeFailed {
                hop_index, source, ..
            }) => {
                assert_eq!(hop_index, 1, "error should identify hop 1 (second hop)");
                assert_eq!(source.to_string(), "proxy refused connection");
            }
            Err(e) => panic!("expected HandshakeFailed, got: {e}"),
            Ok(_) => panic!("expected error"),
        }

        server_jh1.abort();
        server_jh2.abort();
    }

    // ===== IP Address Endpoint Tests =====

    #[tokio::test]
    async fn test_ip_endpoint_resolved() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler, captured) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(handler)]);

        let hop = make_hop(ProtocolSpec::Http, &addr.ip().to_string(), addr.port());
        let target = make_ip_target("93.184.216.34".parse().unwrap(), 443);
        let result = executor.execute(&[hop], &target).await;

        assert!(result.is_ok());

        let captured_target = captured.lock().unwrap().take().unwrap();
        assert_eq!(
            captured_target.host,
            TargetHost::Ip("93.184.216.34".parse().unwrap())
        );
        assert_eq!(captured_target.port, 443);

        server_jh.abort();
    }

    // ===== Mixed Protocol Chain Tests =====

    #[tokio::test]
    async fn test_socks5_to_http_chain() {
        let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr1 = listener1.local_addr().unwrap();

        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();

        let server_jh1 = tokio::spawn(async move {
            let (_stream, _) = listener1.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let server_jh2 = tokio::spawn(async move {
            let (_stream, _) = listener2.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler1, captured1) = MockHandler::new(ProtocolSpec::Socks5);
        let (handler2, captured2) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(handler1), Box::new(handler2)]);

        let hop1 = make_hop(ProtocolSpec::Socks5, &addr1.ip().to_string(), addr1.port());
        let hop2 = make_hop(ProtocolSpec::Http, &addr2.ip().to_string(), addr2.port());
        let target = make_target("target.example.com", 8080);
        let result = executor.execute(&[hop1, hop2], &target).await;

        assert!(result.is_ok());

        let target1 = captured1.lock().unwrap().take().unwrap();
        assert_eq!(target1.host, TargetHost::Ip(addr2.ip()));

        let target2 = captured2.lock().unwrap().take().unwrap();
        assert_eq!(
            target2.host,
            TargetHost::Domain("target.example.com".to_string())
        );

        server_jh1.abort();
        server_jh2.abort();
    }

    #[tokio::test]
    async fn test_http_to_socks5_chain() {
        let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr1 = listener1.local_addr().unwrap();

        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();

        let server_jh1 = tokio::spawn(async move {
            let (_stream, _) = listener1.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let server_jh2 = tokio::spawn(async move {
            let (_stream, _) = listener2.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler1, captured1) = MockHandler::new(ProtocolSpec::Http);
        let (handler2, captured2) = MockHandler::new(ProtocolSpec::Socks5);
        let executor = ChainExecutor::new(vec![Box::new(handler1), Box::new(handler2)]);

        let hop1 = make_hop(ProtocolSpec::Http, &addr1.ip().to_string(), addr1.port());
        let hop2 = make_hop(ProtocolSpec::Socks5, &addr2.ip().to_string(), addr2.port());
        let target = make_target("target.example.com", 443);
        let result = executor.execute(&[hop1, hop2], &target).await;

        assert!(result.is_ok());

        let target1 = captured1.lock().unwrap().take().unwrap();
        assert_eq!(target1.host, TargetHost::Ip(addr2.ip()));

        let target2 = captured2.lock().unwrap().take().unwrap();
        assert_eq!(
            target2.host,
            TargetHost::Domain("target.example.com".to_string())
        );

        server_jh1.abort();
        server_jh2.abort();
    }

    #[tokio::test]
    async fn test_socks5_to_socks5_chain() {
        use std::sync::Arc;

        let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr1 = listener1.local_addr().unwrap();

        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();

        let server_jh1 = tokio::spawn(async move {
            let (_stream, _) = listener1.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let server_jh2 = tokio::spawn(async move {
            let (_stream, _) = listener2.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        struct MultiTargetRecorder {
            protocol: ProtocolSpec,
            targets: Arc<std::sync::Mutex<Vec<TargetAddr>>>,
        }

        impl HopHandler for MultiTargetRecorder {
            fn protocol(&self) -> ProtocolSpec {
                self.protocol
            }

            fn handshake<'a>(
                &'a self,
                stream: BoxStream,
                target: &'a TargetAddr,
                _hop: &'a ProxyHopSpec,
            ) -> HandshakeFuture<'a> {
                let targets = self.targets.clone();
                let target_clone = target.clone();
                Box::pin(async move {
                    targets.lock().unwrap().push(target_clone);
                    Ok(stream)
                })
            }
        }

        let targets = Arc::new(std::sync::Mutex::new(Vec::new()));
        let handler: Box<dyn HopHandler> = Box::new(MultiTargetRecorder {
            protocol: ProtocolSpec::Socks5,
            targets: targets.clone(),
        });
        let executor = ChainExecutor::new(vec![handler]);

        let hop1 = make_hop(ProtocolSpec::Socks5, &addr1.ip().to_string(), addr1.port());
        let hop2 = make_hop(ProtocolSpec::Socks5, &addr2.ip().to_string(), addr2.port());
        let target = make_target("target.example.com", 443);
        let result = executor.execute(&[hop1, hop2], &target).await;

        assert!(result.is_ok());

        let captured_targets = targets.lock().unwrap();
        assert_eq!(captured_targets.len(), 2);

        // First SOCKS5 hop targets second SOCKS5 proxy
        assert_eq!(captured_targets[0].host, TargetHost::Ip(addr2.ip()));
        assert_eq!(captured_targets[0].port, addr2.port());

        // Second SOCKS5 hop targets final destination (domain preserved)
        assert_eq!(
            captured_targets[1].host,
            TargetHost::Domain("target.example.com".to_string())
        );
        assert_eq!(captured_targets[1].port, 443);

        server_jh1.abort();
        server_jh2.abort();
    }

    #[tokio::test]
    async fn test_http_to_http_chain() {
        use std::sync::Arc;

        let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr1 = listener1.local_addr().unwrap();

        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();

        let server_jh1 = tokio::spawn(async move {
            let (_stream, _) = listener1.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let server_jh2 = tokio::spawn(async move {
            let (_stream, _) = listener2.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        // Handler that records all targets it receives
        struct MultiTargetRecorder {
            protocol: ProtocolSpec,
            targets: Arc<std::sync::Mutex<Vec<TargetAddr>>>,
        }

        impl HopHandler for MultiTargetRecorder {
            fn protocol(&self) -> ProtocolSpec {
                self.protocol
            }

            fn handshake<'a>(
                &'a self,
                stream: BoxStream,
                target: &'a TargetAddr,
                _hop: &'a ProxyHopSpec,
            ) -> HandshakeFuture<'a> {
                let targets = self.targets.clone();
                let target_clone = target.clone();
                Box::pin(async move {
                    targets.lock().unwrap().push(target_clone);
                    Ok(stream)
                })
            }
        }

        let targets = Arc::new(std::sync::Mutex::new(Vec::new()));
        let handler: Box<dyn HopHandler> = Box::new(MultiTargetRecorder {
            protocol: ProtocolSpec::Http,
            targets: targets.clone(),
        });
        let executor = ChainExecutor::new(vec![handler]);

        let hop1 = make_hop(ProtocolSpec::Http, &addr1.ip().to_string(), addr1.port());
        let hop2 = make_hop(ProtocolSpec::Http, &addr2.ip().to_string(), addr2.port());
        let target = make_target("target.example.com", 80);
        let result = executor.execute(&[hop1, hop2], &target).await;

        assert!(result.is_ok());

        let captured_targets = targets.lock().unwrap();
        assert_eq!(captured_targets.len(), 2);

        // First hop should target second proxy
        assert_eq!(captured_targets[0].host, TargetHost::Ip(addr2.ip()));
        assert_eq!(captured_targets[0].port, addr2.port());

        // Second hop should target final destination
        assert_eq!(
            captured_targets[1].host,
            TargetHost::Domain("target.example.com".to_string())
        );
        assert_eq!(captured_targets[1].port, 80);

        server_jh1.abort();
        server_jh2.abort();
    }

    // ===== Chain Validation Tests =====

    #[test]
    fn test_validate_chain_valid() {
        let executor = ChainExecutor::new(vec![]);
        let chain = vec![
            make_hop(ProtocolSpec::Http, "127.0.0.1", 8080),
            make_hop(ProtocolSpec::Socks5, "127.0.0.1", 1080),
        ];
        assert!(executor.validate_chain(&chain).is_ok());
    }

    #[test]
    fn test_validate_chain_empty_protocols() {
        let executor = ChainExecutor::new(vec![]);
        let chain = vec![ProxyHopSpec {
            protocols: vec![],
            endpoint: EndpointSpec {
                host: "127.0.0.1".to_string(),
                port: 8080,
            },
            credentials: None,
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        }];
        assert!(executor.validate_chain(&chain).is_err());
    }

    #[test]
    fn test_validate_chain_empty_host() {
        let executor = ChainExecutor::new(vec![]);
        let chain = vec![make_hop(ProtocolSpec::Http, "", 8080)];
        assert!(executor.validate_chain(&chain).is_err());
    }

    #[test]
    fn test_validate_chain_zero_port() {
        let executor = ChainExecutor::new(vec![]);
        let chain = vec![ProxyHopSpec {
            protocols: vec![ProtocolSpec::Http],
            endpoint: EndpointSpec {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            credentials: None,
            rule: None,
            local_bind: None,
            tls: false,
            server_name: None,
        }];
        assert!(executor.validate_chain(&chain).is_err());
    }

    // ===== Error Display Tests =====

    #[test]
    fn test_chain_error_display() {
        let err = ChainError::EmptyChain;
        assert_eq!(
            err.to_string(),
            "chain is empty, at least one hop is required"
        );

        let err = ChainError::InvalidChain {
            reason: "test reason".to_string(),
        };
        assert_eq!(err.to_string(), "invalid chain: test reason");

        let err = ChainError::ConnectFailed {
            hop_index: 0,
            endpoint: "127.0.0.1:8080".to_string(),
            source: ConnectError::ConnectionRefused,
        };
        assert!(err.to_string().contains("hop 0"));
        assert!(err.to_string().contains("127.0.0.1:8080"));
        assert!(err.to_string().contains("connection refused"));
    }

    // ===== TLS Wrapping Tests =====

    #[tokio::test]
    async fn test_tls_wrapper_called_when_hop_tls_true() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (_handler, _captured) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(_handler)]);

        let tls_called = Arc::new(AtomicBool::new(false));
        let tls_called_clone = tls_called.clone();

        let tls_wrapper: TlsWrapper = Box::new(move |stream, _server_name| {
            let called = tls_called_clone.clone();
            Box::pin(async move {
                called.store(true, Ordering::Relaxed);
                // Just pass through - don't actually do TLS in this test
                Ok(stream)
            })
        });

        let executor = executor.with_tls_wrapper(tls_wrapper);

        let mut hop = make_hop(ProtocolSpec::Http, &addr.ip().to_string(), addr.port());
        hop.tls = true;
        hop.server_name = Some("test.example.com".to_string());

        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;

        assert!(result.is_ok());
        assert!(
            tls_called.load(Ordering::Relaxed),
            "TLS wrapper should have been called"
        );

        server_jh.abort();
    }

    #[tokio::test]
    async fn test_tls_wrapper_not_called_when_hop_tls_false() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (_handler, _captured) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(_handler)]);

        let tls_called = Arc::new(AtomicBool::new(false));
        let tls_called_clone = tls_called.clone();

        let tls_wrapper: TlsWrapper = Box::new(move |stream, _server_name| {
            let called = tls_called_clone.clone();
            Box::pin(async move {
                called.store(true, Ordering::Relaxed);
                Ok(stream)
            })
        });

        let executor = executor.with_tls_wrapper(tls_wrapper);

        let hop = make_hop(ProtocolSpec::Http, &addr.ip().to_string(), addr.port());
        // hop.tls defaults to false

        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;

        assert!(result.is_ok());
        assert!(
            !tls_called.load(Ordering::Relaxed),
            "TLS wrapper should NOT have been called"
        );

        server_jh.abort();
    }

    #[tokio::test]
    async fn test_tls_wrapper_uses_server_name_from_hop() {
        use std::sync::Mutex;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler, _) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(handler)]);

        let captured_name = Arc::new(Mutex::new(None::<String>));
        let captured_name_clone = captured_name.clone();

        let tls_wrapper: TlsWrapper = Box::new(move |stream, server_name| {
            let captured = captured_name_clone.clone();
            Box::pin(async move {
                *captured.lock().unwrap() = Some(server_name);
                Ok(stream)
            })
        });

        let executor = executor.with_tls_wrapper(tls_wrapper);

        let mut hop = make_hop(ProtocolSpec::Http, &addr.ip().to_string(), addr.port());
        hop.tls = true;
        hop.server_name = Some("custom-sni.example.com".to_string());

        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;

        assert!(result.is_ok());
        let name = captured_name.lock().unwrap().take().unwrap();
        assert_eq!(name, "custom-sni.example.com");

        server_jh.abort();
    }

    #[tokio::test]
    async fn test_tls_wrapper_falls_back_to_endpoint_host() {
        use std::sync::Mutex;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler, _) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(handler)]);

        let captured_name = Arc::new(Mutex::new(None::<String>));
        let captured_name_clone = captured_name.clone();

        let tls_wrapper: TlsWrapper = Box::new(move |stream, server_name| {
            let captured = captured_name_clone.clone();
            Box::pin(async move {
                *captured.lock().unwrap() = Some(server_name);
                Ok(stream)
            })
        });

        let executor = executor.with_tls_wrapper(tls_wrapper);

        // hop with no server_name - should use endpoint host
        let hop = make_hop(ProtocolSpec::Http, &addr.ip().to_string(), addr.port());
        // hop.tls defaults to false, so set it to true
        let mut hop = hop;
        hop.tls = true;
        // No server_name set - should fallback to endpoint.host

        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;

        assert!(result.is_ok());
        let name = captured_name.lock().unwrap().take().unwrap();
        assert_eq!(name, addr.ip().to_string());

        server_jh.abort();
    }

    #[tokio::test]
    async fn test_tls_failure_propagates_as_handshake_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_jh = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (handler, _) = MockHandler::new(ProtocolSpec::Http);
        let executor = ChainExecutor::new(vec![Box::new(handler)]);

        let tls_wrapper: TlsWrapper = Box::new(|_stream, _server_name| {
            Box::pin(async move {
                Err(Box::<dyn std::error::Error + Send + Sync>::from(
                    "TLS handshake failed: certificate rejected",
                ))
            })
        });

        let executor = executor.with_tls_wrapper(tls_wrapper);

        let mut hop = make_hop(ProtocolSpec::Http, &addr.ip().to_string(), addr.port());
        hop.tls = true;

        let target = make_target("example.com", 80);
        let result = executor.execute(&[hop], &target).await;

        match result {
            Err(ChainError::HandshakeFailed {
                hop_index,
                protocol,
                source,
            }) => {
                assert_eq!(hop_index, 0);
                assert_eq!(protocol, "tls");
                assert!(source.to_string().contains("TLS handshake failed"));
            }
            Err(e) => panic!("expected HandshakeFailed, got: {e}"),
            Ok(_) => panic!("expected error"),
        }

        server_jh.abort();
    }
}
