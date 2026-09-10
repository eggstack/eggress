//! Compiled runtime model: `RuntimeConfig` and compiled DTOs.
//!
//! `parse -> validate -> compile` remains one-way: compilation modules may
//! assume validation invariants only where the current design already
//! guarantees them.

use zeroize::Zeroize;

use eggress_core::ProtocolId;
use eggress_routing::scheduler::SchedulerKind;
use eggress_routing::UpstreamGroupId;

/// Compiled TLS material for a native reverse server control channel.
#[derive(Clone)]
pub struct CompiledReverseServerTls {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    pub client_ca_pem: Option<Vec<u8>>,
    pub require_client_cert: bool,
}

impl std::fmt::Debug for CompiledReverseServerTls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledReverseServerTls")
            .field("has_cert", &!self.cert_pem.is_empty())
            .field("has_key", &!self.key_pem.is_empty())
            .field("has_client_ca", &self.client_ca_pem.is_some())
            .field("require_client_cert", &self.require_client_cert)
            .finish()
    }
}

impl Drop for CompiledReverseServerTls {
    fn drop(&mut self) {
        self.key_pem.zeroize();
        if let Some(ref mut ca) = self.client_ca_pem {
            ca.zeroize();
        }
    }
}

/// Compiled TLS material for a native reverse client control channel.
#[derive(Clone)]
pub struct CompiledReverseClientTls {
    pub ca_pem: Option<Vec<u8>>,
    pub server_name: String,
    pub client_cert_pem: Option<Vec<u8>>,
    pub client_key_pem: Option<Vec<u8>>,
}

impl std::fmt::Debug for CompiledReverseClientTls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledReverseClientTls")
            .field("has_ca", &self.ca_pem.is_some())
            .field("server_name", &self.server_name)
            .field("has_client_cert", &self.client_cert_pem.is_some())
            .field("has_client_key", &self.client_key_pem.is_some())
            .finish()
    }
}

impl Drop for CompiledReverseClientTls {
    fn drop(&mut self) {
        if let Some(ref mut key) = self.client_key_pem {
            key.zeroize();
        }
    }
}

/// Compiled reverse server configuration with resolved defaults and parsed addresses.
#[derive(Clone)]
pub struct CompiledReverseServerConfig {
    pub id: String,
    pub control_bind: std::net::SocketAddr,
    pub external_bind: std::net::SocketAddr,
    pub auth_username: Option<String>,
    pub auth_password: Option<String>,
    pub max_control_connections: u32,
    pub read_timeout_ms: u64,
    pub allow_bind: Option<Vec<std::net::SocketAddr>>,
    pub max_listeners_per_client: u32,
    pub max_streams_per_listener: u32,
    pub max_pending_external: u32,
    pub pproxy_compat: bool,
    pub tls: Option<CompiledReverseServerTls>,
}

impl std::fmt::Debug for CompiledReverseServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `auth_password` is redacted so debug logging of the compiled
        // runtime config can never leak credentials.
        f.debug_struct("CompiledReverseServerConfig")
            .field("id", &self.id)
            .field("control_bind", &self.control_bind)
            .field("external_bind", &self.external_bind)
            .field("auth_username", &self.auth_username)
            .field("auth_password", &"****")
            .field("max_control_connections", &self.max_control_connections)
            .field("read_timeout_ms", &self.read_timeout_ms)
            .field("allow_bind", &self.allow_bind)
            .field("max_listeners_per_client", &self.max_listeners_per_client)
            .field("max_streams_per_listener", &self.max_streams_per_listener)
            .field("max_pending_external", &self.max_pending_external)
            .field("pproxy_compat", &self.pproxy_compat)
            .field("tls", &self.tls)
            .finish()
    }
}

impl Drop for CompiledReverseServerConfig {
    fn drop(&mut self) {
        if let Some(password) = &mut self.auth_password {
            password.zeroize();
        }
    }
}

