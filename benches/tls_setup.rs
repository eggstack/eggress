use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

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

    group.finish();
}

criterion_group!(benches, tls_setup_benchmark);
criterion_main!(benches);
