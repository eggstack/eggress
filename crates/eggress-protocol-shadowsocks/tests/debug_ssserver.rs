// Debug test: capture ssserver stderr to understand what it sees
use std::time::Duration;

use eggress_core::{BoxStream, TargetAddr, TargetHost};
use eggress_protocol_shadowsocks::method::CipherMethod;
use eggress_protocol_shadowsocks::tcp::shadowsocks_connect;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
#[ignore]
async fn debug_ssserver_interaction() {
    // Start TCP echo server
    let echo_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = echo_listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match echo_listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            eprintln!("[echo] received {n} bytes, echoing back");
                            if stream.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            eprintln!("[echo] read error: {e}");
                            break;
                        }
                    }
                }
            });
        }
    });

    let ss_port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };

    // Start ssserver with stderr piped
    let mut child = std::process::Command::new("ssserver")
        .args([
            "-s",
            &format!("127.0.0.1:{ss_port}"),
            "-m",
            "aes-256-gcm",
            "-k",
            "testpass",
            "-v",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start ssserver");

    // Wait for ssserver
    let mut ready = false;
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{ss_port}"))
            .await
            .is_ok()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(ready, "ssserver failed to start on port {ss_port}");
    eprintln!("[test] ssserver ready on port {ss_port}");
    eprintln!("[test] echo server on {echo_addr}");

    // Connect to ssserver
    let stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{ss_port}"))
        .await
        .unwrap();
    let boxed: BoxStream = Box::new(stream);
    let target = TargetAddr {
        host: TargetHost::Ip(echo_addr.ip()),
        port: echo_addr.port(),
    };
    let method = CipherMethod::Aes256Gcm;

    let mut conn = shadowsocks_connect(boxed, &target, method, "testpass", None)
        .await
        .unwrap();
    eprintln!("[test] shadowsocks handshake complete");

    // Write data
    conn.write_all(b"hello").await.unwrap();
    conn.flush().await.unwrap();
    eprintln!("[test] sent 'hello' through AEAD stream");

    // Read with timeout
    let mut buf = vec![0u8; 1024];
    let result = tokio::time::timeout(Duration::from_secs(5), conn.read(&mut buf)).await;
    match result {
        Ok(Ok(n)) => {
            eprintln!("[test] read {n} bytes: {:?}", &buf[..n]);
            assert_eq!(&buf[..n], b"hello");
            eprintln!("[test] SUCCESS!");
        }
        Ok(Err(e)) => {
            eprintln!("[test] read error: {e}");
        }
        Err(_) => {
            eprintln!("[test] READ TIMED OUT after 5 seconds");
        }
    }

    // Capture ssserver output
    child.kill().ok();
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("[test] ssserver stderr:\n{stderr}");
    eprintln!("[test] ssserver stdout:\n{stdout}");
}
