//! Routing data model: IDs, requests, decisions, and explanation DTOs.
//!
//! Matching logic lives in [`crate::matcher`]; route selection orchestration
//! lives in [`crate::router`]; diagnostic DTO construction lives in
//! [`crate::explain`]. This module owns only the shared types so matcher,
//! router, and explain boundaries stay narrow.

use std::sync::Arc;

use eggress_core::{ClientIdentity, ProtocolId, RejectReason, TargetAddr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransportKind {
    #[default]
    Tcp,
    Udp,
    /// Reverse-tunneled TCP traffic: the client (agent) dials the server,
    /// hands it an external listen address, and forwards the resulting
    /// connections back over the control channel. Routing decisions are
    /// still evaluated against the resolved external target.
    ReverseTcp,
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportKind::Tcp => write!(f, "tcp"),
            TransportKind::Udp => write!(f, "udp"),
            TransportKind::ReverseTcp => write!(f, "reverse_tcp"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UpstreamGroupId(pub Arc<str>);

impl std::fmt::Display for UpstreamGroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpstreamExplanation {
    pub id: String,
    pub health: String,
    pub eligible: bool,
    pub active: u64,
    pub in_flight: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouteExplanation {
    pub target: String,
    pub listener: String,
    pub protocol: String,
    pub transport: String,
    pub matched_rule: Option<String>,
    pub action: String,
    pub upstream_group: Option<String>,
    pub scheduler: Option<String>,
    pub eligible_upstreams: Vec<UpstreamExplanation>,
    pub selected_upstream: Option<String>,
    pub chain: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleId(pub Arc<str>);

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub(crate) static DEFAULT_RULE_ID: std::sync::LazyLock<RuleId> =
    std::sync::LazyLock::new(|| RuleId(Arc::from("default")));

#[derive(Debug, Clone)]
pub enum RouteActionSpec {
    Direct,
    UpstreamGroup(UpstreamGroupId),
    Reject(RejectReason),
}

pub struct RouteRequest<'a> {
    pub target: &'a TargetAddr,
    pub source: Option<std::net::SocketAddr>,
    pub listener: &'a str,
    pub inbound_protocol: ProtocolId,
    pub identity: &'a ClientIdentity,
    pub transport: TransportKind,
}

#[derive(Debug, Clone)]
pub enum RouteDecision {
    Direct {
        rule: RuleId,
    },
    UpstreamGroup {
        rule: RuleId,
        group: UpstreamGroupId,
    },
    Reject {
        rule: RuleId,
        reason: RejectReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SelectionReason {
    Normal,
    DirectFallback,
    UnhealthyFallback,
}

impl std::fmt::Display for SelectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectionReason::Normal => write!(f, "normal"),
            SelectionReason::DirectFallback => write!(f, "direct-fallback"),
            SelectionReason::UnhealthyFallback => write!(f, "unhealthy-fallback"),
        }
    }
}

pub enum SelectedRoute {
    Direct {
        decision: RouteDecision,
        selection_reason: SelectionReason,
    },
    Upstream {
        decision: RouteDecision,
        group: UpstreamGroupId,
        upstream: eggress_core::UpstreamId,
        chain: std::sync::Arc<eggress_uri::ProxyChainSpec>,
        pending_lease: crate::lease::PendingLease,
        selection_reason: SelectionReason,
    },
}

impl std::fmt::Debug for SelectedRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectedRoute::Direct {
                decision,
                selection_reason,
            } => f
                .debug_struct("SelectedRoute::Direct")
                .field("decision", decision)
                .field("selection_reason", selection_reason)
                .finish(),
            SelectedRoute::Upstream {
                decision,
                group,
                upstream,
                chain,
                selection_reason,
                ..
            } => f
                .debug_struct("SelectedRoute::Upstream")
                .field("decision", decision)
                .field("group", group)
                .field("upstream", upstream)
                .field("chain", chain)
                .field("selection_reason", selection_reason)
                .finish(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("route rejected by policy: {reason}")]
    Rejected { rule: RuleId, reason: RejectReason },
    #[error("no eligible upstream for group {0}")]
    NoEligibleUpstream(UpstreamGroupId),
    #[error("unknown upstream group: {0}")]
    UnknownGroup(UpstreamGroupId),
}

pub trait RouteService: Send + Sync {
    /// Decide and select from one routing snapshot.
    fn route(&self, request: &RouteRequest<'_>) -> Result<SelectedRoute, RouteError>;
}
