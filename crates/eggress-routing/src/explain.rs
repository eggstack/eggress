//! Route explanation DTO construction.
//!
//! Separated from [`crate::router`] so diagnostic rendering does not dominate
//! the core matcher/selection modules. Behavior is preserved exactly.

use crate::health;
use crate::model::{RouteExplanation, RouteRequest, UpstreamExplanation};
use crate::router::Router;

impl Router {
    pub fn explain(&self, request: &RouteRequest, generation: u64) -> RouteExplanation {
        let decision = self.decide(request);
        let target = request.target.to_string();
        let listener = request.listener.to_string();
        let protocol = request.inbound_protocol.to_string();
        let transport = request.transport.to_string();

        let (matched_rule, action, upstream_group, scheduler, eligible, selected, chain) =
            match &decision {
                crate::model::RouteDecision::Direct { rule } => (
                    Some(rule.to_string()),
                    "direct".to_string(),
                    None,
                    None,
                    vec![],
                    None,
                    None,
                ),
                crate::model::RouteDecision::Reject { rule, reason } => (
                    Some(rule.to_string()),
                    format!("reject({})", reason),
                    None,
                    None,
                    vec![],
                    None,
                    None,
                ),
                crate::model::RouteDecision::UpstreamGroup { rule, group } => {
                    let group_arc = self.groups.get(group);
                    let group_id = group.to_string();

                    if let Some(upstream_group) = group_arc {
                        let sched_name = match upstream_group.scheduler_kind {
                            crate::scheduler::SchedulerKind::FirstAvailable => "first-available",
                            crate::scheduler::SchedulerKind::RoundRobin => "round-robin",
                            crate::scheduler::SchedulerKind::Random => "random",
                            crate::scheduler::SchedulerKind::LeastConnections => {
                                "least-connections"
                            }
                        };

                        let eligible_upstreams: Vec<UpstreamExplanation> = upstream_group
                            .members
                            .iter()
                            .map(|m| {
                                let health_state = m.health.state();
                                let eligible = health::is_eligible(m);
                                UpstreamExplanation {
                                    id: m.id.to_string(),
                                    health: format!("{:?}", health_state),
                                    eligible,
                                    active: m.active.load(std::sync::atomic::Ordering::Relaxed),
                                    in_flight: m
                                        .in_flight
                                        .load(std::sync::atomic::Ordering::Relaxed),
                                }
                            })
                            .collect();

                        let candidates: Vec<_> = upstream_group
                            .members
                            .iter()
                            .filter(|m| health::is_eligible(m))
                            .cloned()
                            .collect();

                        let (sel, sel_chain) = if !candidates.is_empty() {
                            if let Some(sel) = upstream_group.scheduler.preview(
                                upstream_group,
                                &candidates,
                                request,
                            ) {
                                let chain_str =
                                    format!("{}", eggress_uri::RedactedUri::new(&sel.chain));
                                (Some(sel.id.to_string()), Some(chain_str))
                            } else {
                                (None, None)
                            }
                        } else {
                            match &upstream_group.fallback {
                                crate::upstream::GroupFallback::Direct
                                | crate::upstream::GroupFallback::Reject => (None, None),
                                crate::upstream::GroupFallback::UseUnhealthy => {
                                    let enabled_members: Vec<_> = upstream_group
                                        .members
                                        .iter()
                                        .filter(|m| m.is_enabled())
                                        .cloned()
                                        .collect();
                                    if enabled_members.is_empty() {
                                        (None, None)
                                    } else if let Some(sel) = upstream_group
                                        .scheduler
                                        .preview(upstream_group, &enabled_members, request)
                                        .or_else(|| enabled_members.first().cloned())
                                    {
                                        // preview uses is_eligible internally; for enabled
                                        // unhealthy members it may return None, so fall back
                                        // to a deterministic first-enabled choice to avoid
                                        // reporting None while select would return an
                                        // unhealthy upstream. A non-mutating preview is
                                        // kept to preserve the `explain_does_not_mutate`
                                        // invariant.
                                        let chain_str = format!(
                                            "{}",
                                            eggress_uri::RedactedUri::new(&sel.chain)
                                        );
                                        (Some(sel.id.to_string()), Some(chain_str))
                                    } else {
                                        let sel = &enabled_members[0];
                                        let chain_str = format!(
                                            "{}",
                                            eggress_uri::RedactedUri::new(&sel.chain)
                                        );
                                        (Some(sel.id.to_string()), Some(chain_str))
                                    }
                                }
                            }
                        };

                        (
                            Some(rule.to_string()),
                            format!("upstream group {}", group_id),
                            Some(group_id),
                            Some(sched_name.to_string()),
                            eligible_upstreams,
                            sel,
                            sel_chain,
                        )
                    } else {
                        (
                            Some(rule.to_string()),
                            format!("upstream group {}", group_id),
                            Some(group_id),
                            None,
                            vec![],
                            None,
                            None,
                        )
                    }
                }
            };

        RouteExplanation {
            target,
            listener,
            protocol,
            transport,
            matched_rule,
            action,
            upstream_group,
            scheduler,
            eligible_upstreams: eligible,
            selected_upstream: selected,
            chain,
            generation,
        }
    }
}
