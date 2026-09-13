use criterion::{criterion_group, criterion_main, Criterion};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Runtime;

/// Upstream echo server: accepts one connection, echoes until EOF, then
/// half-closes so the relay's downstream direction can complete.
async fn run_echo_server(listener: tokio::net::TcpListener) {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut buf = vec![0u8; 65536];
    loop {
        match stream.read(&mut buf).await {
            Ok(0) => {
                let _ = stream.shutdown().await;
                break;
            }
            Ok(n) => {
                if stream.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

/// Proxy hop using the generic relay engine under test.
async fn run_relay_proxy(
    listener: tokio::net::TcpListener,
    upstream_addr: std::net::SocketAddr,
) -> eggress_relay::RelayReport {
    let (client_stream, _) = listener.accept().await.unwrap();
    let server_stream = tokio::net::TcpStream::connect(upstream_addr).await.unwrap();
    eggress_relay::relay(client_stream, server_stream)
        .await
        .unwrap_or_else(|failure| panic!("relay benchmark failed unexpectedly: {failure}"))
}

/// Proxy-hop baseline using `tokio::io::copy_bidirectional` with the same
/// listener/upstream topology, for diagnostic comparison only.
async fn run_baseline_proxy(
    listener: tokio::net::TcpListener,
    upstream_addr: std::net::SocketAddr,
) -> (u64, u64) {
    let (mut client_stream, _) = listener.accept().await.unwrap();
    let mut server_stream = tokio::net::TcpStream::connect(upstream_addr).await.unwrap();
    tokio::io::copy_bidirectional(&mut client_stream, &mut server_stream)
        .await
        .unwrap()
}

async fn round_trip_via_relay(payload_len: usize) {
    let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = echo.local_addr().unwrap();
    let echo_task = tokio::spawn(run_echo_server(echo));

    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy.local_addr().unwrap();
    let proxy_task = tokio::spawn(run_relay_proxy(proxy, echo_addr));

    let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let payload = vec![0xABu8; payload_len];
    client.write_all(&payload).await.unwrap();
    client.shutdown().await.unwrap();

    let mut received = Vec::new();
    client.read_to_end(&mut received).await.unwrap();
    assert_eq!(received.len(), payload_len);
    assert!(received.iter().all(|&b| b == 0xAB));

    let report = proxy_task.await.unwrap();
    assert_eq!(report.bytes_upstream, payload_len as u64);
    assert_eq!(report.bytes_downstream, payload_len as u64);
    echo_task.await.unwrap();
}

async fn round_trip_via_baseline(payload_len: usize) {
    let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = echo.local_addr().unwrap();
    let echo_task = tokio::spawn(run_echo_server(echo));

    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy.local_addr().unwrap();
    let proxy_task = tokio::spawn(run_baseline_proxy(proxy, echo_addr));

    let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let payload = vec![0xABu8; payload_len];
    client.write_all(&payload).await.unwrap();
    client.shutdown().await.unwrap();

    let mut received = Vec::new();
    client.read_to_end(&mut received).await.unwrap();
    assert_eq!(received.len(), payload_len);

    let (up, down) = proxy_task.await.unwrap();
    assert_eq!(up, payload_len as u64);
    assert_eq!(down, payload_len as u64);
    echo_task.await.unwrap();
}

fn tcp_relay_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("tcp_relay");

    group.bench_function("1KB_relay", |b| {
        b.iter(|| {
            rt.block_on(round_trip_via_relay(1024));
        });
    });

    group.bench_function("64KB_relay", |b| {
        b.iter(|| {
            rt.block_on(round_trip_via_relay(65536));
        });
    });

    group.bench_function("1KB_copy_bidirectional_baseline", |b| {
        b.iter(|| {
            rt.block_on(round_trip_via_baseline(1024));
        });
    });

    group.bench_function("64KB_copy_bidirectional_baseline", |b| {
        b.iter(|| {
            rt.block_on(round_trip_via_baseline(65536));
        });
    });

    group.finish();
}

criterion_group!(benches, tcp_relay_benchmark);
criterion_main!(benches);
