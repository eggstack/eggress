use std::sync::Arc;

use eggress_core::{ProtocolId, RejectReason};
use eggress_routing::scheduler::SchedulerKind;
use eggress_routing::UpstreamGroupId;

use crate::error::ConfigError;
use crate::model::{
    ConfigFile, HealthConfigToml, LeafMatcher, ListenerUdpConfig, MatchExprConfig, RuleConfig,
};
use crate::validate::validate_duration;

/// Compiled reverse server configuration with resolved defaults and parsed addresses.
#[derive(Debug, Clone)]
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
}

/// Compiled reverse client configuration with resolved defaults and parsed addresses.
#[derive(Debug, Clone)]
pub struct CompiledReverseClientConfig {
    pub id: String,
    pub server_addr: std::net::SocketAddr,
    pub auth_username: Option<String>,
    pub auth_password: Option<String>,
    pub reconnect_initial_ms: u64,
    pub reconnect_max_ms: u64,
    pub default_target_host: Option<String>,
    pub default_target_port: Option<u16>,
    pub read_timeout_ms: u64,
    pub drain_grace_ms: u64,
    pub parallel_connections: u32,
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

#[derive(Debug, Clone)]
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
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListenerConfig {
    pub name: String,
    pub bind: String,
    pub protocols: Vec<ProtocolId>,
    pub connection_limit: Option<u32>,
    pub auth: Option<crate::model::AuthConfig>,
    pub udp: Option<CompiledListenerUdpConfig>,
    pub tls: Option<CompiledListenerTlsConfig>,
    pub shadowsocks: Option<crate::model::ShadowsocksListenerConfig>,
    pub trojan: Option<crate::model::ListenerTrojanConfig>,
    pub transparent: Option<CompiledTransparentConfig>,
    pub unix: Option<CompiledUnixListenerConfig>,
}

/// Compiled TLS configuration for a listener.
#[derive(Debug, Clone)]
pub struct CompiledListenerTlsConfig {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    pub alpn: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    pub id: String,
    pub chain: eggress_uri::ProxyChainSpec,
    pub health: eggress_routing::health::HealthConfig,
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
pub struct AdminConfig {
    pub bind: String,
    pub enabled: bool,
    pub metrics: bool,
    pub pac: Option<eggress_admin::PacConfig>,
    pub static_content: Vec<eggress_admin::StaticRoute>,
}

fn compile_reject_reason(s: &str) -> Result<RejectReason, ConfigError> {
    match s {
        "unsupported-protocol" => Ok(RejectReason::UnsupportedProtocol),
        "auth-required" => Ok(RejectReason::AuthRequired),
        "access-denied" => Ok(RejectReason::AccessDenied),
        "blocked" => Ok(RejectReason::Blocked),
        "internal-error" => Ok(RejectReason::InternalError),
        _ => Err(ConfigError::validation(
            "reject",
            &format!("unknown reject reason: {}", s),
        )),
    }
}

fn compile_protocol(s: &str) -> Result<ProtocolId, ConfigError> {
    match s {
        "http" => Ok(ProtocolId::Http),
        "socks4" => Ok(ProtocolId::Socks4),
        "socks5" => Ok(ProtocolId::Socks5),
        "shadowsocks" => Ok(ProtocolId::Shadowsocks),
        "trojan" => Ok(ProtocolId::Trojan),
        // H2/WebSocket/Raw are recognized in the URI parser and exist as
        // protocol crates, but they are NOT yet integrated through the
        // supervisor. We refuse them in `[[listeners]]`/`[[upstreams]]`
        // configs with a structured diagnostic so users get an honest
        // failure rather than a silently broken listener. See
        // docs/PHASE_25_28_HARDENING_COMPLETION.md (H5/H6/H7).
        "h2" | "websocket" | "ws" | "wss" | "raw" | "tunnel" => Err(ConfigError::validation(
            "protocols",
            &format!(
                "'{}' is not yet integrated through the runtime supervisor \
                     (protocol-crate only). Use it directly via the protocol crate, \
                     or rely on a stdio TCP listener/upstream instead.",
                s
            ),
        )),
        _ => Err(ConfigError::validation(
            "protocols",
            &format!("unknown protocol: {}", s),
        )),
    }
}

fn compile_transport(s: &str) -> Result<eggress_routing::TransportKind, ConfigError> {
    match s {
        "tcp" => Ok(eggress_routing::TransportKind::Tcp),
        "udp" => Ok(eggress_routing::TransportKind::Udp),
        "reverse_tcp" => Ok(eggress_routing::TransportKind::ReverseTcp),
        _ => Err(ConfigError::validation(
            "transport",
            &format!("unknown transport: {}", s),
        )),
    }
}

fn compile_matcher(rule: &RuleConfig) -> Result<eggress_routing::MatchExpr, ConfigError> {
    if let Some(ref match_expr) = rule.match_expr {
        return compile_match_config(match_expr);
    }

    if let Some(ref exact) = rule.host_exact {
        if rule.host_suffix.is_none()
            && rule.host_regex.is_none()
            && rule.destination_port.is_none()
            && !rule.any.unwrap_or(false)
        {
            return Ok(eggress_routing::MatchExpr::HostExact(Arc::from(
                exact.as_str(),
            )));
        }
    }
    if let Some(ref suffix) = rule.host_suffix {
        if rule.host_exact.is_none()
            && rule.host_regex.is_none()
            && rule.destination_port.is_none()
            && !rule.any.unwrap_or(false)
        {
            return Ok(eggress_routing::MatchExpr::HostSuffix(Arc::from(
                suffix.as_str(),
            )));
        }
    }
    if let Some(ref regex_str) = rule.host_regex {
        if rule.host_exact.is_none()
            && rule.host_suffix.is_none()
            && rule.destination_port.is_none()
            && !rule.any.unwrap_or(false)
        {
            let re = regex::Regex::new(regex_str).map_err(|e| {
                ConfigError::validation(
                    "host_regex",
                    &format!("invalid regex '{}': {}", regex_str, e),
                )
            })?;
            return Ok(eggress_routing::MatchExpr::HostRegex(re));
        }
    }
    if let Some(port) = rule.destination_port {
        if rule.host_exact.is_none()
            && rule.host_suffix.is_none()
            && rule.host_regex.is_none()
            && !rule.any.unwrap_or(false)
        {
            return Ok(eggress_routing::MatchExpr::DestinationPort(
                eggress_routing::PortMatcher::Exact(port),
            ));
        }
    }
    if rule.any.unwrap_or(false)
        || (rule.host_exact.is_none()
            && rule.host_suffix.is_none()
            && rule.host_regex.is_none()
            && rule.destination_port.is_none())
    {
        return Ok(eggress_routing::MatchExpr::Any);
    }
    Err(ConfigError::validation(&rule.id, "ambiguous matcher"))
}

const MAX_EXPRESSION_DEPTH: usize = 10;
const MAX_NODE_COUNT: usize = 100;

fn compile_match_config(
    config: &MatchExprConfig,
) -> Result<eggress_routing::MatchExpr, ConfigError> {
    let mut node_count = 0;
    compile_match_config_limited(config, 0, &mut node_count)
}

fn compile_match_config_limited(
    config: &MatchExprConfig,
    depth: usize,
    node_count: &mut usize,
) -> Result<eggress_routing::MatchExpr, ConfigError> {
    *node_count += 1;
    if *node_count > MAX_NODE_COUNT {
        return Err(ConfigError::validation(
            "match",
            &format!("expression exceeds maximum node count ({})", MAX_NODE_COUNT),
        ));
    }
    if depth > MAX_EXPRESSION_DEPTH {
        return Err(ConfigError::validation(
            "match",
            &format!(
                "expression exceeds maximum depth ({})",
                MAX_EXPRESSION_DEPTH
            ),
        ));
    }

    match config {
        MatchExprConfig::Composite(composite) => {
            if let Some(ref all) = composite.all {
                if all.is_empty() {
                    return Err(ConfigError::validation("match.all", "must not be empty"));
                }
                let exprs: Vec<eggress_routing::MatchExpr> = all
                    .iter()
                    .map(|c| compile_match_config_limited(c, depth + 1, node_count))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(eggress_routing::MatchExpr::All(exprs));
            }
            if let Some(ref any_of) = composite.any_of {
                if any_of.is_empty() {
                    return Err(ConfigError::validation("match.any_of", "must not be empty"));
                }
                let exprs: Vec<eggress_routing::MatchExpr> = any_of
                    .iter()
                    .map(|c| compile_match_config_limited(c, depth + 1, node_count))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(eggress_routing::MatchExpr::AnyOf(exprs));
            }
            if let Some(ref not) = composite.not {
                let inner = compile_match_config_limited(not, depth + 1, node_count)?;
                return Ok(eggress_routing::MatchExpr::Not(Box::new(inner)));
            }
            Err(ConfigError::validation(
                "match",
                "composite must have exactly one of: all, any_of, not",
            ))
        }
        MatchExprConfig::Leaf(leaf) => compile_leaf_matcher(leaf),
    }
}

fn compile_leaf_matcher(leaf: &LeafMatcher) -> Result<eggress_routing::MatchExpr, ConfigError> {
    let mut matchers = Vec::new();

    if let Some(ref exact) = leaf.host_exact {
        matchers.push(eggress_routing::MatchExpr::HostExact(Arc::from(
            exact.as_str(),
        )));
    }
    if let Some(ref suffix) = leaf.host_suffix {
        matchers.push(eggress_routing::MatchExpr::HostSuffix(Arc::from(
            suffix.as_str(),
        )));
    }
    if let Some(ref regex_str) = leaf.host_regex {
        let re = regex::Regex::new(regex_str).map_err(|e| {
            ConfigError::validation(
                "host_regex",
                &format!("invalid regex '{}': {}", regex_str, e),
            )
        })?;
        matchers.push(eggress_routing::MatchExpr::HostRegex(re));
    }
    if let Some(port) = leaf.destination_port {
        matchers.push(eggress_routing::MatchExpr::DestinationPort(
            eggress_routing::PortMatcher::Exact(port),
        ));
    }
    if let Some(ref range) = leaf.destination_port_range {
        if range.len() != 2 {
            return Err(ConfigError::validation(
                "destination_port_range",
                "must have exactly 2 elements [start, end]",
            ));
        }
        if range[0] > range[1] {
            return Err(ConfigError::validation(
                "destination_port_range",
                &format!("start ({}) must be <= end ({})", range[0], range[1]),
            ));
        }
        matchers.push(eggress_routing::MatchExpr::DestinationPort(
            eggress_routing::PortMatcher::Range {
                start: range[0],
                end: range[1],
            },
        ));
    }
    if let Some(ref ports) = leaf.destination_port_set {
        if ports.is_empty() {
            return Err(ConfigError::validation(
                "destination_port_set",
                "must not be empty",
            ));
        }
        matchers.push(eggress_routing::MatchExpr::DestinationPort(
            eggress_routing::PortMatcher::Set(Arc::from(ports.as_slice())),
        ));
    }
    if let Some(ref cidr) = leaf.destination_cidr {
        let net: ipnet::IpNet = cidr.parse().map_err(|e: ipnet::AddrParseError| {
            ConfigError::validation(
                "destination_cidr",
                &format!("invalid CIDR '{}': {}", cidr, e),
            )
        })?;
        matchers.push(eggress_routing::MatchExpr::DestinationCidr(net));
    }
    if let Some(ref cidr) = leaf.source_cidr {
        let net: ipnet::IpNet = cidr.parse().map_err(|e: ipnet::AddrParseError| {
            ConfigError::validation("source_cidr", &format!("invalid CIDR '{}': {}", cidr, e))
        })?;
        matchers.push(eggress_routing::MatchExpr::SourceCidr(net));
    }
    if let Some(source_port) = leaf.source_port {
        matchers.push(eggress_routing::MatchExpr::SourcePort(
            eggress_routing::PortMatcher::Exact(source_port),
        ));
    }
    if let Some(ref name) = leaf.listener {
        matchers.push(eggress_routing::MatchExpr::Listener(Arc::from(
            name.as_str(),
        )));
    }
    if let Some(ref proto) = leaf.protocol {
        let protocol_id = compile_protocol(proto)?;
        matchers.push(eggress_routing::MatchExpr::Protocol(protocol_id));
    }
    if let Some(ref ident) = leaf.identity {
        matchers.push(eggress_routing::MatchExpr::Identity(Arc::from(
            ident.as_str(),
        )));
    }
    if let Some(ref transport_str) = leaf.transport {
        let transport_kind = compile_transport(transport_str)?;
        matchers.push(eggress_routing::MatchExpr::Transport(transport_kind));
    }
    if let Some(ref name) = leaf.reverse_listener {
        matchers.push(eggress_routing::MatchExpr::ReverseListener(Arc::from(
            name.as_str(),
        )));
    }

    match matchers.len() {
        0 => Ok(eggress_routing::MatchExpr::Any),
        1 => Ok(matchers.into_iter().next().unwrap()),
        _ => Ok(eggress_routing::MatchExpr::All(matchers)),
    }
}

fn compile_action(
    rule: &RuleConfig,
    group_ids: &std::collections::HashSet<&str>,
) -> Result<eggress_routing::RouteActionSpec, ConfigError> {
    if let Some(direct) = rule.direct {
        if direct {
            return Ok(eggress_routing::RouteActionSpec::Direct);
        }
        return Err(ConfigError::validation(
            &rule.id,
            "direct action must be true",
        ));
    }
    if let Some(ref group) = rule.upstream_group {
        if !group_ids.contains(group.as_str()) {
            return Err(ConfigError::validation(
                &rule.id,
                &format!("unknown upstream group: {}", group),
            ));
        }
        return Ok(eggress_routing::RouteActionSpec::UpstreamGroup(
            UpstreamGroupId(Arc::from(group.as_str())),
        ));
    }
    if let Some(ref reject) = rule.reject {
        let reason = compile_reject_reason(reject)?;
        return Ok(eggress_routing::RouteActionSpec::Reject(reason));
    }
    Err(ConfigError::validation(&rule.id, "missing action"))
}

pub fn compile_config(config: &ConfigFile) -> Result<RuntimeConfig, ConfigError> {
    let process = compile_process(config);
    let timeouts = compile_timeouts(config)?;
    let listeners = compile_listeners(config)?;
    let upstreams = compile_upstreams(config)?;
    let groups = compile_groups(config)?;
    let rules = compile_rules(config)?;
    let default_action = compile_default_action(config);
    let admin = compile_admin(config);
    let reverse_servers = compile_reverse_servers(config)?;
    let reverse_clients = compile_reverse_clients(config)?;

    Ok(RuntimeConfig {
        process,
        timeouts,
        listeners,
        upstreams,
        groups,
        rules,
        default_action,
        admin,
        reverse_servers,
        reverse_clients,
    })
}

fn compile_process(config: &ConfigFile) -> ProcessConfig {
    let defaults = ProcessConfig::default();
    let process = config.process.as_ref();

    ProcessConfig {
        log_format: process
            .and_then(|p| p.log_format.clone())
            .unwrap_or(defaults.log_format),
        log_level: process
            .and_then(|p| p.log_level.clone())
            .unwrap_or(defaults.log_level),
        shutdown_grace: process
            .and_then(|p| p.shutdown_grace.as_ref())
            .and_then(|s| parse_duration_opt(s))
            .unwrap_or(defaults.shutdown_grace),
    }
}

fn compile_timeouts(config: &ConfigFile) -> Result<TimeoutConfig, ConfigError> {
    let defaults = TimeoutConfig::default();
    let timeouts = config.timeouts.as_ref();

    Ok(TimeoutConfig {
        handshake: timeouts
            .and_then(|t| t.handshake.as_ref())
            .map(|s| validate_duration(s))
            .transpose()?
            .unwrap_or(defaults.handshake),
        connect: timeouts
            .and_then(|t| t.connect.as_ref())
            .map(|s| validate_duration(s))
            .transpose()?
            .unwrap_or(defaults.connect),
    })
}

fn compile_listeners(config: &ConfigFile) -> Result<Vec<ListenerConfig>, ConfigError> {
    let listeners = match &config.listeners {
        Some(l) => l,
        None => return Ok(vec![]),
    };

    listeners
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let path = format!("listeners[{}]", i);

            let protocols: Vec<ProtocolId> = l
                .protocols
                .iter()
                .map(|p| compile_protocol(p))
                .collect::<Result<Vec<_>, _>>()?;

            if protocols.is_empty() {
                return Err(ConfigError::validation(
                    &path,
                    "protocols must not be empty",
                ));
            }

            let udp = match (l.udp_enabled, l.udp.as_ref()) {
                (None, None) => None,
                (None, Some(udp_cfg)) => {
                    Some(compile_listener_udp_config(udp_cfg, &protocols, &path)?)
                }
                (Some(true), None) => Some(compile_listener_udp_defaults(&protocols, &path)?),
                (Some(true), Some(udp_cfg)) => {
                    Some(compile_listener_udp_config(udp_cfg, &protocols, &path)?)
                }
                (Some(false), None) => None,
                (Some(false), Some(udp_cfg)) => {
                    if udp_cfg.enabled.unwrap_or(true) {
                        return Err(ConfigError::validation(
                            &path,
                            "udp_enabled = false conflicts with [listeners.udp] enabled = true",
                        ));
                    }
                    Some(compile_listener_udp_config(udp_cfg, &protocols, &path)?)
                }
            };

            if let Some(ref udp_cfg) = udp {
                if udp_cfg.mode == eggress_udp::UdpMode::ShadowsocksUdp {
                    let ss = l.shadowsocks.as_ref().ok_or_else(|| {
                        ConfigError::validation(
                            &path,
                            "shadowsocks_udp mode requires [listeners.shadowsocks] section with method and password",
                        )
                    })?;
                    if ss.method.is_empty() {
                        return Err(ConfigError::validation(
                            &format!("{}.shadowsocks.method", path),
                            "shadowsocks method must not be empty",
                        ));
                    }
                    if ss.password.is_empty() {
                        return Err(ConfigError::validation(
                            &format!("{}.shadowsocks.password", path),
                            "shadowsocks password must not be empty",
                        ));
                    }
                }
            }

            let tls = match l.tls.as_ref() {
                Some(tls_cfg) => {
                    let cert_pem = std::fs::read(&tls_cfg.cert).map_err(|e| {
                        ConfigError::validation(
                            &format!("{}.tls.cert", path),
                            &format!("failed to read cert file: {}", e),
                        )
                    })?;
                    let key_pem = std::fs::read(&tls_cfg.key).map_err(|e| {
                        ConfigError::validation(
                            &format!("{}.tls.key", path),
                            &format!("failed to read key file: {}", e),
                        )
                    })?;
                    // Validate PEM at compile time
                    let mut builder = eggress_transport_tls::TlsServerConfigBuilder::new()
                        .with_certificate_pem(&cert_pem)
                        .and_then(|b| b.with_key_pem(&key_pem));
                    if let Some(ref alpn) = tls_cfg.alpn {
                        let alpn_bytes: Vec<Vec<u8>> =
                            alpn.iter().map(|s| s.as_bytes().to_vec()).collect();
                        builder = builder.map(|b| b.with_alpn(alpn_bytes));
                    }
                    builder.map_err(|e| {
                        ConfigError::validation(
                            &format!("{}.tls", path),
                            &format!("invalid TLS config: {}", e),
                        )
                    })?;
                    let alpn = tls_cfg
                        .alpn
                        .as_ref()
                        .map(|protocols| protocols.iter().map(|p| p.as_bytes().to_vec()).collect())
                        .unwrap_or_default();
                    Some(CompiledListenerTlsConfig {
                        cert_pem,
                        key_pem,
                        alpn,
                    })
                }
                None => None,
            };

            let transparent = compile_transparent_config(l.transparent.as_ref())?;

            let unix = compile_unix_listener_config(l.unix.as_ref())?;

            let auth = l.auth.as_ref().map(|a| -> Result<_, ConfigError> {
                let resolved_password = resolve_password(
                    a.password.as_deref(),
                    a.password_env.as_deref(),
                    &path,
                )?;
                Ok(crate::model::AuthConfig {
                    auth_type: a.auth_type.clone(),
                    username: a.username.clone(),
                    password: resolved_password,
                    password_env: None,
                })
            })
            .transpose()?;

            Ok(ListenerConfig {
                name: l.name.clone(),
                bind: l.bind.clone(),
                protocols,
                connection_limit: l.connection_limit,
                auth,
                udp,
                tls,
                shadowsocks: l.shadowsocks.clone(),
                trojan: l.trojan.clone(),
                transparent,
                unix,
            })
        })
        .collect()
}

