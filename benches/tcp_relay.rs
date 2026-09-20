use std::num::NonZeroUsize;

use criterion::{criterion_group, criterion_main, Criterion};
use eggress_relay::{HalfClosePolicy, RelayOptions};
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

async fn run_echo_connection(mut stream: tokio::net::TcpStream) {
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

struct RelayFixture {
    proxy_addr: std::net::SocketAddr,
    stop: tokio::sync::broadcast::Sender<()>,
    accept_tasks: Vec<tokio::task::JoinHandle<()>>,
}

#[derive(Clone, Copy)]
enum RelayMode {
    CopyBidirectional,
    Relay(RelayOptions),
}

impl RelayFixture {
    async fn start(mode: RelayMode) -> Self {
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo.local_addr().unwrap();
        let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy.local_addr().unwrap();
        let (stop, _) = tokio::sync::broadcast::channel(1);

        let mut echo_stop = stop.subscribe();
        let echo_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = echo_stop.recv() => break,
                    result = echo.accept() => match result {
                        Ok((stream, _)) => { tokio::spawn(run_echo_connection(stream)); }
                        Err(_) => break,
                    },
                }
            }
        });

        let mut proxy_stop = stop.subscribe();
        let proxy_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = proxy_stop.recv() => break,
                    result = proxy.accept() => match result {
                        Ok((mut client, _)) => {
                            tokio::spawn(async move {
                                let mut server = tokio::net::TcpStream::connect(echo_addr).await.unwrap();
                                match mode {
                                    RelayMode::CopyBidirectional => {
                                        let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
                                    }
                                    RelayMode::Relay(options) => {
                                        let _ = eggress_relay::relay_with_options(client, server, options).await;
                                    }
                                }
                            });
                        }
                        Err(_) => break,
                    },
                }
            }
        });

        Self {
            proxy_addr,
            stop,
            accept_tasks: vec![echo_task, proxy_task],
        }
    }

    async fn transfer(&self, payload_len: usize) {
        let mut client = tokio::net::TcpStream::connect(self.proxy_addr)
            .await
            .unwrap();
        let payload = vec![0xABu8; payload_len];
        client.write_all(&payload).await.unwrap();
        client.shutdown().await.unwrap();
        let mut received = Vec::with_capacity(payload_len);
        client.read_to_end(&mut received).await.unwrap();
        assert_eq!(received.len(), payload_len);
    }

    async fn transfer_concurrent(&self, count: usize, payload_len: usize) {
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..count {
            let addr = self.proxy_addr;
            tasks.spawn(async move {
                let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
                let payload = vec![0xCDu8; payload_len];
                client.write_all(&payload).await.unwrap();
                client.shutdown().await.unwrap();
                let mut received = Vec::with_capacity(payload_len);
                client.read_to_end(&mut received).await.unwrap();
                assert_eq!(received.len(), payload_len);
            });
        }
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
    }

    async fn stop(self) {
        let _ = self.stop.send(());
        for task in self.accept_tasks {
            let _ = task.await;
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

    group.bench_function("1MB_relay", |b| {
        b.iter(|| {
            rt.block_on(round_trip_via_relay(1024 * 1024));
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

    let relay_fixture = rt.block_on(RelayFixture::start(RelayMode::Relay(
        RelayOptions::default(),
    )));
    for (name, size) in [
        ("steady_1KB_relay", 1024),
        ("steady_64KB_relay", 65536),
        ("steady_1MB_relay", 1024 * 1024),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| rt.block_on(relay_fixture.transfer(size)));
        });
    }
    group.bench_function("steady_16x_64KB_relay", |b| {
        b.iter(|| rt.block_on(relay_fixture.transfer_concurrent(16, 65536)));
    });

    let baseline_fixture = rt.block_on(RelayFixture::start(RelayMode::CopyBidirectional));
    group.bench_function("steady_64KB_copy_bidirectional_baseline", |b| {
        b.iter(|| rt.block_on(baseline_fixture.transfer(65536)));
    });

    rt.block_on(relay_fixture.stop());
    rt.block_on(baseline_fixture.stop());
    group.finish();

    let mut buffer_group = c.benchmark_group("tcp_relay_buffer_matrix");
    for buffer_size in [16 * 1024, 32 * 1024, 64 * 1024] {
        let fixture = rt.block_on(RelayFixture::start(RelayMode::Relay(RelayOptions::new(
            NonZeroUsize::new(buffer_size).unwrap(),
            HalfClosePolicy::Drain,
        ))));
        let label = format!("{buffer_size}_bytes");
        buffer_group.bench_function(format!("{label}_1KB"), |b| {
            b.iter(|| rt.block_on(fixture.transfer(1024)));
        });
        buffer_group.bench_function(format!("{label}_64KB"), |b| {
            b.iter(|| rt.block_on(fixture.transfer(65536)));
        });
        buffer_group.bench_function(format!("{label}_1MB"), |b| {
            b.iter(|| rt.block_on(fixture.transfer(1024 * 1024)));
        });
        buffer_group.bench_function(format!("{label}_16x_64KB"), |b| {
            b.iter(|| rt.block_on(fixture.transfer_concurrent(16, 65536)));
        });
        rt.block_on(fixture.stop());
    }
    buffer_group.finish();
}

criterion_group!(benches, tcp_relay_benchmark);
criterion_main!(benches);
