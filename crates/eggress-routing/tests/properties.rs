use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use proptest::prelude::*;

use eggress_core::{ClientIdentity, ProtocolId, RejectReason, TargetAddr, TargetHost};
use eggress_routing::{
    CompiledRule, MatchExpr, PortMatcher, RouteActionSpec, RouteDecision, RouteRequest, Router,
    TransportKind,
};

fn target_domain(host: &str, port: u16) -> TargetAddr {
    TargetAddr {
        host: TargetHost::Domain(host.to_string()),
        port,
    }
}

fn target_ip(ip: IpAddr, port: u16) -> TargetAddr {
    TargetAddr {
        host: TargetHost::Ip(ip),
        port,
    }
}

fn make_request<'a>(
    target: &'a TargetAddr,
    source: Option<SocketAddr>,
    listener: &'a str,
    protocol: ProtocolId,
    identity: &'a ClientIdentity,
) -> RouteRequest<'a> {
    RouteRequest {
        target,
        source,
        listener,
        inbound_protocol: protocol,
        identity,
        transport: TransportKind::Tcp,
    }
}

const ANON: ClientIdentity = ClientIdentity::Anonymous;

fn is_reject(decision: &RouteDecision) -> bool {
    matches!(decision, RouteDecision::Reject { .. })
}

fn is_direct(decision: &RouteDecision) -> bool {
    matches!(decision, RouteDecision::Direct { .. })
}

fn is_direct_default(decision: &RouteDecision) -> bool {
    matches!(decision, RouteDecision::Direct { ref rule, .. } if rule.0.as_ref() == "default")
}

fn is_reject_with_rule(decision: &RouteDecision, expected_rule: &str) -> bool {
    matches!(decision, RouteDecision::Reject { ref rule, .. } if rule.0.as_ref() == expected_rule)
}

