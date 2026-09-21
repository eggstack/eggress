//! Private readiness/signal run-loop phase for `ServiceSupervisor::run()`.
//!
//! Publishes readiness at the same semantic point as the baseline (after
//! signal handling is installed), waits on cancellation/CTRL-C/SIGTERM,
//! processes SIGHUP only for file-backed configuration through the canonical
//! `RuntimeState::apply_compiled_config()` transaction, and records reload
//! failure metrics exactly as before.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::reload::ReloadResult;
use super::state::RuntimeState;

/// Run the readiness/signal wait loop until shutdown is requested.
///
/// `config_path` is the startup-captured file path (`""` when in-memory):
/// SIGHUP reload stays file-backed only; in-memory embed startup never
/// acquires file reload semantics.
pub(crate) async fn run_signal_loop(
    readiness: &Arc<AtomicBool>,
    cancel: &CancellationToken,
    config_path: &str,
    state_ref: &Arc<RuntimeState>,
    runtime_metrics: &Arc<dyn eggress_metrics::RuntimeMetrics>,
) {
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
        let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup());

        if let Err(ref e) = sigterm {
            tracing::warn!("failed to register SIGTERM handler: {e}");
        }
        if let Err(ref e) = sighup {
            tracing::warn!("failed to register SIGHUP handler: {e}");
        }

        // Readiness also means signal handling is installed. This
        // prevents a reload signal from racing startup.
        readiness.store(true, Ordering::Release);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("shutdown requested via cancel token");
                    break;
                }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("shutdown signal received");
                    break;
                }
                _ = async { sigterm.as_mut().ok()?.recv().await }, if sigterm.is_ok() => {
                    tracing::info!("shutdown signal received");
                    break;
                }
                _ = async { sighup.as_mut().ok()?.recv().await }, if sighup.is_ok() && !config_path.is_empty() => {
                    tracing::info!("reload signal received, reloading config from {config_path}");
                    let config_path_clone = config_path.to_owned();
                    let load_result = tokio::task::spawn_blocking(move || {
                        eggress_config::compile::load_and_compile(&config_path_clone)
                    }).await;
                    match load_result {
                        Ok(Ok(new_rt_config)) => {
                            match state_ref.apply_compiled_config(&new_rt_config) {
                                ReloadResult::Applied { generation: gen, upstreams: upstream_count } => {
                                    tracing::info!(
                                        generation = gen,
                                        upstreams = upstream_count,
                                        "config reloaded successfully"
                                    );
                                }
                                ReloadResult::Rejected { reason } => {
                                    tracing::error!("reload rejected: {reason}");
                                }
                                ReloadResult::Failed { error } => {
                                    tracing::error!("reload failed (snapshot build): {error}");
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            runtime_metrics.record_reload(false);
                            tracing::error!("reload failed (config load): {e}");
                        }
                        Err(join_err) => {
                            runtime_metrics.record_reload(false);
                            tracing::error!("reload task panicked: {join_err}");
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (config_path, state_ref, runtime_metrics);
        readiness.store(true, Ordering::Release);
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("shutdown requested via cancel token");
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutdown signal received");
            }
        }
    }
}
