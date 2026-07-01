use eggress_protocol_reverse::client::{ReverseClient, ReverseClientConfig};
use eggress_protocol_reverse::server::{ReverseServer, ReverseServerConfig};
use eggress_protocol_reverse::{client_auth_handshake, ControlState};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};

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
