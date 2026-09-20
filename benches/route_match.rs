use std::net::SocketAddr;
use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eggress_core::{ClientIdentity, ProtocolId, TargetAddr, TargetHost};
use eggress_routing::upstream::{GroupFallback, UpstreamGroup, UpstreamRuntime};
use eggress_routing::RouteService;
use eggress_routing::{health::HealthState, scheduler::SchedulerKind};
use eggress_routing::{
    CompiledRule, MatchExpr, PortMatcher, RouteActionSpec, RouteRequest, Router, RuleId,
    TransportKind, UpstreamGroupId,
};
use eggress_uri::parse_proxy_chain;
use ipnet::IpNet;

fn make_domain_target(domain: &str, port: u16) -> TargetAddr {
    TargetAddr {
        host: TargetHost::Domain(domain.to_string()),
        port,
    }
}

fn make_ip_target(ip: &str, port: u16) -> TargetAddr {
    TargetAddr {
        host: TargetHost::Ip(ip.parse().unwrap()),
        port,
    }
}

fn route_match_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("route_match");

    // Build a router with diverse rules
    let rules = vec![
        CompiledRule {
            id: RuleId(Arc::from("block-ads")),
            matcher: MatchExpr::HostSuffix(Arc::from("ads.example.com")),
            action: RouteActionSpec::Reject(eggress_core::RejectReason::Blocked),
        },
        CompiledRule {
            id: RuleId(Arc::from("internal")),
            matcher: MatchExpr::DestinationCidr("10.0.0.0/8".parse::<IpNet>().unwrap()),
            action: RouteActionSpec::Direct,
        },
        CompiledRule {
            id: RuleId(Arc::from("dns-port")),
            matcher: MatchExpr::DestinationPort(PortMatcher::Exact(53)),
            action: RouteActionSpec::Direct,
        },
        CompiledRule {
            id: RuleId(Arc::from("high-ports")),
            matcher: MatchExpr::DestinationPort(PortMatcher::Range {
                start: 8000,
                end: 9000,
            }),
            action: RouteActionSpec::UpstreamGroup(UpstreamGroupId(Arc::from("fast"))),
        },
        CompiledRule {
            id: RuleId(Arc::from("ssh-source")),
            matcher: MatchExpr::All(vec![
                MatchExpr::DestinationPort(PortMatcher::Exact(22)),
                MatchExpr::SourceCidr("192.168.0.0/16".parse::<IpNet>().unwrap()),
            ]),
            action: RouteActionSpec::Direct,
        },
        CompiledRule {
            id: RuleId(Arc::from("google-suffix")),
            matcher: MatchExpr::HostSuffix(Arc::from("google.com")),
            action: RouteActionSpec::UpstreamGroup(UpstreamGroupId(Arc::from("us-west"))),
        },
        CompiledRule {
            id: RuleId(Arc::from("blocked-regex")),
            matcher: MatchExpr::HostRegex(regex::Regex::new(r"^ads?\d*\.").unwrap()),
            action: RouteActionSpec::Reject(eggress_core::RejectReason::Blocked),
        },
        CompiledRule {
            id: RuleId(Arc::from("ipv6-ula")),
            matcher: MatchExpr::DestinationCidr("fc00::/7".parse::<IpNet>().unwrap()),
            action: RouteActionSpec::Direct,
        },
        CompiledRule {
            id: RuleId(Arc::from("socks-transport")),
            matcher: MatchExpr::All(vec![
                MatchExpr::Protocol(ProtocolId::Socks5),
                MatchExpr::Transport(TransportKind::Udp),
            ]),
            action: RouteActionSpec::UpstreamGroup(UpstreamGroupId(Arc::from("socks-proxy"))),
        },
    ];

    let router = Router::new(rules, RouteActionSpec::Direct);

    // Request that matches rule 0 (block-ads) - early match
    let req_block_ads = RouteRequest {
        target: &make_domain_target("ads.example.com", 443),
        source: None,
        listener: "socks-in",
        inbound_protocol: ProtocolId::Socks5,
        identity: &ClientIdentity::Anonymous,
        transport: TransportKind::Tcp,
    };

    // Request that matches rule 1 (internal CIDR)
    let req_internal = RouteRequest {
        target: &make_ip_target("10.0.1.5", 80),
        source: None,
        listener: "http-in",
        inbound_protocol: ProtocolId::Http,
        identity: &ClientIdentity::Anonymous,
        transport: TransportKind::Tcp,
    };

    // Request that matches rule 5 (google.com suffix)
    let req_google = RouteRequest {
        target: &make_domain_target("mail.google.com", 443),
        source: None,
        listener: "http-in",
        inbound_protocol: ProtocolId::Http,
        identity: &ClientIdentity::Username("alice".to_string()),
        transport: TransportKind::Tcp,
    };

    // Request that matches default (no rule match)
    let req_default = RouteRequest {
        target: &make_domain_target("unknown.io", 8080),
        source: None,
        listener: "http-in",
        inbound_protocol: ProtocolId::Http,
        identity: &ClientIdentity::Anonymous,
        transport: TransportKind::Tcp,
    };

    // Request that matches rule 7 (ipv6 ULA)
    let req_ipv6 = RouteRequest {
        target: &make_ip_target("fd00::1", 443),
        source: Some("192.168.1.100:54321".parse::<SocketAddr>().unwrap()),
        listener: "socks-in",
        inbound_protocol: ProtocolId::Socks5,
        identity: &ClientIdentity::Anonymous,
        transport: TransportKind::Tcp,
    };

    // Request that matches rule 8 (socks5 + udp)
    let req_socks_udp = RouteRequest {
        target: &make_domain_target("game.example.com", 12345),
        source: None,
        listener: "socks-in",
        inbound_protocol: ProtocolId::Socks5,
        identity: &ClientIdentity::Anonymous,
        transport: TransportKind::Udp,
    };

    // Request that matches rule 3 (high-ports range) - late match
    let req_high_port = RouteRequest {
        target: &make_domain_target("service.example.com", 8443),
        source: None,
        listener: "http-in",
        inbound_protocol: ProtocolId::Http,
        identity: &ClientIdentity::Anonymous,
        transport: TransportKind::Tcp,
    };

    group.bench_function("early_match_host_suffix", |b| {
        b.iter(|| {
            router.decide(&req_block_ads);
        });
    });

    group.bench_function("cidr_match", |b| {
        b.iter(|| {
            router.decide(&req_internal);
        });
    });

    group.bench_function("mid_match_host_suffix", |b| {
        b.iter(|| {
            router.decide(&req_google);
        });
    });

    group.bench_function("no_match_default", |b| {
        b.iter(|| {
            router.decide(&req_default);
        });
    });

    group.bench_function("ipv6_cidr_match", |b| {
        b.iter(|| {
            router.decide(&req_ipv6);
        });
    });

    group.bench_function("compound_match_all", |b| {
        b.iter(|| {
            router.decide(&req_socks_udp);
        });
    });

    group.bench_function("late_match_port_range", |b| {
        b.iter(|| {
            router.decide(&req_high_port);
        });
    });

    group.finish();
}