/// Compiled reverse client configuration with resolved defaults and parsed addresses.
#[derive(Clone)]
pub struct CompiledReverseClientConfig {
    pub id: String,
    pub server_addr: std::net::SocketAddr,
    pub server_chain: Option<eggress_uri::ProxyChainSpec>,
    pub auth_username: Option<String>,
    pub auth_password: Option<String>,
    pub reconnect_initial_ms: u64,
    pub reconnect_max_ms: u64,
    pub default_target_host: Option<String>,
    pub default_target_port: Option<u16>,
    pub read_timeout_ms: u64,
    pub drain_grace_ms: u64,
    pub parallel_connections: u32,
    pub pproxy_compat: bool,
    pub tls: Option<CompiledReverseClientTls>,
}

impl std::fmt::Debug for CompiledReverseClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `auth_password` is redacted; see CompiledReverseServerConfig.
        f.debug_struct("CompiledReverseClientConfig")
            .field("id", &self.id)
            .field("server_addr", &self.server_addr)
            .field("server_chain", &self.server_chain)
            .field("auth_username", &self.auth_username)
            .field("auth_password", &"****")
            .field("reconnect_initial_ms", &self.reconnect_initial_ms)
            .field("reconnect_max_ms", &self.reconnect_max_ms)
            .field("default_target_host", &self.default_target_host)
            .field("default_target_port", &self.default_target_port)
            .field("read_timeout_ms", &self.read_timeout_ms)
            .field("drain_grace_ms", &self.drain_grace_ms)
            .field("parallel_connections", &self.parallel_connections)
            .field("pproxy_compat", &self.pproxy_compat)
            .field("tls", &self.tls)
            .finish()
    }
}

impl Drop for CompiledReverseClientConfig {
    fn drop(&mut self) {
        if let Some(password) = &mut self.auth_password {
            password.zeroize();
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub process: ProcessConfig,
    pub timeouts: TimeoutConfig,
    pub listeners: Vec<ListenerConfig>,
    pub upstreams: Vec<UpstreamConfig>,
    pub groups: Vec<UpstreamGroupConfig>,
    pub rules: Vec<eggress_routing::CompiledRule>,
    pub default_action: eggress_routing::RouteActionSpec,
    pub admin: Option<AdminConfig>,
    pub reverse_servers: Vec<CompiledReverseServerConfig>,
    pub reverse_clients: Vec<CompiledReverseClientConfig>,
}

#[derive(Debug, Clone)]
pub struct ProcessConfig {
    pub log_format: String,
    pub log_level: String,
    pub shutdown_grace: std::time::Duration,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            log_format: "text".to_string(),
            log_level: "info".to_string(),
            shutdown_grace: std::time::Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeoutConfig {
    pub handshake: std::time::Duration,
    pub connect: std::time::Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            handshake: std::time::Duration::from_secs(10),
            connect: std::time::Duration::from_secs(30),
        }
    }
}

/// Compiled transparent proxy configuration with resolved defaults.
#[derive(Debug, Clone)]
pub struct CompiledTransparentConfig {
    pub enabled: bool,
    pub protocol: String,
}

impl Default for CompiledTransparentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            protocol: "redir".to_string(),
        }
    }
}

/// Compiled Unix domain socket listener configuration with resolved defaults.
#[derive(Debug, Clone)]
pub struct CompiledUnixListenerConfig {
    pub path: std::path::PathBuf,
    pub unlink_existing: bool,
    pub mode: u32,
}

/// Compiled UDP listener configuration with resolved defaults.
#[derive(Debug, Clone)]
pub struct CompiledListenerUdpConfig {
    pub mode: eggress_udp::UdpMode,
    pub enabled: bool,
    pub bind: std::net::SocketAddr,
    pub advertise: Option<std::net::IpAddr>,
    pub idle_timeout: std::time::Duration,
    pub target_idle_timeout: std::time::Duration,
    pub max_associations: usize,
    pub max_targets_per_association: usize,
    pub max_datagram_size: usize,
    pub client_pin: bool,
    pub allow_private_egress: bool,
    pub max_associations_global: usize,
    pub fixed_target: Option<eggress_core::TargetAddr>,
    pub upstream_connect_timeout: std::time::Duration,
    pub upstream_udp_bind: std::net::SocketAddr,
}

