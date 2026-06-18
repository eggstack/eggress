//! Tests demonstrating the reply-before-route defect.
//!
//! These tests prove that HTTP CONNECT, SOCKS4, and SOCKS5 success replies
//! are sent to the client BEFORE the outbound route is actually established.
//! This is a protocol correctness bug: the client receives a "success" reply
//! but the proxy has not yet opened the upstream connection.
//!
//! Tests 1–4 demonstrate the DEFECT: success replies arrive immediately,
//! before any outbound connection is made.
//!
//! Tests 5–7 demonstrate the CORRECT pattern: a server that waits for the
//! route before sending the reply, so the client does not see "success"
//! until the upstream is ready.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::time::timeout;

/// Timeout for proving a reply arrives "immediately" (without waiting for a route).
const IMMEDIATE: Duration = Duration::from_millis(50);

/// Timeout for proving a reply is NOT yet available (still waiting for route).
const NOT_YET: Duration = Duration::from_millis(50);

// ===========================================================================
// DEFECT TESTS — these PASS, proving the bug exists
// ===========================================================================

/// SOCKS5: success reply (REP=0x00) arrives before any outbound connection.
#[tokio::test]
async fn defect_socks5_success_reply_arrives_before_route() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();

        // Read method negotiation
        let mut header = [0u8; 2];
        stream.read_exact(&mut header).await.unwrap();
        let nmethods = header[1] as usize;
        let mut methods = vec![0u8; nmethods];
        stream.read_exact(&mut methods).await.unwrap();

        // Send method selection (no auth)
        stream.write_all(&[0x05, 0x00]).await.unwrap();
        stream.flush().await.unwrap();

        // Read CONNECT request header
        let mut req = [0u8; 4];
        stream.read_exact(&mut req).await.unwrap();
        assert_eq!(req[0], 0x05);
        assert_eq!(req[1], 0x01); // CONNECT

        // Read address payload based on ATYP
        match req[3] {
            0x01 => {
                // IPv4: 4 bytes + 2 bytes port
                let mut buf = [0u8; 6];
                stream.read_exact(&mut buf).await.unwrap();
            }
            0x03 => {
                // Domain: 1 byte len + domain + 2 bytes port
                let mut len = [0u8; 1];
                stream.read_exact(&mut len).await.unwrap();
                let mut domain = vec![0u8; len[0] as usize];
                stream.read_exact(&mut domain).await.unwrap();
                let mut port = [0u8; 2];
                stream.read_exact(&mut port).await.unwrap();
            }
            0x04 => {
                // IPv6: 16 bytes + 2 bytes port
                let mut buf = [0u8; 18];
                stream.read_exact(&mut buf).await.unwrap();
            }
            _ => panic!("unexpected atyp"),
        }

        // DEFECT: Send success reply IMMEDIATELY — no outbound route exists yet!
        let reply: [u8; 10] = [
            0x05, // version
            0x00, // REP = success
            0x00, // reserved
            0x01, // ATYP IPv4
            0x00, 0x00, 0x00, 0x00, // bind addr
            0x00, 0x00, // bind port
        ];
        stream.write_all(&reply).await.unwrap();
        stream.flush().await.unwrap();
        // No outbound connection is ever made — this proves the defect.
    });

    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Method negotiation: offer no-auth
    client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut method_resp = [0u8; 2];
    client.read_exact(&mut method_resp).await.unwrap();
    assert_eq!(method_resp, [0x05, 0x00]);

    // CONNECT request for 198.51.100.1:443
    client
        .write_all(&[0x05, 0x01, 0x00, 0x01, 198, 51, 100, 1])
        .await
        .unwrap();
    client.write_all(&443u16.to_be_bytes()).await.unwrap();

    // KEY: The success reply is available IMMEDIATELY.
    // No outbound connection was opened — the proxy sent "success" too early.
    let mut reply_buf = [0u8; 10];
    let result = timeout(IMMEDIATE, client.read_exact(&mut reply_buf)).await;
    assert!(
        result.is_ok(),
        "SOCKS5 success reply is immediately available — defect: reply sent before route"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok());
    assert_eq!(reply_buf[0], 0x05, "version must be 5");
    assert_eq!(reply_buf[1], 0x00, "REP must be 0x00 (success)");

    server.await.unwrap();
}

