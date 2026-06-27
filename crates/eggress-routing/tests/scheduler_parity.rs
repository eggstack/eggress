use std::sync::atomic::Ordering;
use std::sync::Arc;

use eggress_core::{ClientIdentity, ProtocolId, TargetAddr, TargetHost, UpstreamId};
use eggress_routing::health::HealthState;
use eggress_routing::lease::PendingLease;
use eggress_routing::scheduler::{
    LeastConnectionsScheduler, RoundRobinScheduler, Scheduler, SchedulerKind,
};
use eggress_routing::upstream::{GroupFallback, UpstreamGroup, UpstreamRuntime};
use eggress_routing::{RouteRequest, TransportKind, UpstreamGroupId};
use eggress_uri::ProxyChainSpec;

fn make_upstream(id: &str) -> Arc<UpstreamRuntime> {
    Arc::new(UpstreamRuntime::new(
        UpstreamId::new(id),
        ProxyChainSpec { hops: vec![] },
    ))
}

fn make_upstream_unhealthy(id: &str) -> Arc<UpstreamRuntime> {
    Arc::new(UpstreamRuntime::new_with_health(
        UpstreamId::new(id),
        ProxyChainSpec { hops: vec![] },
        HealthState::Unhealthy,
    ))
}

fn make_group(
    members: Vec<Arc<UpstreamRuntime>>,
    scheduler: SchedulerKind,
    fallback: GroupFallback,
) -> UpstreamGroup {
    UpstreamGroup::new(
        UpstreamGroupId(Arc::from("test-group")),
        scheduler,
        Arc::from(members),
        fallback,
    )
}

fn dummy_request<'a>(target: &'a TargetAddr) -> RouteRequest<'a> {
    RouteRequest {
        target,
        source: None,
        listener: "test",
        inbound_protocol: ProtocolId::Http,
        identity: &ClientIdentity::Anonymous,
        transport: TransportKind::Tcp,
    }
}

fn target_domain(host: &str, port: u16) -> TargetAddr {
    TargetAddr {
        host: TargetHost::Domain(host.to_string()),
        port,
    }
}

// ---------------------------------------------------------------------------
// 1. round_robin_cycles_across_multiple_selections
// ---------------------------------------------------------------------------

#[test]
fn round_robin_cycles_across_multiple_selections() {
    let a = make_upstream("a");
    let b = make_upstream("b");
    let c = make_upstream("c");
    let group = make_group(
        vec![a.clone(), b.clone(), c.clone()],
        SchedulerKind::RoundRobin,
        GroupFallback::Reject,
    );

    let target = target_domain("example.com", 80);
    let req = dummy_request(&target);
    let scheduler = RoundRobinScheduler::new();

    let selections: Vec<_> = (0..6)
        .map(|_| {
            scheduler
                .select(&group, &group.members, &req)
                .unwrap()
                .id
                .to_string()
        })
        .collect();

    assert_eq!(
        selections,
        vec!["a", "b", "c", "a", "b", "c"],
        "round-robin should cycle a, b, c, a, b, c"
    );
}

// ---------------------------------------------------------------------------
// 2. round_robin_skips_disabled_upstream
// ---------------------------------------------------------------------------

#[test]
fn round_robin_skips_disabled_upstream() {
    let a = make_upstream("a");
    let b = make_upstream("b");
    let c = make_upstream("c");
    b.set_enabled(false);

    let group = make_group(
        vec![a.clone(), b.clone(), c.clone()],
        SchedulerKind::RoundRobin,
        GroupFallback::Reject,
    );

    let target = target_domain("example.com", 80);
    let req = dummy_request(&target);
    let scheduler = RoundRobinScheduler::new();

    let selections: Vec<_> = (0..6)
        .map(|_| {
            scheduler
                .select(&group, &group.members, &req)
                .unwrap()
                .id
                .to_string()
        })
        .collect();

    // Cursor advances by 1 each call. With [a, b(disabled), c]:
    // call 1: start=0 → a
    // call 2: start=1 → skip b, pick c
    // call 3: start=2 → c
    // call 4: start=0 → a
    // call 5: start=1 → skip b, pick c
    // call 6: start=2 → c
    for s in &selections {
        assert_ne!(s, "b", "disabled upstream 'b' must never be selected");
    }
    assert_eq!(selections[0], "a");
    assert_eq!(selections[1], "c");
    assert_eq!(selections[2], "c");
    assert_eq!(selections[3], "a");
    assert_eq!(selections[4], "c");
    assert_eq!(selections[5], "c");
}

