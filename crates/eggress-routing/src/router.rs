//! Route selection orchestration: `Router` plus shared service handles.
//!
//! Matching logic lives in [`crate::matcher`]; data-model definitions live in
//! [`crate::model`]; diagnostic DTO construction lives in [`crate::explain`].
//! This module owns decision/selection and the atomic snapshot-swapped
//! [`SharedRoutingService`].

use crate::health;
use crate::lease;
use crate::model::{
    RouteActionSpec, RouteDecision, RouteError, RouteRequest, RouteService, SelectedRoute,
    SelectionReason, UpstreamGroupId, DEFAULT_RULE_ID,
};
use crate::rule::CompiledRule;

pub struct Router {
    pub(crate) rules: Vec<CompiledRule>,
    pub(crate) default_action: RouteActionSpec,
    pub(crate) groups:
        std::collections::HashMap<UpstreamGroupId, std::sync::Arc<crate::upstream::UpstreamGroup>>,
}

impl Router {
    pub fn new(rules: Vec<CompiledRule>, default_action: RouteActionSpec) -> Self {
        Self {
            rules,
            default_action,
            groups: std::collections::HashMap::new(),
        }
    }

    pub fn with_groups(
        rules: Vec<CompiledRule>,
        default_action: RouteActionSpec,
        groups: Vec<(UpstreamGroupId, crate::upstream::UpstreamGroup)>,
    ) -> Self {
        Self {
            rules,
            default_action,
            groups: groups
                .into_iter()
                .map(|(id, g)| (id, std::sync::Arc::new(g)))
                .collect(),
        }
    }

    pub fn decide(&self, request: &RouteRequest) -> RouteDecision {
        for rule in &self.rules {
            if rule.matcher.matches(request) {
                return match &rule.action {
                    RouteActionSpec::Direct => RouteDecision::Direct {
                        rule: rule.id.clone(),
                    },
                    RouteActionSpec::UpstreamGroup(group) => RouteDecision::UpstreamGroup {
                        rule: rule.id.clone(),
                        group: group.clone(),
                    },
                    RouteActionSpec::Reject(reason) => RouteDecision::Reject {
                        rule: rule.id.clone(),
                        reason: reason.clone(),
                    },
                };
            }
        }
        match &self.default_action {
            RouteActionSpec::Direct => RouteDecision::Direct {
                rule: (*DEFAULT_RULE_ID).clone(),
            },
            RouteActionSpec::UpstreamGroup(group) => RouteDecision::UpstreamGroup {
                rule: (*DEFAULT_RULE_ID).clone(),
                group: group.clone(),
            },
            RouteActionSpec::Reject(reason) => RouteDecision::Reject {
                rule: (*DEFAULT_RULE_ID).clone(),
                reason: reason.clone(),
            },
        }
    }

    pub fn rules(&self) -> &[CompiledRule] {
        &self.rules
    }

    pub fn default_action(&self) -> &RouteActionSpec {
        &self.default_action
    }

    pub fn groups(
        &self,
    ) -> &std::collections::HashMap<UpstreamGroupId, std::sync::Arc<crate::upstream::UpstreamGroup>>
    {
        &self.groups
    }

    /// Select an upstream for a previously evaluated decision.
    pub fn select(
        &self,
        decision: &RouteDecision,
        request: &RouteRequest<'_>,
    ) -> Result<SelectedRoute, RouteError> {
        match decision {
            RouteDecision::Direct { .. } => Ok(SelectedRoute::Direct {
                decision: decision.clone(),
                selection_reason: SelectionReason::Normal,
            }),
            RouteDecision::Reject { rule, reason } => Err(RouteError::Rejected {
                rule: rule.clone(),
                reason: reason.clone(),
            }),
            RouteDecision::UpstreamGroup { group, .. } => {
                let upstream_group = self
                    .groups
                    .get(group)
                    .ok_or_else(|| RouteError::UnknownGroup(group.clone()))?;

                let candidates: Vec<_> = upstream_group
                    .members
                    .iter()
                    .filter(|m| health::is_eligible(m))
                    .cloned()
                    .collect();

                let (selected, selection_reason) = if !candidates.is_empty() {
                    let sel = upstream_group
                        .scheduler
                        .select(upstream_group, &candidates, request)
                        .ok_or_else(|| RouteError::NoEligibleUpstream(group.clone()))?;
                    (sel, SelectionReason::Normal)
                } else {
                    match &upstream_group.fallback {
                        crate::upstream::GroupFallback::Reject => {
                            return Err(RouteError::NoEligibleUpstream(group.clone()));
                        }
                        crate::upstream::GroupFallback::Direct => {
                            return Ok(SelectedRoute::Direct {
                                decision: decision.clone(),
                                selection_reason: SelectionReason::DirectFallback,
                            });
                        }
                        crate::upstream::GroupFallback::UseUnhealthy => {
                            let enabled_members: Vec<_> = upstream_group
                                .members
                                .iter()
                                .filter(|m| m.is_enabled())
                                .cloned()
                                .collect();
                            let sel = upstream_group
                                .scheduler
                                .select_enabled(upstream_group, &enabled_members, request)
                                .or_else(|| enabled_members.first().cloned())
                                .ok_or_else(|| RouteError::NoEligibleUpstream(group.clone()))?;
                            (sel, SelectionReason::UnhealthyFallback)
                        }
                    }
                };

                let pending_lease = lease::PendingLease::new(selected.clone());

                Ok(SelectedRoute::Upstream {
                    decision: decision.clone(),
                    group: group.clone(),
                    upstream: selected.id.clone(),
                    chain: selected.chain.clone(),
                    pending_lease,
                    selection_reason,
                })
            }
        }
    }
}

impl RouteService for Router {
    fn route(&self, request: &RouteRequest<'_>) -> Result<SelectedRoute, RouteError> {
        let decision = self.decide(request);
        self.select(&decision, request)
    }
}

pub struct RoutingServiceInner {
    pub router: std::sync::Arc<Router>,
}

pub struct SharedRoutingService {
    inner: arc_swap::ArcSwap<RoutingServiceInner>,
}

impl SharedRoutingService {
    pub fn new(router: Router) -> Self {
        Self {
            inner: arc_swap::ArcSwap::from_pointee(RoutingServiceInner {
                router: std::sync::Arc::new(router),
            }),
        }
    }

    pub fn new_arc(router: std::sync::Arc<Router>) -> Self {
        Self {
            inner: arc_swap::ArcSwap::from_pointee(RoutingServiceInner { router }),
        }
    }

    pub fn router(&self) -> std::sync::Arc<Router> {
        self.inner.load().router.clone()
    }

    /// Evaluate policy against one routing snapshot without selecting an
    /// upstream. This is for callers, such as reverse routing, that use the
    /// decision only as an authorization gate.
    pub fn policy_decision(&self, request: &RouteRequest<'_>) -> RouteDecision {
        self.inner.load().router.decide(request)
    }

    pub fn swap(&self, router: Router) {
        let new_inner = RoutingServiceInner {
            router: std::sync::Arc::new(router),
        };
        self.inner.store(std::sync::Arc::new(new_inner));
    }

    pub fn swap_arc(&self, router: std::sync::Arc<Router>) {
        let new_inner = RoutingServiceInner { router };
        self.inner.store(std::sync::Arc::new(new_inner));
    }
}

impl RouteService for SharedRoutingService {
    fn route(&self, request: &RouteRequest<'_>) -> Result<SelectedRoute, RouteError> {
        let inner = self.inner.load();
        let decision = inner.router.decide(request);
        inner.router.select(&decision, request)
    }
}
