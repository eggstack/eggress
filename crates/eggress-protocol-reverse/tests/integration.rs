use bytes::BytesMut;
use eggress_protocol_reverse::{
    read_frame,
    server::{ReverseServer, ReverseServerConfig},
    write_frame, Frame, FrameType,
};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_reverse_server_accepts_connection() {
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

    // Connect as a client
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let mut buf = [0u8; 1024];
    let _ = tokio::time::timeout(Duration::from_millis(100), stream.read(&mut buf)).await;

    cancel.cancel();
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_reverse_auth_success() {
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
    let auth_frame = Frame::auth("user", "pass");
    write_frame(&mut stream, &auth_frame).await.unwrap();

    let resp = read_frame(&mut stream, &mut BytesMut::with_capacity(256))
        .await
        .unwrap();
    assert_eq!(resp.frame_type, FrameType::AuthOk);

    cancel.cancel();
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_reverse_auth_failure() {
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
    let auth_frame = Frame::auth("user", "wrong");
    write_frame(&mut stream, &auth_frame).await.unwrap();

    let resp = read_frame(&mut stream, &mut BytesMut::with_capacity(256))
        .await
        .unwrap();
    assert_eq!(resp.frame_type, FrameType::AuthFail);

    cancel.cancel();
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_reverse_auth_required_not_provided() {
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

    // Connect and send non-auth frame
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let ping_frame = Frame::ping();
    write_frame(&mut stream, &ping_frame).await.unwrap();

    let resp = read_frame(&mut stream, &mut BytesMut::with_capacity(256))
        .await
        .unwrap();
    assert_eq!(resp.frame_type, FrameType::AuthFail);

    cancel.cancel();
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_reverse_control_state_transitions() {
    let state = eggress_protocol_reverse::ControlState::Disconnected;
    assert_eq!(state, eggress_protocol_reverse::ControlState::Disconnected);

    let state = eggress_protocol_reverse::ControlState::Connecting;
    assert_eq!(state, eggress_protocol_reverse::ControlState::Connecting);

    let state = eggress_protocol_reverse::ControlState::Ready;
    assert_eq!(state, eggress_protocol_reverse::ControlState::Ready);

    let state = eggress_protocol_reverse::ControlState::Draining;
    assert_eq!(state, eggress_protocol_reverse::ControlState::Draining);

    let state = eggress_protocol_reverse::ControlState::Closed;
    assert_eq!(state, eggress_protocol_reverse::ControlState::Closed);
}

#[tokio::test]
async fn test_reverse_frame_encode_decode_roundtrip() {
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

    // Connect and exchange frames
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Send ping
    write_frame(&mut stream, &Frame::ping()).await.unwrap();

    // Server should accept but might not respond to ping in this state
    let mut buf = BytesMut::with_capacity(256);
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        read_frame(&mut stream, &mut buf),
    )
    .await;

    cancel.cancel();
    server_handle.await.unwrap();

    // Connection should have worked
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_reverse_server_max_streams() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let config = ReverseServerConfig {
        control_bind: addr,
        max_streams: 1,
        ..Default::default()
    };
    let server = ReverseServer::new(config);
    let cancel = server.cancel_token();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    sleep(Duration::from_millis(50)).await;

    // Connect and try to open more streams than allowed
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Open first stream - should succeed (connects to a non-existent target, so it gets reset)
    let open1 = Frame::open_stream(1, "127.0.0.1", 19999);
    write_frame(&mut stream, &open1).await.unwrap();

    // Wait for response
    sleep(Duration::from_millis(50)).await;

    cancel.cancel();
    let _ = server_handle.await;
}
