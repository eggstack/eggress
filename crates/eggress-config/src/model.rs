use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MatchExprConfig {
    Composite(CompositeMatcher),
    Leaf(Box<LeafMatcher>),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeMatcher {
    #[serde(default)]
    pub all: Option<Vec<MatchExprConfig>>,
    #[serde(default)]
    pub any_of: Option<Vec<MatchExprConfig>>,
    #[serde(default)]
    pub not: Option<Box<MatchExprConfig>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct LeafMatcher {
    pub host_exact: Option<String>,
    pub host_suffix: Option<String>,
    pub host_regex: Option<String>,
    pub destination_port: Option<u16>,
    pub destination_port_range: Option<Vec<u16>>,
    pub destination_port_set: Option<Vec<u16>>,
    pub destination_cidr: Option<String>,
    pub source_cidr: Option<String>,
    pub source_port: Option<u16>,
    pub listener: Option<String>,
    pub protocol: Option<String>,
    pub identity: Option<String>,
    pub transport: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub version: Option<u32>,
    pub process: Option<ProcessConfig>,
    pub timeouts: Option<TimeoutConfig>,
    pub listeners: Option<Vec<ListenerConfig>>,
    pub upstreams: Option<Vec<UpstreamConfig>>,
    pub upstream_groups: Option<Vec<UpstreamGroupConfig>>,
    pub rules: Option<Vec<RuleConfig>>,
    pub rules_file: Option<String>,
    pub routing: Option<RoutingConfig>,
    pub admin: Option<AdminConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ProcessConfig {
    pub log_format: Option<String>,
    pub log_level: Option<String>,
    pub shutdown_grace: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TimeoutConfig {
    pub handshake: Option<String>,
    pub connect: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListenerUdpConfig {
    pub enabled: Option<bool>,
    pub mode: Option<String>,
    pub bind: Option<String>,
    pub advertise: Option<String>,
    pub idle_timeout: Option<String>,
    pub target_idle_timeout: Option<String>,
    pub max_associations: Option<usize>,
    pub max_targets_per_association: Option<usize>,
    pub max_datagram_size: Option<usize>,
    pub client_pin: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListenerConfig {
    pub name: String,
    pub bind: String,
    pub protocols: Vec<String>,
    pub connection_limit: Option<u32>,
    pub auth: Option<AuthConfig>,
    #[serde(default)]
    pub udp_enabled: Option<bool>,
    #[serde(default)]
    pub udp: Option<ListenerUdpConfig>,
    #[serde(default)]
    pub tls: Option<ListenerTlsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListenerTlsConfig {
    pub cert: String,
    pub key: String,
    #[serde(default)]
    pub alpn: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    #[serde(rename = "type")]
    pub auth_type: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub password_env: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpstreamConfig {
    pub id: String,
    pub uri: String,
    #[serde(default)]
    pub health: Option<HealthConfigToml>,
}

#[derive(Debug, Deserialize)]
pub struct HealthConfigToml {
    pub mode: Option<String>,
    pub interval: Option<String>,
    pub timeout: Option<String>,
    pub failures_to_unhealthy: Option<u32>,
    pub successes_to_healthy: Option<u32>,
    pub initial_state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpstreamGroupConfig {
    pub id: String,
    pub scheduler: Option<String>,
    pub members: Vec<String>,
    pub fallback: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RuleConfig {
    pub id: String,
    pub host_exact: Option<String>,
    pub host_suffix: Option<String>,
    pub host_regex: Option<String>,
    pub destination_port: Option<u16>,
    #[serde(default)]
    pub any: Option<bool>,
    #[serde(default, rename = "match")]
    pub match_expr: Option<MatchExprConfig>,
    pub direct: Option<bool>,
    pub upstream_group: Option<String>,
    pub reject: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RoutingConfig {
    pub default: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AdminConfig {
    pub bind: Option<String>,
    pub enabled: Option<bool>,
    pub metrics: Option<bool>,
    pub pac: Option<PacConfigToml>,
    pub static_content: Option<Vec<StaticContentToml>>,
}

#[derive(Debug, Deserialize)]
pub struct PacConfigToml {
    pub path: Option<String>,
    pub proxy: String,
    pub direct_fallback: Option<bool>,
    pub direct_hosts: Option<Vec<String>>,
    pub direct_suffixes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct StaticContentToml {
    pub path: String,
    pub content_type: Option<String>,
    pub body: Option<String>,
}