// ---------------------------------------------------------------------------
// 3. round_robin_skips_unhealthy_upstream
// ---------------------------------------------------------------------------

#[test]
fn round_robin_skips_unhealthy_upstream() {
    let a = make_upstream("a");
    let b = make_upstream_unhealthy("b");
    let c = make_upstream("c");

    let group = make_group(
        vec![a.clone(), b.clone(), c.clone()],
        SchedulerKind::RoundRobin,
        GroupFallback::Reject,
    );

    let target = target_domain("example.com", 80);
    let req = dummy_request(&target);
    let scheduler = RoundRobinScheduler::new();

    let selections: Vec<_> = (0..6)
        .map(|_| {
            scheduler
                .select(&group, &group.members, &req)
                .unwrap()
                .id
                .to_string()
        })
        .collect();

    // Same cursor behavior as disabled test: cursor advances by 1 each call.
    // With [a, b(unhealthy), c]:
    // call 1: start=0 → a
    // call 2: start=1 → skip b, pick c
    // call 3: start=2 → c
    // call 4: start=0 → a
    // call 5: start=1 → skip b, pick c
    // call 6: start=2 → c
    for s in &selections {
        assert_ne!(s, "b", "unhealthy upstream 'b' must never be selected");
    }
    assert_eq!(selections[0], "a");
    assert_eq!(selections[1], "c");
    assert_eq!(selections[2], "c");
    assert_eq!(selections[3], "a");
    assert_eq!(selections[4], "c");
    assert_eq!(selections[5], "c");
}

// ---------------------------------------------------------------------------
// 4. least_connections_picks_minimum_load
// ---------------------------------------------------------------------------

#[test]
fn least_connections_picks_minimum_load() {
    let a = make_upstream("a");
    let b = make_upstream("b");
    let c = make_upstream("c");

    // a: load=5, b: load=2, c: load=8
    a.active.fetch_add(5, Ordering::Relaxed);
    b.active.fetch_add(2, Ordering::Relaxed);
    c.active.fetch_add(8, Ordering::Relaxed);

    let group = make_group(
        vec![a.clone(), b.clone(), c.clone()],
        SchedulerKind::LeastConnections,
        GroupFallback::Reject,
    );

    let target = target_domain("example.com", 80);
    let req = dummy_request(&target);
    let scheduler = LeastConnectionsScheduler;

    let selected = scheduler.select(&group, &group.members, &req).unwrap();
    assert_eq!(
        selected.id,
        UpstreamId::new("b"),
        "should pick upstream with load=2"
    );
}

// ---------------------------------------------------------------------------
// 5. least_connections_tie_breaks_by_position
// ---------------------------------------------------------------------------

#[test]
fn least_connections_tie_breaks_by_position() {
    let a = make_upstream("a");
    let b = make_upstream("b");

    // Both at load=3
    a.active.fetch_add(3, Ordering::Relaxed);
    b.active.fetch_add(3, Ordering::Relaxed);

    let group = make_group(
        vec![a.clone(), b.clone()],
        SchedulerKind::LeastConnections,
        GroupFallback::Reject,
    );

    let target = target_domain("example.com", 80);
    let req = dummy_request(&target);
    let scheduler = LeastConnectionsScheduler;

    // min_by_key is stable, so first matching member wins on tie
    let selected = scheduler.select(&group, &group.members, &req).unwrap();
    assert_eq!(
        selected.id,
        UpstreamId::new("a"),
        "tie should be broken by first position"
    );
}

// ---------------------------------------------------------------------------
// 6. failed_pending_lease_releases_in_flight
// ---------------------------------------------------------------------------

#[test]
fn failed_pending_lease_releases_in_flight() {
    let u = make_upstream("up-1");
    assert_eq!(u.in_flight.load(Ordering::Relaxed), 0);

    let _pending = PendingLease::new(u.clone());
    assert_eq!(u.in_flight.load(Ordering::Relaxed), 1);

    // Drop without calling established()
    drop(_pending);
    assert_eq!(
        u.in_flight.load(Ordering::Relaxed),
        0,
        "in_flight should return to 0 after drop"
    );
}

// ---------------------------------------------------------------------------
// 7. active_lease_decrements_on_drop
// ---------------------------------------------------------------------------

