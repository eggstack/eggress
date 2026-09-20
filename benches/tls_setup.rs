use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use eggress_core::BoxStream;
use tokio::io::AsyncWriteExt;

struct TlsFixture {
    addr: std::net::SocketAddr,
    client_config: Arc<rustls::ClientConfig>,
    stop: tokio::sync::broadcast::Sender<()>,
    server_task: tokio::task::JoinHandle<()>,
}

enum TlsServerMode {
    PerConnection {
        certificate_pem: Arc<[u8]>,
        private_key_pem: Arc<[u8]>,
    },
    Prepared(Arc<rustls::ServerConfig>),
}

impl TlsFixture {
    async fn start(mode: TlsServerMode, client_config: Arc<rustls::ClientConfig>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (stop, _) = tokio::sync::broadcast::channel(1);
        let mut stop_rx = stop.subscribe();
        let server_task = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    _ = stop_rx.recv() => break,
                    result = listener.accept() => result,
                };
                let (stream, _) = match accepted {
                    Ok(connection) => connection,
                    Err(_) => break,
                };
                let server_config = match &mode {
                    TlsServerMode::PerConnection {
                        certificate_pem,
                        private_key_pem,
                    } => eggress_transport_tls::TlsServerConfigBuilder::new()
                        .with_certificate_pem(certificate_pem)
                        .unwrap()
                        .with_key_pem(private_key_pem)
                        .unwrap()
                        .build()
                        .unwrap(),
                    TlsServerMode::Prepared(config) => Arc::clone(config),
                };
                let boxed: BoxStream = Box::new(stream);
                let _ = eggress_transport_tls::tls_accept(boxed, server_config).await;
            }
        });
        Self {
            addr,
            client_config,
            stop,
            server_task,
        }
    }

    async fn handshake(&self) {
        let stream = tokio::net::TcpStream::connect(self.addr).await.unwrap();
        let boxed: BoxStream = Box::new(stream);
        let mut tls_stream =
            eggress_transport_tls::tls_connect(boxed, Arc::clone(&self.client_config), "localhost")
                .await
                .unwrap();
        let _ = tls_stream.shutdown().await;
    }

    async fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.server_task.await;
    }
}

fn tls_setup_benchmark(c: &mut Criterion) {
    eggress_transport_tls::install_default_crypto_provider();
    let mut group = c.benchmark_group("tls_setup");

    group.bench_function("default_verified_client_config_access", |b| {
        b.iter(|| black_box(eggress_transport_tls::default_client_config().unwrap()))
    });
    group.bench_function("default_h2_client_config_access", |b| {
        b.iter(|| black_box(eggress_transport_tls::default_h2_client_config().unwrap()))
    });
    group.bench_function("chain_executor_construction", |b| {
        b.iter(|| black_box(eggress_outbound::build_chain_executor(None, None)));
    });

    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let certificate_pem = certificate.cert.pem();
    let private_key_pem = certificate.key_pair.serialize_pem();
    group.bench_function("listener_server_config_construction", |b| {
        b.iter(|| {
            let config = eggress_transport_tls::TlsServerConfigBuilder::new()
                .with_certificate_pem(certificate_pem.as_bytes())
                .unwrap()
                .with_key_pem(private_key_pem.as_bytes())
                .unwrap()
                .build()
                .unwrap();
            black_box(config)
        });
    });

    let client_config = eggress_transport_tls::TlsClientConfigBuilder::new()
        .with_custom_ca_pem(certificate_pem.as_bytes())
        .unwrap()
        .build()
        .unwrap();
    let prepared_server_config = eggress_transport_tls::TlsServerConfigBuilder::new()
        .with_certificate_pem(certificate_pem.as_bytes())
        .unwrap()
        .with_key_pem(private_key_pem.as_bytes())
        .unwrap()
        .build()
        .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let prepared_fixture = rt.block_on(TlsFixture::start(
        TlsServerMode::Prepared(prepared_server_config),
        Arc::clone(&client_config),
    ));
    group.bench_function("prepared_listener_tls_connection_handshake", |b| {
        b.iter(|| rt.block_on(prepared_fixture.handshake()));
    });
    let per_connection_fixture = rt.block_on(TlsFixture::start(
        TlsServerMode::PerConnection {
            certificate_pem: Arc::from(certificate_pem.as_bytes()),
            private_key_pem: Arc::from(private_key_pem.as_bytes()),
        },
        client_config,
    ));
    group.bench_function(
        "component_per_connection_server_config_tls_handshake",
        |b| {
            b.iter(|| rt.block_on(per_connection_fixture.handshake()));
        },
    );
    rt.block_on(prepared_fixture.stop());
    rt.block_on(per_connection_fixture.stop());

    group.finish();
}

criterion_group!(benches, tls_setup_benchmark);
criterion_main!(benches);
