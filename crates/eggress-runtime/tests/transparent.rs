//! Transparent proxy integration tests.
//!
//! These tests verify transparent proxy configuration and behavior.
//! Gated tests require root and iptables, run with:
//! EGRESS_REQUIRE_TRANSPARENT_INTEROP=1

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
fn test_transparent_config_validation() {
    let config = r#"
version = 1

[[listeners]]
name = "redir-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.transparent]
enabled = true
protocol = "redir"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_ok(),
        "transparent config should be valid, got {:?}",
        result.err()
    );
}

#[test]
fn test_transparent_config_disabled_passes_validation() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.transparent]
enabled = false
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_ok(),
        "disabled transparent config should be valid, got {:?}",
        result.err()
    );
}

#[test]
fn test_transparent_capability_check_macos() {
    let status = eggress_runtime::platform::check_capability(
        eggress_runtime::platform::PlatformCapability::MacosPfOriginalDst,
    );
    match std::env::consts::OS {
        "macos" => {
            assert!(
                matches!(
                    status,
                    eggress_runtime::platform::CapabilityStatus::Available
                        | eggress_runtime::platform::CapabilityStatus::MissingPrivilege
                        | eggress_runtime::platform::CapabilityStatus::KernelUnsupported
                ),
                "unexpected macOS PF status: {:?}",
                status
            );
        }
        "linux" => {
            assert_eq!(
                status,
                eggress_runtime::platform::CapabilityStatus::UnsupportedPlatform,
                "Linux should report UnsupportedPlatform for macOS PF"
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

#[test]
fn test_transparent_capability_check_linux_original_dst() {
    let status = eggress_runtime::platform::check_capability(
        eggress_runtime::platform::PlatformCapability::LinuxOriginalDstIpv4,
    );
    match std::env::consts::OS {
        "linux" => {
            assert!(
                matches!(
                    status,
                    eggress_runtime::platform::CapabilityStatus::Available
                        | eggress_runtime::platform::CapabilityStatus::KernelUnsupported
                        | eggress_runtime::platform::CapabilityStatus::MissingPrivilege
                ),
                "unexpected Linux original dst status: {:?}",
                status
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

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_original_destination_recovery_mock() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stream = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let result = eggress_server::listener::transparent::get_original_destination(&stream);
    // On kernels with SO_ORIGINAL_DST support, the call may succeed and return
    // the listener's address even without iptables redirect. Accept both
    // NoOriginalDestination and Ok(addr) as valid outcomes.
    match result {
        Err(eggress_server::listener::transparent::TransparentError::NoOriginalDestination) => {}
        Ok(addr) => {
            // Kernel returned original destination — valid on some configurations
            assert_eq!(
                addr,
                listener.local_addr().unwrap(),
                "returned address should match listener"
            );
        }
        other => panic!(
            "expected NoOriginalDestination or Ok(listener_addr), got: {:?}",
            other
        ),
    }
}

#[cfg(not(target_os = "linux"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_original_destination_returns_unsupported_on_non_linux() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stream = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let result = eggress_server::listener::transparent::get_original_destination(&stream);
    assert!(
        matches!(
            result,
            Err(eggress_server::listener::transparent::TransparentError::UnsupportedPlatform)
        ),
        "expected UnsupportedPlatform on non-Linux, got: {:?}",
        result
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_transparent_metrics_initialized() {
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

    assert_eq!(
        state.transparent_accepted_total.load(Ordering::Relaxed),
        0,
        "transparent_accepted_total should start at 0"
    );
    assert_eq!(
        state
            .transparent_original_dst_failed_total
            .load(Ordering::Relaxed),
        0,
        "transparent_original_dst_failed_total should start at 0"
    );

    let token = sup.shutdown_token();
    let jh = tokio::task::spawn_blocking(move || sup.run());
    token.cancel();
    jh.await.ok();
}

#[test]
fn test_transparent_reject_invalid_protocol() {
    let config = r#"
version = 1

[[listeners]]
name = "redir-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.transparent]
enabled = true
protocol = "invalid-protocol"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_err(),
        "invalid transparent protocol should fail validation"
    );
}

#[test]
fn test_transparent_pf_protocol_accepted() {
    let config = r#"
version = 1

[[listeners]]
name = "pf-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.transparent]
enabled = true
protocol = "pf"
"#;
    let f = write_config(config);
    let path = f.path().to_str().unwrap();
    let result = eggress_runtime::ServiceSupervisor::start(path);
    assert!(
        result.is_ok(),
        "pf protocol should be accepted in config, got {:?}",
        result.err()
    );
}