/// Compile default UDP config when `udp_enabled = true` but no `[listeners.udp]` section.
fn compile_listener_udp_defaults(
    protocols: &[ProtocolId],
    path: &str,
) -> Result<CompiledListenerUdpConfig, ConfigError> {
    if !protocols.contains(&ProtocolId::Socks5) {
        return Err(ConfigError::validation(
            path,
            "udp_enabled = true requires socks5 protocol",
        ));
    }
    Ok(CompiledListenerUdpConfig::default())
}

/// Compile a `[listeners.udp]` section into `CompiledListenerUdpConfig`.
fn compile_listener_udp_config(
    udp: &ListenerUdpConfig,
    protocols: &[ProtocolId],
    path: &str,
) -> Result<CompiledListenerUdpConfig, ConfigError> {
    let defaults = CompiledListenerUdpConfig::default();
    let udp_path = format!("{}.udp", path);

    let mode = match udp.mode.as_deref() {
        Some("standalone_pproxy_udp") | Some("standalone") => {
            eggress_udp::UdpMode::StandalonePproxyUdp
        }
        Some("shadowsocks_udp") | Some("shadowsocks") => eggress_udp::UdpMode::ShadowsocksUdp,
        Some("socks5_udp_associate") | Some("socks5") | None => {
            eggress_udp::UdpMode::Socks5UdpAssociate
        }
        Some(other) => {
            return Err(ConfigError::validation(
                &format!("{}.mode", udp_path),
                &format!(
                    "unknown UDP mode '{}'; expected 'socks5_udp_associate', 'standalone_pproxy_udp', or 'shadowsocks_udp'",
                    other
                ),
            ));
        }
    };

    if mode == eggress_udp::UdpMode::Socks5UdpAssociate && !protocols.contains(&ProtocolId::Socks5)
    {
        return Err(ConfigError::validation(
            path,
            "UDP config requires socks5 protocol",
        ));
    }

    if mode == eggress_udp::UdpMode::ShadowsocksUdp {
        let has_ss_section = protocols.contains(&ProtocolId::Shadowsocks);
        if !has_ss_section {
            return Err(ConfigError::validation(
                path,
                "shadowsocks_udp mode requires shadowsocks protocol",
            ));
        }
        if !udp.client_pin.unwrap_or(true) {
            return Err(ConfigError::validation(
                &format!("{}.udp.client_pin", path),
                "shadowsocks_udp mode requires client_pin = true for security",
            ));
        }
    }

    let enabled = udp.enabled.unwrap_or(defaults.enabled);
    if !enabled {
        return Ok(CompiledListenerUdpConfig {
            enabled: false,
            ..defaults
        });
    }

    let bind_str = udp.bind.as_deref().unwrap_or("127.0.0.1:0");
    let bind: std::net::SocketAddr = bind_str.parse().map_err(|_| {
        ConfigError::validation(
            &format!("{}.bind", udp_path),
            &format!("invalid socket address: {}", bind_str),
        )
    })?;

    let advertise = match &udp.advertise {
        Some(addr_str) => {
            let ip: std::net::IpAddr = addr_str.parse().map_err(|_| {
                ConfigError::validation(
                    &format!("{}.advertise", udp_path),
                    &format!("invalid IP address: {}", addr_str),
                )
            })?;
            Some(ip)
        }
        None => None,
    };

    let idle_timeout = udp
        .idle_timeout
        .as_deref()
        .map(validate_duration)
        .transpose()
        .map_err(|e| {
            ConfigError::validation(&format!("{}.idle_timeout", udp_path), &e.to_string())
        })?
        .unwrap_or(defaults.idle_timeout);

    let target_idle_timeout = udp
        .target_idle_timeout
        .as_deref()
        .map(validate_duration)
        .transpose()
        .map_err(|e| {
            ConfigError::validation(&format!("{}.target_idle_timeout", udp_path), &e.to_string())
        })?
        .unwrap_or(defaults.target_idle_timeout);

    let max_associations = udp.max_associations.unwrap_or(defaults.max_associations);
    if max_associations == 0 {
        return Err(ConfigError::validation(
            &format!("{}.max_associations", udp_path),
            "must be greater than 0",
        ));
    }

    let max_targets_per_association = udp
        .max_targets_per_association
        .unwrap_or(defaults.max_targets_per_association);
    if max_targets_per_association == 0 {
        return Err(ConfigError::validation(
            &format!("{}.max_targets_per_association", udp_path),
            "must be greater than 0",
        ));
    }

    let max_datagram_size = udp.max_datagram_size.unwrap_or(defaults.max_datagram_size);
    if !(257..=65535).contains(&max_datagram_size) {
        return Err(ConfigError::validation(
            &format!("{}.max_datagram_size", udp_path),
            &format!("must be between 257 and 65535, got {}", max_datagram_size),
        ));
    }

    let client_pin = udp.client_pin.unwrap_or(defaults.client_pin);

    let allow_private_egress = udp
        .allow_private_egress
        .unwrap_or(defaults.allow_private_egress);

    let max_associations_global = udp
        .max_associations_global
        .unwrap_or(defaults.max_associations_global);
    if max_associations_global == 0 {
        return Err(ConfigError::validation(
            &format!("{}.max_associations_global", udp_path),
            "must be greater than 0",
        ));
    }

    Ok(CompiledListenerUdpConfig {
        mode,
        enabled,
        bind,
        advertise,
        idle_timeout,
        target_idle_timeout,
        max_associations,
        max_targets_per_association,
        max_datagram_size,
        client_pin,
        allow_private_egress,
        max_associations_global,
    })
}

