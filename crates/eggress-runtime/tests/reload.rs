use std::io::Write;

use tempfile::NamedTempFile;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[test]
fn valid_reload_changes_routing() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Applied {
            generation,
            upstreams,
        } => {
            assert_eq!(generation, 1);
            assert_eq!(upstreams, 0);
        }
        other => panic!("expected Applied, got {:?}", other),
    }
}

#[test]
fn invalid_reload_preserves_old_routing() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    let gen_before = sup.state().generation();

    // Write an invalid config to the same file path
    let invalid = "this is not valid toml {{{";
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path1)
            .unwrap();
        f.write_all(invalid.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Failed { error } => {
            assert!(
                error.contains("config") || error.contains("load"),
                "error should mention config issue: {}",
                error
            );
        }
        other => panic!("expected Failed for invalid config, got {:?}", other),
    }

    let gen_after = sup.state().generation();
    assert_eq!(
        gen_before, gen_after,
        "generation should not change on failed reload"
    );
}

#[test]
fn admin_generation_increments_on_reload() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let path1 = f1.path().to_str().unwrap();
    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    assert_eq!(sup.state().generation(), 0);

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Applied { generation, .. } => {
            assert_eq!(generation, 1);
            assert_eq!(sup.state().generation(), 1);
        }
        other => panic!("expected Applied, got {:?}", other),
    }

    let result2 = sup.reload_config();
    match result2 {
        eggress_runtime::supervisor::ReloadResult::Applied { generation, .. } => {
            assert_eq!(generation, 2);
            assert_eq!(sup.state().generation(), 2);
        }
        other => panic!("expected Applied, got {:?}", other),
    }
}

#[test]
fn unsupported_topology_change_is_rejected() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
    let f1 = write_config(config1);
    let f2 = write_config(config2);
    let path1 = f1.path().to_str().unwrap();
    let path2 = f2.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    // Write config2 content to the path that reload_config reads
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path1)
            .unwrap();
        let content = std::fs::read_to_string(path2).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Rejected { reason } => {
            assert!(
                reason.contains("listener count") || reason.contains("restart required"),
                "reason should mention topology change: {}",
                reason
            );
        }
        other => panic!("expected Rejected, got {:?}", other),
    }
}

#[test]
fn reload_rejects_listener_name_change() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "http-changed"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let f2 = write_config(config2);
    let path1 = f1.path().to_str().unwrap();
    let path2 = f2.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path1)
            .unwrap();
        let content = std::fs::read_to_string(path2).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Rejected { reason } => {
            assert!(
                reason.contains("name changed"),
                "reason should mention name change: {}",
                reason
            );
        }
        other => panic!("expected Rejected for name change, got {:?}", other),
    }
}

#[test]
fn reload_rejects_bind_address_change() {
    let config1 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let config2 = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:9090"
protocols = ["http"]
"#;
    let f1 = write_config(config1);
    let f2 = write_config(config2);
    let path1 = f1.path().to_str().unwrap();
    let path2 = f2.path().to_str().unwrap();

    let mut sup = eggress_runtime::ServiceSupervisor::start(path1).unwrap();

    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path1)
            .unwrap();
        let content = std::fs::read_to_string(path2).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let result = sup.reload_config();
    match result {
        eggress_runtime::supervisor::ReloadResult::Rejected { reason } => {
            assert!(
                reason.contains("bind"),
                "reason should mention bind change: {}",
                reason
            );
        }
        other => panic!("expected Rejected for bind change, got {:?}", other),
    }
}