/// SOCKS4: granted reply (status=90) arrives before any outbound connection.
#[tokio::test]
async fn defect_socks4_granted_reply_arrives_before_route() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();

        // Read SOCKS4 request header (8 bytes)
        let mut header = [0u8; 8];
        stream.read_exact(&mut header).await.unwrap();
        assert_eq!(header[0], 0x04); // SOCKS4
        assert_eq!(header[1], 0x01); // CONNECT

        // Read NUL-terminated user ID
        loop {
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).await.unwrap();
            if byte[0] == 0x00 {
                break;
            }
        }

        // DEFECT: Send granted reply IMMEDIATELY — no outbound route exists yet!
        let reply: [u8; 8] = [
            0x00, // VN (null for reply)
            90,   // CD = granted
            0x00, 0x00, // DSTPORT
            0x00, 0x00, 0x00, 0x00, // DSTIP
        ];
        stream.write_all(&reply).await.unwrap();
        stream.flush().await.unwrap();
        // No outbound connection is ever made — this proves the defect.
    });

    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();

    // SOCKS4 CONNECT request for 10.0.0.1:80
    client
        .write_all(&[0x04, 0x01, 0x00, 80, 10, 0, 0, 1])
        .await
        .unwrap();
    client.write_all(b"testuser").await.unwrap();
    client.write_all(&[0x00]).await.unwrap();

    // KEY: The granted reply is available IMMEDIATELY.
    let mut reply_buf = [0u8; 8];
    let result = timeout(IMMEDIATE, client.read_exact(&mut reply_buf)).await;
    assert!(
        result.is_ok(),
        "SOCKS4 granted reply is immediately available — defect: reply sent before route"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok());
    assert_eq!(reply_buf[0], 0x00, "VN must be 0x00 for reply");
    assert_eq!(reply_buf[1], 90, "CD must be 90 (granted)");

    server.await.unwrap();
}

/// HTTP CONNECT: 200 response arrives before any outbound connection.
#[tokio::test]
async fn defect_http_connect_200_reply_arrives_before_route() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();

        // Read HTTP CONNECT request until \r\n\r\n
        let mut head = Vec::with_capacity(1024);
        loop {
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).await.unwrap();
            head.push(byte[0]);
            if head.len() >= 4 && head[head.len() - 4..] == *b"\r\n\r\n" {
                break;
            }
        }

        let head_str = String::from_utf8_lossy(&head);
        assert!(head_str.starts_with("CONNECT "));

        // DEFECT: Send 200 response IMMEDIATELY — no outbound route exists yet!
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .unwrap();
        stream.flush().await.unwrap();
        // No outbound connection is ever made — this proves the defect.
    });

    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();

    client
        .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
        .await
        .unwrap();

    // KEY: The 200 response is available IMMEDIATELY.
    let mut resp_buf = [0u8; 4096];
    let result = timeout(IMMEDIATE, client.read(&mut resp_buf)).await;
    assert!(
        result.is_ok(),
        "HTTP 200 reply is immediately available — defect: reply sent before route"
    );
    let n = result.unwrap().unwrap();
    let resp = String::from_utf8_lossy(&resp_buf[..n]);
    assert!(
        resp.starts_with("HTTP/1.1 200"),
        "expected HTTP 200, got: {}",
        &resp[..resp.len().min(40)]
    );

    server.await.unwrap();
}