fn compile_health_config(
    health: Option<&HealthConfigToml>,
) -> Result<eggress_routing::health::HealthConfig, ConfigError> {
    let defaults = eggress_routing::health::HealthConfig::default();
    let Some(h) = health else {
        return Ok(defaults);
    };

    let interval = h
        .interval
        .as_deref()
        .map(validate_duration)
        .transpose()?
        .unwrap_or(defaults.interval);

    let timeout = h
        .timeout
        .as_deref()
        .map(validate_duration)
        .transpose()?
        .unwrap_or(defaults.timeout);

    let failures_to_unhealthy = h
        .failures_to_unhealthy
        .unwrap_or(defaults.failures_to_unhealthy);
    if failures_to_unhealthy == 0 {
        return Err(ConfigError::validation(
            "health.failures_to_unhealthy",
            "must be greater than 0",
        ));
    }

    let successes_to_healthy = h
        .successes_to_healthy
        .unwrap_or(defaults.successes_to_healthy);
    if successes_to_healthy == 0 {
        return Err(ConfigError::validation(
            "health.successes_to_healthy",
            "must be greater than 0",
        ));
    }

    let initial_state = match h.initial_state.as_deref() {
        Some("unknown") | None => defaults.initial_state,
        Some("healthy") => eggress_routing::health::HealthState::Healthy,
        Some("unhealthy") => eggress_routing::health::HealthState::Unhealthy,
        Some("disabled") => eggress_routing::health::HealthState::Disabled,
        Some(other) => {
            return Err(ConfigError::validation(
                "health.initial_state",
                &format!(
                    "unknown state '{}', must be one of: unknown, healthy, unhealthy, disabled",
                    other
                ),
            ));
        }
    };

    Ok(eggress_routing::health::HealthConfig {
        interval,
        timeout,
        failures_to_unhealthy,
        successes_to_healthy,
        initial_state,
    })
}

