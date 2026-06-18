use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use eggress_core::UpstreamId;

use crate::health::{HealthCell, HealthProbe, HealthState};
use crate::scheduler::SchedulerKind;
use crate::UpstreamGroupId;

use eggress_uri::ProxyChainSpec;

pub struct UpstreamRuntime {
    pub id: UpstreamId,
    pub chain: Arc<ProxyChainSpec>,
    pub enabled: AtomicBool,
    pub active: AtomicU64,
    pub in_flight: AtomicU64,
    pub health: HealthCell,
    pub health_probe: Option<HealthProbe>,
}

impl UpstreamRuntime {
    pub fn new(id: UpstreamId, chain: ProxyChainSpec) -> Self {
        Self {
            id,
            chain: Arc::new(chain),
            enabled: AtomicBool::new(true),
            active: AtomicU64::new(0),
            in_flight: AtomicU64::new(0),
            health: HealthCell::new(HealthState::Unknown),
            health_probe: None,
        }
    }

    pub fn new_with_health(id: UpstreamId, chain: ProxyChainSpec, state: HealthState) -> Self {
        Self {
            id,
            chain: Arc::new(chain),
            enabled: AtomicBool::new(true),
            active: AtomicU64::new(0),
            in_flight: AtomicU64::new(0),
            health: HealthCell::new(state),
            health_probe: None,
        }
    }

    pub fn with_health_probe(mut self, probe: HealthProbe) -> Self {
        self.health_probe = Some(probe);
        self
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn current_load(&self) -> u64 {
        self.active.load(Ordering::Relaxed) + self.in_flight.load(Ordering::Relaxed)
    }
}

pub enum GroupFallback {
    Reject,
    Direct,
    UseUnhealthy,
}

pub struct UpstreamGroup {
    pub id: UpstreamGroupId,
    pub scheduler: SchedulerKind,
    pub members: Arc<[Arc<UpstreamRuntime>]>,
    pub fallback: GroupFallback,
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("group must have at least one member")]
    EmptyGroup,
    #[error("duplicate upstream ID in group members")]
    DuplicateUpstreamId,
}

pub fn validate_group(group: &UpstreamGroup) -> Result<(), ValidationError> {
    if group.members.is_empty() {
        return Err(ValidationError::EmptyGroup);
    }
    let mut seen = std::collections::HashSet::new();
    for member in group.members.iter() {
        if !seen.insert(member.id) {
            return Err(ValidationError::DuplicateUpstreamId);
        }
    }
    Ok(())
}