#[test]
fn active_lease_decrements_on_drop() {
    let u = make_upstream("up-1");

    let pending = PendingLease::new(u.clone());
    assert_eq!(u.in_flight.load(Ordering::Relaxed), 1);
    assert_eq!(u.active.load(Ordering::Relaxed), 0);

    let active = pending.established();
    assert_eq!(u.in_flight.load(Ordering::Relaxed), 0);
    assert_eq!(u.active.load(Ordering::Relaxed), 1);

    drop(active);
    assert_eq!(
        u.active.load(Ordering::Relaxed),
        0,
        "active should return to 0 after drop"
    );
}

// ---------------------------------------------------------------------------
// 8. direct_fallback_only_when_configured
// ---------------------------------------------------------------------------

#[test]
fn direct_fallback_only_when_configured() {
    use eggress_routing::{
        CompiledRule, MatchExpr, RouteActionSpec, RouteService, Router, RuleId, SelectedRoute,
        SelectionReason,
    };

    // All upstreams disabled, GroupFallback::Direct
    let u1 = make_upstream("u1");
    u1.set_enabled(false);

    let group_id = UpstreamGroupId(Arc::from("grp"));
    let group = UpstreamGroup::new(
        group_id.clone(),
        SchedulerKind::FirstAvailable,
        Arc::from([u1.clone()]),
        GroupFallback::Direct,
    );

    let rule = CompiledRule {
        id: RuleId(Arc::from("r1")),
        matcher: MatchExpr::Any,
        action: RouteActionSpec::UpstreamGroup(group_id.clone()),
    };
    let router = Router::with_groups(vec![rule], RouteActionSpec::Direct, vec![(group_id, group)]);

    let target = target_domain("example.com", 443);
    let req = dummy_request(&target);
    let result = router.select(&router.decide(&req), &req);
    assert!(result.is_ok());
    match result.unwrap() {
        SelectedRoute::Direct {
            selection_reason, ..
        } => {
            assert_eq!(selection_reason, SelectionReason::DirectFallback);
        }
        other => panic!(
            "expected Direct with DirectFallback reason, got {:?}",
            other
        ),
    }
}

// ---------------------------------------------------------------------------
// 9. reject_when_all_upstreams_fail
// ---------------------------------------------------------------------------

#[test]
fn reject_when_all_upstreams_fail() {
    use eggress_routing::{
        CompiledRule, MatchExpr, RouteActionSpec, RouteError, RouteService, Router, RuleId,
    };

    let u1 = make_upstream("u1");
    u1.set_enabled(false);
    let u2 = make_upstream("u2");
    u2.set_enabled(false);

    let group_id = UpstreamGroupId(Arc::from("grp"));
    let group = UpstreamGroup::new(
        group_id.clone(),
        SchedulerKind::FirstAvailable,
        Arc::from([u1.clone(), u2.clone()]),
        GroupFallback::Reject,
    );

    let rule = CompiledRule {
        id: RuleId(Arc::from("r1")),
        matcher: MatchExpr::Any,
        action: RouteActionSpec::UpstreamGroup(group_id.clone()),
    };
    let router = Router::with_groups(vec![rule], RouteActionSpec::Direct, vec![(group_id, group)]);

    let target = target_domain("example.com", 443);
    let req = dummy_request(&target);
    let result = router.select(&router.decide(&req), &req);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RouteError::NoEligibleUpstream(_)
    ));
}

// ---------------------------------------------------------------------------
// 10. health_unavailable_upstream_skipped
// ---------------------------------------------------------------------------

#[test]
fn health_unavailable_upstream_skipped() {
    let healthy = make_upstream("healthy");
    let unhealthy = make_upstream_unhealthy("unhealthy");

    let group = make_group(
        vec![healthy.clone(), unhealthy.clone()],
        SchedulerKind::FirstAvailable,
        GroupFallback::Reject,
    );

    let target = target_domain("example.com", 80);
    let req = dummy_request(&target);

    // Use the scheduler directly with eligible candidates only
    let candidates: Vec<_> = group
        .members
        .iter()
        .filter(|m| eggress_routing::health::is_eligible(m))
        .cloned()
        .collect();

    assert_eq!(candidates.len(), 1, "only 1 upstream should be eligible");
    assert_eq!(candidates[0].id, UpstreamId::new("healthy"));

    let selected = group.scheduler.select(&group, &candidates, &req).unwrap();
    assert_eq!(selected.id, UpstreamId::new("healthy"));
}