fn compile_upstreams(config: &ConfigFile) -> Result<Vec<UpstreamConfig>, ConfigError> {
    let upstreams = match &config.upstreams {
        Some(u) => u,
        None => return Ok(vec![]),
    };

    upstreams
        .iter()
        .map(|u| {
            eggress_routing::upstream::validate_upstream_id(&u.id)
                .map_err(|e| ConfigError::validation(&format!("upstream {}", u.id), &e))?;

            let chain = eggress_uri::parse_proxy_chain(&u.uri).map_err(|e| {
                ConfigError::validation(
                    &format!("upstream {}", u.id),
                    &format!("invalid URI: {}", e),
                )
            })?;

            let health = compile_health_config(u.health.as_ref()).map_err(|e| match e {
                ConfigError::Validation { path, message } => {
                    ConfigError::validation(&format!("upstream {}.{}", u.id, path), &message)
                }
                other => other,
            })?;

            Ok(UpstreamConfig {
                id: u.id.clone(),
                chain,
                health,
            })
        })
        .collect()
}

fn compile_groups(config: &ConfigFile) -> Result<Vec<UpstreamGroupConfig>, ConfigError> {
    let groups = match &config.upstream_groups {
        Some(g) => g,
        None => return Ok(vec![]),
    };

    groups
        .iter()
        .map(|g| {
            let scheduler = match g.scheduler.as_deref() {
                Some("round-robin") | None => SchedulerKind::RoundRobin,
                Some("first-available") => SchedulerKind::FirstAvailable,
                Some("random") => SchedulerKind::Random,
                Some("least-connections") => SchedulerKind::LeastConnections,
                Some(other) => {
                    return Err(ConfigError::validation(
                        &format!("group {}", g.id),
                        &format!("unknown scheduler: {}", other),
                    ))
                }
            };

            let fallback = match g.fallback.as_deref() {
                Some("reject") | None => GroupFallback::Reject,
                Some("direct") => GroupFallback::Direct,
                Some("use-unhealthy") => GroupFallback::UseUnhealthy,
                Some(other) => {
                    return Err(ConfigError::validation(
                        &format!("group {}", g.id),
                        &format!("unknown fallback: {}", other),
                    ))
                }
            };

            Ok(UpstreamGroupConfig {
                id: UpstreamGroupId(Arc::from(g.id.as_str())),
                scheduler,
                members: g.members.clone(),
                fallback,
            })
        })
        .collect()
}

