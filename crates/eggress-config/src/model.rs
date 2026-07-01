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
    pub reverse_listener: Option<String>,
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
    #[serde(default)]
    pub reverse_servers: Option<Vec<ReverseServerConfig>>,
    #[serde(default)]
    pub reverse_clients: Option<Vec<ReverseClientConfig>>,
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
    #[serde(default)]
    pub shadowsocks: Option<ShadowsocksListenerConfig>,
    #[serde(default)]
    pub transparent: Option<TransparentConfig>,
    #[serde(default)]
    pub unix: Option<UnixListenerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransparentConfig {
    pub enabled: Option<bool>,
    /// Transparent proxy protocol: "redir" (iptables REDIRECT/nftables) or "pf" (macOS PF).
    pub protocol: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnixListenerConfig {
    /// Filesystem path for the Unix domain socket.
    pub path: String,
    /// Whether to remove an existing socket file before binding.
    pub unlink_existing: Option<bool>,
    /// File permissions for the socket (e.g., 0o666).
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShadowsocksListenerConfig {
    pub method: String,
    pub password: String,
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

/// Configuration for a reverse proxy server (acceptor side).
///
/// The reverse server accepts control connections from remote clients and
/// dispatches accepted connections back through the control channel.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReverseServerConfig {
    /// Unique identifier for this reverse server.
    pub id: String,
    /// Address to bind the control listener on (e.g., "0.0.0.0:8443").
    pub control_bind: String,
    /// Optional username for authentication.
    pub auth_username: Option<String>,
    /// Optional password for authentication.
    pub auth_password: Option<String>,
    /// Optional password environment variable.
    pub auth_password_env: Option<String>,
    /// Maximum concurrent streams per control client.
    pub max_streams: Option<u32>,
    /// Heartbeat interval (e.g., "30s").
    pub heartbeat_interval: Option<String>,
}

/// Configuration for a reverse proxy control client.
///
/// The control client connects to a remote reverse server and services
/// incoming stream-open requests by connecting to local targets.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReverseClientConfig {
    /// Unique identifier for this reverse client.
    pub id: String,
    /// Address of the reverse server to connect to.
    pub server_addr: String,
    /// Optional username for authentication.
    pub auth_username: Option<String>,
    /// Optional password for authentication.
    pub auth_password: Option<String>,
    /// Optional password environment variable.
    pub auth_password_env: Option<String>,
    /// Reconnect backoff initial delay (e.g., "1s").
    pub reconnect_initial: Option<String>,
    /// Reconnect backoff max delay (e.g., "30s").
    pub reconnect_max: Option<String>,
    /// Heartbeat interval (e.g., "30s").
    pub heartbeat_interval: Option<String>,
}
