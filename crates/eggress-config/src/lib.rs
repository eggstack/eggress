pub mod compile;
pub mod error;
pub mod file;
pub mod model;
pub mod validate;

pub use compile::RuntimeConfig;
pub use error::ConfigError;

pub fn load_and_validate(path: &str) -> Result<RuntimeConfig, ConfigError> {
    let contents = file::load_config_file(path)?;
    let config: model::ConfigFile = toml::from_str(&contents)?;
    if let Some(version) = config.version {
        if version != 1 {
            return Err(ConfigError::UnsupportedVersion(version));
        }
    }
    validate::validate_config(&config).map_err(|errors| {
        let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        ConfigError::validation("config", &messages.join("; "))
    })?;
    compile::compile_config(&config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_config(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn minimal_valid_config() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
        let rt = result.unwrap();
        assert_eq!(rt.listeners.len(), 1);
        assert_eq!(rt.listeners[0].name, "http-in");
    }

    #[test]
    fn full_config_all_sections() {
        let config = r#"
version = 1

[process]
log_format = "json"
log_level = "debug"
shutdown_grace = "10s"

[timeouts]
handshake = "5s"
connect = "30s"

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http", "socks5"]
connection_limit = 1000

[listeners.auth]
type = "password"
username = "admin"
password = "secret"

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1.example:1080"

[[upstreams]]
id = "proxy2"
uri = "http://proxy2.example:8080"

[[upstream_groups]]
id = "main"
scheduler = "round-robin"
members = ["proxy1", "proxy2"]
fallback = "reject"

[[rules]]
id = "block-ads"
host_suffix = "ads.example.com"
reject = "blocked"

[[rules]]
id = "route-corp"
host_suffix = "corp.internal"
upstream_group = "main"

[[rules]]
id = "allow-all"
direct = true

[routing]
default = "direct"

[admin]
bind = "127.0.0.1:9090"
enabled = true
metrics = true
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
        let rt = result.unwrap();
        assert_eq!(rt.listeners.len(), 1);
        assert_eq!(rt.upstreams.len(), 2);
        assert_eq!(rt.groups.len(), 1);
        assert_eq!(rt.rules.len(), 3);
        assert!(rt.admin.is_some());
    }

    #[test]
    fn invalid_toml_syntax() {
        let config = r#"
version = 1
[[listeners
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ConfigError::Parse(_)),
            "expected Parse error, got {:?}",
            err
        );
    }

    #[test]
    fn unsupported_version() {
        let config = r#"
version = 2
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ConfigError::UnsupportedVersion(2)),
            "expected UnsupportedVersion, got {:?}",
            err
        );
    }

    #[test]
    fn invalid_duration_string() {
        let config = r#"
[timeouts]
handshake = "not-a-duration"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_uri() {
        let config = r#"
[[upstreams]]
id = "bad"
uri = "not-a-uri"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err());
    }

    #[test]
    fn duplicate_listener_names() {
        let config = r#"
[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8081"
protocols = ["http"]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err());
    }

    #[test]
    fn duplicate_upstream_ids() {
        let config = r#"
[[upstreams]]
id = "proxy1"
uri = "socks5://:1080"

[[upstreams]]
id = "proxy1"
uri = "http://:8080"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err());
    }

    #[test]
    fn duplicate_group_ids() {
        let config = r#"
[[upstreams]]
id = "proxy1"
uri = "socks5://:1080"

[[upstream_groups]]
id = "main"
members = ["proxy1"]

[[upstream_groups]]
id = "main"
members = ["proxy1"]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_group_reference_in_rule() {
        let config = r#"
[[rules]]
id = "r1"
host_exact = "example.com"
upstream_group = "nonexistent"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_member_reference_in_group() {
        let config = r#"
[[upstream_groups]]
id = "main"
members = ["nonexistent-proxy"]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err());
    }

    #[test]
    fn validation_errors_include_path_info() {
        let config = r#"
[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8081"
protocols = ["http"]
"#;
        let f = write_config(config);
        let toml_content = std::fs::read_to_string(f.path()).unwrap();
        let config_file: model::ConfigFile = toml::from_str(&toml_content).unwrap();
        let result = validate::validate_config(&config_file);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        let msg = format!("{}", errors[0]);
        assert!(
            msg.contains("listeners[1]"),
            "error should contain path: {}",
            msg
        );
    }

    #[test]
    fn missing_file() {
        let result = load_and_validate("/nonexistent/path/config.toml");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::Io(_)));
    }

    #[test]
    fn compile_process_defaults() {
        let config = model::ConfigFile {
            version: None,
            process: None,
            timeouts: None,
            listeners: None,
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
        };
        let rt = compile::compile_config(&config).unwrap();
        assert_eq!(rt.process.log_level, "info");
        assert_eq!(rt.process.log_format, "text");
        assert_eq!(
            rt.process.shutdown_grace,
            std::time::Duration::from_secs(30)
        );
    }

    #[test]
    fn compile_admin_defaults() {
        let config = model::ConfigFile {
            version: None,
            process: None,
            timeouts: None,
            listeners: None,
            upstreams: None,
            upstream_groups: None,
            rules: None,
            rules_file: None,
            routing: None,
            admin: None,
        };
        let rt = compile::compile_config(&config).unwrap();
        assert!(rt.admin.is_none());
    }

    #[test]
    fn parse_duration_seconds() {
        let d = validate::validate_duration("30s").unwrap();
        assert_eq!(d, std::time::Duration::from_secs(30));
    }

    #[test]
    fn parse_duration_millis() {
        let d = validate::validate_duration("500ms").unwrap();
        assert_eq!(d, std::time::Duration::from_millis(500));
    }

    #[test]
    fn parse_duration_minutes() {
        let d = validate::validate_duration("5m").unwrap();
        assert_eq!(d, std::time::Duration::from_secs(300));
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(validate::validate_duration("abc").is_err());
    }

    #[test]
    fn empty_config_valid() {
        let config = "";
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "empty config should be valid: {:?}",
            result.err()
        );
    }

    #[test]
    fn invalid_host_regex() {
        let config = r#"
[[rules]]
id = "bad-regex"
host_regex = "[invalid"
direct = true
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err());
    }

    #[test]
    fn recursive_match_all() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[rules]]
