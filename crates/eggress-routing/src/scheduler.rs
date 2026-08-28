use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::health::is_eligible;
use crate::upstream::{UpstreamGroup, UpstreamRuntime};
use crate::RouteRequest;

pub trait RandomIndex: Send + Sync {
    fn index(&self, upper: usize) -> Option<usize>;
}

/// Random source backed by fastrand's process-global generator.
pub struct FastrandRandom;

impl RandomIndex for FastrandRandom {
    fn index(&self, upper: usize) -> Option<usize> {
        (upper > 0).then(|| fastrand::usize(0..upper))
    }
}

/// Repeatable random source for tests and deterministic callers.
///
/// Values are consumed in order and wrap when the input list is exhausted;
/// the call counter uses wrapping arithmetic at `usize::MAX`.
pub struct DeterministicRandom {
    values: Mutex<Vec<usize>>,
    counter: AtomicUsize,
}

impl DeterministicRandom {
    pub fn new(values: Vec<usize>) -> Self {
        Self {
            values: Mutex::new(values),
            counter: AtomicUsize::new(0),
        }
    }
}

impl RandomIndex for DeterministicRandom {
    fn index(&self, upper: usize) -> Option<usize> {
        let idx = self.counter.fetch_add(1, Ordering::Relaxed);
        let values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        if values.is_empty() || upper == 0 {
            return None;
        }
        Some(values[idx % values.len()] % upper)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerKind {
    FirstAvailable,
    RoundRobin,
    Random,
    LeastConnections,
}

pub trait Scheduler: Send + Sync {
    fn select(
        &self,
        group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>>;

    /// Select among enabled members without considering health state.
    ///
    /// This is used only for `use-unhealthy` fallback. Normal selection must
    /// continue to use [`Scheduler::select`], which excludes unhealthy peers.
    fn select_enabled(
        &self,
        group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        self.select(group, candidates, request)
    }

    fn preview(
        &self,
        group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        self.select(group, candidates, request)
    }
}

pub struct FirstAvailableScheduler;

impl Scheduler for FirstAvailableScheduler {
    fn select(
        &self,
        _group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        _request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        candidates.iter().find(|m| is_eligible(m)).cloned()
    }

    fn select_enabled(
        &self,
        _group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        _request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        candidates.iter().find(|m| m.is_enabled()).cloned()
    }
}

pub struct RoundRobinScheduler {
    cursor: AtomicU64,
}

impl RoundRobinScheduler {
    pub fn new() -> Self {
        Self {
            cursor: AtomicU64::new(0),
        }
    }
}

impl Default for RoundRobinScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler for RoundRobinScheduler {
    fn select(
        &self,
        _group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        _request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        select_round_robin(&self.cursor, candidates, is_eligible)
    }

    fn select_enabled(
        &self,
        _group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        _request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        select_round_robin(&self.cursor, candidates, |member| member.is_enabled())
    }

    fn preview(
        &self,
        _group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        _request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        if candidates.is_empty() {
            return None;
        }
        let start = self.cursor.load(Ordering::Relaxed) as usize;
        let len = candidates.len();
        for i in 0..len {
            let idx = (start + i) % len;
            if is_eligible(&candidates[idx]) {
                return Some(candidates[idx].clone());
            }
        }
        None
    }
}

pub struct RandomScheduler {
    rng: Arc<dyn RandomIndex>,
}

impl RandomScheduler {
    pub fn new() -> Self {
        Self {
            rng: Arc::new(FastrandRandom),
        }
    }

    pub fn with_rng(rng: Arc<dyn RandomIndex>) -> Self {
        Self { rng }
    }
}

impl Default for RandomScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler for RandomScheduler {
    fn select(
        &self,
        _group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        _request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        select_random(&*self.rng, candidates, is_eligible)
    }

    fn select_enabled(
        &self,
        _group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        _request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        select_random(&*self.rng, candidates, |member| member.is_enabled())
    }
}

/// Selects the least-loaded eligible member, rotating ties for fairness.
pub struct LeastConnectionsScheduler;

static LEAST_CONNECTIONS_CURSOR: AtomicU64 = AtomicU64::new(0);

impl Scheduler for LeastConnectionsScheduler {
    fn select(
        &self,
        _group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        _request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        select_least_connections(candidates, is_eligible)
    }

    fn select_enabled(
        &self,
        _group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        _request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        select_least_connections(candidates, |member| member.is_enabled())
    }
}

fn select_least_connections<F>(
    candidates: &[Arc<UpstreamRuntime>],
    eligible: F,
) -> Option<Arc<UpstreamRuntime>>
where
    F: Fn(&UpstreamRuntime) -> bool,
{
    let min_load = candidates
        .iter()
        .filter(|member| eligible(member))
        .map(|member| member.current_load())
        .min()?;
    let tied: Vec<_> = candidates
        .iter()
        .filter(|member| eligible(member) && member.current_load() == min_load)
        .collect();
    let index = LEAST_CONNECTIONS_CURSOR.fetch_add(1, Ordering::Relaxed) as usize;
    Some(tied[index % tied.len()].clone())
}

fn select_round_robin<F>(
    cursor: &AtomicU64,
    candidates: &[Arc<UpstreamRuntime>],
    eligible: F,
) -> Option<Arc<UpstreamRuntime>>
where
    F: Fn(&UpstreamRuntime) -> bool,
{
    if candidates.is_empty() {
        return None;
    }
    let len = candidates.len();
    let eligible_indices: Vec<usize> = (0..len).filter(|&idx| eligible(&candidates[idx])).collect();
    if eligible_indices.is_empty() {
        return None;
    }
    let start = cursor.fetch_add(1, Ordering::Relaxed) as usize;
    Some(candidates[eligible_indices[start % eligible_indices.len()]].clone())
}

fn select_random<F>(
    rng: &dyn RandomIndex,
    candidates: &[Arc<UpstreamRuntime>],
    eligible: F,
) -> Option<Arc<UpstreamRuntime>>
where
    F: Fn(&UpstreamRuntime) -> bool,
{
    if candidates.is_empty() {
        return None;
    }
    let eligible_indices: Vec<usize> = (0..candidates.len())
        .filter(|&idx| eligible(&candidates[idx]))
        .collect();
    let index = rng.index(eligible_indices.len())?;
    Some(candidates[eligible_indices[index]].clone())
}

pub fn resolve_scheduler(kind: SchedulerKind) -> Arc<dyn Scheduler> {
    match kind {
        SchedulerKind::FirstAvailable => Arc::new(FirstAvailableScheduler),
        SchedulerKind::RoundRobin => Arc::new(RoundRobinScheduler::new()),
        SchedulerKind::Random => Arc::new(RandomScheduler::new()),
        SchedulerKind::LeastConnections => Arc::new(LeastConnectionsScheduler),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upstream::{GroupFallback, UpstreamRuntime};
    use eggress_core::UpstreamId;
    use eggress_uri::ProxyChainSpec;

    fn make_upstream(id: &str) -> Arc<UpstreamRuntime> {
        Arc::new(UpstreamRuntime::new(
            UpstreamId::new(id),
            ProxyChainSpec { hops: vec![] },
        ))
    }

    #[test]
    fn deterministic_random_index_returns_sequential_values() {
        let rng = DeterministicRandom::new(vec![2, 0, 1]);
        assert_eq!(rng.index(10), Some(2));
        assert_eq!(rng.index(10), Some(0));
        assert_eq!(rng.index(10), Some(1));
    }

    #[test]
    fn deterministic_random_index_wraps_around() {
        let rng = DeterministicRandom::new(vec![1, 3]);
        assert_eq!(rng.index(10), Some(1));
        assert_eq!(rng.index(10), Some(3));
        assert_eq!(rng.index(10), Some(1));
    }

    #[test]
    fn deterministic_random_index_modulo_upper() {
        let rng = DeterministicRandom::new(vec![5, 12, 7]);
        assert_eq!(rng.index(3), Some(2));
        assert_eq!(rng.index(3), Some(0));
        assert_eq!(rng.index(3), Some(1));
    }

    #[test]
    fn random_scheduler_deterministic_with_seed() {
        let rng = Arc::new(DeterministicRandom::new(vec![0, 1, 2, 0]));
        let scheduler = RandomScheduler::with_rng(rng);

        let a = make_upstream("a");
        let b = make_upstream("b");
        let c = make_upstream("c");

        let group = UpstreamGroup {
            id: crate::UpstreamGroupId("test".into()),
            scheduler: Arc::new(FirstAvailableScheduler),
            scheduler_kind: SchedulerKind::Random,
            members: Arc::from(vec![a.clone(), b.clone(), c.clone()]),
            fallback: GroupFallback::Reject,
        };

        let target = crate::TargetAddr {
            host: crate::TargetHost::Domain("example.com".to_string()),
            port: 80,
        };
        let identity = crate::ClientIdentity::Anonymous;
        let request = crate::RouteRequest {
            target: &target,
            source: None,
            listener: "test",
            inbound_protocol: crate::ProtocolId::Http,
            identity: &identity,
            transport: crate::TransportKind::Tcp,
        };

        let candidates = vec![a.clone(), b.clone(), c.clone()];

        let selected1 = scheduler.select(&group, &candidates, &request).unwrap();
        assert!(Arc::ptr_eq(&selected1, &a));

        let selected2 = scheduler.select(&group, &candidates, &request).unwrap();
        assert!(Arc::ptr_eq(&selected2, &b));

        let selected3 = scheduler.select(&group, &candidates, &request).unwrap();
        assert!(Arc::ptr_eq(&selected3, &c));

        let selected4 = scheduler.select(&group, &candidates, &request).unwrap();
        assert!(Arc::ptr_eq(&selected4, &a));
    }
}