fn compile_rules(config: &ConfigFile) -> Result<Vec<eggress_routing::CompiledRule>, ConfigError> {
    let mut compiled_rules = Vec::new();

    let group_ids: std::collections::HashSet<&str> = config
        .upstream_groups
        .as_ref()
        .map(|gs| gs.iter().map(|g| g.id.as_str()).collect())
        .unwrap_or_default();

    if let Some(ref rules) = config.rules {
        for r in rules {
            let matcher = compile_matcher(r)?;
            let action = compile_action(r, &group_ids)?;

            compiled_rules.push(eggress_routing::CompiledRule {
                id: eggress_routing::RuleId(Arc::from(r.id.as_str())),
                matcher,
                action,
            });
        }
    }

    if let Some(ref rules_file_path) = config.rules_file {
        if group_ids.len() > 1 {
            return Err(ConfigError::validation(
                "rules_file",
                "rules_file routes all rules to a single group; multiple groups are not supported with rules_file — use explicit [[rules]] instead",
            ));
        }
        let content = std::fs::read_to_string(rules_file_path).map_err(|e| {
            ConfigError::validation(
                "rules_file",
                &format!("failed to read '{}': {}", rules_file_path, e),
            )
        })?;
        let compat_rules = eggress_routing::CompatRegexRule::parse_file(&content).map_err(|e| {
            ConfigError::validation(
                "rules_file",
                &format!("failed to parse '{}': {}", rules_file_path, e),
            )
        })?;
        for (idx, compat) in compat_rules.into_iter().enumerate() {
            compiled_rules.push(eggress_routing::CompiledRule {
                id: eggress_routing::RuleId(Arc::from(format!("rules-file-{}", idx + 1).as_str())),
                matcher: eggress_routing::MatchExpr::HostRegex(compat.pattern),
                action: group_ids
                    .iter()
                    .next()
                    .map(|g| {
                        eggress_routing::RouteActionSpec::UpstreamGroup(
                            eggress_routing::UpstreamGroupId(Arc::from(*g)),
                        )
                    })
                    .unwrap_or(eggress_routing::RouteActionSpec::Direct),
            });
        }
    }

    Ok(compiled_rules)
}

