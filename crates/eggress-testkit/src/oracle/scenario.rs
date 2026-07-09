//! Oracle scenario definitions and registry.
//!
//! Each scenario describes an equivalent pproxy/eggress configuration pair
//! and a client action to exercise. The runner executes both sides and
//! compares normalized outputs.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Target equivalence class for comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquivalenceTarget {
    /// Both should produce identical output bytes.
    Payload,
    /// Both should succeed or both should fail (exact error may differ).
    CoarseResult,
    /// Both should expose the same port/protocol binding.
    BindAddress,
    /// Both should produce the same HTTP status code.
    StatusCode,
}

/// Platform requirements for a scenario.
#[derive(Debug, Clone, Default)]
pub struct PlatformRequirements {
    pub requires_root: bool,
    pub requires_ipv6: bool,
    pub requires_python_package: Option<String>,
    pub required_os: Option<&'static str>,
}

/// Normalization rules applied before comparison.
#[derive(Debug, Clone, Default)]
pub struct NormalizationRules {
    /// Strip pproxy-specific log prefixes from stderr.
    pub strip_log_prefixes: bool,
    /// Normalize port numbers (replace dynamic ports with placeholder).
    pub normalize_ports: bool,
    /// Normalize line endings.
    pub normalize_line_endings: bool,
    /// Strip version strings.
    pub strip_versions: bool,
}

/// A single oracle scenario definition.
#[derive(Debug, Clone)]
pub struct OracleScenario {
    /// Unique scenario identifier.
    pub id: &'static str,
    /// Capability IDs this scenario exercises.
    pub capability_ids: Vec<&'static str>,
    /// Human-readable description.
    pub description: &'static str,
    /// pproxy CLI arguments (excluding `-m pproxy`).
    pub pproxy_args: Vec<&'static str>,
    /// eggress TOML configuration (with `{PORT}` and `{ECHO_PORT}` placeholders).
    pub eggress_toml: &'static str,
    /// Expected equivalence target.
    pub expected_equivalence: EquivalenceTarget,
    /// Normalization rules for this scenario.
    pub normalization: NormalizationRules,
    /// Platform requirements.
    pub platform: PlatformRequirements,
    /// Maximum time for the full scenario execution.
    pub timeout: Duration,
    /// Scenario category for grouping.
    pub category: ScenarioCategory,
}

/// Scenario categories for grouping and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioCategory {
    /// CLI defaults and basic protocol listeners.
    CliDefaults,
    /// HTTP and SOCKS TCP connect scenarios.
    HttpSocksTcp,
    /// Proxy chaining scenarios.
    Chains,
    /// Rule-based routing scenarios.
    Rules,
    /// UDP relay scenarios.
    Udp,
}

/// Get all registered oracle scenarios.
pub fn all_scenarios() -> Vec<OracleScenario> {
    let mut scenarios = Vec::new();
    scenarios.extend(cli_defaults_scenarios());
    scenarios.extend(http_socks_tcp_scenarios());
    scenarios.extend(chain_scenarios());
    scenarios.extend(rule_scenarios());
    scenarios.extend(udp_scenarios());
    scenarios
}

/// Get scenarios by category.
pub fn scenarios_for_category(category: ScenarioCategory) -> Vec<OracleScenario> {
    all_scenarios()
        .into_iter()
        .filter(|s| s.category == category)
        .collect()
}

/// Get a scenario by ID.
pub fn find_scenario(id: &str) -> Option<OracleScenario> {
    all_scenarios().into_iter().find(|s| s.id == id)
}

// ===== CLI/Defaults (7 scenarios) =====

