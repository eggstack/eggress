use std::io::Write;
use std::sync::atomic::Ordering;

use tempfile::NamedTempFile;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[test]
fn full_toml_config_starts_service() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
}

#[test]
fn readiness_starts_false_before_run() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    assert!(
        !sup.state().readiness.load(Ordering::Relaxed),
        "readiness should be false before run()"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bind_conflict_aborts_startup_before_readiness() {
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = held.local_addr().unwrap();
    let bind_addr = addr.to_string();

    let config = format!(
        r#"
version = 1

[[listeners]]
name = "conflict"
bind = "{bind_addr}"
protocols = ["http"]
"#,
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap().to_string();

    let mut supervisor = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let state = supervisor.state().clone();

    let result = tokio::task::spawn_blocking(move || supervisor.run())
        .await
        .expect("spawn_blocking failed");

    assert!(
        matches!(
            result,
            Err(eggress_runtime::RuntimeError::ListenerBind { .. })
        ),
        "expected ListenerBind error, got {result:?}"
    );
    assert!(
        !state.readiness.load(Ordering::Relaxed),
        "readiness should remain false when bind fails"
    );

    drop(held);
}

#[test]
fn runtime_state_has_expected_generation() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    assert_eq!(
        sup.state().generation.load(Ordering::Relaxed),
        0,
        "generation should start at 0"
    );
}

#[test]
fn runtime_state_has_metrics_registry() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    let metrics = sup.state().metrics.clone();
    let rendered = metrics.render_prometheus();
    assert!(
        rendered.contains("eggress_connections_active"),
        "metrics should contain connections_active"
    );
}

#[test]
fn runtime_state_has_active_connections_zero() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let sup = eggress_runtime::ServiceSupervisor::start(path).unwrap();
    assert_eq!(
        sup.state().active_connections.load(Ordering::Relaxed),
        0,
        "active connections should start at 0"
    );
}
