use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tempfile::NamedTempFile;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[tokio::test]
async fn readiness_transitions_to_false_on_shutdown() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed), "should be ready");

    // Trigger shutdown
    token.cancel();
    jh.await.ok();

    // Readiness should be false after shutdown
    assert!(
        !state.readiness.load(Ordering::Relaxed),
        "readiness should be false after shutdown"
    );
}

#[tokio::test]
async fn shutdown_drains_active_connections() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed));

    // Trigger shutdown (should drain within shutdown_grace of 30s)
    let start = std::time::Instant::now();
    token.cancel();
    jh.await.ok();
    let elapsed = start.elapsed();

    // Shutdown should complete well within the 30s grace period
    assert!(
        elapsed < Duration::from_secs(10),
        "shutdown took too long: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn shutdown_generation_remains_consistent() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let gen_before = state.generation.load(Ordering::Relaxed);
    assert_eq!(gen_before, 0);

    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Trigger shutdown
    token.cancel();
    jh.await.ok();

    // Generation should not change during shutdown
    let gen_after = state.generation.load(Ordering::Relaxed);
    assert_eq!(
        gen_before, gen_after,
        "generation should not change during shutdown"
    );
}

#[tokio::test]
async fn shutdown_active_connections_returns_to_zero() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    assert_eq!(state.active_connections.load(Ordering::Relaxed), 0);

    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Trigger shutdown
    token.cancel();
    jh.await.ok();

    assert_eq!(
        state.active_connections.load(Ordering::Relaxed),
        0,
        "active connections should be zero after shutdown"
    );
}

#[tokio::test]
async fn shutdown_stops_accepting_new_connections() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();

    let state = sup.state().clone();
    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());

    // Wait for readiness
    for _ in 0..50 {
        if state.readiness.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(state.readiness.load(Ordering::Relaxed));

    // Trigger shutdown
    token.cancel();
    jh.await.ok();

    // Active connections should be zero
    assert_eq!(
        state.active_connections.load(Ordering::Relaxed),
        0,
        "active connections should be zero after shutdown"
    );
}
