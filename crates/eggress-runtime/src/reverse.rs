//! Runtime-side adapter that wires the `eggress-routing` engine into the
//! `eggress-protocol-reverse` client.
//!
//! The reverse client dials a fixed external host:port and forwards the
//! resulting connections back over the control channel. The runtime is
//! still in charge of authorization: each reconnect consults the router
//! against a synthetic `RouteRequest` (transport = `ReverseTcp`, listener
//! = the configured reverse listener name, target = the external target)
//! and rejects the connection with a structured reason when the policy
//! says no.
//!
//! The adapter implements the `TargetResolver` trait from the protocol
//! crate without requiring the protocol crate to depend on the routing
//! crate directly.
use std::net::SocketAddr;
use std::sync::Arc;

use eggress_core::{ClientIdentity, TargetAddr, TargetHost};
use eggress_protocol_reverse::client::{TargetResolution, TargetResolver};
use eggress_routing::{
    RouteDecision, RouteRequest, RouteService, SharedRoutingService, TransportKind,
};
use tracing::warn;

/// Adapter that gates a fixed external target through a `SharedRoutingService`.
///
/// Each call to `resolve()`:
/// 1. Builds a synthetic `RouteRequest` with `transport = ReverseTcp`,
///    `listener = reverse_listener_name`, and `target = (host, port)`.
/// 2. Calls `router.decide()`. If the decision is `Direct` or
///    `UpstreamGroup`, returns `TargetResolution::Connect`. For reverse
///    traffic, the routing decision is a gate, not a redirect — the
///    reverse client always dials the same external target.
/// 3. If the decision is `Reject`, returns `TargetResolution::Reject`
///    with the reason.
pub struct RouteEngineTargetResolver {
    router: SharedRoutingService,
    target: TargetAddr,
    reverse_listener: Arc<str>,
    client_addr: Option<SocketAddr>,
}

impl RouteEngineTargetResolver {
    /// Build a new resolver.
    pub fn new(
        router: SharedRoutingService,
        host: String,
        port: u16,
        reverse_listener: Arc<str>,
        client_addr: Option<SocketAddr>,
    ) -> Self {
        let target = TargetAddr {
            host: TargetHost::Domain(host),
            port,
        };
        Self {
            router,
            target,
            reverse_listener,
            client_addr,
        }
    }

    /// Borrow the current target. Used by callers that want to display
    /// the configured target without going through the resolver.
    pub fn target(&self) -> &TargetAddr {
        &self.target
    }
}

impl TargetResolver for RouteEngineTargetResolver {
    fn resolve(&self) -> TargetResolution {
        let request = RouteRequest {
            target: &self.target,
            source: self.client_addr,
            listener: self.reverse_listener.as_ref(),
            inbound_protocol: eggress_core::ProtocolId::Reverse,
            identity: &ClientIdentity::Anonymous,
            transport: TransportKind::ReverseTcp,
        };

        match self.router.decide(&request) {
            RouteDecision::Direct { .. } | RouteDecision::UpstreamGroup { .. } => {
                TargetResolution::Connect {
                    host: self.target.host.to_string(),
                    port: self.target.port,
                }
            }
            RouteDecision::Reject { reason, .. } => {
                warn!(
                    listener = %self.reverse_listener,
                    target = %self.target,
                    ?reason,
                    "reverse route rejected by policy",
                );
                TargetResolution::Reject {
                    reason: format!("route rejected: {:?}", reason),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::RejectReason;
    use eggress_routing::{CompiledRule, RouteActionSpec, Router, RuleId, SharedRoutingService};

    fn router_with_default(action: RouteActionSpec) -> SharedRoutingService {
        SharedRoutingService::new(Router::new(Vec::new(), action))
    }

    #[test]
    fn allows_direct_route() {
        let r = router_with_default(RouteActionSpec::Direct);
        let resolver = RouteEngineTargetResolver::new(
            r,
            "127.0.0.1".to_string(),
            8080,
            Arc::from("rev-1"),
            None,
        );
        match resolver.resolve() {
            TargetResolution::Connect { host, port } => {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(port, 8080);
            }
            other => panic!("expected Connect, got {:?}", other),
        }
    }

    #[test]
    fn allows_upstream_group_route() {
        let r = router_with_default(RouteActionSpec::UpstreamGroup(
            eggress_routing::UpstreamGroupId(Arc::from("group-1")),
        ));
        let resolver = RouteEngineTargetResolver::new(
            r,
            "127.0.0.1".to_string(),
            9090,
            Arc::from("rev-1"),
            None,
        );
        // UpstreamGroup decision still maps to Connect for the reverse client
        // — routing is a gate, not a redirect.
        match resolver.resolve() {
            TargetResolution::Connect { port, .. } => assert_eq!(port, 9090),
            other => panic!("expected Connect, got {:?}", other),
        }
    }

    #[test]
    fn rejects_reject_route() {
        let rule = CompiledRule {
            id: RuleId(Arc::from("deny-all")),
            matcher: eggress_routing::MatchExpr::Any,
            action: RouteActionSpec::Reject(RejectReason::AccessDenied),
        };
        let router = SharedRoutingService::new(Router::new(vec![rule], RouteActionSpec::Direct));
        let resolver = RouteEngineTargetResolver::new(
            router,
            "127.0.0.1".to_string(),
            80,
            Arc::from("rev-1"),
            None,
        );
        match resolver.resolve() {
            TargetResolution::Reject { reason } => {
                assert!(reason.contains("route rejected"), "reason: {}", reason);
            }
            other => panic!("expected Reject, got {:?}", other),
        }
    }

    #[test]
    fn reverse_listener_matcher_only_matches_reverse_tcp() {
        // Build a rule that matches reverse listener "rev-1"
        let rule = CompiledRule {
            id: RuleId(Arc::from("reverse-only")),
            matcher: eggress_routing::MatchExpr::ReverseListener(Arc::from("rev-1")),
            action: RouteActionSpec::Direct,
        };
        let router = SharedRoutingService::new(Router::new(
            vec![rule],
            RouteActionSpec::Reject(RejectReason::AccessDenied),
        ));
        let resolver = RouteEngineTargetResolver::new(
            router,
            "127.0.0.1".to_string(),
            8080,
            Arc::from("rev-1"),
            None,
        );
        // The reverse listener matcher should match; default falls through to it
        assert!(matches!(
            resolver.resolve(),
            TargetResolution::Connect { .. }
        ));
    }
}
