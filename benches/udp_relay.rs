use criterion::{criterion_group, criterion_main, Criterion};
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_protocol_socks::socks5::udp_codec::{
    decode_socks5_udp_datagram, encode_socks5_udp_datagram,
};

fn udp_relay_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("udp_codec");

    let ipv4_target = SocksAddr::IPv4([192, 168, 1, 1], 8080);
    let ipv6_target = SocksAddr::IPv6(
        [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
        443,
    );
    let domain_target = SocksAddr::Domain("example.com".to_string(), 443);

    let small_payload = vec![0xABu8; 64];
    let large_payload = vec![0xCDu8; 1400];

    // IPv4 small
    {
        let target = ipv4_target.clone();
        let payload = small_payload.clone();
        group.bench_function("encode_ipv4_small", |b| {
            let mut buf = Vec::with_capacity(256);
            b.iter(|| {
                encode_socks5_udp_datagram(&target, &payload, &mut buf);
            });
        });
    }

    // IPv4 large
    {
        let target = ipv4_target.clone();
        let payload = large_payload.clone();
        group.bench_function("encode_ipv4_large", |b| {
            let mut buf = Vec::with_capacity(2048);
            b.iter(|| {
                encode_socks5_udp_datagram(&target, &payload, &mut buf);
            });
        });
    }

    // Domain small
    {
        let target = domain_target.clone();
        let payload = small_payload.clone();
        group.bench_function("encode_domain_small", |b| {
            let mut buf = Vec::with_capacity(256);
            b.iter(|| {
                encode_socks5_udp_datagram(&target, &payload, &mut buf);
            });
        });
    }

    // IPv6 large
    {
        let target = ipv6_target.clone();
        let payload = large_payload.clone();
        group.bench_function("encode_ipv6_large", |b| {
            let mut buf = Vec::with_capacity(2048);
            b.iter(|| {
                encode_socks5_udp_datagram(&target, &payload, &mut buf);
            });
        });
    }

    // Decode IPv4
    {
        let target = ipv4_target.clone();
        let payload = small_payload.clone();
        let mut encoded = Vec::new();
        encode_socks5_udp_datagram(&target, &payload, &mut encoded);
        group.bench_function("decode_ipv4", |b| {
            b.iter(|| {
                decode_socks5_udp_datagram(&encoded).unwrap();
            });
        });
    }

    // Decode domain
    {
        let target = domain_target.clone();
        let payload = small_payload.clone();
        let mut encoded = Vec::new();
        encode_socks5_udp_datagram(&target, &payload, &mut encoded);
        group.bench_function("decode_domain", |b| {
            b.iter(|| {
                decode_socks5_udp_datagram(&encoded).unwrap();
            });
        });
    }

    // Roundtrip IPv4
    {
        let target = ipv4_target.clone();
        let payload = small_payload.clone();
        group.bench_function("roundtrip_ipv4_small", |b| {
            let mut buf = Vec::with_capacity(256);
            b.iter(|| {
                encode_socks5_udp_datagram(&target, &payload, &mut buf);
                decode_socks5_udp_datagram(&buf).unwrap();
            });
        });
    }

    // Roundtrip domain
    {
        let target = domain_target.clone();
        let payload = small_payload.clone();
        group.bench_function("roundtrip_domain_small", |b| {
            let mut buf = Vec::with_capacity(256);
            b.iter(|| {
                encode_socks5_udp_datagram(&target, &payload, &mut buf);
                decode_socks5_udp_datagram(&buf).unwrap();
            });
        });
    }

    group.finish();
}

criterion_group!(benches, udp_relay_benchmark);
criterion_main!(benches);
