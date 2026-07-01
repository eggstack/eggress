//! Reverse proxy admin support.
//!
//! Provides a small registry that holds a handle to each running
//! `ReverseServer` so the admin HTTP server can expose live reverse
//! state (active control channels, active external streams,
//! bind-denied counters, dropped-stream counters) at `/-/reverse`.
//!
//! The registry is intentionally lightweight: the runtime inserts a
//! handle per reverse server at startup, and admin reads snapshot
//! values directly from the underlying `Arc<ReverseServerState>`.
use std::collections::HashMap;
use std::sync::Arc;

use eggress_protocol_reverse::server::ReverseServerState;

/// Identifier for a reverse server. This is the user-supplied
/// `id = "..."` from the `[[reverse_servers]]` TOML block.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ReverseServerId(pub Arc<str>);

impl std::fmt::Display for ReverseServerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ReverseServerId {
    fn from(s: &str) -> Self {
        Self(Arc::from(s))
    }
}

/// One reverse server entry — its identity and a handle to its state.
pub struct ReverseServerEntry {
    pub id: ReverseServerId,
    pub control_bind: String,
    pub state: Arc<ReverseServerState>,
}

/// Thread-safe registry of reverse servers. The runtime inserts a
/// handle per server at startup; the admin route reads snapshots.
#[derive(Default, Clone)]
pub struct ReverseRegistry {
    inner: Arc<std::sync::RwLock<HashMap<ReverseServerId, ReverseServerEntry>>>,
}

impl ReverseRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a reverse server. Replaces any prior entry with the
    /// same id; in practice ids are unique per supervisor generation.
    pub fn register(&self, entry: ReverseServerEntry) {
        let id = entry.id.clone();
        let mut guard = self.inner.write().expect("reverse registry poisoned");
        guard.insert(id, entry);
    }

    /// Remove a reverse server entry by id. Called when the
    /// supervisor tears the server down.
    pub fn unregister(&self, id: &ReverseServerId) {
        let mut guard = self.inner.write().expect("reverse registry poisoned");
        guard.remove(id);
    }

    /// Snapshot all registered servers' state.
    pub fn snapshot(&self) -> Vec<ReverseServerEntrySnapshot> {
        let guard = self.inner.read().expect("reverse registry poisoned");
        guard
            .values()
            .map(|e| ReverseServerEntrySnapshot {
                id: e.id.0.to_string(),
                control_bind: e.control_bind.clone(),
                state: e.state.snapshot(),
            })
            .collect()
    }

    /// True if no reverse servers are registered.
    pub fn is_empty(&self) -> bool {
        let guard = self.inner.read().expect("reverse registry poisoned");
        guard.is_empty()
    }
}

/// Plain-data snapshot for the admin endpoint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReverseServerEntrySnapshot {
    pub id: String,
    pub control_bind: String,
    #[serde(flatten)]
    pub state: eggress_protocol_reverse::server::ReverseServerStateSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_snapshot() {
        let reg = ReverseRegistry::new();
        assert!(reg.is_empty());
        let state = Arc::new(ReverseServerState::default());
        state
            .active_control
            .store(2, std::sync::atomic::Ordering::Relaxed);
        reg.register(ReverseServerEntry {
            id: ReverseServerId::from("rev-1"),
            control_bind: "127.0.0.1:8080".to_string(),
            state,
        });
        assert!(!reg.is_empty());
        let snap = reg.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].id, "rev-1");
        assert_eq!(snap[0].state.active_control, 2);
    }

    #[test]
    fn unregister_removes_entry() {
        let reg = ReverseRegistry::new();
        let state = Arc::new(ReverseServerState::default());
        let id = ReverseServerId::from("rev-1");
        reg.register(ReverseServerEntry {
            id: id.clone(),
            control_bind: "127.0.0.1:8080".to_string(),
            state,
        });
        reg.unregister(&id);
        assert!(reg.is_empty());
    }

    #[test]
    fn register_replaces_existing_entry() {
        let reg = ReverseRegistry::new();
        let state1 = Arc::new(ReverseServerState::default());
        state1
            .active_control
            .store(1, std::sync::atomic::Ordering::Relaxed);
        reg.register(ReverseServerEntry {
            id: ReverseServerId::from("rev-1"),
            control_bind: "127.0.0.1:8080".to_string(),
            state: state1,
        });
        let state2 = Arc::new(ReverseServerState::default());
        state2
            .active_control
            .store(5, std::sync::atomic::Ordering::Relaxed);
        reg.register(ReverseServerEntry {
            id: ReverseServerId::from("rev-1"),
            control_bind: "127.0.0.1:9090".to_string(),
            state: state2,
        });
        let snap = reg.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].control_bind, "127.0.0.1:9090");
        assert_eq!(snap[0].state.active_control, 5);
    }
}
