#![no_main]
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use libfuzzer_sys::fuzz_target;

use eggress_core::{ClientIdentity, ProtocolId, RejectReason, TargetAddr, TargetHost};
use eggress_routing::{
    CompiledRule, MatchExpr, PortMatcher, RouteActionSpec, RouteRequest, RouteService, Router,
    TransportKind,
};

const ANON: ClientIdentity = ClientIdentity::Anonymous;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // Pick a router "shape" from the first byte. Each shape exercises a
    // different combination of matchers so the fuzzer can reach different
    // branches of `MatchExpr::matches`.
    let router = match data[0] % 4 {
        0 => build_router_with_listeners(),
        1 => build_router_with_ports_and_cidr(),
        2 => build_router_with_identity_and_protocol(),
        _ => build_router_with_composites(),
    };

    // Build a representative request from the remaining bytes.
    let req_bytes = &data[1..];
    let target = pick_target(req_bytes);
    let source = pick_source(req_bytes);
    let listener = pick_listener(req_bytes);
    let protocol = pick_protocol(req_bytes);
    let identity = pick_identity(req_bytes);
    let transport = pick_transport(req_bytes);

    let request = RouteRequest {
        target: &target,
        source,
        listener: &listener,
        inbound_protocol: protocol,
        identity: &identity,
        transport,
    };

    // Exercise both decision and explanation paths.
    let _decision = router.decide(&request);
    let _ = router.explain(&request, 0);
    let _ = router.route(&request);

    // Also exercise a few selected matcher variants directly.
    let _ = MatchExpr::Any.matches(&request);
    let _ = MatchExpr::Not(Box::new(MatchExpr::Any)).matches(&request);
});

fn build_router_with_listeners() -> Router {
    Router::new(
        vec![
            CompiledRule {
                id: eggress_routing::RuleId(Arc::from("listener-allow")),
                matcher: MatchExpr::Listener(Arc::from("lan")),
                action: RouteActionSpec::Direct,
            },
            CompiledRule {
                id: eggress_routing::RuleId(Arc::from("default-reject")),
                matcher: MatchExpr::Any,
                action: RouteActionSpec::Reject(RejectReason::Blocked),
            },
        ],
        RouteActionSpec::Direct,
    )
}

fn build_router_with_ports_and_cidr() -> Router {
    Router::new(
        vec![
            CompiledRule {
                id: eggress_routing::RuleId(Arc::from("https-block")),
                matcher: MatchExpr::DestinationPort(PortMatcher::Exact(443)),
                action: RouteActionSpec::Reject(RejectReason::AccessDenied),
            },
            CompiledRule {
                id: eggress_routing::RuleId(Arc::from("private-cidr")),
                matcher: MatchExpr::DestinationCidr(
                    "10.0.0.0/8".parse().expect("valid cidr"),
                ),
                action: RouteActionSpec::Direct,
            },
        ],
        RouteActionSpec::Direct,
    )
}

fn build_router_with_identity_and_protocol() -> Router {
    Router::new(
        vec![
            CompiledRule {
                id: eggress_routing::RuleId(Arc::from("admin-bypass")),
                matcher: MatchExpr::Identity(Arc::from("admin")),
                action: RouteActionSpec::Direct,
            },
            CompiledRule {
                id: eggress_routing::RuleId(Arc::from("socks-only")),
                matcher: MatchExpr::Protocol(ProtocolId::Socks5),
                action: RouteActionSpec::Direct,
            },
        ],
        RouteActionSpec::Reject(RejectReason::UnsupportedProtocol),
    )
}

fn build_router_with_composites() -> Router {
    Router::new(
        vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("host-and-port")),
            matcher: MatchExpr::All(vec![
                MatchExpr::HostExact(Arc::from("example.com")),
                MatchExpr::DestinationPort(PortMatcher::Range { start: 80, end: 443 }),
            ]),
            action: RouteActionSpec::Direct,
        }],
        RouteActionSpec::Direct,
    )
}

fn pick_target(data: &[u8]) -> TargetAddr {
    if data.is_empty() {
        return TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 80,
        };
    }
    match data[0] % 5 {
        0 => TargetAddr {
            host: TargetHost::Ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
            port: 80,
        },
        1 => TargetAddr {
            host: TargetHost::Ip(IpAddr::V6(Ipv6Addr::LOCALHOST)),
            port: 443,
        },
        2 => TargetAddr {
            host: TargetHost::Ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
            port: 8080,
        },
        3 => TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 443,
        },
        _ => {
            if let Ok(s) = std::str::from_utf8(data) {
                TargetAddr {
                    host: TargetHost::Domain(s.to_string()),
                    port: 80,
                }
            } else {
                TargetAddr {
                    host: TargetHost::Domain("x.test".into()),
                    port: 80,
                }
            }
        }
    }
}

fn pick_source(data: &[u8]) -> Option<SocketAddr> {
    if data.len() < 6 {
        return None;
    }
    match data[0] % 3 {
        0 => None,
        1 => {
            let ip = IpAddr::V4(Ipv4Addr::new(data[1], data[2], data[3], data[4]));
            let port = u16::from_be_bytes([data[5], data[5]]);
            Some(SocketAddr::new(ip, port))
        }
        _ => {
            let mut octets = [0u8; 16];
            for (i, slot) in octets.iter_mut().enumerate() {
                *slot = data.get(i + 1).copied().unwrap_or(0);
            }
            let ip = IpAddr::V6(Ipv6Addr::from(octets));
            let port = u16::from_be_bytes([
                data.get(17).copied().unwrap_or(0),
                data.get(18).copied().unwrap_or(0),
            ]);
            Some(SocketAddr::new(ip, port))
        }
    }
}

fn pick_listener(data: &[u8]) -> String {
    if data.is_empty() {
        return "lan".to_string();
    }
    match data[0] % 3 {
        0 => "lan".to_string(),
        1 => "wan".to_string(),
        _ => "default".to_string(),
    }
}

fn pick_protocol(data: &[u8]) -> ProtocolId {
    if data.is_empty() {
        return ProtocolId::Http;
    }
    match data[0] % 5 {
        0 => ProtocolId::Http,
        1 => ProtocolId::Socks4,
        2 => ProtocolId::Socks5,
        3 => ProtocolId::Shadowsocks,
        _ => ProtocolId::Trojan,
    }
}

fn pick_identity(data: &[u8]) -> ClientIdentity {
    if data.is_empty() {
        return ANON.clone();
    }
    match data[0] % 3 {
        0 => ANON.clone(),
        1 => ClientIdentity::Username("alice".into()),
        _ => ClientIdentity::Opaque("opaque-token-xyz".into()),
    }
}

fn pick_transport(data: &[u8]) -> TransportKind {
    if data.is_empty() {
        return TransportKind::Tcp;
    }
    if data[0].is_multiple_of(2) {
        TransportKind::Tcp
    } else {
        TransportKind::Udp
    }
}
