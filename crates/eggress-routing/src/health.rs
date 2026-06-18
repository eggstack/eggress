use std::net::SocketAddr;
use std::sync::RwLock;
use std::time::{Duration, SystemTime};

use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

use crate::upstream::UpstreamRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HealthState {
    Unknown,
    Healthy,
    Suspect,
    Unhealthy,
    Recovering,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct HealthSnapshot {
    pub state: HealthState,
    pub consecutive_successes: u32,
    pub consecutive_failures: u32,
    pub last_checked_at: Option<SystemTime>,
    pub last_success_at: Option<SystemTime>,
    pub last_failure_at: Option<SystemTime>,
    pub last_latency: Option<Duration>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HealthConfig {
    pub interval: Duration,
    pub timeout: Duration,
    pub failures_to_unhealthy: u32,
    pub successes_to_healthy: u32,
    pub initial_state: HealthState,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            failures_to_unhealthy: 3,
            successes_to_healthy: 2,
            initial_state: HealthState::Unknown,
        }
    }
}

pub struct HealthCell {
    inner: RwLock<HealthSnapshot>,
}

impl HealthCell {
    pub fn new(initial: HealthState) -> Self {
        Self {
            inner: RwLock::new(HealthSnapshot {
                state: initial,
                consecutive_successes: 0,
                consecutive_failures: 0,
                last_checked_at: None,
                last_success_at: None,
                last_failure_at: None,
                last_latency: None,
                last_error: None,
            }),
        }
    }

    pub fn snapshot(&self) -> HealthSnapshot {
        self.inner.read().unwrap().clone()
    }

    pub fn state(&self) -> HealthState {
        self.inner.read().unwrap().state
    }

    pub fn observe_success(&self, latency: Duration, config: &HealthConfig) {
        let mut snap = self.inner.write().unwrap();
        snap.consecutive_successes += 1;
        snap.consecutive_failures = 0;
        snap.last_checked_at = Some(SystemTime::now());
        snap.last_success_at = Some(SystemTime::now());
        snap.last_latency = Some(latency);
        snap.last_error = None;

        snap.state = match snap.state {
            HealthState::Disabled => HealthState::Disabled,
            HealthState::Unhealthy => HealthState::Recovering,
            HealthState::Recovering => {
                if snap.consecutive_successes >= config.successes_to_healthy {
                    HealthState::Healthy
                } else {
                    HealthState::Recovering
                }
            }
            HealthState::Suspect => HealthState::Healthy,
            HealthState::Unknown => {
                if snap.consecutive_successes >= config.successes_to_healthy {
                    HealthState::Healthy
                } else {
                    HealthState::Unknown
                }
            }
            HealthState::Healthy => HealthState::Healthy,
        };
    }

    pub fn observe_failure(&self, error: Option<String>, config: &HealthConfig) {
        let mut snap = self.inner.write().unwrap();
        snap.consecutive_failures += 1;
        snap.consecutive_successes = 0;
        snap.last_checked_at = Some(SystemTime::now());
        snap.last_failure_at = Some(SystemTime::now());
        snap.last_latency = None;
        snap.last_error = error;

        snap.state = match snap.state {
            HealthState::Disabled => HealthState::Disabled,
            HealthState::Unhealthy => HealthState::Unhealthy,
            HealthState::Recovering => HealthState::Unhealthy,
            HealthState::Healthy => {
                if snap.consecutive_failures >= config.failures_to_unhealthy {
                    HealthState::Unhealthy
                } else {
                    HealthState::Suspect
                }
            }
            HealthState::Suspect => {
                if snap.consecutive_failures >= config.failures_to_unhealthy {
                    HealthState::Unhealthy
                } else {
                    HealthState::Suspect
                }
            }
            HealthState::Unknown => {
                if snap.consecutive_failures >= config.failures_to_unhealthy {
                    HealthState::Unhealthy
                } else {
                    HealthState::Suspect
                }
            }
        };
    }
}

#[derive(Debug, Clone)]
pub enum HealthProbe {
    TcpConnect {
        target: SocketAddr,
        timeout: Duration,
    },
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub success: bool,
    pub latency: Duration,
    pub error: Option<String>,
}

