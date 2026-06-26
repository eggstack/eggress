use criterion::{criterion_group, criterion_main, Criterion};
use eggress_core::{TargetAddr, TargetHost};
use eggress_protocol_http::connect::client::{http_connect, HttpConnectLimits};
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Runtime;
use tokio::sync::Notify;

/// Helper that owns a single persistent upstream HTTP CONNECT server.
/// Each connection reads one request and replies with a fixed response.
struct UpstreamFixture {
    addr: std::net::SocketAddr,
    stop: Arc<Notify>,
}

impl UpstreamFixture {
    async fn start(rt: &Runtime, response: &'static [u8]) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let stop = Arc::new(Notify::new());

        let stop_clone = stop.clone();
        rt.spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_clone.notified() => break,
                    accept = listener.accept() => {
                        let (mut stream, _) = match accept {
                            Ok(pair) => pair,
                            Err(_) => continue,
                        };
                        let mut buf = [0u8; 1024];
                        let _ = stream.read(&mut buf).await;
                        let _ = stream.write_all(response).await;
                        let _ = stream.flush().await;
                    }
                }
            }
        });

        Self { addr, stop }
    }

    #[allow(dead_code)]
    fn stop(&self) {
        self.stop.notify_one();
    }
}

fn http_connect_upstream_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("http_connect_upstream");

    // Success upstream: 200 Connection Established.
    let success = rt
        .block_on(UpstreamFixture::start(
            &rt,
            b"HTTP/1.1 200 Connection Established\r\n\r\n",
        ))
        .addr;
    // Auth-required upstream: 407 Proxy Authentication Required.
    let auth_required = rt
        .block_on(UpstreamFixture::start(
            &rt,
            b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n",
        ))
        .addr;

    group.bench_function("open_no_auth", |b| {
        let target = TargetAddr {
            host: TargetHost::Ip(Ipv4Addr::new(127, 0, 0, 1).into()),
            port: 8080,
        };
        let limits = HttpConnectLimits::default();
        b.iter(|| {
            rt.block_on(async {
                let stream = tokio::net::TcpStream::connect(success).await.unwrap();
                let _ = http_connect(Box::new(stream), &target, None, &limits).await;
            });
        });
    });

    group.bench_function("open_with_basic_auth", |b| {
        let target = TargetAddr {
            host: TargetHost::Ip(Ipv4Addr::new(127, 0, 0, 1).into()),
            port: 8080,
        };
        let limits = HttpConnectLimits::default();
        b.iter(|| {
            rt.block_on(async {
                let stream = tokio::net::TcpStream::connect(success).await.unwrap();
                let _ = http_connect(
                    Box::new(stream),
                    &target,
                    Some(("alice", "secret")),
                    &limits,
                )
                .await;
            });
        });
    });

    group.bench_function("rejected_407", |b| {
        let target = TargetAddr {
            host: TargetHost::Ip(Ipv4Addr::new(127, 0, 0, 1).into()),
            port: 8080,
        };
        let limits = HttpConnectLimits::default();
        b.iter(|| {
            rt.block_on(async {
                let stream = tokio::net::TcpStream::connect(auth_required).await.unwrap();
                let _ = http_connect(Box::new(stream), &target, None, &limits).await;
            });
        });
    });

    group.finish();

    // Note: the upstream fixtures stay alive until the runtime drops at
    // process exit. They are intentionally not stopped mid-benchmark to
    // avoid extra shutdown work in the hot path.
}

criterion_group!(benches, http_connect_upstream_benchmark);
criterion_main!(benches);