proptest! {
    #[test]
    fn reject_rule_stops_routing(host in "[a-z]{1,32}", port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("block")),
            matcher: MatchExpr::HostSuffix(Arc::from("blocked.com")),
            action: RouteActionSpec::Reject(RejectReason::Blocked),
        }];
        let router = Router::new(rules, RouteActionSpec::Direct);
        let domain = format!("{}.blocked.com", host);
        let target = target_domain(&domain, port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        prop_assert!(is_reject(&decision), "expected Reject, got {:?}", decision);
    }

    #[test]
    fn direct_fallback_only_when_configured(host in "[a-z]{1,32}", port in 1u16..65535u16) {
        let router = Router::new(vec![], RouteActionSpec::Direct);
        let target = target_domain(&format!("{}.example.com", host), port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        prop_assert!(is_direct(&decision), "expected Direct from default, got {:?}", decision);
    }

    #[test]
    fn reject_default_when_configured(host in "[a-z]{1,32}", port in 1u16..65535u16) {
        let router = Router::new(vec![], RouteActionSpec::Reject(RejectReason::AccessDenied));
        let target = target_domain(&format!("{}.example.com", host), port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        prop_assert!(is_reject(&decision), "expected Reject from default, got {:?}", decision);
    }

    #[test]
    fn first_match_wins_over_later_rules(host in "[a-z]{1,32}", port in 1u16..65535u16) {
        let rules = vec![
            CompiledRule {
                id: eggress_routing::RuleId(Arc::from("first")),
                matcher: MatchExpr::HostSuffix(Arc::from("example.com")),
                action: RouteActionSpec::Reject(RejectReason::Blocked),
            },
            CompiledRule {
                id: eggress_routing::RuleId(Arc::from("second")),
                matcher: MatchExpr::Any,
                action: RouteActionSpec::Direct,
            },
        ];
        let router = Router::new(rules, RouteActionSpec::Direct);
        let target = target_domain(&format!("{}.example.com", host), port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        prop_assert!(
            is_reject_with_rule(&decision, "first"),
            "expected first rule to win, got {:?}",
            decision
        );
    }

    #[test]
    fn match_none_falls_to_default(host in "[a-z]{1,32}", port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("specific")),
            matcher: MatchExpr::HostExact(Arc::from("nomatch.example.com")),
            action: RouteActionSpec::Reject(RejectReason::Blocked),
        }];
        let router = Router::new(rules, RouteActionSpec::Direct);
        let target = target_domain(&format!("{}.other.com", host), port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        prop_assert!(
            is_direct_default(&decision),
            "expected default Direct, got {:?}",
            decision
        );
    }

    #[test]
    fn host_suffix_match_case_insensitive(host in "[aA-zZ]{1,16}", port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::HostSuffix(Arc::from("example.com")),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain(&format!("{}.EXAMPLE.COM", host), port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        prop_assert!(is_direct(&decision), "expected Direct match, got {:?}", decision);
    }

    #[test]
    fn port_range_matching(port in 8000u16..9000u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::DestinationPort(PortMatcher::Range { start: 8000, end: 9000 }),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        prop_assert!(is_direct(&decision), "expected Direct for port in range, got {:?}", decision);
    }

    #[test]
    fn port_exact_match(port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::DestinationPort(PortMatcher::Exact(443)),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        if port == 443 {
            prop_assert!(is_direct(&decision));
        } else {
            prop_assert!(is_reject(&decision));
        }
    }

    #[test]
    fn ipv4_cidr_match(ip in any::<Ipv4Addr>()) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::DestinationCidr("10.0.0.0/8".parse().unwrap()),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_ip(IpAddr::V4(ip), 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        if ip.octets()[0] == 10 {
            prop_assert!(is_direct(&decision));
        } else {
            prop_assert!(is_reject(&decision));
        }
    }

    #[test]
    fn ipv6_cidr_match(ip in any::<Ipv6Addr>()) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::DestinationCidr("::1/128".parse().unwrap()),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_ip(IpAddr::V6(ip), 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        if ip == Ipv6Addr::LOCALHOST {
            prop_assert!(is_direct(&decision));
        } else {
            prop_assert!(is_reject(&decision));
        }
    }

    #[test]
    fn identity_match(username in "[a-z]{1,32}", port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::Identity(Arc::from(username.as_str())),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", port);
        let ident = ClientIdentity::Username(username.clone());
        let req = make_request(&target, None, "l", ProtocolId::Http, &ident);
        let decision = router.decide(&req);
        prop_assert!(
            is_direct(&decision),
            "expected Direct for matching identity, got {:?}",
            decision
        );
    }

    #[test]
    fn identity_no_match_anonymous(username in "[a-z]{1,32}", port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::Identity(Arc::from(username.as_str())),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        prop_assert!(
            is_reject(&decision),
            "expected Reject for anonymous, got {:?}",
            decision
        );
    }

    #[test]
    fn all_composite_needs_all_conditions(port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::All(vec![
                MatchExpr::HostExact(Arc::from("example.com")),
                MatchExpr::DestinationPort(PortMatcher::Exact(443)),
            ]),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));

        let target = target_domain("example.com", port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        if port == 443 {
            prop_assert!(is_direct(&decision));
        } else {
            prop_assert!(is_reject(&decision));
        }
    }

    #[test]
    fn any_of_composite_needs_one_match(port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::AnyOf(vec![
                MatchExpr::DestinationPort(PortMatcher::Exact(80)),
                MatchExpr::DestinationPort(PortMatcher::Exact(443)),
            ]),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        if port == 80 || port == 443 {
            prop_assert!(is_direct(&decision));
        } else {
            prop_assert!(is_reject(&decision));
        }
    }

    #[test]
    fn not_composite_negates(port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::Not(Box::new(MatchExpr::HostExact(Arc::from("example.com")))),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));

        let target_other = target_domain("other.com", port);
        let req_other = make_request(&target_other, None, "l", ProtocolId::Http, &ANON);
        prop_assert!(
            is_direct(&router.decide(&req_other)),
            "NOT should match non-example.com"
        );

        let target_exact = target_domain("example.com", port);
        let req_exact = make_request(&target_exact, None, "l", ProtocolId::Http, &ANON);
        prop_assert!(
            is_reject(&router.decide(&req_exact)),
            "NOT should not match example.com"
        );
    }

    #[test]
    fn protocol_matching(proto in 0u8..=5u8) {
        let protocol = match proto {
            0 => ProtocolId::Http,
            1 => ProtocolId::Socks4,
            2 => ProtocolId::Socks5,
            _ => ProtocolId::Http,
        };
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::Protocol(ProtocolId::Socks5),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", protocol, &ANON);
        let decision = router.decide(&req);
        if proto == 2 {
            prop_assert!(is_direct(&decision));
        } else {
            prop_assert!(is_reject(&decision));
        }
    }

    #[test]
    fn listener_matching(listener in "[a-z]{1,16}", port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::Listener(Arc::from("target_listener")),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", port);
        let req = make_request(&target, None, &listener, ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        if listener == "target_listener" {
            prop_assert!(is_direct(&decision));
        } else {
            prop_assert!(is_reject(&decision));
        }
    }

    #[test]
    fn source_cidr_match(src_port in 1u16..65535u16) {
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::SourceCidr("192.168.0.0/16".parse().unwrap()),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", 80);
        let source = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), src_port);
        let req = make_request(&target, Some(source), "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        prop_assert!(
            is_direct(&decision),
            "expected Direct for 192.168.x.x source, got {:?}",
            decision
        );
    }

    #[test]
    fn source_cidr_no_match(src_ip in any::<Ipv4Addr>(), src_port in 1u16..65535u16) {
        prop_assume!(src_ip.octets()[0] != 192 || src_ip.octets()[1] != 168);
        let rules = vec![CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: MatchExpr::SourceCidr("192.168.0.0/16".parse().unwrap()),
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", 80);
        let source = SocketAddr::new(IpAddr::V4(src_ip), src_port);
        let req = make_request(&target, Some(source), "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        prop_assert!(
            is_reject(&decision),
            "expected Reject for non-192.168.x.x source, got {:?}",
            decision
        );
    }

    #[test]
    fn router_never_panics(host in "[a-z]{1,64}", port in 1u16..65535u16) {
        let rules = vec![
            CompiledRule {
                id: eggress_routing::RuleId(Arc::from("r1")),
                matcher: MatchExpr::All(vec![
                    MatchExpr::HostSuffix(Arc::from("example.com")),
                    MatchExpr::DestinationPort(PortMatcher::Exact(443)),
                ]),
                action: RouteActionSpec::Direct,
            },
            CompiledRule {
                id: eggress_routing::RuleId(Arc::from("r2")),
                matcher: MatchExpr::Not(Box::new(MatchExpr::HostSuffix(Arc::from("blocked.com")))),
                action: RouteActionSpec::Reject(RejectReason::Blocked),
            },
        ];
        let router = Router::new(rules, RouteActionSpec::Direct);
        let target = target_domain(&host, port);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let _ = router.decide(&req);
    }
}
