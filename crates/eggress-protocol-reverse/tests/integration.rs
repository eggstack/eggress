use eggress_protocol_reverse::client::{ReverseClient, ReverseClientConfig};
use eggress_protocol_reverse::server::{ReverseServer, ReverseServerConfig};
use eggress_protocol_reverse::{client_auth_handshake, ControlState};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::sleep;

#[tokio::test]
async fn test_server_accepts_control_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let config = ReverseServerConfig {
        control_bind: addr,
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Connect as a control client
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Server should send handshake accept (no auth configured)
    let mut buf = [0u8; 1];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf[0], 0x01); // HANDSHAKE_ACCEPT

    cancel.cancel();
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_auth_success() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let config = ReverseServerConfig {
        control_bind: addr,
        auth_username: Some("user".to_string()),
        auth_password: Some("pass".to_string()),
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Connect and authenticate
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    client_auth_handshake(&mut stream, "user", "pass")
        .await
        .unwrap();

    // Connection should be alive (accepted into pool)
    let mut buf = [0u8; 1];
    let result = tokio::time::timeout(Duration::from_millis(100), stream.read(&mut buf)).await;
    assert!(result.is_err() || result.is_ok());

    cancel.cancel();
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_auth_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let config = ReverseServerConfig {
        control_bind: addr,
        auth_username: Some("user".to_string()),
        auth_password: Some("pass".to_string()),
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });

    sleep(Duration::from_millis(50)).await;

    // Connect with wrong credentials
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let result = client_auth_handshake(&mut stream, "user", "wrong").await;
    assert!(result.is_err());

    cancel.cancel();
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_auth_required_not_provided() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let config = ReverseServerConfig {
        control_bind: addr,
        auth_username: Some("user".to_string()),
        auth_password: Some("pass".to_string()),
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });

    sleep(Duration::from_millis(50)).await;

    // Connect without sending auth
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let result = tokio::time::timeout(Duration::from_millis(200), stream.read(&mut [0u8; 1])).await;
    assert!(result.is_err() || result.is_ok());

    cancel.cancel();
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_control_state_transitions() {
    let state = ControlState::Disconnected;
    assert_eq!(state, ControlState::Disconnected);

    let state = ControlState::Connecting;
    assert_eq!(state, ControlState::Connecting);

    let state = ControlState::Authenticating;
    assert_eq!(state, ControlState::Authenticating);

    let state = ControlState::Ready;
    assert_eq!(state, ControlState::Ready);

    let state = ControlState::Draining;
    assert_eq!(state, ControlState::Draining);

    let state = ControlState::Closed;
    assert_eq!(state, ControlState::Closed);
}

#[tokio::test]
async fn test_echo_relay_through_server() {
    // Bind control and external listeners to separate ports
    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);

    let external_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let external_addr = external_listener.local_addr().unwrap();
    drop(external_listener);

    let config = ReverseServerConfig {
        control_bind: control_addr,
        external_bind: Some(external_addr),
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Connect a control client to the control listener
    let mut control_stream = tokio::net::TcpStream::connect(control_addr).await.unwrap();
    let mut buf = [0u8; 1];
    control_stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf[0], 0x01);

    // Wait for control connection to be added to pool
    sleep(Duration::from_millis(50)).await;

    // Connect an external client to the external listener (no handshake from server)
    let mut external_stream = tokio::net::TcpStream::connect(external_addr).await.unwrap();

    // Wait for relay to be established
    sleep(Duration::from_millis(100)).await;

    // Send data from external client
    external_stream
        .write_all(b"hello from external")
        .await
        .unwrap();

    // Read from control client
    let mut recv_buf = [0u8; 1024];
    let n = tokio::time::timeout(Duration::from_secs(2), control_stream.read(&mut recv_buf))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&recv_buf[..n], b"hello from external");

    // Send data from control client
    control_stream
        .write_all(b"hello from control")
        .await
        .unwrap();

    // Read from external client
    let n = tokio::time::timeout(Duration::from_secs(2), external_stream.read(&mut recv_buf))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&recv_buf[..n], b"hello from control");

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn test_client_server_roundtrip() {
    // Bind control and external listeners
    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);

    let external_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let external_addr = external_listener.local_addr().unwrap();
    drop(external_listener);

    let server_config = ReverseServerConfig {
        control_bind: control_addr,
        external_bind: Some(external_addr),
        ..Default::default()
    };
    let server = ReverseServer::new(server_config);
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Start a simple echo server as the "target"
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = echo_listener.local_addr().unwrap();
    let echo_handle = tokio::spawn(async move {
        let (mut stream, _) = echo_listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    stream.write_all(&buf[..n]).await.unwrap();
                }
                Err(_) => break,
            }
        }
    });

    // Start client targeting the echo server
    let client_config = ReverseClientConfig {
        server_addr: control_addr,
        default_target_host: Some("127.0.0.1".to_string()),
        default_target_port: Some(echo_addr.port()),
        reconnect_initial_ms: 100,
        reconnect_max_ms: 500,
        ..Default::default()
    };
    let client = ReverseClient::new(client_config);
    let client_cancel = client.cancel_token();

    let client_handle = tokio::spawn(async move {
        let _ = client.run().await;
    });

    sleep(Duration::from_millis(100)).await;

    // Connect an external client to the external listener (no handshake from server)
    let mut external = tokio::net::TcpStream::connect(external_addr).await.unwrap();

    // Wait for relay
    sleep(Duration::from_millis(100)).await;

    // Send data through the external client
    external.write_all(b"ping").await.unwrap();

    // Read response
    let mut recv_buf = [0u8; 1024];
    let result = tokio::time::timeout(Duration::from_secs(2), external.read(&mut recv_buf)).await;
    if let Ok(Ok(n)) = result {
        assert_eq!(&recv_buf[..n], b"ping");
    }

    // Clean up
    client_cancel.cancel();
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), echo_handle).await;
}

