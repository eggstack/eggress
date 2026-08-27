use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::upstream::UpstreamRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseState {
    Pending,
    Transferred,
}

pub struct PendingLease {
    upstream: Arc<UpstreamRuntime>,
    state: LeaseState,
}

impl PendingLease {
    /// Begin best-effort in-flight accounting for the selected upstream.
    ///
    /// The lease owns the accounting until it is either transferred to an
    /// active lease or dropped by a caller that abandons route setup. The
    /// relaxed atomic is observability accounting, not synchronization.
    pub fn new(upstream: Arc<UpstreamRuntime>) -> Self {
        upstream.in_flight.fetch_add(1, Ordering::Relaxed);
        Self {
            upstream,
            state: LeaseState::Pending,
        }
    }

    pub fn established(self) -> ActiveLease {
        let upstream = self.upstream.clone();
        // Mark as transferred so Drop won't decrement in_flight
        let mut this = self;
        this.state = LeaseState::Transferred;
        upstream.in_flight.fetch_sub(1, Ordering::Relaxed);
        upstream.active.fetch_add(1, Ordering::Relaxed);
        ActiveLease { upstream }
    }

    pub fn upstream(&self) -> &UpstreamRuntime {
        &self.upstream
    }
}

impl Drop for PendingLease {
    fn drop(&mut self) {
        if self.state == LeaseState::Pending {
            self.upstream.in_flight.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

pub struct ActiveLease {
    upstream: Arc<UpstreamRuntime>,
}

impl ActiveLease {
    pub fn upstream(&self) -> &UpstreamRuntime {
        &self.upstream
    }
}

impl Drop for ActiveLease {
    fn drop(&mut self) {
        self.upstream.active.fetch_sub(1, Ordering::Relaxed);
    }
}
