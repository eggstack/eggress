use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::health::is_eligible;
use crate::upstream::{UpstreamGroup, UpstreamRuntime};
use crate::RouteRequest;

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
        let start = self.cursor.fetch_add(1, Ordering::Relaxed) as usize;
        let len = candidates.len();
        for i in 0..len {
            let idx = (start + i) % len;
            if is_eligible(&candidates[idx]) {
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

pub struct RandomScheduler;

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
        let len = candidates.len() as u64;
        let start = fastrand::u64(0..len) as usize;
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
        SchedulerKind::Random => Arc::new(RandomScheduler),
        SchedulerKind::LeastConnections => Arc::new(LeastConnectionsScheduler),
    }
}