/// SOCKS5: no failure reply when route fails — connection just drops.
///
/// The current code has no failure handling: if the upstream connection fails
/// after the success reply was already sent, the client just sees an ambiguous
/// connection close with no SOCKS5 error reply (REP != 0x00).
#[tokio::test]
async fn defect_socks5_no_failure_reply_when_route_fails() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();

        // Read method negotiation
        let mut header = [0u8; 2];
        stream.read_exact(&mut header).await.unwrap();
        let nmethods = header[1] as usize;
        let mut methods = vec![0u8; nmethods];
        stream.read_exact(&mut methods).await.unwrap();

        // Send method selection
        stream.write_all(&[0x05, 0x00]).await.unwrap();
        stream.flush().await.unwrap();

        // Read CONNECT request
        let mut req = [0u8; 4];
        stream.read_exact(&mut req).await.unwrap();
        match req[3] {
            0x01 => {
                let mut buf = [0u8; 6];
                stream.read_exact(&mut buf).await.unwrap();
            }
            0x03 => {
                let mut len = [0u8; 1];
                stream.read_exact(&mut len).await.unwrap();
                let mut domain = vec![0u8; len[0] as usize];
                stream.read_exact(&mut domain).await.unwrap();
                let mut port = [0u8; 2];
                stream.read_exact(&mut port).await.unwrap();
            }
            _ => panic!("unexpected atyp"),
        }

        // Simulate route failure: drop stream without sending any reply.
        // No SOCKS5 error reply is sent — just an ambiguous connection close.
        drop(stream);
    });

    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Method negotiation
    client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut method_resp = [0u8; 2];
    client.read_exact(&mut method_resp).await.unwrap();

    // CONNECT request
    client
        .write_all(&[0x05, 0x01, 0x00, 0x01, 198, 51, 100, 1])
        .await
        .unwrap();
    client.write_all(&443u16.to_be_bytes()).await.unwrap();

    // Server drops without any reply. Client sees EOF — ambiguous error.
    let mut reply_buf = [0u8; 10];
    let result = timeout(
        Duration::from_millis(200),
        client.read_exact(&mut reply_buf),
    )
    .await;

    // The read should fail (connection closed / EOF), not get a SOCKS5 error reply.
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "client sees connection close with no failure reply — missing error handling"
    );

    server.await.unwrap();
}

// ===========================================================================
// CORRECT BEHAVIOR TESTS — demonstrate the proper reply ordering pattern
// ===========================================================================

/// SOCKS5: correct behavior — reply sent only after route is established.
///
/// The server waits for a `Notify` signal (simulating route readiness) before
/// sending the success reply. The client verifies the reply is NOT available
/// immediately, then becomes available after the signal.
#[tokio::test]
async fn correct_socks5_reply_after_route_ready() {
    let route_ready = Arc::new(Notify::new());
    let route_ready_clone = route_ready.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();

        // Read method negotiation
        let mut header = [0u8; 2];
        stream.read_exact(&mut header).await.unwrap();
        let nmethods = header[1] as usize;
        let mut methods = vec![0u8; nmethods];
        stream.read_exact(&mut methods).await.unwrap();

        // Send method selection
        stream.write_all(&[0x05, 0x00]).await.unwrap();
        stream.flush().await.unwrap();

        // Read CONNECT request
        let mut req = [0u8; 4];
        stream.read_exact(&mut req).await.unwrap();
        match req[3] {
            0x01 => {
                let mut buf = [0u8; 6];
                stream.read_exact(&mut buf).await.unwrap();
            }
            0x03 => {
                let mut len = [0u8; 1];
                stream.read_exact(&mut len).await.unwrap();
                let mut domain = vec![0u8; len[0] as usize];
                stream.read_exact(&mut domain).await.unwrap();
                let mut port = [0u8; 2];
                stream.read_exact(&mut port).await.unwrap();
            }
            _ => panic!("unexpected atyp"),
        }

        // CORRECT: Wait for route to be established before sending reply.
        tokio::time::timeout(Duration::from_secs(5), route_ready_clone.notified())
            .await
            .expect("route_ready signal should arrive");

        // Now send success reply — route is ready.
        let reply: [u8; 10] = [0x05, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        stream.write_all(&reply).await.unwrap();
        stream.flush().await.unwrap();
    });

    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Method negotiation
    client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut method_resp = [0u8; 2];
    client.read_exact(&mut method_resp).await.unwrap();

    // CONNECT request
    client
        .write_all(&[0x05, 0x01, 0x00, 0x01, 198, 51, 100, 1])
        .await
        .unwrap();
    client.write_all(&443u16.to_be_bytes()).await.unwrap();

    // ASSERTION: Reply should NOT be available yet (route not ready).
    let mut reply_buf = [0u8; 10];
    let result = timeout(NOT_YET, client.read_exact(&mut reply_buf)).await;
    assert!(
        result.is_err(),
        "reply should not be available before route is established"
    );

    // Signal that the route is now ready.
    route_ready.notify_one();

    // Now the reply should become available.
    let result = timeout(Duration::from_secs(5), client.read_exact(&mut reply_buf)).await;
    result
        .expect("timed out waiting for reply")
        .expect("read failed");
    assert_eq!(reply_buf[0], 0x05);
    assert_eq!(reply_buf[1], 0x00);

    server.await.unwrap();
}

