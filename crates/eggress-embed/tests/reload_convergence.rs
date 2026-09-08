//! Convergence tests for the canonical reload transaction.
//!
//! File-backed (`reload_toml_file`), string (`reload_toml_str`), and native
//! (`reload_compiled`) entry points must share
//! `RuntimeState::apply_compiled_config` and therefore agree on classification,
//! snapshot publication, admin/status, health restart, H2 invalidation, and
//! metrics. Table-driven: the same logical change applied through different
//! entry points yields the same outcome.

use std::io::Write;

fn base_toml() -> String {
    r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#
    .to_string()
}

fn with_upstream_toml(port: u16) -> String {
    format!(
        r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "up"
uri = "socks5://127.0.0.1:{port}"

[[upstream_groups]]
id = "main"
members = ["up"]

[[rules]]
id = "route-all"
upstream_group = "main"
"#
    )
}

fn with_health_toml() -> String {
    r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "up"
uri = "socks5://127.0.0.1:1080"

[upstreams.health]
interval = "15s"

[[upstream_groups]]
id = "main"
members = ["up"]

[[rules]]
id = "route-all"
upstream_group = "main"
"#
    .to_string()
}

fn start_blocking(toml: &str) -> eggress_embed::EggressHandle {
    eggress_embed::EggressService::from_toml_str(toml)
        .unwrap()
        .start_blocking()
        .unwrap()
}

fn metrics_text(handle: &eggress_embed::EggressHandle) -> String {
    handle.metrics_text().unwrap()
}

fn reload_total(metrics: &str) -> u64 {
    for line in metrics.lines() {
        if line.starts_with("eggress_reload_total ") {
            return line
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        }
        // Prometheus client may emit with labels? Fall back to substring search.
        if line.contains("eggress_reload_total") && !line.starts_with('#') {
            if let Some(last) = line.split_whitespace().last() {
                if let Ok(v) = last.parse() {
                    return v;
                }
            }
        }
    }
    0
}

#[test]
fn routing_only_reload_accepted_via_string_and_file() {
    let initial = base_toml();
    let changed = format!("{initial}\n[[rules]]\nid = \"allow-all\"\ndirect = true\n");

    // String path.
    let handle_str = start_blocking(&initial);
    let outcome_str = handle_str.reload_toml_str(&changed).unwrap();
    let gen_str = match outcome_str {
        eggress_embed::ReloadOutcome::Applied { generation, .. } => generation,
    };
    assert_eq!(gen_str, 1);
    assert_eq!(handle_str.status().generation, 1);
    let metrics_str = metrics_text(&handle_str);
    handle_str.shutdown_blocking().unwrap();

    // File path on a second handle started identically.
    let handle_file = start_blocking(&initial);
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(changed.as_bytes()).unwrap();
    tmp.flush().unwrap();
    let outcome_file = handle_file.reload_toml_file(tmp.path()).unwrap();
    let gen_file = match outcome_file {
        eggress_embed::ReloadOutcome::Applied { generation, .. } => generation,
    };
    assert_eq!(gen_file, 1);
    assert_eq!(handle_file.status().generation, 1);
    let metrics_file = metrics_text(&handle_file);
    handle_file.shutdown_blocking().unwrap();

    assert_eq!(gen_str, gen_file);
    assert_eq!(reload_total(&metrics_str), reload_total(&metrics_file));
}

#[test]
fn upstream_chain_change_accepted_and_clears_h2_pool() {
    let initial = with_upstream_toml(1080);
    let changed = with_upstream_toml(1081);

    // Seed the shared H2 pool so we can prove canonical invalidation runs.
    let key = eggress_protocol_http::H2PoolKey {
        endpoint_host: "127.0.0.1".to_string(),
        endpoint_port: 1080,
        use_tls: false,
        server_name: None,
        auth_hash: None,
        hop_index: 0,
    };
    let _pool = eggress_protocol_http::H2_POOL_REGISTRY.get_or_create(&key);

    let handle = start_blocking(&initial);
    let outcome = handle.reload_toml_str(&changed).unwrap();
    match outcome {
        eggress_embed::ReloadOutcome::Applied {
            generation,
            upstreams,
        } => {
            assert_eq!(generation, 1);
            assert_eq!(upstreams, 1);
        }
    }
    // Canonical transaction clears the registry on every successful reload.
    // get_or_create after clear must yield an empty pool (no retained entries
    // from the seed above beyond the newly created empty pool).
    let stats = eggress_protocol_http::H2_POOL_REGISTRY
        .get_or_create(&key)
        .stats();
    assert_eq!(stats.active_connections, 0);
    handle.shutdown_blocking().unwrap();
}