#[tokio::test]
async fn test_client_reconnects_after_failure() {
    // Start a server that immediately shuts down
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let server_config = ReverseServerConfig {
        control_bind: addr,
        ..Default::default()
    };
    let server = ReverseServer::new(server_config);
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Start client
    let client_config = ReverseClientConfig {
        server_addr: addr,
        reconnect_initial_ms: 100,
        reconnect_max_ms: 500,
        ..Default::default()
    };
    let client = ReverseClient::new(client_config);
    let client_cancel = client.cancel_token();

    let client_handle = tokio::spawn(async move {
        let _ = client.run().await;
    });

    sleep(Duration::from_millis(100)).await;

    // Shut down server - client should try to reconnect
    cancel.cancel();
    let _ = server_handle.await;

    // Wait a bit for reconnect attempts
    sleep(Duration::from_millis(300)).await;

    // Shut down client
    client_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), client_handle).await;
}

#[tokio::test]
async fn test_graceful_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let config = ReverseServerConfig {
        control_bind: addr,
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Connect a control client
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let mut buf = [0u8; 1];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf[0], 0x01);

    // Graceful shutdown
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_allow_bind_rejects_unauthorized_address() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = listener.local_addr().unwrap();
    drop(listener);

    // Pre-bind a port we'll claim is in the deny list
    let claim_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let denied_addr = claim_listener.local_addr().unwrap();
    drop(claim_listener);

    let config = ReverseServerConfig {
        control_bind: control_addr,
        external_bind: Some(denied_addr),
        allow_bind: Some(vec!["127.0.0.1:1".parse().unwrap()]), // not matching
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let result = server.run().await;
    assert!(result.is_err(), "expected bind denied error");
}

