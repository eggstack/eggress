use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub version: Option<u32>,
    pub process: Option<ProcessConfig>,
    pub timeouts: Option<TimeoutConfig>,
    pub listeners: Option<Vec<ListenerConfig>>,
    pub upstreams: Option<Vec<UpstreamConfig>>,
    pub upstream_groups: Option<Vec<UpstreamGroupConfig>>,
    pub rules: Option<Vec<RuleConfig>>,
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

#[derive(Debug, Deserialize)]
pub struct ListenerConfig {
    pub name: String,
    pub bind: String,
    pub protocols: Vec<String>,
    pub connection_limit: Option<u32>,
    pub auth: Option<AuthConfig>,
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
}