impl Default for CompiledListenerUdpConfig {
    fn default() -> Self {
        Self {
            mode: eggress_udp::UdpMode::Socks5UdpAssociate,
            enabled: true,
            bind: "127.0.0.1:0".parse().unwrap(),
            advertise: None,
            idle_timeout: std::time::Duration::from_secs(60),
            target_idle_timeout: std::time::Duration::from_secs(30),
            max_associations: 1024,
            max_targets_per_association: 64,
            max_datagram_size: 65535,
            client_pin: true,
            allow_private_egress: true,
            max_associations_global: 1024,
            fixed_target: None,
            upstream_connect_timeout: std::time::Duration::from_secs(10),
            upstream_udp_bind: "127.0.0.1:0".parse().unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListenerConfig {
    pub name: String,
    pub bind: String,
    pub protocols: Vec<ProtocolId>,
    pub reuse_port: Option<bool>,
    pub connection_limit: Option<u32>,
    pub auth: Option<crate::model::AuthConfig>,
    pub udp: Option<CompiledListenerUdpConfig>,
    pub tls: Option<CompiledListenerTlsConfig>,
    pub shadowsocks: Option<crate::model::ShadowsocksListenerConfig>,
    pub trojan: Option<crate::model::ListenerTrojanConfig>,
    pub transparent: Option<CompiledTransparentConfig>,
    pub unix: Option<CompiledUnixListenerConfig>,
    pub fixed_target: Option<eggress_core::TargetAddr>,
    pub local_bind: Option<String>,
}

/// Compiled TLS configuration for a listener.
#[derive(Debug, Clone)]
pub struct CompiledListenerTlsConfig {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    pub alpn: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct CompiledH2Config {
    pub max_concurrent_streams: u32,
    pub pool_size: u32,
    pub idle_timeout: std::time::Duration,
    pub keepalive_interval: std::time::Duration,
    pub keepalive_timeout: std::time::Duration,
    pub stream_receive_window: u32,
    pub connection_receive_window: u32,
    pub max_frame_size: u32,
    pub max_header_list_size: u32,
}

impl Default for CompiledH2Config {
    fn default() -> Self {
        Self {
            max_concurrent_streams: 100,
            pool_size: 4,
            idle_timeout: std::time::Duration::from_secs(60),
            keepalive_interval: std::time::Duration::from_secs(30),
            keepalive_timeout: std::time::Duration::from_secs(10),
            stream_receive_window: 65535,
            connection_receive_window: 65535,
            max_frame_size: 16384,
            max_header_list_size: 65535,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    pub id: String,
    pub chain: eggress_uri::ProxyChainSpec,
    pub health: eggress_routing::health::HealthConfig,
    pub h2: Option<CompiledH2Config>,
}

#[derive(Debug, Clone)]
pub struct UpstreamGroupConfig {
    pub id: UpstreamGroupId,
    pub scheduler: SchedulerKind,
    pub members: Vec<String>,
    pub fallback: GroupFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupFallback {
    Reject,
    Direct,
    UseUnhealthy,
}

#[derive(Debug, Clone)]
pub struct PacConfig {
    pub path: String,
    pub proxy_directive: String,
    pub direct_fallback: bool,
    pub direct_hosts: Vec<String>,
    pub direct_suffixes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StaticRoute {
    pub path: String,
    pub content_type: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct AdminConfig {
    pub bind: String,
    pub enabled: bool,
    pub metrics: bool,
    pub auth: Option<AdminAuthConfig>,
    pub pac: Option<PacConfig>,
    pub static_content: Vec<StaticRoute>,
}

#[derive(Clone)]
pub struct AdminAuthConfig {
    pub bearer_token: Option<String>,
    pub basic_username: Option<String>,
    pub basic_password: Option<String>,
}

impl std::fmt::Debug for AdminAuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `bearer_token` and `basic_password` are redacted so debug logging
        // of the compiled admin config can never leak credentials.
        f.debug_struct("AdminAuthConfig")
            .field("bearer_token", &"****")
            .field("basic_username", &self.basic_username)
            .field("basic_password", &"****")
            .finish()
    }
}

impl Drop for AdminAuthConfig {
    fn drop(&mut self) {
        if let Some(token) = &mut self.bearer_token {
            token.zeroize();
        }
        if let Some(password) = &mut self.basic_password {
            password.zeroize();
        }
    }
}
