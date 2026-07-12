//! Integration tests for reverse proxy supervisor wiring.
//!
//! Verifies that the `ServiceSupervisor` correctly spawns reverse servers
//! and clients from TOML configuration and that they work end-to-end.

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

/// Find an available port by binding to port 0.
async fn find_available_port() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

#[test]
fn reverse_server_config_parses_from_toml() {
    let config = r#"
version = 1

[[reverse_servers]]
id = "rev-srv-1"
control_bind = "127.0.0.1:0"
external_bind = "127.0.0.1:0"

[[reverse_clients]]
id = "rev-cli-1"
server_addr = "127.0.0.1:12345"
default_target_host = "127.0.0.1"
default_target_port = 80
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_ok(),
        "reverse server/client config should parse: {:?}",
        result.err()
    );
}

#[test]
fn reverse_client_parallel_connections_parses() {
    let config = r#"
version = 1

[[reverse_clients]]
id = "rev-cli-parallel"
server_addr = "127.0.0.1:12345"
parallel_connections = 3
default_target_host = "127.0.0.1"
default_target_port = 80
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_ok(),
        "parallel_connections config should parse: {:?}",
        result.err()
    );
}

#[test]
fn reverse_server_invalid_bind_address_fails() {
    let config = r#"
version = 1

[[reverse_servers]]
id = "rev-srv-bad"
control_bind = "not-a-valid-address"
external_bind = "127.0.0.1:0"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(result.is_err(), "invalid control_bind should fail");
}

#[test]
fn reverse_client_invalid_address_fails() {
    let config = r#"
version = 1

[[reverse_clients]]
id = "rev-cli-bad"
server_addr = "not-a-valid-address"
default_target_host = "127.0.0.1"
default_target_port = 80
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(result.is_err(), "invalid server_addr should fail");
}

#[test]
fn reverse_server_with_auth_parses() {
    let config = r#"
version = 1

[[reverse_servers]]
id = "rev-srv-auth"
control_bind = "127.0.0.1:0"
external_bind = "127.0.0.1:0"
auth_username = "admin"
auth_password = "secret123"

[[reverse_clients]]
id = "rev-cli-auth"
server_addr = "127.0.0.1:12345"
auth_username = "admin"
auth_password = "secret123"
default_target_host = "127.0.0.1"
default_target_port = 80
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_ok(),
        "auth config should parse: {:?}",
        result.err()
    );
}

#[test]
fn reverse_client_reconnect_config_parses() {
    let config = r#"
version = 1

[[reverse_clients]]
id = "rev-cli-reconnect"
server_addr = "127.0.0.1:12345"
reconnect_initial = "500ms"
reconnect_max = "10s"
default_target_host = "127.0.0.1"
default_target_port = 80
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_ok(),
        "reconnect config should parse: {:?}",
        result.err()
    );
}

