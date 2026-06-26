use criterion::{criterion_group, criterion_main, Criterion};
use eggress_core::relay;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Runtime;

fn tcp_relay_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("tcp_relay");

    group.bench_function("1KB_relay", |b| {
        b.iter(|| {
            rt.block_on(async {
                let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let echo_addr = echo.local_addr().unwrap();

                let jh = tokio::spawn(async move {
                    let (stream, _) = echo.accept().await.unwrap();
                    let (mut reader, mut writer) = stream.into_split();
                    tokio::spawn(async move {
                        let mut buf = [0u8; 4096];
                        loop {
                            match reader.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    if writer.write_all(&buf[..n]).await.is_err() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    });
                });

                let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let proxy_addr = proxy_listener.local_addr().unwrap();

                let proxy_jh = tokio::spawn(async move {
                    let (client_stream, _) = proxy_listener.accept().await.unwrap();
                    let server_stream = tokio::net::TcpStream::connect(echo_addr).await.unwrap();
                    relay::relay(Box::new(client_stream), Box::new(server_stream)).await
                });

                let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
                let payload = vec![0xABu8; 1024];
                client.write_all(&payload).await.unwrap();
                client.shutdown().await.unwrap();

                let mut buf = Vec::new();
                client.read_to_end(&mut buf).await.unwrap();

                let _ = proxy_jh.await;
                jh.await.unwrap();
            });
        });
    });

    group.bench_function("64KB_relay", |b| {
        b.iter(|| {
            rt.block_on(async {
                let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let echo_addr = echo.local_addr().unwrap();

                let jh = tokio::spawn(async move {
                    let (stream, _) = echo.accept().await.unwrap();
                    let (mut reader, mut writer) = stream.into_split();
                    tokio::spawn(async move {
                        let mut buf = [0u8; 65536];
                        loop {
                            match reader.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    if writer.write_all(&buf[..n]).await.is_err() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    });
                });

                let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let proxy_addr = proxy_listener.local_addr().unwrap();

                let proxy_jh = tokio::spawn(async move {
                    let (client_stream, _) = proxy_listener.accept().await.unwrap();
                    let server_stream = tokio::net::TcpStream::connect(echo_addr).await.unwrap();
                    relay::relay(Box::new(client_stream), Box::new(server_stream)).await
                });

                let mut client = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
                let payload = vec![0xABu8; 65536];
                client.write_all(&payload).await.unwrap();
                client.shutdown().await.unwrap();

                let mut buf = Vec::new();
                client.read_to_end(&mut buf).await.unwrap();

                let _ = proxy_jh.await;
                jh.await.unwrap();
            });
        });
    });

    group.finish();
}

criterion_group!(benches, tcp_relay_benchmark);
criterion_main!(benches);
