//! Connection accounting primitives shared by all listener types.
//!
//! [`ListenerConnectionSlot`] enforces per-listener connection limits for
//! listeners that cannot use the core TCP listener wrapper (transparent and
//! Unix sockets). [`ActiveConnectionGuard`] tracks the global
//! `active_connections` counter. TCP listeners account via
//! `eggress_core::listener::PermitStream` instead; the two mechanisms are
//! exclusive — a connection must use exactly one, never both.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Pause between retries when a listener's `accept()` keeps failing (for
/// example fd exhaustion), so the loop does not tight-spin while the system
/// is already resource-starved.
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(100);

/// Log an accept failure, backing off briefly for persistent error classes so
/// repeated failures cannot hot-spin the accept loop. Transient races (a
/// queued connection vanishing before `accept`) retry immediately.
pub(crate) async fn handle_accept_error(context: &str, error: &std::io::Error) {
    match error.kind() {
        std::io::ErrorKind::WouldBlock
        | std::io::ErrorKind::Interrupted
        | std::io::ErrorKind::ConnectionAborted => {}
        _ => tokio::time::sleep(ACCEPT_ERROR_BACKOFF).await,
    }
    tracing::error!("{context} accept error: {error}");
}

/// Per-listener connection slot for listener implementations that cannot use
/// the core TCP listener wrapper (transparent and Unix sockets).
pub(crate) struct ListenerConnectionSlot {
    active: Arc<AtomicU64>,
}

impl ListenerConnectionSlot {
    pub(crate) fn try_acquire(active: &Arc<AtomicU64>, limit: u64) -> Option<Self> {
        active
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |current| {
                (current < limit).then_some(current + 1)
            })
            .ok()
            .map(|_| Self {
                active: active.clone(),
            })
    }
}

impl Drop for ListenerConnectionSlot {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Release);
    }
}

/// Guard for active-connection accounting on non-TCP listeners.
///
/// TCP listeners account via `eggress_core::listener::PermitStream` (semaphore
/// permit held for the whole session). Transparent and Unix listeners use this
/// counter instead. The two mechanisms are **exclusive** — a connection must
/// use exactly one, never both, otherwise the `active_connections` metric
/// double-counts. Call sites below (transparent/Unix/QUIC paths) ensure this
/// invariant; TCP accept paths must not also create this guard.
///
/// Ordering: `AcqRel` on inc pairs with `Release` on dec for visibility of
/// the counter to the metrics reader. `Relaxed` would also be correct for a
/// metrics-only counter, but `AcqRel` is kept for consistency with the
/// previous implementation.
pub(crate) struct ActiveConnectionGuard {
    active: Arc<AtomicU64>,
}

impl ActiveConnectionGuard {
    pub(crate) fn new(active: Arc<AtomicU64>) -> Self {
        active.fetch_add(1, Ordering::AcqRel);
        Self { active }
    }
}

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Release);
    }
}