fn cli_defaults_scenarios() -> Vec<OracleScenario> {
    vec![
        OracleScenario {
            id: "cli.socks5_default",
            capability_ids: vec!["cli.socks5_default", "uri.socks5"],
            description: "SOCKS5 listener on default port with direct routing",
            pproxy_args: vec!["-l", "socks5://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::CliDefaults,
        },
        OracleScenario {
            id: "cli.socks4_default",
            capability_ids: vec!["cli.socks4_default", "uri.socks4"],
            description: "SOCKS4 listener on default port with direct routing",
            pproxy_args: vec!["-l", "socks4://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks4"]
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::CliDefaults,
        },
        OracleScenario {
            id: "cli.http_default",
            capability_ids: vec!["cli.http_default", "uri.http"],
            description: "HTTP CONNECT listener on default port with direct routing",
            pproxy_args: vec!["-l", "http://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["http"]
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::CliDefaults,
        },
        OracleScenario {
            id: "cli.https_default",
            capability_ids: vec!["cli.https_default", "uri.https"],
            description: "HTTPS (TLS) CONNECT listener with direct routing",
            pproxy_args: vec!["-l", "https://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["https"]
tls.cert = "tests/fixtures/cert.pem"
tls.key = "tests/fixtures/key.pem"
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::CliDefaults,
        },
        OracleScenario {
            id: "cli.ss_default",
            capability_ids: vec!["cli.ss_default", "uri.ss"],
            description: "Shadowsocks listener with direct routing",
            pproxy_args: vec![
                "-l",
                "ss://127.0.0.1:{PORT}#testuser:testpass",
                "-r",
                "direct",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["shadowsocks"]
password = "testpass"
method = "aes-256-gcm"
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::CliDefaults,
        },
        OracleScenario {
            id: "cli.trojan_default",
            capability_ids: vec!["cli.trojan_default", "uri.trojan"],
            description: "Trojan listener with direct routing",
            pproxy_args: vec!["-l", "trojan://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["trojan"]
password = "password"
tls.cert = "tests/fixtures/cert.pem"
tls.key = "tests/fixtures/key.pem"
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::CliDefaults,
        },
        OracleScenario {
            id: "cli.mixed_default",
            capability_ids: vec!["cli.mixed_default"],
            description: "Mixed SOCKS5+HTTP listener on single port",
            pproxy_args: vec![
                "-l",
                "socks5://127.0.0.1:{PORT}",
                "-l",
                "http://127.0.0.1:{PORT2}",
                "-r",
                "direct",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5", "http"]
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::CliDefaults,
        },
    ]
}

// ===== HTTP/SOCKS TCP (10 scenarios) =====

fn http_socks_tcp_scenarios() -> Vec<OracleScenario> {
    vec![
        OracleScenario {
            id: "tcp.http_connect",
            capability_ids: vec!["protocol.http_connect"],
            description: "HTTP CONNECT to TCP echo server",
            pproxy_args: vec!["-l", "http://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["http"]
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::HttpSocksTcp,
        },
        OracleScenario {
            id: "tcp.socks4_connect",
            capability_ids: vec!["protocol.socks4_connect"],
            description: "SOCKS4 CONNECT to TCP echo server",
            pproxy_args: vec!["-l", "socks4://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks4"]
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::HttpSocksTcp,
        },
        OracleScenario {
            id: "tcp.socks4a_connect",
            capability_ids: vec!["protocol.socks4a_connect"],
            description: "SOCKS4a CONNECT (domain name) to TCP echo server",
            pproxy_args: vec!["-l", "socks4a://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks4a"]
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::HttpSocksTcp,
        },
        OracleScenario {
            id: "tcp.socks5_connect",
            capability_ids: vec!["protocol.socks5_connect"],
            description: "SOCKS5 CONNECT to TCP echo server",
            pproxy_args: vec!["-l", "socks5://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::HttpSocksTcp,
        },
        OracleScenario {
            id: "tcp.socks5_auth",
            capability_ids: vec!["protocol.socks5_auth"],
            description: "SOCKS5 CONNECT with username/password auth",
            pproxy_args: vec!["-l", "socks5://127.0.0.1:{PORT}#user:pass", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "user"
password = "pass"
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::HttpSocksTcp,
        },
        OracleScenario {
            id: "tcp.socks5_connect_domain",
            capability_ids: vec!["protocol.socks5_connect_domain"],
            description: "SOCKS5 CONNECT via domain name target",
            pproxy_args: vec!["-l", "socks5://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::HttpSocksTcp,
        },
        OracleScenario {
            id: "tcp.socks5_refused",
            capability_ids: vec!["protocol.socks5_refused"],
            description: "SOCKS5 CONNECT to refused port (negative case)",
            pproxy_args: vec!["-l", "socks5://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::HttpSocksTcp,
        },
        OracleScenario {
            id: "tcp.http_forward_get",
            capability_ids: vec!["protocol.http_forward_get"],
            description: "HTTP forward proxy GET request",
            pproxy_args: vec!["-l", "http://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["http"]
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                normalize_line_endings: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::HttpSocksTcp,
        },
        OracleScenario {
            id: "tcp.http_forward_post",
            capability_ids: vec!["protocol.http_forward_post"],
            description: "HTTP forward proxy POST request",
            pproxy_args: vec!["-l", "http://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["http"]
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                normalize_line_endings: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::HttpSocksTcp,
        },
        OracleScenario {
            id: "tcp.socks5_auth_failure",
            capability_ids: vec!["protocol.socks5_auth_failure"],
            description: "SOCKS5 with wrong credentials (negative case)",
            pproxy_args: vec!["-l", "socks5://127.0.0.1:{PORT}#user:pass", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "user"
password = "pass"
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::HttpSocksTcp,
        },
    ]
}

// ===== Chains (5 scenarios) =====

fn chain_scenarios() -> Vec<OracleScenario> {
    vec![
        OracleScenario {
            id: "chain.socks5_to_socks5",
            capability_ids: vec!["routing.chain_socks5_socks5"],
            description: "SOCKS5 chained through another SOCKS5 upstream",
            pproxy_args: vec![
                "-l",
                "socks5://127.0.0.1:{PORT}",
                "-r",
                "socks5://127.0.0.1:{UPSTREAM_PORT}",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]

[[upstream]]
id = "upstream-0"
uri = "socks5://127.0.0.1:{UPSTREAM_PORT}"

[[upstream_group]]
id = "chain-group"
members = ["upstream-0"]

[[rules]]
id = "route-all"
any = true
upstream_group = "chain-group"
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(15),
            category: ScenarioCategory::Chains,
        },
        OracleScenario {
            id: "chain.http_to_socks5",
            capability_ids: vec!["routing.chain_http_socks5"],
            description: "HTTP CONNECT chained through SOCKS5 upstream",
            pproxy_args: vec![
                "-l",
                "http://127.0.0.1:{PORT}",
                "-r",
                "socks5://127.0.0.1:{UPSTREAM_PORT}",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["http"]

[[upstream]]
id = "upstream-0"
uri = "socks5://127.0.0.1:{UPSTREAM_PORT}"

[[upstream_group]]
id = "chain-group"
members = ["upstream-0"]

[[rules]]
id = "route-all"
any = true
upstream_group = "chain-group"
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(15),
            category: ScenarioCategory::Chains,
        },
        OracleScenario {
            id: "chain.socks5_to_http",
            capability_ids: vec!["routing.chain_socks5_http"],
            description: "SOCKS5 chained through HTTP upstream",
            pproxy_args: vec![
                "-l",
                "socks5://127.0.0.1:{PORT}",
                "-r",
                "http://127.0.0.1:{UPSTREAM_PORT}",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]

[[upstream]]
id = "upstream-0"
uri = "http://127.0.0.1:{UPSTREAM_PORT}"

[[upstream_group]]
id = "chain-group"
members = ["upstream-0"]

[[rules]]
id = "route-all"
any = true
upstream_group = "chain-group"
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(15),
            category: ScenarioCategory::Chains,
        },
        OracleScenario {
            id: "chain.socks5_auth_to_socks5",
            capability_ids: vec!["routing.chain_auth"],
            description: "SOCKS5 with auth chained through SOCKS5 upstream",
            pproxy_args: vec![
                "-l",
                "socks5://127.0.0.1:{PORT}#user:pass",
                "-r",
                "socks5://127.0.0.1:{UPSTREAM_PORT}",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "user"
password = "pass"

[[upstream]]
id = "upstream-0"
uri = "socks5://127.0.0.1:{UPSTREAM_PORT}"

[[upstream_group]]
id = "chain-group"
members = ["upstream-0"]

[[rules]]
id = "route-all"
any = true
upstream_group = "chain-group"
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(15),
            category: ScenarioCategory::Chains,
        },
        OracleScenario {
            id: "chain.ss_to_socks5",
            capability_ids: vec!["routing.chain_ss_socks5"],
            description: "Shadowsocks chained through SOCKS5 upstream",
            pproxy_args: vec![
                "-l",
                "ss://127.0.0.1:{PORT}#testuser:testpass",
                "-r",
                "socks5://127.0.0.1:{UPSTREAM_PORT}",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["shadowsocks"]
password = "testpass"
method = "aes-256-gcm"

[[upstream]]
id = "upstream-0"
uri = "socks5://127.0.0.1:{UPSTREAM_PORT}"

[[upstream_group]]
id = "chain-group"
members = ["upstream-0"]

[[rules]]
id = "route-all"
any = true
upstream_group = "chain-group"
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(15),
            category: ScenarioCategory::Chains,
        },
    ]
}

// ===== Rules (5 scenarios) =====

fn rule_scenarios() -> Vec<OracleScenario> {
    vec![
        OracleScenario {
            id: "rules.reject_ip",
            capability_ids: vec!["routing.reject_ip"],
            description: "Rule rejecting connections to specific IP",
            pproxy_args: vec![
                "-l",
                "socks5://127.0.0.1:{PORT}",
                "-r",
                "direct",
                "-b",
                "127.0.0.2",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]

[[rules]]
id = "reject-ip"
any = true
host_regex = "127\\.0\\.0\\.2"
reject = "blocked"
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::Rules,
        },
        OracleScenario {
            id: "rules.reject_domain",
            capability_ids: vec!["routing.reject_domain"],
            description: "Rule rejecting connections to specific domain",
            pproxy_args: vec![
                "-l",
                "socks5://127.0.0.1:{PORT}",
                "-r",
                "direct",
                "-b",
                "blocked.example.com",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]

[[rules]]
id = "reject-domain"
any = true
host_regex = "blocked\\.example\\.com"
reject = "blocked"
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::Rules,
        },
        OracleScenario {
            id: "rules.allow_all",
            capability_ids: vec!["routing.allow_all"],
            description: "Rule allowing all connections (default behavior)",
            pproxy_args: vec!["-l", "socks5://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::Rules,
        },
        OracleScenario {
            id: "rules.block_reject",
            capability_ids: vec!["routing.block_reject"],
            description: "Block action rejecting connections",
            pproxy_args: vec![
                "-l",
                "socks5://127.0.0.1:{PORT}",
                "-r",
                "direct",
                "-b",
                "127.0.0.3",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]

[[rules]]
id = "reject-b"
any = true
host_regex = "127\\.0\\.0\\.3"
reject = "blocked"
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::Rules,
        },
        OracleScenario {
            id: "rules.multiple_reject",
            capability_ids: vec!["routing.multiple_reject"],
            description: "Multiple reject rules with different targets",
            pproxy_args: vec![
                "-l",
                "socks5://127.0.0.1:{PORT}",
                "-r",
                "direct",
                "-b",
                "127.0.0.4",
                "-b",
                "127.0.0.5",
            ],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]

[[rules]]
id = "reject-c"
any = true
host_regex = "127\\.0\\.0\\.4"
reject = "blocked"

[[rules]]
id = "reject-d"
any = true
host_regex = "127\\.0\\.0\\.5"
reject = "blocked"
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::Rules,
        },
    ]
}

// ===== UDP (4 scenarios) =====

fn udp_scenarios() -> Vec<OracleScenario> {
    vec![
        OracleScenario {
            id: "udp.socks5_associate",
            capability_ids: vec!["protocol.socks5_udp_associate"],
            description: "SOCKS5 UDP ASSOCIATE lifecycle",
            pproxy_args: vec!["-l", "socks5://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]
"#,
            expected_equivalence: EquivalenceTarget::CoarseResult,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::Udp,
        },
        OracleScenario {
            id: "udp.socks5_relay",
            capability_ids: vec!["protocol.socks5_udp_relay"],
            description: "SOCKS5 UDP relay with echo payload",
            pproxy_args: vec!["-l", "socks5://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::Udp,
        },
        OracleScenario {
            id: "udp.standalone",
            capability_ids: vec!["protocol.standalone_udp"],
            description: "Standalone UDP relay (pproxy-compatible mode)",
            pproxy_args: vec!["-l", "udp://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
mode = "standalone_pproxy_udp"
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::Udp,
        },
        OracleScenario {
            id: "udp.echo_roundtrip",
            capability_ids: vec!["protocol.udp_echo_roundtrip"],
            description: "UDP echo roundtrip through SOCKS5 proxy",
            pproxy_args: vec!["-l", "socks5://127.0.0.1:{PORT}", "-r", "direct"],
            eggress_toml: r#"
version = 1

[[listeners]]
name = "test"
bind = "127.0.0.1:{PORT}"
protocols = ["socks5"]
"#,
            expected_equivalence: EquivalenceTarget::Payload,
            normalization: NormalizationRules {
                strip_log_prefixes: true,
                normalize_ports: true,
                ..Default::default()
            },
            platform: PlatformRequirements::default(),
            timeout: Duration::from_secs(10),
            category: ScenarioCategory::Udp,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_scenarios_have_unique_ids() {
        let scenarios = all_scenarios();
        let mut ids: Vec<&str> = scenarios.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), scenarios.len(), "duplicate scenario IDs found");
    }

    #[test]
    fn all_scenarios_have_nonempty_capabilities() {
        for scenario in all_scenarios() {
            assert!(
                !scenario.capability_ids.is_empty(),
                "scenario {} has no capability IDs",
                scenario.id
            );
        }
    }

    #[test]
    fn scenario_count_by_category() {
        assert_eq!(cli_defaults_scenarios().len(), 7);
        assert_eq!(http_socks_tcp_scenarios().len(), 10);
        assert_eq!(chain_scenarios().len(), 5);
        assert_eq!(rule_scenarios().len(), 5);
        assert_eq!(udp_scenarios().len(), 4);
        assert_eq!(all_scenarios().len(), 31);
    }

    #[test]
    fn find_scenario_by_id() {
        assert!(find_scenario("cli.socks5_default").is_some());
        assert!(find_scenario("nonexistent").is_none());
    }

    #[test]
    fn scenarios_for_category_filter() {
        let cli = scenarios_for_category(ScenarioCategory::CliDefaults);
        assert_eq!(cli.len(), 7);
        for s in &cli {
            assert_eq!(s.category, ScenarioCategory::CliDefaults);
        }
    }
}