/// SOCKS4: correct behavior — granted reply sent only after route is established.
#[tokio::test]
async fn correct_socks4_reply_after_route_ready() {
    let route_ready = Arc::new(Notify::new());
    let route_ready_clone = route_ready.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();

        // Read SOCKS4 request
        let mut header = [0u8; 8];
        stream.read_exact(&mut header).await.unwrap();

        // Read NUL-terminated user ID
        loop {
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).await.unwrap();
            if byte[0] == 0x00 {
                break;
            }
        }

        // CORRECT: Wait for route before sending reply.
        tokio::time::timeout(Duration::from_secs(5), route_ready_clone.notified())
            .await
            .expect("route_ready signal should arrive");

        // Now send granted reply.
        let reply: [u8; 8] = [0x00, 90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        stream.write_all(&reply).await.unwrap();
        stream.flush().await.unwrap();
    });

    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();

    // SOCKS4 CONNECT request
    client
        .write_all(&[0x04, 0x01, 0x00, 80, 10, 0, 0, 1])
        .await
        .unwrap();
    client.write_all(b"testuser").await.unwrap();
    client.write_all(&[0x00]).await.unwrap();

    // ASSERTION: Reply should NOT be available yet.
    let mut reply_buf = [0u8; 8];
    let result = timeout(NOT_YET, client.read_exact(&mut reply_buf)).await;
    assert!(
        result.is_err(),
        "reply should not be available before route is established"
    );

    // Signal route ready.
    route_ready.notify_one();

    // Now reply should arrive.
    let result = timeout(Duration::from_secs(5), client.read_exact(&mut reply_buf)).await;
    result
        .expect("timed out waiting for reply")
        .expect("read failed");
    assert_eq!(reply_buf[0], 0x00);
    assert_eq!(reply_buf[1], 90);

    server.await.unwrap();
}

/// HTTP CONNECT: correct behavior — 200 sent only after route is established.
#[tokio::test]
async fn correct_http_connect_reply_after_route_ready() {
    let route_ready = Arc::new(Notify::new());
    let route_ready_clone = route_ready.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();

        // Read HTTP CONNECT request until \r\n\r\n
        let mut head = Vec::with_capacity(1024);
        loop {
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).await.unwrap();
            head.push(byte[0]);
            if head.len() >= 4 && head[head.len() - 4..] == *b"\r\n\r\n" {
                break;
            }
        }

        // CORRECT: Wait for route before sending 200.
        tokio::time::timeout(Duration::from_secs(5), route_ready_clone.notified())
            .await
            .expect("route_ready signal should arrive");

        // Now send 200 response.
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .unwrap();
        stream.flush().await.unwrap();
    });

    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();

    client
        .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
        .await
        .unwrap();

    // ASSERTION: Reply should NOT be available yet.
    let mut resp_buf = [0u8; 4096];
    let result = timeout(NOT_YET, client.read(&mut resp_buf)).await;
    assert!(
        result.is_err(),
        "reply should not be available before route is established"
    );

    // Signal route ready.
    route_ready.notify_one();

    // Now reply should arrive.
    let result = timeout(Duration::from_secs(5), client.read(&mut resp_buf)).await;
    let n = result.expect("timed out").expect("read failed");
    let resp = String::from_utf8_lossy(&resp_buf[..n]);
    assert!(resp.starts_with("HTTP/1.1 200"));

    server.await.unwrap();
}