fn route_selection_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("route_select");
    let target = make_domain_target("service.example.com", 443);
    let identity = ClientIdentity::Anonymous;
    let request = RouteRequest {
        target: &target,
        source: None,
        listener: "bench",
        inbound_protocol: ProtocolId::Socks5,
        identity: &identity,
        transport: TransportKind::Tcp,
    };

    for size in [1usize, 8, 32, 128] {
        for scheduler in [SchedulerKind::RoundRobin, SchedulerKind::LeastConnections] {
            for mixed in [false, true] {
                let group_id = UpstreamGroupId(Arc::from(format!("group-{size}-{mixed}")));
                let chain = parse_proxy_chain("http://127.0.0.1:8080").unwrap();
                let members = (0..size)
                    .map(|index| {
                        let state = if mixed && index % 3 == 0 {
                            HealthState::Unhealthy
                        } else if mixed && index % 3 == 1 {
                            HealthState::Recovering
                        } else {
                            HealthState::Healthy
                        };
                        Arc::new(UpstreamRuntime::new_with_health(
                            eggress_core::UpstreamId::new(format!("up-{size}-{index}")),
                            chain.clone(),
                            state,
                        ))
                    })
                    .collect::<Vec<_>>();
                let upstream_group = UpstreamGroup::new(
                    group_id.clone(),
                    scheduler,
                    members.into(),
                    GroupFallback::Reject,
                );
                let router = Router::with_groups(
                    vec![],
                    RouteActionSpec::UpstreamGroup(group_id.clone()),
                    vec![(group_id, upstream_group)],
                );
                let name = format!(
                    "members_{size}_{}_{}",
                    match scheduler {
                        SchedulerKind::RoundRobin => "round_robin",
                        SchedulerKind::LeastConnections => "least_connections",
                        _ => "other",
                    },
                    if mixed { "mixed_health" } else { "all_healthy" }
                );
                group.bench_function(name, |b| {
                    b.iter(|| black_box(router.route(black_box(&request))))
                });
            }
        }
    }

    group.finish();
}

criterion_group!(benches, route_match_benchmark, route_selection_benchmark);
criterion_main!(benches);