id = "block-internal-https"
direct = true

[rules.match]
all = [
  { host_suffix = "corp.internal" },
  { destination_port = 443 },
]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "recursive match all should compile: {:?}",
            result.err()
        );
        let rt = result.unwrap();
        assert_eq!(rt.rules.len(), 1);
    }

    #[test]
    fn recursive_match_any_of() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[rules]]
id = "http-or-socks"
direct = true

[rules.match]
any_of = [
  { protocol = "http" },
  { protocol = "socks5" },
]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "recursive match any_of should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn recursive_match_not() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[rules]]
id = "not-internal"
direct = true

[rules.match]
not = { source_cidr = "10.0.0.0/8" }
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "recursive match not should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn leaf_matcher_port_range() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[rules]]
id = "high-ports"
direct = true

[rules.match]
destination_port_range = [8000, 9000]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "port range should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn leaf_matcher_port_set() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[rules]]
id = "specific-ports"
direct = true

[rules.match]
destination_port_set = [80, 443, 8080]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "port set should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn leaf_matcher_identity() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[rules]]
id = "admin-only"
direct = true

[rules.match]
identity = "admin"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "identity matcher should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn nested_composite_matchers() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[rules]]
id = "nested-example"
direct = true

[rules.match]
all = [
  { host_suffix = "example.com" },
  { any_of = [
      { destination_port = 443 },
      { destination_port = 8443 },
  ]},
  { not = { source_cidr = "10.0.0.0/8" } }
]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "nested composite matchers should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn invalid_cidr_in_match_expr() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[rules]]
id = "bad-cidr"
direct = true

[rules.match]
source_cidr = "not-a-cidr"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err(), "invalid CIDR should fail validation");
    }

    #[test]
    fn invalid_regex_in_match_expr() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[[rules]]
id = "bad-regex"
direct = true

