use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use eggress_core::{ClientIdentity, ProtocolId, TargetAddr, TargetHost};
use eggress_routing::{MatchExpr, RouteActionSpec, RouteRequest, RouteService, Router};

fn target_domain(host: &str, port: u16) -> TargetAddr {
    TargetAddr {
        host: TargetHost::Domain(host.to_string()),
        port,
    }
}

const ANON: ClientIdentity = ClientIdentity::Anonymous;

fn make_request<'a>(
    target: &'a TargetAddr,
    source: Option<SocketAddr>,
    listener: &'a str,
    protocol: ProtocolId,
) -> RouteRequest<'a> {
    RouteRequest {
        target,
        source,
        listener,
        inbound_protocol: protocol,
        identity: &ANON,
    }
}

#[test]
fn listener_name_rule_matches_configured_listener() {
    let router = Router::new(
        vec![eggress_routing::CompiledRule {
            id: eggress_routing::RuleId(Arc::from("listen-http")),
            matcher: MatchExpr::Listener(Arc::from("http-in")),
            action: RouteActionSpec::Direct,
        }],
        RouteActionSpec::Reject(eggress_core::RejectReason::Blocked),
    );

    let target = target_domain("example.com", 80);
    let req = make_request(&target, None, "http-in", ProtocolId::Http);
    let decision = router.decide(&req);
    match decision {
        eggress_routing::RouteDecision::Direct { rule } => {
            assert_eq!(rule.0.as_ref(), "listen-http");
        }
        other => panic!("expected Direct, got {:?}", other),
    }
}

#[test]
fn listener_name_rule_no_match_different_listener() {
    let router = Router::new(
        vec![eggress_routing::CompiledRule {
            id: eggress_routing::RuleId(Arc::from("listen-http")),
            matcher: MatchExpr::Listener(Arc::from("http-in")),
            action: RouteActionSpec::Direct,
        }],
        RouteActionSpec::Reject(eggress_core::RejectReason::Blocked),
    );

    let target = target_domain("example.com", 80);
    let req = make_request(&target, None, "socks-in", ProtocolId::Http);
    let decision = router.decide(&req);
    match decision {
        eggress_routing::RouteDecision::Reject { rule, .. } => {
            assert_eq!(rule.0.as_ref(), "default");
        }
        other => panic!("expected Reject, got {:?}", other),
    }
}

#[test]
fn source_cidr_rule_matches_real_peer_address() {
    let router = Router::new(
        vec![eggress_routing::CompiledRule {
            id: eggress_routing::RuleId(Arc::from("internal-net")),
            matcher: MatchExpr::SourceCidr("192.168.0.0/16".parse().unwrap()),
            action: RouteActionSpec::Direct,
        }],
        RouteActionSpec::Reject(eggress_core::RejectReason::Blocked),
    );

    let target = target_domain("example.com", 80);
    let source = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 12345);
    let req = make_request(&target, Some(source), "http-in", ProtocolId::Http);
    let decision = router.decide(&req);
    match decision {
        eggress_routing::RouteDecision::Direct { rule } => {
            assert_eq!(rule.0.as_ref(), "internal-net");
        }
        other => panic!("expected Direct, got {:?}", other),
    }
}

#[test]
fn source_cidr_rule_no_match_external_address() {
    let router = Router::new(
        vec![eggress_routing::CompiledRule {
            id: eggress_routing::RuleId(Arc::from("internal-net")),
            matcher: MatchExpr::SourceCidr("192.168.0.0/16".parse().unwrap()),
            action: RouteActionSpec::Direct,
        }],
        RouteActionSpec::Reject(eggress_core::RejectReason::Blocked),
    );

    let target = target_domain("example.com", 80);
    let source = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 12345);
    let req = make_request(&target, Some(source), "http-in", ProtocolId::Http);
    let decision = router.decide(&req);
    match decision {
        eggress_routing::RouteDecision::Reject { rule, .. } => {
            assert_eq!(rule.0.as_ref(), "default");
        }
        other => panic!("expected Reject, got {:?}", other),
    }
}

