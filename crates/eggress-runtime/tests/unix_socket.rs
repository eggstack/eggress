//! Unix domain socket integration tests.
//!
//! These tests verify Unix domain socket listener support.

use std::io::Write;
use std::path::PathBuf;

use tempfile::NamedTempFile;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[cfg(unix)]
fn toml_path(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn unix_socket_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir();
    let id: u64 = fastrand::u64(..);
    dir.join(format!("eggress_test_{name}_{id}.sock"))
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_unix_listener_bind_and_accept() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let path = unix_socket_path("bind_accept");
    let listener = eggress_server::listener::unix::UnixListener::bind(&path, true).unwrap();
    let addr = path.clone();

    let connect_handle = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        tokio::net::UnixStream::connect(&addr).await.unwrap()
    });

    let (mut stream, _peer) = listener.accept().await.unwrap();
    let mut connected = connect_handle.await.unwrap();

    stream.write_all(b"hello-unix").await.unwrap();
    stream.flush().await.unwrap();

    let mut buf = [0u8; 16];
    let n = connected.read(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"hello-unix");

    drop(stream);
    drop(connected);
    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_unix_listener_cleanup() {
    let path = unix_socket_path("cleanup");
    {
        let _listener = eggress_server::listener::unix::UnixListener::bind(&path, true).unwrap();
        assert!(
            path.exists(),
            "socket file should exist while listener is alive"
        );
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let result = eggress_server::listener::unix::UnixListener::bind(&path, true);
    assert!(
        result.is_ok(),
        "should be able to re-bind after cleanup, got {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_unix_listener_unlink_existing_replaces_socket() {
    let path = unix_socket_path("unlink_socket");

    // Create a stale *socket* (not a regular file). UnixListener must
    // replace it when unlink_existing=true.
    let stale = eggress_server::listener::unix::UnixListener::bind(&path, true).unwrap();
    stale.cleanup().unwrap();
    // Stale cleanup removed the socket; now rebind with unlink_existing=true.
    let result = eggress_server::listener::unix::UnixListener::bind(&path, true);
    assert!(
        result.is_ok(),
        "bind with unlink_existing=true should succeed when only a socket existed, got {:?}",
        result.err()
    );
    if let Ok(l) = result {
        l.cleanup().ok();
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_unix_listener_unlink_existing_refuses_regular_file() {
    let path = unix_socket_path("unlink_regular");

    std::fs::write(&path, b"important data").unwrap();
    assert!(path.exists(), "regular file should exist");

    let result = eggress_server::listener::unix::UnixListener::bind(&path, true);
    assert!(
        result.is_err(),
        "bind must refuse to unlink regular files (safety check)"
    );
    // The file must still exist and be intact.
    let contents = std::fs::read_to_string(&path).unwrap();
    assert_eq!(contents, "important data");

    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_unix_listener_no_unlink_existing_fails_on_stale() {
    let path = unix_socket_path("no_unlink");

    std::fs::write(&path, b"stale").unwrap();
    assert!(path.exists());

    let result = eggress_server::listener::unix::UnixListener::bind(&path, false);
    assert!(
        result.is_err(),
        "bind with unlink_existing=false should fail on stale socket"
    );
    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_unix_listener_via_config() {
    let path = unix_socket_path("config");
    let config = format!(
        r#"
version = 1

[[listeners]]
name = "socks-unix"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.unix]
path = "{}"
unlink_existing = true
"#,
        path.display()
    );
    let f = write_config(&config);
    let config_path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(config_path);
    assert!(
        result.is_ok(),
        "unix socket config should be valid, got {:?}",
        result.err()
    );

    if let Ok(mut sup) = result {
        let token = sup.shutdown_token();
        let jh = tokio::task::spawn_blocking(move || sup.run());
        token.cancel();
        jh.await.ok();
    }

    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn test_unix_listener_invalid_path_no_parent() {
    use std::path::Path;
    let result =
        eggress_server::listener::unix::UnixListener::bind(Path::new("relative-only.sock"), true);
    assert!(result.is_err(), "relative path without parent should fail");
}

#[cfg(unix)]
#[test]
fn test_unix_listener_config_defaults() {
    let config = eggress_server::listener::unix::UnixListenerConfig::new("/tmp/test.sock");
    assert_eq!(config.path, PathBuf::from("/tmp/test.sock"));
    // new() defaults unlink_existing to true
    assert!(
        config.unlink_existing,
        "default unlink_existing should be true"
    );
}

#[cfg(unix)]
#[test]
fn test_unix_listener_config_from_compiled() {
    let config = eggress_server::listener::unix::UnixListenerConfig::from_compiled(
        std::path::Path::new("/tmp/compiled.sock"),
        true,
        Some(0o660),
    );
    assert_eq!(config.path, PathBuf::from("/tmp/compiled.sock"));
    assert!(config.unlink_existing);
    assert_eq!(config.mode, Some(0o660));
}

#[test]
fn test_unsupported_platform_error() {
    let status = eggress_runtime::platform::check_capability(
        eggress_runtime::platform::PlatformCapability::UnixDomainSockets,
    );
    match std::env::consts::OS {
        "linux" | "macos" | "freebsd" | "openbsd" | "netbsd" => {
            assert_eq!(
                status,
                eggress_runtime::platform::CapabilityStatus::Available,
                "Unix domain sockets should be available on {}",
                std::env::consts::OS
            );
        }
        _ => {
            assert_eq!(
                status,
                eggress_runtime::platform::CapabilityStatus::UnsupportedPlatform,
            );
        }
    }
}