#[test]
fn reverse_client_invalid_reconnect_fails() {
    let config = r#"
version = 1

[[reverse_clients]]
id = "rev-cli-bad-reconnect"
server_addr = "127.0.0.1:12345"
reconnect_initial = "not-a-duration"
default_target_host = "127.0.0.1"
default_target_port = 80
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(result.is_err(), "invalid reconnect duration should fail");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reverse_server_spawns_and_shuts_down() {
    let control_port = find_available_port().await;

    let config = format!(
        r#"
version = 1

[[reverse_servers]]
id = "rev-srv-test"
control_bind = "127.0.0.1:{}"
external_bind = "127.0.0.1:0"
"#,
        control_port.port(),
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap().to_string();

    let mut supervisor = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let state = supervisor.state().clone();
    let cancel_token = supervisor.shutdown_token();

    // Run the supervisor in a background thread
    let handle = tokio::task::spawn_blocking(move || supervisor.run());

    // Give it time to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify readiness
    assert!(
        state.readiness.load(Ordering::Relaxed),
        "supervisor should be ready"
    );

    // Verify reverse registry has the server registered
    let registry_snapshot = state.reverse_registry.snapshot();
    assert_eq!(
        registry_snapshot.len(),
        1,
        "reverse registry should have 1 entry"
    );
    assert_eq!(registry_snapshot[0].id, "rev-srv-test");

    // Shutdown via cancel token
    cancel_token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(3), handle).await;
    assert!(result.is_ok(), "supervisor should shut down within timeout");
    let inner = result.unwrap();
    assert!(
        inner.is_ok(),
        "supervisor should shut down cleanly: {:?}",
        inner.err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reverse_client_spawns_and_connects() {
    let control_port = find_available_port().await;

    let config = format!(
        r#"
version = 1

[[reverse_servers]]
id = "rev-srv"
control_bind = "127.0.0.1:{}"
external_bind = "127.0.0.1:0"

[[reverse_clients]]
id = "rev-cli"
server_addr = "127.0.0.1:{}"
default_target_host = "127.0.0.1"
default_target_port = 80
"#,
        control_port.port(),
        control_port.port(),
    );

    let f = write_config(&config);
    let path = f.path().to_str().unwrap().to_string();

    let mut supervisor = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let state = supervisor.state().clone();
    let cancel_token = supervisor.shutdown_token();

    // Run the supervisor in a background thread
    let handle = tokio::task::spawn_blocking(move || supervisor.run());

    // Give it time to start and for client to connect
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify readiness
    assert!(
        state.readiness.load(Ordering::Relaxed),
        "supervisor should be ready"
    );

    // Verify reverse registry has the server registered
    let registry_snapshot = state.reverse_registry.snapshot();
    assert!(
        !registry_snapshot.is_empty(),
        "reverse registry should have entries"
    );

    // Verify reverse metrics show activity
    let metrics_snap = state.reverse_metrics.snapshot();
    // The client should have attempted at least one connection
    assert!(
        metrics_snap.control_connections_accepted_total >= 1
            || metrics_snap.control_connections_rejected_total >= 1
            || metrics_snap.control_reconnects_total >= 1,
        "expected some reverse metric activity: {:?}",
        metrics_snap,
    );

    // Shutdown
    cancel_token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(3), handle).await;
    assert!(result.is_ok(), "supervisor should shut down within timeout");
    let inner = result.unwrap();
    assert!(
        inner.is_ok(),
        "supervisor should shut down cleanly: {:?}",
        inner.err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reverse_config_reload_succeeds() {
    let config1 = r#"
version = 1

[[reverse_servers]]
id = "rev-srv"
control_bind = "127.0.0.1:0"
external_bind = "127.0.0.1:0"
"#;

    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    // Reloading the same config should succeed (Applied)
    let result = sup.reload_config();
    assert!(
        matches!(
            result,
            eggress_runtime::supervisor::ReloadResult::Applied { .. }
        ),
        "reload should succeed, got: {:?}",
        result
    );
}

#[test]
fn reverse_server_missing_external_bind_fails() {
    let config = r#"
version = 1

[[reverse_servers]]
id = "rev-srv-no-external"
control_bind = "127.0.0.1:0"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(result.is_err(), "missing external_bind should fail");
}

#[test]
fn reverse_server_invalid_external_bind_fails() {
    let config = r#"
version = 1

[[reverse_servers]]
id = "rev-srv-bad-external"
control_bind = "127.0.0.1:0"
external_bind = "not-a-valid-address"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(result.is_err(), "invalid external_bind should fail");
}

#[test]
fn reverse_client_missing_target_fails() {
    let config = r#"
version = 1

[[reverse_clients]]
id = "rev-cli-no-target"
server_addr = "127.0.0.1:12345"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_err(),
        "missing default_target_host/port should fail"
    );
}

#[test]
fn reverse_client_partial_target_fails() {
    let config = r#"
version = 1

[[reverse_clients]]
id = "rev-cli-partial-target"
server_addr = "127.0.0.1:12345"
default_target_host = "127.0.0.1"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_err(),
        "only default_target_host without port should fail"
    );
}

#[test]
fn reverse_server_auth_mismatch_fails() {
    let config = r#"
version = 1

[[reverse_servers]]
id = "rev-srv-auth-mismatch"
control_bind = "127.0.0.1:0"
external_bind = "127.0.0.1:0"
auth_username = "admin"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_err(),
        "auth_username without password should fail"
    );
}

#[test]
fn reverse_client_invalid_server_addr_fails() {
    let config = r#"
version = 1

[[reverse_clients]]
id = "rev-cli-bad-addr"
server_addr = "not-a-valid-address"
default_target_host = "127.0.0.1"
default_target_port = 80
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(result.is_err(), "invalid server_addr should fail");
}

#[test]
fn reverse_server_password_env_var_not_set_fails() {
    let config = r#"
version = 1

[[reverse_servers]]
id = "rev-srv-missing-env"
control_bind = "127.0.0.1:0"
external_bind = "127.0.0.1:0"
auth_username = "admin"
auth_password_env = "EGGRESS_TEST_MISSING_VAR_98765"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    match result {
        Ok(_) => panic!("auth_password_env with unset var should fail at compile time"),
        Err(e) => {
            let msg = format!("{}", e);
            assert!(
                msg.contains("environment variable") && msg.contains("not set"),
                "error should mention env var not set: {}",
                msg
            );
        }
    }
}