fn compile_default_action(config: &ConfigFile) -> eggress_routing::RouteActionSpec {
    let default_str = config.routing.as_ref().and_then(|r| r.default.as_deref());

    match default_str {
        Some("direct") => eggress_routing::RouteActionSpec::Direct,
        Some("reject") => eggress_routing::RouteActionSpec::Reject(RejectReason::Blocked),
        Some(group_id) => {
            eggress_routing::RouteActionSpec::UpstreamGroup(UpstreamGroupId(Arc::from(group_id)))
        }
        None => eggress_routing::RouteActionSpec::Direct,
    }
}

fn compile_admin(config: &ConfigFile) -> Option<AdminConfig> {
    let admin = config.admin.as_ref()?;

    let pac = admin.pac.as_ref().map(|pac_toml| {
        let path = pac_toml.path.clone().unwrap_or_else(|| "/pac".to_string());
        eggress_admin::PacConfig {
            path,
            proxy_directive: pac_toml.proxy.clone(),
            direct_fallback: pac_toml.direct_fallback.unwrap_or(true),
            direct_hosts: pac_toml.direct_hosts.clone().unwrap_or_default(),
            direct_suffixes: pac_toml.direct_suffixes.clone().unwrap_or_default(),
        }
    });

    let static_content = admin
        .static_content
        .as_ref()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| eggress_admin::StaticRoute {
                    path: entry.path.clone(),
                    content_type: entry
                        .content_type
                        .clone()
                        .unwrap_or_else(|| "text/plain".to_string()),
                    body: entry.body.clone().unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    Some(AdminConfig {
        bind: admin
            .bind
            .clone()
            .unwrap_or_else(|| "127.0.0.1:9090".to_string()),
        enabled: admin.enabled.unwrap_or(true),
        metrics: admin.metrics.unwrap_or(true),
        pac,
        static_content,
    })
}

fn compile_transparent_config(
    config: Option<&crate::model::TransparentConfig>,
) -> Result<Option<CompiledTransparentConfig>, ConfigError> {
    let Some(cfg) = config else {
        return Ok(None);
    };

    let enabled = cfg.enabled.unwrap_or(false);
    let protocol = cfg.protocol.as_deref().unwrap_or("redir").to_string();

    match protocol.as_str() {
        "redir" | "pf" => {}
        other => {
            return Err(ConfigError::validation(
                "transparent.protocol",
                &format!(
                    "unknown transparent protocol '{}'; expected 'redir' or 'pf'",
                    other
                ),
            ));
        }
    }

    Ok(Some(CompiledTransparentConfig { enabled, protocol }))
}

fn compile_unix_listener_config(
    config: Option<&crate::model::UnixListenerConfig>,
) -> Result<Option<CompiledUnixListenerConfig>, ConfigError> {
    let Some(cfg) = config else {
        return Ok(None);
    };

    let path = std::path::PathBuf::from(&cfg.path);
    if path.parent().is_none() || path.parent() == Some(std::path::Path::new("")) {
        return Err(ConfigError::validation(
            "unix.path",
            &format!(
                "socket path must be absolute or have a valid parent directory: {}",
                cfg.path
            ),
        ));
    }

    let unlink_existing = cfg.unlink_existing.unwrap_or(true);
    let mode = cfg.mode.unwrap_or(0o660);

    Ok(Some(CompiledUnixListenerConfig {
        path,
        unlink_existing,
        mode,
    }))
}

fn compile_reverse_servers(
    config: &ConfigFile,
) -> Result<Vec<CompiledReverseServerConfig>, ConfigError> {
    let servers = match &config.reverse_servers {
        Some(s) => s,
        None => return Ok(vec![]),
    };

    servers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let path = format!("reverse_servers[{}]", i);

            let control_bind: std::net::SocketAddr = s.control_bind.parse().map_err(|_| {
                ConfigError::validation(
                    &format!("{}.control_bind", path),
                    &format!("invalid socket address: {}", s.control_bind),
                )
            })?;

            let external_bind: std::net::SocketAddr = s.external_bind.parse().map_err(|_| {
                ConfigError::validation(
                    &format!("{}.external_bind", path),
                    &format!("invalid socket address: {}", s.external_bind),
                )
            })?;

            let auth_password = resolve_password(
                s.auth_password.as_deref(),
                s.auth_password_env.as_deref(),
                &path,
            )?;

            if s.auth_username.is_some() != auth_password.is_some() {
                return Err(ConfigError::validation(
                    &path,
                    "reverse server auth requires both auth_username and auth_password",
                ));
            }

            let max_streams = s.max_streams.unwrap_or(1024);

            let heartbeat_interval_ms = s
                .heartbeat_interval
                .as_deref()
                .map(|h| validate_duration(h).map(|d| d.as_millis() as u64))
                .transpose()
                .map_err(|e| {
                    ConfigError::validation(&format!("{}.heartbeat_interval", path), &e.to_string())
                })?
                .unwrap_or(300_000);

            Ok(CompiledReverseServerConfig {
                id: s.id.clone(),
                control_bind,
                external_bind,
                auth_username: s.auth_username.clone(),
                auth_password,
                max_control_connections: 256,
                read_timeout_ms: heartbeat_interval_ms,
                allow_bind: None,
                max_listeners_per_client: 1,
                max_streams_per_listener: max_streams,
                max_pending_external: 1024,
            })
        })
        .collect()
}

