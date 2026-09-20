use std::net::SocketAddr;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use eggress_protocol_socks::socks5::server::SocksAddr;
use eggress_protocol_socks::socks5::udp_codec::{
    decode_socks5_udp_datagram, encode_socks5_udp_datagram,
};
struct UdpFixture {
    client: tokio::net::UdpSocket,
    relay_addr: SocketAddr,
    targets: Vec<SocksAddr>,
    packets: Vec<Vec<u8>>,
    cancel: tokio_util::sync::CancellationToken,
    relay_task: tokio::task::JoinHandle<()>,
    echo_tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl UdpFixture {
    async fn start(target_count: usize, payload_len: usize) -> Self {
        let mut targets = Vec::with_capacity(target_count);
        let mut echo_tasks = Vec::with_capacity(target_count);
        for _ in 0..target_count {
            let target = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let target_addr = target.local_addr().unwrap();
            targets.push(SocksAddr::IPv4(
                match target_addr.ip() {
                    std::net::IpAddr::V4(ip) => ip.octets(),
                    std::net::IpAddr::V6(_) => unreachable!(),
                },
                target_addr.port(),
            ));
            echo_tasks.push(tokio::spawn(async move {
                let mut buf = vec![0u8; 65535];
                while let Ok((n, peer)) = target.recv_from(&mut buf).await {
                    if target.send_to(&buf[..n], peer).await.is_err() {
                        break;
                    }
                }
            }));
        }

        let relay_socket = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let relay_addr = relay_socket.local_addr().unwrap();
        let cancel = tokio_util::sync::CancellationToken::new();
        let metrics = Arc::new(eggress_udp::metrics::UdpMetrics::new());
        let config = eggress_udp::standalone::StandaloneUdpConfig {
            routing: Arc::new(eggress_routing::Router::new(
                vec![],
                eggress_routing::RouteActionSpec::Direct,
            )),
            udp_metrics: metrics,
            limits: eggress_udp::limits::UdpLimits {
                max_datagram_size: 65535,
                max_standalone_flows: target_count.max(8),
                ..Default::default()
            },
            listener: "bench-standalone-udp".to_string(),
            generation: 1,
            allow_private_egress: true,
        };
        let relay_cancel = cancel.clone();
        let relay_task = tokio::spawn(async move {
            let _ =
                eggress_udp::standalone::standalone_udp_relay(relay_socket, config, relay_cancel)
                    .await;
        });
        let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let payload = vec![0xA5; payload_len];
        let packets = targets
            .iter()
            .map(|target| {
                let mut packet = Vec::with_capacity(payload_len + 32);
                encode_socks5_udp_datagram(target, &payload, &mut packet).unwrap();
                packet
            })
            .collect();

        Self {
            client,
            relay_addr,
            targets,
            packets,
            cancel,
            relay_task,
            echo_tasks,
        }
    }

    async fn round_trip(&self, target_index: usize) {
        self.client
            .send_to(&self.packets[target_index], self.relay_addr)
            .await
            .unwrap();
        let mut response = vec![0u8; 65535];
        let (n, _) = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.client.recv_from(&mut response),
        )
        .await
        .unwrap()
        .unwrap();
        let decoded = decode_socks5_udp_datagram(&response[..n]).unwrap();
        assert_eq!(decoded.target, self.targets[target_index]);
    }

    async fn stop(self) {
        self.cancel.cancel();
        let _ = self.relay_task.await;
        for task in self.echo_tasks {
            task.abort();
            let _ = task.await;
        }
    }
}

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
                encode_socks5_udp_datagram(&target, &payload, &mut buf).unwrap();
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
                encode_socks5_udp_datagram(&target, &payload, &mut buf).unwrap();
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
                encode_socks5_udp_datagram(&target, &payload, &mut buf).unwrap();
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
                encode_socks5_udp_datagram(&target, &payload, &mut buf).unwrap();
            });
        });
    }

    // Decode IPv4
    {
        let target = ipv4_target.clone();
        let payload = small_payload.clone();
        let mut encoded = Vec::new();
        encode_socks5_udp_datagram(&target, &payload, &mut encoded).unwrap();
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
        encode_socks5_udp_datagram(&target, &payload, &mut encoded).unwrap();
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
                encode_socks5_udp_datagram(&target, &payload, &mut buf).unwrap();
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
                encode_socks5_udp_datagram(&target, &payload, &mut buf).unwrap();
                decode_socks5_udp_datagram(&buf).unwrap();
            });
        });
    }

    group.finish();
}

fn udp_runtime_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("udp_runtime");
    let small = rt.block_on(UdpFixture::start(1, 64));
    group.bench_function("one_client_one_flow_64B_hot", |b| {
        b.iter(|| rt.block_on(small.round_trip(0)));
    });
    let moderate = rt.block_on(UdpFixture::start(1, 1400));
    group.bench_function("one_client_one_flow_1400B_hot", |b| {
        b.iter(|| rt.block_on(moderate.round_trip(0)));
    });
    let many = rt.block_on(UdpFixture::start(8, 64));
    group.bench_function("eight_target_flow_admission_64B", |b| {
        b.iter(|| {
            rt.block_on(async {
                for index in 0..many.targets.len() {
                    many.round_trip(index).await;
                }
            })
        });
    });
    rt.block_on(small.stop());
    rt.block_on(moderate.stop());
    rt.block_on(many.stop());
    group.finish();
}

criterion_group!(benches, udp_relay_benchmark, udp_runtime_benchmark);
criterion_main!(benches);