pub async fn probe_tcp(target: SocketAddr, timeout: Duration) -> ProbeResult {
    let start = SystemTime::now();
    let result = tokio::time::timeout(timeout, TcpStream::connect(target)).await;
    let latency = start.elapsed().unwrap_or_default();

    match result {
        Ok(Ok(_stream)) => ProbeResult {
            success: true,
            latency,
            error: None,
        },
        Ok(Err(e)) => ProbeResult {
            success: false,
            latency,
            error: Some(e.to_string()),
        },
        Err(_timeout) => ProbeResult {
            success: false,
            latency,
            error: Some("timeout".to_string()),
        },
    }
}

pub fn is_eligible(upstream: &UpstreamRuntime) -> bool {
    if !upstream.is_enabled() {
        return false;
    }
    matches!(
        upstream.health.state(),
        HealthState::Unknown
            | HealthState::Healthy
            | HealthState::Suspect
            | HealthState::Recovering
    )
}

pub struct HealthManager {
    cancel: CancellationToken,
    tasks: tokio::task::JoinSet<()>,
}

impl HealthManager {
    pub fn new(cancel: CancellationToken) -> Self {
        Self {
            cancel,
            tasks: tokio::task::JoinSet::new(),
        }
    }

    pub fn start_probes(
        &mut self,
        upstreams: &[std::sync::Arc<UpstreamRuntime>],
        config: &HealthConfig,
    ) {
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(10));

