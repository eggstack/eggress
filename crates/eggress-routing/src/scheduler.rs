use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::health::is_eligible;
use crate::upstream::{UpstreamGroup, UpstreamRuntime};
use crate::RouteRequest;

pub trait RandomIndex: Send + Sync {
    fn index(&self, upper: usize) -> usize;
}

pub struct FastrandRandom;

impl RandomIndex for FastrandRandom {
    fn index(&self, upper: usize) -> usize {
        fastrand::usize(0..upper)
    }
}

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
    fn index(&self, upper: usize) -> usize {
        let idx = self.counter.fetch_add(1, Ordering::Relaxed);
        let values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        if values.is_empty() {
            return 0;
        }
        values[idx % values.len()] % upper
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
        if candidates.is_empty() {
            return None;
        }
        let len = candidates.len();
        let current = self.cursor.load(Ordering::Relaxed) as usize;
        for i in 0..len {
            let idx = (current + i) % len;
            if is_eligible(&candidates[idx]) {
                let _ = self.cursor.compare_exchange(
                    current as u64,
                    (current + 1) as u64,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                );
                return Some(candidates[idx].clone());
            }
        }
        None
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
        if candidates.is_empty() {
            return None;
        }
        let start = self.rng.index(candidates.len());
        for i in 0..candidates.len() {
            let idx = (start + i) % candidates.len();
            if is_eligible(&candidates[idx]) {
                return Some(candidates[idx].clone());
            }
        }
        None
    }
}

pub struct LeastConnectionsScheduler;

impl Scheduler for LeastConnectionsScheduler {
    fn select(
        &self,
        _group: &UpstreamGroup,
        candidates: &[Arc<UpstreamRuntime>],
        _request: &RouteRequest<'_>,
    ) -> Option<Arc<UpstreamRuntime>> {
        candidates
            .iter()
            .filter(|m| is_eligible(m))
            .min_by_key(|m| m.current_load())
            .cloned()
    }
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
        assert_eq!(rng.index(10), 2);
        assert_eq!(rng.index(10), 0);
        assert_eq!(rng.index(10), 1);
    }

    #[test]
    fn deterministic_random_index_wraps_around() {
        let rng = DeterministicRandom::new(vec![1, 3]);
        assert_eq!(rng.index(10), 1);
        assert_eq!(rng.index(10), 3);
        assert_eq!(rng.index(10), 1);
    }

    #[test]
    fn deterministic_random_index_modulo_upper() {
        let rng = DeterministicRandom::new(vec![5, 12, 7]);
        assert_eq!(rng.index(3), 2);
        assert_eq!(rng.index(3), 0);
        assert_eq!(rng.index(3), 1);
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
