use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use eggress_core::{ClientIdentity, ProtocolId, RejectReason, TargetAddr, TargetHost};

pub mod health;
pub mod lease;
pub mod scheduler;
pub mod upstream;

#[derive(Debug, thiserror::Error)]
pub enum RegexError {
    #[error("invalid regex at line {line}: {source}")]
    InvalidRegex { line: usize, source: regex::Error },
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

#[derive(Debug, Clone)]
pub enum RouteActionSpec {
    Direct,
    UpstreamGroup(UpstreamGroupId),
    Reject(RejectReason),
}

#[derive(Debug, Clone)]
pub enum PortMatcher {
    Exact(u16),
    Range { start: u16, end: u16 },
    Set(Arc<[u16]>),
}

impl PortMatcher {
    pub fn matches(&self, port: u16) -> bool {
        match self {
            PortMatcher::Exact(p) => port == *p,
            PortMatcher::Range { start, end } => port >= *start && port <= *end,
            PortMatcher::Set(ports) => ports.contains(&port),
        }
    }
}

#[derive(Debug, Clone)]
pub enum MatchExpr {
    Any,
    All(Vec<MatchExpr>),
    AnyOf(Vec<MatchExpr>),
    Not(Box<MatchExpr>),
    HostExact(Arc<str>),
    HostSuffix(Arc<str>),
    HostRegex(regex::Regex),
    DestinationCidr(ipnet::IpNet),
    DestinationPort(PortMatcher),
    SourceCidr(ipnet::IpNet),
    Listener(Arc<str>),
    Protocol(ProtocolId),
    Identity(Arc<str>),
}

fn normalize_host_for_exact(host: &str) -> String {
    let h = host.strip_suffix('.').unwrap_or(host);
    if h.parse::<IpAddr>().is_ok() {
        h.to_string()
    } else {
        h.to_ascii_lowercase()
    }
}

impl MatchExpr {
    pub fn matches(&self, request: &RouteRequest<'_>) -> bool {
        match self {
            MatchExpr::Any => true,
            MatchExpr::All(exprs) => exprs.iter().all(|e| e.matches(request)),
            MatchExpr::AnyOf(exprs) => exprs.iter().any(|e| e.matches(request)),
            MatchExpr::Not(inner) => !inner.matches(request),
            MatchExpr::HostExact(expected) => {
                let host_str = request.target.host.to_string();
                let normalized = normalize_host_for_exact(&host_str);
                let expected_norm = normalize_host_for_exact(expected);
                normalized == expected_norm
            }
            MatchExpr::HostSuffix(suffix) => {
                let host_str = request.target.host.to_string();
                let host_lower = host_str
                    .strip_suffix('.')
                    .unwrap_or(&host_str)
                    .to_ascii_lowercase();
                let suffix_clean = suffix
                    .strip_suffix('.')
                    .unwrap_or(suffix)
                    .to_ascii_lowercase();
                let suffix_with_dot = format!(".{}", suffix_clean);
                host_lower == suffix_clean || host_lower.ends_with(&suffix_with_dot)
            }
            MatchExpr::HostRegex(re) => {
                let host_str = request.target.host.to_string();
                re.is_match(&host_str)
            }
            MatchExpr::DestinationCidr(cidr) => {
                if let TargetHost::Ip(ip) = &request.target.host {
                    cidr.contains(ip)
                } else {
                    false
                }
            }
            MatchExpr::DestinationPort(matcher) => matcher.matches(request.target.port),
            MatchExpr::SourceCidr(cidr) => {
                if let Some(addr) = request.source {
                    cidr.contains(&addr.ip())
                } else {
                    false
                }
            }
            MatchExpr::Listener(name) => request.listener == name.as_ref(),
            MatchExpr::Protocol(proto) => request.inbound_protocol == *proto,
            MatchExpr::Identity(name) => match &request.identity {
                ClientIdentity::Anonymous => false,
                ClientIdentity::Username(u) => u == name.as_ref(),
                ClientIdentity::Opaque(o) => o == name.as_ref(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub id: RuleId,
    pub matcher: MatchExpr,
    pub action: RouteActionSpec,
}

pub struct RouteRequest<'a> {
    pub target: &'a TargetAddr,
    pub source: Option<SocketAddr>,
    pub listener: &'a str,
    pub inbound_protocol: ProtocolId,
    pub identity: &'a ClientIdentity,
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

pub struct Router {
    rules: Vec<CompiledRule>,
    default_action: RouteActionSpec,
    groups: std::collections::HashMap<UpstreamGroupId, std::sync::Arc<upstream::UpstreamGroup>>,
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
        groups: Vec<(UpstreamGroupId, upstream::UpstreamGroup)>,
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
                rule: RuleId(Arc::from("default")),
            },
            RouteActionSpec::UpstreamGroup(group) => RouteDecision::UpstreamGroup {
                rule: RuleId(Arc::from("default")),
                group: group.clone(),
            },
            RouteActionSpec::Reject(reason) => RouteDecision::Reject {
                rule: RuleId(Arc::from("default")),
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

    pub fn explain(&self, request: &RouteRequest, generation: u64) -> RouteExplanation {
        let decision = self.decide(request);
        let target = request.target.to_string();
        let listener = request.listener.to_string();
        let protocol = request.inbound_protocol.to_string();

        let (matched_rule, action, upstream_group, scheduler, eligible, selected, chain) =
            match &decision {
                RouteDecision::Direct { rule } => (
                    Some(rule.to_string()),
                    "direct".to_string(),
                    None,
                    None,
                    vec![],
                    None,
                    None,
                ),
                RouteDecision::Reject { rule, reason } => (
                    Some(rule.to_string()),
                    format!("reject({})", reason),
                    None,
                    None,
                    vec![],
                    None,
                    None,
                ),
                RouteDecision::UpstreamGroup { rule, group } => {
                    let group_arc = self.groups.get(group);
                    let group_id = group.to_string();

                    if let Some(upstream_group) = group_arc {
                        let sched_name = match upstream_group.scheduler {
                            scheduler::SchedulerKind::FirstAvailable => "first-available",
                            scheduler::SchedulerKind::RoundRobin => "round-robin",
                            scheduler::SchedulerKind::Random => "random",
                            scheduler::SchedulerKind::LeastConnections => "least-connections",
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
                            let scheduler_inst: Box<dyn scheduler::Scheduler> =
                                match upstream_group.scheduler {
                                    scheduler::SchedulerKind::FirstAvailable => {
                                        Box::new(scheduler::FirstAvailableScheduler)
                                    }
                                    scheduler::SchedulerKind::RoundRobin => {
                                        Box::new(scheduler::RoundRobinScheduler::new())
                                    }
                                    scheduler::SchedulerKind::Random => {
                                        Box::new(scheduler::RandomScheduler)
                                    }
                                    scheduler::SchedulerKind::LeastConnections => {
                                        Box::new(scheduler::LeastConnectionsScheduler)
                                    }
                                };
                            if let Some(sel) =
                                scheduler_inst.select(upstream_group, &candidates, request)
                            {
                                let chain_str =
                                    format!("{}", eggress_uri::RedactedUri::new(&sel.chain));
                                (Some(sel.id.to_string()), Some(chain_str))
                            } else {
                                (None, None)
                            }
                        } else {
                            (None, None)
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

pub enum SelectedRoute {
    Direct {
        decision: RouteDecision,
    },
    Upstream {
        decision: RouteDecision,
        group: UpstreamGroupId,
        upstream: eggress_core::UpstreamId,
        chain: std::sync::Arc<eggress_uri::ProxyChainSpec>,
        lease: lease::ActiveLease,
    },
}

pub trait RouteService: Send + Sync {
    fn decide(&self, request: &RouteRequest<'_>) -> RouteDecision;
    fn select(
        &self,
        decision: &RouteDecision,
        request: &RouteRequest<'_>,
    ) -> Result<SelectedRoute, RouteError>;
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

impl RouteService for Router {
    fn decide(&self, request: &RouteRequest<'_>) -> RouteDecision {
        self.decide(request)
    }

    fn select(
        &self,
        decision: &RouteDecision,
        request: &RouteRequest<'_>,
    ) -> Result<SelectedRoute, RouteError> {
        match decision {
            RouteDecision::Direct { rule: _ } => Ok(SelectedRoute::Direct {
                decision: decision.clone(),
            }),
            RouteDecision::Reject { rule, reason } => Err(RouteError::Rejected {
                rule: rule.clone(),
                reason: reason.clone(),
            }),
            RouteDecision::UpstreamGroup { rule: _, group } => {
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

                if candidates.is_empty() {
                    return Err(RouteError::NoEligibleUpstream(group.clone()));
                }

                use crate::scheduler::Scheduler;
                let scheduler: Box<dyn Scheduler> = match upstream_group.scheduler {
                    scheduler::SchedulerKind::FirstAvailable => {
                        Box::new(scheduler::FirstAvailableScheduler)
                    }
                    scheduler::SchedulerKind::RoundRobin => {
                        Box::new(scheduler::RoundRobinScheduler::new())
                    }
                    scheduler::SchedulerKind::Random => Box::new(scheduler::RandomScheduler),
                    scheduler::SchedulerKind::LeastConnections => {
                        Box::new(scheduler::LeastConnectionsScheduler)
                    }
                };

                let selected = scheduler
                    .select(upstream_group, &candidates, request)
                    .ok_or_else(|| RouteError::NoEligibleUpstream(group.clone()))?;

                let pending = lease::PendingLease::new(selected.clone());
                let lease = pending.established();

                Ok(SelectedRoute::Upstream {
                    decision: decision.clone(),
                    group: group.clone(),
                    upstream: selected.id,
                    chain: selected.chain.clone(),
                    lease,
                })
            }
        }
    }
}

pub struct RoutingServiceInner {
    pub router: std::sync::Arc<Router>,
    pub generation: std::sync::atomic::AtomicU64,
}

pub struct SharedRoutingService {
    inner: arc_swap::ArcSwap<RoutingServiceInner>,
}

impl SharedRoutingService {
    pub fn new(router: Router) -> Self {
        Self {
            inner: arc_swap::ArcSwap::from_pointee(RoutingServiceInner {
                router: std::sync::Arc::new(router),
                generation: std::sync::atomic::AtomicU64::new(0),
            }),
        }
    }

    pub fn generation(&self) -> u64 {
        self.inner
            .load()
            .generation
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn swap(&self, router: Router) {
        let gen = self
            .inner
            .load()
            .generation
            .load(std::sync::atomic::Ordering::Relaxed)
            + 1;
        let new_inner = RoutingServiceInner {
            router: std::sync::Arc::new(router),
            generation: std::sync::atomic::AtomicU64::new(gen),
        };
        self.inner.store(std::sync::Arc::new(new_inner));
    }
}

impl RouteService for SharedRoutingService {
    fn decide(&self, request: &RouteRequest<'_>) -> RouteDecision {
        self.inner.load().router.decide(request)
    }

    fn select(
        &self,
        decision: &RouteDecision,
        request: &RouteRequest<'_>,
    ) -> Result<SelectedRoute, RouteError> {
        self.inner.load().router.select(decision, request)
    }
}

#[derive(Debug)]
pub struct CompatRegexRule {
    pub pattern: regex::Regex,
}

impl CompatRegexRule {
    pub fn parse_line(line: &str) -> Result<Option<Self>, RegexError> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return Ok(None);
        }
        let re = regex::Regex::new(trimmed)
            .map_err(|e| RegexError::InvalidRegex { line: 0, source: e })?;
        Ok(Some(Self { pattern: re }))
    }

    pub fn parse_file(content: &str) -> Result<Vec<Self>, RegexError> {
        let mut rules = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let re = regex::Regex::new(trimmed).map_err(|e| RegexError::InvalidRegex {
                line: idx + 1,
                source: e,
            })?;
            rules.push(Self { pattern: re });
        }
        Ok(rules)
    }

    pub fn matches(&self, hostname: &str, port: u16) -> bool {
        let target = format!("{}:{}", hostname, port);
        self.pattern.is_match(&target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

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
        }
    }

    const ANON: ClientIdentity = ClientIdentity::Anonymous;

    #[test]
    fn host_exact_case_insensitive() {
        let target = target_domain("Example.COM", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::HostExact(Arc::from("example.com")).matches(&req));
    }

    #[test]
    fn host_exact_strips_trailing_dot() {
        let target = target_domain("example.com.", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::HostExact(Arc::from("example.com")).matches(&req));
    }

    #[test]
    fn host_exact_ip_literal_no_lowercasing() {
        let target = target_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 443);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::HostExact(Arc::from("192.168.1.1")).matches(&req));
    }

    #[test]
    fn host_exact_no_match() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::HostExact(Arc::from("other.com")).matches(&req));
    }

    #[test]
    fn host_suffix_exact_match() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::HostSuffix(Arc::from("example.com")).matches(&req));
    }

    #[test]
    fn host_suffix_subdomain() {
        let target = target_domain("www.example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::HostSuffix(Arc::from("example.com")).matches(&req));
    }

    #[test]
    fn host_suffix_no_match() {
        let target = target_domain("notexample.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::HostSuffix(Arc::from("example.com")).matches(&req));
    }

    #[test]
    fn host_suffix_with_trailing_dot() {
        let target = target_domain("www.example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::HostSuffix(Arc::from("example.com.")).matches(&req));
    }

    #[test]
    fn host_suffix_partial_word_no_match() {
        let target = target_domain("notexample.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::HostSuffix(Arc::from("ample.com")).matches(&req));
    }

    #[test]
    fn host_regex_match() {
        let re = regex::Regex::new(r"^www\d+\.example\.com$").unwrap();
        let target = target_domain("www3.example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::HostRegex(re).matches(&req));
    }

    #[test]
    fn host_regex_no_match() {
        let re = regex::Regex::new(r"^www\d+\.example\.com$").unwrap();
        let target = target_domain("www.example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::HostRegex(re).matches(&req));
    }

    #[test]
    fn ipv4_cidr_match() {
        let cidr: ipnet::IpNet = "10.0.0.0/8".parse().unwrap();
        let target = target_ip(IpAddr::V4(Ipv4Addr::new(10, 42, 1, 1)), 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::DestinationCidr(cidr).matches(&req));
    }

    #[test]
    fn ipv4_cidr_no_match() {
        let cidr: ipnet::IpNet = "10.0.0.0/8".parse().unwrap();
        let target = target_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::DestinationCidr(cidr).matches(&req));
    }

    #[test]
    fn ipv6_cidr_match() {
        let cidr: ipnet::IpNet = "fe80::/10".parse().unwrap();
        let target = target_ip(IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)), 443);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::DestinationCidr(cidr).matches(&req));
    }

    #[test]
    fn ipv6_cidr_no_match() {
        let cidr: ipnet::IpNet = "fe80::/10".parse().unwrap();
        let target = target_ip(IpAddr::V6(Ipv6Addr::LOCALHOST), 443);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::DestinationCidr(cidr).matches(&req));
    }

    #[test]
    fn cidr_no_match_on_domain() {
        let cidr: ipnet::IpNet = "10.0.0.0/8".parse().unwrap();
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::DestinationCidr(cidr).matches(&req));
    }

    #[test]
    fn port_exact_match() {
        let target = target_domain("example.com", 443);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::DestinationPort(PortMatcher::Exact(443)).matches(&req));
    }

    #[test]
    fn port_exact_no_match() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::DestinationPort(PortMatcher::Exact(443)).matches(&req));
    }

    #[test]
    fn port_range_match() {
        let target = target_domain("example.com", 8080);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::DestinationPort(PortMatcher::Range {
            start: 8000,
            end: 9000,
        })
        .matches(&req));
    }

