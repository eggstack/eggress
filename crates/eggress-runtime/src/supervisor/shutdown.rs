//! Ordered shutdown: readiness false, listener stop, drain, admin last.
//!
//! Cancellation ordering is load-bearing and must be preserved exactly:
//! listeners stop accepting before connection drain so accept loops cannot
//! hand new connections to the tracker mid-drain, and the admin server stops
//! last so `/-/ready` and `/metrics` stay queryable through the drain window.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::error::RuntimeError;

use super::state::RuntimeState;

/// Everything the shutdown sequence needs, collected from the supervisor so
/// the ordering lives in one reviewable function instead of inline in `run()`.
pub(crate) struct ShutdownPlan {
    pub(crate) readiness: Arc<AtomicBool>,
    pub(crate) listener_cancel: CancellationToken,
    pub(crate) health_cancel: CancellationToken,
    pub(crate) connection_cancel: CancellationToken,
    pub(crate) admin_cancel: CancellationToken,
    pub(crate) state: Arc<RuntimeState>,
    pub(crate) tasks: TaskTracker,
    pub(crate) connection_tasks: TaskTracker,
    pub(crate) admin_tasks: TaskTracker,
    pub(crate) active_connections: Arc<AtomicU64>,
    pub(crate) shutdown_grace: Duration,
    #[cfg(feature = "ssh")]
    pub(crate) ssh_sessions: Arc<eggress_transport_ssh::SshSessionCache>,
    #[cfg(feature = "operations")]
    pub(crate) compatibility_system_proxy: Option<eggress_system_proxy::AppliedProxy>,
}

/// Run the canonical shutdown sequence.
///
/// 1. readiness false (`/-/ready` reports 503 during drain)
/// 2. stop listeners (no new connections accepted)
/// 3. stop health probes
/// 4. close all UDP associations
/// 5. wait for UDP relay tasks (bounded by grace)
/// 6. wait for listener accept loops to exit
/// 7. drain active connections within grace, then force-cancel
/// 8. wait for connection tasks
/// 9. stop admin last; restore the OS proxy if `--sys` applied one.
pub(crate) async fn shutdown_ordered(plan: ShutdownPlan) -> Result<(), RuntimeError> {
    // 1. Set readiness false (admin /-/ready will report 503 during drain)
    plan.readiness.store(false, Ordering::Release);

    // 2. Stop listeners (no new connections accepted)
    plan.listener_cancel.cancel();

    // 3. Stop health probes
    plan.health_cancel.cancel();

    // 4. Close all UDP associations
    plan.state.udp_registry.close_all().await;

    // 5. Wait for UDP relay tasks to complete
    plan.state.udp_tasks.close();
    let _ = tokio::time::timeout(plan.shutdown_grace, plan.state.udp_tasks.wait()).await;

    // 6. Wait for listener accept loops to exit so they cannot hand
    //    new connections to the connection tracker.
    plan.tasks.close();
    plan.tasks.wait().await;

    // 7. Drain active connections within the grace period; force-cancel
    //    afterwards. Admin stays up through this window so operators
    //    can observe drain progress via /-/ready, /-/status, /metrics.
    tracing::info!("draining active connections");

    let deadline = tokio::time::Instant::now() + plan.shutdown_grace;
    loop {
        let active = plan.active_connections.load(Ordering::Acquire);
        if active == 0 {
            tracing::info!("all connections drained");
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(active, "drain timeout reached, forcing shutdown");
            plan.connection_cancel.cancel();
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // 8. Wait for connection tasks (either drained naturally or force-cancelled)
    plan.connection_tasks.close();
    plan.connection_tasks.wait().await;

    #[cfg(feature = "ssh")]
    plan.ssh_sessions.shutdown().await;

    // 9. Now that the proxy has fully stopped accepting and serving
    //    traffic, stop the admin server. /-/ready has been reporting
    //    503 since step 1.
    plan.admin_cancel.cancel();
    plan.admin_tasks.close();
    plan.admin_tasks.wait().await;

    #[cfg(feature = "operations")]
    if let Some(mut proxy) = plan.compatibility_system_proxy {
        proxy.restore().map_err(RuntimeError::Other)?;
    }

    Ok(())
}