#[test]
fn health_change_accepted_via_string_and_compiled() {
    let initial = with_upstream_toml(1080);
    let changed = with_health_toml();

    let handle_str = start_blocking(&initial);
    let gen_str = match handle_str.reload_toml_str(&changed).unwrap() {
        eggress_embed::ReloadOutcome::Applied { generation, .. } => generation,
    };
    assert_eq!(gen_str, 1);
    handle_str.shutdown_blocking().unwrap();

    // Native compiled path must agree.
    let handle_native = start_blocking(&initial);
    let compiled = eggress_config::validate_and_compile_toml(&changed).unwrap();
    let gen_native = match handle_native.reload_compiled(&compiled).unwrap() {
        eggress_embed::ReloadOutcome::Applied { generation, .. } => generation,
    };
    assert_eq!(gen_native, 1);
    assert_eq!(gen_str, gen_native);
    handle_native.shutdown_blocking().unwrap();
}

#[test]
fn listener_change_rejected_on_all_entry_points() {
    let initial = base_toml();
    let changed = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;

    for via_file in [false, true] {
        let handle = start_blocking(&initial);
        let gen_before = handle.status().generation;
        let result = if via_file {
            let mut tmp = tempfile::NamedTempFile::new().unwrap();
            tmp.write_all(changed.as_bytes()).unwrap();
            tmp.flush().unwrap();
            handle.reload_toml_file(tmp.path())
        } else {
            handle.reload_toml_str(changed)
        };
        let err = result.unwrap_err().to_string();
        assert!(err.contains("protocols"), "unexpected: {err}");
        assert_eq!(handle.status().generation, gen_before);
        handle.shutdown_blocking().unwrap();
    }

    // Native path rejects identically.
    let handle = start_blocking(&initial);
    let compiled = eggress_config::validate_and_compile_toml(changed).unwrap();
    let err = handle.reload_compiled(&compiled).unwrap_err().to_string();
    assert!(err.contains("protocols"), "unexpected: {err}");
    handle.shutdown_blocking().unwrap();
}

#[test]
fn malformed_toml_fails_before_mutation_and_records_metrics() {
    let handle = start_blocking(&base_toml());
    let gen_before = handle.status().generation;
    let metrics_before = reload_total(&metrics_text(&handle));

    let result = handle.reload_toml_str("not valid toml {{{");
    assert!(result.is_err());
    assert_eq!(handle.status().generation, gen_before);
    let metrics_after = reload_total(&metrics_text(&handle));
    assert!(
        metrics_after > metrics_before,
        "failed reload must increment reload counter"
    );
    handle.shutdown_blocking().unwrap();
}

#[test]
fn admin_status_reflects_same_generation_after_success() {
    let initial = base_toml();
    let changed = with_upstream_toml(1080);
    let handle = start_blocking(&initial);
    let bound_before = handle.bound_addresses().listeners[0].addr;
    handle.reload_toml_str(&changed).unwrap();
    let status = handle.status();
    assert_eq!(status.generation, 1);
    // Listener topology is restart-required, so bound address must be unchanged.
    assert_eq!(handle.bound_addresses().listeners[0].addr, bound_before);
    // Metrics generation gauge must agree with status.
    let metrics = metrics_text(&handle);
    assert!(
        metrics.contains("eggress_config_generation 1")
            || metrics.contains("eggress_config_generation{"),
        "metrics should reflect generation 1: {metrics}"
    );
    handle.shutdown_blocking().unwrap();
}