        for upstream in upstreams {
            let upstream = upstream.clone();
            let config = config.clone();
            let cancel = self.cancel.clone();
            let semaphore = semaphore.clone();

            self.tasks.spawn(async move {
                let probe = match upstream.health_probe.clone() {
                    Some(p) => p,
                    None => return,
                };

                loop {
                    if cancel.is_cancelled() {
                        break;
                    }

                    let base = config.interval;
                    let jitter_pct = 0.2;
                    let jitter_ms = (base.as_millis() as f64
                        * jitter_pct
                        * (fastrand::f64() * 2.0 - 1.0)) as u64;
                    let delay = if jitter_ms < base.as_millis() as u64 {
                        base + Duration::from_millis(jitter_ms)
                    } else {
                        base
                    };

                    if let Ok(()) = tokio::time::timeout(delay, cancel.cancelled()).await {
                        break;
                    }

                    if cancel.is_cancelled() {
                        break;
                    }

                    let permit = semaphore.clone().acquire_owned().await.unwrap();
                    let upstream = upstream.clone();
                    let config = config.clone();
                    let probe_clone = probe.clone();

                    tokio::spawn(async move {
                        let result = match probe_clone {
                            HealthProbe::TcpConnect { target, timeout } => {
                                probe_tcp(target, timeout).await
                            }
                        };

                        if result.success {
                            upstream.health.observe_success(result.latency, &config);
                        } else {
                            upstream.health.observe_failure(result.error, &config);
                        }

                        drop(permit);
                    });
                }
            });
        }
    }

    pub fn stop_all(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::upstream::UpstreamRuntime;
    use eggress_core::UpstreamId;
    use eggress_uri::ProxyChainSpec;

    fn make_upstream_with_health(id: UpstreamId, state: HealthState) -> Arc<UpstreamRuntime> {
        Arc::new(UpstreamRuntime::new_with_health(
            id,
            ProxyChainSpec { hops: vec![] },
            state,
        ))
    }

    #[test]
    fn health_config_defaults() {
        let config = HealthConfig::default();
        assert_eq!(config.interval, Duration::from_secs(30));
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert_eq!(config.failures_to_unhealthy, 3);
        assert_eq!(config.successes_to_healthy, 2);
        assert_eq!(config.initial_state, HealthState::Unknown);
    }

    #[test]
    fn health_cell_initial_state() {
        let cell = HealthCell::new(HealthState::Healthy);
        assert_eq!(cell.state(), HealthState::Healthy);
        let snap = cell.snapshot();
        assert_eq!(snap.state, HealthState::Healthy);
        assert_eq!(snap.consecutive_successes, 0);
        assert_eq!(snap.consecutive_failures, 0);
    }

    #[test]
    fn unknown_to_healthy_after_successes() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Unknown);
        cell.observe_success(Duration::from_millis(10), &config);
        assert_eq!(cell.state(), HealthState::Unknown);
        cell.observe_success(Duration::from_millis(10), &config);
        assert_eq!(cell.state(), HealthState::Healthy);
    }

    #[test]
    fn unknown_to_suspect_on_failure() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Unknown);
        cell.observe_failure(None, &config);
        assert_eq!(cell.state(), HealthState::Suspect);
    }

    #[test]
    fn healthy_to_suspect_on_failure_below_threshold() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Healthy);
        cell.observe_failure(None, &config);
        assert_eq!(cell.state(), HealthState::Suspect);
    }

    #[test]
    fn healthy_to_unhealthy_at_threshold() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Healthy);
        for _ in 0..config.failures_to_unhealthy {
            cell.observe_failure(None, &config);
        }
        assert_eq!(cell.state(), HealthState::Unhealthy);
    }

    #[test]
    fn suspect_to_healthy_on_success() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Suspect);
        cell.observe_failure(None, &config);
        assert_eq!(cell.state(), HealthState::Suspect);
        cell.observe_success(Duration::from_millis(10), &config);
        assert_eq!(cell.state(), HealthState::Healthy);
    }

    #[test]
    fn suspect_to_unhealthy_at_threshold() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Suspect);
        for _ in 0..config.failures_to_unhealthy {
            cell.observe_failure(None, &config);
        }
        assert_eq!(cell.state(), HealthState::Unhealthy);
    }

    #[test]
    fn unhealthy_to_recovering_on_success() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Unhealthy);
        cell.observe_success(Duration::from_millis(10), &config);
        assert_eq!(cell.state(), HealthState::Recovering);
    }

    #[test]
    fn recovering_to_healthy_after_enough_successes() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Recovering);
        cell.observe_success(Duration::from_millis(10), &config);
        assert_eq!(cell.state(), HealthState::Recovering);
        cell.observe_success(Duration::from_millis(10), &config);
        assert_eq!(cell.state(), HealthState::Healthy);
    }

    #[test]
    fn recovering_to_unhealthy_on_failure() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Recovering);
        cell.observe_failure(None, &config);
        assert_eq!(cell.state(), HealthState::Unhealthy);
    }

    #[test]
    fn disabled_stays_disabled_on_success() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Disabled);
        cell.observe_success(Duration::from_millis(10), &config);
        assert_eq!(cell.state(), HealthState::Disabled);
    }

    #[test]
    fn disabled_stays_disabled_on_failure() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Disabled);
        cell.observe_failure(None, &config);
        assert_eq!(cell.state(), HealthState::Disabled);
    }

    #[test]
    fn snapshot_records_timestamps() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Unknown);
        assert!(cell.snapshot().last_checked_at.is_none());

        cell.observe_success(Duration::from_millis(10), &config);
        let snap = cell.snapshot();
        assert!(snap.last_checked_at.is_some());
        assert!(snap.last_success_at.is_some());
        assert!(snap.last_failure_at.is_none());
        assert_eq!(snap.last_latency, Some(Duration::from_millis(10)));
        assert!(snap.last_error.is_none());

        cell.observe_failure(Some("err".to_string()), &config);
        let snap = cell.snapshot();
        assert!(snap.last_failure_at.is_some());
        assert_eq!(snap.last_error.as_deref(), Some("err"));
    }

    #[test]
    fn failure_resets_success_counter() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Unknown);
        cell.observe_success(Duration::from_millis(10), &config);
        cell.observe_success(Duration::from_millis(10), &config);
        assert_eq!(cell.state(), HealthState::Healthy);
        cell.observe_failure(None, &config);
        assert_eq!(cell.state(), HealthState::Suspect);
        cell.observe_success(Duration::from_millis(10), &config);
        assert_eq!(cell.state(), HealthState::Healthy);
    }

    #[test]
    fn failure_resets_success_counter_for_recovering() {
        let config = HealthConfig::default();
        let cell = HealthCell::new(HealthState::Recovering);
        cell.observe_success(Duration::from_millis(10), &config);
        assert_eq!(cell.state(), HealthState::Recovering);
        cell.observe_failure(None, &config);
        assert_eq!(cell.state(), HealthState::Unhealthy);
    }

    #[test]
    fn eligible_upstream_healthy() {
        let u = make_upstream_with_health(1, HealthState::Healthy);
        assert!(is_eligible(&u));
    }

    #[test]
    fn eligible_upstream_unknown() {
        let u = make_upstream_with_health(1, HealthState::Unknown);
        assert!(is_eligible(&u));
    }

    #[test]
    fn eligible_upstream_suspect() {
        let u = make_upstream_with_health(1, HealthState::Suspect);
        assert!(is_eligible(&u));
    }

    #[test]
    fn eligible_upstream_recovering() {
        let u = make_upstream_with_health(1, HealthState::Recovering);
        assert!(is_eligible(&u));
    }

    #[test]
    fn not_eligible_upstream_unhealthy() {
        let u = make_upstream_with_health(1, HealthState::Unhealthy);
        assert!(!is_eligible(&u));
    }

    #[test]
    fn not_eligible_upstream_disabled_health() {
        let u = make_upstream_with_health(1, HealthState::Disabled);
        assert!(!is_eligible(&u));
    }

    #[test]
    fn not_eligible_upstream_disabled_flag() {
        let u = make_upstream_with_health(1, HealthState::Healthy);
        u.set_enabled(false);
        assert!(!is_eligible(&u));
    }

    #[test]
    fn health_cell_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let config = Arc::new(HealthConfig::default());
        let cell = Arc::new(HealthCell::new(HealthState::Unknown));

        let handles: Vec<_> = (0..100)
            .map(|_| {
                let cell = cell.clone();
                let config = config.clone();
                thread::spawn(move || {
                    cell.observe_success(Duration::from_millis(10), &config);
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let snap = cell.snapshot();
        assert_eq!(snap.consecutive_successes, 100);
    }

    #[tokio::test]
    async fn probe_tcp_success() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let result = probe_tcp(addr, Duration::from_secs(1)).await;
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn probe_tcp_failure() {
        let addr: SocketAddr = "127.0.0.1:19".parse().unwrap();
        let result = probe_tcp(addr, Duration::from_millis(100)).await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn probe_tcp_timeout() {
        let addr: SocketAddr = "192.0.2.1:80".parse().unwrap();
        let result = probe_tcp(addr, Duration::from_millis(50)).await;
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("timeout"));
    }

    #[test]
    fn health_manager_new() {
        let cancel = CancellationToken::new();
        let mut mgr = HealthManager::new(cancel.clone());
        assert!(!cancel.is_cancelled());
        mgr.stop_all();
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn jitter_range_validation() {
        let base = Duration::from_secs(30);
        let jitter_pct = 0.2;
        for _ in 0..1000 {
            let jitter_ms =
                (base.as_millis() as f64 * jitter_pct * (fastrand::f64() * 2.0 - 1.0)) as u64;
            let delay = if jitter_ms < base.as_millis() as u64 {
                base + Duration::from_millis(jitter_ms)
            } else {
                base
            };
            assert!(delay >= base - Duration::from_millis(1));
            assert!(delay <= base + Duration::from_secs(6));
        }
    }

    #[tokio::test]
    async fn health_probes_start_and_stop() {
        let cancel = CancellationToken::new();
        let mut mgr = HealthManager::new(cancel.clone());

        let upstreams: Vec<Arc<UpstreamRuntime>> = (0..3)
            .map(|i| make_upstream_with_health(i, HealthState::Healthy))
            .collect();

        let config = HealthConfig::default();
        mgr.start_probes(&upstreams, &config);
        tokio::time::sleep(Duration::from_millis(100)).await;
        mgr.stop_all();
        assert!(cancel.is_cancelled());
    }
}
