pub mod assoc;
pub mod codec;
pub mod direct;
pub mod error;
pub mod flow;
pub mod limits;
pub mod metrics;
pub mod registry;
pub mod relay;
pub mod security;
pub mod standalone;
pub mod standalone_shadowsocks;
pub mod testkit;
pub mod udp_capability;
pub mod upstream_socks5;

pub use flow::*;
pub use udp_capability::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UdpMode {
    Socks5UdpAssociate,
    StandalonePproxyUdp,
    ShadowsocksUdp,
}