#[test]
fn round_robin_distribution_across_upstreams() {
    let u1 = Arc::new(eggress_routing::upstream::UpstreamRuntime::new(
        eggress_core::UpstreamId::new("p1"),
        eggress_uri::ProxyChainSpec { hops: vec![] },
    ));
    let u2 = Arc::new(eggress_routing::upstream::UpstreamRuntime::new(
        eggress_core::UpstreamId::new("p2"),
        eggress_uri::ProxyChainSpec { hops: vec![] },
    ));

    let group = eggress_routing::upstream::UpstreamGroup::new(
        eggress_routing::UpstreamGroupId(Arc::from("rr-group")),
        eggress_routing::scheduler::SchedulerKind::RoundRobin,
        Arc::from(vec![u1.clone(), u2.clone()]),
        eggress_routing::upstream::GroupFallback::Reject,
    );

    let rules = vec![eggress_routing::CompiledRule {
        id: eggress_routing::RuleId(Arc::from("rr-rule")),
        matcher: MatchExpr::Any,
        action: RouteActionSpec::UpstreamGroup(eggress_routing::UpstreamGroupId(Arc::from(
            "rr-group",
        ))),
    }];

    let router = Router::with_groups(
        rules,
        RouteActionSpec::Direct,
        vec![(
            eggress_routing::UpstreamGroupId(Arc::from("rr-group")),
            group,
        )],
    );

    let target = target_domain("example.com", 80);
    let req = make_request(&target, None, "http-in", ProtocolId::Http);

    let mut selections = Vec::new();
    for _ in 0..4 {
        let decision = router.decide(&req);
        if let eggress_routing::RouteDecision::UpstreamGroup { .. } = decision {
            let selected = router.select(&decision, &req).unwrap();
            match selected {
                eggress_routing::SelectedRoute::Upstream { upstream, .. } => {
                    selections.push(upstream.to_string());
                }
                _ => panic!("expected Upstream selection"),
            }
        }
    }

    assert_eq!(selections[0], "p1");
    assert_eq!(selections[1], "p2");
    assert_eq!(selections[2], "p1");
    assert_eq!(selections[3], "p2");
}

#[test]
fn direct_fallback_reason_preserved() {
    let u1 = Arc::new(eggress_routing::upstream::UpstreamRuntime::new(
        eggress_core::UpstreamId::new("p1"),
        eggress_uri::ProxyChainSpec { hops: vec![] },
    ));
    u1.set_enabled(false);

    let group = eggress_routing::upstream::UpstreamGroup::new(
        eggress_routing::UpstreamGroupId(Arc::from("fallback-group")),
        eggress_routing::scheduler::SchedulerKind::RoundRobin,
        Arc::from(vec![u1.clone()]),
        eggress_routing::upstream::GroupFallback::Direct,
    );

    let rules = vec![eggress_routing::CompiledRule {
        id: eggress_routing::RuleId(Arc::from("fb-rule")),
        matcher: MatchExpr::Any,
        action: RouteActionSpec::UpstreamGroup(eggress_routing::UpstreamGroupId(Arc::from(
            "fallback-group",
        ))),
    }];

    let router = Router::with_groups(
        rules,
        RouteActionSpec::Direct,
        vec![(
            eggress_routing::UpstreamGroupId(Arc::from("fallback-group")),
            group,
        )],
    );

    let target = target_domain("example.com", 80);
    let req = make_request(&target, None, "http-in", ProtocolId::Http);
    let decision = router.decide(&req);
    let selected = router.select(&decision, &req).unwrap();

    match selected {
        eggress_routing::SelectedRoute::Direct {
            selection_reason, ..
        } => {
            assert_eq!(
                selection_reason,
                eggress_routing::SelectionReason::DirectFallback
            );
        }
        other => panic!(
            "expected Direct with DirectFallback reason, got {:?}",
            other
        ),
    }
}

#[test]
fn rule_order_first_match_wins() {
    let router = Router::new(
        vec![
            eggress_routing::CompiledRule {
                id: eggress_routing::RuleId(Arc::from("first")),
                matcher: MatchExpr::Any,
                action: RouteActionSpec::Direct,
            },
            eggress_routing::CompiledRule {
                id: eggress_routing::RuleId(Arc::from("second")),
                matcher: MatchExpr::Any,
                action: RouteActionSpec::Reject(eggress_core::RejectReason::Blocked),
            },
        ],
        RouteActionSpec::Direct,
    );

    let target = target_domain("example.com", 80);
    let req = make_request(&target, None, "http-in", ProtocolId::Http);
    let decision = router.decide(&req);
    match decision {
        eggress_routing::RouteDecision::Direct { rule } => {
            assert_eq!(rule.0.as_ref(), "first");
        }
        other => panic!("expected Direct from first rule, got {:?}", other),
    }
}
