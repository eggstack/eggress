//! Translation model: TOML-serializable structs shared by the semantic
//! builder (`intermediates`) and both renderers (`toml`, `native`).
//!
//! Field-for-field agreement between the two renderers is what keeps the
//! TOML and native paths equivalent (see `native_equivalence` tests).

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct TlsToml {
    pub(crate) cert: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) alpn: Option<Vec<String>>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct ListenerToml {
    pub(crate) name: String,
    pub(crate) bind: String,
    pub(crate) protocols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reuse_port: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) auth: Option<AuthToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) udp: Option<UdpToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) shadowsocks: Option<ShadowsocksToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ssr: Option<SsrToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) trojan: Option<TrojanToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) transparent: Option<TransparentToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) unix: Option<UnixToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tls: Option<TlsToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fixed_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) local_bind: Option<String>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct TransparentToml {
    pub(crate) enabled: bool,
    pub(crate) protocol: String,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct UnixToml {
    pub(crate) path: String,
    pub(crate) unlink_existing: bool,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct ShadowsocksToml {
    pub(crate) method: String,
    pub(crate) password: String,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct SsrToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) auth_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) plugins: Vec<String>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct TrojanToml {
    pub(crate) password: String,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct UdpToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fixed_target: Option<String>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct AuthToml {
    #[serde(rename = "type")]
    pub(crate) r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) password: Option<String>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct UpstreamToml {
    pub(crate) id: String,
    pub(crate) uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) health: Option<HealthToml>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct UpstreamGroupToml {
    pub(crate) id: String,
    pub(crate) scheduler: String,
    pub(crate) members: Vec<String>,
    pub(crate) fallback: String,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct RuleToml {
    pub(crate) id: String,
    pub(crate) any: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) upstream_group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) direct: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "match")]
    pub(crate) r#match: Option<MatchToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) host_regex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reject: Option<String>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct MatchToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) host_regex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) destination_port_regex: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) any_of: Vec<MatchToml>,
}

#[derive(serde::Serialize)]
pub(crate) struct ConfigToml {
    pub(crate) version: u32,
    pub(crate) listeners: Vec<ListenerToml>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) upstreams: Vec<UpstreamToml>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) upstream_groups: Vec<UpstreamGroupToml>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) rules: Vec<RuleToml>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) reverse_servers: Vec<ReverseServerToml>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) reverse_clients: Vec<ReverseClientToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) admin: Option<AdminToml>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct ReverseServerToml {
    pub(crate) id: String,
    pub(crate) control_bind: String,
    pub(crate) external_bind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) auth_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) auth_password: Option<String>,
    pub(crate) pproxy_compat: bool,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct ReverseClientToml {
    pub(crate) id: String,
    pub(crate) server_addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) server_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) auth_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) auth_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parallel_connections: Option<u32>,
    pub(crate) pproxy_compat: bool,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct HealthToml {
    pub(crate) interval: String,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct PacToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) proxy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) direct_fallback: Option<bool>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct AdminToml {
    pub(crate) pac: PacToml,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) static_content: Vec<StaticContentToml>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub(crate) struct StaticContentToml {
    pub(crate) path: String,
    pub(crate) body: String,
}