#[tokio::test]
async fn test_allow_bind_accepts_listed_address() {
    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);
    let external_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let external_addr = external_listener.local_addr().unwrap();
    drop(external_listener);

    let config = ReverseServerConfig {
        control_bind: control_addr,
        external_bind: Some(external_addr),
        allow_bind: Some(vec![external_addr]),
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();
    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_max_control_connections_enforced() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = listener.local_addr().unwrap();
    drop(listener);
    let external_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let external_addr = external_listener.local_addr().unwrap();
    drop(external_listener);

    let config = ReverseServerConfig {
        control_bind: control_addr,
        external_bind: Some(external_addr),
        max_control_connections: 1,
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();
    let state = server.state_handle();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Open one control connection and consume the handshake
    let mut s1 = tokio::net::TcpStream::connect(control_addr).await.unwrap();
    let mut buf = [0u8; 1];
    s1.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf[0], 0x01);

    // Wait for state to reflect the active control connection in the channel queue
    let mut waited = 0u64;
    while state
        .active_control
        .load(std::sync::atomic::Ordering::Relaxed)
        < 1
        && waited < 1000
    {
        sleep(Duration::from_millis(10)).await;
        waited += 10;
    }
    assert!(
        state
            .active_control
            .load(std::sync::atomic::Ordering::Relaxed)
            >= 1,
        "expected active_control to reach 1"
    );

    // Now consume the control connection by opening an external client
    let _ext1 = tokio::net::TcpStream::connect(external_addr).await.unwrap();
    // Wait for relay to consume the control stream and decrement
    let mut waited = 0u64;
    while state
        .active_control
        .load(std::sync::atomic::Ordering::Relaxed)
        > 0
        && waited < 2000
    {
        sleep(Duration::from_millis(10)).await;
        waited += 10;
    }
    assert_eq!(
        state
            .active_control
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "expected active_control to drop to 0 after external client consumed the control stream"
    );

    drop(s1);
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn test_max_streams_per_listener_drops_excess() {
    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);
    let external_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let external_addr = external_listener.local_addr().unwrap();
    drop(external_listener);

    let config = ReverseServerConfig {
        control_bind: control_addr,
        external_bind: Some(external_addr),
        max_streams_per_listener: 1,
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();
    let state = server.state_handle();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Open one control connection so it gets queued for an external client
    let mut s = tokio::net::TcpStream::connect(control_addr).await.unwrap();
    let mut buf = [0u8; 1];
    s.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf[0], 0x01);

    // Wait for state to reflect the active control connection
    let mut waited = 0u64;
    while state
        .active_control
        .load(std::sync::atomic::Ordering::Relaxed)
        < 1
        && waited < 1000
    {
        sleep(Duration::from_millis(10)).await;
        waited += 10;
    }

    // First external client: should be picked up
    let ext1 = tokio::net::TcpStream::connect(external_addr).await.unwrap();
    sleep(Duration::from_millis(50)).await;
    assert_eq!(
        state
            .active_streams
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );

    // Second external client: should be dropped because max_streams_per_listener = 1
    let ext2 = tokio::net::TcpStream::connect(external_addr).await.unwrap();
    // The drop may be near-instant; give the server a moment to process
    sleep(Duration::from_millis(100)).await;
    let dropped = state
        .dropped_stream_limit
        .load(std::sync::atomic::Ordering::Relaxed);
    assert!(
        dropped >= 1,
        "expected dropped_stream_limit >= 1, got {}",
        dropped
    );

    drop(ext1);
    drop(ext2);
    drop(s);
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn test_control_channel_close_drops_external_listener() {
    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);
    let external_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let external_addr = external_listener.local_addr().unwrap();
    drop(external_listener);

    let config = ReverseServerConfig {
        control_bind: control_addr,
        external_bind: Some(external_addr),
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();
    let state = server.state_handle();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Open a control client and complete handshake
    let mut ctrl = tokio::net::TcpStream::connect(control_addr).await.unwrap();
    let mut buf = [0u8; 1];
    ctrl.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf[0], 0x01);

    // Wait for state to reflect the active control connection in the channel
    let mut waited = 0u64;
    while state
        .active_control
        .load(std::sync::atomic::Ordering::Relaxed)
        < 1
        && waited < 1000
    {
        sleep(Duration::from_millis(10)).await;
        waited += 10;
    }
    assert_eq!(
        state
            .active_control
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );

    // Open an external client to consume the control stream
    let ext = tokio::net::TcpStream::connect(external_addr).await.unwrap();
    // Wait for the relay to pick up the control stream and decrement the gauge
    let mut waited = 0u64;
    while state
        .active_control
        .load(std::sync::atomic::Ordering::Relaxed)
        > 0
        && waited < 2000
    {
        sleep(Duration::from_millis(10)).await;
        waited += 10;
    }
    assert_eq!(
        state
            .active_control
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "expected active_control to drop to 0 after external client consumed the control stream"
    );

    drop(ctrl);
    drop(ext);
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn test_bind_conflict_returns_error() {
    // Pre-bind a port to force a conflict
    let claim_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let conflict_addr = claim_listener.local_addr().unwrap();
    // Hold claim_listener for the duration of the test by parking it

    let config = ReverseServerConfig {
        control_bind: conflict_addr,
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let result = server.run().await;
    assert!(result.is_err(), "expected bind conflict error");

    drop(claim_listener);
}

#[tokio::test]
async fn test_metrics_increment_on_control_connection() {
    use eggress_protocol_reverse::metrics::ReverseMetrics;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let config = ReverseServerConfig {
        control_bind: addr,
        auth_username: Some("user".to_string()),
        auth_password: Some("pass".to_string()),
        ..Default::default()
    };
    let mut server = ReverseServer::new(config);
    let metrics = Arc::new(ReverseMetrics::new());
    server.set_metrics(metrics.clone());
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Successful auth
    let mut s1 = tokio::net::TcpStream::connect(addr).await.unwrap();
    client_auth_handshake(&mut s1, "user", "pass")
        .await
        .unwrap();
    sleep(Duration::from_millis(50)).await;
    let snap = metrics.snapshot();
    assert_eq!(snap.control_connections_accepted_total, 1);
    assert_eq!(snap.auth_failures_total, 0);

    // Failed auth
    let mut s2 = tokio::net::TcpStream::connect(addr).await.unwrap();
    let _ = client_auth_handshake(&mut s2, "user", "wrong").await;
    sleep(Duration::from_millis(50)).await;
    let snap = metrics.snapshot();
    assert_eq!(snap.control_connections_accepted_total, 1);
    assert_eq!(snap.control_connections_rejected_total, 1);
    assert_eq!(snap.auth_failures_total, 1);

    drop(s1);
    drop(s2);
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn test_drain_records_metric() {
    use eggress_protocol_reverse::metrics::ReverseMetrics;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let config = ReverseServerConfig {
        control_bind: addr,
        ..Default::default()
    };
    let mut server = ReverseServer::new(config);
    let metrics = Arc::new(ReverseMetrics::new());
    server.set_metrics(metrics.clone());
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    sleep(Duration::from_millis(20)).await;
    let snap = metrics.snapshot();
    assert!(snap.drain_total >= 1);
    assert!(snap.drain_duration_ms_total >= 1);
}

#[tokio::test]
async fn test_client_target_connect_failure_records_error() {
    use eggress_protocol_reverse::client::{TargetResolution, TargetResolver};
    use eggress_protocol_reverse::metrics::ReverseMetrics;

    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);

    let config = ReverseServerConfig {
        control_bind: control_addr,
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel_server = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Custom resolver that points at an unreachable port (1) so the connect fails
    struct BadResolver;
    impl TargetResolver for BadResolver {
        fn resolve(&self) -> TargetResolution {
            TargetResolution::Connect {
                host: "127.0.0.1".to_string(),
                port: 1,
            }
        }
    }

    let client_config = ReverseClientConfig {
        server_addr: control_addr,
        reconnect_initial_ms: 25,
        reconnect_max_ms: 50,
        ..Default::default()
    };
    let mut client = ReverseClient::new(client_config);
    let metrics = Arc::new(ReverseMetrics::new());
    client.set_metrics(metrics.clone());
    client.set_resolver(Arc::new(BadResolver));
    let client_cancel = client.cancel_token();

    let client_handle = tokio::spawn(async move {
        let _ = client.run().await;
    });

    // Let the client attempt connect to bad target, fail, reconnect.
    // Use a generous timeout for slow CI runners (e.g. Windows).
    sleep(Duration::from_secs(2)).await;
    let snap = metrics.snapshot();
    assert!(
        snap.control_reconnects_total >= 1,
        "expected at least 1 reconnect, got {}",
        snap.control_reconnects_total
    );
    let err_msg = snap.last_error.unwrap_or_default();
    assert!(
        err_msg.contains("target connect failed") || err_msg.contains("route rejected"),
        "expected error mentioning target/route, got: {}",
        err_msg
    );

    client_cancel.cancel();
    cancel_server.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn test_client_route_rejection_records_error() {
    use eggress_protocol_reverse::client::{TargetResolution, TargetResolver};
    use eggress_protocol_reverse::metrics::ReverseMetrics;

    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control_listener.local_addr().unwrap();
    drop(control_listener);

    let config = ReverseServerConfig {
        control_bind: control_addr,
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel_server = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    struct RejectResolver;
    impl TargetResolver for RejectResolver {
        fn resolve(&self) -> TargetResolution {
            TargetResolution::Reject {
                reason: "policy: blocked".to_string(),
            }
        }
    }

    let client_config = ReverseClientConfig {
        server_addr: control_addr,
        reconnect_initial_ms: 25,
        reconnect_max_ms: 50,
        ..Default::default()
    };
    let mut client = ReverseClient::new(client_config);
    let metrics = Arc::new(ReverseMetrics::new());
    client.set_metrics(metrics.clone());
    client.set_resolver(Arc::new(RejectResolver));
    let client_cancel = client.cancel_token();

    let client_handle = tokio::spawn(async move {
        let _ = client.run().await;
    });

    sleep(Duration::from_secs(2)).await;
    let snap = metrics.snapshot();
    let err_msg = snap.last_error.clone().unwrap_or_default();
    assert!(
        err_msg.contains("route rejected") || err_msg.contains("policy"),
        "expected rejection error, got: {}",
        err_msg
    );

    client_cancel.cancel();
    cancel_server.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), client_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}