fn compile_reverse_clients(
    config: &ConfigFile,
) -> Result<Vec<CompiledReverseClientConfig>, ConfigError> {
    let clients = match &config.reverse_clients {
        Some(c) => c,
        None => return Ok(vec![]),
    };

    clients
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let path = format!("reverse_clients[{}]", i);

            let server_addr: std::net::SocketAddr = c.server_addr.parse().map_err(|_| {
                ConfigError::validation(
                    &format!("{}.server_addr", path),
                    &format!("invalid socket address: {}", c.server_addr),
                )
            })?;

            let auth_password = resolve_password(
                c.auth_password.as_deref(),
                c.auth_password_env.as_deref(),
                &path,
            )?;

            let reconnect_initial_ms = c
                .reconnect_initial
                .as_deref()
                .map(|d| validate_duration(d).map(|dur| dur.as_millis() as u64))
                .transpose()
                .map_err(|e| {
                    ConfigError::validation(&format!("{}.reconnect_initial", path), &e.to_string())
                })?
                .unwrap_or(1_000);

            let reconnect_max_ms = c
                .reconnect_max
                .as_deref()
                .map(|d| validate_duration(d).map(|dur| dur.as_millis() as u64))
                .transpose()
                .map_err(|e| {
                    ConfigError::validation(&format!("{}.reconnect_max", path), &e.to_string())
                })?
                .unwrap_or(30_000);

            let heartbeat_interval_ms = c
                .heartbeat_interval
                .as_deref()
                .map(|d| validate_duration(d).map(|dur| dur.as_millis() as u64))
                .transpose()
                .map_err(|e| {
                    ConfigError::validation(&format!("{}.heartbeat_interval", path), &e.to_string())
                })?
                .unwrap_or(60_000);

            let parallel_connections = c.parallel_connections.unwrap_or(1);

            // BUG-005: Without default_target_host and default_target_port,
            // the reverse client cannot connect to any target. Reject at compile time.
            if c.default_target_host.is_none() || c.default_target_port.is_none() {
                return Err(ConfigError::validation(
                    &path,
                    "reverse client requires default_target_host and default_target_port",
                ));
            }

            Ok(CompiledReverseClientConfig {
                id: c.id.clone(),
                server_addr,
                auth_username: c.auth_username.clone(),
                auth_password,
                reconnect_initial_ms,
                reconnect_max_ms,
                default_target_host: c.default_target_host.clone(),
                default_target_port: c.default_target_port,
                read_timeout_ms: heartbeat_interval_ms,
                drain_grace_ms: 5_000,
                parallel_connections,
            })
        })
        .collect()
}

/// Resolve a password from either an explicit value or an environment variable.
fn resolve_password(
    password: Option<&str>,
    password_env: Option<&str>,
    path: &str,
) -> Result<Option<String>, ConfigError> {
    if let Some(env_var) = password_env {
        std::env::var(env_var).map(Some).map_err(|_| {
            ConfigError::validation(
                path,
                &format!(
                    "environment variable '{}' not set (referenced by auth_password_env)",
                    env_var
                ),
            )
        })
    } else {
        Ok(password.map(|s| s.to_string()))
    }
}

fn parse_duration_opt(s: &str) -> Option<std::time::Duration> {
    crate::validate::validate_duration(s).ok()
}

pub fn load_and_compile(path: &str) -> Result<RuntimeConfig, crate::error::ConfigError> {
    crate::load_and_validate(path)
}
