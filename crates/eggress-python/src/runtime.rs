//! Shared Tokio runtime for Python outbound operations.
//!
//! A single process-wide multi-thread runtime backs all outbound connectors
//! and streams so short-lived Python objects never pay runtime-startup cost.

use std::sync::{Arc, OnceLock};

pub(crate) static PY_OUTBOUND_RUNTIME: OnceLock<Result<Arc<tokio::runtime::Runtime>, String>> =
    OnceLock::new();

/// Return the shared outbound runtime, building it once on first use.
pub(crate) fn outbound_runtime() -> Result<Arc<tokio::runtime::Runtime>, String> {
    PY_OUTBOUND_RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map(Arc::new)
                .map_err(|e| format!("runtime setup failed: {e}"))
        })
        .as_ref()
        .map(Arc::clone)
        .map_err(Clone::clone)
}