    #[test]
    fn port_range_boundary_start() {
        let target = target_domain("example.com", 8000);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::DestinationPort(PortMatcher::Range {
            start: 8000,
            end: 9000,
        })
        .matches(&req));
    }

    #[test]
    fn port_range_boundary_end() {
        let target = target_domain("example.com", 9000);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::DestinationPort(PortMatcher::Range {
            start: 8000,
            end: 9000,
        })
        .matches(&req));
    }

    #[test]
    fn port_range_no_match_below() {
        let target = target_domain("example.com", 7999);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::DestinationPort(PortMatcher::Range {
            start: 8000,
            end: 9000,
        })
        .matches(&req));
    }

    #[test]
    fn port_range_no_match_above() {
        let target = target_domain("example.com", 9001);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::DestinationPort(PortMatcher::Range {
            start: 8000,
            end: 9000,
        })
        .matches(&req));
    }

    #[test]
    fn port_set_match() {
        let ports: Arc<[u16]> = Arc::from([80, 443, 8080]);
        let target = target_domain("example.com", 443);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::DestinationPort(PortMatcher::Set(ports)).matches(&req));
    }

    #[test]
    fn port_set_no_match() {
        let ports: Arc<[u16]> = Arc::from([80, 443, 8080]);
        let target = target_domain("example.com", 9999);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::DestinationPort(PortMatcher::Set(ports)).matches(&req));
    }

    #[test]
    fn source_cidr_match() {
        let cidr: ipnet::IpNet = "192.168.0.0/16".parse().unwrap();
        let source = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 12345);
        let target = target_domain("example.com", 80);
        let req = make_request(&target, Some(source), "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::SourceCidr(cidr).matches(&req));
    }

    #[test]
    fn source_cidr_no_match() {
        let cidr: ipnet::IpNet = "192.168.0.0/16".parse().unwrap();
        let source = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 12345);
        let target = target_domain("example.com", 80);
        let req = make_request(&target, Some(source), "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::SourceCidr(cidr).matches(&req));
    }

    #[test]
    fn source_cidr_no_source() {
        let cidr: ipnet::IpNet = "192.168.0.0/16".parse().unwrap();
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::SourceCidr(cidr).matches(&req));
    }

    #[test]
    fn listener_match() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "http-in", ProtocolId::Http, &ANON);
        assert!(MatchExpr::Listener(Arc::from("http-in")).matches(&req));
    }

    #[test]
    fn listener_no_match() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "http-in", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::Listener(Arc::from("socks-in")).matches(&req));
    }

    #[test]
    fn protocol_match() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Socks5, &ANON);
        assert!(MatchExpr::Protocol(ProtocolId::Socks5).matches(&req));
    }

    #[test]
    fn protocol_no_match() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::Protocol(ProtocolId::Socks5).matches(&req));
    }

    #[test]
    fn identity_match_username() {
        let target = target_domain("example.com", 80);
        let ident = ClientIdentity::Username("alice".to_string());
        let req = make_request(&target, None, "l", ProtocolId::Http, &ident);
        assert!(MatchExpr::Identity(Arc::from("alice")).matches(&req));
    }

    #[test]
    fn identity_no_match_anonymous() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::Identity(Arc::from("alice")).matches(&req));
    }

    #[test]
    fn identity_match_opaque() {
        let target = target_domain("example.com", 80);
        let ident = ClientIdentity::Opaque("token123".to_string());
        let req = make_request(&target, None, "l", ProtocolId::Http, &ident);
        assert!(MatchExpr::Identity(Arc::from("token123")).matches(&req));
    }

    #[test]
    fn any_always_matches() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::Any.matches(&req));
    }

    #[test]
    fn all_requires_all_match() {
        let target = target_domain("example.com", 443);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let expr = MatchExpr::All(vec![
            MatchExpr::HostExact(Arc::from("example.com")),
            MatchExpr::DestinationPort(PortMatcher::Exact(443)),
        ]);
        assert!(expr.matches(&req));
    }

    #[test]
    fn all_fails_if_one_fails() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let expr = MatchExpr::All(vec![
            MatchExpr::HostExact(Arc::from("example.com")),
            MatchExpr::DestinationPort(PortMatcher::Exact(443)),
        ]);
        assert!(!expr.matches(&req));
    }

    #[test]
    fn any_of_requires_one_match() {
        let target = target_domain("other.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let expr = MatchExpr::AnyOf(vec![
            MatchExpr::HostExact(Arc::from("example.com")),
            MatchExpr::DestinationPort(PortMatcher::Exact(80)),
        ]);
        assert!(expr.matches(&req));
    }

    #[test]
    fn any_of_fails_if_none_match() {
        let target = target_domain("other.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let expr = MatchExpr::AnyOf(vec![
            MatchExpr::HostExact(Arc::from("example.com")),
            MatchExpr::DestinationPort(PortMatcher::Exact(443)),
        ]);
        assert!(!expr.matches(&req));
    }

    #[test]
    fn not_negates() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let expr = MatchExpr::Not(Box::new(MatchExpr::HostExact(Arc::from("other.com"))));
        assert!(expr.matches(&req));
    }

    #[test]
    fn not_negates_false_to_true() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let expr = MatchExpr::Not(Box::new(MatchExpr::HostExact(Arc::from("example.com"))));
        assert!(!expr.matches(&req));
    }

    #[test]
    fn first_match_wins() {
        let rules = vec![
            CompiledRule {
                id: RuleId(Arc::from("r1")),
                matcher: MatchExpr::Any,
                action: RouteActionSpec::Direct,
            },
            CompiledRule {
                id: RuleId(Arc::from("r2")),
                matcher: MatchExpr::Any,
                action: RouteActionSpec::Reject(RejectReason::Blocked),
            },
        ];
        let router = Router::new(rules, RouteActionSpec::Direct);
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        match decision {
            RouteDecision::Direct { rule } => assert_eq!(rule.0.as_ref(), "r1"),
            _ => panic!("expected Direct"),
        }
    }

    #[test]
    fn default_action_when_no_match() {
        let router = Router::new(vec![], RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        match decision {
            RouteDecision::Reject { rule, reason } => {
                assert_eq!(rule.0.as_ref(), "default");
                assert_eq!(reason, RejectReason::Blocked);
            }
            _ => panic!("expected Reject"),
        }
    }

    #[test]
    fn upstream_group_action() {
        let group = UpstreamGroupId(Arc::from("my-proxy"));
        let rules = vec![CompiledRule {
            id: RuleId(Arc::from("r1")),
            matcher: MatchExpr::HostSuffix(Arc::from("corp.internal")),
            action: RouteActionSpec::UpstreamGroup(group.clone()),
        }];
        let router = Router::new(rules, RouteActionSpec::Direct);
        let target = target_domain("app.corp.internal", 443);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        match decision {
            RouteDecision::UpstreamGroup { rule, group: g } => {
                assert_eq!(rule.0.as_ref(), "r1");
                assert_eq!(g, group);
            }
            _ => panic!("expected UpstreamGroup"),
        }
    }

    #[test]
    fn reject_action() {
        let rules = vec![CompiledRule {
            id: RuleId(Arc::from("block")),
            matcher: MatchExpr::HostSuffix(Arc::from("blocked.com")),
            action: RouteActionSpec::Reject(RejectReason::AccessDenied),
        }];
        let router = Router::new(rules, RouteActionSpec::Direct);
        let target = target_domain("evil.blocked.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let decision = router.decide(&req);
        match decision {
            RouteDecision::Reject { rule, reason } => {
                assert_eq!(rule.0.as_ref(), "block");
                assert_eq!(reason, RejectReason::AccessDenied);
            }
            _ => panic!("expected Reject"),
        }
    }

    #[test]
    fn router_accessors() {
        let rules = vec![CompiledRule {
            id: RuleId(Arc::from("r1")),
            matcher: MatchExpr::Any,
            action: RouteActionSpec::Direct,
        }];
        let router = Router::new(rules, RouteActionSpec::Direct);
        assert_eq!(router.rules().len(), 1);
        assert!(matches!(router.default_action(), RouteActionSpec::Direct));
    }

    #[test]
    fn compat_regex_skip_empty_and_comments() {
        let result = CompatRegexRule::parse_line("").unwrap();
        assert!(result.is_none());
        let result = CompatRegexRule::parse_line("# comment").unwrap();
        assert!(result.is_none());
        let result = CompatRegexRule::parse_line("  ").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn compat_regex_parse_valid() {
        let rule = CompatRegexRule::parse_line(r"^example\.com:\d+$")
            .unwrap()
            .unwrap();
        assert!(rule.matches("example.com", 443));
        assert!(!rule.matches("other.com", 443));
    }

    #[test]
    fn compat_regex_invalid_returns_error() {
        let err = CompatRegexRule::parse_line(r"[invalid").unwrap_err();
        assert!(matches!(err, RegexError::InvalidRegex { .. }));
    }

    #[test]
    fn compat_regex_parse_file() {
        let content = "# comment\n\n^example\\.com:443$\n^other\\.com:80$\n";
        let rules = CompatRegexRule::parse_file(content).unwrap();
        assert_eq!(rules.len(), 2);
        assert!(rules[0].matches("example.com", 443));
        assert!(rules[1].matches("other.com", 80));
    }

    #[test]
    fn compat_regex_parse_file_error_line_number() {
        let content = "valid\n[bad\n";
        match CompatRegexRule::parse_file(content) {
            Err(RegexError::InvalidRegex { line, .. }) => assert_eq!(line, 2),
            _ => panic!("expected error"),
        }
    }

    #[test]
    fn compat_regex_matches_hostname_port() {
        let rule = CompatRegexRule::parse_line(r".*:80").unwrap().unwrap();
        assert!(rule.matches("anything.com", 80));
        assert!(!rule.matches("anything.com", 443));
    }

    #[test]
    fn nested_not_all_any_of() {
        let target = target_domain("www.example.com", 443);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        let expr = MatchExpr::All(vec![
            MatchExpr::HostSuffix(Arc::from("example.com")),
            MatchExpr::Not(Box::new(MatchExpr::DestinationPort(PortMatcher::Exact(80)))),
            MatchExpr::AnyOf(vec![
                MatchExpr::DestinationPort(PortMatcher::Exact(443)),
                MatchExpr::DestinationPort(PortMatcher::Exact(8443)),
            ]),
        ]);
        assert!(expr.matches(&req));
    }

    #[test]
    fn empty_all_matches() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(MatchExpr::All(vec![]).matches(&req));
    }

    #[test]
    fn empty_any_of_no_match() {
        let target = target_domain("example.com", 80);
        let req = make_request(&target, None, "l", ProtocolId::Http, &ANON);
        assert!(!MatchExpr::AnyOf(vec![]).matches(&req));
    }

    use crate::lease::PendingLease;
    use crate::scheduler::{
        FirstAvailableScheduler, LeastConnectionsScheduler, RandomScheduler, RoundRobinScheduler,
        Scheduler, SchedulerKind,
    };
    use crate::upstream::{validate_group, GroupFallback, UpstreamGroup, UpstreamRuntime};
    use eggress_core::UpstreamId;
    use eggress_uri::ProxyChainSpec;
    use std::sync::atomic::Ordering;

    fn make_upstream(id: UpstreamId) -> Arc<UpstreamRuntime> {
        Arc::new(UpstreamRuntime::new(id, ProxyChainSpec { hops: vec![] }))
    }

    fn make_group(
        members: Vec<Arc<UpstreamRuntime>>,
        scheduler: crate::scheduler::SchedulerKind,
    ) -> UpstreamGroup {
        UpstreamGroup {
            id: UpstreamGroupId(Arc::from("test-group")),
            scheduler,
            members: Arc::from(members),
            fallback: GroupFallback::Reject,
        }
    }

    fn dummy_request<'a>(target: &'a TargetAddr) -> RouteRequest<'a> {
        RouteRequest {
            target,
            source: None,
            listener: "test",
            inbound_protocol: ProtocolId::Http,
            identity: &ClientIdentity::Anonymous,
        }
    }

    // --- UpstreamRuntime tests ---

    #[test]
    fn upstream_runtime_load_tracking() {
        let u = make_upstream(1);
        assert_eq!(u.current_load(), 0);
        u.active.fetch_add(5, Ordering::Relaxed);
        assert_eq!(u.current_load(), 5);
        u.in_flight.fetch_add(3, Ordering::Relaxed);
        assert_eq!(u.current_load(), 8);
        u.active.fetch_sub(2, Ordering::Relaxed);
        assert_eq!(u.current_load(), 6);
    }

    #[test]
    fn upstream_runtime_enabled_default() {
        let u = make_upstream(1);
        assert!(u.is_enabled());
        u.set_enabled(false);
        assert!(!u.is_enabled());
        u.set_enabled(true);
        assert!(u.is_enabled());
    }

    // --- PendingLease tests ---

    #[test]
    fn pending_lease_decrements_in_flight_on_drop() {
        let u = make_upstream(1);
        {
            let _lease = PendingLease::new(u.clone());
            assert_eq!(u.in_flight.load(Ordering::Relaxed), 1);
        }
        assert_eq!(u.in_flight.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn pending_lease_established_converts_to_active() {
        let u = make_upstream(1);
        let pending = PendingLease::new(u.clone());
        assert_eq!(u.in_flight.load(Ordering::Relaxed), 1);
        assert_eq!(u.active.load(Ordering::Relaxed), 0);

        let active = pending.established();
        assert_eq!(u.in_flight.load(Ordering::Relaxed), 0);
        assert_eq!(u.active.load(Ordering::Relaxed), 1);
        assert_eq!(active.upstream().id, 1);
    }

    #[test]
    fn active_lease_decrements_active_on_drop() {
        let u = make_upstream(1);
        let pending = PendingLease::new(u.clone());
        let active = pending.established();
        assert_eq!(u.active.load(Ordering::Relaxed), 1);
        drop(active);
        assert_eq!(u.active.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn pending_lease_upstream_accessor() {
        let u = make_upstream(42);
        let lease = PendingLease::new(u.clone());
        assert_eq!(lease.upstream().id, 42);
    }

    // --- FirstAvailableScheduler tests ---

    #[test]
    fn first_available_preserves_order() {
        let u1 = make_upstream(1);
        let u2 = make_upstream(2);
        let u3 = make_upstream(3);
        let group = make_group(
            vec![u1.clone(), u2.clone(), u3.clone()],
            SchedulerKind::FirstAvailable,
        );
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let scheduler = FirstAvailableScheduler;
        let selected = scheduler.select(&group, &group.members, &req).unwrap();
        assert_eq!(selected.id, 1);
    }

    #[test]
    fn first_available_skips_disabled() {
        let u1 = make_upstream(1);
        u1.set_enabled(false);
        let u2 = make_upstream(2);
        let u3 = make_upstream(3);
        let group = make_group(
            vec![u1.clone(), u2.clone(), u3.clone()],
            SchedulerKind::FirstAvailable,
        );
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let scheduler = FirstAvailableScheduler;
        let selected = scheduler.select(&group, &group.members, &req).unwrap();
        assert_eq!(selected.id, 2);
    }

    // --- RoundRobinScheduler tests ---

    #[test]
    fn round_robin_cycles_deterministically() {
        let u1 = make_upstream(1);
        let u2 = make_upstream(2);
        let u3 = make_upstream(3);
        let group = make_group(
            vec![u1.clone(), u2.clone(), u3.clone()],
            SchedulerKind::RoundRobin,
        );
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let scheduler = RoundRobinScheduler::new();

        assert_eq!(
            scheduler.select(&group, &group.members, &req).unwrap().id,
            1
        );
        assert_eq!(
            scheduler.select(&group, &group.members, &req).unwrap().id,
            2
        );
        assert_eq!(
            scheduler.select(&group, &group.members, &req).unwrap().id,
            3
        );
        assert_eq!(
            scheduler.select(&group, &group.members, &req).unwrap().id,
            1
        );
    }

    #[test]
    fn round_robin_skips_disabled() {
        let u1 = make_upstream(1);
        u1.set_enabled(false);
        let u2 = make_upstream(2);
        let u3 = make_upstream(3);
        let group = make_group(
            vec![u1.clone(), u2.clone(), u3.clone()],
            SchedulerKind::RoundRobin,
        );
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let scheduler = RoundRobinScheduler::new();

        // cursor 0: checks idx 0 (disabled), idx 1 (u2) -> returns u2
        assert_eq!(
            scheduler.select(&group, &group.members, &req).unwrap().id,
            2
        );
        // cursor 1: checks idx 1 (u2) -> returns u2
        assert_eq!(
            scheduler.select(&group, &group.members, &req).unwrap().id,
            2
        );
        // cursor 2: checks idx 2 (u3) -> returns u3
        assert_eq!(
            scheduler.select(&group, &group.members, &req).unwrap().id,
            3
        );
        // cursor 3: wraps to idx 0 (disabled), idx 1 (u2) -> returns u2
        assert_eq!(
            scheduler.select(&group, &group.members, &req).unwrap().id,
            2
        );
    }

    #[test]
    fn round_robin_empty_returns_none() {
        let group = make_group(vec![], SchedulerKind::RoundRobin);
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let scheduler = RoundRobinScheduler::new();
        assert!(scheduler.select(&group, &group.members, &req).is_none());
    }

    // --- RandomScheduler tests ---

    #[test]
    fn random_selects_enabled_member() {
        let u1 = make_upstream(1);
        u1.set_enabled(false);
        let u2 = make_upstream(2);
        let u3 = make_upstream(3);
        let group = make_group(
            vec![u1.clone(), u2.clone(), u3.clone()],
            SchedulerKind::Random,
        );
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let scheduler = RandomScheduler;

        for _ in 0..100 {
            let selected = scheduler.select(&group, &group.members, &req).unwrap();
            assert!(selected.is_enabled());
        }
    }

    #[test]
    fn random_empty_returns_none() {
        let group = make_group(vec![], SchedulerKind::Random);
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let scheduler = RandomScheduler;
        assert!(scheduler.select(&group, &group.members, &req).is_none());
    }

    // --- LeastConnectionsScheduler tests ---

    #[test]
    fn least_connections_picks_minimum_load() {
        let u1 = make_upstream(1);
        let u2 = make_upstream(2);
        let u3 = make_upstream(3);
        u2.active.fetch_add(5, Ordering::Relaxed);
        u3.active.fetch_add(10, Ordering::Relaxed);

        let group = make_group(
            vec![u1.clone(), u2.clone(), u3.clone()],
            SchedulerKind::LeastConnections,
        );
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let scheduler = LeastConnectionsScheduler;
        let selected = scheduler.select(&group, &group.members, &req).unwrap();
        assert_eq!(selected.id, 1);
    }

    #[test]
    fn least_connections_tie_breaking_deterministic() {
        let u1 = make_upstream(1);
        let u2 = make_upstream(2);
        u1.active.fetch_add(5, Ordering::Relaxed);
        u2.active.fetch_add(5, Ordering::Relaxed);

        let group = make_group(
            vec![u1.clone(), u2.clone()],
            SchedulerKind::LeastConnections,
        );
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let scheduler = LeastConnectionsScheduler;

        // min_by_key is stable, so earlier member wins on tie
        let selected = scheduler.select(&group, &group.members, &req).unwrap();
        assert_eq!(selected.id, 1);
    }

    #[test]
    fn least_connections_skips_disabled() {
        let u1 = make_upstream(1);
        u1.set_enabled(false);
        let u2 = make_upstream(2);
        u2.active.fetch_add(5, Ordering::Relaxed);
        let u3 = make_upstream(3);

        let group = make_group(
            vec![u1.clone(), u2.clone(), u3.clone()],
            SchedulerKind::LeastConnections,
        );
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let scheduler = LeastConnectionsScheduler;
        let selected = scheduler.select(&group, &group.members, &req).unwrap();
        assert_eq!(selected.id, 3);
    }

    // --- Group validation tests ---

    #[test]
    fn validate_group_empty() {
        let group = UpstreamGroup {
            id: UpstreamGroupId(Arc::from("g")),
            scheduler: SchedulerKind::FirstAvailable,
            members: Arc::from([]),
            fallback: GroupFallback::Reject,
        };
        assert!(validate_group(&group).is_err());
    }

    #[test]
    fn validate_group_duplicate_ids() {
        let u1 = make_upstream(1);
        let u2 = make_upstream(1);
        let group = UpstreamGroup {
            id: UpstreamGroupId(Arc::from("g")),
            scheduler: SchedulerKind::FirstAvailable,
            members: Arc::from([u1, u2]),
            fallback: GroupFallback::Reject,
        };
        assert!(validate_group(&group).is_err());
    }

    #[test]
    fn validate_group_valid() {
        let u1 = make_upstream(1);
        let u2 = make_upstream(2);
        let group = UpstreamGroup {
            id: UpstreamGroupId(Arc::from("g")),
            scheduler: SchedulerKind::FirstAvailable,
            members: Arc::from([u1, u2]),
            fallback: GroupFallback::Reject,
        };
        assert!(validate_group(&group).is_ok());
    }

    // --- Concurrent lease operations ---

    #[test]
    fn concurrent_leases_no_underflow() {
        let u = make_upstream(1);
        let handles: Vec<_> = (0..100)
            .map(|_| {
                let u = u.clone();
                std::thread::spawn(move || {
                    let pending = PendingLease::new(u.clone());
                    let _active = pending.established();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(u.active.load(Ordering::Relaxed), 0);
        assert_eq!(u.in_flight.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn shared_routing_service_generation_starts_at_zero() {
        let router = Router::new(vec![], RouteActionSpec::Direct);
        let service = super::SharedRoutingService::new(router);
        assert_eq!(service.generation(), 0);
    }

    #[test]
    fn shared_routing_service_swap_increments_generation() {
        let router = Router::new(vec![], RouteActionSpec::Direct);
        let service = super::SharedRoutingService::new(router);
        assert_eq!(service.generation(), 0);

        let router2 = Router::new(vec![], RouteActionSpec::Direct);
        service.swap(router2);
        assert_eq!(service.generation(), 1);

        let router3 = Router::new(vec![], RouteActionSpec::Direct);
        service.swap(router3);
        assert_eq!(service.generation(), 2);
    }

    #[test]
    fn shared_routing_service_swap_atomic_replaces_router() {
        let rule = CompiledRule {
            id: RuleId(Arc::from("match-first")),
            matcher: MatchExpr::HostExact(Arc::from("first.com")),
            action: RouteActionSpec::Direct,
        };
        let router1 = Router::new(vec![rule], RouteActionSpec::Direct);
        let service = super::SharedRoutingService::new(router1);

        let target1 = target_domain("first.com", 80);
        let req1 = dummy_request(&target1);
        let decision1 = service.decide(&req1);
        assert!(matches!(decision1, RouteDecision::Direct { .. }));

        let rule2 = CompiledRule {
            id: RuleId(Arc::from("match-second")),
            matcher: MatchExpr::HostExact(Arc::from("second.com")),
            action: RouteActionSpec::Reject(RejectReason::Blocked),
        };
        let router2 = Router::new(vec![rule2], RouteActionSpec::Direct);
        service.swap(router2);

        let target2 = target_domain("second.com", 80);
        let req2 = dummy_request(&target2);
        let decision2 = service.decide(&req2);
        assert!(matches!(decision2, RouteDecision::Reject { .. }));

        let req3 = dummy_request(&target1);
        let decision3 = service.decide(&req3);
        assert!(matches!(decision3, RouteDecision::Direct { .. }));
    }

    #[test]
    fn shared_routing_service_implements_route_service_trait() {
        let router = Router::new(vec![], RouteActionSpec::Direct);
        let service = super::SharedRoutingService::new(router);

        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let decision = RouteService::decide(&service, &req);
        assert!(matches!(decision, RouteDecision::Direct { .. }));
    }

    #[test]
    fn explain_returns_correct_rule_id() {
        let rule = CompiledRule {
            id: RuleId(Arc::from("test-rule")),
            matcher: MatchExpr::HostExact(Arc::from("example.com")),
            action: RouteActionSpec::Direct,
        };
        let router = Router::new(vec![rule], RouteActionSpec::Direct);
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let explanation = router.explain(&req, 1);
        assert_eq!(explanation.matched_rule.as_deref(), Some("test-rule"));
    }

    #[test]
    fn explain_returns_correct_action_type_direct() {
        let rule = CompiledRule {
            id: RuleId(Arc::from("r1")),
            matcher: MatchExpr::Any,
            action: RouteActionSpec::Direct,
        };
        let router = Router::new(vec![rule], RouteActionSpec::Direct);
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let explanation = router.explain(&req, 0);
        assert_eq!(explanation.action, "direct");
        assert!(explanation.upstream_group.is_none());
        assert!(explanation.scheduler.is_none());
    }

    #[test]
    fn explain_returns_correct_action_type_reject() {
        let rule = CompiledRule {
            id: RuleId(Arc::from("block")),
            matcher: MatchExpr::Any,
            action: RouteActionSpec::Reject(RejectReason::Blocked),
        };
        let router = Router::new(vec![rule], RouteActionSpec::Direct);
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let explanation = router.explain(&req, 0);
        assert!(explanation.action.contains("reject"));
        assert!(explanation.action.contains("blocked"));
    }

    #[test]
    fn explain_lists_eligible_upstreams() {
        let u1 = make_upstream(1);
        let u2 = make_upstream(2);
        u2.set_enabled(false);
        let u3 = make_upstream(3);
        u3.health.observe_failure(
            None,
            &crate::health::HealthConfig {
                failures_to_unhealthy: 1,
                ..Default::default()
            },
        );

        let group_id = UpstreamGroupId(Arc::from("my-group"));
        let group = UpstreamGroup {
            id: group_id.clone(),
            scheduler: SchedulerKind::FirstAvailable,
            members: Arc::from([u1.clone(), u2.clone(), u3.clone()]),
            fallback: GroupFallback::Reject,
        };

        let rule = CompiledRule {
            id: RuleId(Arc::from("proxy-rule")),
            matcher: MatchExpr::Any,
            action: RouteActionSpec::UpstreamGroup(group_id.clone()),
        };
        let router =
            Router::with_groups(vec![rule], RouteActionSpec::Direct, vec![(group_id, group)]);
        let target = target_domain("example.com", 443);
        let req = dummy_request(&target);
        let explanation = router.explain(&req, 5);

        assert_eq!(explanation.eligible_upstreams.len(), 3);
        assert!(explanation.eligible_upstreams[0].eligible);
        assert!(!explanation.eligible_upstreams[1].eligible);
        assert!(!explanation.eligible_upstreams[2].eligible);
    }

    #[test]
    fn explain_reports_selected_upstream() {
        let u1 = make_upstream(1);
        let u2 = make_upstream(2);

        let group_id = UpstreamGroupId(Arc::from("sel-group"));
        let group = UpstreamGroup {
            id: group_id.clone(),
            scheduler: SchedulerKind::FirstAvailable,
            members: Arc::from([u1.clone(), u2.clone()]),
            fallback: GroupFallback::Reject,
        };

        let rule = CompiledRule {
            id: RuleId(Arc::from("r1")),
            matcher: MatchExpr::Any,
            action: RouteActionSpec::UpstreamGroup(group_id.clone()),
        };
        let router =
            Router::with_groups(vec![rule], RouteActionSpec::Direct, vec![(group_id, group)]);
        let target = target_domain("example.com", 443);
        let req = dummy_request(&target);
        let explanation = router.explain(&req, 0);

        assert_eq!(explanation.selected_upstream.as_deref(), Some("1"));
    }

    #[test]
    fn explain_does_not_mutate_scheduler_state() {
        let u1 = make_upstream(1);
        let u2 = make_upstream(2);

        let group_id = UpstreamGroupId(Arc::from("no-mut"));
        let group = UpstreamGroup {
            id: group_id.clone(),
            scheduler: SchedulerKind::RoundRobin,
            members: Arc::from([u1.clone(), u2.clone()]),
            fallback: GroupFallback::Reject,
        };

        let rule = CompiledRule {
            id: RuleId(Arc::from("r1")),
            matcher: MatchExpr::Any,
            action: RouteActionSpec::UpstreamGroup(group_id.clone()),
        };
        let router =
            Router::with_groups(vec![rule], RouteActionSpec::Direct, vec![(group_id, group)]);

        let target = target_domain("example.com", 443);
        let req = dummy_request(&target);

        let e1 = router.explain(&req, 0);
        let e2 = router.explain(&req, 0);

        assert_eq!(e1.selected_upstream, e2.selected_upstream);
    }

    #[test]
    fn explain_for_direct_default_route() {
        let router = Router::new(vec![], RouteActionSpec::Direct);
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let explanation = router.explain(&req, 3);

        assert_eq!(explanation.matched_rule.as_deref(), Some("default"));
        assert_eq!(explanation.action, "direct");
        assert_eq!(explanation.generation, 3);
    }

    #[test]
    fn explain_for_reject_default_route() {
        let router = Router::new(vec![], RouteActionSpec::Reject(RejectReason::Blocked));
        let target = target_domain("example.com", 80);
        let req = dummy_request(&target);
        let explanation = router.explain(&req, 0);

        assert_eq!(explanation.matched_rule.as_deref(), Some("default"));
        assert!(explanation.action.contains("reject"));
    }

    #[test]
    fn explain_for_upstream_group_default_route() {
        let u1 = make_upstream(1);
        let group_id = UpstreamGroupId(Arc::from("default-group"));
        let group = UpstreamGroup {
            id: group_id.clone(),
            scheduler: SchedulerKind::FirstAvailable,
            members: Arc::from([u1]),
            fallback: GroupFallback::Reject,
        };

        let router = Router::with_groups(
            vec![],
            RouteActionSpec::UpstreamGroup(group_id.clone()),
            vec![(group_id, group)],
        );

        let target = target_domain("example.com", 443);
        let req = dummy_request(&target);
        let explanation = router.explain(&req, 0);

        assert_eq!(explanation.matched_rule.as_deref(), Some("default"));
        assert_eq!(explanation.upstream_group.as_deref(), Some("default-group"));
        assert!(explanation.selected_upstream.is_some());
    }

    #[test]
    fn explain_json_output_is_valid_json() {
        let u1 = make_upstream(1);
        let group_id = UpstreamGroupId(Arc::from("json-group"));
        let group = UpstreamGroup {
            id: group_id.clone(),
            scheduler: SchedulerKind::LeastConnections,
            members: Arc::from([u1]),
            fallback: GroupFallback::Reject,
        };

        let rule = CompiledRule {
            id: RuleId(Arc::from("json-rule")),
            matcher: MatchExpr::Any,
            action: RouteActionSpec::UpstreamGroup(group_id.clone()),
        };
        let router =
            Router::with_groups(vec![rule], RouteActionSpec::Direct, vec![(group_id, group)]);
        let target = target_domain("example.com", 443);
        let req = dummy_request(&target);
        let explanation = router.explain(&req, 7);

        let json = serde_json::to_string(&explanation).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["target"], "example.com:443");
        assert_eq!(parsed["listener"], "test");
        assert_eq!(parsed["protocol"], "http");
        assert_eq!(parsed["matched_rule"], "json-rule");
        assert_eq!(parsed["scheduler"], "least-connections");
        assert_eq!(parsed["generation"], 7);
    }

    #[test]
    fn explain_human_readable_contains_expected_fields() {
        let u1 = make_upstream(1);
        let u2 = make_upstream(2);
        let group_id = UpstreamGroupId(Arc::from("hr-group"));
        let group = UpstreamGroup {
            id: group_id.clone(),
            scheduler: SchedulerKind::RoundRobin,
            members: Arc::from([u1, u2]),
            fallback: GroupFallback::Reject,
        };

        let rule = CompiledRule {
            id: RuleId(Arc::from("hr-rule")),
            matcher: MatchExpr::HostSuffix(Arc::from("corp.internal")),
            action: RouteActionSpec::UpstreamGroup(group_id.clone()),
        };
        let router =
            Router::with_groups(vec![rule], RouteActionSpec::Direct, vec![(group_id, group)]);
        let target = target_domain("app.corp.internal", 443);
        let req = dummy_request(&target);
        let explanation = router.explain(&req, 2);

        assert_eq!(explanation.target, "app.corp.internal:443");
        assert_eq!(explanation.listener, "test");
        assert_eq!(explanation.protocol, "http");
        assert_eq!(explanation.matched_rule.as_deref(), Some("hr-rule"));
        assert_eq!(explanation.upstream_group.as_deref(), Some("hr-group"));
        assert_eq!(explanation.scheduler.as_deref(), Some("round-robin"));
        assert_eq!(explanation.generation, 2);

        let json = serde_json::to_string_pretty(&explanation).unwrap();
        assert!(json.contains("\"app.corp.internal:443\""));
        assert!(json.contains("\"hr-rule\""));
        assert!(json.contains("\"round-robin\""));
        assert!(json.contains("\"hr-group\""));
    }
}