[rules.match]
host_regex = "[invalid"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err(), "invalid regex should fail validation");
    }

    #[test]
    fn health_config_all_fields_compiles() {
        let config = r#"
version = 1

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1:1080"

[upstreams.health]
mode = "tcp_connect"
interval = "15s"
timeout = "3s"
failures_to_unhealthy = 5
successes_to_healthy = 3
initial_state = "healthy"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "health config with all fields should compile: {:?}",
            result.err()
        );
        let rt = result.unwrap();
        assert_eq!(rt.upstreams.len(), 1);
        let health = &rt.upstreams[0].health;
        assert_eq!(health.interval, std::time::Duration::from_secs(15));
        assert_eq!(health.timeout, std::time::Duration::from_secs(3));
        assert_eq!(health.failures_to_unhealthy, 5);
        assert_eq!(health.successes_to_healthy, 3);
        assert_eq!(
            health.initial_state,
            eggress_routing::health::HealthState::Healthy
        );
    }

    #[test]
    fn health_config_missing_uses_defaults() {
        let config = r#"
version = 1

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1:1080"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "missing health config should use defaults: {:?}",
            result.err()
        );
        let rt = result.unwrap();
        let health = &rt.upstreams[0].health;
        assert_eq!(health.interval, std::time::Duration::from_secs(30));
        assert_eq!(health.timeout, std::time::Duration::from_secs(5));
        assert_eq!(health.failures_to_unhealthy, 3);
        assert_eq!(health.successes_to_healthy, 2);
        assert_eq!(
            health.initial_state,
            eggress_routing::health::HealthState::Unknown
        );
    }

    #[test]
    fn health_config_partial_fields_use_defaults() {
        let config = r#"
version = 1

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1:1080"

[upstreams.health]
interval = "10s"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "partial health config should compile: {:?}",
            result.err()
        );
        let rt = result.unwrap();
        let health = &rt.upstreams[0].health;
        assert_eq!(health.interval, std::time::Duration::from_secs(10));
        assert_eq!(health.timeout, std::time::Duration::from_secs(5));
    }

    #[test]
    fn health_config_invalid_interval_rejected() {
        let config = r#"
version = 1

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1:1080"

[upstreams.health]
interval = "not-a-duration"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err(), "invalid health interval should fail");
    }

    #[test]
    fn health_config_invalid_timeout_rejected() {
        let config = r#"
version = 1

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1:1080"

[upstreams.health]
timeout = "abc"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err(), "invalid health timeout should fail");
    }

    #[test]
    fn health_config_zero_failures_rejected() {
        let config = r#"
version = 1

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1:1080"

[upstreams.health]
failures_to_unhealthy = 0
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err(), "zero failures_to_unhealthy should fail");
    }

    #[test]
    fn health_config_zero_successes_rejected() {
        let config = r#"
version = 1

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1:1080"

[upstreams.health]
successes_to_healthy = 0
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err(), "zero successes_to_healthy should fail");
    }

    #[test]
    fn health_config_invalid_initial_state_rejected() {
        let config = r#"
version = 1

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1:1080"

[upstreams.health]
initial_state = "bogus"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err(), "invalid initial_state should fail");
    }

    #[test]
    fn health_config_invalid_mode_rejected() {
        let config = r#"
version = 1

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1:1080"

[upstreams.health]
mode = "http_get"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err(), "invalid health mode should fail");
    }

    #[test]
    fn health_config_disabled_initial_state() {
        let config = r#"
version = 1

[[upstreams]]
id = "proxy1"
uri = "socks5://proxy1:1080"

[upstreams.health]
initial_state = "disabled"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "disabled initial_state should be valid: {:?}",
            result.err()
        );
        let rt = result.unwrap();
        assert_eq!(
            rt.upstreams[0].health.initial_state,
            eggress_routing::health::HealthState::Disabled
        );
    }

    #[test]
    fn pac_config_compiles() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[admin]
enabled = true

[admin.pac]
path = "/proxy.pac"
proxy = "127.0.0.1:8080"
direct_fallback = true
direct_hosts = ["localhost"]
direct_suffixes = ["local"]
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "PAC config should compile: {:?}",
            result.err()
        );
        let rt = result.unwrap();
        let admin = rt.admin.unwrap();
        let pac = admin.pac.unwrap();
        assert_eq!(pac.path, "/proxy.pac");
        assert_eq!(pac.proxy_directive, "127.0.0.1:8080");
        assert!(pac.direct_fallback);
        assert_eq!(pac.direct_hosts, vec!["localhost".to_string()]);
        assert_eq!(pac.direct_suffixes, vec!["local".to_string()]);
    }

    #[test]
    fn static_content_config_compiles() {
        let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:8080"
protocols = ["http"]

[admin]
enabled = true

[[admin.static_content]]
path = "/status"
content_type = "text/html"
body = "<h1>OK</h1>"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_ok(),
            "static content config should compile: {:?}",
            result.err()
        );
        let rt = result.unwrap();
        let admin = rt.admin.unwrap();
        assert_eq!(admin.static_content.len(), 1);
        assert_eq!(admin.static_content[0].path, "/status");
        assert_eq!(admin.static_content[0].content_type, "text/html");
        assert_eq!(admin.static_content[0].body, "<h1>OK</h1>");
    }

    #[test]
    fn duplicate_static_paths_rejected() {
        let config = r#"
version = 1

[[admin.static_content]]
path = "/foo"
body = "a"

[[admin.static_content]]
path = "/foo"
body = "b"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err(), "duplicate static paths should be rejected");
    }

    #[test]
    fn reserved_path_collision_rejected() {
        let config = r#"
version = 1

[[admin.static_content]]
path = "/metrics"
body = "override"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_err(),
            "reserved path collision should be rejected"
        );
    }

    #[test]
    fn pac_path_must_start_with_slash() {
        let config = r#"
version = 1

[admin.pac]
path = "proxy.pac"
proxy = "127.0.0.1:8080"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_err(),
            "PAC path not starting with / should be rejected"
        );
    }

    #[test]
    fn static_path_must_start_with_slash() {
        let config = r#"
version = 1

[[admin.static_content]]
path = "status"
body = "ok"
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(
            result.is_err(),
            "static path not starting with / should be rejected"
        );
    }

    #[test]
    fn static_empty_body_rejected() {
        let config = r#"
version = 1

[[admin.static_content]]
path = "/status"
body = ""
"#;
        let f = write_config(config);
        let path = f.path().to_str().unwrap();
        let result = load_and_validate(path);
        assert!(result.is_err(), "empty body should be rejected");
    }
}
